use crate::misc::compute_tokens_ratio;
use crate::{Contract, ContractExt};
use near_contract_standards::fungible_token::core::ext_ft_core;
pub(crate) use near_contract_standards::fungible_token::metadata::{
    ext_ft_metadata, FungibleTokenMetadata,
};
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json;
use near_sdk::{
    env, ext_contract, near_bindgen, AccountId, Promise, PromiseError, PromiseOrValue, ONE_NEAR,
};
use std::cmp::Ordering;

#[derive(BorshDeserialize, BorshSerialize)]
pub(crate) struct TokenWallet {
    token_id: AccountId,
    metadata: FungibleTokenMetadata,
    deposit: u128,
    liquidity: u128,
}

pub(crate) trait TokenWalletProvider {
    fn create_token_wallet(&mut self, token: AccountId) -> Promise;

    fn on_created_tokens_wallets(
        &mut self,
        token1_id: AccountId,
        token2_id: AccountId,
        token1_metadata: Result<FungibleTokenMetadata, PromiseError>,
        token1_wallet_storage_balance: Result<StorageBalance, PromiseError>,
        token2_metadata: Result<FungibleTokenMetadata, PromiseError>,
        token2_wallet_storage_balance: Result<StorageBalance, PromiseError>,
    );
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct PoolView {
    pub token_ids: [AccountId; 2],
    pub decimals: [u8; 2],
    pub amounts: [U128; 2],
    pub ratio: String,
}

#[near_bindgen]
impl TokenWalletProvider for Contract {
    fn create_token_wallet(&mut self, token: AccountId) -> Promise {
        // first fetch token metadata and then creates a wallet for it
        ext_ft_metadata::ext(token.clone()).ft_metadata().and(
            ext_storage_management::ext(token)
                .with_attached_deposit(ONE_NEAR)
                .storage_deposit(Some(env::current_account_id()), Some(true)),
        )
    }

    #[private]
    fn on_created_tokens_wallets(
        &mut self,
        token1_id: AccountId,
        token2_id: AccountId,
        #[callback_result] token1_metadata: Result<FungibleTokenMetadata, PromiseError>,
        #[callback_result] token1_wallet_storage_balance: Result<StorageBalance, PromiseError>,
        #[callback_result] token2_metadata: Result<FungibleTokenMetadata, PromiseError>,
        #[callback_result] token2_wallet_storage_balance: Result<StorageBalance, PromiseError>,
    ) {
        let _ = token1_wallet_storage_balance
            .unwrap_or_else(|_| env::panic_str("Token1 wallet failed to register"));

        self.token1_wallet = Some(TokenWallet::new(
            token1_id,
            token1_metadata.unwrap_or_else(|_| env::panic_str("Failed to fetch Token1 metadata")),
        ));

        let _ = token2_wallet_storage_balance
            .unwrap_or_else(|_| env::panic_str("Token2 wallet failed to register"));

        self.token2_wallet = Some(TokenWallet::new(
            token2_id,
            token2_metadata.unwrap_or_else(|_| env::panic_str("Failed to fetch Token2 metadata")),
        ));
    }
}

#[ext_contract(ext_storage_management)]
pub trait StorageManagementProvider {
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance;
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    #[payable]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        env::log_str(&*format!(
            "SenderId: {:?} Amount: {:?} Msg: {:?}",
            sender_id, amount, msg
        ));

        let token_id = env::predecessor_account_id();

        let result = match serde_json::from_str::<TransferCommand>(&msg) {
            Ok(TransferCommand {
                r#type: TransferType::Swap,
            }) => self.on_transfer_swap(sender_id, token_id, amount),
            _ => self.on_transfer_deposit(sender_id, token_id, amount),
        };

        if let Err(e) = result {
            env::log_str(&*format!("Transfer failed. Error: {}", e));

            // In case of any error refund full transferred amount
            PromiseOrValue::Value(amount)
        } else {
            PromiseOrValue::Value(0.into())
        }
    }
}

#[near_bindgen]
impl Contract {
    fn on_transfer_deposit(
        &mut self,
        sender_id: AccountId,
        token_id: AccountId,
        amount: U128,
    ) -> Result<(), &'static str> {
        if !self.is_owner(&sender_id) {
            return Err("Deposit can be added only by contract owner");
        }

        let token_wallet = self.get_token_wallet_mut(&token_id)?;

        token_wallet.deposit = token_wallet
            .deposit
            .checked_add(amount.0)
            .ok_or("Token deposit overflow")?;

