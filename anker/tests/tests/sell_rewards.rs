use anker::{error::AnkerError, token::MicroUst};
use lido::token::Lamports;
use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;
use testlib::{anker_context::Context, assert_solido_error};

const DEPOSIT_AMOUNT: u64 = 1_000_000_000; // 1e9 units

#[tokio::test]
async fn test_successful_sell_rewards() {
    let mut context = Context::new_with_initialized_token_pool().await;
    context.deposit(Lamports(DEPOSIT_AMOUNT)).await;
    // Donate something to Solido's reserve so we can see some rewards.
    context
        .solido_context
        .fund(
            context.solido_context.reserve_address,
            Lamports(DEPOSIT_AMOUNT),
        )
        .await;
    // Update the exchange rate so we see some rewards.
    context.solido_context.advance_to_normal_epoch(1);
    context.solido_context.update_exchange_rate().await;

    context.sell_rewards().await;

    let ust_balance = context.get_ust_balance(context.ust_reserve).await;
    // Exchange rate is 12 stSol : 13 Sol
    // We have 1 stSOL, our rewards were 1 - (1 * 12/13) = 0.076923077
    // Initially there are 10 StSol and 10 UST in the AMM
    // We should get 10 - (10*10 / 10.076923077) = 0.07633587793834806 UST
    assert_eq!(ust_balance, MicroUst(76335877));
}

#[tokio::test]
async fn test_rewards_fail_with_different_reserve() {
    let mut context = Context::new_with_initialized_token_pool().await;
    context.deposit(Lamports(DEPOSIT_AMOUNT)).await;
    // Donate something to Solido's reserve so we can see some rewards.
    context
        .solido_context
        .fund(
            context.solido_context.reserve_address,
            Lamports(DEPOSIT_AMOUNT),
        )
        .await;
    // Update the exchange rate so we see some rewards.
    context.solido_context.advance_to_normal_epoch(1);
    context.solido_context.update_exchange_rate().await;

    context.ust_reserve = Pubkey::new_unique();

    let result = context.try_sell_rewards().await;
    assert_solido_error!(result, AnkerError::InvalidDerivedAccount);
}
