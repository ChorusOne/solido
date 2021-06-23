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
async fn test_successful_add_validator() {
    let (mut context, lido_accounts) = setup().await;

    let accounts = simple_add_validator_to_pool(&mut context, &lido_accounts).await;

    let lido = lido_accounts.get_solido(&mut context).await;
    let has_stake_account = lido.validators.get(&accounts.vote_account.pubkey()).is_ok();

    // Validator is inside the credit structure
    assert!(has_stake_account);

    let has_token_account = lido
        .validators
        .entries
        .iter()
        .any(|pe| pe.entry.fee_address == accounts.fee_account.pubkey());

    // Validator token account is the same one as provided
    assert!(has_token_account);

    assert_eq!(lido.validators.len(), 1);
}
