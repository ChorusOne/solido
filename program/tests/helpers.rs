#![allow(dead_code)] // Some methods are used for tests
use lido::{
    instruction::{self, initialize},
    processor,
    state::{FeeDistribution, Lido, Validator},
    token::Lamports,
    DEPOSIT_AUTHORITY, RESERVE_AUTHORITY,
};
use solana_program::instruction::Instruction;
use solana_program::system_program;
use solana_program::{program_pack::Pack, pubkey::Pubkey, system_instruction};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    borsh::try_from_slice_unchecked,
    signature::{Keypair, Signer},
    transaction::Transaction,
    transport,
    transport::TransportError,
};
use solana_vote_program::vote_instruction;
use solana_vote_program::vote_state::{VoteInit, VoteState};

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
    ProgramTest::new("lido", id(), processor!(processor::process))
}

/// Sign and send a transaction with a fresh block hash.
///
/// The payer always signs, but additional signers can be passed as well.
///
/// After this, `self.last_blockhash` will contain the block hash used for
/// this transaction.
async fn send_transaction(
    context: &mut ProgramTestContext,
    instructions: &[Instruction],
    additional_signers: Vec<&Keypair>,
) -> transport::Result<()> {
    // Before we send the transaction, get a new block hash, to ensure that
    // if you call `send_transaction` twice with the same payload, we actually
    // send two transactions, instead of the second one silently being ignored.
    context.last_blockhash = context
        .banks_client
        .get_new_blockhash(&context.last_blockhash)
        .await
        .unwrap()
        .0;

    let mut transaction = Transaction::new_with_payer(instructions, Some(&context.payer.pubkey()));

    // Sign with the payer, and additional signers if any.
    let mut signers = additional_signers;
    signers.push(&context.payer);
    transaction.sign(&signers, context.last_blockhash);

    context.banks_client.process_transaction(transaction).await
}

pub struct ValidatorAccounts {
    pub node_account: Keypair,
    pub vote_account: Keypair,
    pub fee_account: Keypair,
}

