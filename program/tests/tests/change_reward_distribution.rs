// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

use solana_program_test::tokio;
use solana_sdk::signature::Signer;

use lido::error::LidoError;
use lido::state::{FeeRecipients, RewardDistribution};

use crate::assert_solido_error;
use crate::context::Context;

#[tokio::test]
async fn test_successful_change_reward_distribution() {
    let mut context = Context::new_with_maintainer().await;

    let solido = context.get_solido().await;
    assert_eq!(solido.reward_distribution, context.reward_distribution);
    assert_eq!(
        solido.fee_recipients.treasury_account,
        context.treasury_st_sol_account,
    );
    assert_eq!(
        solido.fee_recipients.developer_account,
        context.developer_st_sol_account,
    );

    let new_fee = RewardDistribution {
        treasury_fee: 87,
        validation_fee: 44,
        developer_fee: 54,
        st_sol_appreciation: 122,
    };

    let new_treasury_owner = context.deterministic_keypair.new_keypair();
    let new_treasury_addr = context
        .create_st_sol_account(new_treasury_owner.pubkey())
        .await;

    let new_developer_owner = context.deterministic_keypair.new_keypair();
    let new_developer_addr = context
        .create_st_sol_account(new_developer_owner.pubkey())
        .await;

    context
        .try_change_reward_distribution(
            &new_fee,
            &FeeRecipients {
                developer_account: new_developer_addr,
                treasury_account: new_treasury_addr,
            },
        )
        .await
        .expect("Failed to change fees.");

    let solido = context.get_solido().await;
    assert_eq!(solido.reward_distribution, new_fee);
    assert_eq!(solido.fee_recipients.treasury_account, new_treasury_addr,);
    assert_eq!(solido.fee_recipients.developer_account, new_developer_addr,);
}

#[tokio::test]
async fn test_change_reward_distribution_wrong_minter() {
    let mut context = Context::new_with_maintainer().await;

    let wrong_mint_authority = context.deterministic_keypair.new_keypair();
    let wrong_mint = context.create_mint(wrong_mint_authority.pubkey()).await;

    // Create an SPL token account that is not stSOL.
    context.st_sol_mint = wrong_mint;
    let not_st_sol_owner = context.deterministic_keypair.new_keypair();
    let not_st_sol_account = context
        .create_st_sol_account(not_st_sol_owner.pubkey())
        .await;

    let solido = context.get_solido().await;

    let result = context
        .try_change_reward_distribution(
            &solido.reward_distribution,
            &FeeRecipients {
                developer_account: not_st_sol_account,
                ..solido.fee_recipients
            },
        )
        .await;
    assert_solido_error!(result, LidoError::InvalidMint);

    let result = context
        .try_change_reward_distribution(
            &solido.reward_distribution,
            &FeeRecipients {
                treasury_account: not_st_sol_account,
                ..solido.fee_recipients
            },
        )
        .await;
    assert_solido_error!(result, LidoError::InvalidFeeRecipient);
}
