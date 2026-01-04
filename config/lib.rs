#![cfg_attr(not(feature = "std"), no_std, no_main)]

use ink::env::{DefaultEnvironment, Environment};

pub type AccountId = <DefaultEnvironment as Environment>::AccountId;

#[ink::contract]
mod config {
    /// All information stored for the configurable parameters of the protocol
    #[ink(storage)]
    pub struct Config {
        admin: AccountId,
        base_interest_rate: u64,
        optimal_utilization: u64,
        slope1: u64,
        slope2: u64,
        boost: u64,
        min_stars_to_vouch: u32,
        cooldown_period: Timestamp,
        loan_term: Timestamp, // Default loan term (separate from cooldown)
        exposure_cap: u64,
        reserve_factor: u8,
        max_rate: u64,
        // Loan tier configuration
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
    }

    // Custom result type for the contract
    pub type ConfigResult<T> = core::result::Result<T, Error>;

    impl Config {
        pub const DEFAULT_BASE_INTEREST_RATE: u64 = 10_000_000_000; // 10% scaled by 1e9
        pub const DEFAULT_OPTIMAL_UTILIZATION: u64 = 80_000_000_000; // 80% scaled by 1e9
        pub const DEFAULT_SLOPE1: u64 = 4_000_000_000; // +4% pre-optimal
        pub const DEFAULT_SLOPE2: u64 = 75_000_000_000; // +75% post-optimal
        pub const DEFAULT_BOOST: u64 = 2_000_000_000; // +2 boost
        pub const DEFAULT_MIN_STARS_TO_VOUCH: u32 = 50;
        pub const DEFAULT_COOLDOWN_PERIOD: Timestamp = 60_000; // 1 minute in ms (for demo - can be changed later)
        pub const DEFAULT_LOAN_TERM: Timestamp = 2_592_000_000; // 30 days in ms
        pub const DEFAULT_EXPOSURE_CAP: u64 = 50_000_000; // 5% scaled by 1e9
        pub const DEFAULT_RESERVE_FACTOR: u8 = 20; // 20%
        pub const DEFAULT_MAX_RATE: u64 = 100_000_000_000; // Cap at 100%
        // Loan tier defaults (matching previous hardcoded values)
        pub const DEFAULT_LOAN_TIER_SCALING_FACTOR: Balance = 1_000_000_000; // 1e9 (TOKEN_DECIMALS)
        pub const DEFAULT_LOAN_TIER1_MAX_SCALED_AMOUNT: Balance = 1000;
        pub const DEFAULT_LOAN_TIER2_MAX_SCALED_AMOUNT: Balance = 10000;
        pub const DEFAULT_LOAN_TIER1_MIN_STARS: u32 = 5;
        pub const DEFAULT_LOAN_TIER1_MIN_VOUCHES: u32 = 1;
        pub const DEFAULT_LOAN_TIER2_MIN_STARS: u32 = 20;
        pub const DEFAULT_LOAN_TIER2_MIN_VOUCHES: u32 = 2;
        pub const DEFAULT_LOAN_TIER3_MIN_STARS: u32 = 50;
        pub const DEFAULT_LOAN_TIER3_MIN_VOUCHES: u32 = 3;
        // Default grace period: 7 days (allows time for repayment after due date)
        pub const DEFAULT_GRACE_PERIOD: Timestamp = 604_800_000; // 7 days in ms
        // Star discount defaults
        pub const DEFAULT_STAR_DISCOUNT_PERCENT_PER_STAR: u64 = 1; // 1% discount per star
        pub const DEFAULT_MAX_STAR_DISCOUNT_PERCENT: u64 = 50; // 50% maximum discount cap

