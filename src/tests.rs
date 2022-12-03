use crate::tokens::{PoolView, TransferCommand, TransferType};
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

const ONE_USDT: u128 = 1_000_000; // 6 decimals
const ONE_USDN: u128 = 1_000_000; // 6 decimals
const ONE_ETH: u128 = 1_000_000_000_000_000_000; // 18 decimals

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
    let owner = gen_user_account(&worker, "owner.test.near").await?;
    let contract = build_contract(
        &worker,
        "./",
        json!({
          "owner_id": owner.id(),
          "tokens": (usdn_token_id, usdt_token_id),
        }),
    )
    .await?;

    // Transfer some $NEAR funds to owner sub-account
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
    mint_tokens(
        &usdt_contract,
        usdn_contract.as_account(),
        (100_000 * ONE_USDT).into(),
    )
    .await?;

    // Mint USDT tokens for an owner
    mint_tokens(&usdt_contract, &owner, (100_000 * ONE_USDT).into()).await?;

    // Exchange some USDT for USDN tokens for an owner
    exchange_usdt_for_usdn(&usdn_contract, &owner, (100_000 * ONE_USDN).into()).await?;

    // Send some USDN as deposit by owner
    deposit_tokens(
        &usdn_contract,
        &owner,
        contract.as_account(),
        (100_000 * ONE_USDN).into(),
    )
    .await?;

    // Send some USDT as deposit by owner
    deposit_tokens(
        &usdt_contract,
        &owner,
        contract.as_account(),
        (100_000 * ONE_USDT).into(),
    )
    .await?;

    // Add some liquidity to the contract swap pool using owner's deposit
    add_liquidity(
        &contract,
        &owner,
        [(50_000 * ONE_USDT).into(), (50_000 * ONE_USDN).into()],
    )
    .await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            amounts,
            ratio,
            ..
        } if amounts == [(50_000 * ONE_USDT).into(), (50_000 * ONE_USDN).into()] && ratio == U256::from(2_500_000_000_000_000_000_000u128).to_string()
    );

    // Add some liquidity to the contract swap pool using an owner's deposit
    add_liquidity(
        &contract,
        &owner,
        [(500 * ONE_USDT).into(), (100 * ONE_USDN).into()],
    )
    .await?;

    // Remove some liquidity from the contract swap pool to an owner's deposit
    remove_liquidity(
        &contract,
        &owner,
        [(150 * ONE_USDT).into(), (10 * ONE_USDN).into()],
    )
    .await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            amounts,
            ratio,
            ..
        } if amounts == [(50_350 * ONE_USDT).into(), (50_090 * ONE_USDN).into()] && ratio == U256::from(2_522_031_500_000_000_000_000u128).to_string()
    );

    Ok(())
}

#[tokio::test]
async fn test_swap_usdn_usdt() -> anyhow::Result<()> {
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

    // Register a sub-user account
    let user = gen_user_account(&worker, "user.test.near").await?;

    // Register a sub-user account as owner
    let owner = gen_user_account(&worker, "owner.test.near").await?;
    let contract = build_contract(
        &worker,
        "./",
        json!({
          "owner_id": owner.id(),
          "tokens": (usdn_token_id, usdt_token_id),
        }),
    )
    .await?;

    // Transfer some $NEAR funds to user sub-account
    let _ = contract
        .as_account()
        .transfer_near(user.id(), 25 * ONE_NEAR)
        .await?
        .into_result()?;

    // Transfer some $NEAR funds to owner sub-account
    let _ = contract
        .as_account()
        .transfer_near(owner.id(), 25 * ONE_NEAR)
        .await?
        .into_result()?;

    // Register a user at USDN fungible token contract
    register_user(&usdn_contract, &user).await?;

    // Register an user at USDT fungible token contract
    register_user(&usdt_contract, &user).await?;

    // Register an owner at USDN fungible token contract
    register_user(&usdn_contract, &owner).await?;

    // Register an owner at USDT fungible token contract
    register_user(&usdt_contract, &owner).await?;

    // Register USDN fungible token contract at USDT fungible token contract
    register_user(&usdt_contract, usdn_contract.as_account()).await?;

    // Mint USDT tokens for USDN exchange
    mint_tokens(
        &usdt_contract,
        usdn_contract.as_account(),
        (300_000 * ONE_USDT).into(),
    )
    .await?;

    // Mint USDT tokens for a user
    mint_tokens(&usdt_contract, &user, (100_000 * ONE_USDT).into()).await?;

    // Exchange some USDT for USDN tokens for a user
    exchange_usdt_for_usdn(&usdn_contract, &user, (100_000 * ONE_USDN).into()).await?;

    // Mint USDT tokens for an owner
    mint_tokens(&usdt_contract, &owner, (100_000 * ONE_USDT).into()).await?;

    // Exchange some USDT for USDN tokens for an owner
    exchange_usdt_for_usdn(&usdn_contract, &owner, (100_000 * ONE_USDN).into()).await?;

    // Send some USDN as deposit by owner
    deposit_tokens(
        &usdn_contract,
        &owner,
        contract.as_account(),
        (100_000 * ONE_USDN).into(),
    )
    .await?;

    // Send some USDT as deposit by owner
    deposit_tokens(
        &usdt_contract,
        &owner,
        contract.as_account(),
        (100_000 * ONE_USDT).into(),
    )
    .await?;

    // Add some liquidity to the contract swap pool using owner's deposit
    add_liquidity(
        &contract,
        &owner,
        [(50_000 * ONE_USDN).into(), (50_000 * ONE_USDT).into()],
    )
    .await?;

    // Swap 1000 USDN for USDT
    swap_tokens(
        &usdn_contract,
        &user,
        contract.as_account(),
        (1_000 * ONE_USDN).into(),
    )
    .await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            amounts,
            ratio,
            ..
        } if amounts == [(51_000 * ONE_USDN).into(), 49_019_607_843.into()] && ratio == U256::from(2_499_999_999_993_000_000_000u128).to_string()
    );

    // Swap 1000 USDT for USDN
    swap_tokens(
        &usdt_contract,
        &user,
        contract.as_account(),
        (1_000 * ONE_USDT).into(),
    )
    .await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            amounts,
            ratio,
            ..
        } if amounts == [49_937_933_851.into(), 50_019_607_843.into()] && ratio == U256::from(2_497_875_867_716_694_793_393u128).to_string()
    );

    Ok(())
}

