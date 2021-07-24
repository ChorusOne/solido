// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use crate::context::{Context, StakeDeposit};
use crate::{assert_error_code, assert_solido_error};

use lido::error::LidoError;
use lido::token::Lamports;
use solana_program_test::tokio;
use solana_sdk::signer::Signer;

pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
pub const TEST_STAKE_DEPOSIT_AMOUNT: Lamports = Lamports(10_000_000_000);

#[tokio::test]
async fn test_withdrawal() {
    let (mut context, stake_accounts) = Context::new_with_two_stake_accounts().await;

    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;
    let start_epoch = epoch_schedule.first_normal_epoch;
    let slots_per_epoch = epoch_schedule.slots_per_epoch;

    let validator = context.add_validator().await;
    // Now we make a deposit, and then delegate part of it.
    context.deposit(TEST_DEPOSIT_AMOUNT).await;

    let stake_account = context
        .stake_deposit(
            validator.vote_account,
            StakeDeposit::Append,
            TEST_STAKE_DEPOSIT_AMOUNT,
        )
        .await;
    context.context.warp_to_slot(start_slot).unwrap();
}
