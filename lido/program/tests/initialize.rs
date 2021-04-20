mod helpers;

use helpers::{program_test, LidoAccounts};
use solana_program_test::tokio;

#[tokio::test]
async fn success_initialize() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    LidoAccounts::new()
        .initialize_lido(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    // let stake_pool_accounts = StakePoolAccounts::new();
    // stake_pool_accounts
    //     .initialize_stake_pool(&mut banks_client, &payer, &recent_blockhash)
    //     .await
    //     .unwrap();

    // // Stake pool now exists
    // let stake_pool = get_account(&mut banks_client, &stake_pool_accounts.stake_pool.pubkey()).await;
    // assert_eq!(stake_pool.data.len(), get_packed_len::<state::StakePool>());
    // assert_eq!(stake_pool.owner, id());

    // // Validator stake list storage initialized
    // let validator_list = get_account(
    //     &mut banks_client,
    //     &stake_pool_accounts.validator_list.pubkey(),
    // )
    // .await;
    // let validator_list =
    //     try_from_slice_unchecked::<state::ValidatorList>(validator_list.data.as_slice()).unwrap();
    // assert_eq!(validator_list.is_valid(), true);
}
