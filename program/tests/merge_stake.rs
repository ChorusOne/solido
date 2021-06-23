#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{
    get_account, program_test, simple_add_validator_to_pool, LidoAccounts, ValidatorAccounts,
};
use solana_program::{borsh::try_from_slice_unchecked, instruction::InstructionError};
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::{signature::Signer, transaction::TransactionError, transport::TransportError};

use lido::{error::LidoError, state::Lido, token::Lamports};
use spl_stake_pool::stake_program;

async fn setup() -> (ProgramTestContext, LidoAccounts, Vec<ValidatorAccounts>) {
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

    let mut validator_accounts = Vec::new();
    for _ in 0..NUMBER_VALIDATORS {
        let accounts = simple_add_validator_to_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &lido_accounts,
        )
        .await;

        validator_accounts.push(accounts);
    }
    (context, lido_accounts, validator_accounts)
}
const NUMBER_VALIDATORS: u64 = 4;
const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);

// TODO(#226): We test only merging inactive stake accounts, test also other combinations.
#[tokio::test]
async fn test_successful_merge_stake() {
    let (mut context, lido_accounts, validators) = setup().await;

    lido_accounts
        .deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    // Delegate the deposit.
    let validator_account = &validators[0];
    let mut stake_account_pubkeys = Vec::new();
    for _ in 0..2 {
        let stake_account = lido_accounts
            .stake_deposit(
                &mut context.banks_client,
                &context.payer,
                &context.last_blockhash,
                &validator_account.vote_account.pubkey(),
                Lamports(10_000_000_000),
            )
            .await;

        stake_account_pubkeys.push(stake_account);
    }
    let mut stake_accounts_before = Vec::new();
    for stake_account in &stake_account_pubkeys {
        let account = get_account(&mut context.banks_client, stake_account).await;
        let stake_state =
            try_from_slice_unchecked::<stake_program::StakeState>(&account.data).unwrap();
        if let stake_program::StakeState::Stake(meta, stake) = stake_state {
            stake_accounts_before.push((meta, stake));
        } else {
            assert!(false, "Stake state should have been StakeState::Stake.");
        }
    }

    let solido_account = get_account(&mut context.banks_client, &lido_accounts.lido.pubkey()).await;
    let solido_before = try_from_slice_unchecked::<Lido>(solido_account.data.as_slice()).unwrap();

    lido_accounts
        .merge_stake(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &validator_account.vote_account.pubkey(),
            0,
        )
        .await
        .unwrap();

    let account = context
        .banks_client
        .get_account(stake_account_pubkeys[0])
        .await
        .unwrap();
    // This stake account shouldn't exist anymore.
    assert!(account.is_none());
    let account = get_account(&mut context.banks_client, &stake_account_pubkeys[1]).await;
    let stake_account_after =
        try_from_slice_unchecked::<stake_program::StakeState>(&account.data).unwrap();
    if let stake_program::StakeState::Stake(meta, stake) = stake_account_after {
        let sum = 20_000_000_000 - meta.rent_exempt_reserve;
        assert_eq!(
            stake.delegation.stake, sum,
            "Delegated stake should be {}, it is {} instead.",
            sum, stake.delegation.stake
        );
    } else {
        assert!(false, "Stake state should have been StakeState::Stake.");
    }

    let solido_account = get_account(&mut context.banks_client, &lido_accounts.lido.pubkey()).await;
    let solido_after = try_from_slice_unchecked::<Lido>(solido_account.data.as_slice()).unwrap();
    assert_eq!(
        solido_after
            .validators
            .get(&validator_account.vote_account.pubkey())
            .unwrap()
            .entry
            .stake_accounts_balance,
        Lamports(20_000_000_000)
    );

    let validator_before = solido_before
        .validators
        .get(&validator_account.vote_account.pubkey())
        .unwrap();
    let validator_after = solido_after
        .validators
        .get(&validator_account.vote_account.pubkey())
        .unwrap();
    assert_eq!(
        validator_after.entry.stake_accounts_seed_begin,
        validator_before.entry.stake_accounts_seed_begin + 1,
        "Validator's stake_accounts_seed_begin is {}, should be {}",
        validator_after.entry.stake_accounts_seed_begin,
        validator_before.entry.stake_accounts_seed_begin + 1,
    );
}

#[tokio::test]
async fn test_merge_validator_with_one_stake() {
    let (mut context, lido_accounts, validators) = setup().await;

    lido_accounts
        .deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    // Delegate the deposit.
    let validator_account = &validators[0];
    lido_accounts
        .stake_deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &validator_account.vote_account.pubkey(),
            Lamports(10_000_000_000),
        )
        .await;
    match lido_accounts
        .merge_stake(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &validator_account.vote_account.pubkey(),
            0,
        )
        .await
        .err()
        .unwrap()
    {
        TransportError::TransactionError(TransactionError::InstructionError(
            _,
            InstructionError::Custom(error_index),
        )) => {
            let program_error = LidoError::InvalidStakeAccount as u32;
            assert_eq!(error_index, program_error);
        }
        _ => {
            panic!("Wrong error occurs while merging stake accounts on a validator with a single stake account.")
        }
    }
}
