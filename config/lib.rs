#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod config {
    // Hardcoded constants
    const OPTIMAL_UTILIZATION: u64 = 80_000_000_000; // 80% scaled by 1e9
    const SLOPE1: u64 = 4_000_000_000; // +4% pre-optimal
    const SLOPE2: u64 = 75_000_000_000; // +75% post-optimal
    const EXPOSURE_CAP: u64 = 50_000_000; // 5% scaled by 1e9
    const RESERVE_FACTOR: u8 = 20; // 20%
    const MAX_RATE: u64 = 100_000_000_000; // Cap at 100%
    
    /// All information stored for the configurable parameters of the protocol
    #[ink(storage)]
    pub struct Config {
        admin: Option<AccountId>,
        base_interest_rate: u64,
        boost: u64,
        min_stars_to_vouch: u32,
        cooldown_period: Timestamp,
        loan_term: Timestamp, // Default loan term (separate from cooldown)
        //Loan tier configuration
        loan_tier_scaling_factor: Balance,
        loan_tier1_max_scaled_amount: Balance,
        loan_tier2_max_scaled_amount: Balance,
        loan_tier1_min_stars: u32,
        loan_tier1_min_vouches: u32,
        loan_tier2_min_stars: u32,
        loan_tier2_min_vouches: u32,
        loan_tier3_min_stars: u32,
        loan_tier3_min_vouches: u32,
        // Default grace period - time after due date before loan can be marked as defaulted
        default_grace_period: Timestamp,
        // Star-based discount configuration
        star_discount_percent_per_star: u64, // Discount percentage per star (e.g., 1 = 1% per star)
        max_star_discount_percent: u64, // Maximum discount cap (e.g., 50 = 50% max discount)
    }

    // Custom error types for the contract
    #[derive(Debug, PartialEq, Eq)]
    #[ink::scale_derive(Encode, Decode, TypeInfo)]
    pub enum Error {
        NotAdmin,
         InvalidValue,
        AlreadyAdmin
     }

    // Custom result type for the contract
    pub type ConfigResult<T> = core::result::Result<T, Error>;

    impl Config {
        /// Constructor that initializes configuration with defaults and admin
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                admin: None,
                base_interest_rate: 10_000_000_000, // 10% scaled by 1e9
                boost: 2_000_000_000, // +2 boost
                min_stars_to_vouch: 50,
                cooldown_period: 60_000, // 1 minute in ms (for demo - can be changed later)
                loan_term: 2_592_000_000, // 30 days in ms
                loan_tier_scaling_factor: 1_000_000_000, // 1e9 (TOKEN_DECIMALS)
                loan_tier1_max_scaled_amount: 1000,
                loan_tier2_max_scaled_amount: 10000,
                loan_tier1_min_stars: 5,
                loan_tier1_min_vouches: 1,
                loan_tier2_min_stars: 20,
                loan_tier2_min_vouches: 2,
                loan_tier3_min_stars: 50,
                loan_tier3_min_vouches: 3,
                default_grace_period: 604_800_000, // 7 days in ms
                star_discount_percent_per_star: 1, // 1% discount per star
                max_star_discount_percent: 50, // 50% maximum discount cap
            }
        }

        #[ink(message)]
        pub fn set_admin(&mut self, admin: AccountId) -> ConfigResult<()> {
            if self.admin.is_some() {
                return Err(Error::AlreadyAdmin);
            }
            self.admin = Some(admin);
            Ok(())
        }

        #[ink(message)]
        pub fn get_converted_caller(&self) -> AccountId {
            let caller = self.env().caller();
            self.env().to_account_id(caller)
        }


        /// Ensure that the caller of other functions is the admin
        fn ensure_admin(&mut self) -> ConfigResult<()> {
            let caller = self.get_converted_caller();
            match self.admin {
                Some(admin) if admin == caller => Ok(()),
                Some(_) => Err(Error::NotAdmin),
                None => Err(Error::NotAdmin),
            }
        }

        /// Setter functions for configuration parameters

        #[ink(message)]
        pub fn update_base_interest_rate(&mut self, new_rate: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.base_interest_rate = new_rate;
            Ok(())
        }

        // Getter functions for hardcoded constants
        
        #[ink(message)]
        pub fn get_optimal_utilization(&self) -> u64 {
            OPTIMAL_UTILIZATION
        }
        
        #[ink(message)]
        pub fn get_slope1(&self) -> u64 {
            SLOPE1
        }
        
        #[ink(message)]
        pub fn get_slope2(&self) -> u64 {
            SLOPE2
        }
        
        #[ink(message)]
        pub fn get_exposure_cap(&self) -> u64 {
            EXPOSURE_CAP
        }
        
        #[ink(message)]
        pub fn get_reserve_factor(&self) -> u8 {
            RESERVE_FACTOR
        }
        
        #[ink(message)]
        pub fn get_max_rate(&self) -> u64 {
            MAX_RATE
        }


        #[ink(message)]
        pub fn update_boost(&mut self, new_boost: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.boost = new_boost;
            Ok(())
        }

        #[ink(message)]
        pub fn update_min_stars_to_vouch(&mut self, new_min: u32) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.min_stars_to_vouch = new_min;
            Ok(())
        }

        #[ink(message)]
        pub fn update_cooldown_period(&mut self, new_period: Timestamp) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.cooldown_period = new_period;
            Ok(())
        }

        #[ink(message)]
        pub fn update_loan_term(&mut self, new_term: Timestamp) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.loan_term = new_term;
            Ok(())
        }


        #[ink(message)]
        pub fn update_loan_tier_scaling_factor(&mut self, new_factor: Balance) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.loan_tier_scaling_factor = new_factor;
            Ok(())
        }

        #[ink(message)]
        pub fn update_loan_tier1_max_scaled_amount(&mut self, new_max: Balance) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.loan_tier1_max_scaled_amount = new_max;
            Ok(())
        }

        #[ink(message)]
        pub fn update_loan_tier2_max_scaled_amount(&mut self, new_max: Balance) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.loan_tier2_max_scaled_amount = new_max;
            Ok(())
        }

        #[ink(message)]
        pub fn update_loan_tier1_requirements(&mut self, min_stars: u32, min_vouches: u32) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.loan_tier1_min_stars = min_stars;
            self.loan_tier1_min_vouches = min_vouches;
            Ok(())
        }

        #[ink(message)]
        pub fn update_loan_tier2_requirements(&mut self, min_stars: u32, min_vouches: u32) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.loan_tier2_min_stars = min_stars;
            self.loan_tier2_min_vouches = min_vouches;
            Ok(())
        }

        #[ink(message)]
        pub fn update_loan_tier3_requirements(&mut self, min_stars: u32, min_vouches: u32) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.loan_tier3_min_stars = min_stars;
            self.loan_tier3_min_vouches = min_vouches;
            Ok(())
        }

        #[ink(message)]
        pub fn update_default_grace_period(&mut self, new_period: Timestamp) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.default_grace_period = new_period;
            Ok(())
        }

        #[ink(message)]
        pub fn update_star_discount_percent_per_star(&mut self, new_discount: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.star_discount_percent_per_star = new_discount;
            Ok(())
        }

        #[ink(message)]
        pub fn update_max_star_discount_percent(&mut self, new_max: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            // Validate: max discount should be between 0 and 100 (percentage)
            if new_max > 100 {
                return Err(Error::InvalidValue);
            }
            self.max_star_discount_percent = new_max;
            Ok(())
        }

        /// Getter functions for configuration parameters

        #[ink(message)]
        pub fn get_base_interest_rate(&self) -> u64 {
            self.base_interest_rate
        }


        #[ink(message)]
        pub fn get_boost(&self) -> u64 {
            self.boost
        }

        #[ink(message)]
        pub fn get_min_stars_to_vouch(&self) -> u32 {
            self.min_stars_to_vouch
        }

        #[ink(message)]
        pub fn get_cooldown_period(&self) -> Timestamp {
            self.cooldown_period
        }

        #[ink(message)]
        pub fn get_loan_term(&self) -> Timestamp {
            self.loan_term
        }


        /// Getter for loan tier scaling factor
        #[ink(message)]
        pub fn loan_tier_scaling_factor(&self) -> Balance {
            self.loan_tier_scaling_factor
        }

        /// Getter for tier 1 maximum scaled amount
        #[ink(message)]
        pub fn loan_tier1_max_scaled_amount(&self) -> Balance {
            self.loan_tier1_max_scaled_amount
        }

        /// Getter for tier 2 maximum scaled amount
        #[ink(message)]
        pub fn loan_tier2_max_scaled_amount(&self) -> Balance {
            self.loan_tier2_max_scaled_amount
        }

        /// Getter for tier 1 requirements (min_stars, min_vouches)
        #[ink(message)]
        pub fn loan_tier1_requirements(&self) -> (u32, u32) {
            (self.loan_tier1_min_stars, self.loan_tier1_min_vouches)
        }

        /// Getter for tier 2 requirements (min_stars, min_vouches)
        #[ink(message)]
        pub fn loan_tier2_requirements(&self) -> (u32, u32) {
            (self.loan_tier2_min_stars, self.loan_tier2_min_vouches)
        }

        /// Getter for tier 3 requirements (min_stars, min_vouches)
        #[ink(message)]
        pub fn loan_tier3_requirements(&self) -> (u32, u32) {
            (self.loan_tier3_min_stars, self.loan_tier3_min_vouches)
        }

        /// Getter for default grace period
        #[ink(message)]
        pub fn get_default_grace_period(&self) -> Timestamp {
            self.default_grace_period
        }

        /// Getter for star discount percent per star
        #[ink(message)]
        pub fn get_star_discount_percent_per_star(&self) -> u64 {
            self.star_discount_percent_per_star
        }

        /// Getter for maximum star discount percent
        #[ink(message)]
        pub fn get_max_star_discount_percent(&self) -> u64 {
            self.max_star_discount_percent
        }
    }

}