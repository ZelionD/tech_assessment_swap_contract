use crate::tokens::PoolView;
use assert_matches::assert_matches;
use near_sdk::json_types::U128;
use near_sdk::serde_json::{self, json};
use near_sdk::{ONE_NEAR, ONE_YOCTO};
use near_units::parse_near;
use primitive_types::U256;
use std::str::FromStr;
use workspaces::network::{NetworkClient, NetworkInfo};
use workspaces::{
    types::{KeyType, SecretKey},
    Account, AccountId, Contract, DevNetwork, Worker,
};

#[tokio::test]
async fn test_create_wallets() -> anyhow::Result<()> {
    let worker_testnet = workspaces::testnet_archival().await?;
    let worker = workspaces::sandbox().await?;

    let usdt_token_id = "usdt.fakes.testnet".parse()?;
    let _ = worker
        .import_contract(&usdt_token_id, &worker_testnet)
        .initial_balance(parse_near!("10000000 N"))
        .with_data()
        .block_height(72_000_000)
        .transact()
        .await?;

    let usdn_token_id = "usdn.testnet".parse()?;
    let _ = worker
        .import_contract(&usdn_token_id, &worker_testnet)
        .initial_balance(parse_near!("10000000 N"))
        .with_data()
        .block_height(82_000_000)
        .transact()
        .await?;

    // Register a sub-user account as owner
    let owner = gen_user_account(&worker, "user.test.near").await?;
    let contract = build_contract(
        &worker,
        "./",
        json!({
          "owner_id": owner.id(),
        }),
    )
    .await?;

    create_token_wallets(&contract, &owner, &usdn_token_id, &usdt_token_id).await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            token_ids,
            decimals,
            ..
        } if token_ids == [
          near_sdk::AccountId::new_unchecked(usdn_token_id.to_string()),
          near_sdk::AccountId::new_unchecked(usdt_token_id.to_string())
        ] && decimals == [6, 6]
    );

    Ok(())
}

#[tokio::test]
async fn test_liquidity() -> anyhow::Result<()> {
    let worker_testnet = workspaces::testnet_archival().await?;
    let worker = workspaces::sandbox().await?;

    let usdt_token_id = "usdt.fakes.testnet".parse()?;
    let usdt_contract = worker
        .import_contract(&usdt_token_id, &worker_testnet)
        .initial_balance(parse_near!("10000000 N"))
        .with_data()
        .block_height(72_000_000)
        .transact()
        .await?;

    let usdn_token_id = "usdn.testnet".parse()?;
    let usdn_contract = worker
        .import_contract(&usdn_token_id, &worker_testnet)
        .initial_balance(parse_near!("10000000 N"))
        .with_data()
        .block_height(82_000_000)
        .transact()
        .await?;

    // Register a sub-user account as owner
    let owner = gen_user_account(&worker, "user.test.near").await?;
    let contract = build_contract(
        &worker,
        "./",
        json!({
          "owner_id": owner.id(),
          "tokens": (usdn_token_id, usdt_token_id),
        }),
    )
    .await?;

    // Transfer some $NEAR funds to sub-account
    let _ = contract
        .as_account()
        .transfer_near(owner.id(), 50 * ONE_NEAR)
        .await?
        .into_result()?;

    // Register an owner at USDN fungible token contract
    register_user(&usdn_contract, &owner).await?;

    // Register an owner at USDT fungible token contract
    register_user(&usdt_contract, &owner).await?;

    // Register USDN fungible token contract at USDT fungible token contract
    register_user(&usdt_contract, usdn_contract.as_account()).await?;

    // Mint USDT tokens for USDN exchange
    mint_usdt_tokens(&usdt_contract, usdn_contract.as_account(), 5_000_000.into()).await?;

    // Mint USDT tokens for an owner
    mint_usdt_tokens(&usdt_contract, &owner, 1_000_000.into()).await?;

    // Exchange some USDT for USDN tokens for an owner
    exchange_usdt_for_usdn(&usdn_contract, &owner, 1_000_000.into()).await?;

    // Send some USDN as deposit by owner
    deposit_tokens(
        &usdn_contract,
        &owner,
        contract.as_account(),
        1_000_000.into(),
    )
    .await?;

    // Send some USDT as deposit by owner
    deposit_tokens(
        &usdt_contract,
        &owner,
        contract.as_account(),
        1_000_000.into(),
    )
    .await?;

    // Add some liquidity to the contract swap pool using owner's deposit
    add_liquidity(&contract, &owner, [1_000.into(), 1_000.into()]).await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            amounts,
            ratio,
            ..
        } if amounts == [1_000.into(), 1_000.into()] && ratio == U256::from(1_000_000).to_string()
    );

    // Add some liquidity to the contract swap pool using an owner's deposit
    add_liquidity(&contract, &owner, [500.into(), 100.into()]).await?;

    // Remove some liquidity from the contract swap pool to an owner's deposit
    remove_liquidity(&contract, &owner, [150.into(), 10.into()]).await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            amounts,
            ratio,
            ..
        } if amounts == [1_350.into(), 1_090.into()] && ratio == U256::from(1_471_500).to_string()
    );

    Ok(())
}

pub(crate) async fn gen_user_account<T>(
    worker: &Worker<T>,
    account_id: &str,
) -> anyhow::Result<Account>
where
    T: DevNetwork + Send + Sync,
{
    let id = workspaces::AccountId::from_str(account_id)?;
    let sk = SecretKey::from_random(KeyType::ED25519);

    let account = worker.create_tla(id, sk).await?.into_result()?;

    Ok(account)
}

