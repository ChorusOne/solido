#![cfg(feature = "test-bpf")]

use crate::assert_solido_error;
use crate::context::Context;

use lido::error::LidoError;
use solana_program_test::tokio;
use solana_sdk::signature::{Keypair, Signer};

#[tokio::test]
async fn test_successful_add_remove_maintainer() {
    let mut context = Context::new_empty().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.maintainers.len(), 0);

    let maintainer = Keypair::new();

    context
        .try_add_maintainer(maintainer.pubkey())
        .await
        .expect("Failed to add maintainer.");

    let solido = context.get_solido().await;
    assert_eq!(solido.maintainers.len(), 1);
    assert_eq!(solido.maintainers.entries[0].pubkey, maintainer.pubkey());

    // Adding the maintainer a second time should fail.
    let result = context.try_add_maintainer(maintainer.pubkey()).await;
    assert_solido_error!(result, LidoError::DuplicatedEntry);

    context
        .try_remove_maintainer(maintainer.pubkey())
        .await
        .expect("Failed to remove maintainer.");

    let solido = context.get_solido().await;
    let has_maintainer = solido
        .maintainers
        .entries
        .iter()
        .any(|pe| pe.pubkey == maintainer.pubkey());
    assert!(!has_maintainer);
    assert_eq!(solido.maintainers.len(), 0);
}
