#![cfg_attr(not(feature = "std"), no_std, no_main)]

/// This contract manages vouch relationships between users

#[ink::contract]
mod vouch {
    use ink::storage::Mapping;
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
        relationships: Mapping<(Address, Address), VouchRelationship>,
        borrower_exposure: Mapping<Address, Balance>,
        borrower_vouchers: Mapping<Address, Vec<Address>>,
    }

    /// Events for the vouch contract
    #[ink(event)]
    pub struct VouchCreated {
        voucher: Address,
        borrower: Address,
        stars: u32,
        capital: Balance,
    }

    #[ink(event)]
    pub struct VouchResolved {
        voucher: Address,
        borrower: Address,
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
                relationships: Mapping::default(),
                borrower_exposure: Mapping::default(),
                borrower_vouchers: Mapping::default(),
            }
        }

        /// Vouch for a borrower by staking stars and capital
        #[ink(message)]
        pub fn vouch_for(&mut self, borrower: Address, stars: u32, capital_percent: u8) -> Result<(), Error> {
            let caller = self.env().caller();
            let caller_stars = self.reputation.get_stars(caller);

            if self.reputation.can_vouch(borrower) == false {
                return Err(Error::UnableToVouch);
            }
            if caller_stars < stars {
                return Err(Error::NotEnoughStars);
            }

            let deposit = self.lending_pool.get_user_deposit(caller);

            // Calculate staked capital (percent of deposit)
            let staked_capital = (deposit * (capital_percent as Balance)) / 100;
            if staked_capital == 0 {
                return Err(Error::ZeroAmount);
            }

            // Stake stars in Reputation
            self.reputation.stake_stars(caller, stars).map_err(|_| Error::UnableToVouch)?;

            // Check exposure cap
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

            // Store the relationship
            let key = (caller, borrower);
            let relationship = VouchRelationship {
                staked_stars: stars,
                staked_capital,
                created_at: self.env().block_timestamp(),
                status: Status::Active,
            };
            self.relationships.insert(&key, &relationship);

            // Track exposure per borrower
            self.borrower_exposure.insert(&borrower, &(current_exposure + staked_capital));

            // Track voucher in the borrower's voucher list
            let mut vouchers = self.borrower_vouchers.get(&borrower).unwrap_or_default();
            if !vouchers.contains(&caller) {
                vouchers.push(caller);
                self.borrower_vouchers.insert(&borrower, &vouchers);
            }

            // Emit event
            self.env().emit_event(VouchCreated {
                voucher: caller,
                borrower,
                stars,
                capital: staked_capital,
            });

            Ok(())
        }

        /// Get count of active vouches for a borrower
        #[ink(message)]
        pub fn get_vouches_for(&self, borrower: Address) -> u32 {
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

        /// Resolve all vouch relationships for a borrower upon loan completion
        #[ink(message)]
        pub fn resolve_all(&mut self, borrower: Address, success: bool) -> Result<(), Error> {
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
                    let _ = self.reputation.unstake_stars(*voucher, relationship.staked_stars, success);

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