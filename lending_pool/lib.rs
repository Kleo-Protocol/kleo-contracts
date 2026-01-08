#![cfg_attr(not(feature = "std"), no_std, no_main)]

use ink::env::{DefaultEnvironment, Environment};

pub type AccountId = <DefaultEnvironment as Environment>::AccountId;

/// The lending pool contract is used to manage the pool with auto rate in accruals.
/// As Kleo uses a loan manager contract to handle loans, this contract will mainly
/// provide the pool where the contracts are created from, and handle certain calculations.

#[ink::contract]
mod lending_pool {
    use ink::storage::Mapping;
    use config::ConfigRef;
    use ink::storage::Lazy;
    use ink::U256;
    use ink::primitives::AccountIdMapper;

    /// All information that is needed to store in the contract
    #[ink(storage)]
    pub struct LendingPool{
        config: ConfigRef, // Contract address of Config
        total_liquidity: Lazy<Balance>,
        total_borrowed: Lazy<Balance>,
        reserved_funds: Lazy<Balance>,
        total_principal_deposits: Lazy<Balance>, // Total principal deposited (excluding interest)
        user_deposits: Mapping<AccountId, Balance>,
        user_staked_capital: Mapping<AccountId, Balance>, // Staked capital per user (in 10 decimals)
        last_update: Lazy<Timestamp>,
        vouch_contract: Lazy<Option<Address>>, // Authorized vouch contract address
        loan_manager: Lazy<Option<Address>>, // Authorized loan manager contract address
    }

    /// Events for lending pool actions
    #[ink(event)]
    pub struct Deposit {
        depositor: AccountId,
        amount: Balance
    }

    #[ink(event)]
    pub struct Withdraw {
        withdrawer: AccountId,
        amount: Balance,
    }

    #[ink(event)]
    pub struct RepaymentReceived {
        amount: Balance,
    }

    /// User information structure
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[ink::scale_derive(Encode, Decode, TypeInfo)]
    pub struct UserInfo {
        /// User's total deposit (in 18 decimals)
        pub deposit: Balance,
        /// User's staked capital backing vouches (in 18 decimals)
        pub staked_capital: Balance,
        /// Available balance that can be withdrawn (in 18 decimals)
        pub available_balance: Balance,
        /// User's accrued yield (in 18 decimals)
        pub yield_amount: Balance,
        /// User's total share of the pool including interest (in 18 decimals)
        pub total_share: Balance,
    }


    // Custom error types for the contract
    #[derive(Debug, PartialEq, Eq)]
    #[ink::scale_derive(Encode, Decode, TypeInfo)]
    pub enum Error {
        ZeroAmount,
        NegativeAmount,
        Overflow,
        UnavailableFunds,
        TransactionFailed,
        AmountMismatch,
        Unauthorized,
    }

    impl LendingPool {
        // Decimal conversion constant: 10^8 to convert between 10 decimals and 18 decimals
        const DECIMAL_CONVERSION: u128 = 100_000_000; // 10^8

        /// Convert amount from 18 decimals (chain format) to 10 decimals (storage format)
        fn convert_18_to_10_decimals(&self, amount_18: Balance) -> Balance {
            (amount_18 as u128)
                .checked_div(Self::DECIMAL_CONVERSION)
                .unwrap_or(0) as Balance
        }

        /// Convert amount from 10 decimals (storage format) to 18 decimals (chain format)
        fn convert_10_to_18_decimals(&self, amount_10: Balance) -> Balance {
            (amount_10 as u128)
                .checked_mul(Self::DECIMAL_CONVERSION)
                .unwrap_or(0) as Balance
        }

