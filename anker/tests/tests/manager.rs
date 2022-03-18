use std::str::FromStr;

use anker::{error::AnkerError, wormhole::TerraAddress};
use lido::{error::LidoError, token::Lamports};
use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;
use solana_sdk::{signature::Keypair, signer::Signer};
use testlib::{
    anker_context::{setup_token_pool, Context},
    assert_solido_error,
};

const DEPOSIT_AMOUNT: Lamports = Lamports(1_000_000_000);

#[tokio::test]
async fn test_successful_change_token_swap_pool() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(DEPOSIT_AMOUNT)
        .await;
    let mut new_token_pool = setup_token_pool(&mut context.solido_context).await;

    // Copy UST token info from original Token Swap pool.
    new_token_pool.ust_mint_address = context.token_pool_context.ust_mint_address;
    let ust_mint_authority =
        Keypair::from_bytes(&context.token_pool_context.ust_mint_authority.to_bytes()).unwrap();
    new_token_pool.ust_mint_authority = ust_mint_authority;
    new_token_pool.token_a = context
        .solido_context
        .create_spl_token_account(
            new_token_pool.ust_mint_address,
            new_token_pool.get_authority(),
        )
        .await;

    new_token_pool
        .initialize_token_pool(&mut context.solido_context)
        .await;
    let new_token_pool_address = new_token_pool.swap_account.pubkey();
    let result = context
        .try_change_token_swap_pool(new_token_pool_address)
        .await;
    assert!(result.is_ok());
    let anker = context.get_anker().await;
    assert_eq!(anker.token_swap_pool, new_token_pool_address);
}

#[tokio::test]
async fn test_change_token_swap_pool_invalid_pool() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(DEPOSIT_AMOUNT)
        .await;
    let new_token_swap = Pubkey::new_unique();
    let result = context.try_change_token_swap_pool(new_token_swap).await;
    assert_solido_error!(result, AnkerError::WrongSplTokenSwapParameters);
}

#[tokio::test]
async fn test_change_token_swap_pool_different_minters() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(DEPOSIT_AMOUNT)
        .await;
    let mut new_token_pool = setup_token_pool(&mut context.solido_context).await;
    new_token_pool
        .initialize_token_pool(&mut context.solido_context)
        .await;
    let new_token_pool_address = new_token_pool.swap_account.pubkey();

    let result = context
        .try_change_token_swap_pool(new_token_pool_address)
        .await;
    assert_solido_error!(result, AnkerError::WrongSplTokenSwapParameters);
}

#[tokio::test]
async fn test_change_token_swap_pool_different_manager() {
    let mut context = Context::new().await;
    // Token pool doesn't matter, and can be left uninitialized/invalid, as the
    // manager is evaluated earlier.
    let new_token_swap = Pubkey::new_unique();
    context.solido_context.manager = context.solido_context.deterministic_keypair.new_keypair();
    let anker = context.get_anker().await;
    let result = context.try_change_token_swap_pool(new_token_swap).await;
    assert_solido_error!(result, LidoError::InvalidManager);
    let new_anker = context.get_anker().await;
    assert_eq!(anker.token_swap_pool, new_anker.token_swap_pool);
}

#[tokio::test]
async fn test_successful_change_terra_rewards_destination() {
    let mut context = Context::new().await;
    let new_terra_rewards_address =
        TerraAddress::from_str("terra1fex9f78reuwhfsnc8sun6mz8rl9zwqh03fhwf3").unwrap();
    let manager = Keypair::from_bytes(&context.solido_context.manager.to_bytes()).unwrap();
    let result = context
        .try_change_terra_rewards_destination(&manager, new_terra_rewards_address.clone())
        .await;
    assert!(result.is_ok());
    let anker = context.get_anker().await;
    assert_eq!(anker.terra_rewards_destination, new_terra_rewards_address);
}

#[tokio::test]
async fn test_change_terra_rewards_destination_different_manager() {
    let mut context = Context::new().await;
    let new_terra_rewards_address =
        TerraAddress::from_str("terra1fex9f78reuwhfsnc8sun6mz8rl9zwqh03fhwf3").unwrap();
    let wrong_manager = context.solido_context.deterministic_keypair.new_keypair();
    let anker = context.get_anker().await;
    let result = context
        .try_change_terra_rewards_destination(&wrong_manager, new_terra_rewards_address)
        .await;
    assert_solido_error!(result, LidoError::InvalidManager);
    let new_anker = context.get_anker().await;
    assert_eq!(
        anker.terra_rewards_destination,
        new_anker.terra_rewards_destination
    );
}

#[tokio::test]
async fn test_successful_change_sell_rewards_min_out_bps() {
    let mut context = Context::new().await;
    let sell_rewards_min_out_bps = 10;
    let manager = Keypair::from_bytes(&context.solido_context.manager.to_bytes()).unwrap();
    let result = context
        .try_change_sell_rewards_min_out_bps(&manager, sell_rewards_min_out_bps)
        .await;
    assert!(result.is_ok());
    let anker = context.get_anker().await;
    assert_eq!(anker.sell_rewards_min_out_bps, sell_rewards_min_out_bps);
}

#[tokio::test]
async fn test_change_sell_rewards_min_out_bps_more_than_100_percent() {
    let mut context = Context::new().await;
    let sell_rewards_min_out_bps = 1_000_001;
    let manager = Keypair::from_bytes(&context.solido_context.manager.to_bytes()).unwrap();
    let result = context
        .try_change_sell_rewards_min_out_bps(&manager, sell_rewards_min_out_bps)
        .await;
    assert_solido_error!(result, AnkerError::InvalidSellRewardsMinBps);
}
