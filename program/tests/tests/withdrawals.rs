// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use crate::context::{Context, StakeDeposit};

use lido::token::{Lamports, StLamports};
use solana_program::stake::state::StakeState;
use solana_program_test::tokio;

pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
pub const TEST_WITHDRAW_AMOUNT: StLamports = StLamports(10_000_000_000);

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

    // This function gives us a delegated stake account with Sol withdrawn from
    // the given stake account.
    let split_stake_account = context
        .withdraw(user, token_addr, TEST_WITHDRAW_AMOUNT, stake_account)
        .await;

    let split_stake_sol_balance = context.get_sol_balance(split_stake_account).await;
    let solido = context.get_solido().await;
    let amount_lamports = solido
        .exchange_rate
        .exchange_st_sol(TEST_WITHDRAW_AMOUNT)
        .unwrap();

    // Amount should be the same as `TEST_WITHDRAW_AMOUNT` because
    // no rewards were distributed
    assert_eq!(amount_lamports, Lamports(10_000_000_000));

    // Assert the new uninitialized stake account's balance is incremented by 10 Sol.
    assert_eq!(
        split_stake_sol_balance,
        (amount_lamports + Lamports(rent.minimum_balance(std::mem::size_of::<StakeState>())))
            .unwrap()
    );
    let stake_account_balance_after = context.get_sol_balance(stake_account).await;
    assert_eq!(
        (stake_account_balance_before - stake_account_balance_after).unwrap(),
        Lamports(10_000_000_000)
    );

    // Check that the stake was indeed withdrawn from the given stake account
    assert_eq!(stake_account_balance_after, Lamports(90_000_000_000));
}