        #[ink(constructor)]
        pub fn new(config_address: Address) -> Self {
            let config =
                ink::env::call::FromAddr::from_addr(config_address);
            let block_timestamp = Self::env().block_timestamp();
            
            // Make this a mutable instance to set last_update to the latest block timestamp
            let mut instance = Self {
                config,
                total_liquidity: Lazy::new(),
                total_borrowed: Lazy::new(),
                reserved_funds: Lazy::new(),
                total_principal_deposits: Lazy::new(),
                user_deposits: Mapping::default(),
                user_staked_capital: Mapping::default(),
                last_update: Lazy::new(),
                vouch_contract: Lazy::default(),
                loan_manager: Lazy::default(),
            };
            instance.last_update.set(&block_timestamp);
            instance
        }

        /// Set the vouch contract address (can only be set once)
        /// This should be called after the Vouch contract is deployed
        #[ink(message)]
        pub fn set_vouch_contract(&mut self, vouch_address: Address) -> Result<(), Error> {
            // Check if vouch contract is already set
            if self.vouch_contract.get().is_some() {
                return Err(Error::Unauthorized);
            }
            self.vouch_contract.set(&Some(vouch_address));
            Ok(())
        }

        /// Set the loan manager contract address (can only be set once)
        /// This should be called after the LoanManager contract is deployed
        #[ink(message)]
        pub fn set_loan_manager(&mut self, loan_manager_address: Address) -> Result<(), Error> {
            // Check if loan manager is already set
            if self.loan_manager.get().is_some() {
                return Err(Error::Unauthorized);
            }
            self.loan_manager.set(&Some(loan_manager_address));
            Ok(())
        }

        /// Internal helper to check if caller is the authorized vouch contract
        fn ensure_vouch_contract(&self) -> Result<(), Error> {
            let caller = Self::env().caller();
            let vouch_contract = self.vouch_contract.get()
                .and_then(|opt| opt)
                .ok_or(Error::Unauthorized)?;
            if caller != vouch_contract {
                return Err(Error::Unauthorized);
            }
            Ok(())
        }

        /// Internal helper to check if caller is the authorized loan manager
        fn ensure_loan_manager(&self) -> Result<(), Error> {
            let caller = Self::env().caller();
            let loan_manager = self.loan_manager.get()
                .and_then(|opt| opt)
                .ok_or(Error::Unauthorized)?;
            if caller != loan_manager {
                return Err(Error::Unauthorized);
            }
            Ok(())
        }

        /// Deposits to the lending pool
        #[ink(message, payable)]
        pub fn deposit(&mut self, account_id: AccountId) -> Result<Balance, Error> {
            let deposited_u256 = self.env().transferred_value();
            if deposited_u256 == U256::zero() {
                return Err(Error::ZeroAmount);
            }

            if deposited_u256 > U256::from(u128::MAX) {
                return Err(Error::Overflow);
            }
            let deposited_18: Balance = deposited_u256.as_u128(); // 18 decimals from chain

            let caller_acc = account_id;

            // Convert to 10 decimals for user_deposits storage
            let deposited_10 = self.convert_18_to_10_decimals(deposited_18);

            // Update the user's deposit balance (stored in 10 decimals)
            let current_balance = self.user_deposits.get(&caller_acc).unwrap_or(0);
            let new_balance = current_balance.saturating_add(deposited_10);
            self.user_deposits.insert(&caller_acc, &new_balance);
            
            // Verify the insert worked (read back immediately)
            let verified_balance = self.user_deposits.get(&caller_acc).unwrap_or(0);
            
            // Update total liquidity (stored in 18 decimals)
            let mut total_liquidity = self.total_liquidity.get_or_default();
            total_liquidity = total_liquidity.saturating_add(deposited_18);
            self.total_liquidity.set(&total_liquidity);
            
            // Update total principal deposits (stored in 18 decimals)
            let mut total_principal = self.total_principal_deposits.get_or_default();
            total_principal = total_principal.saturating_add(deposited_18);
            self.total_principal_deposits.set(&total_principal);

            // Emit deposit event (use 18 decimals for consistency)
            self.env().emit_event(Deposit {
                depositor: caller_acc,
                amount: deposited_18,
            });

            Ok(verified_balance)
        }

