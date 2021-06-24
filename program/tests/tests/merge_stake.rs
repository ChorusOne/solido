#![cfg(feature = "test-bpf")]

use crate::assert_solido_error;
use crate::context::Context;
use lido::{error::LidoError, token::Lamports};
use solana_program::borsh::try_from_slice_unchecked;
use solana_program_test::tokio;
use spl_stake_pool::stake_program;
const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);

// TODO(#226): We test only merging inactive stake accounts, test also other combinations.
#[tokio::test]
async fn test_successful_merge_stake() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;
    context.deposit(TEST_DEPOSIT_AMOUNT).await;

    let mut stake_account_pubkeys = Vec::new();
    for _ in 0..2 {
        let stake_account = context
            .stake_deposit(validator.vote_account, Lamports(10_000_000_000))
            .await;

        stake_account_pubkeys.push(stake_account);
    }

    let mut stake_accounts_before = Vec::new();
    for stake_account in &stake_account_pubkeys {
        let account = context.get_account(*stake_account).await;
        let stake_state =
            try_from_slice_unchecked::<stake_program::StakeState>(&account.data).unwrap();
        if let stake_program::StakeState::Stake(meta, stake) = stake_state {
            stake_accounts_before.push((meta, stake));
        } else {
            assert!(false, "Stake state should have been StakeState::Stake.");
        }
    }

    let solido_before = context.get_solido().await;

    context.merge_stake(validator.vote_account, 0, 1).await;

    let account = context.try_get_account(stake_account_pubkeys[0]).await;
    assert!(account.is_none());
    let account = context.get_account(stake_account_pubkeys[1]).await;
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

    let solido_after = context.get_solido().await;
    assert_eq!(
        solido_after
            .validators
            .get(&validator.vote_account)
            .unwrap()
            .entry
            .stake_accounts_balance,
        Lamports(20_000_000_000)
    );

    let validator_before = solido_before
        .validators
        .get(&validator.vote_account)
        .unwrap();
    let validator_after = solido_after
        .validators
        .get(&validator.vote_account)
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
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;
    context.deposit(TEST_DEPOSIT_AMOUNT).await;

    context
        .stake_deposit(validator.vote_account, TEST_DEPOSIT_AMOUNT)
        .await;
    let result = context.try_merge_stake(validator.vote_account, 0, 1).await;
    assert_solido_error!(result, LidoError::InvalidStakeAccount);
}
