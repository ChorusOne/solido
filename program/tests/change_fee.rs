#![cfg(feature = "test-bpf")]

mod helpers;

use crate::helpers::create_token_account;
use helpers::{create_mint, id, program_test, LidoAccounts};
use lido::{error::LidoError, instruction, state::FeeDistribution};
use rand::{thread_rng, Rng};
use solana_program::instruction::InstructionError;
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::{Transaction, TransactionError},
    transport::TransportError,
};

async fn setup() -> (ProgramTestContext, LidoAccounts) {
    let mut context = program_test().start_with_context().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts.initialize_lido(&mut context).await;

    (context, lido_accounts)
}
pub const TEST_DEPOSIT_AMOUNT: u64 = 1000;

#[tokio::test]
async fn test_successful_change_fee() {
    let (mut context, lido_accounts) = setup().await;

    let lido = lido_accounts.get_solido(&mut context).await;
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
        let manager = context.payer.pubkey();
        create_token_account(
            &mut context,
            &k,
            &lido_accounts.st_sol_mint.pubkey(),
            &manager,
        )
        .await;
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
        Some(&context.payer.pubkey()),
    );
    transaction.sign(
        &[&context.payer, &lido_accounts.manager],
        context.last_blockhash,
    );
    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    let lido = lido_accounts.get_solido(&mut context).await;
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
    let (mut context, lido_accounts) = setup().await;
    let fee_recipient_keys = [Keypair::new(), Keypair::new()];
    let mut rng = thread_rng();
    let n: usize = rng.gen_range(0..fee_recipient_keys.len());
    let wrong_mint = Keypair::new();
    let manager = context.payer.pubkey();
    create_mint(&mut context, &wrong_mint, &manager).await;

    for (i, k) in fee_recipient_keys.iter().enumerate() {
        let minter;
        if i == n {
            minter = wrong_mint.pubkey();
        } else {
            minter = lido_accounts.st_sol_mint.pubkey();
        }
        let manager = context.payer.pubkey();
        create_token_account(&mut context, &k, &minter, &manager).await;
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
        Some(&context.payer.pubkey()),
    );
    transaction.sign(
        &[&context.payer, &lido_accounts.manager],
        context.last_blockhash,
    );
    let err = context
        .banks_client
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
