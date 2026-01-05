#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod test_to_account_id {
    //pub type AccountId = <ink::env::DefaultEnvironment as Environment>::AccountId;

    #[ink(storage)]
    pub struct TestContract {}

    impl TestContract {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {}
        }

        #[ink(message)]
        pub fn get_converted_caller(&self) -> AccountId {
            let caller: Address = self.env().caller();
            self.env().to_account_id(caller)
        }
    }
}