use crate::account::{Account, VAccount};
use crate::misc::RunningState;
use crate::storage::StorageKey;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// The contract's owner account id
    pub(crate) owner_id: AccountId,
    /// Contract's state, e.g. running, paused
    pub(crate) running_state: RunningState,
    /// User versioned accounts data keyed by AccountId
    pub(crate) accounts: LookupMap<AccountId, VAccount>,
}

#[near_bindgen]
impl Contract {
    /// Initializes contract
    #[init]
    pub fn init(owner_id: Option<AccountId>) -> Self {
        Self::new(owner_id.unwrap_or_else(|| env::predecessor_account_id()))
    }
}

impl Contract {
    /// Creates a contract and sets an owner
    pub(crate) fn new(owner_id: AccountId) -> Self {
        Self {
            owner_id,
            running_state: RunningState::Running,
            accounts: LookupMap::new(StorageKey::Accounts),
        }
    }

    /// Checks if contract is at running state
    pub(crate) fn assert_contract_running(&self) {
        require!(
            self.running_state == RunningState::Running,
            "Contract paused"
        );
    }

    /// Checks if the caller is an owner of the contract
    pub(crate) fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Not allowed"
        );
    }

    /// Returns account by provided `account_id`
    pub(crate) fn get_account(&self, account_id: &AccountId) -> Result<Account, &'static str> {
        self.accounts
            .get(account_id)
            .map(|v_acc| Account::from(v_acc))
            .ok_or("Account is not registered")
    }
}
