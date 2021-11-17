use lido::token::Lamports;
use solana_program::program_pack::Pack;
use solana_program_test::tokio;
use testlib::anker_context::Context;

const DEPOSIT_AMOUNT: u64 = 1_000_000_000; // 1e9 units

#[tokio::test]
async fn test_successful_claim_rewards() {
    let mut context = Context::new().await;
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

    context.claim_rewards().await;

    let ust_account = context
        .solido_context
        .get_account(context.ust_rewards_account)
        .await;
    let ust_spl_account: spl_token::state::Account =
        spl_token::state::Account::unpack_from_slice(ust_account.data.as_slice()).unwrap();

    // Exchange rate is 12 stSol : 13 Sol
    // We have 1 stSOL, our rewards were 1 - (1 * 12/13) = 0.076923077
    // Initially there are 10 StSol and 10 UST in the AMM
    // We should get 10 - (10*10 / 10.076923077) = 0.07633587793834806 UST
    assert_eq!(ust_spl_account.amount, 76335877);
}
