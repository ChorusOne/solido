use solana_program::pubkey::Pubkey;
use solana_program_test::processor;
use solana_program_test::*;
use solana_sdk::{signature::Signer, transaction::Transaction};
use solido::{
    entrypoint::process_instruction,
    model::ValidatorStakeList,
    state::{Fee, StakePool},
};

solana_program::declare_id!("5QuBzCtUC6pHgFEQJ5d2qX7ktyyHba9HVXLQVUEiAf7d");

#[tokio::test]
async fn process_deposit_test() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let data_amount = 500u64.to_be_bytes();
    let _stake_pool = stake_pool();
    let _validator_stake_list = validator_stake_list();

    let kine_instruction =
        solana_program::instruction::Instruction::new_with_bytes(id(), &data_amount, Vec::new());
    let tx = Transaction::new_signed_with_payer(
        &[kine_instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    assert!(banks_client.process_transaction(tx).await.is_ok());
}

pub fn program_test() -> ProgramTest {
    ProgramTest::new("solido", id(), processor!(process_instruction))
}

fn stake_pool() -> StakePool {
    StakePool {
        version: 0u8,
        owner: id(),
        deposit_bump_seed: 0u8,
        withdraw_bump_seed: 0u8,
        validator_stake_list: Pubkey::new_unique(),
        pool_mint: Pubkey::new_unique(),
        owner_fee_account: Pubkey::new_unique(),
        token_program_id: Pubkey::new_unique(),
        stake_total: 0u64,
        pool_total: 0u64,
        last_update_epoch: 0u64,
        fee: Fee {
            denominator: 0u64,
            numerator: 0u64,
        },
    }
}

fn validator_stake_list() -> ValidatorStakeList {
    ValidatorStakeList::default()
}
