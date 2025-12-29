#![cfg_attr(not(feature = "std"), no_std, no_main)]

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
        amount: Balance,
        successful: bool,
    }
    /// All information that is needed to store in the contract
    #[ink(storage)]
    pub struct Reputation {
        config: ConfigRef, // Contract address of Config
        user_reps: Mapping<Address, UserReputation>,
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

        #[ink(message)]
        pub fn get_stars(&self, user: Address) -> u32 {
            self.user_reps.get(&user).map_or(0, |rep| rep.stars)
        }
    }
}