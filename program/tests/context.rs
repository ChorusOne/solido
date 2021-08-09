// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Holds a test context, which makes it easier to test with a Solido instance set up.

use num_traits::cast::FromPrimitive;
use rand::prelude::StdRng;
use rand::SeedableRng;
use solana_program::program_pack::Pack;
use solana_program::rent::Rent;
use solana_program::stake::state::Stake;
use solana_program::system_instruction;
use solana_program::system_program;
use solana_program::{borsh::try_from_slice_unchecked, sysvar};
use solana_program::{clock::Clock, instruction::Instruction};
use solana_program::{instruction::InstructionError, stake_history::StakeHistory};
use solana_program_test::{processor, ProgramTest, ProgramTestBanksClientExt, ProgramTestContext};
use solana_sdk::account::ReadableAccount;
use solana_sdk::account::{from_account, Account};
use solana_sdk::account_info::AccountInfo;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use solana_sdk::transaction::TransactionError;
use solana_sdk::transport;
use solana_sdk::transport::TransportError;
use solana_vote_program::vote_instruction;
use solana_vote_program::vote_state::{VoteInit, VoteState};

use lido::token::{Lamports, StLamports};
use lido::{
    error::LidoError, instruction, RESERVE_ACCOUNT, REWARDS_WITHDRAW_AUTHORITY, STAKE_AUTHORITY,
};
use lido::{
    state::{FeeRecipients, Lido, RewardDistribution, Validator},
    MINT_AUTHORITY,
};

pub struct DeterministicKeypairGen {
    rng: StdRng,
}

impl DeterministicKeypairGen {
    fn new() -> Self {
        let rng = StdRng::seed_from_u64(0);
        DeterministicKeypairGen { rng }
    }
    pub fn new_keypair(&mut self) -> Keypair {
        Keypair::generate(&mut self.rng)
    }
}

#[test]
fn test_deterministic_key() {
    let mut deterministic_keypair = DeterministicKeypairGen::new();
    let kp1 = deterministic_keypair.new_keypair();
    let expected_result: &[u8] = &[
        178, 247, 245, 129, 214, 222, 60, 6, 168, 34, 253, 110, 126, 130, 101, 251, 192, 15, 132,
        1, 105, 106, 91, 220, 52, 245, 166, 210, 255, 63, 146, 47, 237, 208, 246, 222, 52, 42, 30,
        106, 114, 54, 214, 36, 79, 35, 216, 62, 237, 252, 236, 208, 89, 163, 134, 200, 80, 85, 112,
        20, 152, 231, 112, 51,
    ];
    assert_eq!(kp1.to_bytes(), expected_result);
}

// This id is only used throughout these tests.
solana_program::declare_id!("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc");

pub struct Context {
    pub deterministic_keypair: DeterministicKeypairGen,
    /// Inner test context that contains the banks client and recent block hash.
    pub context: ProgramTestContext,

    /// A nonce to make similar transactions distinct, incremented after every
    /// `send_transaction`.
    pub nonce: u64,

    // Key pairs for the accounts in the Solido instance.
    pub solido: Keypair,
    pub manager: Keypair,
    pub st_sol_mint: Pubkey,
    pub maintainer: Option<Keypair>,
    pub validator: Option<ValidatorAccounts>,

    pub treasury_st_sol_account: Pubkey,
    pub developer_st_sol_account: Pubkey,
    pub reward_distribution: RewardDistribution,

    pub reserve_address: Pubkey,
    pub stake_authority: Pubkey,
    pub mint_authority: Pubkey,
    pub withdraw_authority: Pubkey,
}

pub struct ValidatorAccounts {
    pub node_account: Keypair,
    pub vote_account: Pubkey,
    pub fee_account: Pubkey,
}

