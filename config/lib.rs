#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod config {
    /// All information stored for the configurable parameters of the protocol
    #[ink(storage)]
    pub struct Config {
        admin: Address,
        base_interest_rate: u64,
        optimal_utilization: u64,
        slope1: u64,
        slope2: u64,
        boost: u64,
        min_stars_to_vouch: u32,
        cooldown_period: Timestamp,
        exposure_cap: u64,
        reserve_factor: u8,
        max_rate: u64,
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
        pub const DEFAULT_BASE_INTEREST_RATE: u64 = 10_000_000; // 10%
        pub const DEFAULT_OPTIMAL_UTILIZATION: u64 = 80_000_000_000; // 80% scaled by 1e9
        pub const DEFAULT_SLOPE1: u64 = 4_000_000_000; // +4% pre-optimal
        pub const DEFAULT_SLOPE2: u64 = 75_000_000_000; // +75% post-optimal
        pub const DEFAULT_BOOST: u64 = 2_000_000_000; // +2 boost
        pub const DEFAULT_MIN_STARS_TO_VOUCH: u32 = 50;
        pub const DEFAULT_COOLDOWN_PERIOD: Timestamp = 2_592_000_000; // 30 days in ms
        pub const DEFAULT_EXPOSURE_CAP: u64 = 5_000_000_000; // 5% scaled by 1e9
        pub const DEFAULT_RESERVE_FACTOR: u8 = 20; // 20%
        pub const DEFAULT_MAX_RATE: u64 = 100_000_000_000; // Cap at 100%

        /// Constructor that initializes configuration with defaults and admin
        #[ink(constructor)]
        pub fn new() -> Self {
            let caller = Self::env().caller();
            Self {
                admin: caller,
                base_interest_rate: Self::DEFAULT_BASE_INTEREST_RATE,
                optimal_utilization: Self::DEFAULT_OPTIMAL_UTILIZATION,
                slope1: Self::DEFAULT_SLOPE1,
                slope2: Self::DEFAULT_SLOPE2,
                boost: Self::DEFAULT_BOOST,
                min_stars_to_vouch: Self::DEFAULT_MIN_STARS_TO_VOUCH,
                cooldown_period: Self::DEFAULT_COOLDOWN_PERIOD,
                exposure_cap: Self::DEFAULT_EXPOSURE_CAP,
                reserve_factor: Self::DEFAULT_RESERVE_FACTOR,
                max_rate: Self::DEFAULT_MAX_RATE,
            }
        }

        /// Ensure that the caller of other functions is the admin
        fn ensure_admin(&self) -> ConfigResult<()> {
            let caller = self.env().caller();
            if caller != self.admin {
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
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ink::env::{test};

        /// Helper function to set the caller in tests
        pub fn set_caller(caller: Address) {
            test::set_caller(caller);
        }

        /// This will be the default admin address for tests
        fn default_admin() -> Address {
            // Example test address (H160)
            "d43593c715fdd31c61141abd04a99fd6822c8558"
                .parse()
                .expect("valid H160")
        }

        #[ink::test]
        fn initializes_with_defaults() {
            let admin = default_admin();
            let config = Config::new(admin);

            assert_eq!(config.admin, admin);
            assert_eq!(config.get_base_interest_rate(), Config::DEFAULT_BASE_INTEREST_RATE);
            assert_eq!(config.get_optimal_utilization(), Config::DEFAULT_OPTIMAL_UTILIZATION);
            assert_eq!(config.get_slope1(), Config::DEFAULT_SLOPE1);
            assert_eq!(config.get_slope2(), Config::DEFAULT_SLOPE2);
            assert_eq!(config.get_boost(), Config::DEFAULT_BOOST);
            assert_eq!(config.get_min_stars_to_vouch(), Config::DEFAULT_MIN_STARS_TO_VOUCH);
            assert_eq!(config.get_cooldown_period(), Config::DEFAULT_COOLDOWN_PERIOD);
            assert_eq!(config.get_exposure_cap(), Config::DEFAULT_EXPOSURE_CAP);
            assert_eq!(config.get_reserve_factor(), Config::DEFAULT_RESERVE_FACTOR);
            assert_eq!(config.get_max_rate(), Config::DEFAULT_MAX_RATE);
        }

        #[ink::test]
        fn admin_can_update_all_params() {
            let admin = default_admin();
            set_caller(admin);

            let mut config = Config::new(admin);

            config.update_base_interest_rate(10).unwrap();
            config.update_optimal_utilization(90_000_000_000).unwrap();
            config.update_slope1(5).unwrap();
            config.update_slope2(80).unwrap();
            config.update_boost(3).unwrap();
            config.update_min_stars_to_vouch(75).unwrap();
            config.update_cooldown_period(123_456).unwrap();
            config.update_exposure_cap(6_000_000_000).unwrap();
            config.update_reserve_factor(12).unwrap();
            config.update_max_rate(110_000_000_000).unwrap();

            assert_eq!(config.get_base_interest_rate(), 10);
            assert_eq!(config.get_optimal_utilization(), 90_000_000_000);
            assert_eq!(config.get_slope1(), 5);
            assert_eq!(config.get_slope2(), 80);
            assert_eq!(config.get_boost(), 3);
            assert_eq!(config.get_min_stars_to_vouch(), 75);
            assert_eq!(config.get_cooldown_period(), 123_456);
            assert_eq!(config.get_exposure_cap(), 6_000_000_000);
            assert_eq!(config.get_reserve_factor(), 12);
            assert_eq!(config.get_max_rate(), 110_000_000_000);
        }

        #[ink::test]
        fn non_admin_cannot_update() {
            let admin = default_admin();
            let non_admin: Address = "1111111111111111111111111111111111111111"
                .parse()
                .expect("valid H160");

            set_caller(admin);
            let mut config = Config::new(admin);

            set_caller(non_admin);
            let result = config.update_base_interest_rate(15);
            assert_eq!(result, Err(Error::NotAdmin));

            // Ensure values did not change
            assert_eq!(config.get_base_interest_rate(), Config::DEFAULT_BASE_INTEREST_RATE);
        }
    }
}