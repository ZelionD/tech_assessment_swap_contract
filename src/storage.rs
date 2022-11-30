use near_sdk::borsh::{self, BorshSerialize};
use near_sdk::{BorshStorageKey, StorageUsage};

pub(crate) const U128_STORAGE: StorageUsage = 16;
pub(crate) const U64_STORAGE: StorageUsage = 8;
pub(crate) const U8_STORAGE: StorageUsage = 1;

/// Max length of account id [64 bytes]
pub(crate) const MAX_ACCOUNT_ID_LENGTH: StorageUsage = 64;

// Serialized AccountId with maximum id length (len + id) [68 bytes]
pub(crate) const ACCOUNT_ID_STORAGE: StorageUsage =
    std::mem::size_of::<u32>() as u64 + MAX_ACCOUNT_ID_LENGTH;

/// Every storage entry additionally uses 40 bytes
/// - key len as u64,
/// - key ptr as u64,
/// - value len as u64,
/// - value ptr as u64,
/// - register as u64
pub(crate) const STORAGE_ENTRY: StorageUsage = 5 * U64_STORAGE;

/// enum::StorageKey size [1 byte]
const ENUM_STORAGE_KEY: StorageUsage = U8_STORAGE;

/// Current user account struct size
///
/// - storage_balance: [U128_STORAGE]
/// - storage_usage: [U64_STORAGE]
pub(crate) const ACCOUNT_STORAGE: StorageUsage = U128_STORAGE + U64_STORAGE;

/// Versioned user account size
///
/// - enum VAccount [U8_STORAGE]
/// - current account variant value [ACCOUNT_STORAGE]
pub(crate) const VACCOUNT_STORAGE: StorageUsage = U8_STORAGE + ACCOUNT_STORAGE;

/// Initial (minimum) storage in bytes used by registered user account with maximum id length (64 symbols)
///
/// - `Contract::accounts` (LookupMap<AccountId, VAccount>) entry [STORAGE_ENTRY] + [ENUM_STORAGE_KEY] + [ACCOUNT_ID_STORAGE] + [VACCOUNT_STORAGE]
pub(crate) const REGISTERED_MAX_ID_LENGTH_ACCOUNT_INITIAL_STORAGE: StorageUsage =
    STORAGE_ENTRY + ENUM_STORAGE_KEY + ACCOUNT_ID_STORAGE + VACCOUNT_STORAGE;

#[derive(BorshStorageKey, BorshSerialize)]
pub(crate) enum StorageKey {
    Accounts,
}