        /// Withdraw funds from the lending pool
        /// amount: in 10 decimals (storage format)
        #[ink(message)]
        pub fn withdraw(&mut self, amount: Balance, account_id: AccountId) -> Result<(), Error> {
            let caller_acc = account_id;
            if amount == 0 {
                return Err(Error::ZeroAmount);
            }

            self.accrue_interest();

            // Convert amount from 10 decimals to 18 decimals for calculations
            let amount_18 = self.convert_10_to_18_decimals(amount);

            let user_deposit = self.user_deposits.get(&caller_acc).unwrap_or(0);
            let total_liquidity = self.total_liquidity.get_or_default();
            let total_principal = self.total_principal_deposits.get_or_default();
            
            // Convert user_deposit from 10 decimals to 18 decimals for comparison
            let user_deposit_18 = self.convert_10_to_18_decimals(user_deposit);
            
            // Get user's staked capital (in 10 decimals) and convert to 18 decimals
            let user_staked_10 = self.user_staked_capital.get(&caller_acc).unwrap_or(0);
            let user_staked_18 = self.convert_10_to_18_decimals(user_staked_10);
            
            // Calculate user's share of the pool (principal + interest)
            // User can withdraw up to their share: (user_deposit / total_principal) * total_liquidity
            // If total_liquidity >= total_principal, user_share should be >= user_deposit_18
            // If total_liquidity < total_principal (loans disbursed), user_share will be < user_deposit_18
            let user_share = if total_principal > 0 && total_liquidity > 0 {
                // Calculate user's share with overflow protection
                // If calculation overflows, fall back to user_deposit_18 (at least their principal)
                (user_deposit_18 as u128)
                    .checked_mul(total_liquidity as u128)
                    .and_then(|v| v.checked_div(total_principal as u128))
                    .unwrap_or(user_deposit_18) as Balance
            } else {
                user_deposit_18 // Fallback if no principal or liquidity
            };
            
            // Cap user_share at total_liquidity (can't withdraw more than what's in the pool)
            let user_share = user_share.min(total_liquidity);
            
            // Calculate available balance: user_share minus staked capital
            // Users cannot withdraw staked capital that's backing active vouches
            let available_balance = user_share.saturating_sub(user_staked_18);
            
            if amount_18 > available_balance {
                return Err(Error::UnavailableFunds);
            }
            
            // Calculate the user's share of the pool (principal + interest)
            // User's share = (user_deposit / total_principal_deposits) * total_liquidity
            // When withdrawing, we need to reduce both user_deposit and total_principal proportionally
            let principal_to_reduce_18 = if total_principal > 0 && total_liquidity > 0 {
                // The withdrawal amount represents a fraction of total_liquidity
                // Reduce principal by the same fraction
                (amount_18 as u128)
                    .checked_mul(total_principal as u128)
                    .and_then(|v| v.checked_div(total_liquidity as u128))
                    .unwrap_or(0) as Balance
            } else {
                amount_18 // Fallback if no principal or liquidity
            };

            // Convert principal_to_reduce back to 10 decimals for user_deposits storage
            let principal_to_reduce_10 = self.convert_18_to_10_decimals(principal_to_reduce_18);

            // Update user's deposit balance (reduce by principal portion, stored in 10 decimals)
            let new_user_deposit = user_deposit.saturating_sub(principal_to_reduce_10);
            self.user_deposits.insert(&caller_acc, &new_user_deposit);

            // Update total liquidity (stored in 18 decimals)
            let mut total_liquidity = self.total_liquidity.get_or_default();
            total_liquidity = total_liquidity.saturating_sub(amount_18);
            self.total_liquidity.set(&total_liquidity);
            
            // Update total principal deposits (stored in 18 decimals)
            let mut total_principal = self.total_principal_deposits.get_or_default();
            total_principal = total_principal.saturating_sub(principal_to_reduce_18);
            self.total_principal_deposits.set(&total_principal);

            // Transfer in 18 decimals (chain format)
            if self.env().transfer(AccountIdMapper::to_address(caller_acc.as_ref()), U256::from(amount_18)).is_err() {
                return Err(Error::TransactionFailed);
            }

            self.env().emit_event(Withdraw {
                withdrawer: caller_acc,
                amount: amount_18, // Emit in 18 decimals
            });

            Ok(())
        }

