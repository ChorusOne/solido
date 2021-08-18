// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use crate::context::{Context, StakeDeposit};
use lido::token::Lamports;
use solana_program::stake::state::StakeState;
use solana_program_test::tokio;
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

    let solido = context.get_solido().await;
    let val = &solido.validators.entries[0];
    let stake_account_before = context.get_stake_account_from_seed(&val.pubkey, 0).await;
    let unstake_lamports = Lamports(1_000_000_000);
    context.unstake(unstake_lamports).await;
    let stake_account_after = context.get_stake_account_from_seed(&val.pubkey, 0).await;

    assert_eq!(
        (stake_account_before.balance.total() - stake_account_after.balance.total()).unwrap(),
        unstake_lamports
    );

    let rent = context.get_rent().await;
    let stake_rent = rent.minimum_balance(std::mem::size_of::<StakeState>());

    let unstake_account = context.get_unstake_account_from_seed(&val.pubkey, 0).await;
    assert_eq!(
        unstake_account.balance.deactivating,
        (unstake_lamports - Lamports(stake_rent)).unwrap()
    );
}
