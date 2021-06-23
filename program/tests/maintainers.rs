#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{program_test, simple_add_maintainer, simple_remove_maintainer, LidoAccounts};
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::{Keypair, Signer};

async fn setup() -> (ProgramTestContext, LidoAccounts) {
    let mut context = program_test().start_with_context().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts.initialize_lido(&mut context).await;
    (context, lido_accounts)
}

#[tokio::test]
async fn test_successful_add_remove_maintainer() {
    let (mut context, lido_accounts) = setup().await;

    let maintainer = Keypair::new();
    simple_add_maintainer(&mut context, &maintainer.pubkey(), &lido_accounts).await;

    let lido = lido_accounts.get_solido(&mut context).await;

    let has_maintainer = lido
        .maintainers
        .entries
        .iter()
        .any(|pe| pe.pubkey == maintainer.pubkey());
    assert!(has_maintainer);

    simple_remove_maintainer(&mut context, &lido_accounts, &maintainer.pubkey())
        .await
        .unwrap();

    let lido = lido_accounts.get_solido(&mut context).await;

    let has_maintainer = lido
        .maintainers
        .entries
        .iter()
        .any(|pe| pe.pubkey == maintainer.pubkey());
    assert!(!has_maintainer);
}
