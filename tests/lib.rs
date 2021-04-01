use solana_program_test::*;
use solana_program_test::processor;
use solana_program::pubkey::Pubkey;
use solido::{entrypoint::process_instruction, processor::Processor};

solana_program::declare_id!("5QuBzCtUC6pHgFEQJ5d2qX7ktyyHba9HVXLQVUEiAf7d");

#[tokio::test]
async fn process_deposit_test() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    
}


pub fn program_test() -> ProgramTest {
    ProgramTest::new("solido", id(), processor!(process_instruction))
}

// #[tokio::test]
// async fn test_create_account_transfer_sol() {
//     let fund_sda_initial_balance: u64 = 1000;
//     let (mut banks_client, payer, recent_blockhash) = program_test().start().await;

    // let eth_address = hex::decode("d8Af89d15090e23E8FBA0872C2330f1205a4068A").unwrap();

    // let (fund_sda, fund_bump_seed) =
    //     Pubkey::find_program_address(&[&eth_address, vec![0].as_slice()], &id());
    // banks_client
    //     .process_transaction(Transaction::new_signed_with_payer(
    //         &[system_instruction::transfer(
    //             &payer.pubkey(),
    //             &fund_sda,
    //             fund_sda_initial_balance,
    //         )],
    //         Some(&payer.pubkey()),
    //         &[&payer],
    //         recent_blockhash,
    //     ))
    //     .await
    //     .unwrap();

    // let recipient = Keypair::new();
    // let transfer_ins = system_instruction::transfer(&fund_sda, &recipient.pubkey(), 1);

    // let (sda, secp_instruction, serialized_command) = create_signature_instruction(
    //     eth_address,
    //     vec![transfer_ins],
    //     vec![vec![0, fund_bump_seed]],
    //     0,
    // );

    // let rent_exempt_balance = fund_rent_exempt_sda(&mut banks_client, &payer, &sda).await;

    // let account_metas = vec![
    //     AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
    //     AccountMeta::new(sda, false),
    //     AccountMeta::new_readonly(solana_sdk::sysvar::rent::id(), false),
    //     AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
    //     AccountMeta::new(recipient.pubkey(), false),
    //     AccountMeta::new(fund_sda, false),
    //     AccountMeta::new(payer.pubkey(), true),
    // ];
    // let kine_instruction = solana_program::instruction::Instruction::new_with_bytes(
//         id(),
//         serialized_command.as_slice(),
//         account_metas,
//     );
//     let tx = Transaction::new_signed_with_payer(
//         &[secp_instruction, kine_instruction],
//         Some(&payer.pubkey()),
//         &[&payer],
//         recent_blockhash,
//     );
//     assert!(banks_client.process_transaction(tx).await.is_ok());
//     assert_eq!(
//         banks_client.get_balance(sda).await.unwrap(),
//         rent_exempt_balance
//     );
//     assert_eq!(
//         1,
//         banks_client.get_balance(recipient.pubkey()).await.unwrap(),
//     );
// }