        #[ink(message)]
        pub fn get_current_rate(&self) -> u64 {
            let total_liquidity = self.total_liquidity.get_or_default();
            if total_liquidity == 0 {
                return self.config.get_base_interest_rate();
            }

            let total_borrowed = self.total_borrowed.get_or_default();

            // Utilization = borrowed / liquidity, scaled by 1e9 for precision
            // Use checked arithmetic to prevent overflow traps when values are very large
            let utilization = (total_borrowed as u128)
                .checked_mul(1_000_000_000u128)
                .and_then(|v| v.checked_div(total_liquidity as u128))
                .unwrap_or(0) as u64;
            
            // Cap utilization at 1e9 (100%) to prevent issues with calculation overflow
            // This protects against cases where stored values are incorrectly scaled
            let utilization = utilization.min(1_000_000_000u64);

            let base = self.config.get_base_interest_rate();
            let optimal = self.config.get_optimal_utilization();
            let slope1 = self.config.get_slope1();
            let slope2 = self.config.get_slope2();
            let max_rate = self.config.get_max_rate();

            // Safety check: if optimal is 0, return base rate to prevent division by zero
            if optimal == 0 {
                return base.min(max_rate);
            }

            let rate = if utilization <= optimal {
                // base + (utilization / optimal) * slope1
                // Use checked arithmetic to prevent overflow
                let additional = (utilization as u128)
                    .checked_mul(slope1 as u128)
                    .and_then(|v| v.checked_div(optimal as u128))
                    .unwrap_or(0) as u64;
                base.saturating_add(additional)
            } else {
                // base + slope1 + ((utilization - optimal) / (1e9 - optimal)) * slope2
                let excess = utilization.saturating_sub(optimal);
                let max_excess = 1_000_000_000u64.saturating_sub(optimal);
                let additional = if max_excess == 0 {
                    0
                } else {
                    // Use checked arithmetic to prevent overflow
                    (excess as u128)
                        .checked_mul(slope2 as u128)
                        .and_then(|v| v.checked_div(max_excess as u128))
                        .unwrap_or(0) as u64
                };
                base.saturating_add(slope1).saturating_add(additional)
            };

            // Cap at maximum allowed rate to prevent overflow
            rate.min(max_rate)
        }

        /// Internal function to get accrued interest since last update
        /// If enough time has passed, it will update total liquidity and reserved funds
        fn accrue_interest(&mut self) {
            let current_time = self.env().block_timestamp();
            let last = self.last_update.get_or_default();
            let elapsed = current_time.saturating_sub(last);
            if elapsed == 0 {
                return;
            }

            let total_borrowed = self.total_borrowed.get_or_default();
            if total_borrowed == 0 {
                self.last_update.set(&current_time);
                return;
            }

            // Get current dynamic rate (same logic as get_current_rate)
            let rate = self.get_current_rate(); // Reuses the public logic

            // Yearly denominator for scaled rates (assuming rates are in "per year" basis)
            // 365.25 days * 24 hours * 60 min * 60 sec * 1000 ms ≈ 31_557_600_000 ms
            const YEAR_MS: u128 = 31_557_600_000u128;

            // interest = borrowed * rate * elapsed_ms / YEAR_MS
            // All values scaled appropriately (rate already scaled by 1e9, e.g., 5% = 5_000_000_000, 10% = 10_000_000_000)
            let interest = (total_borrowed as u128)
                .checked_mul(rate as u128)
                .and_then(|v| v.checked_mul(elapsed as u128))
                .and_then(|v| v.checked_div(YEAR_MS))
                .unwrap_or(0) as Balance;

            if interest == 0 {
                self.last_update.set(&current_time);
                return;
            }

            // Add interest to total liquidity
            let mut liquidity = self.total_liquidity.get_or_default();
            liquidity = liquidity.saturating_add(interest);
            self.total_liquidity.set(&liquidity);

            // Skim reserve factor
            let reserve_factor = self.config.get_reserve_factor(); // e.g., 10
            let reserve_add = interest.saturating_mul(reserve_factor as Balance) / 100;
            let mut reserves = self.reserved_funds.get_or_default();
            reserves = reserves.saturating_add(reserve_add);
            self.reserved_funds.set(&reserves);

            // Update timestamp
            self.last_update.set(&current_time);

            // Optional: emit event
            // self.env().emit_event(InterestAccrued { amount: interest, reserves: reserve_add });
        }

