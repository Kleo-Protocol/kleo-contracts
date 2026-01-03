#![cfg_attr(not(feature = "std"), no_std, no_main)]

use ink::env::{DefaultEnvironment, Environment};

pub type AccountId = <DefaultEnvironment as Environment>::AccountId;

/// This contract manages reputation or star system across the application

#[ink::contract]
mod reputation {
    use ink::storage::Mapping;
    use ink::storage::Lazy;
    use config::ConfigRef;
    use ink::prelude::vec::Vec;

    /// Struct for User Reputation
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq)]
    pub struct UserReputation {
        stars: u32,
        stars_at_stake: u32,
        loan_history: Vec<LoanStat>,
        vouch_history: Vec<VouchStat>,
        creation_time: Timestamp,
        banned: bool,
    }

    /// Struct for Loan State
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq)]
    pub struct LoanStat {
        amount: Balance,
        repaid: bool,
    }

    /// Struct for Vouch State
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq)]
    pub struct VouchStat {
        borrower: AccountId,
        successful: bool,
    }
    /// All information that is needed to store in the contract
    #[ink(storage)]
    pub struct Reputation {
        config: ConfigRef, // Contract address of Config
        user_reps: Mapping<AccountId, UserReputation>,
        vouch_contract: Lazy<Option<AccountId>>, // Authorized vouch contract address
        loan_manager: Lazy<Option<AccountId>>, // Authorized loan manager contract address
    }


    // Custom error types for the contract
    #[derive(Debug, PartialEq, Eq)]
    #[ink::scale_derive(Encode, Decode, TypeInfo)]
    pub enum Error {
        UserNotFound,
        InsufficientStars,
        InsufficientStakedStars,
        UserBanned,
        Unauthorized,
    }

    impl Reputation {
        #[ink(constructor)]
        pub fn new(config_address: Address) -> Self {
            let config =
                ink::env::call::FromAddr::from_addr(config_address);
            Self {
                config,
                user_reps: Mapping::default(),
                vouch_contract: Lazy::default(),
                loan_manager: Lazy::default(),
            }
        }

        /// Set the vouch contract address (can only be set once)
        /// This should be called after the Vouch contract is deployed
        #[ink(message)]
        pub fn set_vouch_contract(&mut self, vouch_address: AccountId) -> Result<(), Error> {
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
        pub fn set_loan_manager(&mut self, loan_manager_address: AccountId) -> Result<(), Error> {
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
            let caller_acc = Self::env().to_account_id(caller);
            let vouch_contract = self.vouch_contract.get()
                .and_then(|opt| opt)
                .ok_or(Error::Unauthorized)?;
            if caller_acc != vouch_contract {
                return Err(Error::Unauthorized);
            }
            Ok(())
        }

        /// Internal helper to check if caller is the authorized loan manager
        fn ensure_loan_manager(&self) -> Result<(), Error> {
            let caller = Self::env().caller();
            let caller_acc = Self::env().to_account_id(caller);
            let loan_manager = self.loan_manager.get()
                .and_then(|opt| opt)
                .ok_or(Error::Unauthorized)?;
            if caller_acc != loan_manager {
                return Err(Error::Unauthorized);
            }
            Ok(())
        }

        /// Function to get stars of a user
        #[ink(message)]
        pub fn get_stars(&self, user: AccountId) -> u32 {
            self.user_reps.get(&user).map_or(0, |rep| rep.stars)
        }

        /// Function to add stars to a user
        /// Only callable by authorized contracts (loan manager or vouch contract)
        #[ink(message)]
        pub fn add_stars(&mut self, user: AccountId, amount: u32) -> Result<(), Error> {
            // Verify caller is an authorized contract (loan manager or vouch contract)
            let caller = Self::env().caller();
            let caller_acc = Self::env().to_account_id(caller);
            let loan_manager = self.loan_manager.get().and_then(|opt| opt);
            let vouch_contract = self.vouch_contract.get().and_then(|opt| opt);
            
            let is_authorized = match (loan_manager, vouch_contract) {
                (Some(lm), Some(vc)) => caller_acc == lm || caller_acc == vc,
                (Some(lm), None) => caller_acc == lm,
                (None, Some(vc)) => caller_acc == vc,
                (None, None) => false,
            };
            
            if !is_authorized {
                return Err(Error::Unauthorized);
            }

            let now = Self::env().block_timestamp();
            let cooldown_period = self.config.get_cooldown_period();

            let mut rep = self.user_reps.get(&user).unwrap_or(UserReputation {
                stars: 1,
                stars_at_stake: 0,
                loan_history: Vec::new(),
                vouch_history: Vec::new(),
                creation_time: now,
                banned: false,
            });

            // Ignore star accrual while the account is still inside its cooldown window.
            if now.saturating_sub(rep.creation_time) < cooldown_period {
                self.user_reps.insert(&user, &rep);
                return Ok(());
            }

            rep.stars += amount;

            self.user_reps.insert(&user, &rep);

            Ok(())
        }

        /// Function to check if a user can vouch based on their stars
        #[ink(message)]
        pub fn can_vouch(&self, user: AccountId) -> bool {
            let min_stars = self.config.get_min_stars_to_vouch();
            let current_stars = self.user_reps.get(&user)
                .map(|rep| if rep.banned { 0 } else { rep.stars })
                .unwrap_or(0);

            current_stars >= min_stars
        }

        /// Slash stars from a user (penalty for defaults)
        /// Only callable by the authorized loan manager contract
        #[ink(message)]
        pub fn slash_stars(&mut self, user: AccountId, amount: u32) -> Result<(), Error> {
            // Verify caller is the authorized loan manager
            self.ensure_loan_manager()?;

            let mut rep = self.user_reps.get(&user).ok_or(Error::UserNotFound)?;

            // Saturating subtract - never go below 0
            rep.stars = rep.stars.saturating_sub(amount);

            if rep.stars == 0 {
                rep.banned = true;
            }

            self.user_reps.insert(&user, &rep);

            Ok(())
        }

        /// Function to stake stars for a user
        /// Only callable by the authorized vouch contract
        #[ink(message)]
        pub fn stake_stars(&mut self, user: AccountId, amount: u32) -> Result<(), Error> {
            // Verify caller is the authorized vouch contract
            self.ensure_vouch_contract()?;

            let mut rep = self.user_reps.get(&user).ok_or(Error::UserNotFound)?;

            if rep.banned {
                return Err(Error::UserBanned);
            }

            if amount > rep.stars {
                return Err(Error::InsufficientStars);
            }

            rep.stars -= amount;
            rep.stars_at_stake += amount;

            self.user_reps.insert(&user, &rep);

            Ok(())
        }

        /// Function to unstake stars for a user after vouching and loan is repaid successfully
        /// Only callable by the authorized vouch contract
        #[ink(message)]
        pub fn unstake_stars(&mut self, user: AccountId, amount: u32, borrower: AccountId, success: bool) -> Result<(), Error> {
            // Verify caller is the authorized vouch contract
            self.ensure_vouch_contract()?;

            let mut rep = self.user_reps.get(&user).ok_or(Error::UserNotFound)?;

            if rep.banned {
                return Err(Error::UserBanned);
            }

            if amount > rep.stars_at_stake {
                return Err(Error::InsufficientStakedStars);
            }

            // Remove the staked amount first
            rep.stars_at_stake -= amount;

            if success {
                // Successful vouch -> return stake + bonus (e.g., +2 stars)
                rep.stars += amount + self.config.get_boost() as u32;

                // Update vouch history with the actual borrower
                rep.vouch_history.push(VouchStat {
                    borrower,
                    successful: true,
                });
            } else {
                // Failed vouch -> don't return stars as penalty
                // Update vouch history with the actual borrower
                rep.vouch_history.push(VouchStat {
                    borrower,
                    successful: false,
                });
            }

            self.user_reps.insert(&user, &rep);

            Ok(())
        }
    }
}