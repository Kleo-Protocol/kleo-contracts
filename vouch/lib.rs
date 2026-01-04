#![cfg_attr(not(feature = "std"), no_std, no_main)]

use ink::env::{DefaultEnvironment, Environment};

pub type AccountId = <DefaultEnvironment as Environment>::AccountId;

/// This contract manages vouch relationships between users

#[ink::contract]
mod vouch {
    use ink::storage::Mapping;
    use ink::storage::Lazy;
    use ink::prelude::vec::Vec;
    use config::ConfigRef;
    use reputation::ReputationRef;
    use lending_pool::LendingPoolRef;

    /// Enum for loan status
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq, Eq, Copy, Clone)]
    pub enum Status {
        Active,
        Fulfilled,
        Defaulted
    }

    /// Struct for Vouch Relationship
    #[ink::storage_item(packed)]
    #[derive(Debug, PartialEq)]
    pub struct VouchRelationship {
        loan_id: u64,          // Loan this vouch is for
        staked_stars: u32,
        staked_capital: Balance,
        created_at: Timestamp,
        status: Status
    }

    /// All information that is needed to store in the contract
    #[ink(storage)]
    pub struct Vouch {
        config: ConfigRef, // Contract address of Config
        reputation: ReputationRef, // Contract address of Reputation
        lending_pool: LendingPoolRef, // Contract address of LendingPool
        loan_manager: Lazy<Option<Address>>, // Contract address of LoanManager (authorized to resolve vouches)
        relationships: Mapping<(AccountId, AccountId), VouchRelationship>, // (voucher, borrower) -> relationship
        loan_vouchers: Mapping<u64, Vec<AccountId>>, // loan_id -> list of vouchers
        borrower_exposure: Mapping<AccountId, Balance>,
        borrower_vouchers: Mapping<AccountId, Vec<AccountId>>, // Kept for backward compatibility
    }

    /// Events for the vouch contract
    #[ink(event)]
    pub struct VouchCreated {
        voucher: AccountId,
        borrower: AccountId,
        stars: u32,
        capital: Balance,
    }

    #[ink(event)]
    pub struct VouchResolved {
        voucher: AccountId,
        borrower: AccountId,
        success: bool,
    }

    /// Error types for the contract
    #[derive(Debug, PartialEq, Eq)]
    #[ink::scale_derive(Encode, Decode, TypeInfo)]  
    pub enum Error {
        NotEnoughStars,
        NotEnoughCapital,
        UnableToVouch,
        ZeroAmount,
        ExposureCapExceeded,
        AlreadyResolved,
        RelationshipNotFound,
        Unauthorized,
    }


    impl Vouch {
        #[ink(constructor)]
        pub fn new(config_address: Address, reputation_address: Address, lending_pool_address: Address) -> Self {
            let config =
                ink::env::call::FromAddr::from_addr(config_address);
            let reputation =
                ink::env::call::FromAddr::from_addr(reputation_address);
            let lending_pool =
                ink::env::call::FromAddr::from_addr(lending_pool_address);
            Self {
                config,
                reputation,
                lending_pool,
                loan_manager: Lazy::default(),
                relationships: Mapping::default(),
                loan_vouchers: Mapping::default(),
                borrower_exposure: Mapping::default(),
                borrower_vouchers: Mapping::default()
            }
        }

        /// Set the loan manager address (can only be set once)
        /// This should be called after the LoanManager contract is deployed
        #[ink(message)]
        pub fn set_loan_manager(&mut self, loan_manager: Address) -> Result<(), Error> {
            // Check if loan manager is already set
            if self.loan_manager.get().is_some() {
                return Err(Error::Unauthorized);
            }
            self.loan_manager.set(&Some(loan_manager));
            Ok(())
        }

        /// Vouch for a specific loan (called by loan_manager after validation)
        /// Only callable by loan_manager
        #[ink(message)]
        pub fn vouch_for_loan(&mut self, loan_id: u64, borrower: AccountId, voucher: AccountId, stars: u32, capital_percent: u8) -> Result<(), Error> {
            // Verify caller is the authorized loan manager
            let caller = Self::env().caller();
            let loan_manager = self.loan_manager.get()
                .and_then(|opt| opt)
                .ok_or(Error::Unauthorized)?;
            if caller != loan_manager {
                return Err(Error::Unauthorized);
            }

            let voucher_stars = self.reputation.get_stars(voucher);

            // Check if voucher meets minimum stars requirement to vouch
            if !self.reputation.can_vouch(voucher) {
                return Err(Error::UnableToVouch);
            }
            if voucher_stars < stars {
                return Err(Error::NotEnoughStars);
            }

            let deposit = self.lending_pool.get_user_deposit(voucher);
            if deposit == 0 {
                return Err(Error::NotEnoughCapital);
            }

            // Calculate staked capital (percent of deposit)
            let staked_capital = (deposit * (capital_percent as Balance)) / 100;
            if staked_capital == 0 {
                return Err(Error::ZeroAmount);
            }

            // Check exposure cap BEFORE staking (to avoid staking if cap is exceeded)
            let exposure_cap = self.config.get_exposure_cap();
            let current_exposure = self.borrower_exposure.get(&borrower).unwrap_or(0);
            let total_liquidity = self.lending_pool.get_total_liquidity();
            let max_allowed = (total_liquidity as u128)
                .saturating_mul(exposure_cap as u128)
                .checked_div(1_000_000_000u128)
                .unwrap_or(0) as Balance;

            if max_allowed == 0 || current_exposure.saturating_add(staked_capital) > max_allowed {
                return Err(Error::ExposureCapExceeded);
            }

            // Stake stars in Reputation (after all validations pass)
            self.reputation.stake_stars(voucher, stars).map_err(|_| Error::UnableToVouch)?;

            // Store the relationship
            let key = (voucher, borrower);
            let relationship = VouchRelationship {
                loan_id,
                staked_stars: stars,
                staked_capital,
                created_at: self.env().block_timestamp(),
                status: Status::Active,
            };
            self.relationships.insert(&key, &relationship);

            // Track exposure per borrower
            self.borrower_exposure.insert(&borrower, &(current_exposure + staked_capital));

            // Track voucher in the loan's voucher list
            let mut loan_vouchers = self.loan_vouchers.get(&loan_id).unwrap_or_default();
            if !loan_vouchers.contains(&voucher) {
                loan_vouchers.push(voucher);
                self.loan_vouchers.insert(&loan_id, &loan_vouchers);
            }

            // Track voucher in the borrower's voucher list (for backward compatibility)
            let mut borrower_vouchers_list = self.borrower_vouchers.get(&borrower).unwrap_or_default();
            if !borrower_vouchers_list.contains(&voucher) {
                borrower_vouchers_list.push(voucher);
                self.borrower_vouchers.insert(&borrower, &borrower_vouchers_list);
            }

            // Emit event
            self.env().emit_event(VouchCreated {
                voucher,
                borrower,
                stars,
                capital: staked_capital,
            });

            Ok(())
        }

        /// Get count of active vouches for a specific loan
        #[ink(message)]
        pub fn get_vouches_for_loan(&self, loan_id: u64) -> u32 {
            let vouchers = self.loan_vouchers.get(&loan_id).unwrap_or_default();
            vouchers.len() as u32
        }

        /// Get all voucher addresses for a specific loan
        #[ink(message)]
        pub fn get_vouchers_for_loan(&self, loan_id: u64) -> Vec<AccountId> {
            self.loan_vouchers.get(&loan_id).unwrap_or_default()
        }

        /// Get count of active vouches for a borrower (backward compatibility)
        #[ink(message)]
        pub fn get_vouches_for(&self, borrower: AccountId) -> u32 {
            let vouchers = self.borrower_vouchers.get(&borrower).unwrap_or_default();
            let mut count: u32 = 0;
            for voucher in vouchers {
                if let Some(rel) = self.relationships.get(&(voucher, borrower)) {
                    if rel.status == Status::Active {
                        count += 1;
                    }
                }
            }
            count
        }

        /// Get all voucher addresses for a borrower (backward compatibility)
        #[ink(message)]
        pub fn get_all_vouchers(&self, borrower: AccountId) -> Vec<AccountId> {
            self.borrower_vouchers.get(&borrower).unwrap_or_default()
        }

        /// Resolve all vouch relationships for a loan upon loan completion
        /// Only callable by the authorized loan manager contract
        #[ink(message)]
        pub fn resolve_loan(&mut self, loan_id: u64, borrower: AccountId, success: bool) -> Result<(), Error> {
            // Verify caller is the authorized loan manager
            let caller = Self::env().caller();
            let loan_manager = self.loan_manager.get()
                .and_then(|opt| opt)
                .ok_or(Error::Unauthorized)?;
            if caller != loan_manager {
                return Err(Error::Unauthorized);
            }

            let vouchers = self.loan_vouchers.get(&loan_id).unwrap_or_default();

            for voucher in vouchers.iter() {
                let key = (*voucher, borrower);
                if let Some(mut relationship) = self.relationships.get(&key) {
                    // Only resolve vouches for this specific loan
                    if relationship.loan_id != loan_id || relationship.status != Status::Active {
                        continue;
                    }

                    // Update status
                    relationship.status = if success { Status::Fulfilled } else { Status::Defaulted };
                    self.relationships.insert(&key, &relationship);

                    // Unstake/slash stars via Reputation
                    let _ = self.reputation.unstake_stars(*voucher, relationship.staked_stars, borrower, success);

                    // If failure, slash capital via LendingPool
                    if !success {
                        let _ = self.lending_pool.slash_stake(*voucher, relationship.staked_capital);
                    }

                    self.env().emit_event(VouchResolved {
                        voucher: *voucher,
                        borrower,
                        success,
                    });
                }
            }

            // Clear voucher list for this loan
            self.loan_vouchers.remove(&loan_id);

            // Decrement borrower exposure by total staked capital for this loan
            let mut current_exposure = self.borrower_exposure.get(&borrower).unwrap_or(0);
            let mut total_staked_for_loan = 0u128;
            for voucher in vouchers.iter() {
                let key = (*voucher, borrower);
                if let Some(rel) = self.relationships.get(&key) {
                    if rel.loan_id == loan_id {
                        total_staked_for_loan += rel.staked_capital as u128;
                    }
                }
            }
            current_exposure = current_exposure.saturating_sub(total_staked_for_loan as Balance);
            if current_exposure == 0 {
                self.borrower_exposure.remove(&borrower);
            } else {
                self.borrower_exposure.insert(&borrower, &current_exposure);
            }

            Ok(())
        }

        /// Resolve all vouch relationships for a borrower (backward compatibility)
        /// Only callable by the authorized loan manager contract
        #[ink(message)]
        pub fn resolve_all(&mut self, borrower: AccountId, success: bool) -> Result<(), Error> {
            // Verify caller is the authorized loan manager
            let caller = Self::env().caller();
            let loan_manager = self.loan_manager.get()
                .and_then(|opt| opt)
                .ok_or(Error::Unauthorized)?;
            if caller != loan_manager {
                return Err(Error::Unauthorized);
            }

            let vouchers = self.borrower_vouchers.get(&borrower).unwrap_or_default();

            for voucher in vouchers.iter() {
                let key = (*voucher, borrower);
                if let Some(mut relationship) = self.relationships.get(&key) {
                    if relationship.status != Status::Active {
                        continue;
                    }

                    // Update status
                    relationship.status = if success { Status::Fulfilled } else { Status::Defaulted };
                    self.relationships.insert(&key, &relationship);

                    // Unstake/slash stars via Reputation
                    let _ = self.reputation.unstake_stars(*voucher, relationship.staked_stars, borrower, success);

                    // If failure, slash capital via LendingPool
                    if !success {
                        let _ = self.lending_pool.slash_stake(*voucher, relationship.staked_capital);
                    }

                    self.env().emit_event(VouchResolved {
                        voucher: *voucher,
                        borrower,
                        success,
                    });
                }
            }

            // Reset borrower exposure to 0
            self.borrower_exposure.remove(&borrower);

            // Clear voucher list for this borrower
            self.borrower_vouchers.remove(&borrower);

            Ok(())
        }
    }

}