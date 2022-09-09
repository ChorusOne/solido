// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use bincode::deserialize;
use solana_program::stake::state::StakeState;
use solana_program_test::tokio;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transport;

use lido::{
    error::LidoError,
    state::StakeDeposit,
    token::{Lamports, StLamports},
    MINIMUM_STAKE_ACCOUNT_BALANCE,
};
use testlib::{
    assert_solido_error,
    solido_context::{send_transaction, Context},
};

/// Shared context for tests where a given amount has been deposited and staked.
struct WithdrawContext {
    context: Context,
    /// User who deposited initially.
    user: Keypair,
    /// The user's stSOL account.
    token_addr: Pubkey,
    /// Stake account for the staked deposit.
    stake_account: Pubkey,
}

impl WithdrawContext {
    async fn new(stake_amount: Lamports) -> WithdrawContext {
        let mut context = Context::new_with_maintainer_and_validator().await;

        let (user, token_addr) = context.deposit(stake_amount).await;
        let validator = context.validator.take().unwrap();
        let stake_account = context
            .stake_deposit(validator.vote_account, StakeDeposit::Append, stake_amount)
            .await;
        context.validator = Some(validator);

        context.advance_to_normal_epoch(0);
        context.update_exchange_rate().await;

        WithdrawContext {
            context,
            user,
            token_addr,
            stake_account,
        }
    }

    async fn try_withdraw(&mut self, amount: StLamports) -> transport::Result<Pubkey> {
        let vote_account = self.context.validator.as_ref().unwrap().vote_account;
        self.context
            .try_withdraw(
                &self.user,
                self.token_addr,
                amount,
                vote_account,
                self.stake_account,
            )
            .await
    }
}

