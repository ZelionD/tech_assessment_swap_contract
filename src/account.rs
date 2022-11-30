use crate::misc::Hash;
use crate::storage::{MAX_ACCOUNT_ID_LENGTH, REGISTERED_MAX_ID_LENGTH_ACCOUNT_INITIAL_STORAGE};
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{env, AccountId, Balance, StorageUsage};

/// Current account data struct
///
/// Contains most recent storage data
#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct Account {
    /// Total account storage deposit amount in $NEAR
    pub(crate) storage_balance: u128,
    /// Total used by account storage amount in bytes
    pub(crate) storage_usage: StorageUsage,
}

/// Versioned account data struct
///
/// Could contain legacy account data structs,
/// which would be upgraded to current version upon next write access
#[derive(BorshDeserialize, BorshSerialize)]
pub(crate) enum VAccount {
    Current(Account),
}

impl Account {
    /// Creates new account data struct with optionally provided initial storage balance
    pub(crate) fn new(account_id: &AccountId, storage_balance: Option<Balance>) -> Self {
        Self {
            storage_balance: storage_balance.unwrap_or_default(),
            storage_usage: Self::initial_storage_usage(Some(account_id)),
        }
    }

    /// Returns storage usage by optionally provided `account_id`, otherwise use maximum account id length
    pub(crate) fn initial_storage_usage(account_id: Option<&AccountId>) -> u64 {
        // compute storage usage diff for specific `account id` length, which is used when saving account data in lookup map
        let storage_diff = account_id
            .map(|account_id| 2 * (MAX_ACCOUNT_ID_LENGTH - account_id.as_bytes().len() as u64))
            .unwrap_or_default();

        REGISTERED_MAX_ID_LENGTH_ACCOUNT_INITIAL_STORAGE - storage_diff
    }

    /// Returns required deposit by optionally provided `account_id`
    pub(crate) fn required_deposit(account_id: Option<&AccountId>) -> U128 {
        (Self::initial_storage_usage(account_id) as Balance * env::storage_byte_cost()).into()
    }

    /// Returns storage balance
    pub(crate) fn storage_balance(&self) -> StorageBalance {
        StorageBalance {
            total: self.storage_balance.into(),
            available: self
                .storage_balance
                .saturating_sub(self.storage_usage as Balance * env::storage_byte_cost())
                .into(),
        }
    }
}

impl From<Account> for VAccount {
    fn from(account: Account) -> Self {
        Self::Current(account)
    }
}

impl From<VAccount> for Account {
    fn from(v_acc: VAccount) -> Self {
        match v_acc {
            VAccount::Current(account) => account,
        }
    }
}

impl Hash for AccountId {
    fn hash(&self) -> Vec<u8> {
        env::sha256(self.as_bytes())
    }
}
