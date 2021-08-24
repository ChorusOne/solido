// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use solana_program_test::tokio;

use crate::assert_solido_error;
use crate::context::Context;

use lido::error::LidoError;
use lido::token::StLamports;

#[tokio::test]
async fn test_successful_add_validator() {
    let mut context = Context::new_with_maintainer().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 0);

    let validator = context.add_validator().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 1);
    assert_eq!(solido.validators.entries[0].pubkey, validator.vote_account);
    assert_eq!(
        solido.validators.entries[0].entry.fee_address,
        validator.fee_account
    );

    // Adding the validator a second time should fail.
    let result = context.try_add_validator(&validator).await;
    assert_solido_error!(result, LidoError::DuplicatedEntry);
}

#[tokio::test]
async fn test_remove_validator_without_unclaimed_credits() {
    let mut context = Context::new_with_maintainer_and_validator().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 1);

    let vote_account = solido.validators.entries[0].pubkey;
    assert_eq!(solido.validators.entries[0].entry.fee_credit, StLamports(0));

    context
        .try_remove_validator(vote_account)
        .await
        .expect("Failed to remove validator.");

    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 0);
}

#[tokio::test]
async fn test_deactivate_validator() {
    let mut context = Context::new_with_maintainer().await;

    let validator = context.add_validator().await;

    // Initially, the validator should be active.
    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 1);
    assert!(solido.validators.entries[0].entry.active);

    context.deactivate_validator(validator.vote_account).await;

    // After deactivation, it should be inactive.
    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 1);
    assert!(!solido.validators.entries[0].entry.active);

    // Deactivation is idempotent.
    context.deactivate_validator(validator.vote_account).await;
    let solido_after_second_deactivation = context.get_solido().await;
    assert_eq!(solido, solido_after_second_deactivation);
}
