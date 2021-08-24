// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use crate::assert_solido_error;
use crate::context::{Context, StakeDeposit};
use lido::MINIMUM_STAKE_ACCOUNT_BALANCE;
use lido::{error::LidoError, token::Lamports};
use solana_program::stake::state::StakeState;
use solana_program_test::tokio;
use solana_sdk::signer::Signer;
const STAKE_AMOUNT: Lamports = Lamports(10_000_000_000);

impl UnstakeContext {
    async fn new(stake_amount: Lamports) -> Context {
        let mut context = Context::new_with_maintainer_and_validator().await;
        context.deposit(stake_amount).await;
        let validator = context.validator.take().unwrap();
        context
            .stake_deposit(validator.vote_account, StakeDeposit::Append, STAKE_AMOUNT)
            .await;
        context.validator = Some(validator);

        let epoch_schedule = context.context.genesis_config().epoch_schedule;
        let start_slot = epoch_schedule.first_normal_slot;

        context.context.warp_to_slot(start_slot).unwrap();
        context.update_exchange_rate().await;

        context
    }
}

#[tokio::test]
async fn test_successful_unstake() {
    let mut context = UnstakeContext::new(STAKE_AMOUNT).await;
    let unstake_lamports = Lamports(1_000_000_000);

    let solido = context.context.get_solido().await;
    let validator = &solido.validators.entries[0];

    let stake_account_before = context
        .context
        .get_stake_account_from_seed(&validator, 0)
        .await;
    context.context.unstake(unstake_lamports).await;
    let stake_account_after = context
        .context
        .get_stake_account_from_seed(&validator, 0)
        .await;
    assert_eq!(
        (stake_account_before.balance.total() - stake_account_after.balance.total()).unwrap(),
        unstake_lamports
    );
    let unstake_account = context
        .context
        .get_unstake_account_from_seed(&validator, 0)
        .await;

    let rent = context.context.get_rent().await;
    let stake_rent = rent.minimum_balance(std::mem::size_of::<StakeState>());
    // The rent will not become deactivated.
    assert_eq!(
        unstake_account.balance.deactivating,
        (unstake_lamports - Lamports(stake_rent)).unwrap()
    );
}

#[tokio::test]
async fn test_unstake_balance_combinations() {
    let mut context = UnstakeContext::new(STAKE_AMOUNT).await;
    let result = context.context.try_unstake(STAKE_AMOUNT).await;
    // Should fail, because the validator will have less than the minimum.
    assert_solido_error!(result, LidoError::InvalidAmount);
    // Should fail, because we tried to unstake more than the validator has.
    let result = context
        .context
        .try_unstake((STAKE_AMOUNT + Lamports(1)).unwrap())
        .await;
    assert!(result.is_err()); // Insufficient funds error from Stake Program.

    // Unstaking less than the rent should fail
    let rent = context.context.get_rent().await;
    let stake_rent = rent.minimum_balance(std::mem::size_of::<StakeState>());
    let result = context.context.try_unstake(Lamports(stake_rent - 1)).await;
    assert!(result.is_err());

    // If we unstake so that the validator still has the minimum, it should work.
    context
        .context
        .unstake((STAKE_AMOUNT - MINIMUM_STAKE_ACCOUNT_BALANCE).unwrap())
        .await;
}

#[tokio::test]
async fn test_unstake_with_funded_destination_stake() {
    let mut context = UnstakeContext::new(STAKE_AMOUNT).await;
    let validator = &context.context.get_solido().await.validators.entries[0];
    let (unstake_address, _) = validator.find_unstake_account_address(
        &crate::context::id(),
        &context.context.solido.pubkey(),
        0,
    );
    context
        .context
        .fund(unstake_address, Lamports(500_000_000))
        .await;
    let unstake_lamports = Lamports(1_000_000_000);

    context.context.unstake(unstake_lamports).await;
    let unstake_account = context
        .context
        .get_unstake_account_from_seed(&validator, 0)
        .await;
    // Since we already had something in the account that paid for the rent, we
    // can unstake all the requested amount.
    assert_eq!(unstake_account.balance.deactivating, unstake_lamports);
}
