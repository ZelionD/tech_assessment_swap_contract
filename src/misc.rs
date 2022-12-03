use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use primitive_types::U256;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum RunningState {
    Running,
    Paused,
}

pub(crate) trait Hash {
    fn hash(&self) -> Vec<u8>;
}

pub(crate) fn compute_tokens_ratio(
    token1_amount: u128,
    token2_amount: u128,
) -> Result<U256, &'static str> {
    U256::from(token1_amount)
        .checked_mul(U256::from(token2_amount))
        .ok_or("Computation overflow")
}
