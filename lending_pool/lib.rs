#![cfg_attr(not(feature = "std"), no_std, no_main)]

/// The lending pool contract is used to manage the pool with auto rate in accruals.
/// As Kleo uses a loan manager contract to handle loans, this contract will mainly
/// provide the pool where the contracts are created from, and handle certain calculations.

#[ink::contract]
mod lending_pool {
    use ink::storage::Mapping;
    use config::ConfigRef;
    use ink::storage::Lazy;
    use ink::U256;

    /// All information that is needed to store in the contract
    #[ink(storage)]
    pub struct LendingPool{
        config: ConfigRef, // Contract address of Config
        total_liquidity: Lazy<Balance>,
        total_borrowed: Lazy<Balance>,
        reserved_funds: Lazy<Balance>,
        user_deposits: Mapping<Address, Balance>,
        last_update: Lazy<Timestamp>,
    }

    /// Events for lending pool actions
    #[ink(event)]
    pub struct Deposit {
        depositor: Address,
        amount: Balance
    }

    #[ink(event)]
    pub struct Withdraw {
        withdrawer: Address,
        amount: Balance,
    }

    #[ink(event)]
    pub struct RepaymentReceived {
        amount: Balance,
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
        AmountMismatch
    }

    impl LendingPool {
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
                user_deposits: Mapping::default(),
                last_update: Lazy::new(),
            };
            instance.last_update.set(&block_timestamp);
            instance
        }

        /// Deposits to the lending pool
        #[ink(message, payable)]
        pub fn deposit(&mut self) -> Result<(), Error> {
            let deposited_u256 = self.env().transferred_value();
            if deposited_u256 == U256::zero() {
                return Err(Error::ZeroAmount);
            }

            if deposited_u256 > U256::from(u128::MAX) {
                return Err(Error::Overflow);
            }
            let deposited: Balance = deposited_u256.as_u128();

            let caller: Address = self.env().caller();

            // Update the user's deposit balance
            let current_balance = self.user_deposits.get(&caller).unwrap_or(0);
            self.user_deposits.insert(&caller, &(current_balance + deposited));

            // Update total liquidity
            let mut total_liquidity = self.total_liquidity.get_or_default();
            total_liquidity += deposited;
            self.total_liquidity.set(&total_liquidity);

            // Emit deposit event
            self.env().emit_event(Deposit {
                depositor: caller,
                amount: deposited,
            });

            Ok(())
        }

        /// Withdraw funds from the lending pool
        #[ink(message)]
        pub fn withdraw(&mut self, amount: Balance) -> Result<(), Error> {
            let caller: Address = self.env().caller();
            if amount == 0 {
                return Err(Error::ZeroAmount);
            }

            self.accrue_interest();

            let user_balance = self.user_deposits.get(&caller).unwrap_or(0);
            if amount > user_balance {
                return Err(Error::UnavailableFunds);
            }
            // Update user's deposit balance
            self.user_deposits.insert(&caller, &(user_balance - amount));

            // Update total liquidity
            let mut total_liquidity = self.total_liquidity.get_or_default();
            total_liquidity -= amount;
            self.total_liquidity.set(&total_liquidity);

            if self.env().transfer(caller, U256::from(amount)).is_err() {
                return Err(Error::TransactionFailed);
            }

            self.env().emit_event(Withdraw {
                withdrawer: caller,
                amount,
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
            let utilization = (total_borrowed as u128 * 1_000_000_000u128)
                .checked_div(total_liquidity as u128)
                .unwrap_or(0) as u64;

            let base = self.config.get_base_interest_rate();
            let optimal = self.config.get_optimal_utilization();
            let slope1 = self.config.get_slope1();
            let slope2 = self.config.get_slope2();
            let max_rate = self.config.get_max_rate();

            let rate = if utilization <= optimal {
                // base + (utilization / optimal) * slope1
                base + (utilization as u128 * slope1 as u128 / optimal as u128) as u64
            } else {
                // base + slope1 + ((utilization - optimal) / (1e9 - optimal)) * slope2
                let excess = utilization.saturating_sub(optimal);
                let max_excess = 1_000_000_000u64.saturating_sub(optimal);
                let additional = if max_excess == 0 {
                    0
                } else {
                    (excess as u128 * slope2 as u128 / max_excess as u128) as u64
                };
                base + slope1 + additional
            };

            // Cap at maximum allowed rate
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
            // All values scaled appropriately (rate already scaled, e.g., 5% = 50_000_000 for 1e9 base)
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
            liquidity += interest;
            self.total_liquidity.set(&liquidity);

            // Skim reserve factor
            let reserve_factor = self.config.get_reserve_factor(); // e.g., 10
            let reserve_add = interest.saturating_mul(reserve_factor as Balance) / 100;
            let mut reserves = self.reserved_funds.get_or_default();
            reserves += reserve_add;
            self.reserved_funds.set(&reserves);

            // Update timestamp
            self.last_update.set(&current_time);

            // Optional: emit event
            // self.env().emit_event(InterestAccrued { amount: interest, reserves: reserve_add });
        }

        /// Get user's accrued yield
        #[ink(message)]
        pub fn get_user_yield(&mut self, user: Address) -> Balance {
            // First, ensure interest is up-to-date
            self.accrue_interest();

            let user_deposit = self.user_deposits.get(&user).unwrap_or(0);
            if user_deposit == 0 {
                return 0;
            }

            let total_liquidity = self.total_liquidity.get_or_default();
            if total_liquidity == 0 {
                return 0;
            }

            // Total liquidity includes all accrued interest up to now.
            // The "base" liquidity without interest is approximated as total_liquidity - total_borrowed
            // (since borrowed principal doesn't earn interest, only the supplied part does).
            // More precisely: the interest earned by suppliers = total_liquidity - (total_supplied_principal)
            // But we don't track total_supplied_principal separately, so we use:
            // total_liquidity - total_borrowed as the accrued interest pool.
            let accrued_interest_pool = total_liquidity.saturating_sub(self.total_borrowed.get_or_default());

            // Pro-rata share
            (user_deposit as u128 * accrued_interest_pool as u128 / total_liquidity as u128) as Balance
        }

        /// Get user's deposit balance
        #[ink(message)]
        pub fn get_user_deposit(&self, user: Address) -> Balance {
            self.user_deposits.get(&user).unwrap_or(0)
        }

        /// Disburse part of liquidity (add a borrow basically)
        #[ink(message)]
        pub fn disburse(&mut self, amount: Balance, to: Address) -> Result<(), Error> {
            self.accrue_interest();

            let mut total_borrowed = self.total_borrowed.get_or_default();
            let mut total_liquidity = self.total_liquidity.get_or_default();

            if amount > (total_liquidity - total_borrowed) {
                return Err(Error::UnavailableFunds);
            }

            // Update total liquidity and total borrowed
            total_borrowed += amount;
            total_liquidity -= amount;
            self.total_borrowed.set(&total_borrowed);
            self.total_liquidity.set(&total_liquidity);

            // Transfer disbursed amount to the borrower
            if self.env().transfer(to, U256::from(amount)).is_err() {
                return Err(Error::TransactionFailed);
            }

            Ok(())
        }

        /// Repay a loan (reduce borrowed amount)
        #[ink(message, payable)]
        pub fn receive_repayment(&mut self, amount: Balance) -> Result<(), Error> {
            let received_u256 = self.env().transferred_value();
            if received_u256 == U256::zero() {
                return Err(Error::ZeroAmount);
            }
            if received_u256 > U256::from(u128::MAX) {
                return Err(Error::Overflow);
            }
            let received: Balance = received_u256.as_u128();

            if received != amount {
                return Err(Error::AmountMismatch);
            }

            self.accrue_interest();

            // Update total borrowed and total liquidity
            let mut borrowed = self.total_borrowed.get_or_default();
            borrowed = borrowed.saturating_sub(amount);
            self.total_borrowed.set(&borrowed);

            let mut liquidity = self.total_liquidity.get_or_default();
            liquidity += amount;
            self.total_liquidity.set(&liquidity);

            self.env().emit_event(RepaymentReceived { amount });

            Ok(())
        }

        /// Slash part of the position of a voucher
        #[ink(message)]
        pub fn slash_stake(&mut self, user: Address, amount: Balance) -> Result<(), Error> {
            self.accrue_interest();
            
            let user_balance = self.user_deposits.get(&user).unwrap_or(0);
            if amount > user_balance {
                return Err(Error::UnavailableFunds);
            }
            // Update user's deposit balance
            self.user_deposits.insert(&user, &(user_balance - amount));

            // Update total liquidity
            let mut total_liquidity = self.total_liquidity.get_or_default();
            total_liquidity -= amount;
            self.total_liquidity.set(&total_liquidity);

            // Update reserved funds
            let mut reserved_funds = self.reserved_funds.get_or_default();
            reserved_funds += amount;
            self.reserved_funds.set(&reserved_funds);

            Ok(())
        }
    }
}