        /// Get user's accrued yield (read-only, doesn't accrue interest)
        /// Returns yield in 18 decimals (chain format)
        /// Note: This calculates yield based on current state without accruing interest.
        /// For up-to-date yield, call accrue_interest_and_get_user_yield instead.
        #[ink(message)]
        pub fn get_user_yield(&self, account_id: AccountId) -> Balance {
            self.calculate_user_yield(account_id)
        }

        /// Get user's accrued yield and ensure interest is up-to-date
        /// Returns yield in 18 decimals (chain format)
        /// This version accrues interest before calculating yield for accurate results
        #[ink(message)]
        pub fn accrue_interest_and_get_user_yield(&mut self, account_id: AccountId) -> Balance {
            // First, ensure interest is up-to-date
            self.accrue_interest();
            self.calculate_user_yield(account_id)
        }

        /// Internal helper to calculate user yield without mutating state
        fn calculate_user_yield(&self, account_id: AccountId) -> Balance {
            let caller_acc = account_id;

            let user_deposit_10 = self.user_deposits.get(&caller_acc).unwrap_or(0);
            if user_deposit_10 == 0 {
                return 0;
            }

            // Convert user_deposit from 10 decimals to 18 decimals for calculations
            let user_deposit_18 = self.convert_10_to_18_decimals(user_deposit_10);

            let total_liquidity = self.total_liquidity.get_or_default();
            if total_liquidity == 0 {
                return 0;
            }

            // Calculate total accrued interest: current liquidity minus total principal deposits
            // This represents the interest that has been earned by all depositors
            let total_principal = self.total_principal_deposits.get_or_default();
            let accrued_interest = total_liquidity.saturating_sub(total_principal);

            // Calculate user's pro-rata share of the interest based on their deposit proportion
            // If no principal deposits exist, return 0 to avoid division by zero
            if total_principal == 0 {
                return 0;
            }

            // User's yield = (user_deposit_18 / total_principal_deposits) * total_accrued_interest
            // All values are in 18 decimals
            // Use checked arithmetic to prevent overflow
            (user_deposit_18 as u128)
                .checked_mul(accrued_interest as u128)
                .and_then(|v| v.checked_div(total_principal as u128))
                .unwrap_or(0) as Balance
        }

        /// Get user's deposit balance
        #[ink(message)]
        pub fn get_user_deposit(&self, user: AccountId) -> Balance {
            self.user_deposits.get(&user).unwrap_or(0)
        }

        #[ink(message)]
        pub fn get_total_liquidity(&self) -> Balance {
            self.total_liquidity.get_or_default()
        }