/// Sign and send a transaction with a fresh block hash.
///
/// The payer always signs, but additional signers can be passed as well.
///
/// Takes a nonce to ensure that sending the same instruction twice will result
/// in distinct transactions. This function increments the nonce after using it.
pub async fn send_transaction(
    context: &mut ProgramTestContext,
    nonce: &mut u64,
    instructions: &[Instruction],
    additional_signers: Vec<&Keypair>,
) -> transport::Result<()> {
    let mut instructions_mut = instructions.to_vec();

    // If we try to send exactly the same transaction twice, the second one will
    // not be considered distinct by the runtime, and it will not execute, but
    // instead immediately complete successfully. This is undesirable in tests,
    // sometimes we do want to repeat a transaction, e.g. update the exchange
    // rate twice in the same epoch, and confirm that the second one is rejected.
    // Normally the way to do this in Solana is to wait for a new recent block
    // hash. If the block hash is different, the transactions will be distinct.
    // Unfortunately, `get_new_blockhash` interacts badly with `warp_to_slot`.
    // See also https://github.com/solana-labs/solana/issues/18201. To work
    // around this, instead of changing the block hash, add a memo instruction
    // with a nonce to every transaction, to make the transactions distinct.
    let memo = spl_memo::build_memo(&format!("nonce={}", *nonce).as_bytes(), &[]);
    instructions_mut.push(memo);
    *nonce += 1;

    // However, if we execute many transactions and don't request a new block
    // hash, the block hash will eventually be too old. `solana_program_test`
    // doesn’t tell you that this is the problem, instead `process_transaction`
    // will fail with a timeout `IoError`. So do refresh the block hash every
    // 300 transactions.
    if *nonce % 300 == 299 {
        context.last_blockhash = context
            .banks_client
            .get_new_blockhash(&context.last_blockhash)
            .await
            .expect("Failed to get a new blockhash.")
            .0;
    }

    // Change this to true to enable more verbose test output.
    if false {
        for (i, instruction) in instructions_mut.iter().enumerate() {
            println!(
                "Instruction #{} calls program {}.",
                i, instruction.program_id
            );
            for (j, account) in instruction.accounts.iter().enumerate() {
                println!(
                    "  Account {:2}: [{}{}] {}",
                    j,
                    if account.is_writable { 'W' } else { '-' },
                    if account.is_signer { 'S' } else { '-' },
                    account.pubkey,
                );
            }
        }
    }

    let mut transaction =
        Transaction::new_with_payer(&instructions_mut, Some(&context.payer.pubkey()));

    // Sign with the payer, and additional signers if any.
    let mut signers = additional_signers;
    signers.push(&context.payer);
    transaction.sign(&signers, context.last_blockhash);

    let result = context.banks_client.process_transaction(transaction).await;

    // If the transaction failed, try to be helpful by converting the error code
    // back to a message if possible.
    match result {
        Err(TransportError::TransactionError(TransactionError::InstructionError(
            _,
            InstructionError::Custom(error_code),
        ))) => {
            println!("Transaction failed with InstructionError::Custom.");
            match LidoError::from_u32(error_code) {
                Some(err) => println!(
                    "If this error originated from Solido, it was this variant: {:?}",
                    err
                ),
                None => println!("This error is not a known Solido error."),
            }
        }
        _ => {}
    }

    result
}

/// The different ways to stake some amount from the reserve.
pub enum StakeDeposit {
    /// Stake into a new stake account, and delegate the new account.
    ///
    /// This consumes the end seed of the validator's stake accounts.
    Append,

    /// Stake into temporary stake account, and immediately merge it.
    ///
    /// This merges into the stake account at `end_seed - 1`.
    Merge,
}

impl Context {
    /// Set up a new test context with an initialized Solido instance.
    ///
    /// The instance contains no maintainers yet.
    pub async fn new_empty() -> Context {
        let mut deterministic_keypair = DeterministicKeypairGen::new();
        let manager = deterministic_keypair.new_keypair();
        let solido = deterministic_keypair.new_keypair();

        let reward_distribution = RewardDistribution {
            validation_fee: 5,
            treasury_fee: 3,
            developer_fee: 2,
            st_sol_appreciation: 90,
        };

        let (reserve_address, _) = Pubkey::find_program_address(
            &[&solido.pubkey().to_bytes()[..], RESERVE_ACCOUNT],
            &id(),
        );

        let (stake_authority, _) = Pubkey::find_program_address(
            &[&solido.pubkey().to_bytes()[..], STAKE_AUTHORITY],
            &id(),
        );
        let (mint_authority, _) =
            Pubkey::find_program_address(&[&solido.pubkey().to_bytes()[..], MINT_AUTHORITY], &id());

        let (withdraw_authority, _) = Pubkey::find_program_address(
            &[&solido.pubkey().to_bytes()[..], REWARDS_WITHDRAW_AUTHORITY],
            &id(),
        );

        // Note: this name *must* match the name of the crate that contains the
        // program. If it does not, then it will still partially work, but we get
        // weird errors about resizing accounts.
        let program_crate_name = "lido";
        let program_test = ProgramTest::new(
            program_crate_name,
            id(),
            processor!(lido::processor::process),
        );

        let mut result = Self {
            context: program_test.start_with_context().await,
            nonce: 0,
            manager,
            solido,
            st_sol_mint: Pubkey::default(),
            maintainer: None,
            validator: None,
            treasury_st_sol_account: Pubkey::default(),
            developer_st_sol_account: Pubkey::default(),
            reward_distribution,
            reserve_address,
            stake_authority,
            mint_authority,
            withdraw_authority,
            deterministic_keypair: deterministic_keypair,
        };

        result.st_sol_mint = result.create_mint(result.mint_authority).await;

        let treasury_owner = result.deterministic_keypair.new_keypair();
        result.treasury_st_sol_account =
            result.create_st_sol_account(treasury_owner.pubkey()).await;

        let developer_owner = result.deterministic_keypair.new_keypair();
        result.developer_st_sol_account =
            result.create_st_sol_account(developer_owner.pubkey()).await;

        let max_validators = 10_000;
        let max_maintainers = 1000;
        let solido_size = Lido::calculate_size(max_validators, max_maintainers);
        let rent = result.context.banks_client.get_rent().await.unwrap();
        let rent_solido = rent.minimum_balance(solido_size);

        let rent_reserve = rent.minimum_balance(0);
        result
            .fund(result.reserve_address, Lamports(rent_reserve))
            .await;

        let payer = result.context.payer.pubkey();
        send_transaction(
            &mut result.context,
            &mut result.nonce,
            &[
                system_instruction::create_account(
                    &payer,
                    &result.solido.pubkey(),
                    rent_solido,
                    solido_size as u64,
                    &id(),
                ),
                instruction::initialize(
                    &id(),
                    result.reward_distribution.clone(),
                    max_validators,
                    max_maintainers,
                    &instruction::InitializeAccountsMeta {
                        lido: result.solido.pubkey(),
                        manager: result.manager.pubkey(),
                        st_sol_mint: result.st_sol_mint,
                        treasury_account: result.treasury_st_sol_account,
                        developer_account: result.developer_st_sol_account,
                        reserve_account: result.reserve_address,
                    },
                ),
            ],
            vec![&result.solido],
        )
        .await
        .expect("Failed to initialize Solido instance.");

        result
    }

