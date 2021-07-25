// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use crate::context::{Context, StakeDeposit};

use lido::token::{Lamports, StLamports};
use solana_program_test::tokio;
use solana_sdk::signer::Signer;

pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
pub const TEST_WITHDRAW_AMOUNT: StLamports = StLamports(10_000_000_000);

#[tokio::test]
async fn test_withdrawal() {
    let mut context = Context::new_with_maintainer_and_validator().await;
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
    let start_epoch = epoch_schedule.first_normal_epoch;
    let slots_per_epoch = epoch_schedule.slots_per_epoch;

    context.context.warp_to_slot(start_slot).unwrap();

    context.update_exchange_rate().await;

    context
        .withdraw(user, token_addr, TEST_WITHDRAW_AMOUNT, 0, stake_account)
        .await;
}
