#![allow(dead_code)] // Some methods are used for tests
use lido::{
    instruction::{self, initialize},
    processor,
    state::{FeeDistribution, Lido, Maintainers, Validator, Validators, LIDO_CONSTANT_SIZE},
    token::Lamports,
    DEPOSIT_AUTHORITY, FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_AUTHORITY,
};
use solana_program::{hash::Hash, program_pack::Pack, pubkey::Pubkey, system_instruction};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    borsh::{get_instance_packed_len, try_from_slice_unchecked},
    signature::{Keypair, Signer},
    transaction::Transaction,
    transport::TransportError,
};
use stakepool_account::StakePoolAccounts;

use self::stakepool_account::{create_mint, transfer, ValidatorStakeAccount};

pub mod stakepool_account;
pub const MAX_VALIDATORS: u32 = 10_000;
pub const MAX_MAINTAINERS: u32 = 100;

// This id is only used throughout these tests.
solana_program::declare_id!("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc");

pub async fn get_account(banks_client: &mut BanksClient, pubkey: &Pubkey) -> Account {
    banks_client
        .get_account(*pubkey)
        .await
        .expect("account not found")
        .expect("account empty")
}

pub fn program_test() -> ProgramTest {
    let mut program = ProgramTest::new("lido", id(), processor!(processor::process));
    program.add_program(
        "spl_stake_pool",
        spl_stake_pool::id(),
        processor!(spl_stake_pool::processor::Processor::process),
    );
    program
}

pub struct LidoAccounts {
    pub manager: Keypair,
    pub lido: Keypair,
    pub mint_program: Keypair,
    pub pool_token_to: Keypair,
    pub maintainer: Keypair,

    // Fees
    pub insurance_account: Keypair,
    pub treasury_account: Keypair,
    pub manager_fee_account: Keypair,
    pub fee_distribution: FeeDistribution,

    pub reserve_authority: Pubkey,
    pub deposit_authority: Pubkey,
    pub stake_pool_authority: Pubkey,
    pub fee_manager_authority: Pubkey,
    pub stake_pool_accounts: StakePoolAccounts,
}

