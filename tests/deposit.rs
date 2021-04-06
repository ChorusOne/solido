mod helpers;

use helpers::*;

use solana_program::{hash::Hash, pubkey::Pubkey};
use solana_program_test::{tokio, BanksClient, ProgramTest};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use solido::{
    entrypoint::process_instruction,
    instructions::StakePoolInstruction,
    model::ValidatorStakeList,
    state::{Fee, StakePool},
};

async fn setup() -> (BanksClient, Keypair, Hash, StakePoolAccounts) {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let stake_pool_accounts = StakePoolAccounts::new();
    stake_pool_accounts
        .initialize_stake_pool(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    (banks_client, payer, recent_blockhash, stake_pool_accounts)
}

#[tokio::test]
async fn process_deposit_test() {
    let (mut banks_client, payer, recent_blockhash, stake_pool_accounts) = setup().await;

    let user = Keypair::new();
}
