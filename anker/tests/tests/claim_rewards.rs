use solana_program_test::tokio;
use testlib::anker_context::Context;

#[tokio::test]
async fn test_successful_claim_rewards() {
    let mut context = Context::new().await;

    context.claim_rewards().await;
}
