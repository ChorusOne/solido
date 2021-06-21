#![allow(dead_code)]

use solana_program::sysvar;

use super::create_token_account;

use {
    solana_program::{
        borsh::get_packed_len, hash::Hash, program_pack::Pack, pubkey::Pubkey, system_instruction,
        system_program,
    },
    solana_program_test::*,
    solana_sdk::{
        signature::{Keypair, Signer},
        transaction::Transaction,
        transport::TransportError,
    },
    solana_vote_program::{
        self, vote_instruction,
        vote_state::{VoteInit, VoteState},
    },
    spl_stake_pool::{self, borsh::get_instance_packed_len, stake_program, state},
};

use lido::token::Lamports;

pub const TEST_STAKE_AMOUNT: u64 = 1_500_000_000;
pub const MAX_TEST_VALIDATORS: u32 = 10_000;

pub async fn create_mint(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    pool_mint: &Keypair,
    manager: &Pubkey,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let mint_rent = rent.minimum_balance(spl_token::state::Mint::LEN);

    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &pool_mint.pubkey(),
                mint_rent,
                spl_token::state::Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &pool_mint.pubkey(),
                &manager,
                None,
                0,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, pool_mint], *recent_blockhash);
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn transfer(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    recipient: &Pubkey,
    amount: Lamports,
) {
    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::transfer(
            &payer.pubkey(),
            recipient,
            amount.0,
        )],
        Some(&payer.pubkey()),
        &[payer],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();
}

pub async fn mint_tokens(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    mint: &Pubkey,
    account: &Pubkey,
    mint_authority: &Keypair,
    amount: u64,
) -> Result<(), TransportError> {
    let transaction = Transaction::new_signed_with_payer(
        &[spl_token::instruction::mint_to(
            &spl_token::id(),
            mint,
            account,
            &mint_authority.pubkey(),
            &[],
            amount,
        )
        .unwrap()],
        Some(&payer.pubkey()),
        &[payer, mint_authority],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn get_token_balance(banks_client: &mut BanksClient, token: &Pubkey) -> u64 {
    let token_account = banks_client.get_account(*token).await.unwrap().unwrap();
    let account_info: spl_token::state::Account =
        spl_token::state::Account::unpack_from_slice(token_account.data.as_slice()).unwrap();
    account_info.amount
}

pub async fn get_token_supply(banks_client: &mut BanksClient, mint: &Pubkey) -> u64 {
    let mint_account = banks_client.get_account(*mint).await.unwrap().unwrap();
    let account_info =
        spl_token::state::Mint::unpack_from_slice(mint_account.data.as_slice()).unwrap();
    account_info.supply
}

pub async fn delegate_tokens(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    account: &Pubkey,
    manager: &Keypair,
    delegate: &Pubkey,
    amount: u64,
) {
    let transaction = Transaction::new_signed_with_payer(
        &[spl_token::instruction::approve(
            &spl_token::id(),
            &account,
            &delegate,
            &manager.pubkey(),
            &[],
            amount,
        )
        .unwrap()],
        Some(&payer.pubkey()),
        &[payer, manager],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();
}

#[allow(clippy::too_many_arguments)]
pub async fn create_stake_pool(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake_pool: &Keypair,
    validator_list: &Keypair,
    reserve_stake: &Pubkey,
    pool_mint: &Pubkey,
    pool_token_account: &Pubkey,
    manager: &Pubkey,
    staker: &Pubkey,
    deposit_authority: &Pubkey,
    fee: &state::Fee,
    max_validators: u32,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let rent_stake_pool = rent.minimum_balance(get_packed_len::<state::StakePool>());
    let validator_list_size =
        get_instance_packed_len(&state::ValidatorList::new(max_validators)).unwrap();
    let rent_validator_list = rent.minimum_balance(validator_list_size);

    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &stake_pool.pubkey(),
                rent_stake_pool,
                get_packed_len::<state::StakePool>() as u64,
                &spl_stake_pool::id(),
            ),
            system_instruction::create_account(
                &payer.pubkey(),
                &validator_list.pubkey(),
                rent_validator_list,
                validator_list_size as u64,
                &spl_stake_pool::id(),
            ),
            lido::instruction::initialize_stake_pool_with_authority(
                &spl_stake_pool::id(),
                &lido::instruction::InitializeStakePoolWithAuthorityAccountsMeta {
                    stake_pool: stake_pool.pubkey(),
                    manager: *manager,
                    staker: *staker,
                    validator_list: validator_list.pubkey(),
                    reserve_stake: *reserve_stake,
                    pool_mint: *pool_mint,
                    fee_account: *pool_token_account,
                    deposit_authority: *deposit_authority,
                    sysvar_clock: sysvar::clock::id(),
                    sysvar_rent: sysvar::rent::id(),
                    sysvar_token: spl_token::id(),
                },
                *fee,
                max_validators,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(&vec![payer, stake_pool, validator_list], *recent_blockhash);
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn create_vote(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    validator: &Keypair,
    vote: &Keypair,
) {
    let rent = banks_client.get_rent().await.unwrap();
    let rent_voter = rent.minimum_balance(VoteState::size_of());

    let mut instructions = vec![system_instruction::create_account(
        &payer.pubkey(),
        &validator.pubkey(),
        42,
        0,
        &system_program::id(),
    )];
    instructions.append(&mut vote_instruction::create_account(
        &payer.pubkey(),
        &vote.pubkey(),
        &VoteInit {
            node_pubkey: validator.pubkey(),
            authorized_voter: validator.pubkey(),
            ..VoteInit::default()
        },
        rent_voter,
    ));

    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[validator, vote, payer],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();
}

pub async fn create_independent_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Keypair,
    authorized: &stake_program::Authorized,
    lockup: &stake_program::Lockup,
    stake_amount: Lamports,
) -> Lamports {
    let rent = banks_client.get_rent().await.unwrap();
    let min_rent_balance =
        Lamports(rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()));
    let lamports = (min_rent_balance + stake_amount).unwrap();

    let transaction = Transaction::new_signed_with_payer(
        &stake_program::create_account(
            &payer.pubkey(),
            &stake.pubkey(),
            authorized,
            lockup,
            lamports.0,
        ),
        Some(&payer.pubkey()),
        &[payer, stake],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    lamports
}

pub async fn create_blank_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Keypair,
) -> u64 {
    let rent = banks_client.get_rent().await.unwrap();
    let lamports = rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()) + 1;

    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::create_account(
            &payer.pubkey(),
            &stake.pubkey(),
            lamports,
            std::mem::size_of::<stake_program::StakeState>() as u64,
            &stake_program::id(),
        )],
        Some(&payer.pubkey()),
        &[payer, stake],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    lamports
}

