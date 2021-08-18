// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use crate::context::{Context, StakeDeposit};
use lido::{state::Validator, token::Lamports};
use solana_program_test::tokio;
use solana_sdk::signer::Signer;
const STAKE_AMOUNT: Lamports = Lamports(10_000_000_000);

#[tokio::test]
async fn test_successful_unstake() {
    let mut context = Context::new_with_maintainer_and_validator().await;
    context.deposit(STAKE_AMOUNT).await;
    let validator = context.validator.take().unwrap();
    context
        .stake_deposit(validator.vote_account, StakeDeposit::Append, STAKE_AMOUNT)
        .await;
    context.validator = Some(validator);

    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;

    context.context.warp_to_slot(start_slot).unwrap();
    context.update_exchange_rate().await;

    let unstake_lamports = Lamports(1_000_000_000);
    context.unstake(unstake_lamports).await;
}

#[tokio::test]
async fn test_unstake_with_funded_destination_stake() {
    let mut context = Context::new_with_maintainer_and_validator().await;
    context.deposit(STAKE_AMOUNT).await;
    let validator = context.validator.take().unwrap();
    context
        .stake_deposit(validator.vote_account, StakeDeposit::Append, STAKE_AMOUNT)
        .await;
    context.validator = Some(validator);

    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;

    context.context.warp_to_slot(start_slot).unwrap();
    context.update_exchange_rate().await;

    let validator = &context.get_solido().await.validators.entries[0];
    let (unstake_address, _) = Validator::find_unstake_account_address(
        &crate::context::id(),
        &context.solido.pubkey(),
        &validator.pubkey,
        0,
    );
    context.fund(unstake_address, Lamports(500_000_000)).await;
    let unstake_lamports = Lamports(1_000_000_000);
    context.unstake(unstake_lamports).await;
}
