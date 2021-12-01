use anker::error::AnkerError;
use lido::error::LidoError;
use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;
use solana_sdk::{signature::Keypair, signer::Signer};
use testlib::{anker_context::Context, assert_solido_error};

#[tokio::test]
async fn test_successful_change_token_swap_pool() {
    let mut context = Context::new_with_initialized_token_pool().await;
    // Initialize other context to get an initialized token pool.
    let other_context = Context::new_with_initialized_token_pool().await;
    let new_token_swap = other_context.token_pool_context.swap_account.pubkey();

    let manager = Keypair::from_bytes(&context.solido_context.manager.to_bytes()).unwrap();
    let result = context
        .try_change_token_swap_pool(&manager, new_token_swap)
        .await;
    assert!(result.is_ok());

    let anker = context.get_anker().await;
    assert_eq!(anker.token_swap_pool, new_token_swap);
}

#[tokio::test]
async fn test_change_token_swap_pool_invalid_pool() {
    let mut context = Context::new_with_initialized_token_pool().await;
    let new_token_swap = Pubkey::new_unique();
    let manager = Keypair::from_bytes(&context.solido_context.manager.to_bytes()).unwrap();
    let result = context
        .try_change_token_swap_pool(&manager, new_token_swap)
        .await;
    assert_solido_error!(result, AnkerError::WrongSplTokenSwapParameters);
}

#[tokio::test]
async fn test_change_token_swap_pool_different_manager() {
    let mut context = Context::new_with_initialized_token_pool().await;
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
