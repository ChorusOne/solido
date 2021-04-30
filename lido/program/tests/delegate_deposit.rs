mod helpers;

use bincode::deserialize;
use borsh::BorshDeserialize;
use helpers::{
    program_test,
    stakepool_account::{
        create_token_account, get_account, get_token_balance, simple_add_validator_to_pool,
        transfer, ValidatorStakeAccount,
    },
    LidoAccounts,
};
use lido::{id, instruction};
use solana_program::{borsh::try_from_slice_unchecked, hash::Hash, pubkey::Pubkey};
use solana_program_test::{tokio, BanksClient};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};

use spl_stake_pool::{
    minimum_stake_lamports, stake_program,
    state::{StakePool, ValidatorList},
};

async fn setup() -> (
    BanksClient,
    Keypair,
    Hash,
    LidoAccounts,
    Vec<ValidatorStakeAccount>,
) {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    let validator = simple_add_validator_to_pool(
        &mut banks_client,
        &payer,
        &recent_blockhash,
        &lido_accounts.stake_pool_accounts,
    )
    .await;

    (
        banks_client,
        payer,
        recent_blockhash,
        lido_accounts,
        vec![validator],
    )
}
pub const TEST_DEPOSIT_AMOUNT: u64 = 100_000_000_000;
pub const TEST_DELEGATE_DEPOSIT_AMOUNT: u64 = 10_000_000_000;

#[tokio::test]
async fn test_successful_delegate_deposit_stake_pool_deposit() {
    let (mut banks_client, payer, recent_blockhash, lido_accounts, validators) = setup().await;
    let user = Keypair::new();
    let recipient = Keypair::new();

    create_token_account(
        &mut banks_client,
        &payer,
        &recent_blockhash,
        &recipient,
        &lido_accounts.mint_program.pubkey(),
        &user.pubkey(),
    )
    .await
    .unwrap();

    transfer(
        &mut banks_client,
        &payer,
        &recent_blockhash,
        &user.pubkey(),
        TEST_DEPOSIT_AMOUNT,
    )
    .await;

    let mut transaction = Transaction::new_with_payer(
        &[instruction::deposit(
            &id(),
            &lido_accounts.lido.pubkey(),
            &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
            &lido_accounts.owner.pubkey(),
            &user.pubkey(),
            &recipient.pubkey(),
            &lido_accounts.mint_program.pubkey(),
            &lido_accounts.reserve_authority,
            TEST_DEPOSIT_AMOUNT,
        )
        .unwrap()],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &user], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    // Delegate the deposit
    let validator_account = validators.get(0).unwrap();

    let (stake_account, _) =
        Pubkey::find_program_address(&[&validator_account.vote.pubkey().to_bytes()[..32]], &id());

    let mut transaction = Transaction::new_with_payer(
        &[instruction::delegate_deposit(
            &id(),
            &lido_accounts.lido.pubkey(),
            &validator_account.vote.pubkey(),
            &lido_accounts.reserve_authority,
            &stake_account,
            &lido_accounts.deposit_authority,
            TEST_DELEGATE_DEPOSIT_AMOUNT,
        )
        .unwrap()],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let stake_pool_before = get_account(
        &mut banks_client,
        &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool_before = StakePool::try_from_slice(&stake_pool_before.data.as_slice()).unwrap();

    let validator_list = get_account(
        &mut banks_client,
        &lido_accounts.stake_pool_accounts.validator_list.pubkey(),
    )
    .await;

    let validator_list =
        try_from_slice_unchecked::<ValidatorList>(validator_list.data.as_slice()).unwrap();
    let validator_stake_item_before = validator_list
        .find(&validator_account.vote.pubkey())
        .unwrap();

    let token_pool_account = Keypair::new();
    create_token_account(
        &mut banks_client,
        &payer,
        &recent_blockhash,
        &token_pool_account,
        &lido_accounts.stake_pool_accounts.pool_mint.pubkey(),
        &lido_accounts.stake_pool_token_reserve_authority,
    )
    .await
    .unwrap();

    let mut transaction = Transaction::new_with_payer(
        &[instruction::stake_pool_delegate(
            &id(),
            &lido_accounts.lido.pubkey(),
            &validator_account.vote.pubkey(),
            &stake_account,
            &lido_accounts.deposit_authority,
            &token_pool_account.pubkey(),
            &spl_stake_pool::id(),
            &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
            &lido_accounts.stake_pool_accounts.validator_list.pubkey(),
            &lido_accounts.stake_pool_accounts.withdraw_authority,
            &validator_account.stake_account,
            &lido_accounts.stake_pool_accounts.pool_mint.pubkey(),
        )
        .unwrap()],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    // Stake pool should add its balance to the pool balance
    let stake_pool = get_account(
        &mut banks_client,
        &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool = StakePool::try_from_slice(&stake_pool.data.as_slice()).unwrap();
    assert_eq!(
        stake_pool.total_stake_lamports,
        stake_pool_before.total_stake_lamports + TEST_DELEGATE_DEPOSIT_AMOUNT
    );
    assert_eq!(
        stake_pool.pool_token_supply,
        stake_pool_before.pool_token_supply + TEST_DELEGATE_DEPOSIT_AMOUNT
    );

    // Check minted tokens
    let lido_token_balance =
        get_token_balance(&mut banks_client, &token_pool_account.pubkey()).await;
    assert_eq!(lido_token_balance, TEST_DELEGATE_DEPOSIT_AMOUNT);

    // Check balances in validator stake account list storage
    let validator_list = get_account(
        &mut banks_client,
        &lido_accounts.stake_pool_accounts.validator_list.pubkey(),
    )
    .await;
    let validator_list =
        try_from_slice_unchecked::<ValidatorList>(validator_list.data.as_slice()).unwrap();
    let validator_stake_item = validator_list
        .find(&validator_account.vote.pubkey())
        .unwrap();
    assert_eq!(
        validator_stake_item.stake_lamports,
        validator_stake_item_before.stake_lamports + TEST_DELEGATE_DEPOSIT_AMOUNT
    );

    // Check validator stake account actual SOL balance
    let validator_stake_account =
        get_account(&mut banks_client, &validator_account.stake_account).await;
    let stake_state =
        deserialize::<stake_program::StakeState>(&validator_stake_account.data).unwrap();
    let meta = stake_state.meta().unwrap();
    assert_eq!(
        validator_stake_account.lamports - minimum_stake_lamports(&meta),
        validator_stake_item.stake_lamports
    );
}

#[tokio::test]
async fn test_stake_exists_delegate_deposit() {} // TODO