        /// Disburse part of liquidity (add a borrow basically)
        /// Only callable by the authorized loan manager contract
        /// amount: in 10 decimals (storage format)
        #[ink(message)]
        pub fn disburse(&mut self, amount: Balance, to: AccountId) -> Result<(), Error> {
            // Verify caller is the authorized loan manager
            self.ensure_loan_manager()?;

            self.accrue_interest();

            // Convert amount from 10 decimals to 18 decimals for calculations and transfer
            let amount_18 = self.convert_10_to_18_decimals(amount);

            let mut total_borrowed = self.total_borrowed.get_or_default();
            let mut total_liquidity = self.total_liquidity.get_or_default();

            if amount_18 > (total_liquidity - total_borrowed) {
                return Err(Error::UnavailableFunds);
            }

            // Update total liquidity and total borrowed (both in 18 decimals)
            total_borrowed = total_borrowed.saturating_add(amount_18);
            total_liquidity = total_liquidity.saturating_sub(amount_18);
            self.total_borrowed.set(&total_borrowed);
            self.total_liquidity.set(&total_liquidity);

            // Transfer disbursed amount to the borrower (in 18 decimals)
            if self.env().transfer(AccountIdMapper::to_address(to.as_ref()), U256::from(amount_18)).is_err() {
                return Err(Error::TransactionFailed);
            }

            Ok(())
        }

        /// Repay a loan (reduce borrowed amount)
        /// amount: in 18 decimals (chain format, matching transferred_value)
        #[ink(message, payable)]
        pub fn receive_repayment(&mut self, amount: Balance) -> Result<(), Error> {
            let received_u256 = self.env().transferred_value();
            if received_u256 == U256::zero() {
                return Err(Error::ZeroAmount);
            }
            if received_u256 > U256::from(u128::MAX) {
                return Err(Error::Overflow);
            }
            let received: Balance = received_u256.as_u128(); // 18 decimals

            if received != amount {
                return Err(Error::AmountMismatch);
            }

            self.accrue_interest();

            // Update total borrowed and total liquidity
            let mut borrowed = self.total_borrowed.get_or_default();
            borrowed = borrowed.saturating_sub(amount);
            self.total_borrowed.set(&borrowed);

            let mut liquidity = self.total_liquidity.get_or_default();
            liquidity = liquidity.saturating_add(amount);
            self.total_liquidity.set(&liquidity);

            self.env().emit_event(RepaymentReceived { amount });

            Ok(())
        }

        /// Slash part of the position of a voucher
        /// Only callable by the authorized vouch contract
        /// amount: in 10 decimals (storage format)
        /// Note: This function also decreases staked capital tracking automatically
        #[ink(message)]
        pub fn slash_stake(&mut self, user: AccountId, amount: Balance) -> Result<(), Error> {
            // Verify caller is the authorized vouch contract
            self.ensure_vouch_contract()?;

            self.accrue_interest();
            
            let user_balance = self.user_deposits.get(&user).unwrap_or(0);
            if amount > user_balance {
                return Err(Error::UnavailableFunds);
            }
            
            // Decrease staked capital tracking (release the lock)
            let current_staked = self.user_staked_capital.get(&user).unwrap_or(0);
            if amount > current_staked {
                // If slashing more than staked, just clear all staked capital
                self.user_staked_capital.remove(&user);
            } else {
                let new_staked = current_staked.saturating_sub(amount);
                if new_staked == 0 {
                    self.user_staked_capital.remove(&user);
                } else {
                    self.user_staked_capital.insert(&user, &new_staked);
                }
            }
            
            // Convert amount from 10 decimals to 18 decimals for calculations
            let amount_18 = self.convert_10_to_18_decimals(amount);
            
            let total_liquidity = self.total_liquidity.get_or_default();
            let total_principal = self.total_principal_deposits.get_or_default();
            
            // Calculate principal reduction (same logic as withdraw)
            let principal_to_reduce_18 = if total_principal > 0 && total_liquidity > 0 {
                (amount_18 as u128)
                    .checked_mul(total_principal as u128)
                    .and_then(|v| v.checked_div(total_liquidity as u128))
                    .unwrap_or(0) as Balance
            } else {
                amount_18
            };

            // Convert principal_to_reduce back to 10 decimals for user_deposits storage
            let principal_to_reduce_10 = self.convert_18_to_10_decimals(principal_to_reduce_18);

            // Update user's deposit balance (stored in 10 decimals)
            let new_user_balance = user_balance.saturating_sub(principal_to_reduce_10);
            self.user_deposits.insert(&user, &new_user_balance);

            // Update total liquidity (stored in 18 decimals)
            let mut total_liquidity = self.total_liquidity.get_or_default();
            total_liquidity = total_liquidity.saturating_sub(amount_18);
            self.total_liquidity.set(&total_liquidity);
            
            // Update total principal deposits (stored in 18 decimals)
            let mut total_principal = self.total_principal_deposits.get_or_default();
            total_principal = total_principal.saturating_sub(principal_to_reduce_18);
            self.total_principal_deposits.set(&total_principal);

            // Note: Reserved funds are NOT added here anymore
            // They are only added in handle_default_recovery if there's a deficit

            Ok(())
        }