        /// Constructor that initializes configuration with defaults and admin
        #[ink(constructor)]
        pub fn new() -> Self {
            let caller= Self::env().caller();
            let caller_acc = Self::env().to_account_id(caller);
            Self { 
                admin: caller_acc,
                base_interest_rate: Self::DEFAULT_BASE_INTEREST_RATE,
                optimal_utilization: Self::DEFAULT_OPTIMAL_UTILIZATION,
                slope1: Self::DEFAULT_SLOPE1,
                slope2: Self::DEFAULT_SLOPE2,
                boost: Self::DEFAULT_BOOST,
                min_stars_to_vouch: Self::DEFAULT_MIN_STARS_TO_VOUCH,
                cooldown_period: Self::DEFAULT_COOLDOWN_PERIOD,
                loan_term: Self::DEFAULT_LOAN_TERM,
                exposure_cap: Self::DEFAULT_EXPOSURE_CAP,
                reserve_factor: Self::DEFAULT_RESERVE_FACTOR,
                max_rate: Self::DEFAULT_MAX_RATE,
                loan_tier_scaling_factor: Self::DEFAULT_LOAN_TIER_SCALING_FACTOR,
                loan_tier1_max_scaled_amount: Self::DEFAULT_LOAN_TIER1_MAX_SCALED_AMOUNT,
                loan_tier2_max_scaled_amount: Self::DEFAULT_LOAN_TIER2_MAX_SCALED_AMOUNT,
                loan_tier1_min_stars: Self::DEFAULT_LOAN_TIER1_MIN_STARS,
                loan_tier1_min_vouches: Self::DEFAULT_LOAN_TIER1_MIN_VOUCHES,
                loan_tier2_min_stars: Self::DEFAULT_LOAN_TIER2_MIN_STARS,
                loan_tier2_min_vouches: Self::DEFAULT_LOAN_TIER2_MIN_VOUCHES,
                loan_tier3_min_stars: Self::DEFAULT_LOAN_TIER3_MIN_STARS,
                loan_tier3_min_vouches: Self::DEFAULT_LOAN_TIER3_MIN_VOUCHES,
                default_grace_period: Self::DEFAULT_GRACE_PERIOD,
                star_discount_percent_per_star: Self::DEFAULT_STAR_DISCOUNT_PERCENT_PER_STAR,
                max_star_discount_percent: Self::DEFAULT_MAX_STAR_DISCOUNT_PERCENT,
            }
        }

        /// Ensure that the caller of other functions is the admin
        fn ensure_admin(&self) -> ConfigResult<()> {
            // #region agent log
            let log_data = format!(r#"{{"sessionId":"debug-session","runId":"run1","hypothesisId":"A","location":"config/lib.rs:{}","message":"ensure_admin entry","data":{{"admin":"{:?}"}},"timestamp":{}}}"#, line!(), self.admin, 0u64);
            let _ = std::fs::OpenOptions::new().create(true).append(true).open("/Users/fabiansanchezd/Documents/kleo-contracts/.cursor/debug.log").and_then(|mut f| std::io::Write::write_all(&mut f, format!("{}\n", log_data).as_bytes()));
            // #endregion
            let caller = self.env().caller();
            let caller_acc = self.env().to_account_id(caller);
            // #region agent log
            let log_data2 = format!(r#"{{"sessionId":"debug-session","runId":"run1","hypothesisId":"A","location":"config/lib.rs:{}","message":"ensure_admin caller check","data":{{"caller":"{:?}","admin":"{:?}","match":{}}},"timestamp":{}}}"#, line!(), caller_acc, self.admin, caller_acc == self.admin, 0u64);
            let _ = std::fs::OpenOptions::new().create(true).append(true).open("/Users/fabiansanchezd/Documents/kleo-contracts/.cursor/debug.log").and_then(|mut f| std::io::Write::write_all(&mut f, format!("{}\n", log_data2).as_bytes()));
            // #endregion
            if caller_acc != self.admin {
                return Err(Error::NotAdmin);
            }
            Ok(())
        }

        /// Setter functions for configuration parameters

        #[ink(message)]
        pub fn update_base_interest_rate(&mut self, new_rate: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.base_interest_rate = new_rate;
            Ok(())
        }

        #[ink(message)]
        pub fn update_optimal_utilization(&mut self, new_optimal: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.optimal_utilization = new_optimal;
            Ok(())
        }

        #[ink(message)]
        pub fn update_slope1(&mut self, new_slope: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.slope1 = new_slope;
            Ok(())
        }

        #[ink(message)]
        pub fn update_slope2(&mut self, new_slope: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.slope2 = new_slope;
            Ok(())
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
        pub fn update_exposure_cap(&mut self, new_cap: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.exposure_cap = new_cap;
            Ok(())
        }

        #[ink(message)]
        pub fn update_reserve_factor(&mut self, new_factor: u8) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.reserve_factor = new_factor;
            Ok(())
        }

        #[ink(message)]
        pub fn update_max_rate(&mut self, new_max: u64) -> ConfigResult<()> {
            self.ensure_admin()?;
            self.max_rate = new_max;
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
            self.max_star_discount_percent = new_max;
            Ok(())
        }

        /// Getter functions for configuration parameters

        #[ink(message)]
        pub fn get_base_interest_rate(&self) -> u64 {
            self.base_interest_rate
        }

        #[ink(message)]
        pub fn get_optimal_utilization(&self) -> u64 {
            self.optimal_utilization
        }

        #[ink(message)]
        pub fn get_slope1(&self) -> u64 {
            self.slope1
        }

        #[ink(message)]
        pub fn get_slope2(&self) -> u64 {
            self.slope2
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

        #[ink(message)]
        pub fn get_exposure_cap(&self) -> u64 {
            self.exposure_cap
        }

        #[ink(message)]
        pub fn get_reserve_factor(&self) -> u8 {
            self.reserve_factor
        }

        #[ink(message)]
        pub fn get_max_rate(&self) -> u64 {
            self.max_rate
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