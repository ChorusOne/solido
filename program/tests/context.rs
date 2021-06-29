//! Holds a test context, which makes it easier to test with a Solido instance set up.

use num_traits::cast::FromPrimitive;
use solana_program::borsh::try_from_slice_unchecked;
use solana_program::instruction::Instruction;
use solana_program::instruction::InstructionError;
use solana_program::program_pack::Pack;
use solana_program::rent::Rent;
use solana_program::system_instruction;
use solana_program::system_program;
use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use solana_sdk::account::Account;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use solana_sdk::transaction::TransactionError;
use solana_sdk::transport;
use solana_sdk::transport::TransportError;
use solana_vote_program::vote_instruction;
use solana_vote_program::vote_state::{VoteInit, VoteState};

use lido::error::LidoError;
use lido::state::{FeeRecipients, Lido, RewardDistribution, Validator};
use lido::token::{Lamports, StLamports};
use lido::{instruction, RESERVE_AUTHORITY, STAKE_AUTHORITY};
use spl_stake_pool::stake_program::{Meta, Stake, StakeState};

// This id is only used throughout these tests.
solana_program::declare_id!("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc");

pub struct Context {
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

impl Context {
    /// Set up a new test context with an initialized Solido instance.
    ///
    /// The instance contains no maintainers yet.
    pub async fn new_empty() -> Context {
        let manager = Keypair::new();
        let solido = Keypair::new();

        let reward_distribution = RewardDistribution {
            treasury_fee: 3,
            validation_fee: 4,
            developer_fee: 3,
            st_sol_appreciation: 90,
        };

        let (reserve_address, _) = Pubkey::find_program_address(
            &[&solido.pubkey().to_bytes()[..], RESERVE_AUTHORITY],
            &id(),
        );

        let (stake_authority, _) = Pubkey::find_program_address(
            &[&solido.pubkey().to_bytes()[..], STAKE_AUTHORITY],
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
        };

        result.st_sol_mint = result.create_mint(result.reserve_address).await;

        let treasury_owner = Keypair::new();
        result.treasury_st_sol_account =
            result.create_st_sol_account(treasury_owner.pubkey()).await;

        let developer_owner = Keypair::new();
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
                )
                .unwrap(),
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
                .stake_deposit(validator.vote_account, Lamports(10_000_000_000))
                .await;

            stake_accounts.push(stake_account);
        }
        result.validator = Some(validator);
        (result, stake_accounts)
    }

    /// Initialize a new SPL token mint, return its instance address.
    pub async fn create_mint(&mut self, mint_authority: Pubkey) -> Pubkey {
        let mint = Keypair::new();

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
        let account = Keypair::new();

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

    /// Create a vote account for the given validator.
    pub async fn create_vote_account(&mut self, node_key: &Keypair) -> Pubkey {
        let rent = self.context.banks_client.get_rent().await.unwrap();
        let rent_voter = rent.minimum_balance(VoteState::size_of());

        let initial_balance = Lamports(42);
        let size_bytes = 0;

        let vote_account = Keypair::new();

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
                ..VoteInit::default()
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
            )
            .unwrap()],
            vec![&self.manager],
        )
        .await
    }

    /// Create a new key pair and add it as maintainer.
    pub async fn add_maintainer(&mut self) -> Keypair {
        let maintainer = Keypair::new();
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
            )
            .unwrap()],
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
            )
            .unwrap()],
            vec![&self.manager],
        )
        .await
    }

    /// Create a new key pair and add it as maintainer.
    pub async fn add_validator(&mut self) -> ValidatorAccounts {
        let node_account = Keypair::new();
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
            )
            .unwrap()],
            vec![&self.manager],
        )
        .await
    }

    /// Create a new account, deposit from it, and return the resulting stSOL account.
    pub async fn deposit(&mut self, amount: Lamports) -> Pubkey {
        // Create a new user who is going to do the deposit. The user's account
        // will hold the SOL to deposit, and it will also be the owner of the
        // stSOL account that holds the proceeds.
        let user = Keypair::new();
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
                },
                amount,
            )
            .unwrap()],
            vec![&user],
        )
        .await
        .expect("Failed to call Deposit on Solido instance.");

        recipient
    }

    /// Stake the given amount to the given validator, return the resulting stake account.
    pub async fn try_stake_deposit(
        &mut self,
        validator_vote_account: Pubkey,
        amount: Lamports,
    ) -> transport::Result<Pubkey> {
        let solido = self.get_solido().await;

        let validator_entry = solido
            .validators
            .get(&validator_vote_account)
            .expect("Trying to stake with a non-member validator.");

        let (stake_account, _) = Validator::find_stake_account_address(
            &id(),
            &self.solido.pubkey(),
            &validator_vote_account,
            validator_entry.entry.stake_accounts_seed_end,
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
                    validator_vote_account: validator_vote_account,
                    reserve: self.reserve_address,
                    stake_account_end: stake_account,
                    stake_authority: self.stake_authority,
                },
                amount,
            )
            .unwrap()],
            vec![maintainer],
        )
        .await?;

        Ok(stake_account)
    }

    /// Stake the given amount to the given validator, return the resulting stake account.
    pub async fn stake_deposit(
        &mut self,
        validator_vote_account: Pubkey,
        amount: Lamports,
    ) -> Pubkey {
        self.try_stake_deposit(validator_vote_account, amount)
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

    /// Merge two accounts of a given validator.
    pub async fn try_merge_stake(
        &mut self,
        validator_vote_account: Pubkey,
        from_seed: u64,
        to_seed: u64,
    ) -> transport::Result<()> {
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
                from_seed,
                to_seed,
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
        .await
    }

    pub async fn update_exchange_rate(&mut self) {
        self.try_update_exchange_rate()
            .await
            .expect("Failed to update exchange rate.");
    }

    /// Merge two accounts of a given validator.
    pub async fn merge_stake(
        &mut self,
        validator_vote_account: Pubkey,
        from_seed: u64,
        to_seed: u64,
    ) {
        self.try_merge_stake(validator_vote_account, from_seed, to_seed)
            .await
            .expect("Failed to call MergeStake on Solido instance.")
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
    pub async fn get_stake_state(&mut self, stake_account: Pubkey) -> (Meta, Stake) {
        let account = self.get_account(stake_account).await;
        let stake_state = try_from_slice_unchecked::<StakeState>(&account.data).unwrap();
        if let StakeState::Stake(meta, stake) = stake_state {
            (meta, stake)
        } else {
            panic!("Stake state should have been StakeState::Stake.");
        }
    }
}

#[macro_export]
macro_rules! assert_solido_error {
    ($result:expr, $error:expr) => {
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