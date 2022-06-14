use solana_program_test::tokio;

use testlib::solido_context::Context;

#[tokio::test]
async fn test_set_max_validation_fee() {
    let mut context = Context::new_with_maintainer_and_validator().await;
    let validator = &context.get_solido().await.validators.entries[0];

    // increase max_validation_fee
    let result = context.try_set_max_validation_fee(context.max_validation_fee + 1);
    assert_eq!(result.await.is_ok(), true);

    let solido = context.get_solido().await;
    assert_eq!(solido.max_validation_fee, context.max_validation_fee + 1);

    let result = context.try_deactivate_validator_if_commission_exceeds_max(validator.pubkey);
    assert_eq!(result.await.is_ok(), true);

    // check validator is not deactivated
    let validator = &context.get_solido().await.validators.entries[0];
    assert_eq!(validator.entry.active, true);

    // increase max_validation_fee abouve 100%
    assert_eq!(context.try_set_max_validation_fee(101).await.is_err(), true);

    // decrease max_validation_fee
    let result = context.try_set_max_validation_fee(context.max_validation_fee - 1);
    assert_eq!(result.await.is_ok(), true);

    let result = context.try_deactivate_validator_if_commission_exceeds_max(validator.pubkey);
    assert_eq!(result.await.is_ok(), true);

    // check validator is deactivated
    let validator = &context.get_solido().await.validators.entries[0];
    assert_eq!(validator.entry.active, false);
}
