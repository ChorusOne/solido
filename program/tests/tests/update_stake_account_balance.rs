// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use lido::error::LidoError;
use lido::state::StakeDeposit;
use lido::token::Lamports;
use lido::MINIMUM_STAKE_ACCOUNT_BALANCE;
use solana_program_test::tokio;
use testlib::assert_solido_error;
use testlib::solido_context::Context;

#[tokio::test]
async fn test_update_stake_account_balance() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;

    // If we try to withdraw initially, that should work, but there is nothing to withdraw.
    // The 2nd time it runs, should succeed, but nothing should change
    let solido_before = context.get_solido().await;
    for _ in 0..2 {
        context
            .update_stake_account_balance(validator.vote_account)
            .await;
    }
    let solido_after = context.get_solido().await;
    assert_eq!(solido_before, solido_after);

    // Deposit and stake the deposit with the validator. This creates one stake account.
    let initial_amount = MINIMUM_STAKE_ACCOUNT_BALANCE;
    context.deposit(initial_amount).await;
    let stake_account = context
        .stake_deposit(validator.vote_account, StakeDeposit::Append, initial_amount)
        .await;

    // We should be able to withdraw the inactive stake. It should be a no-op,
    // because we already knew the current validator's balance.
    let solido_before = context.get_solido().await;
    context
        .update_stake_account_balance(validator.vote_account)
        .await;
    let solido_after = context.get_solido().await;
    assert_eq!(solido_before, solido_after);

    // Skip ahead a number of epochs.
    context.advance_to_normal_epoch(0);

    // So after we update the exchange rate, we should be allowed to withdraw the inactive stake.
    context.update_exchange_rate().await;
    context
        .update_stake_account_balance(validator.vote_account)
        .await;

    // Create a second deposit and stake account, so we also test that
    // `WithdrawInactiveStake` works when multiple stake accounts are
    // involved.
    let extra_amount = Lamports(150_000_000_000);
    context.deposit(extra_amount).await;
    let stake_account_2 = context
        .stake_deposit(validator.vote_account, StakeDeposit::Append, extra_amount)
        .await;

    // Donate into both stake accounts, so we have some change to observe.
    let donation = Lamports(100);
    context.fund(stake_account, donation).await;
    context.fund(stake_account_2, donation).await;

    let reserve_before = context.get_sol_balance(context.reserve_address).await;
    context
        .update_stake_account_balance(validator.vote_account)
        .await;
    let reserve_after = context.get_sol_balance(context.reserve_address).await;

    // The donation should have been withdrawn back to the reserve.
    let increase = (reserve_after - reserve_before).unwrap();
    assert_eq!(increase, (donation * 2).unwrap());

    // =============== fee distribution ===============

    // Increment the vote account credits, to simulate the validator voting in
    // this epoch, which means it will receive rewards at the start of the next
    // epoch. The number of votes is not relevant, as long as it is positive;
    // Rewards should be observed in stake accounts.
    context
        .context
        .increment_vote_account_credits(&validator.vote_account, 1);

    // We are going to skip ahead one more epoch. The number of SOL we receive
    // is not a nice round number, so instead of hard-coding the numbers here,
    // record the change in balances, so we can perform some checks on those.
    let vote_account_before = context.get_sol_balance(validator.vote_account).await;
    let treasury_before = context
        .get_st_sol_balance(context.treasury_st_sol_account)
        .await;
    let developer_before = context
        .get_st_sol_balance(context.developer_st_sol_account)
        .await;
    let solido_before = context.get_solido().await;
    let validator_before = solido_before
        .validators
        .find(&validator.vote_account)
        .unwrap();

    let account = context.get_account(validator.vote_account).await;
    let vote_account_rent = Lamports(context.get_rent().await.minimum_balance(account.data.len()));
    assert_eq!(vote_account_before, vote_account_rent);

    context.advance_to_normal_epoch(1);

    // In this new epoch, we should not be allowed to distribute fees,
    // yet, because we haven’t updated the exchange rate yet.
    let result = context
        .try_update_stake_account_balance(validator.vote_account)
        .await;
    assert_solido_error!(result, LidoError::ExchangeRateNotUpdatedInThisEpoch);

    // The rewards received is the reward accumulated in stake accounts. The
    // number looks arbitrary, but this is the amount that the current reward
    // configuration yields, so we have to deal with it.
    context.update_exchange_rate().await;
    let arbitrary_rewards: u64 = 18_976_413_379;
    context
        .update_stake_account_balance(validator.vote_account)
        .await;
    let vote_account_after = context.get_sol_balance(validator.vote_account).await;
    let treasury_after = context
        .get_st_sol_balance(context.treasury_st_sol_account)
        .await;
    let developer_after = context
        .get_st_sol_balance(context.developer_st_sol_account)
        .await;
    let solido_after = context.get_solido().await;
    let validator_after = solido_after
        .validators
        .find(&validator.vote_account)
        .unwrap();

    let rewards = (validator_after.stake_accounts_balance
        - validator_before.stake_accounts_balance)
        .expect("Does not underflow, because we received rewards.");
    assert_eq!(rewards, Lamports(arbitrary_rewards));

    let validation_commission = (vote_account_after - vote_account_before).unwrap();
    // validation commission is 5% of total rewards, solido_rewards is 95% of total rewards
    assert_eq!(
        validation_commission,
        Lamports((5.0 * (rewards.0 as f64) / 95.0) as u64)
    );

    // The treasury balance increase, when converted back to SOL, should be equal
    // to 3% of the rewards. Three lamports differ due to rounding errors.
    let treasury_fee = (treasury_after - treasury_before).unwrap();
    let treasury_fee_sol = solido_after
        .lido
        .exchange_rate
        .exchange_st_sol(treasury_fee)
        .unwrap();
    assert_eq!(treasury_fee_sol, Lamports(rewards.0 * 3 / 100 - 1));

    // The developer balance increase, when converted back to SOL, should be equal
    // to 2% of the rewards. Two lamport differ due to rounding errors.
    let developer_fee = (developer_after - developer_before).unwrap();
    let developer_fee_sol = solido_after
        .lido
        .exchange_rate
        .exchange_st_sol(developer_fee)
        .unwrap();
    assert_eq!(developer_fee_sol, Lamports(rewards.0 * 2 / 100 - 1));
}
