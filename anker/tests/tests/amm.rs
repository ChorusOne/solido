use lido::token::Lamports;
use solana_program::program_pack::Pack;
use solana_program_test::tokio;
use solana_sdk::signer::Signer;
use testlib::anker_context::Context;

#[tokio::test]
async fn test_successful_token_swap() {
    let mut context = Context::new_with_initialized_token_pool().await;
    let (st_sol_kp, st_sol_token) = context
        .solido_context
        .deposit(Lamports(10_000_000_000))
        .await;

    let ust_address = context
        .solido_context
        .create_spl_token_account(
            context.token_pool_context.ust_mint_address,
            st_sol_kp.pubkey(),
        )
        .await;

    context
        .swap_st_sol_for_ust(&st_sol_token, &ust_address, &st_sol_kp, 1_000_000_000, 0)
        .await;

    let ust_account = context.solido_context.get_account(ust_address).await;
    let ust_spl_account: spl_token::state::Account =
        spl_token::state::Account::unpack_from_slice(ust_account.data.as_slice()).unwrap();

    // For the constant product AMM:
    // 10 - (10*10 / 11) = 0.9090909090909083
    assert_eq!(ust_spl_account.amount, 909090909);
}
