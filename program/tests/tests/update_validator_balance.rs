#![cfg(feature = "test-bpf")]

use crate::context::Context;

use lido::token::{StLamports, Lamports};
use solana_program_test::tokio;

#[tokio::test]
async fn test_successful_fee_distribution() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;

    // If we try to update initially, that should work, but there is nothing to update.
    let solido_before = context.get_solido().await;
    context.update_validator_balance(validator.vote_account).await;
    let solido_after = context.get_solido().await;
    assert_eq!(solido_before, solido_after);

    // Deposit and stake the deposit with the validator. This creates one stake account.
    let initial_amount = Lamports(1_000_000_000);
    context.deposit(initial_amount).await;
    let stake_account = context.stake_deposit(validator.vote_account, initial_amount).await;

    // We should be able to update the validator balance. It should be a no-op,
    // because we already knew the current validator's balance.
    let solido_before = context.get_solido().await;
    context.update_validator_balance(validator.vote_account).await;
    let solido_after = context.get_solido().await;
    assert_eq!(solido_before, solido_after);

    // Put additional SOL into the stake account, outside of Solido.
    let donation = Lamports(100_000);
    context.fund(stake_account, donation).await;

    // If we now update the validator balance, we *should* see changes.
    let treasury_before = context.get_st_sol_balance(context.treasury_st_sol_account).await;
    let developer_before = context.get_st_sol_balance(context.developer_st_sol_account).await;
    let solido_before = context.get_solido().await;

    context.update_validator_balance(validator.vote_account).await;
    let solido_after = context.get_solido().await;
    let treasury_after = context.get_st_sol_balance(context.treasury_st_sol_account).await;
    let developer_after = context.get_st_sol_balance(context.developer_st_sol_account).await;

    // For one, we expect the balance to be updated.
    assert_eq!(solido_before.validators.entries[0].entry.stake_accounts_balance, initial_amount);
    assert_eq!(
        solido_after.validators.entries[0].entry.stake_accounts_balance,
        (initial_amount + donation).unwrap(),
    );

    // Aside from that, the additional amount should have caused fees to be paid.
    // This is still the initial epoch, so the exchange rate is 1:1.
    // The test context sets up the fee to be 10%, and that 10% is split into
    // 5% validation, 3% treasury, and 2% developer.
    assert_eq!(treasury_before, StLamports(0));
    assert_eq!(developer_before, StLamports(0));
    assert_eq!(solido_before.validators.entries[0].entry.fee_credit, StLamports(0));
    assert_eq!(treasury_after, StLamports(3_000));
    assert_eq!(developer_after, StLamports(2_000));
    assert_eq!(solido_after.validators.entries[0].entry.fee_credit, StLamports(5_000));
}
