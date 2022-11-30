use crate::misc::RunningState;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, PanicOnDefault};
use near_sdk::{near_bindgen, AccountId};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// The contract's owner account id
    pub(crate) owner_id: AccountId,
    /// Contract's state, e.g. running, paused
    pub(crate) running_state: RunningState,
}

#[near_bindgen]
impl Contract {
    /// Initializes contract
    #[init]
    pub fn init() -> Self {
        let owner_id = env::predecessor_account_id();

        Self::new(owner_id.clone())
    }
}

impl Contract {
    /// Creates a contract and sets an owner as caller AccountId
    pub(crate) fn new(owner_id: AccountId) -> Self {
        Self {
            owner_id,
            running_state: RunningState::Running,
        }
    }

    /// Checks if the caller is an owner of the contract
    pub(crate) fn is_owner(&self) -> bool {
        env::predecessor_account_id() == self.owner_id
    }
}