impl LidoAccounts {
    pub fn new() -> Self {
        let manager = Keypair::new();
        let lido = Keypair::new();
        let mint_program = Keypair::new();
        let pool_token_to = Keypair::new();
        let maintainer = Keypair::new();

        // Fees
        let insurance_account = Keypair::new();
        let treasury_account = Keypair::new();
        let manager_fee_account = Keypair::new();

        let fee_distribution = FeeDistribution {
            insurance_fee: 2,
            treasury_fee: 2,
            validation_fee: 2,
            manager_fee: 4,
        };

        let (reserve_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..], RESERVE_AUTHORITY],
            &id(),
        );

        let (deposit_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..], DEPOSIT_AUTHORITY],
            &id(),
        );
        let (stake_pool_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..], STAKE_POOL_AUTHORITY],
            &id(),
        );

        let (fee_manager_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..], FEE_MANAGER_AUTHORITY],
            &id(),
        );

        let mut stake_pool_accounts = StakePoolAccounts::new(stake_pool_authority);
        stake_pool_accounts.deposit_authority = reserve_authority;
        Self {
            manager,
            lido,
            mint_program,
            pool_token_to,
            maintainer,
            insurance_account,
            treasury_account,
            manager_fee_account,
            fee_distribution,
            reserve_authority,
            deposit_authority,
            stake_pool_authority,
            fee_manager_authority,
            stake_pool_accounts,
        }
    }

    pub async fn initialize_lido(
        &mut self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Result<(), TransportError> {
        self.stake_pool_accounts.deposit_authority = self.deposit_authority;
        let reserve_lamports = Lamports(1);
        self.stake_pool_accounts
            .initialize_stake_pool(
                banks_client,
                payer,
                &self.manager,
                recent_blockhash,
                reserve_lamports,
                &self.fee_manager_authority,
            )
            .await?;

        create_mint(
            banks_client,
            payer,
            recent_blockhash,
            &self.mint_program,
            &self.reserve_authority,
        )
        .await?;

        create_token_account(
            banks_client,
            &payer,
            &recent_blockhash,
            &self.pool_token_to,
            &self.stake_pool_accounts.pool_mint.pubkey(),
            &self.stake_pool_authority,
        )
        .await
        .unwrap();

        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &self.insurance_account,
            &self.mint_program.pubkey(),
            &self.insurance_account.pubkey(),
        )
        .await
        .unwrap();
        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &self.treasury_account,
            &self.mint_program.pubkey(),
            &self.treasury_account.pubkey(),
        )
        .await
        .unwrap();
        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &self.manager_fee_account,
            &self.mint_program.pubkey(),
            &self.manager_fee_account.pubkey(),
        )
        .await
        .unwrap();

        let lido_size = Lido::calculate_size(MAX_VALIDATORS, MAX_MAINTAINERS);
        let rent = banks_client.get_rent().await.unwrap();
        let rent_lido = rent.minimum_balance(lido_size);
        let rent_reserve = rent.minimum_balance(0);
        let mut transaction = Transaction::new_with_payer(
            &[
                system_instruction::transfer(
                    &payer.pubkey(),
                    &self.reserve_authority,
                    rent_reserve,
                ),
                system_instruction::create_account(
                    &payer.pubkey(),
                    &self.lido.pubkey(),
                    rent_lido,
                    lido_size as u64,
                    &id(),
                ),
                initialize(
                    &id(),
                    self.fee_distribution.clone(),
                    MAX_VALIDATORS,
                    MAX_MAINTAINERS,
                    &instruction::InitializeAccountsMeta {
                        lido: self.lido.pubkey(),
                        stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                        manager: self.manager.pubkey(),
                        mint_program: self.mint_program.pubkey(),
                        pool_token_to: self.pool_token_to.pubkey(),
                        fee_token: self.stake_pool_accounts.pool_fee_account.pubkey(),
                        insurance_account: self.insurance_account.pubkey(),
                        treasury_account: self.treasury_account.pubkey(),
                        manager_fee_account: self.manager_fee_account.pubkey(),
                        reserve_account: self.reserve_authority,
                    },
                )
                .unwrap(),
            ],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer, &self.lido], *recent_blockhash);
        banks_client.process_transaction(transaction).await?;

        // Add a maintainer
        simple_add_maintainer(
            banks_client,
            payer,
            recent_blockhash,
            &self.maintainer.pubkey(),
            self,
        )
        .await?;
        Ok(())
    }

    pub async fn deposit(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        deposit_amount: Lamports,
    ) -> Keypair {
        let user = Keypair::new();
        let recipient = Keypair::new();

        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &recipient,
            &self.mint_program.pubkey(),
            &user.pubkey(),
        )
        .await
        .unwrap();

        transfer(
            banks_client,
            payer,
            recent_blockhash,
            &user.pubkey(),
            deposit_amount,
        )
        .await;

        let mut transaction = Transaction::new_with_payer(
            &[instruction::deposit(
                &id(),
                &instruction::DepositAccountsMeta {
                    lido: self.lido.pubkey(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    pool_token_to: self.pool_token_to.pubkey(),
                    manager: self.manager.pubkey(),
                    user: user.pubkey(),
                    recipient: recipient.pubkey(),
                    mint_program: self.mint_program.pubkey(),
                    reserve_account: self.reserve_authority,
                },
                deposit_amount,
            )
            .unwrap()],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[&payer, &user], *recent_blockhash);
        banks_client.process_transaction(transaction).await.unwrap();
        recipient
    }

    pub async fn stake_deposit(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator: &ValidatorStakeAccount,
        delegate_amount: Lamports,
    ) -> Pubkey {
        let lido_account = get_account(banks_client, &self.lido.pubkey()).await;
        let lido = try_from_slice_unchecked::<Lido>(lido_account.data.as_slice()).unwrap();

        let (_key, validator_state) = lido
            .validators
            .get(&validator.stake_pool_stake_account)
            .expect("Trying to stake with a non-mebmer validator.");

        let (stake_account, _) = Validator::find_stake_account_address(
            &id(),
            &self.lido.pubkey(),
            &validator.stake_pool_stake_account,
            validator_state.stake_accounts_seed_end,
        );

        let mut transaction = Transaction::new_with_payer(
            &[instruction::stake_deposit(
                &id(),
                &instruction::StakeDepositAccountsMeta {
                    lido: self.lido.pubkey(),
                    validator_stake_pool_stake_account: validator.stake_pool_stake_account,
                    validator_vote_account: validator.vote.pubkey(),
                    reserve: self.reserve_authority,
                    stake_account_end: stake_account,
                    deposit_authority: self.deposit_authority,
                },
                delegate_amount,
            )
            .unwrap()],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer], *recent_blockhash);
        banks_client.process_transaction(transaction).await.unwrap();

        stake_account
    }

    pub async fn deposit_active_stake_to_pool(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator: &ValidatorStakeAccount,
        stake_account: &Pubkey,
    ) {
        let mut transaction = Transaction::new_with_payer(
            &[instruction::deposit_active_stake_to_pool(
                &id(),
                &instruction::DepositActiveStakeToPoolAccountsMeta {
                    lido: self.lido.pubkey(),
                    maintainer: self.maintainer.pubkey(),
                    validator: validator.vote.pubkey(),
                    stake_account_begin: *stake_account,
                    deposit_authority: self.deposit_authority,
                    pool_token_to: self.pool_token_to.pubkey(),
                    stake_pool_program: spl_stake_pool::id(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    stake_pool_validator_list: self.stake_pool_accounts.validator_list.pubkey(),
                    stake_pool_withdraw_authority: self.stake_pool_accounts.withdraw_authority,
                    stake_pool_validator_stake_account: validator.stake_pool_stake_account,
                    stake_pool_mint: self.stake_pool_accounts.pool_mint.pubkey(),
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer, &self.maintainer], *recent_blockhash);
        banks_client.process_transaction(transaction).await.unwrap();
    }

    async fn create_validator_stake_account(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        staker: &Pubkey,
        stake_account: &Pubkey,
        validator: &Pubkey,
    ) {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::create_validator_stake_account(
                &id(),
                &lido::instruction::CreateValidatorStakeAccountMeta {
                    lido: self.lido.pubkey(),
                    manager: self.manager.pubkey(),
                    stake_pool_program: spl_stake_pool::id(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    staker: *staker,
                    funder: payer.pubkey(),
                    stake_account: *stake_account,
                    validator: *validator,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.manager],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.unwrap();
    }

    pub async fn add_validator(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        stake: &Pubkey,
        validator_token_account: &Pubkey,
    ) -> Option<TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::add_validator(
                &id(),
                &lido::instruction::AddValidatorMeta {
                    lido: self.lido.pubkey(),
                    manager: self.manager.pubkey(),
                    stake_pool_manager_authority: self.stake_pool_authority,
                    stake_pool_program: spl_stake_pool::id(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    stake_pool_withdraw_authority: self.stake_pool_accounts.withdraw_authority,
                    stake_pool_validator_list: self.stake_pool_accounts.validator_list.pubkey(),
                    stake_account: *stake,
                    validator_token_account: *validator_token_account,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.manager],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.err()
    }

    pub async fn remove_validator(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        new_authority: &Pubkey,
        validator_stake: &Pubkey,
        transient_stake: &Pubkey,
    ) -> Option<TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::remove_validator(
                &id(),
                &lido::instruction::RemoveValidatorMeta {
                    lido: self.lido.pubkey(),
                    manager: self.manager.pubkey(),
                    stake_pool_manager_authority: self.stake_pool_authority,
                    stake_pool_program: spl_stake_pool::id(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    stake_pool_withdraw_authority: self.stake_pool_accounts.withdraw_authority,
                    new_withdraw_authority: *new_authority,
                    stake_pool_validator_list: self.stake_pool_accounts.validator_list.pubkey(),
                    stake_account_to_remove: *validator_stake,
                    transient_stake: *transient_stake,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.manager],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.err()
    }

    pub async fn distribute_fees(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Option<TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::distribute_fees(
                &id(),
                &lido::instruction::DistributeFeesMeta {
                    lido: self.lido.pubkey(),
                    manager: self.manager.pubkey(),
                    token_holder_stake_pool: self.pool_token_to.pubkey(),
                    mint_program: self.mint_program.pubkey(),
                    reserve_authority: self.reserve_authority,
                    insurance_account: self.insurance_account.pubkey(),
                    treasury_account: self.treasury_account.pubkey(),
                    manager_fee_account: self.manager_fee_account.pubkey(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    stake_pool_fee_account: self.stake_pool_accounts.pool_fee_account.pubkey(),
                    stake_pool_manager_fee_account: self.fee_manager_authority,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.manager],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.err()
    }

    pub async fn claim_validator_fees(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator_token: &Pubkey,
    ) -> Result<(), TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::claim_validator_fees(
                &id(),
                &lido::instruction::ClaimValidatorFeeMeta {
                    lido: self.lido.pubkey(),
                    mint_program: self.mint_program.pubkey(),
                    reserve_authority: self.reserve_authority,
                    validator_token: *validator_token,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await
    }
    pub async fn increase_validator_stake(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        transient_stake: &Pubkey,
        validator_vote: &Pubkey,
        lamports: Lamports,
    ) -> Result<(), TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[instruction::increase_validator_stake(
                &id(),
                lamports,
                &instruction::IncreaseValidatorStakeMeta {
                    lido: self.lido.pubkey(),
                    maintainer: self.maintainer.pubkey(),
                    stake_pool_program: spl_stake_pool::id(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    stake_pool_manager_authority: self.stake_pool_authority,
                    stake_pool_withdraw_authority: self.stake_pool_accounts.withdraw_authority,
                    stake_pool_validator_list: self.stake_pool_accounts.validator_list.pubkey(),
                    stake_pool_reserve_stake: self.stake_pool_accounts.reserve_stake.pubkey(),
                    transient_stake: *transient_stake,
                    validator_vote: *validator_vote,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.maintainer],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await
    }

    pub async fn decrease_validator_stake(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        transient_stake: &Pubkey,
        validator_stake: &Pubkey,
        lamports: Lamports,
    ) -> Result<(), TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[instruction::decrease_validator_stake(
                &id(),
                lamports,
                &instruction::DecreaseValidatorStakeMeta {
                    lido: self.lido.pubkey(),
                    maintainer: self.maintainer.pubkey(),
                    stake_pool_program: spl_stake_pool::id(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    stake_pool_manager_authority: self.stake_pool_authority,
                    stake_pool_withdraw_authority: self.stake_pool_accounts.withdraw_authority,
                    stake_pool_validator_list: self.stake_pool_accounts.validator_list.pubkey(),
                    validator_stake: *validator_stake,
                    transient_stake: *transient_stake,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.maintainer],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await
    }
}

pub async fn create_token_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    account: &Keypair,
    pool_mint: &Pubkey,
    manager: &Pubkey,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let account_rent = rent.minimum_balance(spl_token::state::Account::LEN);

    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &account.pubkey(),
                account_rent,
                spl_token::state::Account::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_account(
                &spl_token::id(),
                &account.pubkey(),
                pool_mint,
                manager,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, account], *recent_blockhash);
    banks_client.process_transaction(transaction).await
}

pub async fn simple_add_validator_to_pool(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    lido_accounts: &LidoAccounts,
) -> ValidatorStakeAccount {
    let validator_stake =
        ValidatorStakeAccount::new(&lido_accounts.stake_pool_accounts.stake_pool.pubkey());
    validator_stake
        .create_and_delegate(
            banks_client,
            &payer,
            &recent_blockhash,
            lido_accounts,
            &lido_accounts.stake_pool_accounts.staker,
        )
        .await;

    create_token_account(
        banks_client,
        payer,
        recent_blockhash,
        &validator_stake.validator_token_account,
        &lido_accounts.mint_program.pubkey(),
        &validator_stake.validator_token_account.pubkey(),
    )
    .await
    .unwrap();

    let error = lido_accounts
        .add_validator(
            banks_client,
            &payer,
            &recent_blockhash,
            &validator_stake.stake_pool_stake_account,
            &validator_stake.validator_token_account.pubkey(),
        )
        .await;
    assert!(error.is_none());

    validator_stake
}

pub async fn simple_add_maintainer(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    maintainer: &Pubkey,
    lido_accounts: &LidoAccounts,
) -> Result<(), TransportError> {
    let transaction = Transaction::new_signed_with_payer(
        &[lido::instruction::add_maintainer(
            &id(),
            &lido::instruction::AddMaintainerMeta {
                lido: lido_accounts.lido.pubkey(),
                manager: lido_accounts.manager.pubkey(),
                maintainer: *maintainer,
            },
        )
        .unwrap()],
        Some(&payer.pubkey()),
        &[payer, &lido_accounts.manager],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn simple_remove_maintainer(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    lido_accounts: &LidoAccounts,
    maintainer: &Pubkey,
) -> Result<(), TransportError> {
    let transaction = Transaction::new_signed_with_payer(
        &[lido::instruction::remove_maintainer(
            &id(),
            &lido::instruction::RemoveMaintainerMeta {
                lido: lido_accounts.lido.pubkey(),
                manager: lido_accounts.manager.pubkey(),
                maintainer: *maintainer,
            },
        )
        .unwrap()],
        Some(&payer.pubkey()),
        &[payer, &lido_accounts.manager],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await?;
    Ok(())
}
