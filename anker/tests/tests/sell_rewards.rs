use anker::{error::AnkerError, token::MicroUst};
use lido::token::{Lamports, StLamports};
use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;
use std::mem;
use testlib::{anker_context::Context, assert_solido_error};

const DEPOSIT_AMOUNT: u64 = 1_000_000_000; // 1e9 units

#[tokio::test]
async fn test_successful_sell_rewards() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(Lamports(DEPOSIT_AMOUNT))
        .await;

    let anker_before = context.get_anker().await;
    context.sell_rewards().await;
    let anker_after = context.get_anker().await;
    assert_eq!(
        anker_after.metrics.swapped_rewards_st_sol_total
            - anker_before.metrics.swapped_rewards_st_sol_total,
        Ok(StLamports(0_923_076_923))
    );
    assert_eq!(
        anker_after.metrics.swapped_rewards_ust_total
            - anker_before.metrics.swapped_rewards_ust_total,
        Ok(MicroUst(76_335_877))
    );

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
    context
        .initialize_token_pool_and_deposit(Lamports(DEPOSIT_AMOUNT))
        .await;
    context.sell_rewards().await;

    let ust_balance = context.get_ust_balance(context.ust_reserve).await;
    assert_eq!(ust_balance, MicroUst(76_335_877));
}

#[tokio::test]
async fn test_rewards_fail_with_different_reserve() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(Lamports(DEPOSIT_AMOUNT))
        .await;

    context.ust_reserve = Pubkey::new_unique();

    let result = context.try_sell_rewards().await;
    assert_solido_error!(result, AnkerError::InvalidDerivedAccount);
}