        Ok(())
    }

    fn on_transfer_swap(
        &mut self,
        sender_id: AccountId,
        token_id: AccountId,
        amount: U128,
    ) -> Result<(), &'static str> {
        let (token_in, token_out) = self.get_swap_tokens_wallets_mut(&token_id)?;

        // TODO: some swap computation
        Err("Not implemented")
    }

    /// Adds liquidity to the pool from owner's deposit by provided amounts
    #[payable]
    #[handle_result]
    pub fn add_liquidity(&mut self, amounts: [U128; 2]) -> Result<(), &'static str> {
        self.assert_owner();

        let token1_wallet = self
            .token1_wallet
            .as_mut()
            .ok_or("Token1 wallet is not created")?;
        let token2_wallet = self
            .token2_wallet
            .as_mut()
            .ok_or("Token2 wallet is not created")?;

        // Move tokens from deposit
        token1_wallet.deposit = token1_wallet
            .deposit
            .checked_sub(amounts[0].0)
            .ok_or("Not enough deposit for Token1")?;
        token2_wallet.deposit = token2_wallet
            .deposit
            .checked_sub(amounts[1].0)
            .ok_or("Not enough deposit for Token2")?;

        // Move tokens to liquidity
        token1_wallet.liquidity = token1_wallet
            .liquidity
            .checked_add(amounts[0].0)
            .ok_or("Liquidity overflow for Token1")?;
        token2_wallet.liquidity = token2_wallet
            .liquidity
            .checked_add(amounts[1].0)
            .ok_or("Liquidity overflow for Token2")?;

        Ok(())
    }

    /// Remove liquidity from the pool to owner's deposit by provided amounts
    #[payable]
    #[handle_result]
    pub fn remove_liquidity(&mut self, amounts: [U128; 2]) -> Result<(), &'static str> {
        self.assert_owner();

        let token1_wallet = self
            .token1_wallet
            .as_mut()
            .ok_or("Token1 wallet is not created")?;
        let token2_wallet = self
            .token2_wallet
            .as_mut()
            .ok_or("Token2 wallet is not created")?;

        // Move tokens from liquidity
        token1_wallet.liquidity = token1_wallet
            .liquidity
            .checked_sub(amounts[0].0)
            .ok_or("Not enough liquidity for Token1")?;
        token2_wallet.liquidity = token2_wallet
            .liquidity
            .checked_sub(amounts[1].0)
            .ok_or("Not enough liquidity for Token2")?;

        // Move tokens to deposit
        token1_wallet.deposit = token1_wallet
            .deposit
            .checked_add(amounts[0].0)
            .ok_or("Deposit overflow for Token1")?;
        token2_wallet.deposit = token2_wallet
            .deposit
            .checked_add(amounts[1].0)
            .ok_or("Deposit overflow for Token2")?;

        Ok(())
    }

    #[handle_result]
    pub fn get_pool(&self) -> Result<PoolView, &'static str> {
        let token1_wallet = self
            .token1_wallet
            .as_ref()
            .ok_or("Token1 wallet is not created")?;
        let token2_wallet = self
            .token2_wallet
            .as_ref()
            .ok_or("Token2 wallet is not created")?;

        Ok(PoolView {
            token_ids: [
                token1_wallet.token_id.clone(),
                token2_wallet.token_id.clone(),
            ],
            decimals: [
                token1_wallet.metadata.decimals,
                token2_wallet.metadata.decimals,
            ],
            amounts: [
                token1_wallet.liquidity.into(),
                token2_wallet.liquidity.into(),
            ],
            ratio: compute_tokens_ratio(token1_wallet.liquidity, token2_wallet.liquidity)?
                .to_string(),
        })
    }

    pub(crate) fn get_swap_tokens_wallets_mut(
        &mut self,
        token_id_in: &AccountId,
    ) -> Result<(&mut TokenWallet, &mut TokenWallet), &'static str> {
        let token1_wallet = self
            .token1_wallet
            .as_mut()
            .ok_or("Token1 wallet is not registered")?;
        let token2_wallet = self
            .token2_wallet
            .as_mut()
            .ok_or("Token2 wallet is not registered")?;

        match token_id_in.cmp(&token1_wallet.token_id) {
            Ordering::Equal => Ok((token1_wallet, token2_wallet)),
            _ => Ok((token2_wallet, token1_wallet)),
        }
    }

    pub(crate) fn get_token_wallet_mut(
        &mut self,
        token_id: &AccountId,
    ) -> Result<&mut TokenWallet, &'static str> {
        let token1_wallet = self
            .token1_wallet
            .as_mut()
            .ok_or("Token1 wallet is not registered")?;
        let token2_wallet = self
            .token2_wallet
            .as_mut()
            .ok_or("Token2 wallet is not registered")?;

        if token_id == &token1_wallet.token_id {
            return Ok(token1_wallet);
        }

        if token_id == &token2_wallet.token_id {
            return Ok(token2_wallet);
        }

        Err("Token is not supported")
    }
}

impl TokenWallet {
    pub(crate) fn new(token_id: AccountId, metadata: FungibleTokenMetadata) -> Self {
        Self {
            token_id,
            metadata,
            deposit: 0,
            liquidity: 0,
        }
    }
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde", rename_all = "snake_case")]
pub(crate) struct TransferCommand {
    r#type: TransferType,
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde", rename_all = "snake_case")]
pub(crate) enum TransferType {
    Swap,
}