pub(crate) async fn build_contract<T>(
    worker: &Worker<T>,
    project_path: &str,
    args_json: serde_json::Value,
) -> anyhow::Result<Contract>
where
    T: NetworkInfo + NetworkClient + DevNetwork + Send + Sync,
{
    let wasm = workspaces::compile_project(project_path).await?;
    let (id, sk) = worker.dev_generate().await;

    let contract = worker
        .create_tla_and_deploy(id, sk, &wasm)
        .await?
        .into_result()?;

    // initialize contract
    let res = contract
        .call("init")
        .args_json(args_json)
        .max_gas()
        .transact()
        .await;

    match res {
        Ok(_) => (),
        Err(_) => {
            return Err(anyhow::Error::msg(format!(
                "Failed to build & init contract. Log {:?}",
                res
            )))
        }
    };

    Ok(contract)
}

async fn register_user(target_contract: &Contract, user: &Account) -> anyhow::Result<()> {
    let res = user
        .call(target_contract.id(), "storage_deposit")
        .args_json(json!({
            "registration_only": true,
        }))
        .deposit(ONE_NEAR)
        .max_gas()
        .transact()
        .await?;

    match res.clone().into_result() {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::Error::msg(format!(
            "Registration of user `{:?}` failed on target contract `{:?}`. Log {:?}",
            user.id(),
            target_contract.id(),
            res
        ))),
    }
}

async fn mint_usdt_tokens(
    usdt_contract: &Contract,
    user: &Account,
    amount: U128,
) -> anyhow::Result<()> {
    let res = user
        .call(usdt_contract.id(), "mint")
        .args_json(json!({
            "account_id": user.id(),
            "amount": amount,
        }))
        .max_gas()
        .transact()
        .await?;

    match res.clone().into_result() {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::Error::msg(format!(
            "Mint USDT for user `{:?}` failed. Log {:?}",
            user.id(),
            res
        ))),
    }
}

async fn exchange_usdt_for_usdn(
    usdn_contract: &Contract,
    user: &Account,
    amount: U128,
) -> anyhow::Result<()> {
    let res = usdn_contract
        .call("ft_transfer")
        .args_json(json!({
            "receiver_id": user.id(),
            "amount": amount,
            "msg": "",
        }))
        .max_gas()
        .deposit(ONE_YOCTO)
        .transact()
        .await?;

    match res.clone().into_result() {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::Error::msg(format!(
            "Exchange USDT to USDN for user `{:?}` failed. Log {:?}",
            user.id(),
            res
        ))),
    }
}

async fn deposit_tokens(
    token_contract: &Contract,
    sender: &Account,
    receiver: &Account,
    amount: U128,
) -> anyhow::Result<()> {
    let res = sender
        .call(token_contract.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": receiver.id(),
            "amount": amount,
            "msg": "",
        }))
        .max_gas()
        .deposit(ONE_YOCTO)
        .transact()
        .await?;

    match res.clone().into_result() {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::Error::msg(format!(
            "Deposit tokens `{:?}` by user `{:?}` to user `{:?}` failed. Log {:?}",
            token_contract.id(),
            sender.id(),
            receiver.id(),
            res
        ))),
    }
}

async fn add_liquidity(
    pool_contract: &Contract,
    user: &Account,
    amounts: [U128; 2],
) -> anyhow::Result<()> {
    let res = user
        .call(pool_contract.id(), "add_liquidity")
        .args_json(json!({
            "amounts": amounts,
        }))
        .deposit(ONE_YOCTO)
        .max_gas()
        .transact()
        .await?;

    match res.clone().into_result() {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::Error::msg(format!(
            "Add liquidity to the pool `{:?}` by user `{:?}` failed. Log {:?}",
            pool_contract.id(),
            user.id(),
            res
        ))),
    }
}

async fn remove_liquidity(
    pool_contract: &Contract,
    user: &Account,
    amounts: [U128; 2],
) -> anyhow::Result<()> {
    let res = user
        .call(pool_contract.id(), "remove_liquidity")
        .args_json(json!({
            "amounts": amounts,
        }))
        .deposit(ONE_YOCTO)
        .max_gas()
        .transact()
        .await?;

    match res.clone().into_result() {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::Error::msg(format!(
            "Remove liquidity from the pool `{:?}` by user `{:?}` failed. Log {:?}",
            pool_contract.id(),
            user.id(),
            res
        ))),
    }
}

async fn get_pool_view(pool_contract: &Contract) -> anyhow::Result<PoolView> {
    let res = pool_contract.view("get_pool").args_json(json!(())).await;

    match res {
        Ok(res) => res
            .json::<PoolView>()
            .map_err(|e| anyhow::Error::msg(format!("Parse `PoolView` failed. {:?}", e))),
        Err(_) => Err(anyhow::Error::msg(format!(
            "View pool `{:?}` failed. Log {:?}",
            pool_contract.id(),
            res
        ))),
    }
}

async fn create_token_wallets(
    pool_contract: &Contract,
    owner: &Account,
    token1_id: &AccountId,
    token2_id: &AccountId,
) -> anyhow::Result<()> {
    let res = owner
        .call(pool_contract.id(), "owner_create_wallets")
        .args_json(json!({
          "token1": token1_id,
          "token2": token2_id,
        }))
        .max_gas()
        .deposit(2 * ONE_NEAR)
        .transact()
        .await?;

    match res.clone().into_result() {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::Error::msg(format!(
            "Failed to create tokens wallets. Log {:?}",
            res
        ))),
    }
}
