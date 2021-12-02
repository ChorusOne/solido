use anker::error::AnkerError;
use lido::{error::LidoError, token::Lamports};
use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;
use solana_sdk::{signature::Keypair, signer::Signer};
use testlib::{
    anker_context::{setup_token_pool, Context},
    assert_solido_error,
};

const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(1_000_000_000);

#[tokio::test]
async fn test_successful_change_token_swap_pool() {
    let mut context = Context::new_with_token_pool_rewards(TEST_DEPOSIT_AMOUNT).await;
    let mut new_token_pool = setup_token_pool(&mut context.solido_context).await;

    // Copy UST token info from original Token Swap pool.
    new_token_pool.ust_mint_address = context.token_pool_context.ust_mint_address;
    let ust_mint_authority =
        Keypair::from_bytes(&context.token_pool_context.ust_mint_authority.to_bytes()).unwrap();
    let ust_mint_authority_pubkey = ust_mint_authority.pubkey();
    new_token_pool.ust_mint_authority = ust_mint_authority;
    new_token_pool.token_a = context
        .solido_context
        .create_spl_token_account(new_token_pool.ust_mint_address, ust_mint_authority_pubkey)
        .await;

    new_token_pool
        .initialize_token_pool(&mut context.solido_context)
        .await;
    let new_token_pool_address = new_token_pool.swap_account.pubkey();

    // Keypair doesn't implement copy, hack to copy it so we can pass ref. later.
    let manager = Keypair::from_bytes(&context.solido_context.manager.to_bytes()).unwrap();
    let result = context
        .try_change_token_swap_pool(&manager, new_token_pool_address)
        .await;
    assert!(result.is_ok());
    let anker = context.get_anker().await;
    assert_eq!(anker.token_swap_pool, new_token_pool_address);
}

#[tokio::test]
async fn test_change_token_swap_pool_invalid_pool() {
    let mut context = Context::new_with_token_pool_rewards(TEST_DEPOSIT_AMOUNT).await;
    let new_token_swap = Pubkey::new_unique();
    let manager = Keypair::from_bytes(&context.solido_context.manager.to_bytes()).unwrap();
    let result = context
        .try_change_token_swap_pool(&manager, new_token_swap)
        .await;
    assert_solido_error!(result, AnkerError::WrongSplTokenSwapParameters);
}

#[tokio::test]
async fn test_change_token_swap_pool_different_minters() {
    let mut context = Context::new_with_token_pool_rewards(TEST_DEPOSIT_AMOUNT).await;
    let mut new_token_pool = setup_token_pool(&mut context.solido_context).await;
    new_token_pool.ust_mint_address = context
        .solido_context
        .create_mint(new_token_pool.ust_mint_authority.pubkey())
        .await;
    new_token_pool
        .initialize_token_pool(&mut context.solido_context)
        .await;
    let new_token_pool_address = new_token_pool.swap_account.pubkey();

    let manager = Keypair::from_bytes(&context.solido_context.manager.to_bytes()).unwrap();
    let result = context
        .try_change_token_swap_pool(&manager, new_token_pool_address)
        .await;
    assert_solido_error!(result, AnkerError::WrongSplTokenSwapParameters);
}

#[tokio::test]
async fn test_change_token_swap_pool_different_manager() {
    let mut context = Context::new_with_token_pool_rewards(Lamports(1)).await;
    let new_token_swap = Pubkey::new_unique();
    let wrong_manager = context.solido_context.deterministic_keypair.new_keypair();
    let anker = context.get_anker().await;
    let result = context
        .try_change_token_swap_pool(&wrong_manager, new_token_swap)
        .await;
    assert_solido_error!(result, LidoError::InvalidManager);
    let new_anker = context.get_anker().await;
    assert_eq!(anker.token_swap_pool, new_anker.token_swap_pool);
}
