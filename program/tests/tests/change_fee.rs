#![cfg(feature = "test-bpf")]

use solana_program_test::tokio;
use solana_sdk::signature::{Keypair, Signer};

use lido::state::{FeeDistribution, FeeRecipients};
use lido::error::LidoError;

use crate::assert_solido_error;
use crate::context::Context;

#[tokio::test]
async fn test_successful_change_fee() {
    let mut context = Context::new_with_maintainer().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.fee_distribution, context.fee_distribution);
    assert_eq!(
        solido.fee_recipients.treasury_account,
        context.treasury_st_sol_account,
    );
    assert_eq!(
        solido.fee_recipients.developer_account,
        context.developer_st_sol_account,
    );

    let new_fee = FeeDistribution {
        treasury_fee: 87,
        validation_fee: 44,
        developer_fee: 54,
    };

    let new_treasury_owner = Keypair::new();
    let new_treasury_addr = context.create_st_sol_account(new_treasury_owner.pubkey()).await;

    let new_developer_owner = Keypair::new();
    let new_developer_addr = context.create_st_sol_account(new_developer_owner.pubkey()).await;

    context.try_change_fee_distribution(
        &new_fee,
        &FeeRecipients {
            developer_account: new_developer_addr,
            treasury_account: new_treasury_addr,
        },
    ).await
        .expect("Failed to change fees.");

    let solido = context.get_solido().await;
    assert_eq!(solido.fee_distribution, new_fee);
    assert_eq!(
        solido.fee_recipients.treasury_account,
        new_treasury_addr,
    );
    assert_eq!(
        solido.fee_recipients.developer_account,
        new_developer_addr,
    );
}

#[tokio::test]
async fn test_change_fee_wrong_minter() {
    let mut context = Context::new_with_maintainer().await;

    let wrong_mint_authority = Keypair::new();
    let wrong_mint = context.create_mint(wrong_mint_authority.pubkey()).await;

    // Create an SPL token account that is not stSOL.
    context.st_sol_mint = wrong_mint;
    let not_st_sol_owner = Keypair::new();
    let not_st_sol_account = context.create_st_sol_account(not_st_sol_owner.pubkey()).await;

    let solido = context.get_solido().await;

    let result = context.try_change_fee_distribution(
        &solido.fee_distribution,
        &FeeRecipients {
            developer_account: not_st_sol_account,
            .. solido.fee_recipients
        },
    ).await;
    assert_solido_error!(result, LidoError::InvalidFeeRecipient);

    let result = context.try_change_fee_distribution(
        &solido.fee_distribution,
        &FeeRecipients {
            treasury_account: not_st_sol_account,
            .. solido.fee_recipients
        },
    ).await;
    assert_solido_error!(result, LidoError::InvalidFeeRecipient);
}
