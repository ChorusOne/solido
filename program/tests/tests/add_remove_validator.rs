// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use solana_program_test::tokio;

use crate::assert_solido_error;
use crate::context::{Context, StakeDeposit};

use lido::error::LidoError;
use lido::state::Validator;
use lido::token::Lamports;
use lido::token::StLamports;

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

    let result = context.try_remove_validator(vote_account).await;

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
#[tokio::test]
async fn test_removing_validator_with_stake_accounts_should_fail() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;

    // Sanity check before we start: the validator should have zero balance in zero stake accounts.
    let solido_before = context.get_solido().await;
    let validator_before: &Validator = &solido_before.validators.entries[0].entry;
    assert_eq!(validator_before.stake_accounts_balance, Lamports(0));
    assert_eq!(validator_before.stake_seeds.stake_accounts_seed_begin, 0);
    assert_eq!(validator_before.stake_seeds.stake_accounts_seed_end, 0);

    // Now we make a deposit, and then delegate part of it.
    context.deposit(TEST_DEPOSIT_AMOUNT).await;

    let stake_account = context
        .stake_deposit(
            validator.vote_account,
            StakeDeposit::Append,
            TEST_STAKE_DEPOSIT_AMOUNT,
        )
        .await;

    // The amount that we staked, should now be in the stake account.
    assert_eq!(
        context.get_sol_balance(stake_account).await,
        TEST_STAKE_DEPOSIT_AMOUNT
    );

    // We should also have recorded in the Solido state that this validator now
    // has balance in a stake account.
    let solido_after = context.get_solido().await;

    let validator_after = &solido_after.validators.entries[0].entry;
    assert_eq!(
        validator_after.stake_accounts_balance,
        TEST_STAKE_DEPOSIT_AMOUNT
    );

    // This was also the first deposit, so that should have created one stake account.
    assert_eq!(validator_after.stake_seeds.begin, 0);
    assert_eq!(validator_after.stake_seeds.end, 1);

    let result = context.try_remove_validator(validator.vote_account).await;

    // The validator should not be able to be removed if it is still active
    //  (i.e. the active flag is set toe true OR it has stake accounts)
    assert_solido_error!(result, LidoError::ValidatorIsStillActive);
}
