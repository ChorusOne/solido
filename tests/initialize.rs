mod helpers;

use helpers::*;

use solana_program::{hash::Hash, pubkey::Pubkey};
use solana_program_test::{tokio, BanksClient, ProgramTest};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use solido::{
    entrypoint::process_instruction,
    instructions::StakePoolInstruction,
    model::ValidatorStakeList,
    state::{Fee, StakePool},
};

#[tokio::test]
async fn process_deposit_test() {
    // let (mut banks_client, payer, recent_blockhash, stake_pool_accounts, validator_stake_account) =
    //     setup().await;

    // let (mut banks_client, payer, recent_blockhash, stake_pool_address, validators_list_address) =
    //     initialize_test().await;

    // println!("{} {}", stake_pool_address, validators_list_address);

    // let _stake_pool = stake_pool();
    // let _validator_stake_list = validator_stake_list();

    // let kine_instruction =
    //     solana_program::instruction::Instruction::new_with_bytes(id(), &data_amount, Vec::new());
    // let tx = Transaction::new_signed_with_payer(
    //     &[kine_instruction],
    //     Some(&payer.pubkey()),
    //     &[&payer],
    //     recent_blockhash,
    // );
    // assert!(banks_client.process_transaction(tx).await.is_ok());
}

#[tokio::test]
async fn test_initialize() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let stake_pool_accounts = StakePoolAccounts::new();
    stake_pool_accounts
        .initialize_stake_pool(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    // Stake pool now exists
    let stake_pool = get_account(&mut banks_client, &stake_pool_accounts.stake_pool.pubkey()).await;
    println!("{}", stake_pool.owner);
    assert_eq!(stake_pool.data.len(), StakePool::LEN);
    assert_eq!(stake_pool.owner, id());

    // Validator stake list storage initialized
    let validator_stake_list = get_account(
        &mut banks_client,
        &stake_pool_accounts.validator_stake_list.pubkey(),
    )
    .await;
    let validator_stake_list =
        ValidatorStakeList::deserialize(validator_stake_list.data.as_slice()).unwrap();
    assert_eq!(validator_stake_list.is_initialized(), true);
}
