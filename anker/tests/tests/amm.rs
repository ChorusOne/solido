use anker::token::MicroUst;
use lido::token::{Lamports, StLamports};
use solana_program_test::tokio;
use solana_sdk::signer::Signer;
use testlib::anker_context::Context;

#[tokio::test]
async fn test_successful_token_swap() {
    let mut context = Context::new().await;
    context.initialize_token_pool().await;
    let (st_sol_keypair, st_sol_token) = context
        .solido_context
        .deposit(Lamports(10_000_000_000))
        .await;

    let ust_address = context
        .create_ust_token_account(st_sol_keypair.pubkey())
        .await;

    let amount_in = StLamports(1_000_000_000);
    let min_amount_out = MicroUst(0);
    context
        .swap_st_sol_for_ust(
            &st_sol_token,
            &ust_address,
            &st_sol_keypair,
            amount_in,
            min_amount_out,
        )
        .await;

    let ust_balance = context.get_ust_balance(ust_address).await;
    // For the constant product AMM:
    // 10000 - (10*10000 / 11) = 909.0909090909099
    assert_eq!(ust_balance, MicroUst(909_090_909));
}