        /// Handle default recovery: compare total slashed capital to loan amount
        /// Only callable by the authorized vouch contract
        /// total_slashed_capital: Total capital slashed from all vouchers (in 10 decimals)
        /// loan_amount: The loan amount that defaulted (in 10 decimals)
        /// 
        /// Logic:
        /// - If slashed capital >= loan amount: Add loan amount back to liquidity (covers the default)
        /// - If slashed capital < loan amount: Add slashed amount back to liquidity, add deficit to reserved_funds
        #[ink(message)]
        pub fn handle_default_recovery(&mut self, total_slashed_capital: Balance, loan_amount: Balance) -> Result<(), Error> {
            // Verify caller is the authorized vouch contract
            self.ensure_vouch_contract()?;

            // Convert to 18 decimals for calculations
            let total_slashed_18 = self.convert_10_to_18_decimals(total_slashed_capital);
            let loan_amount_18 = self.convert_10_to_18_decimals(loan_amount);

            // Get current state before modifications
            let current_liquidity = self.total_liquidity.get_or_default();
            let current_principal = self.total_principal_deposits.get_or_default();

            if total_slashed_18 >= loan_amount_18 {
                // Slashed capital is sufficient to cover the default
                // Add the loan amount back to liquidity (this covers the loss from the default)
                let mut total_liquidity = current_liquidity;
                total_liquidity = total_liquidity.saturating_add(loan_amount_18);
                self.total_liquidity.set(&total_liquidity);

                // Also add back to total_principal_deposits proportionally
                // Calculate how much principal to add back (same proportion as current pool)
                let principal_to_add_18 = if current_liquidity > 0 && current_principal > 0 {
                    (loan_amount_18 as u128)
                        .checked_mul(current_principal as u128)
                        .and_then(|v| v.checked_div(current_liquidity as u128))
                        .unwrap_or(0) as Balance
                } else {
                    loan_amount_18
                };
                let mut total_principal = current_principal;
                total_principal = total_principal.saturating_add(principal_to_add_18);
                self.total_principal_deposits.set(&total_principal);
            } else {
                // Slashed capital is insufficient - there's a deficit
                // Add what we can recover back to liquidity
                let mut total_liquidity = current_liquidity;
                total_liquidity = total_liquidity.saturating_add(total_slashed_18);
                self.total_liquidity.set(&total_liquidity);

                // Add back to total_principal_deposits proportionally
                let principal_to_add_18 = if current_liquidity > 0 && current_principal > 0 {
                    (total_slashed_18 as u128)
                        .checked_mul(current_principal as u128)
                        .and_then(|v| v.checked_div(current_liquidity as u128))
                        .unwrap_or(0) as Balance
                } else {
                    total_slashed_18
                };
                let mut total_principal = current_principal;
                total_principal = total_principal.saturating_add(principal_to_add_18);
                self.total_principal_deposits.set(&total_principal);

                // Calculate deficit and add to reserved funds
                let deficit = loan_amount_18.saturating_sub(total_slashed_18);
                let mut reserved_funds = self.reserved_funds.get_or_default();
                reserved_funds = reserved_funds.saturating_add(deficit);
                self.reserved_funds.set(&reserved_funds);
            }

            Ok(())
        }

