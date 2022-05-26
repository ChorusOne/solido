// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use solana_program_test::tokio;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::Signer;

use testlib::assert_solido_error;
use testlib::solido_context::{Context, ValidatorAccounts};

use lido::error::LidoError;
use lido::token::Lamports;

pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
pub const TEST_STAKE_DEPOSIT_AMOUNT: Lamports = Lamports(10_000_000_000);

#[tokio::test]
async fn test_successful_add_validator() {
    let mut context = Context::new_with_maintainer().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 0);

    let validator = context.add_validator().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 1);
    assert_eq!(solido.validators.entries[0].pubkey, validator.vote_account);

    // Adding the validator a second time should fail.
    let result = context.try_add_validator(&validator).await;
    assert_solido_error!(result, LidoError::DuplicatedEntry);
}

#[tokio::test]
async fn test_add_validator_with_invalid_owner() {
    let mut context = Context::new_with_maintainer().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 0);
    let node_key = context.deterministic_keypair.new_keypair();
    let real_vote_account = context
        .create_vote_account(&node_key, Pubkey::new_unique(), 100)
        .await;

    let vote_account = context.get_account(real_vote_account).await;
    let owner = context.deterministic_keypair.new_keypair();

    let invalid_vote_account = context
        .create_account(&owner.pubkey(), vote_account.data.len())
        .await;

    let node_account = context.deterministic_keypair.new_keypair();
    let result = context
        .try_add_validator(&ValidatorAccounts {
            node_account: node_account,
            vote_account: invalid_vote_account.pubkey(),
        })
        .await;
    assert_solido_error!(result, LidoError::ValidatorVoteAccountHasDifferentOwner);
}

#[tokio::test]
async fn test_successful_remove_validator() {
    let mut context = Context::new_with_maintainer_and_validator().await;
    let validator = &context.get_solido().await.validators.entries[0];
    context.deactivate_validator(validator.pubkey).await;
    context
        .try_remove_validator(validator.pubkey)
        .await
        .unwrap();

    let solido = context.get_solido().await;
    assert_eq!(solido.validators.len(), 0);
}

#[tokio::test]
async fn test_removing_validator_with_stake_accounts_should_fail() {
    let (mut context, _) = Context::new_with_two_stake_accounts().await;
    let validator = &context.get_solido().await.validators.entries[0];
    let result = context.try_remove_validator(validator.pubkey).await;

    // The validator should not be able to be removed if it is still active
    //  (i.e. the active flag is set toe true OR it has stake accounts)
    assert_solido_error!(result, LidoError::ValidatorIsStillActive);
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