#[tokio::test]
async fn test_swap_eth_usdt() -> anyhow::Result<()> {
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

    let eth_token_id = "eth.fakes.testnet".parse()?;
    let eth_contract = worker
        .import_contract(&eth_token_id, &worker_testnet)
        .initial_balance(parse_near!("10000000 N"))
        .with_data()
        .block_height(82_000_000)
        .transact()
        .await?;

    // Register a sub-user account
    let user = gen_user_account(&worker, "user.test.near").await?;

    // Register a sub-user account as owner
    let owner = gen_user_account(&worker, "owner.test.near").await?;
    let contract = build_contract(
        &worker,
        "./",
        json!({
          "owner_id": owner.id(),
          "tokens": (eth_token_id, usdt_token_id),
        }),
    )
    .await?;

    // Transfer some $NEAR funds to user sub-account
    let _ = contract
        .as_account()
        .transfer_near(user.id(), 25 * ONE_NEAR)
        .await?
        .into_result()?;

    // Transfer some $NEAR funds to owner sub-account
    let _ = contract
        .as_account()
        .transfer_near(owner.id(), 25 * ONE_NEAR)
        .await?
        .into_result()?;

    // Register a user at ETH fungible token contract
    register_user(&eth_contract, &user).await?;

    // Register an user at USDT fungible token contract
    register_user(&usdt_contract, &user).await?;

    // Register an owner at ETH fungible token contract
    register_user(&eth_contract, &owner).await?;

    // Register an owner at USDT fungible token contract
    register_user(&usdt_contract, &owner).await?;

    // Mint ETH tokens for a user
    mint_tokens(&eth_contract, &user, (50 * ONE_ETH).into()).await?;

    // Mint USDT tokens for a user
    mint_tokens(&usdt_contract, &user, (100_000 * ONE_USDT).into()).await?;

    // Mint ETH tokens for an owner
    mint_tokens(&eth_contract, &owner, (50 * ONE_ETH).into()).await?;

    // Mint USDT tokens for an owner
    mint_tokens(&usdt_contract, &owner, (100_000 * ONE_USDT).into()).await?;

    // Send some ETH as deposit by owner
    deposit_tokens(
        &eth_contract,
        &owner,
        contract.as_account(),
        (50 * ONE_ETH).into(),
    )
    .await?;

    // Send some USDT as deposit by owner
    deposit_tokens(
        &usdt_contract,
        &owner,
        contract.as_account(),
        (100_000 * ONE_USDT).into(),
    )
    .await?;

    // Add some liquidity to the contract swap pool using owner's deposit
    add_liquidity(
        &contract,
        &owner,
        [(50 * ONE_ETH).into(), (100_000 * ONE_USDT).into()],
    )
    .await?;

    // Swap 1 ETH for USDT
    swap_tokens(&eth_contract, &user, contract.as_account(), ONE_ETH.into()).await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            amounts,
            ratio,
            ..
        } if amounts == [(51 * ONE_ETH).into(), 98_039_215_686.into()] && ratio == U256::from(4_999_999_999_986_000_000_000_000_000_000u128).to_string()
    );

    // Swap 2000 USDT for ETH
    swap_tokens(
        &usdt_contract,
        &user,
        contract.as_account(),
        (2_000 * ONE_USDT).into(),
    )
    .await?;

    let pool_view = get_pool_view(&contract).await?;
    assert_matches!(
        pool_view,
        PoolView {
            amounts,
            ratio,
            ..
        } if amounts == [49_937_933_850_548_209_693.into(), 100_039_215_686.into()] && ratio == U256::from(4_995_751_735_388_192_838_819_182_844_398u128).to_string()
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

async fn mint_tokens(
    token_contract: &Contract,
    user: &Account,
    amount: U128,
) -> anyhow::Result<()> {
    let res = user
        .call(token_contract.id(), "mint")
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
            "Mint token `{:?}` for user `{:?}` failed. Log {:?}",
            token_contract.id(),
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

async fn swap_tokens(
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
            "msg": serde_json::to_string(&TransferCommand { r#type: TransferType::Swap }).unwrap(),
        }))
        .max_gas()
        .deposit(ONE_YOCTO)
        .transact()
        .await?;

    match res.clone().into_result() {
        Ok(_) => Ok(()),
        Err(_) => Err(anyhow::Error::msg(format!(
            "Swap tokens `{:?}` by user `{:?}` at pool `{:?}` failed. Log {:?}",
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
