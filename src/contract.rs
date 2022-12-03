use crate::account::{Account, VAccount};
use crate::misc::RunningState;
use crate::storage::StorageKey;
use crate::tokens::*;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::{env, near_bindgen, require, AccountId, PanicOnDefault, Promise, ONE_NEAR};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// The contract's owner account id
    pub(crate) owner_id: AccountId,
    /// Contract's state, e.g. running, paused
    pub(crate) running_state: RunningState,
    /// User versioned accounts data keyed by AccountId
    pub(crate) accounts: LookupMap<AccountId, VAccount>,
    /// Token1 wallet entry containing information about deposit & liquidity in the pool
    pub(crate) token1_wallet: Option<TokenWallet>,
    /// Token2 wallet entry containing information about deposit & liquidity in the pool
    pub(crate) token2_wallet: Option<TokenWallet>,
}

#[near_bindgen]
impl Contract {
    /// Initializes contract
    #[init]
    pub fn init(owner_id: Option<AccountId>, tokens: Option<(AccountId, AccountId)>) -> Self {
        let mut contract = Self {
            owner_id: owner_id.unwrap_or_else(|| env::predecessor_account_id()),
            running_state: RunningState::Running,
            accounts: LookupMap::new(StorageKey::Accounts),
            token1_wallet: None,
            token2_wallet: None,
        };

        if let Some((token1, token2)) = tokens {
            contract.create_wallets(token1, token2);
        }

        contract
    }

    /// Requests a wallet registration in fungible token contract
    pub(crate) fn create_wallets(&mut self, token1: AccountId, token2: AccountId) -> Promise {
        self.create_token_wallet(token1.clone())
            .and(self.create_token_wallet(token2.clone()))
            .then(Self::ext(env::current_account_id()).on_created_tokens_wallets(token1, token2))
    }

    /// Owner's function to register wallets
    #[payable]
    pub fn owner_create_wallets(&mut self, token1: AccountId, token2: AccountId) -> Promise {
        // One wallet registration requires exactly 1 NEAR, to call storage_deposit
        require!(
            env::attached_deposit() == 2 * ONE_NEAR,
            format!("Requires exactly 2 NEAR to create wallets",)
        );

        self.assert_owner();

        self.create_wallets(token1, token2)
    }
}

impl Contract {
    /// Checks if contract is at running state
    pub(crate) fn assert_contract_running(&self) {
        require!(
            self.running_state == RunningState::Running,
            "Contract paused"
        );
    }

    /// Asserts if the caller is not an owner of the contract
    pub(crate) fn assert_owner(&self) {
        require!(self.is_owner(&env::predecessor_account_id()), "Not allowed");
    }

    /// Checks ifn the caller is an owner of the contract
    pub(crate) fn is_owner(&self, account_id: &AccountId) -> bool {
        account_id == &self.owner_id
    }

    /// Returns account by provided `account_id`
    pub(crate) fn get_account(&self, account_id: &AccountId) -> Result<Account, &'static str> {
        self.accounts
            .get(account_id)
            .map(|v_acc| Account::from(v_acc))
            .ok_or("Account is not registered")
    }
}
