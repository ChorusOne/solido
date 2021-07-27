// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use crate::{
    assert_solido_error,
    context::{Context, StakeDeposit},
};

use lido::{
    error::LidoError,
    token::{Lamports, StLamports},
};
use solana_program::stake::state::StakeState;
use solana_program_test::tokio;

pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);

#[tokio::test]
async fn test_withdrawal() {
    let mut context = Context::new_with_maintainer_and_validator().await;
    let rent = context.get_rent().await;

    let (user, token_addr) = context.deposit(TEST_DEPOSIT_AMOUNT).await;
    let validator = context.validator.take().unwrap();
    let stake_account = context
        .stake_deposit(
            validator.vote_account,
            StakeDeposit::Append,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;
    context.validator = Some(validator);

    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;

    context.context.warp_to_slot(start_slot).unwrap();
    context.update_exchange_rate().await;

    let stake_account_balance_before = context.get_sol_balance(stake_account).await;

    let stake_state_size = std::mem::size_of::<StakeState>();
    let minimum_rent = rent.minimum_balance(stake_state_size);

    // Test withdrawing 1 Lamport less than the minimum rent. Should fail
    let split_stake_account = context
        .try_withdraw(
            &user,
            token_addr,
            StLamports(minimum_rent - 1),
            stake_account,
        )
        .await;
    assert!(split_stake_account.is_err());

    // Test withdrawing a value that will leave the stake account with 1 Sol - 1
    // Lamport. Should fail, because the the stake should have at least 1 Sol.
    let split_stake_account = context
        .try_withdraw(&user, token_addr, StLamports(99_000_000_001), stake_account)
        .await;
    assert_solido_error!(split_stake_account, LidoError::InvalidAmount);

    // Should overflow when we try to withdraw more than the stake account has.
    let split_stake_account = context
        .try_withdraw(
            &user,
            token_addr,
            StLamports(100_000_000_001),
            stake_account,
        )
        .await;
    assert_solido_error!(split_stake_account, LidoError::CalculationFailure);

    let test_withdraw_amount = StLamports(minimum_rent + 1);
    // `minimum_rent + 1` is needed by the stake program during the split.
    // This should return an activated stake account with `minimum_rent + 1` Sol.
    let split_stake_account = context
        .withdraw(&user, token_addr, test_withdraw_amount, stake_account)
        .await;

    let split_stake_sol_balance = context.get_sol_balance(split_stake_account).await;
    let solido = context.get_solido().await;
    let amount_lamports = solido
        .exchange_rate
        .exchange_st_sol(test_withdraw_amount)
        .unwrap();

    // Amount should be the same as `minimum_rent + 1` because
    // no rewards were distributed
    assert_eq!(amount_lamports, Lamports(minimum_rent + 1));

    // Assert the new uninitialized stake account's balance is incremented by 10 Sol.
    assert_eq!(split_stake_sol_balance, amount_lamports);
    let stake_account_balance_after = context.get_sol_balance(stake_account).await;
    assert_eq!(
        (stake_account_balance_before - stake_account_balance_after).unwrap(),
        Lamports(minimum_rent + 1)
    );

    // Check that the stake was indeed withdrawn from the given stake account
    // Hard-coded the amount - rent, in case rent changes we'll know.
    assert_eq!(stake_account_balance_after, Lamports(99_997_717_119));

    // Test if we updated the metrics
    let solido_after = context.get_solido().await;
    assert_eq!(
        solido_after.metrics.withdraw_amount.total_st_sol_amount,
        test_withdraw_amount
    );
    assert_eq!(
        solido_after.metrics.withdraw_amount.total_sol_amount,
        Lamports(test_withdraw_amount.0)
    );
    assert_eq!(solido_after.metrics.withdraw_amount.count, 1);
}

#[tokio::test]
async fn test_withdrawal_from_different_validator() {
    let mut context = Context::new_with_maintainer_and_validator().await;
    let validator = context.validator.take().unwrap();
    let other_validator = context.add_validator().await;

    let (user, token_addr) = context.deposit(TEST_DEPOSIT_AMOUNT).await;
    let stake_account = context
        .stake_deposit(
            validator.vote_account,
            StakeDeposit::Append,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;
    context.validator = Some(other_validator);

    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;

    context.context.warp_to_slot(start_slot).unwrap();
    context.update_exchange_rate().await;

    let split_stake_account = context
        .try_withdraw(&user, token_addr, StLamports(1_000_000_000), stake_account)
        .await;
    assert_solido_error!(split_stake_account, LidoError::ValidatorWithMoreStakeExists);
}