#[tokio::test]
async fn test_withdraw_less_than_rent_fails() {
    let mut context = WithdrawContext::new((MINIMUM_STAKE_ACCOUNT_BALANCE * 2).unwrap()).await;

    let rent = context.context.get_rent().await;
    let stake_state_size = std::mem::size_of::<StakeState>();
    let minimum_rent = rent.minimum_balance(stake_state_size);

    // Test withdrawing 1 Lamport less than the minimum rent. Should fail.
    let result = context.try_withdraw(StLamports(minimum_rent - 1)).await;
    assert!(result.is_err());

    // The stake program requires one more lamport than the rent-exempt amount
    // for succesful withdrawals.
    let result = context.try_withdraw(StLamports(minimum_rent + 1)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_withdraw_from_inactive_validator() {
    let mut context = WithdrawContext::new((MINIMUM_STAKE_ACCOUNT_BALANCE * 2).unwrap()).await;

    let validator = context.context.validator.as_ref().unwrap();
    let vote_account = validator.vote_account.clone();
    context.context.deactivate_validator(vote_account).await;

    let result = context.try_withdraw(StLamports(MINIMUM_STAKE_ACCOUNT_BALANCE.0 - 1));
    assert!(result.await.is_ok());
}

#[tokio::test]
async fn test_withdraw_beyond_min_balance_fails() {
    let mut context = WithdrawContext::new((MINIMUM_STAKE_ACCOUNT_BALANCE * 2).unwrap()).await;

    // Test leaving less than the minimum amount in the stake account.
    let result = context
        .try_withdraw(StLamports(MINIMUM_STAKE_ACCOUNT_BALANCE.0 + 1))
        .await;
    assert_solido_error!(result, LidoError::InvalidAmount);

    // But leaving exactly the minimum should work.
    let result = context
        .try_withdraw(StLamports(MINIMUM_STAKE_ACCOUNT_BALANCE.0))
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_withdraw_beyond_10_percent_fails() {
    let mut context = WithdrawContext::new(Lamports(LAMPORTS_PER_SOL * 1000)).await;

    // We can withdraw at most 10% of the balance + 10 SOL.
    let max_withdraw = Lamports(LAMPORTS_PER_SOL * 110);

    // Withdrawing more should fail.
    let result = context.try_withdraw(StLamports(max_withdraw.0 + 1)).await;
    assert_solido_error!(result, LidoError::InvalidAmount);

    // But exactly the max should work.
    let result = context.try_withdraw(StLamports(max_withdraw.0)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_withdraw_underflow() {
    let amount = Lamports(LAMPORTS_PER_SOL * 5);
    let mut context = WithdrawContext::new(amount).await;

    // If we try to withdraw more than the stake account balance, that should
    // report an underflow.
    let result = context.try_withdraw(StLamports(amount.0 + 1)).await;
    assert_solido_error!(result, LidoError::CalculationFailure);
}

#[tokio::test]
async fn test_withdrawal_result() {
    let amount = Lamports(100_000_000_000);
    let mut context = WithdrawContext::new(amount).await;

    let stake_account_balance_before = context.context.get_sol_balance(context.stake_account).await;

    let rent = context.context.get_rent().await;
    let stake_state_size = std::mem::size_of::<StakeState>();
    let minimum_rent = rent.minimum_balance(stake_state_size);

    let test_withdraw_amount = StLamports(minimum_rent + 1);
    // `minimum_rent + 1` is needed by the stake program during the split.
    // This should return an activated stake account with `minimum_rent + 1` Sol.
    let split_stake_account = context.try_withdraw(test_withdraw_amount).await.unwrap();

    let split_stake_sol_balance = context.context.get_sol_balance(split_stake_account).await;
    let solido = context.context.get_solido().await.lido;
    let amount_lamports = solido
        .exchange_rate
        .exchange_st_sol(test_withdraw_amount)
        .unwrap();

    // Amount should be the same as `minimum_rent + 1` because
    // no rewards were distributed
    assert_eq!(amount_lamports, Lamports(minimum_rent + 1));

    // Assert the new uninitialized stake account's balance is incremented by 10 Sol.
    assert_eq!(split_stake_sol_balance, amount_lamports);
    let stake_account_balance_after = context.context.get_sol_balance(context.stake_account).await;
    assert_eq!(
        (stake_account_balance_before - stake_account_balance_after).unwrap(),
        Lamports(minimum_rent + 1)
    );

    // Check that the stake was indeed withdrawn from the given stake account
    // Hard-coded the amount - rent, in case rent changes we'll know.
    assert_eq!(stake_account_balance_after, Lamports(99_997_717_119));

    // Test if we updated the metrics
    let solido_after = context.context.get_solido().await.lido;
    assert_eq!(
        solido_after.metrics.withdraw_amount.total_st_sol_amount,
        test_withdraw_amount
    );
    assert_eq!(
        solido_after.metrics.withdraw_amount.total_sol_amount,
        Lamports(test_withdraw_amount.0)
    );
    assert_eq!(solido_after.metrics.withdraw_amount.count, 1);

    // Check that the staker/withdrawer authorities are set to the user.
    let stake_data = context.context.get_account(split_stake_account).await;
    if let StakeState::Stake(meta, _stake) = deserialize::<StakeState>(&stake_data.data).unwrap() {
        assert_eq!(meta.authorized.staker, context.user.pubkey());
        assert_eq!(meta.authorized.withdrawer, context.user.pubkey());
    }

    // Try to withdraw all stake SOL to user's account. First we need to
    // deactivate the stake account
    context
        .context
        .deactivate_stake_account(split_stake_account, &context.user)
        .await;

    // Wait for the deactivation to be complete.
    let epoch_schedule = context.context.context.genesis_config().epoch_schedule;
    context
        .context
        .context
        .warp_to_slot(epoch_schedule.first_normal_slot + epoch_schedule.slots_per_epoch)
        .unwrap();

    // Withdraw from stake account.
    let withdraw_from_stake_instruction = solana_program::stake::instruction::withdraw(
        &split_stake_account,
        &context.user.pubkey(),
        &context.user.pubkey(),
        minimum_rent + 1,
        None,
    );
    send_transaction(
        &mut context.context.context,
        &[withdraw_from_stake_instruction],
        vec![&context.user],
    )
    .await
    .unwrap();
    assert_eq!(
        context.context.get_sol_balance(context.user.pubkey()).await,
        Lamports(minimum_rent + 1)
    );
}

#[tokio::test]
async fn test_withdraw_fails_if_validator_with_more_stake_exists() {
    let mut context = Context::new_with_maintainer().await;
    let validator_1 = context.add_validator().await;
    let validator_2 = context.add_validator().await;

    let (user, token_addr) = context.deposit(Lamports(100_000_000_000)).await;

    let stake_account_1 = context
        .stake_deposit(
            validator_1.vote_account,
            StakeDeposit::Append,
            Lamports(40_000_000_000),
        )
        .await;
    let stake_account_2 = context
        .stake_deposit(
            validator_2.vote_account,
            StakeDeposit::Append,
            Lamports(60_000_000_000),
        )
        .await;

    // Wait for the stake accounts to become active.
    context.advance_to_normal_epoch(0);
    context.update_exchange_rate().await;

    // We should not be allowed to withdraw from validator 1, because validator 2 has more stake.
    let split_stake_account = context
        .try_withdraw(
            &user,
            token_addr,
            StLamports(1_000_000_000),
            validator_1.vote_account,
            stake_account_1,
        )
        .await;

    assert_solido_error!(split_stake_account, LidoError::ValidatorWithMoreStakeExists);

    // But we should be allowed to withdraw from validator 2.
    context
        .withdraw(
            &user,
            token_addr,
            StLamports(1_000_000_000),
            validator_2.vote_account,
            stake_account_2,
        )
        .await;
}

#[tokio::test]
async fn test_withdraw_enforces_picking_most_stake_validator_in_presence_of_unstake_accounts() {
    let mut context = Context::new_with_maintainer().await;
    let validator_1 = context.add_validator().await;
    let validator_2 = context.add_validator().await;

    let (user, token_addr) = context.deposit(Lamports(100_000_000_000)).await;

    // Prepare two stake accounts, such that validator 1 has the most stake.
    let stake_account_1 = context
        .stake_deposit(
            validator_1.vote_account,
            StakeDeposit::Append,
            Lamports(60_000_000_000),
        )
        .await;
    let stake_account_2 = context
        .stake_deposit(
            validator_2.vote_account,
            StakeDeposit::Append,
            Lamports(40_000_000_000),
        )
        .await;

    // Wait for the stake to become active, so we can withdraw.
    context.advance_to_normal_epoch(0);
    context.update_exchange_rate().await;

    // Then unstake from validator 1. Now the effective stake is 30 SOL for validator 1,
    // and 40 SOL for validator 2, even though validator 1 has a higher stake accounts
    // balance.
    context
        .unstake(validator_1.vote_account, Lamports(30_000_000_000))
        .await;

    // Withdrawing from validator 1 should fail.
    let split_stake_account = context
        .try_withdraw(
            &user,
            token_addr,
            StLamports(1_000_000_000),
            validator_1.vote_account,
            stake_account_1,
        )
        .await;

    assert_solido_error!(split_stake_account, LidoError::ValidatorWithMoreStakeExists);

    // Withdrawing from validator 2 should succeed.
    context
        .withdraw(
            &user,
            token_addr,
            StLamports(1_000_000_000),
            validator_2.vote_account,
            stake_account_2,
        )
        .await;
}