    /// Set up a new test context, where the Solido instance has a single maintainer.
    pub async fn new_with_maintainer() -> Context {
        let mut result = Context::new_empty().await;
        result.maintainer = Some(result.add_maintainer().await);
        result
    }

    /// Set up a new test context, where the Solido instance has a single maintainer and single validator.
    pub async fn new_with_maintainer_and_validator() -> Context {
        let mut result = Context::new_with_maintainer().await;
        result.validator = Some(result.add_validator().await);
        result
    }

    /// Set up a new test context, where the Solido instance has a single maintainer, one
    /// validator. Deposits 20 Sol and stake 2 accounts with 10 Sol each.
    pub async fn new_with_two_stake_accounts() -> (Context, Vec<Pubkey>) {
        let mut result = Context::new_with_maintainer().await;
        let validator = result.add_validator().await;

        result.deposit(Lamports(20_000_000_000)).await;
        let mut stake_accounts = Vec::new();
        for _ in 0..2 {
            let stake_account = result
                .stake_deposit(
                    validator.vote_account,
                    StakeDeposit::Append,
                    Lamports(10_000_000_000),
                )
                .await;

            stake_accounts.push(stake_account);
        }
        result.validator = Some(validator);
        (result, stake_accounts)
    }

    /// Send a memo transaction. This can be used for printf debugging, because
    /// the memo will show up in the test logs, between the other transaction
    /// logs.
    pub async fn memo(&mut self, message: &str) {
        let memo_instr = spl_memo::build_memo(message.as_bytes(), &[]);
        send_transaction(&mut self.context, &mut self.nonce, &[memo_instr], vec![])
            .await
            .expect("Failed to send memo transaction.")
    }

    /// Initialize a new SPL token mint, return its instance address.
    pub async fn create_mint(&mut self, mint_authority: Pubkey) -> Pubkey {
        let mint = self.deterministic_keypair.new_keypair();

        let rent = self.context.banks_client.get_rent().await.unwrap();
        let mint_rent = rent.minimum_balance(spl_token::state::Mint::LEN);

        let payer = self.context.payer.pubkey();
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[
                system_instruction::create_account(
                    &payer,
                    &mint.pubkey(),
                    mint_rent,
                    spl_token::state::Mint::LEN as u64,
                    &spl_token::id(),
                ),
                spl_token::instruction::initialize_mint(
                    &spl_token::id(),
                    &mint.pubkey(),
                    &mint_authority,
                    None,
                    0,
                )
                .unwrap(),
            ],
            vec![&mint],
        )
        .await
        .expect("Failed to create SPL token mint.");