        /// Increase staked capital for a user (only callable by vouch contract)
        /// amount: in 10 decimals (storage format)
        #[ink(message)]
        pub fn increase_staked_capital(&mut self, user: AccountId, amount: Balance) -> Result<(), Error> {
            // Verify caller is the authorized vouch contract
            self.ensure_vouch_contract()?;

            if amount == 0 {
                return Err(Error::ZeroAmount);
            }

            let user_deposit = self.user_deposits.get(&user).unwrap_or(0);
            let current_staked = self.user_staked_capital.get(&user).unwrap_or(0);
            let new_staked = current_staked.saturating_add(amount);

            // Ensure staked capital doesn't exceed user's deposit
            if new_staked > user_deposit {
                return Err(Error::UnavailableFunds);
            }

            self.user_staked_capital.insert(&user, &new_staked);
            Ok(())
        }

        /// Decrease staked capital for a user (only callable by vouch contract)
        /// amount: in 10 decimals (storage format)
        #[ink(message)]
        pub fn decrease_staked_capital(&mut self, user: AccountId, amount: Balance) -> Result<(), Error> {
            // Verify caller is the authorized vouch contract
            self.ensure_vouch_contract()?;

            if amount == 0 {
                return Err(Error::ZeroAmount);
            }

            let current_staked = self.user_staked_capital.get(&user).unwrap_or(0);
            if amount > current_staked {
                return Err(Error::UnavailableFunds);
            }

            let new_staked = current_staked.saturating_sub(amount);
            if new_staked == 0 {
                self.user_staked_capital.remove(&user);
            } else {
                self.user_staked_capital.insert(&user, &new_staked);
            }
            Ok(())
        }

        /// Get user's staked capital
        /// Returns amount in 10 decimals (storage format)
        #[ink(message)]
        pub fn get_user_staked_capital(&self, user: AccountId) -> Balance {
            self.user_staked_capital.get(&user).unwrap_or(0)
        }

        /// Get comprehensive user information
        /// Returns all user data including deposit, staked capital, available balance, and yield
        #[ink(message)]
        pub fn get_user_info(&self, account_id: AccountId) -> UserInfo {
            let user_deposit_10 = self.user_deposits.get(&account_id).unwrap_or(0);
            let user_staked_10 = self.user_staked_capital.get(&account_id).unwrap_or(0);
            
            // Convert to 18 decimals for return values
            let user_deposit_18 = self.convert_10_to_18_decimals(user_deposit_10);
            let user_staked_18 = self.convert_10_to_18_decimals(user_staked_10);
            
            // Calculate user's share and available balance
            let total_liquidity = self.total_liquidity.get_or_default();
            let total_principal = self.total_principal_deposits.get_or_default();
            
            let user_share = if total_principal > 0 && total_liquidity > 0 {
                (user_deposit_18 as u128)
                    .checked_mul(total_liquidity as u128)
                    .and_then(|v| v.checked_div(total_principal as u128))
                    .unwrap_or(user_deposit_18) as Balance
            } else {
                user_deposit_18
            };
            
            let user_share = user_share.min(total_liquidity);
            let available_balance = user_share.saturating_sub(user_staked_18);
            
            // Calculate yield
            let yield_amount = self.calculate_user_yield(account_id);
            
            UserInfo {
                deposit: user_deposit_18, // 18 decimals
                staked_capital: user_staked_18, // 18 decimals
                available_balance, // 18 decimals
                yield_amount, // 18 decimals
                total_share: user_share, // 18 decimals
            }
        }
    }
}