#![cfg(feature = "test-bpf")]

use crate::assert_solido_error;
use crate::context::{Context, StakeDeposit};

use lido::error::LidoError;
use lido::token::{Lamports, StLamports};

use solana_program_test::tokio;

#[tokio::test]
async fn test_collect_validator_fee() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;

    // If we try to collect fee initially, that should work, but there is nothing to update.
    let solido_before = context.get_solido().await;
    context.collect_validator_fee(validator.vote_account).await;
    let solido_after = context.get_solido().await;
    assert_eq!(solido_before, solido_after);

    // Deposit and stake the deposit with the validator. This creates one stake account.
    let initial_amount = Lamports(1_000_000_000);
    context.deposit(initial_amount).await;
    context
        .stake_deposit(validator.vote_account, StakeDeposit::Append, initial_amount)
        .await;

    // We should be able to collect validator's fees. It should be a no-op,
    // because there were no rewards.
    let fees = context.collect_validator_fee(validator.vote_account).await;
    assert_eq!(fees, Lamports(0));

    // Skip ahead a number of epochs.
    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;
    let slots_per_epoch = epoch_schedule.slots_per_epoch;
    context.context.warp_to_slot(start_slot).unwrap();

    // In this new epoch, we should not be allowed to collect the validator's fee,
    // yet, because we haven’t updated the exchange rate yet.
    let result = context
        .try_collect_validator_fee(validator.vote_account)
        .await;
    assert_solido_error!(result, LidoError::ExchangeRateNotUpdatedInThisEpoch);

    // So after we update the exchange rate, we should be allowed to collect the validator's fee,
    context.update_exchange_rate().await;
    let fees = context.collect_validator_fee(validator.vote_account).await;
    assert_eq!(fees, Lamports(0));

    // Increment the vote account credits, to simulate the validator voting in
    // this epoch, which means it will receive rewards at the start of the next
    // epoch. The number of votes is not relevant, as long as it is positive;
    // All the rewards should go to the vote account.
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
    let validator_before = solido_before.validators.entries[0].entry.fee_credit;

    context
        .context
        .warp_to_slot(start_slot + slots_per_epoch)
        .unwrap();
    let account = context.get_account(validator.vote_account).await;
    let vote_account_rent = Lamports(context.get_rent().await.minimum_balance(account.data.len()));
    assert_eq!(vote_account_before, vote_account_rent);

    // The rewards received is the reward accumulated in the vote account. The
    // number looks arbitrary, but this is the amount that the current reward
    // configuration yields, so we have to deal with it.
    context.update_exchange_rate().await;
    let rewards = context.collect_validator_fee(validator.vote_account).await;
    assert_eq!(rewards, Lamports(1246_030_107_210));
    let treasury_after = context
        .get_st_sol_balance(context.treasury_st_sol_account)
        .await;
    let developer_after = context
        .get_st_sol_balance(context.developer_st_sol_account)
        .await;
    let solido_after = context.get_solido().await;
    let validator_after = solido_after.validators.entries[0].entry.fee_credit;

    // The treasury balance increase, when converted back to SOL, should be equal
    // to 3% of the rewards.
    let treasury_fee = (treasury_after - treasury_before).unwrap();
    let treasury_fee_sol = solido_after
        .exchange_rate
        .exchange_st_sol(treasury_fee)
        .unwrap();
    assert_eq!(treasury_fee_sol, Lamports(rewards.0 / 100 * 3));

    // The developer balance increase, when converted back to SOL, should be equal
    // to 2% of the rewards.
    let developer_fee = (developer_after - developer_before).unwrap();
    let developer_fee_sol = solido_after
        .exchange_rate
        .exchange_st_sol(developer_fee)
        .unwrap();
    assert_eq!(developer_fee_sol, Lamports(rewards.0 / 100 * 2));

    // The validator balance increase, when converted back to SOL, should be equal
    // to 5% of the rewards.
    let validator_fee = (validator_after - validator_before).unwrap();
    let validator_fee_sol = solido_after
        .exchange_rate
        .exchange_st_sol(validator_fee)
        .unwrap();

    assert_eq!(validator_fee_sol, Lamports(rewards.0 / 100 * 5));

    // Claim validator fee
    let claimed_fee = context.claim_validator_fee(validator.vote_account).await;
    assert_eq!(
        claimed_fee,
        solido_after
            .exchange_rate
            .exchange_sol(validator_fee_sol)
            .unwrap()
    );
}

#[tokio::test]
async fn test_collect_validator_fee_withdraws_donations_to_the_reserve() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;

    let initial_amount = Lamports(2_000_000_000);
    context.deposit(initial_amount).await;

    // Donate to the vote account.
    let donation = Lamports(100_000);
    context.fund(validator.vote_account, donation).await;

    let reserve_before = context.get_sol_balance(context.reserve_address).await;
    let treasury_before = context
        .get_st_sol_balance(context.treasury_st_sol_account)
        .await;
    let developer_before = context
        .get_st_sol_balance(context.developer_st_sol_account)
        .await;
    let solido_before = context.get_solido().await;

    let vote_donation = context.collect_validator_fee(validator.vote_account).await;
    let reserve_after = context.get_sol_balance(context.reserve_address).await;

    // The donation should have been withdrawn back to the reserve.
    assert_eq!(reserve_after, (reserve_before + vote_donation).unwrap());
    assert_eq!(vote_donation, donation);

    let solido_after = context.get_solido().await;
    let treasury_after = context
        .get_st_sol_balance(context.treasury_st_sol_account)
        .await;
    let developer_after = context
        .get_st_sol_balance(context.developer_st_sol_account)
        .await;

    // The additional amount should have caused fees to be paid. This is still
    // the initial epoch, so the exchange rate is 1:1. The test context sets up
    // the fee to be 10%, and that 10% is split into 5% validation, 3% treasury,
    // and 2% developer.
    assert_eq!(treasury_before, StLamports(0));
    assert_eq!(developer_before, StLamports(0));
    assert_eq!(
        solido_before.validators.entries[0].entry.fee_credit,
        StLamports(0)
    );
    assert_eq!(treasury_after, StLamports(3_000));
    assert_eq!(developer_after, StLamports(2_000));
    assert_eq!(
        solido_after.validators.entries[0].entry.fee_credit,
        StLamports(5_000)
    );
}