        mint.pubkey()
    }

    /// Create a new SPL token account holding stSOL, return its address.
    pub async fn create_st_sol_account(&mut self, owner: Pubkey) -> Pubkey {
        let rent = self.context.banks_client.get_rent().await.unwrap();
        let account_rent = rent.minimum_balance(spl_token::state::Account::LEN);
        let account = self.deterministic_keypair.new_keypair();

        let payer = self.context.payer.pubkey();
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[
                system_instruction::create_account(
                    &payer,
                    &account.pubkey(),
                    account_rent,
                    spl_token::state::Account::LEN as u64,
                    &spl_token::id(),
                ),
                spl_token::instruction::initialize_account(
                    &spl_token::id(),
                    &account.pubkey(),
                    &self.st_sol_mint,
                    &owner,
                )
                .unwrap(),
            ],
            vec![&account],
        )
        .await
        .expect("Failed to create token account.");

        account.pubkey()
    }

    /// Create an initialized but undelegated stake account (outside of Solido).
    pub async fn create_stake_account(
        &mut self,
        fund_amount: Lamports,
        authorized_staker_withdrawer: Pubkey,
    ) -> Pubkey {
        use solana_program::stake::instruction as stake;
        use solana_program::stake::state::{Authorized, Lockup};

        let keypair = self.deterministic_keypair.new_keypair();

        let instructions = stake::create_account(
            &self.context.payer.pubkey(),
            &keypair.pubkey(),
            &Authorized {
                staker: authorized_staker_withdrawer,
                withdrawer: authorized_staker_withdrawer,
            },
            &Lockup::default(),
            fund_amount.0,
        );
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &instructions[..],
            vec![&keypair],
        )
        .await
        .expect("Failed to initialize stake account.");

        keypair.pubkey()
    }

    /// Delegate a stake account, outside of Solido.
    pub async fn delegate_stake_account(
        &mut self,
        stake_account: Pubkey,
        vote_account: Pubkey,
        authorized_staker: &Keypair,
    ) {
        use solana_program::stake::instruction as stake;
        let instr =
            stake::delegate_stake(&stake_account, &authorized_staker.pubkey(), &vote_account);
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instr],
            vec![authorized_staker],
        )
        .await
        .expect("Failed to delegate stake.");
    }

    /// Merge two stake accounts, outside of Solido.
    ///
    /// The authorized staker and withdrawer must be the same for both accounts.
    pub async fn merge_stake_accounts(
        &mut self,
        source: Pubkey,
        destination: Pubkey,
        authorized_staker_withdrawer: &Keypair,
    ) {
        use solana_program::stake::instruction as stake;
        let instructions = stake::merge(
            &destination,
            &source,
            &authorized_staker_withdrawer.pubkey(),
        );
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &instructions,
            vec![authorized_staker_withdrawer],
        )
        .await
        .expect("Failed to merge stake.");
    }

    /// Deactivate a stake account, outside of Solido.
    pub async fn deactivate_stake_account(
        &mut self,
        stake_account: Pubkey,
        authorized_staker: &Keypair,
    ) {
        use solana_program::stake::instruction as stake;
        let instruction = stake::deactivate_stake(&stake_account, &authorized_staker.pubkey());
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction],
            vec![authorized_staker],
        )
        .await
        .expect("Failed to deactivate stake.");
    }

    /// Create a vote account for the given validator.
    pub async fn create_vote_account(&mut self, node_key: &Keypair) -> Pubkey {
        let rent = self.context.banks_client.get_rent().await.unwrap();
        let rent_voter = rent.minimum_balance(VoteState::size_of());

        let initial_balance = Lamports(42);
        let size_bytes = 0;

        let vote_account = self.deterministic_keypair.new_keypair();

        let payer = self.context.payer.pubkey();
        let mut instructions = vec![system_instruction::create_account(
            &payer,
            &node_key.pubkey(),
            initial_balance.0,
            size_bytes,
            &system_program::id(),
        )];

        instructions.extend(vote_instruction::create_account(
            &payer,
            &vote_account.pubkey(),
            &VoteInit {
                node_pubkey: node_key.pubkey(),
                authorized_voter: node_key.pubkey(),
                authorized_withdrawer: self.withdraw_authority,
                commission: 100,
            },
            rent_voter,
        ));
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &instructions,
            vec![node_key, &vote_account],
        )
        .await
        .expect("Failed to create vote account.");
        vote_account.pubkey()
    }

    /// Make `amount` appear in the recipient's account by transferring it from the context's funder.
    pub async fn fund(&mut self, recipient: Pubkey, amount: Lamports) {
        // Prevent test authors from shooting themselves in their feet by not
        // allowing to leave an account non-rent-exempt, because such accounts
        // might or might not be gone after this function returns, depending on
        // the current epoch and slot, which is very unexpected.
        let rent = self
            .context
            .banks_client
            .get_rent()
            .await
            .expect("Failed to get rent.");
        let min_balance = Lamports(rent.minimum_balance(0));
        let current_balance = self.get_sol_balance(recipient).await;
        if (current_balance + amount).unwrap() < min_balance {
            panic!(
                "You are trying to fund {} with {}, but that would not make the \
                account rent-exempt, it needs at least {} for that.",
                recipient, amount, min_balance,
            )
        }

        let payer = self.context.payer.pubkey();
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[system_instruction::transfer(&payer, &recipient, amount.0)],
            vec![],
        )
        .await
        .unwrap_or_else(|_| panic!("Failed to transfer {} to {}.", amount, recipient));

        // Sanity check to confirm that the account is still there. It should
        // not have been rent-collected, because we enforced that we made it
        // rent-exempt.
        let balance = self.get_sol_balance(recipient).await;
        assert!(
            balance >= amount,
            "Just funded {} with {} but now the balance is {}.",
            recipient,
            amount,
            balance
        );
    }

    pub async fn try_add_maintainer(&mut self, maintainer: Pubkey) -> transport::Result<()> {
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[lido::instruction::add_maintainer(
                &id(),
                &lido::instruction::AddMaintainerMeta {
                    lido: self.solido.pubkey(),
                    manager: self.manager.pubkey(),
                    maintainer: maintainer,
                },
            )],
            vec![&self.manager],
        )
        .await
    }

    /// Create a new key pair and add it as maintainer.
    pub async fn add_maintainer(&mut self) -> Keypair {
        let maintainer = self.deterministic_keypair.new_keypair();
        self.try_add_maintainer(maintainer.pubkey())
            .await
            .expect("Failed to add maintainer.");
        maintainer
    }

    pub async fn try_remove_maintainer(&mut self, maintainer: Pubkey) -> transport::Result<()> {
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[lido::instruction::remove_maintainer(
                &id(),
                &lido::instruction::RemoveMaintainerMeta {
                    lido: self.solido.pubkey(),
                    manager: self.manager.pubkey(),
                    maintainer: maintainer,
                },
            )],
            vec![&self.manager],
        )
        .await
    }

    pub async fn try_add_validator(
        &mut self,
        accounts: &ValidatorAccounts,
    ) -> transport::Result<()> {
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[lido::instruction::add_validator(
                &id(),
                &lido::instruction::AddValidatorMeta {
                    lido: self.solido.pubkey(),
                    manager: self.manager.pubkey(),
                    validator_vote_account: accounts.vote_account,
                    validator_fee_st_sol_account: accounts.fee_account,
                },
            )],
            vec![&self.manager],
        )
        .await
    }

    /// Create a new key pair and add it as maintainer.
    pub async fn add_validator(&mut self) -> ValidatorAccounts {
        let node_account = self.deterministic_keypair.new_keypair();
        let fee_account = self.create_st_sol_account(node_account.pubkey()).await;
        let vote_account = self.create_vote_account(&node_account).await;

        let accounts = ValidatorAccounts {
            node_account,
            vote_account,
            fee_account,
        };

        self.try_add_validator(&accounts)
            .await
            .expect("Failed to add validator.");

        accounts
    }

    pub async fn try_remove_validator(&mut self, vote_account: Pubkey) -> transport::Result<()> {
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[lido::instruction::remove_validator(
                &id(),
                &lido::instruction::RemoveValidatorMeta {
                    lido: self.solido.pubkey(),
                    manager: self.manager.pubkey(),
                    validator_vote_account_to_remove: vote_account,
                },
            )],
            vec![&self.manager],
        )
        .await
    }

    /// Create a new account, deposit from it, and return the resulting owner and stSOL account.
    pub async fn deposit(&mut self, amount: Lamports) -> (Keypair, Pubkey) {
        // Create a new user who is going to do the deposit. The user's account
        // will hold the SOL to deposit, and it will also be the owner of the
        // stSOL account that holds the proceeds.
        let user = self.deterministic_keypair.new_keypair();
        let recipient = self.create_st_sol_account(user.pubkey()).await;

        // Fund the user account, so the user can deposit that into Solido.
        self.fund(user.pubkey(), amount).await;

        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction::deposit(
                &id(),
                &instruction::DepositAccountsMeta {
                    lido: self.solido.pubkey(),
                    user: user.pubkey(),
                    recipient: recipient,
                    st_sol_mint: self.st_sol_mint,
                    reserve_account: self.reserve_address,
                    mint_authority: self.mint_authority,
                },
                amount,
            )],
            vec![&user],
        )
        .await
        .expect("Failed to call Deposit on Solido instance.");

        (user, recipient)
    }

    /// Withdraw from the validator at `self.validator`.
    pub async fn try_withdraw(
        &mut self,
        user: &Keypair,
        st_sol_account: Pubkey,
        amount: StLamports,
        source_stake_account: Pubkey,
    ) -> transport::Result<Pubkey> {
        // Where the new stake will live.
        let new_stake = self.deterministic_keypair.new_keypair();
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[
                // authorize_burn_instruction,
                instruction::withdraw(
                    &id(),
                    &instruction::WithdrawAccountsMeta {
                        lido: self.solido.pubkey(),
                        st_sol_mint: self.st_sol_mint,
                        st_sol_account_owner: user.pubkey(),
                        st_sol_account,
                        validator_vote_account: self.validator.as_ref().unwrap().vote_account,
                        source_stake_account,
                        destination_stake_account: new_stake.pubkey(),
                        stake_authority: self.stake_authority,
                    },
                    amount,
                ),
            ],
            vec![user, &new_stake],
        )
        .await?;
        Ok(new_stake.pubkey())
    }

    /// Withdraw from the validator at `self.validator`.
    pub async fn withdraw(
        &mut self,
        user: &Keypair,
        st_sol_account: Pubkey,
        amount: StLamports,
        source_stake_account: Pubkey,
    ) -> Pubkey {
        self.try_withdraw(user, st_sol_account, amount, source_stake_account)
            .await
            .expect("Failed to call Withdraw on Solido instance.")
    }

    /// Stake the given amount to the given validator, return the resulting stake account.
    pub async fn try_stake_deposit(
        &mut self,
        validator_vote_account: Pubkey,
        approach: StakeDeposit,
        amount: Lamports,
    ) -> transport::Result<Pubkey> {
        let solido = self.get_solido().await;

        let validator_entry = solido
            .validators
            .get(&validator_vote_account)
            .expect("Trying to stake with a non-member validator.");

        let (stake_account_end, _) = Validator::find_stake_account_address(
            &id(),
            &self.solido.pubkey(),
            &validator_vote_account,
            validator_entry.entry.stake_accounts_seed_end,
        );

        let (stake_account_merge_into, _) = Validator::find_stake_account_address(
            &id(),
            &self.solido.pubkey(),
            &validator_vote_account,
            match approach {
                StakeDeposit::Append => validator_entry.entry.stake_accounts_seed_end,
                // We do a wrapping sub here, so we can call stake-merge initially,
                // when end is 0, such that the account to merge into is not the
                // same as the end account.
                StakeDeposit::Merge => validator_entry
                    .entry
                    .stake_accounts_seed_end
                    .wrapping_sub(1),
            },
        );

        let maintainer = self
            .maintainer
            .as_ref()
            .expect("Must have maintainer to call StakeDeposit.");

        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction::stake_deposit(
                &id(),
                &instruction::StakeDepositAccountsMeta {
                    lido: self.solido.pubkey(),
                    maintainer: maintainer.pubkey(),
                    validator_vote_account,
                    reserve: self.reserve_address,
                    stake_account_merge_into,
                    stake_account_end,
                    stake_authority: self.stake_authority,
                },
                amount,
            )],
            vec![maintainer],
        )
        .await?;

        Ok(stake_account_end)
    }

    /// Stake the given amount to the given validator, return the resulting stake account.
    pub async fn stake_deposit(
        &mut self,
        validator_vote_account: Pubkey,
        approach: StakeDeposit,
        amount: Lamports,
    ) -> Pubkey {
        self.try_stake_deposit(validator_vote_account, approach, amount)
            .await
            .expect("Failed to call StakeDeposit on Solido instance.")
    }

    pub async fn try_change_reward_distribution(
        &mut self,
        new_reward_distribution: &RewardDistribution,
        new_fee_recipients: &FeeRecipients,
    ) -> transport::Result<()> {
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction::change_reward_distribution(
                &id(),
                new_reward_distribution.clone(),
                &instruction::ChangeRewardDistributionMeta {
                    lido: self.solido.pubkey(),
                    manager: self.manager.pubkey(),
                    treasury_account: new_fee_recipients.treasury_account,
                    developer_account: new_fee_recipients.developer_account,
                },
            )],
            vec![&self.manager],
        )
        .await
    }

    pub async fn try_update_exchange_rate(&mut self) -> transport::Result<()> {
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction::update_exchange_rate(
                &id(),
                &instruction::UpdateExchangeRateAccountsMeta {
                    lido: self.solido.pubkey(),
                    reserve: self.reserve_address,
                    st_sol_mint: self.st_sol_mint,
                },
            )],
            vec![],
        )
        .await
    }

    pub async fn update_exchange_rate(&mut self) {
        self.try_update_exchange_rate()
            .await
            .expect("Failed to update exchange rate.");
    }

    /// Merge two accounts of a given validator.
    ///
    /// Returns the address that stake was merged into.
    pub async fn try_merge_stake(
        &mut self,
        validator_vote_account: Pubkey,
        from_seed: u64,
        to_seed: u64,
    ) -> transport::Result<Pubkey> {
        let (from_stake_account, _) = Validator::find_stake_account_address(
            &id(),
            &self.solido.pubkey(),
            &validator_vote_account,
            from_seed,
        );

        let (to_stake_account, _) = Validator::find_stake_account_address(
            &id(),
            &self.solido.pubkey(),
            &validator_vote_account,
            to_seed,
        );

        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction::merge_stake(
                &id(),
                &instruction::MergeStakeMeta {
                    lido: self.solido.pubkey(),
                    validator_vote_account: validator_vote_account,
                    stake_authority: self.stake_authority,
                    from_stake: from_stake_account,
                    to_stake: to_stake_account,
                },
            )],
            vec![],
        )
        .await?;

        Ok(to_stake_account)
    }

    /// Merge two accounts of a given validator.
    pub async fn merge_stake(
        &mut self,
        validator_vote_account: Pubkey,
        from_seed: u64,
        to_seed: u64,
    ) -> Pubkey {
        self.try_merge_stake(validator_vote_account, from_seed, to_seed)
            .await
            .expect("Failed to call MergeStake on Solido instance.")
    }

    /// Observe the new validator balance and write it to the state,
    /// distribute any rewards received.
    pub async fn try_withdraw_inactive_stake(
        &mut self,
        validator_vote_account: Pubkey,
    ) -> transport::Result<()> {
        let solido = self.get_solido().await;
        let validator = solido.validators.get(&validator_vote_account).unwrap();
        let begin = validator.entry.stake_accounts_seed_begin;
        let end = validator.entry.stake_accounts_seed_end;

        let stake_account_addrs: Vec<Pubkey> = (begin..end)
            .map(|seed| {
                Validator::find_stake_account_address(
                    &id(),
                    &self.solido.pubkey(),
                    &validator_vote_account,
                    seed,
                )
                .0
            })
            .collect();

        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction::withdraw_inactive_stake(
                &id(),
                &instruction::WithdrawInactiveStakeMeta {
                    lido: self.solido.pubkey(),
                    validator_vote_account: validator_vote_account,
                    stake_accounts: stake_account_addrs,
                    reserve: self.reserve_address,
                    stake_authority: self.stake_authority,
                },
            )],
            vec![],
        )
        .await
    }

    pub async fn withdraw_inactive_stake(&mut self, validator_vote_account: Pubkey) {
        self.try_withdraw_inactive_stake(validator_vote_account)
            .await
            .expect("Failed to withdraw inactive stake.");
    }

    /// Observe the new validator balance and write it to the state,
    /// distribute any rewards received.
    pub async fn try_collect_validator_fee(
        &mut self,
        validator_vote_account: Pubkey,
    ) -> transport::Result<Lamports> {
        let solido = self.get_solido().await;
        let reserve_balance_before = self.get_sol_balance(self.reserve_address).await;
        let rewards_withdraw_authority = solido
            .get_rewards_withdraw_authority(&id(), &self.solido.pubkey())
            .unwrap();
        let vote_account = self.get_account(validator_vote_account).await;
        let vote_account_rent = self
            .get_rent()
            .await
            .minimum_balance(vote_account.data.len());
        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction::collect_validator_fee(
                &id(),
                &instruction::CollectValidatorFeeMeta {
                    lido: self.solido.pubkey(),
                    validator_vote_account: validator_vote_account,
                    st_sol_mint: self.st_sol_mint,
                    mint_authority: self.mint_authority,
                    treasury_st_sol_account: self.treasury_st_sol_account,
                    developer_st_sol_account: self.developer_st_sol_account,
                    reserve: self.reserve_address,
                    rewards_withdraw_authority,
                },
            )],
            vec![],
        )
        .await?;
        let reserve_balance_after = self.get_sol_balance(self.reserve_address).await;
        let vote_account = self.get_account(validator_vote_account).await;
        assert_eq!(vote_account.lamports(), vote_account_rent);

        Ok((reserve_balance_after - reserve_balance_before)
            .expect("Reserve balance should have increased after validator fee collection."))
    }

    pub async fn collect_validator_fee(&mut self, validator_vote_account: Pubkey) -> Lamports {
        self.try_collect_validator_fee(validator_vote_account)
            .await
            .expect("Failed to collect validator fee.")
    }

    /// Claim validator fee and return the amount claimed.
    pub async fn try_claim_validator_fee(
        &mut self,
        validator_vote_account: Pubkey,
    ) -> transport::Result<StLamports> {
        let solido_before = self.get_solido().await;
        let validator_before = solido_before
            .validators
            .get(&validator_vote_account)
            .unwrap();

        let validator_balance_before = self
            .get_st_sol_balance(validator_before.entry.fee_address)
            .await;

        send_transaction(
            &mut self.context,
            &mut self.nonce,
            &[instruction::claim_validator_fee(
                &id(),
                &instruction::ClaimValidatorFeeMeta {
                    lido: self.solido.pubkey(),
                    validator_fee_st_sol_account: validator_before.entry.fee_address,
                    st_sol_mint: self.st_sol_mint,
                    mint_authority: self.mint_authority,
                },
            )],
            vec![],
        )
        .await?;
        let solido_after = self.get_solido().await;
        let validator_after = solido_after
            .validators
            .get(&validator_vote_account)
            .unwrap();
        // Assert all the credits are taken from.
        assert_eq!(validator_after.entry.fee_credit, StLamports(0));

        let validator_balance_after = self
            .get_st_sol_balance(validator_before.entry.fee_address)
            .await;

        // Assert validator's balance increased by the `fee_credit`.
        assert_eq!(
            validator_balance_after,
            (validator_balance_before + validator_before.entry.fee_credit).unwrap()
        );

        Ok(validator_before.entry.fee_credit)
    }

    pub async fn claim_validator_fee(&mut self, validator_vote_account: Pubkey) -> StLamports {
        self.try_claim_validator_fee(validator_vote_account)
            .await
            .expect("Failed to claim validator fee.")
    }

    pub async fn try_get_account(&mut self, address: Pubkey) -> Option<Account> {
        self.context
            .banks_client
            .get_account(address)
            .await
            .expect("Failed to get account, why does this happen in tests?")
    }

    pub async fn try_get_sol_balance(&mut self, address: Pubkey) -> Option<Lamports> {
        self.context
            .banks_client
            .get_balance(address)
            .await
            .ok()
            .map(Lamports)
    }

    pub async fn get_account(&mut self, address: Pubkey) -> Account {
        self.try_get_account(address)
            .await
            .unwrap_or_else(|| panic!("Account {} does not exist.", address))
    }

    pub async fn get_sol_balance(&mut self, address: Pubkey) -> Lamports {
        self.try_get_sol_balance(address)
            .await
            .unwrap_or_else(|| panic!("Account {} does not exist.", address))
    }

    pub async fn get_st_sol_balance(&mut self, address: Pubkey) -> StLamports {
        let token_account = self.get_account(address).await;
        let account_info: spl_token::state::Account =
            spl_token::state::Account::unpack_from_slice(token_account.data.as_slice()).unwrap();

        assert_eq!(account_info.mint, self.st_sol_mint);

        StLamports(account_info.amount)
    }

    pub async fn get_solido(&mut self) -> Lido {
        let lido_account = self.get_account(self.solido.pubkey()).await;
        // This returns a Result because it can cause an IO error, but that should
        // not happen in the test environment. (And if it does, then the test just
        // fails.)
        try_from_slice_unchecked::<Lido>(lido_account.data.as_slice()).unwrap()
    }

    pub async fn get_rent(&mut self) -> Rent {
        self.context
            .banks_client
            .get_rent()
            .await
            .expect("Failed to get rent.")
    }
    pub async fn get_clock(&mut self) -> Clock {
        let account = self
            .context
            .banks_client
            .get_account(sysvar::clock::id())
            .await
            .expect("Clock account should exist.")
            .expect("Clock account should exist.");
        from_account(&account).expect("Could not get Clock from account.")
    }
    pub async fn get_stake_history(&mut self) -> StakeHistory {
        let account = self
            .context
            .banks_client
            .get_account(sysvar::stake_history::id())
            .await
            .expect("Stake History account should exist.")
            .expect("Stake History account should exist.");
        from_account(&account).expect("Could not get Stake History from account.")
    }

    pub async fn get_stake_state(&mut self, stake_account: Pubkey) -> Stake {
        let account = self.get_account(stake_account).await;
        lido::stake_account::deserialize_stake_account(&account.data).unwrap()
    }

    pub async fn get_stake_rent_exempt_reserve(&mut self, stake_account: Pubkey) -> Lamports {
        let account = self.get_account(stake_account).await;
        lido::stake_account::deserialize_rent_exempt_reserve(&account.data).unwrap()
    }
}

