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

            Ok(())
        }

        /// Get user's deposit balance
        #[ink(message)]
        pub fn get_user_deposit(&self, user: Address) -> Balance {
            self.user_deposits.get(&user).unwrap_or(0)
        }

        /// Disburse part of liquidity (add a borrow basically)
        #[ink(message)]
        pub fn disburse(&mut self, amount: Balance, to: Address) -> Result<(), Error> {
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

            self.accrue_interest(); // Update yields first

            let mut borrowed = self.total_borrowed.get_or_default();
            borrowed = borrowed.saturating_sub(amount);
            self.total_borrowed.set(&borrowed);

            let mut liquidity = self.total_liquidity.get_or_default();
            liquidity += amount;
            self.total_liquidity.set(&liquidity);

            // Optional: Emit event if needed
            self.env().emit_event(RepaymentReceived { amount });

            Ok(())
        }

        /// Slash part of the position of a voucher
        #[ink(message)]
        pub fn slash_stake(&mut self, user: Address, amount: Balance) -> Result<(), Error> {
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