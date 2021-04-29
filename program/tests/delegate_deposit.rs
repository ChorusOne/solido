mod helpers;

use helpers::{
    program_test,
    stakepool_account::{
        create_token_account, get_account, get_token_balance, simple_add_validator_to_pool,
        transfer, ValidatorStakeAccount,
    },
    LidoAccounts,
};
use lido::{id, instruction, state};
use solana_program::{borsh::get_packed_len, hash::Hash, pubkey::Pubkey, system_instruction};
use solana_program_test::{tokio, BanksClient};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
    transport::TransportError,
};

use lido::DEPOSIT_AUTHORITY_ID;

async fn create_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: Hash,
    to: &Keypair,
    owner: &Pubkey,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let mint_rent = rent.minimum_balance(0);

    banks_client
        .process_transaction(Transaction::new_signed_with_payer(
            &[system_instruction::create_account(
                &payer.pubkey(),
                &to.pubkey(),
                mint_rent,
                0,
                owner,
            )],
            Some(&payer.pubkey()),
            &[payer, to],
            recent_blockhash,
        ))
        .await?;
    Ok(())
}

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
async fn test_successful_delegate_deposit() {
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

    // program_id: &Pubkey,
    // lido: &Pubkey,
    // validator: &Pubkey,
    // reserve: &Pubkey,
    // stake: &Pubkey,
    // deposit_authority: &Pubkey,
    // withdraw_authority: &Pubkey,

    // stake_pool_program: &Pubkey,
    // stake_pool: &Pubkey,
    // stake_pool_validator_list: &Pubkey,
    // stake_pool_withdraw_authority: &Pubkey,
    // stake_pool_validator_stake_account: &Pubkey,
    // stake_pool_mint: &Pubkey,
    // amount: u64,
    println!(
        "VALIDATOR VOTE: {}\nValidator: {}",
        validator_account.vote.pubkey(),
        validator_account.validator.pubkey()
    );

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
}
