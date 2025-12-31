#![cfg_attr(not(feature = "std"), no_std, no_main)]

use ink::env::{DefaultEnvironment, Environment};

pub type AccountId = <DefaultEnvironment as Environment>::AccountId;

/// This contract manages reputation or star system across the application

#[ink::contract]
mod reputation {
    use ink::storage::Mapping;
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
    }


    // Custom error types for the contract
    #[derive(Debug, PartialEq, Eq)]
    #[ink::scale_derive(Encode, Decode, TypeInfo)]
    pub enum Error {
        UserNotFound,
        InsufficientStars,
        InsufficientStakedStars,
        UserBanned,
    }

    impl Reputation {
        #[ink(constructor)]
        pub fn new(config_address: Address) -> Self {
            let config =
                ink::env::call::FromAddr::from_addr(config_address);
            Self {
                config,
                user_reps: Mapping::default(),
            }
        }

        /// Function to get stars of a user
        #[ink(message)]
        pub fn get_stars(&self, user: AccountId) -> u32 {
            self.user_reps.get(&user).map_or(0, |rep| rep.stars)
        }

        /// Function to add stars to a user
        #[ink(message)]
        pub fn add_stars(&mut self, user: AccountId, amount: u32) -> Result<(), Error> {
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

        #[ink(message)]
        pub fn slash_stars(&mut self, user: AccountId, amount: u32) -> Result<(), Error> {
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
        #[ink(message)]
        pub fn stake_stars(&mut self, user: AccountId, amount: u32) -> Result<(), Error> {
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
        #[ink(message)]
        pub fn unstake_stars(&mut self, user: AccountId, amount: u32, success: bool) -> Result<(), Error> {
            let mut rep = self.user_reps.get(&user).ok_or(Error::UserNotFound)?;
            let caller= Self::env().caller();
            let caller_acc = Self::env().to_account_id(caller);

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

                // Update vouch history
                rep.vouch_history.push(VouchStat {
                    borrower: caller_acc,
                    successful: true,
                });
            } else {
                // Failed vouch -> don't return stars as penalty
                // Update vouch history
                rep.vouch_history.push(VouchStat {
                    borrower: caller_acc,
                    successful: false,
                });
            }

            self.user_reps.insert(&user, &rep);

            Ok(())
        }
    }
}