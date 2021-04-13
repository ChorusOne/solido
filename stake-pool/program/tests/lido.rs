use spl_stake_pool::lido::processor::*;

mod helpers;

use {
    borsh::BorshSerialize,
    helpers::*,
    solana_program::{
        borsh::get_packed_len,
        hash::Hash,
        instruction::{AccountMeta, Instruction},
        program_pack::Pack,
        system_instruction, sysvar,
    },
    solana_program_test::*,
    solana_sdk::{
        instruction::InstructionError,
        signature::{Keypair, Signer},
        transaction::Transaction,
        transaction::TransactionError,
        transport::TransportError,
    },
    spl_stake_pool::{
        borsh::{get_instance_packed_len, try_from_slice_unchecked},
        error, id, instruction, state,
    },
};

#[tokio::test]
async fn lido_test() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let rent = banks_client.get_rent().await.unwrap();

    let keypairs = vec![Keypair::new(), Keypair::new(), Keypair::new()];
    let stake_pool = Keypair::new();
    let members_list = Keypair::new();
    let lido_account = Keypair::new();

    let members_list_size = get_instance_packed_len(&LidoMembers::new(100)).unwrap();
    let rent_members_list = rent.minimum_balance(members_list_size);

    // Create members contract

    let mut transaction = Transaction::new_with_payer(
        &[system_instruction::create_account(
            &payer.pubkey(),
            &members_list.pubkey(),
            rent_members_list,
            members_list_size as u64,
            &id(),
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &members_list], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let lido_size = get_instance_packed_len(&Lido::default()).unwrap();
    let rent_lido = rent.minimum_balance(lido_size);

    let mut transaction = Transaction::new_with_payer(
        &[system_instruction::create_account(
            &payer.pubkey(),
            &lido_account.pubkey(),
            rent_lido,
            lido_size as u64,
            &id(),
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &lido_account], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let accounts = vec![
        AccountMeta::new(lido_account.pubkey(), false),
        AccountMeta::new(members_list.pubkey(), false),
        AccountMeta::new_readonly(solana_program::sysvar::rent::id(), false),
        AccountMeta::new(stake_pool.pubkey(), false),
    ];

    let instruction = Instruction {
        program_id: id(),
        accounts: accounts,
        data: LidoInstruction::Initialize {
            members_list_account: keypairs.iter().map(|kp| kp.pubkey()).collect(),
            stake_pool_account: stake_pool.pubkey(),
        }
        .try_to_vec()
        .unwrap(),
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();
}