pub async fn delegate_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Pubkey,
    authorized: &Keypair,
    vote: &Pubkey,
) {
    let mut transaction = Transaction::new_with_payer(
        &[stake_program::delegate_stake(
            &stake,
            &authorized.pubkey(),
            &vote,
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, authorized], *recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();
}

pub async fn authorize_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Pubkey,
    authorized: &Keypair,
    new_authorized: &Pubkey,
    stake_authorize: stake_program::StakeAuthorize,
) {
    let mut transaction = Transaction::new_with_payer(
        &[stake_program::authorize(
            &stake,
            &authorized.pubkey(),
            &new_authorized,
            stake_authorize,
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, authorized], *recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();
}

pub struct ValidatorAccounts {
    pub node_account: Keypair,
    pub vote_account: Keypair,
    pub fee_account: Keypair,
}

impl ValidatorAccounts {
    pub async fn new(
        banks_client: &mut BanksClient,
        payer: &Keypair,
        st_sol_mint: &Pubkey,
        recent_blockhash: &Hash,
    ) -> Self {
        let accounts = ValidatorAccounts {
            node_account: Keypair::new(),
            vote_account: Keypair::new(),
            fee_account: Keypair::new(),
        };

        create_vote(
            banks_client,
            &payer,
            &recent_blockhash,
            &accounts.node_account,
            &accounts.vote_account,
        )
        .await;

        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &accounts.fee_account,
            st_sol_mint,
            &accounts.node_account.pubkey(),
        )
        .await
        .unwrap();

        accounts
    }
}

pub struct StakePoolAccounts {
    pub stake_pool: Keypair,
    pub validator_list: Keypair,
    pub reserve_stake: Keypair,
    pub pool_mint: Keypair,
    pub pool_fee_account: Keypair,
    pub staker: Pubkey,
    pub withdraw_authority: Pubkey,
    pub deposit_authority: Pubkey,
    pub deposit_authority_keypair: Option<Keypair>,
    pub fee: state::Fee,
    pub max_validators: u32,
}

impl StakePoolAccounts {
    pub fn new(staker: Pubkey) -> Self {
        let stake_pool = Keypair::new();
        let validator_list = Keypair::new();
        let stake_pool_address = &stake_pool.pubkey();
        let (withdraw_authority, _) = Pubkey::find_program_address(
            &[&stake_pool_address.to_bytes(), b"withdraw"],
            &spl_stake_pool::id(),
        );
        let (deposit_authority, _) = Pubkey::find_program_address(
            &[&stake_pool_address.to_bytes(), b"deposit"],
            &spl_stake_pool::id(),
        );
        let reserve_stake = Keypair::new();
        let pool_mint = Keypair::new();
        let pool_fee_account = Keypair::new();

        Self {
            stake_pool,
            validator_list,
            reserve_stake,
            pool_mint,
            pool_fee_account,
            staker,
            withdraw_authority,
            deposit_authority,
            deposit_authority_keypair: None,
            fee: state::Fee {
                numerator: 10,
                denominator: 100,
            },
            max_validators: MAX_TEST_VALIDATORS,
        }
    }

    pub fn new_with_deposit_authority(staker: Pubkey, deposit_authority: Keypair) -> Self {
        let mut stake_pool_accounts = Self::new(staker);
        stake_pool_accounts.deposit_authority = deposit_authority.pubkey();
        stake_pool_accounts.deposit_authority_keypair = Some(deposit_authority);
        stake_pool_accounts
    }

    pub async fn initialize_stake_pool(
        &self,
        mut banks_client: &mut BanksClient,
        payer: &Keypair,
        manager: &Pubkey,
        recent_blockhash: &Hash,
        reserve_lamports: Lamports,
        fee_manager: &Pubkey,
    ) -> Result<(), TransportError> {
        create_mint(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.pool_mint,
            &self.withdraw_authority,
        )
        .await?;
        create_token_account(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.pool_fee_account,
            &self.pool_mint.pubkey(),
            &fee_manager,
        )
        .await?;
        create_independent_stake_account(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.reserve_stake,
            &stake_program::Authorized {
                staker: self.withdraw_authority,
                withdrawer: self.withdraw_authority,
            },
            &stake_program::Lockup::default(),
            reserve_lamports,
        )
        .await;
        create_stake_pool(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.stake_pool,
            &self.validator_list,
            &self.reserve_stake.pubkey(),
            &self.pool_mint.pubkey(),
            &self.pool_fee_account.pubkey(),
            manager,
            &self.staker,
            &self.deposit_authority,
            &self.fee,
            self.max_validators,
        )
        .await?;
        Ok(())
    }

    pub async fn update_all(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator_vote_accounts: &[Pubkey],
        no_merge: bool,
    ) -> Option<TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[
                spl_stake_pool::instruction::update_validator_list_balance(
                    &spl_stake_pool::id(),
                    &self.stake_pool.pubkey(),
                    &self.withdraw_authority,
                    &self.validator_list.pubkey(),
                    &self.reserve_stake.pubkey(),
                    validator_vote_accounts,
                    0,
                    no_merge,
                ),
                spl_stake_pool::instruction::update_stake_pool_balance(
                    &spl_stake_pool::id(),
                    &self.stake_pool.pubkey(),
                    &self.withdraw_authority,
                    &self.validator_list.pubkey(),
                    &self.reserve_stake.pubkey(),
                    &self.pool_fee_account.pubkey(),
                    &self.pool_mint.pubkey(),
                ),
            ],
            Some(&payer.pubkey()),
            &[payer],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.err()
    }
}
