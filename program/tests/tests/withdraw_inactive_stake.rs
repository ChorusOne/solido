#![cfg(feature = "test-bpf")]

use crate::context::{Context, StakeDeposit};
use lido::token::Lamports;
use solana_program_test::tokio;

#[tokio::test]
async fn test_withdraw_inactive_stake() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;

    // If we try to withdraw initially, that should work, but there is nothing to withdraw.
    // The 2nd time it runs, should succeed, but nothing should change
    let solido_before = context.get_solido().await;
    for _ in 0..2 {
        context
            .withdraw_inactive_stake(validator.vote_account)
            .await;
    }
    let solido_after = context.get_solido().await;
    assert_eq!(solido_before, solido_after);

    // Deposit and stake the deposit with the validator. This creates one stake account.
    let initial_amount = Lamports(1_000_000_000);
    context.deposit(initial_amount).await;
    let stake_account = context
        .stake_deposit(validator.vote_account, StakeDeposit::Append, initial_amount)
        .await;

    // We should be able to withdraw the inactive stake. It should be a no-op,
    // because we already knew the current validator's balance.
    let solido_before = context.get_solido().await;
    context
        .withdraw_inactive_stake(validator.vote_account)
        .await;
    let solido_after = context.get_solido().await;
    assert_eq!(solido_before, solido_after);

    // Skip ahead a number of epochs.
    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;
    context.context.warp_to_slot(start_slot).unwrap();

    // So after we update the exchange rate, we should be allowed to withdraw the inactive stake.
    context.update_exchange_rate().await;
    context
        .withdraw_inactive_stake(validator.vote_account)
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
        .withdraw_inactive_stake(validator.vote_account)
        .await;
    let reserve_after = context.get_sol_balance(context.reserve_address).await;

    // The donation should have been withdrawn back to the reserve.
    let increase = (reserve_after - reserve_before).unwrap();
    assert_eq!(increase, (donation * 2).unwrap());
}
