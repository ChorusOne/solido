#![allow(dead_code)] // Some methods are used for tests
use lido::{
    instruction::{self, initialize},
    processor,
    state::{FeeDistribution, Lido, Validator},
    token::Lamports,
    DEPOSIT_AUTHORITY, RESERVE_AUTHORITY,
};
use solana_program::{hash::Hash, program_pack::Pack, pubkey::Pubkey, system_instruction};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    borsh::try_from_slice_unchecked,
    signature::{Keypair, Signer},
    transaction::Transaction,
    transport::TransportError,
};
use stakepool_account::{create_mint, transfer};
use solana_vote_program::vote_state::{VoteInit, VoteState};
use solana_vote_program::vote_instruction;
use solana_program::system_program;

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

        create_vote_account(
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

pub struct LidoAccounts {
    pub manager: Keypair,
    pub lido: Keypair,
    pub st_sol_mint: Keypair,
    pub maintainer: Keypair,

    // Fees
    pub treasury_account: Keypair,
    pub developer_account: Keypair,
    pub fee_distribution: FeeDistribution,

    pub reserve_authority: Pubkey,
    pub deposit_authority: Pubkey,
}

impl LidoAccounts {
    pub fn new() -> Self {
        let manager = Keypair::new();
        let lido = Keypair::new();
        let st_sol_mint = Keypair::new();
        let maintainer = Keypair::new();

        // Fees
        let treasury_account = Keypair::new();
        let developer_account = Keypair::new();

        let fee_distribution = FeeDistribution {
            treasury_fee: 2,
            validation_fee: 2,
            developer_fee: 4,
        };

        let (reserve_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..], RESERVE_AUTHORITY],
            &id(),
        );

        let (deposit_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..], DEPOSIT_AUTHORITY],
            &id(),
        );

        Self {
            manager,
            lido,
            st_sol_mint,
            maintainer,
            treasury_account,
            developer_account,
            fee_distribution,
            reserve_authority,
            deposit_authority,
        }
    }

    pub async fn initialize_lido(
        &mut self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Result<(), TransportError> {
        let reserve_lamports = Lamports(1);

        create_mint(
            banks_client,
            payer,
            recent_blockhash,
            &self.st_sol_mint,
            &self.reserve_authority,
        )
        .await?;

        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &self.treasury_account,
            &self.st_sol_mint.pubkey(),
            &self.treasury_account.pubkey(),
        )
        .await
        .unwrap();
        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &self.developer_account,
            &self.st_sol_mint.pubkey(),
            &self.developer_account.pubkey(),
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
                        manager: self.manager.pubkey(),
                        st_sol_mint: self.st_sol_mint.pubkey(),
                        treasury_account: self.treasury_account.pubkey(),
                        developer_account: self.developer_account.pubkey(),
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
            &self.st_sol_mint.pubkey(),
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
                    user: user.pubkey(),
                    recipient: recipient.pubkey(),
                    st_sol_mint: self.st_sol_mint.pubkey(),
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
        validator_vote_account: &Pubkey,
        delegate_amount: Lamports,
    ) -> Pubkey {
        let lido_account = get_account(banks_client, &self.lido.pubkey()).await;
        let lido = try_from_slice_unchecked::<Lido>(lido_account.data.as_slice()).unwrap();

        let validator_entry = lido
            .validators
            .get(validator_vote_account)
            .expect("Trying to stake with a non-member validator.");

        let (stake_account, _) = Validator::find_stake_account_address(
            &id(),
            &self.lido.pubkey(),
            validator_vote_account,
            validator_entry.entry.stake_accounts_seed_end,
        );

        let mut transaction = Transaction::new_with_payer(
            &[instruction::stake_deposit(
                &id(),
                &instruction::StakeDepositAccountsMeta {
                    lido: self.lido.pubkey(),
                    maintainer: self.maintainer.pubkey(),
                    validator_vote_account: *validator_vote_account,
                    reserve: self.reserve_authority,
                    stake_account_end: stake_account,
                    deposit_authority: self.deposit_authority,
                },
                delegate_amount,
            )
            .unwrap()],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer, &self.maintainer], *recent_blockhash);
        banks_client.process_transaction(transaction).await.unwrap();

        stake_account
    }

    pub async fn add_validator(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator_vote_account: &Pubkey,
        validator_fee_st_sol_account: &Pubkey,
    ) -> Result<(), TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::add_validator(
                &id(),
                &lido::instruction::AddValidatorMeta {
                    lido: self.lido.pubkey(),
                    manager: self.manager.pubkey(),
                    validator_vote_account: *validator_vote_account,
                    validator_fee_st_sol_account: *validator_fee_st_sol_account,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.manager],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await
    }

    pub async fn remove_validator(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator_vote_account: &Pubkey,
    ) -> Result<(), TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::remove_validator(
                &id(),
                &lido::instruction::RemoveValidatorMeta {
                    lido: self.lido.pubkey(),
                    manager: self.manager.pubkey(),
                    validator_vote_account_to_remove: *validator_vote_account,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.manager],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await
    }

    pub async fn distribute_fees(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Result<(), TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::distribute_fees(
                &id(),
                &lido::instruction::DistributeFeesMeta {
                    lido: self.lido.pubkey(),
                    maintainer: self.maintainer.pubkey(),
                    st_sol_mint: self.st_sol_mint.pubkey(),
                    reserve_authority: self.reserve_authority,
                    treasury_account: self.treasury_account.pubkey(),
                    developer_account: self.developer_account.pubkey(),
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, &self.maintainer],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await
    }

    pub async fn claim_validator_fees(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator_fee_st_sol_account: &Pubkey,
    ) -> Result<(), TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[lido::instruction::claim_validator_fees(
                &id(),
                &lido::instruction::ClaimValidatorFeeMeta {
                    lido: self.lido.pubkey(),
                    st_sol_mint: self.st_sol_mint.pubkey(),
                    reserve_authority: self.reserve_authority,
                    validator_fee_st_sol_account: *validator_fee_st_sol_account,
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer],
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
    mint: &Pubkey,
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
                mint,
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
) -> ValidatorAccounts {
    let accounts = ValidatorAccounts::new(
        banks_client,
        payer,
        &lido_accounts.st_sol_mint.pubkey(),
        recent_blockhash,
    )
    .await;

    lido_accounts
        .add_validator(
            banks_client,
            &payer,
            &recent_blockhash,
            &accounts.vote_account.pubkey(),
            &accounts.fee_account.pubkey(),
        )
        .await
        .unwrap();

    accounts
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

pub async fn create_vote_account(
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
