use anker::{error::AnkerError, token::MicroUst};
use lido::token::Lamports;
use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;
use std::mem;
use testlib::{anker_context::Context, assert_solido_error};

const DEPOSIT_AMOUNT: u64 = 1_000_000_000; // 1e9 units

#[tokio::test]
async fn test_successful_sell_rewards() {
    let mut context = Context::new_with_token_pool_rewards(Lamports(DEPOSIT_AMOUNT)).await;
    context.sell_rewards().await;

    let ust_account = context
        .solido_context
        .get_account(context.ust_reserve)
        .await;
    let ust_spl_account: spl_token::state::Account =
        spl_token::state::Account::unpack_from_slice(ust_account.data.as_slice()).unwrap();

    // Exchange rate is 12 stSol : 13 Sol
    // We have 1 stSOL, our rewards were 1 - (1 * 12/13) = 0.076923077
    // Initially there are 10 StSol and 10 UST in the AMM
    // We should get 10 - (10*10 / 10.076923077) = 0.07633587793834806 UST
    assert_eq!(ust_spl_account.amount, 76335877);
}

// Create a token pool where the token a and b are swapped (what matters is that
// they are stSOL and UST), the order shouldn't make a difference.
#[tokio::test]
async fn test_successful_sell_rewards_pool_a_b_token_swapped() {
    let mut context = Context::new().await;
    // Swap the tokens a and b on Token Swap creation.
    mem::swap(
        &mut context.token_pool_context.token_a,
        &mut context.token_pool_context.token_b,
    );
    context.initialize_token_pool().await;
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
    // Initially there are 10 StSol and 10_000 UST in the AMM
    // We should get 10000 - (10*10000 / 10.076923077) = 76.33587793834886 UST
    assert_eq!(ust_balance, MicroUst(76_335_877));

    // Test claiming the reward again fails.
    let result = context.try_sell_rewards().await;
    assert_solido_error!(result, AnkerError::ZeroRewardsToClaim);
}

#[tokio::test]
async fn test_rewards_fail_with_different_reserve() {
    let mut context = Context::new().await;
    context.initialize_token_pool().await;
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
