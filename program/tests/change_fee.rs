#![cfg(feature = "test-bpf")]

mod helpers;

use crate::helpers::create_token_account;
use helpers::{create_mint, get_account, id, program_test, LidoAccounts};
use lido::{
    error::LidoError,
    instruction,
    state::{FeeDistribution, Lido},
};
use rand::{thread_rng, Rng};
use solana_program::{borsh::try_from_slice_unchecked, hash::Hash, instruction::InstructionError};
use solana_program_test::{tokio, BanksClient};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::{Transaction, TransactionError},
    transport::TransportError,
};

async fn setup() -> (BanksClient, Keypair, Hash, LidoAccounts) {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    (banks_client, payer, recent_blockhash, lido_accounts)
}
pub const TEST_DEPOSIT_AMOUNT: u64 = 1000;

#[tokio::test]
async fn test_successful_change_fee() {
    let (mut banks_client, payer, recent_blockhash, lido_accounts) = setup().await;

    let lido = try_from_slice_unchecked::<Lido>(
        get_account(&mut banks_client, &lido_accounts.lido.pubkey())
            .await
            .data
            .as_slice(),
    )
    .unwrap();
    assert_eq!(lido.fee_distribution, lido_accounts.fee_distribution);
    assert_eq!(
        lido.fee_recipients.treasury_account,
        lido_accounts.treasury_account.pubkey(),
    );
    assert_eq!(
        lido.fee_recipients.developer_account,
        lido_accounts.developer_account.pubkey(),
    );

    let new_fee = FeeDistribution {
        treasury_fee: 87,
        validation_fee: 44,
        developer_fee: 54,
    };

    let fee_recipient_keys = [Keypair::new(), Keypair::new(), Keypair::new()];
    for k in fee_recipient_keys.iter() {
        create_token_account(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &k,
            &lido_accounts.st_sol_mint.pubkey(),
            &payer.pubkey(),
        )
        .await
        .expect("Failed to create token account");
    }

    let mut transaction = Transaction::new_with_payer(
        &[instruction::change_fee_distribution(
            &id(),
            new_fee.clone(),
            &instruction::ChangeFeeSpecMeta {
                lido: lido_accounts.lido.pubkey(),
                manager: lido_accounts.manager.pubkey(),
                treasury_account: fee_recipient_keys[1].pubkey(),
                developer_account: fee_recipient_keys[2].pubkey(),
            },
        )
        .unwrap()],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &lido_accounts.manager], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let lido = try_from_slice_unchecked::<Lido>(
        get_account(&mut banks_client, &lido_accounts.lido.pubkey())
            .await
            .data
            .as_slice(),
    )
    .unwrap();
    assert_eq!(lido.fee_distribution, new_fee);
    assert_eq!(
        lido.fee_recipients.treasury_account,
        fee_recipient_keys[1].pubkey(),
    );
    assert_eq!(
        lido.fee_recipients.developer_account,
        fee_recipient_keys[2].pubkey(),
    );
}
#[tokio::test]
async fn test_change_fee_wrong_minter() {
    let (mut banks_client, payer, recent_blockhash, lido_accounts) = setup().await;
    let fee_recipient_keys = [Keypair::new(), Keypair::new()];
    let mut rng = thread_rng();
    let n: usize = rng.gen_range(0..fee_recipient_keys.len());
    let wrong_mint = Keypair::new();
    create_mint(
        &mut banks_client,
        &payer,
        &recent_blockhash,
        &wrong_mint,
        &payer.pubkey(),
    )
    .await
    .unwrap();

    for (i, k) in fee_recipient_keys.iter().enumerate() {
        let minter;
        if i == n {
            minter = wrong_mint.pubkey();
        } else {
            minter = lido_accounts.st_sol_mint.pubkey();
        }
        create_token_account(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &k,
            &minter,
            &payer.pubkey(),
        )
        .await
        .expect("Failed to create token account");
    }

    let mut transaction = Transaction::new_with_payer(
        &[instruction::change_fee_distribution(
            &id(),
            lido_accounts.fee_distribution.clone(),
            &instruction::ChangeFeeSpecMeta {
                lido: lido_accounts.lido.pubkey(),
                manager: lido_accounts.manager.pubkey(),
                treasury_account: fee_recipient_keys[0].pubkey(),
                developer_account: fee_recipient_keys[1].pubkey(),
            },
        )
        .unwrap()],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &lido_accounts.manager], recent_blockhash);
    let err = banks_client
        .process_transaction(transaction)
        .await
        .err()
        .unwrap();
    match err {
        TransportError::TransactionError(TransactionError::InstructionError(
            _,
            InstructionError::Custom(error_index),
        )) => {
            let program_error = LidoError::InvalidFeeRecipient as u32;
            assert_eq!(error_index, program_error);
        }
        _ => panic!("Wrong error occurs while try to set new manager without signature"),
    }
}
