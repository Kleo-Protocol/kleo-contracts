#![cfg_attr(not(feature = "std"), no_std, no_main)]

use ink::env::{DefaultEnvironment, Environment};

pub type AccountId = <DefaultEnvironment as Environment>::AccountId;

/// The loans are managed by the LoanManager contract, which interacts with all other contracts

#[ink::contract]
mod loan_manager {
    use config::ConfigRef;
    use reputation::ReputationRef;
    use lending_pool::LendingPoolRef;
    use vouch::VouchRef;
    use ink::prelude::vec::Vec;
    use ink::storage::Lazy;
    use ink::storage::Mapping;


    /// Struct for loan information
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq)]
    pub struct Loan {
        loan_id: u64,
        borrower: AccountId,
        amount: Balance,
        interest_rate: u64,
        term: Timestamp,
        purpose: Vec<u8>,
        start_time: Timestamp,
        status: LoanStatus,
        vouchers: Vec<AccountId>
    }

    /// Enun for Loan Status
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq)]
    pub enum LoanStatus {
        Active,
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
        LoanNotOverdue,
        SlashFailed,
        ResolveFailed,
    }

    /// All information that is needed to store in the contract
    #[ink(storage)]
    pub struct LoanManager {
        config: ConfigRef,
        reputation: ReputationRef,
        lending_pool: LendingPoolRef,
        vouch: VouchRef,
        loans: Mapping<u64, Loan>,
        next_loan_id: Lazy<u64>,
    }

    impl LoanManager {
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
                loans: Mapping::default(),
                next_loan_id: Lazy::default(),
            }
        }

        /// Request a loan from the lending pool
        #[ink(message)]
        pub fn request_loan(&mut self, amount: Balance, _purpose: Vec<u8>) -> Result<u64, Error> {
            if amount == 0 {
                return Err(Error::ZeroAmount);
            }

            let caller= Self::env().caller();
            let caller_acc = Self::env().to_account_id(caller);

            // Calculate tier-based requirements for this loan amount
            let (min_stars, min_vouches) = self.calculate_requirements(amount);

            // Verify stars via reputation contract
            let stars = self.reputation.get_stars(caller_acc);
            if stars < min_stars {
                return Err(Error::InsufficientReputation);
            }

            // Verify vouches via vouch contract
            let vouches = self.vouch.get_vouches_for(caller_acc);
            if vouches < min_vouches {
                return Err(Error::InsufficientVouches);
            }

            // Fetch current rate from lending pool and adjust by stars
            let base_rate = self.lending_pool.get_current_rate();
            let adjusted_rate = self.adjust_rate_by_stars(base_rate, stars);

            // Get loan term from config (default 30 days in ms)
            let term = self.config.get_cooldown_period(); // Using cooldown as default term

            // Get the vouchers list for this borrower
            let vouchers_list = self.vouch.get_all_vouchers(caller_acc);

            // Create loan record
            let loan_id = self.next_loan_id.get_or_default();
            let loan = Loan {
                loan_id,
                borrower: caller_acc,
                amount,
                interest_rate: adjusted_rate,
                term,
                purpose: _purpose,
                start_time: self.env().block_timestamp(),
                status: LoanStatus::Active,
                vouchers: vouchers_list,
            };

            // Store the loan
            self.loans.insert(loan_id, &loan);
            self.next_loan_id.set(&(loan_id + 1));

            // Disburse funds via lending pool
            self.lending_pool.disburse(amount, caller_acc)
                .map_err(|_| Error::DisbursementFailed)?;

            // Emit LoanRequested event
            self.env().emit_event(LoanRequested {
                id: loan_id,
                borrower: caller_acc,
                amount,
                term,
            });

            Ok(loan_id)
        }

        /// Check if a loan is overdue and handle defaulting
        /// Slashes borrower's stars and resolves vouches as failed
        #[ink(message)]
        pub fn check_default(&mut self, loan_id: u64) -> Result<(), Error> {
            let mut loan = self.loans.get(loan_id).ok_or(Error::LoanNotFound)?;

            // Only active loans can be defaulted
            if loan.status != LoanStatus::Active {
                return Err(Error::LoanNotActive);
            }

            // Check if loan is overdue
            let current_time = self.env().block_timestamp();
            let due_time = loan.start_time.saturating_add(loan.term);
            if current_time <= due_time {
                return Err(Error::LoanNotOverdue);
            }

            // Mark loan as defaulted
            loan.status = LoanStatus::Defaulted;
            self.loans.insert(loan_id, &loan);

            // Slash borrower's stars via reputation contract
            // Slash amount proportional to loan amount (e.g., 1 star per 1000 units)
            let stars_to_slash = (loan.amount / 1_000_000_000_000).max(1) as u32;
            let _ = self.reputation.slash_stars(loan.borrower, stars_to_slash);

            // Resolve all vouch relationships as failed
            self.vouch.resolve_all(loan.borrower, false)
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
        fn calculate_requirements(&self, amount: Balance) -> (u32, u32) {
            // Tier 1: Small loans (< 1000 units) - minimal requirements
            // Tier 2: Medium loans (1000-10000 units) - moderate requirements
            // Tier 3: Large loans (> 10000 units) - high requirements
            let scaled_amount = amount / 1_000_000_000_000; // Scale down for comparison

            if scaled_amount < 1000 {
                // Tier 1: Small loans
                (5, 1)  // 5 stars, 1 vouch
            } else if scaled_amount < 10000 {
                // Tier 2: Medium loans
                (20, 2) // 20 stars, 2 vouches
            } else {
                // Tier 3: Large loans
                (50, 3) // 50 stars, 3 vouches
            }
        }

        /// Internal: Adjust interest rate based on borrower's stars
        /// Higher stars result in lower interest rates
        fn adjust_rate_by_stars(&self, base_rate: u64, stars: u32) -> u64 {
            // Each star reduces rate by 1%, capped at 50% reduction
            // Example: 10 stars = 10% reduction, 50+ stars = 50% reduction
            let discount_percent = (stars as u64).min(50);
            let discount = base_rate.saturating_mul(discount_percent) / 100;
            base_rate.saturating_sub(discount)
        }
    }
}