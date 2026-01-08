#![cfg_attr(not(feature = "std"), no_std, no_main)]

/// The loans are managed by the LoanManager contract, which interacts with all other contracts

#[ink::contract]
mod loan_manager {
    use config::ConfigRef;
    use reputation::ReputationRef;
    use lending_pool::LendingPoolRef;
    use vouch::VouchRef;
    use ink::storage::Mapping;
    use ink::U256;
    use ink::prelude::vec::Vec;

    /// Struct for loan information
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq)]
    pub struct Loan {
        loan_id: u64,
        interest_rate: u64,
        term: Timestamp,
        start_time: Timestamp,
        amount: Balance,  // u128
        borrower: AccountId,  // [u8; 32]
        status: LoanStatus,
        total_repayment_amount: Balance, // Fixed repayment amount calculated at loan creation
    }

    /// All information that is needed to store in the contract
    #[ink(storage)]
    pub struct LoanManager {
        config: ConfigRef,
        reputation: ReputationRef,
        lending_pool: LendingPoolRef,
        vouch: VouchRef,
        lending_pool_address: Address,
        loans: Mapping<u64, Loan>,
        next_loan_id: u64,
    }

    /// Enum for Loan Status
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq, Eq, Copy, Clone)]
    pub enum LoanStatus {
        Pending,  // Waiting for vouches
        Active,   // Funded and active
        Repaid,
        Defaulted
    }

    /// Events for lending pool actions
    #[ink(event)]
    pub struct LoanRequested{
        id: u64,
        borrower: AccountId,
        amount: Balance,
        term: Timestamp,
    }

    #[ink(event)]
    pub struct LoanRepaid{
        id: u64,
        borrower: AccountId,
        amount: Balance,
    }

    #[ink(event)]
    pub struct LoanDefaulted {
        id: u64,
        borrower: AccountId,
        amount: Balance,
    }

    /// Error types for the contract
    #[derive(Debug, PartialEq, Eq)]
    #[ink::scale_derive(Encode, Decode, TypeInfo)]
    pub enum Error {
        InsufficientReputation,
        InsufficientVouches,
        ZeroAmount,
        DisbursementFailed,
        LoanNotFound,
        LoanNotActive,
        LoanNotPending,
        LoanNotOverdue,
        SlashFailed,
        ResolveFailed,
        Unauthorized,
        RepaymentFailed,
        InvalidRepaymentAmount,
        Overflow,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl LoanManager {
        // Decimal conversion constant: 10^8 to convert between 10 decimals and 18 decimals
        const DECIMAL_CONVERSION: u128 = 100_000_000; // 10^8

        /// Convert amount from 10 decimals (storage format) to 18 decimals (chain format)
        fn convert_10_to_18_decimals(&self, amount_10: Balance) -> Balance {
            (amount_10 as u128)
                .checked_mul(Self::DECIMAL_CONVERSION)
                .unwrap_or(0) as Balance
        }

        // Token decimals constant - matches the scaling factor used throughout the contract (1e9)
        // This should match the native currency's decimal places
        const TOKEN_DECIMALS: Balance = 1_000_000_000; // 1e9

        #[ink(constructor)]
        pub fn new(config_address: Address, reputation_address: Address, lending_pool_address: Address, vouch_address: Address) -> Self {
            let config: ConfigRef =
                ink::env::call::FromAddr::from_addr(config_address);
            let reputation: ReputationRef =
                ink::env::call::FromAddr::from_addr(reputation_address);
            let lending_pool: LendingPoolRef =
                ink::env::call::FromAddr::from_addr(lending_pool_address);
            let vouch: VouchRef =
                ink::env::call::FromAddr::from_addr(vouch_address);
            Self {
                config,
                reputation,
                lending_pool,
                vouch,
                lending_pool_address,
                loans: Mapping::default(),
                next_loan_id: 1,
            }
        }

        /// Request a loan from the lending pool
        /// Creates a pending loan that requires vouches before disbursement
        #[ink(message)]
        pub fn request_loan(&mut self, amount: Balance, loan_term: Timestamp, account_id: AccountId) -> Result<u64> {
            if amount == 0 {
                return Err(Error::ZeroAmount);
            }
            let caller: AccountId = account_id;

            // Calculate tier-based requirements for this loan amount
            let (min_stars, _min_vouches) = self.calculate_requirements(amount);

            // Verify stars via reputation contract (still required for loan request)
            let stars = self.reputation.get_stars(caller);
            if stars < min_stars {
               return Err(Error::InsufficientReputation);
            }

            // Fetch current rate from lending pool and adjust by stars
            let base_rate = self.lending_pool.get_current_rate();
            let adjusted_rate = self.adjust_rate_by_stars(base_rate, stars);

            // Calculate total repayment amount (principal + interest) at loan creation
            // Repayment = amount * (1 + interest_rate_percentage)
            // The interest_rate is stored as percentage * 1e9 (e.g., 10% = 10_000_000_000)
            // To get the percentage: adjusted_rate / 100_000_000_000 (1e11)
            // So: repayment = amount * (1 + adjusted_rate / 100_000_000_000)
            // Example: 100 tokens at 10% = 100 * (1 + 10_000_000_000 / 100_000_000_000) = 100 * 1.10 = 110
            let total_repayment = (amount as u128)
                .checked_mul(100_000_000_000u128 + adjusted_rate as u128)
                .and_then(|v| v.checked_div(100_000_000_000u128))
                .unwrap_or(amount) as Balance;

            // Create pending loan record (no vouches yet, no disbursement)
            let loan_id = self.next_loan_id;
            let loan = Loan {
                loan_id,
                interest_rate: adjusted_rate,
                term: loan_term,
                start_time: 0, // Will be set when loan becomes Active
                status: LoanStatus::Pending,
                amount: amount,
                borrower: caller,
                total_repayment_amount: total_repayment,
            };

            // Store the loan
            self.loans.insert(loan_id, &loan);
            self.next_loan_id = loan_id + 1;

            // Emit LoanRequested event
            self.env().emit_event(LoanRequested {
                id: loan_id,
                borrower: caller,
                amount,
                term: loan_term,
            });

            Ok(loan_id)
        }

        // Vouch for a pending loan
        // Validates loan is pending, then creates vouch and checks if disbursement is ready
        #[ink(message)]
        pub fn vouch_for_loan(&mut self, loan_id: u64, stars: u32, capital_percent: u8, voucher_account_id: AccountId, loan_manager_address: Address) -> Result<()> {
            let loan = self.loans.get(loan_id).ok_or(Error::LoanNotFound)?;

            // Only pending loans can receive vouches
            if loan.status != LoanStatus::Pending {
                return Err(Error::LoanNotPending);
            }

            // Create vouch via vouch contract
            self.vouch.vouch_for_loan(loan_id, loan.borrower, voucher_account_id, stars, capital_percent, loan_manager_address)
                .map_err(|_| Error::ResolveFailed)?;

            // Check if we now have enough vouches to disburse
            let (_min_stars, min_vouches) = self.calculate_requirements(loan.amount);
            let current_vouches = self.vouch.get_vouches_for_loan(loan_id);

            if current_vouches >= min_vouches {
                // Auto-disburse when threshold is met
                self.disburse_loan(loan_id)?;
            }

            Ok(())
        }

        // Internal function to disburse a loan that has enough vouches
        fn disburse_loan(&mut self, loan_id: u64) -> Result<()> {
            let mut loan = self.loans.get(loan_id).ok_or(Error::LoanNotFound)?;

            if loan.status != LoanStatus::Pending {
                return Err(Error::LoanNotPending);
            }

            // Update loan to Active status
            loan.status = LoanStatus::Active;
            loan.start_time = self.env().block_timestamp();
            self.loans.insert(loan_id, &loan);

            // Disburse funds via lending pool
            self.lending_pool.disburse(loan.amount, loan.borrower)
                .map_err(|_| Error::DisbursementFailed)?;

            Ok(())
        }

        // Get loan information (for external queries)
        #[ink(message)]
        pub fn get_loan(&self, loan_id: u64) -> Option<Loan> {
            self.loans.get(loan_id)
        }

        // Repay a loan
        // Calculates the repayment amount (principal + interest) and processes the repayment
        // Marks the loan as repaid and resolves vouches as successful
        #[ink(message, payable)]
        pub fn repay_loan(&mut self, loan_id: u64, borrower_account_id: AccountId, loan_manager_address: Address) -> Result<()> {
            let mut loan = self.loans.get(loan_id).ok_or(Error::LoanNotFound)?;

            // Only active loans can be repaid
            if loan.status != LoanStatus::Active {
                return Err(Error::LoanNotActive);
            }

            // Verify caller is the borrower
            if borrower_account_id != loan.borrower {
                return Err(Error::Unauthorized);
            }

            // Calculate repayment amount (principal + interest)
            let repayment_amount = self.calculate_repayment_amount(&loan);

            let repaid_u256 = self.env().transferred_value();
            if repaid_u256 > U256::from(u128::MAX) {
                return Err(Error::Overflow);
            }
            let repaid: Balance = repaid_u256.as_u128();

            // Convert repayment_amount from 10 decimals to 18 decimals for comparison
            // The chain uses 18 decimals, but our contract uses 10 decimals
            const DECIMAL_CONVERSION: u128 = 100_000_000; // 10^8 (to convert from 10 to 18 decimals)
            let repayment_amount_18_decimals = (repayment_amount as u128)
                .checked_mul(DECIMAL_CONVERSION)
                .unwrap_or(0) as Balance;

            // Verify the repaid amount matches the required repayment (both in 18 decimals)
            if repaid != repayment_amount_18_decimals {
                return Err(Error::InvalidRepaymentAmount);
            }

            // Forward payment to lending pool's receive_repayment
            // Use build_call to forward the payment with the repayment amount
            // Note: repaid is in 18 decimals, but we need to pass the amount parameter in the same decimals
            // The lending pool expects the amount parameter to match the transferred value (18 decimals)
            use ink::env::call::{build_call, ExecutionInput, Selector};
            use ink::env::DefaultEnvironment;
            
            // Use the repaid amount (18 decimals) for forwarding, and pass it as the amount parameter
            let repayment_u256 = U256::from(repaid);
            
            let result = build_call::<DefaultEnvironment>()
                .call(self.lending_pool_address)
                .transferred_value(repayment_u256)
                .exec_input(
                    ExecutionInput::new(Selector::new(ink::selector_bytes!("receive_repayment")))
                        .push_arg(&repaid) // Pass repaid (18 decimals) to match transferred value
                )
                .returns::<Result<()>>()
                .try_invoke();
            
            match result {
                Ok(Ok(_)) => {},
                _ => return Err(Error::RepaymentFailed),
            }

            // Mark loan as repaid
            loan.status = LoanStatus::Repaid;
            self.loans.insert(loan_id, &loan);

            // Resolve all vouch relationships for this loan as successful
            // Pass 0 for loan_amount since it's not used when success=true
            self.vouch.resolve_loan(loan_id, loan.borrower, true, 0, loan_manager_address)
                .map_err(|_| Error::ResolveFailed)?;

            // Emit LoanRepaid event
            self.env().emit_event(LoanRepaid {
                id: loan_id,
                borrower: loan.borrower,
                amount: repayment_amount,
            });

            Ok(())
        }

        /// Check if a loan is overdue and handle defaulting
        /// 
        /// This function can be called by anyone to trigger default processing for overdue loans.
        /// It includes safeguards to prevent premature defaults:
        /// - Only active loans can be defaulted (prevents double-processing)
        /// - Loan must be past due date + grace period (configurable buffer)
        /// 
        /// The grace period provides a buffer after the due date, allowing borrowers time to
        /// repay and preventing race conditions with repayment transactions.
        /// 
        /// Slashes borrower's stars and resolves vouches as failed
        #[ink(message)]
        pub fn check_default(&mut self, loan_id: u64, loan_manager_address: Address, vouch_contract_address: Address) -> Result<()> {
            let mut loan = self.loans.get(loan_id).ok_or(Error::LoanNotFound)?;

            // Only active loans can be defaulted (prevents double-processing)
            if loan.status != LoanStatus::Active {
                return Err(Error::LoanNotActive);
            }

            // Check if loan is overdue and past grace period
            let current_time = self.env().block_timestamp();
            let due_time = loan.start_time.saturating_add(loan.term);
            let grace_period = self.config.get_default_grace_period();
            let defaultable_time = due_time.saturating_add(grace_period);
            
            // Loan can only be defaulted after due_time + grace_period
            // This prevents premature defaults and provides a buffer for repayments
            if current_time <= defaultable_time {
                return Err(Error::LoanNotOverdue);
            }

            // Mark loan as defaulted
            loan.status = LoanStatus::Defaulted;
            self.loans.insert(loan_id, &loan);

            // Slash borrower's stars via reputation contract
            // Slash amount proportional to loan amount, using consistent token decimals
            let stars_to_slash = (loan.amount / Self::TOKEN_DECIMALS).max(1) as u32;
            let _ = self.reputation.slash_stars(loan.borrower, stars_to_slash);

            // Resolve all vouch relationships for this loan as failed
            // Pass loan.amount to compare with slashed capital for recovery calculation
            self.vouch.resolve_loan(loan_id, loan.borrower, false, loan.amount, loan_manager_address)
                .map_err(|_| Error::ResolveFailed)?;

            // Emit LoanDefaulted event
            self.env().emit_event(LoanDefaulted {
                id: loan_id,
                borrower: loan.borrower,
                amount: loan.amount,
            });

            Ok(())
        }

        /// Internal: Calculate tier-based requirements for a loan amount
        /// Returns (min_stars_required, min_vouches_required)
        /// Tier thresholds and requirements are configurable via the Config contract.
        /// This avoids hardcoded magic numbers and allows protocol upgrades without
        /// redeploying the LoanManager.
        fn calculate_requirements(&self, amount: Balance) -> (u32, u32) {
            // Scaling factor used to normalize the loan amount before tier comparison
            let scaling_factor = self.config.loan_tier_scaling_factor();
            let scaled_amount = if scaling_factor > 0 {
                amount / scaling_factor
            } else {
                // Fallback to no scaling if misconfigured; preserves previous behavior shape
                amount
            };
            // Configurable tier thresholds expressed in the same scaled units
            let tier1_max = self.config.loan_tier1_max_scaled_amount();
            let tier2_max = self.config.loan_tier2_max_scaled_amount();
            if scaled_amount < tier1_max {
                // Tier 1 requirements
                self.config.loan_tier1_requirements()
            } else if scaled_amount < tier2_max {
                // Tier 2 requirements
                self.config.loan_tier2_requirements()
            } else {
                // Tier 3 requirements
                self.config.loan_tier3_requirements()
            }
        }

        /// Internal: Adjust interest rate based on borrower's stars
        /// Higher stars result in lower interest rates
        /// Uses configurable parameters from Config contract
        fn adjust_rate_by_stars(&self, base_rate: u64, stars: u32) -> u64 {
            // Get configurable discount parameters
            let discount_per_star = self.config.get_star_discount_percent_per_star();
            let max_discount = self.config.get_max_star_discount_percent();
            
            // Calculate discount: stars * discount_per_star, capped at max_discount
            // Example: 10 stars * 1% per star = 10% reduction, capped at 50%
            let discount_percent = (stars as u64)
                .saturating_mul(discount_per_star)
                .min(max_discount);
            
            // Apply discount to base rate
            let discount = base_rate.saturating_mul(discount_percent) / 100;
            base_rate.saturating_sub(discount)
        }

        /// Internal: Calculate repayment amount (principal + interest)
        /// Returns the fixed repayment amount calculated at loan creation
        fn calculate_repayment_amount(&self, loan: &Loan) -> Balance {
            loan.total_repayment_amount
        }

        /// Get the repayment amount for a loan
        /// Returns the fixed repayment amount (principal + interest) calculated at loan creation
        /// Returns value in 18 decimals (chain format) for consistency with total liquidity
        #[ink(message)]
        pub fn get_repayment_amount(&self, loan_id: u64) -> Result<Balance> {
            let loan = self.loans.get(loan_id).ok_or(Error::LoanNotFound)?;
            // Convert from 10 decimals (storage) to 18 decimals (chain format)
            Ok(self.convert_10_to_18_decimals(loan.total_repayment_amount))
        }

        /// Get all pending loans
        /// Returns a vector of loan IDs that are currently pending
        #[ink(message)]
        pub fn get_all_pending_loans(&self) -> Vec<u64> {
            let mut pending_loans = Vec::new();
            // Iterate through all possible loan IDs (from 1 to next_loan_id - 1)
            let max_id = self.next_loan_id;
            for loan_id in 1..max_id {
                if let Some(loan) = self.loans.get(loan_id) {
                    if loan.status == LoanStatus::Pending {
                        pending_loans.push(loan_id);
                    }
                }
            }
            pending_loans
        }

        /// Get all active loans
        /// Returns a vector of loan IDs that are currently active
        #[ink(message)]
        pub fn get_all_active_loans(&self) -> Vec<u64> {
            let mut active_loans = Vec::new();
            // Iterate through all possible loan IDs (from 1 to next_loan_id - 1)
            let max_id = self.next_loan_id;
            for loan_id in 1..max_id {
                if let Some(loan) = self.loans.get(loan_id) {
                    if loan.status == LoanStatus::Active {
                        active_loans.push(loan_id);
                    }
                }
            }
            active_loans
        }
    }
}
