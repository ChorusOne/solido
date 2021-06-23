#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{program_test, simple_add_validator_to_pool, LidoAccounts};
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::Signer;

async fn setup() -> (ProgramTestContext, LidoAccounts) {
    let mut context = program_test().start_with_context().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts.initialize_lido(&mut context).await;
    (context, lido_accounts)
}

#[tokio::test]
async fn test_successful_remove_validator() {
    let (mut context, lido_accounts) = setup().await;

    let accounts = simple_add_validator_to_pool(&mut context, &lido_accounts).await;

    let lido = lido_accounts.get_solido(&mut context).await;
    assert_eq!(lido.validators.len(), 1);

    lido_accounts
        .remove_validator(&mut context, &accounts.vote_account.pubkey())
        .await;

    let lido = lido_accounts.get_solido(&mut context).await;
    assert_eq!(lido.validators.len(), 0);
}

// TODO(#179) Add Test for Remove Validator with Unclaimed Rewards
#[tokio::test]
async fn test_remove_validator_with_unclaimed_rewards() {}