impl ValidatorAccounts {
    pub async fn new(context: &mut ProgramTestContext, st_sol_mint: &Pubkey) -> Self {
        let accounts = ValidatorAccounts {
            node_account: Keypair::new(),
            vote_account: Keypair::new(),
            fee_account: Keypair::new(),
        };

        create_vote_account(context, &accounts.node_account, &accounts.vote_account).await;

        create_token_account(
            context,
            &accounts.fee_account,
            st_sol_mint,
            &accounts.node_account.pubkey(),
        )
        .await;

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

    pub async fn initialize_lido(&mut self, context: &mut ProgramTestContext) {
        create_mint(context, &self.st_sol_mint, &self.reserve_authority).await;

        create_token_account(
            context,
            &self.treasury_account,
            &self.st_sol_mint.pubkey(),
            &self.treasury_account.pubkey(),
        )
        .await;
        create_token_account(
            context,
            &self.developer_account,
            &self.st_sol_mint.pubkey(),
            &self.developer_account.pubkey(),
        )
        .await;

        let lido_size = Lido::calculate_size(MAX_VALIDATORS, MAX_MAINTAINERS);
        let rent = context.banks_client.get_rent().await.unwrap();
        let rent_lido = rent.minimum_balance(lido_size);
        let rent_reserve = rent.minimum_balance(0);
        send_transaction(
            context,
            &[
                system_instruction::transfer(
                    &context.payer.pubkey(),
                    &self.reserve_authority,
                    rent_reserve,
                ),
                system_instruction::create_account(
                    &context.payer.pubkey(),
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
            vec![&self.lido],
        )
        .await
        .expect("Failed to initialize Solido instance.");

        // Add a maintainer
        simple_add_maintainer(context, &self.maintainer.pubkey(), self).await;
    }

    pub async fn get_solido(&self, context: &mut ProgramTestContext) -> Lido {
        let lido_account = get_account(&mut context.banks_client, &self.lido.pubkey()).await;
        // This returns a Result because it can cause an IO error, but that should
        // not happen in the test environment. (And if it does, then the test just
        // fails.)
        try_from_slice_unchecked::<Lido>(lido_account.data.as_slice()).unwrap()
    }

    /// Create a new stSOL account, deposit the given amount, and return the stSOL account.
    pub async fn deposit(
        &self,
        context: &mut ProgramTestContext,
        deposit_amount: Lamports,
    ) -> Keypair {
        let user = Keypair::new();
        let recipient = Keypair::new();

        create_token_account(
            context,
            &recipient,
            &self.st_sol_mint.pubkey(),
            &user.pubkey(),
        )
        .await;

        // Fund the user account, so the user can deposit that into Solido.
        transfer(context, &user.pubkey(), deposit_amount).await;

        send_transaction(
            context,
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
            vec![&user],
        )
        .await
        .expect("Failed to call Deposit on Solido instance.");

        recipient
    }

    pub async fn stake_deposit(
        &self,
        context: &mut ProgramTestContext,
        validator_vote_account: &Pubkey,
        delegate_amount: Lamports,
    ) -> Pubkey {
        let lido_account = get_account(&mut context.banks_client, &self.lido.pubkey()).await;
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

        send_transaction(
            context,
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
            vec![&self.maintainer],
        )
        .await
        .unwrap();

        stake_account
    }

    pub async fn add_validator(
        &self,
        context: &mut ProgramTestContext,
        validator_vote_account: &Pubkey,
        validator_fee_st_sol_account: &Pubkey,
    ) {
        send_transaction(
            context,
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
            vec![&self.manager],
        )
        .await
        .expect("Failed to add validator to Solido instance.")
    }

    pub async fn remove_validator(
        &self,
        context: &mut ProgramTestContext,
        validator_vote_account: &Pubkey,
    ) {
        send_transaction(
            context,
            &[lido::instruction::remove_validator(
                &id(),
                &lido::instruction::RemoveValidatorMeta {
                    lido: self.lido.pubkey(),
                    manager: self.manager.pubkey(),
                    validator_vote_account_to_remove: *validator_vote_account,
                },
            )
            .unwrap()],
            vec![&self.manager],
        )
        .await
        .expect("Failed to remove validator from Solido instance.")
    }

    pub async fn distribute_fees(
        &self,
        context: &mut ProgramTestContext,
    ) -> Result<(), TransportError> {
        send_transaction(
            context,
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
            vec![&self.maintainer],
        )
        .await
    }

    pub async fn claim_validator_fees(
        &self,
        context: &mut ProgramTestContext,
        validator_fee_st_sol_account: &Pubkey,
    ) -> Result<(), TransportError> {
        send_transaction(
            context,
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
            vec![],
        )
        .await
    }
}

pub async fn create_token_account(
    context: &mut ProgramTestContext,
    account: &Keypair,
    mint: &Pubkey,
    manager: &Pubkey,
) {
    let rent = context.banks_client.get_rent().await.unwrap();
    let account_rent = rent.minimum_balance(spl_token::state::Account::LEN);

    send_transaction(
        context,
        &[
            system_instruction::create_account(
                &context.payer.pubkey(),
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
        vec![account],
    )
    .await
    .expect("Failed to create token account.")
}

pub async fn simple_add_validator_to_pool(
    context: &mut ProgramTestContext,
    lido_accounts: &LidoAccounts,
) -> ValidatorAccounts {
    let accounts = ValidatorAccounts::new(context, &lido_accounts.st_sol_mint.pubkey()).await;

    lido_accounts
        .add_validator(
            context,
            &accounts.vote_account.pubkey(),
            &accounts.fee_account.pubkey(),
        )
        .await;

    accounts
}

pub async fn simple_add_maintainer(
    context: &mut ProgramTestContext,
    maintainer: &Pubkey,
    lido_accounts: &LidoAccounts,
) {
    send_transaction(
        context,
        &[lido::instruction::add_maintainer(
            &id(),
            &lido::instruction::AddMaintainerMeta {
                lido: lido_accounts.lido.pubkey(),
                manager: lido_accounts.manager.pubkey(),
                maintainer: *maintainer,
            },
        )
        .unwrap()],
        vec![&lido_accounts.manager],
    )
    .await
    .expect("Failed to add maintainer.")
}

pub async fn simple_remove_maintainer(
    context: &mut ProgramTestContext,
    lido_accounts: &LidoAccounts,
    maintainer: &Pubkey,
) -> Result<(), TransportError> {
    send_transaction(
        context,
        &[lido::instruction::remove_maintainer(
            &id(),
            &lido::instruction::RemoveMaintainerMeta {
                lido: lido_accounts.lido.pubkey(),
                manager: lido_accounts.manager.pubkey(),
                maintainer: *maintainer,
            },
        )
        .unwrap()],
        vec![&lido_accounts.manager],
    )
    .await
}

pub async fn create_vote_account(
    context: &mut ProgramTestContext,
    validator: &Keypair,
    vote: &Keypair,
) {
    let rent = context.banks_client.get_rent().await.unwrap();
    let rent_voter = rent.minimum_balance(VoteState::size_of());

    let initial_balance = Lamports(42);
    let size_bytes = 0;

    let mut instructions = vec![system_instruction::create_account(
        &context.payer.pubkey(),
        &validator.pubkey(),
        initial_balance.0,
        size_bytes,
        &system_program::id(),
    )];
    instructions.append(&mut vote_instruction::create_account(
        &context.payer.pubkey(),
        &vote.pubkey(),
        &VoteInit {
            node_pubkey: validator.pubkey(),
            authorized_voter: validator.pubkey(),
            ..VoteInit::default()
        },
        rent_voter,
    ));
    send_transaction(context, &instructions, vec![validator, vote])
        .await
        .expect("Failed to create vote account.")
}

pub async fn create_mint(context: &mut ProgramTestContext, pool_mint: &Keypair, manager: &Pubkey) {
    let rent = context.banks_client.get_rent().await.unwrap();
    let mint_rent = rent.minimum_balance(spl_token::state::Mint::LEN);

    send_transaction(
        context,
        &[
            system_instruction::create_account(
                &context.payer.pubkey(),
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
        vec![pool_mint],
    )
    .await
    .expect("Failed to create SPL token mint.")
}

pub async fn transfer(context: &mut ProgramTestContext, recipient: &Pubkey, amount: Lamports) {
    send_transaction(
        context,
        &[system_instruction::transfer(
            &context.payer.pubkey(),
            recipient,
            amount.0,
        )],
        vec![],
    )
    .await
    .unwrap_or_else(|_| panic!("Failed to transfer {} to {}.", amount, recipient))
}

pub async fn get_token_balance(banks_client: &mut BanksClient, token: &Pubkey) -> u64 {
    let token_account = banks_client.get_account(*token).await.unwrap().unwrap();
    let account_info: spl_token::state::Account =
        spl_token::state::Account::unpack_from_slice(token_account.data.as_slice()).unwrap();
    account_info.amount
}