/// Return an `AccountInfo` for the given account, with `is_signer` and `is_writable` set to false.
pub fn get_account_info<'a>(address: &'a Pubkey, account: &'a mut Account) -> AccountInfo<'a> {
    let is_signer = false;
    let is_writable = false;
    let is_executable = false;
    AccountInfo::new(
        address,
        is_signer,
        is_writable,
        &mut account.lamports,
        &mut account.data,
        &account.owner,
        is_executable,
        account.rent_epoch,
    )
}

#[macro_export]
macro_rules! assert_solido_error {
    ($result:expr, $error:expr $(, /* Accept an optional trailing comma. */)?) => {
        // Open a scope so the imports don't clash.
        {
            use solana_program::instruction::InstructionError;
            use solana_sdk::transaction::TransactionError;
            use solana_sdk::transport::TransportError;
            match $result {
                Err(TransportError::TransactionError(TransactionError::InstructionError(
                    _,
                    InstructionError::Custom(error_code),
                ))) => assert_eq!(
                    error_code,
                    $error as u32,
                    "Expected custom error with code for {}, got different code.",
                    stringify!($error)
                ),
                unexpected => panic!(
                    "Expected {} error, not {:?}",
                    stringify!($error),
                    unexpected
                ),
            }
        }
    };
}

/// Like `assert_solido_error`, but instead of testing for a Solido error, it tests
/// for a raw error code. Can be used to test for errors returned by different programs.
#[macro_export]
macro_rules! assert_error_code {
    ($result:expr, $error_code:expr $(, /* Accept an optional trailing comma. */)?) => {
        // Open a scope so the imports don't clash.
        {
            use solana_program::instruction::InstructionError;
            use solana_sdk::transaction::TransactionError;
            use solana_sdk::transport::TransportError;
            match $result {
                Err(TransportError::TransactionError(TransactionError::InstructionError(
                    _,
                    InstructionError::Custom(error_code),
                ))) => assert_eq!(
                    error_code, $error_code as u32,
                    "Custom error has an unexpected error code.",
                ),
                unexpected => panic!(
                    "Expected custom error with code {} error, not {:?}",
                    $error_code, unexpected
                ),
            }
        }
    };
}
