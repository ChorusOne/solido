#![cfg(feature = "test-bpf")]

mod helpers;

use borsh::BorshDeserialize;
use helpers::{
    get_account, program_test, simple_add_validator_to_pool,
    stakepool_account::{get_token_balance, transfer, ValidatorStakeAccount},
    LidoAccounts,
};
use lido::token::{Lamports, Rational, StakePoolTokenLamports};
use solana_program::pubkey::Pubkey;
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::Signer;
use spl_stake_pool::state::StakePool;

async fn setup() -> (ProgramTestContext, LidoAccounts, Vec<ValidatorStakeAccount>) {
    let mut context = program_test().start_with_context().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
        )
        .await
        .unwrap();

    let mut stake_accounts = Vec::new();
    for _ in 0..STAKE_ACCOUNTS {
        let validator_stake_account = simple_add_validator_to_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &lido_accounts,
        )
        .await;

        stake_accounts.push(validator_stake_account);
    }
    (context, lido_accounts, stake_accounts)
}
const STAKE_ACCOUNTS: u64 = 4;
const TEST_A_DEPOSIT_AMOUNT: Lamports = Lamports(200_000_000_000);
const TEST_B_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
const EXTRA_STAKE_AMOUNT: Lamports = Lamports(50_000_000_000);

#[tokio::test]
async fn test_successful_update_balance() {
    let (mut context, lido_accounts, stake_accounts) = setup().await;

    // Make a deposit to the Lido reserve.
    lido_accounts
        .deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            TEST_A_DEPOSIT_AMOUNT,
        )
        .await;

    // Create a stake account from the now-funded Lido reserve.
    let validator_account = stake_accounts.get(0).unwrap();
    let validator_stake = lido_accounts
        .stake_deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            validator_account,
            TEST_A_DEPOSIT_AMOUNT,
        )
        .await;

    // Delegate the newly created stake account to Lido's stake pool.
    lido_accounts
        .deposit_active_stake_to_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            validator_account,
            &validator_stake,
        )
        .await;

    // Transfer EXTRA_STAKE_AMOUNT Lamports to every validator outside of Lido
    // and the stake pool, to simulate validation rewards being paid out.
    for stake_account in &stake_accounts {
        transfer(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &stake_account.stake_account,
            EXTRA_STAKE_AMOUNT,
        )
        .await;
    }

    // Warp to the next epoch, such that the next balance refresh is not a
    // no-op.
    context.warp_to_slot(50_000).unwrap();

    // Refresh Lido's view of the stake account balances, and its own balance.
    let error = lido_accounts
        .stake_pool_accounts
        .update_all(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            stake_accounts
                .iter()
                .map(|v| v.vote.pubkey())
                .collect::<Vec<Pubkey>>()
                .as_slice(),
            false,
        )
        .await;
    assert!(error.is_none());

    // Make another deposit to the Lido reserve.
    let recipient = lido_accounts
        .deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            TEST_B_DEPOSIT_AMOUNT,
        )
        .await;

    let stake_pool = get_account(
        &mut context.banks_client,
        &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool = StakePool::try_from_slice(&stake_pool.data.as_slice()).unwrap();

    // The total reward is the the sum of what each stake account received.
    let reward = (EXTRA_STAKE_AMOUNT * STAKE_ACCOUNTS).unwrap();

    // Of that reward, Lido claims a fraction as fee.
    let fee_lamports_expected = (reward
        * Rational {
            numerator: lido_accounts.stake_pool_accounts.fee.numerator,
            denominator: lido_accounts.stake_pool_accounts.fee.denominator,
        })
    .unwrap();

    // Confirm that the value of the tokens we received, is equal to the fee we
    // claimed.
    let fee_pool_tokens_received = StakePoolTokenLamports(
        get_token_balance(
            &mut context.banks_client,
            &lido_accounts.stake_pool_accounts.pool_fee_account.pubkey(),
        )
        .await,
    );
    // One Lamport is lost due to rounding down.
    assert_eq!(
        Some(Lamports(
            stake_pool
                .calc_lamports_withdraw_amount(fee_pool_tokens_received.0)
                .unwrap()
        )),
        fee_lamports_expected - Lamports(1)
    );

    assert_eq!(
        reward + TEST_A_DEPOSIT_AMOUNT,
        Some(Lamports(stake_pool.total_stake_lamports)),
    );

    let lido_tokens_for_a = StakePoolTokenLamports(
        get_token_balance(
            &mut context.banks_client,
            &lido_accounts.pool_token_to.pubkey(),
        )
        .await,
    );
    // In general, the stake pool tokens we got cannot be compared to the
    // deposit in SOL, but in this particular case, the exchange rate is 1,
    // so we expect the values to be the same.
    assert_eq!(lido_tokens_for_a.0, TEST_A_DEPOSIT_AMOUNT.0);

    // Check amount new user received
    let received_tokens_b = StakePoolTokenLamports(
        get_token_balance(&mut context.banks_client, &recipient.pubkey()).await,
    );

    // In this case the exchange rate is not 1, so we need to compensate for
    // that.
    assert_eq!(
        received_tokens_b.0,
        (TEST_B_DEPOSIT_AMOUNT
            * Rational {
                numerator: stake_pool.pool_token_supply,
                denominator: stake_pool.total_stake_lamports,
            })
        .unwrap()
        .0
    );
}
