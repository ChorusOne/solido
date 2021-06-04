use std::fmt;

use anchor_client::{Client, Cluster, Program};
use anchor_lang::AnchorDeserialize;
use clap::Clap;
use lido::{
    state::{FeeDistribution, Lido},
    DEPOSIT_AUTHORITY, FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_AUTHORITY,
};
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_program::{pubkey::Pubkey, system_instruction};
use solana_sdk::{
    borsh::try_from_slice_unchecked,
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    signature::{Keypair, Signer},
    signer::signers::Signers,
    transaction::Transaction,
};
use spl_stake_pool::{
    find_stake_program_address,
    state::{Fee, StakePool},
};

use crate::{
    multisig::{get_multisig_program_address, propose_instruction, ProposeInstructionOutput},
    spl_token_utils::{push_create_spl_token_account, push_create_spl_token_mint},
    stake_pool_helpers::{command_create_pool, CreatePoolOutput},
    util::PubkeyBase58,
    Config, OutputMode,
};

const STAKE_POOL_WITHDRAW_AUTHORITY_ID: &[u8] = b"withdraw";

pub fn send_transaction(
    config: &Config,
    transaction: Transaction,
) -> solana_client::client_error::Result<()> {
    if config.dry_run {
        config.rpc().simulate_transaction(&transaction)?;
    } else {
        let _signature = match config.output_mode {
            OutputMode::Text => {
                // In text mode, we can display a spinner.
                config
                    .program
                    .rpc()
                    .send_and_confirm_transaction_with_spinner(&transaction)?
            }
            OutputMode::Json => {
                // In json mode, printing a spinner to stdout would break the
                // json that we also print to stdout, so opt for the silent
                // version.
                config
                    .program
                    .rpc()
                    .send_and_confirm_transaction(&transaction)?
            }
        };
    }
    Ok(())
}

pub fn sign_and_send_transaction<T: Signers>(
    config: &Config,
    instructions: &[Instruction],
    signers: &T,
) -> Result<(), crate::Error> {
    let mut tx = Transaction::new_with_payer(instructions, Some(&config.fee_payer.pubkey()));

    let (recent_blockhash, _fee_calculator) = config.rpc().get_recent_blockhash()?;
    tx.sign(signers, recent_blockhash);
    send_transaction(&config, tx)?;

    Ok(())
}

pub fn get_anchor_program(
    cluster: Cluster,
    payer: Keypair,
    multisig_program_id: &Pubkey,
) -> Program {
    let client = Client::new_with_options(cluster, payer, CommitmentConfig::confirmed());
    client.program(*multisig_program_id)
}

#[derive(Clap, Debug)]
pub struct CreateSolidoOpts {
    /// Address of the Solido program.
    #[clap(long, value_name = "address")]
    pub solido_program_id: Pubkey,

    /// Address of the SPL stake pool program.
    #[clap(long, value_name = "address")]
    pub stake_pool_program_id: Pubkey,

    /// Numerator of the fee fraction.
    #[clap(long, value_name = "int")]
    pub fee_numerator: u64,

    /// Denominator of the fee fraction.
    #[clap(long, value_name = "int")]
    pub fee_denominator: u64,

    /// The maximum number of validators that this Solido instance will support.
    #[clap(long, value_name = "int")]
    pub max_validators: u32,

    /// The maximum number of maintainers that this Solido instance will support.
    #[clap(long, value_name = "int")]
    pub max_maintainers: u32,

    // Fees are divided proportionally to the sum of all specified fees, for instance,
    // if all the fees are the same value, they will be divided equally.
    /// Insurance fee share
    #[clap(long, value_name = "int")]
    pub insurance_fee: u32,
    /// Treasury fee share
    #[clap(long, value_name = "int")]
    pub treasury_fee: u32,
    /// Validation fee share, to be divided equally among validators
    #[clap(long, value_name = "int")]
    pub validation_fee: u32,
    /// Manager fee share
    #[clap(long, value_name = "int")]
    pub manager_fee: u32,

    /// Account who will own the stSOL SPL token account that receives insurance fees.
    #[clap(long, value_name = "address")]
    pub insurance_account_owner: Pubkey,
    /// Account who will own the stSOL SPL token account that receives treasury fees.
    #[clap(long, value_name = "address")]
    pub treasury_account_owner: Pubkey,
    /// Account who will own the stSOL SPL token account that receives the manager fees.
    #[clap(long, value_name = "address")]
    pub manager_fee_account_owner: Pubkey,

    /// If manager is defined, creates an instance with the manager, otherwise
    /// use the default fee payer.
    #[clap(long, value_name = "address")]
    pub manager: Option<Pubkey>,
}

#[derive(Serialize)]
pub struct CreateSolidoOutput {
    /// Account that stores the data for this Solido instance.
    pub solido_address: PubkeyBase58,

    /// Manages the deposited sol and token minting.
    pub reserve_authority: PubkeyBase58,

    /// Owner of the `fee_address`.
    pub fee_authority: PubkeyBase58,

    /// Manager of the stake pool, derived program address owned by the Solido instance.
    pub stake_pool_authority: PubkeyBase58,

    /// SPL token mint account for StSol tokens.
    pub st_sol_mint_address: PubkeyBase58,

    /// The only depositor of the stake pool.
    pub pool_token_to: PubkeyBase58,

    /// stSOL SPL token account that holds the insurance funds.
    pub insurance_account: PubkeyBase58,

    /// stSOL SPL token account that holds the treasury funds.
    pub treasury_account: PubkeyBase58,

    /// stSOL SPL token account that receives the manager fees.
    pub manager_fee_account: PubkeyBase58,

    /// Details of the stake pool managed by Solido.
    pub stake_pool: CreatePoolOutput,
}

impl fmt::Display for CreateSolidoOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Solido details:")?;
        writeln!(
            f,
            "  Solido address:                {}",
            self.solido_address
        )?;
        writeln!(
            f,
            "  Reserve authority:             {}",
            self.reserve_authority
        )?;
        writeln!(f, "  Fee authority:                 {}", self.fee_authority)?;
        writeln!(
            f,
            "  Stake pool authority:          {}",
            self.stake_pool_authority
        )?;
        writeln!(
            f,
            "  stSOL mint:                    {}",
            self.st_sol_mint_address
        )?;
        writeln!(f, "  Solido's pool account:         {}", self.pool_token_to)?;
        writeln!(
            f,
            "  Insurance SPL token account:   {}",
            self.insurance_account
        )?;
        writeln!(
            f,
            "  Treasury SPL token account:    {}",
            self.treasury_account
        )?;
        writeln!(
            f,
            "  Manager fee SPL token account: {}",
            self.treasury_account
        )?;
        writeln!(f, "Stake pool details:\n{}", self.stake_pool)?;
        Ok(())
    }
}

pub fn command_create_solido(
    config: Config,
    opts: CreateSolidoOpts,
) -> Result<CreateSolidoOutput, crate::Error> {
    let lido_keypair = Keypair::new();

    let (reserve_authority, _) = lido::find_authority_program_address(
        &opts.solido_program_id,
        &lido_keypair.pubkey(),
        RESERVE_AUTHORITY,
    );

    let (fee_authority, _) = lido::find_authority_program_address(
        &opts.solido_program_id,
        &lido_keypair.pubkey(),
        FEE_MANAGER_AUTHORITY,
    );

    let (deposit_authority, _) = lido::find_authority_program_address(
        &opts.solido_program_id,
        &lido_keypair.pubkey(),
        DEPOSIT_AUTHORITY,
    );

    let (stake_pool_authority, _) = lido::find_authority_program_address(
        &opts.solido_program_id,
        &lido_keypair.pubkey(),
        STAKE_POOL_AUTHORITY,
    );

    let stake_pool = command_create_pool(
        &config,
        &opts.stake_pool_program_id,
        &stake_pool_authority,
        &deposit_authority,
        &fee_authority,
        &config.fee_payer,
        Fee {
            numerator: opts.fee_numerator,
            denominator: opts.fee_denominator,
        },
        opts.max_validators,
    )?;

    let lido_size = Lido::calculate_size(opts.max_validators, opts.max_maintainers);
    let lido_account_balance = config
        .program
        .rpc()
        .get_minimum_balance_for_rent_exemption(lido_size)?;

    let mut instructions = Vec::new();

    // We need to fund Lido's reserve account so it is rent-exempt, otherwise it
    // might disappear.
    let min_balance_empty_data_account = config
        .program
        .rpc()
        .get_minimum_balance_for_rent_exemption(0)?;
    instructions.push(system_instruction::transfer(
        &config.fee_payer.pubkey(),
        &reserve_authority,
        min_balance_empty_data_account,
    ));

    // Set up the Lido stSOL SPL token mint account.
    let st_sol_mint_keypair =
        push_create_spl_token_mint(&config, &mut instructions, &reserve_authority)?;

    // Ideally we would set up the entire instance in a single transaction, but
    // Solana transaction size limits are so low that we need to break our
    // instructions down into multiple transactions. So set up the mint first,
    // then continue.
    sign_and_send_transaction(
        &config,
        &instructions[..],
        &[&config.fee_payer, &st_sol_mint_keypair],
    )?;
    instructions.clear();
    eprintln!("Did send mint init.");

    // Set up the SPL token account that holds Lido's stake pool tokens.
    let pool_token_to_keypair = push_create_spl_token_account(
        &config,
        &mut instructions,
        &stake_pool.mint_address.0,
        &stake_pool_authority,
    )?;

    sign_and_send_transaction(
        &config,
        &instructions[..],
        &vec![&config.fee_payer, &pool_token_to_keypair],
    )?;
    instructions.clear();
    eprintln!("Did send SPL account inits part 1.");

    // Set up the SPL token account that receive the fees in stSOL.
    let insurance_keypair = push_create_spl_token_account(
        &config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        &opts.insurance_account_owner,
    )?;
    let treasury_keypair = push_create_spl_token_account(
        &config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        &opts.treasury_account_owner,
    )?;
    let manager_fee_keypair = push_create_spl_token_account(
        &config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        &opts.manager_fee_account_owner,
    )?;
    sign_and_send_transaction(
        &config,
        &instructions[..],
        &vec![
            &config.fee_payer,
            &insurance_keypair,
            &treasury_keypair,
            &manager_fee_keypair,
        ],
    )?;
    instructions.clear();
    eprintln!("Did send SPL account inits.");

    // Create the account that holds the Solido instance itself.
    instructions.push(system_instruction::create_account(
        &config.fee_payer.pubkey(),
        &lido_keypair.pubkey(),
        lido_account_balance,
        lido_size as u64,
        &opts.solido_program_id,
    ));

    instructions.push(lido::instruction::initialize(
        &opts.solido_program_id,
        FeeDistribution {
            insurance_fee: opts.insurance_fee,
            treasury_fee: opts.treasury_fee,
            validation_fee: opts.validation_fee,
            manager_fee: opts.manager_fee,
        },
        opts.max_validators,
        opts.max_maintainers,
        &lido::instruction::InitializeAccountsMeta {
            lido: lido_keypair.pubkey(),
            stake_pool: stake_pool.stake_pool_address.0,
            mint_program: st_sol_mint_keypair.pubkey(),
            pool_token_to: pool_token_to_keypair.pubkey(),
            fee_token: stake_pool.fee_address.0,
            manager: opts.manager.unwrap_or(config.fee_payer.pubkey()),
            insurance_account: insurance_keypair.pubkey(),
            treasury_account: treasury_keypair.pubkey(),
            manager_fee_account: manager_fee_keypair.pubkey(),
            reserve_account: reserve_authority,
        },
    )?);

    sign_and_send_transaction(
        &config,
        &instructions[..],
        &[&config.fee_payer, &lido_keypair],
    )?;
    eprintln!("Did send Lido init.");

    let result = CreateSolidoOutput {
        solido_address: lido_keypair.pubkey().into(),
        reserve_authority: reserve_authority.into(),
        fee_authority: fee_authority.into(),
        stake_pool_authority: stake_pool_authority.into(),
        st_sol_mint_address: st_sol_mint_keypair.pubkey().into(),
        pool_token_to: pool_token_to_keypair.pubkey().into(),
        insurance_account: insurance_keypair.pubkey().into(),
        treasury_account: treasury_keypair.pubkey().into(),
        manager_fee_account: manager_fee_keypair.pubkey().into(),
        stake_pool,
    };
    Ok(result)
}

#[derive(Clap, Debug)]
pub struct AddValidatorOpts {
    /// Address of the Solido program.
    #[clap(long, value_name = "address")]
    pub solido_program_id: Pubkey,
    /// Account that stores the data for this Solido instance.
    #[clap(long, value_name = "address")]
    pub solido_address: Pubkey,

    /// Stake pool program id.
    #[clap(long, value_name = "address")]
    stake_pool_program_id: Pubkey,
    /// Address of the validator vote account.
    #[clap(long, value_name = "address")]
    pub validator_vote: Pubkey,
    /// Validator stSol token account.
    #[clap(long, value_name = "address")]
    pub validator_rewards_address: Pubkey,

    // TDOO(Ruud): Maybe move this to the previous (general) arguments passed to the program.
    /// Issue commands through the passed multisig account.
    #[clap(long, requires = "multisig-program-id", value_name = "address")]
    pub multisig_address: Option<Pubkey>,
    /// When issuing commands Multisig program id.
    #[clap(long, requires = "multisig-address", value_name = "address")]
    pub multisig_program_id: Option<Pubkey>,
}

/// Command to add a validator to Solido.
pub fn command_add_validator(
    config: Config,
    cluster: Cluster,
    opts: AddValidatorOpts,
) -> Result<Option<ProposeInstructionOutput>, crate::Error> {
    let solido = get_solido(&config.rpc(), &opts.solido_address)?;
    let stake_pool = get_stake_pool(&config.rpc(), &solido.stake_pool_account)?;

    let (stake_pool_authority, _) = lido::find_authority_program_address(
        &opts.solido_program_id,
        &opts.solido_address,
        STAKE_POOL_AUTHORITY,
    );

    let stake_pool_withdraw_authority = Pubkey::create_program_address(
        &[
            &solido.stake_pool_account.to_bytes()[..],
            STAKE_POOL_WITHDRAW_AUTHORITY_ID,
            &[stake_pool.withdraw_bump_seed],
        ],
        &opts.stake_pool_program_id,
    )?;

    let (stake_account, _) = find_stake_program_address(
        &opts.stake_pool_program_id,
        &opts.validator_vote,
        &solido.stake_pool_account,
    );

    let execution_method = get_execution_method(
        config.fee_payer.pubkey(),
        opts.multisig_program_id,
        opts.multisig_address,
    );
    let instruction = lido::instruction::add_validator(
        &opts.solido_program_id,
        &lido::instruction::AddValidatorMeta {
            lido: opts.solido_address,
            manager: execution_method.get_pubkey(),
            stake_pool_manager_authority: stake_pool_authority,
            stake_pool_program: opts.stake_pool_program_id,
            stake_pool: solido.stake_pool_account,
            stake_pool_withdraw_authority,
            stake_pool_validator_list: stake_pool.validator_list,
            stake_account,
            validator_token_account: opts.validator_rewards_address,
        },
    )?;
    execution_method.send_instruction(
        cluster,
        config,
        opts.multisig_program_id,
        opts.multisig_address,
        instruction,
    )
}
#[derive(Clap, Debug)]
pub struct AddRemoveMaintainerOpts {
    /// Address of the Solido program.
    #[clap(long, value_name = "address")]
    pub solido_program_id: Pubkey,
    /// Account that stores the data for this Solido instance.
    #[clap(long, value_name = "address")]
    pub solido_address: Pubkey,

    // Maintainer to add or remove.
    #[clap(long, value_name = "address")]
    pub maintainer_address: Pubkey,

    /// Issue commands through the passed multisig account.
    #[clap(long, requires = "multisig-program-id", value_name = "address")]
    pub multisig_address: Option<Pubkey>,
    /// When issuing commands Multisig program id.
    #[clap(long, requires = "multisig-address", value_name = "address")]
    pub multisig_program_id: Option<Pubkey>,
}

/// Command to add a validator to Solido.
pub fn command_add_maintainer(
    config: Config,
    cluster: Cluster,
    opts: AddRemoveMaintainerOpts,
) -> Result<Option<ProposeInstructionOutput>, crate::Error> {
    let execution_method = get_execution_method(
        config.fee_payer.pubkey(),
        opts.multisig_program_id,
        opts.multisig_address,
    );
    let instruction = lido::instruction::add_maintainer(
        &opts.solido_program_id,
        &lido::instruction::AddMaintainerMeta {
            lido: opts.solido_address,
            manager: execution_method.get_pubkey(),
            maintainer: opts.maintainer_address,
        },
    )?;
    execution_method.send_instruction(
        cluster,
        config,
        opts.multisig_program_id,
        opts.multisig_address,
        instruction,
    )
}

/// Command to add a validator to Solido.
pub fn command_remove_maintainer(
    config: Config,
    cluster: Cluster,
    opts: AddRemoveMaintainerOpts,
) -> Result<Option<ProposeInstructionOutput>, crate::Error> {
    let execution_method = get_execution_method(
        config.fee_payer.pubkey(),
        opts.multisig_program_id,
        opts.multisig_address,
    );
    let instruction = lido::instruction::remove_maintainer(
        &opts.solido_program_id,
        &lido::instruction::RemoveMaintainerMeta {
            lido: opts.solido_address,
            manager: execution_method.get_pubkey(),
            maintainer: opts.maintainer_address,
        },
    )?;
    execution_method.send_instruction(
        cluster,
        config,
        opts.multisig_program_id,
        opts.multisig_address,
        instruction,
    )
}

#[derive(Clap, Debug)]
pub struct CreateValidatorStakeAccountOpts {
    /// Address of the Solido program.
    #[clap(long, value_name = "address")]
    pub solido_program_id: Pubkey,
    /// Account that stores the data for this Solido instance.
    #[clap(long, value_name = "address")]
    pub solido_address: Pubkey,

    /// Stake pool program id
    #[clap(long, value_name = "address")]
    stake_pool_program_id: Pubkey,
    /// Address of the validator vote account.
    #[clap(long, value_name = "address")]
    pub validator_vote: Pubkey,

    // TDOO(Ruud): Maybe move this to the previous (general) arguments passed to the program
    /// Issue commands through the passed multisig account
    #[clap(long, requires = "multisig-program-id", value_name = "address")]
    pub multisig_address: Option<Pubkey>,
    /// When issuing commands Multisig program id
    #[clap(long, requires = "multisig-address", value_name = "address")]
    pub multisig_program_id: Option<Pubkey>,
}

/// Command to add a validator to Solido.
pub fn command_create_validator_stake_account(
    config: Config,
    cluster: Cluster,
    opts: CreateValidatorStakeAccountOpts,
) -> Result<Option<ProposeInstructionOutput>, crate::Error> {
    let solido = get_solido(&config.rpc(), &opts.solido_address)?;

    let (stake_pool_authority, _) = lido::find_authority_program_address(
        &opts.solido_program_id,
        &opts.solido_address,
        STAKE_POOL_AUTHORITY,
    );

    let (stake_account, _) = find_stake_program_address(
        &opts.stake_pool_program_id,
        &opts.validator_vote,
        &solido.stake_pool_account,
    );

    let funder = config.fee_payer.pubkey();
    let execution_method = get_execution_method(
        config.fee_payer.pubkey(),
        opts.multisig_program_id,
        opts.multisig_address,
    );
    let instruction = lido::instruction::create_validator_stake_account(
        &opts.solido_program_id,
        &lido::instruction::CreateValidatorStakeAccountMeta {
            lido: opts.solido_address,
            manager: execution_method.get_pubkey(),
            stake_pool_program: opts.stake_pool_program_id,
            stake_pool: solido.stake_pool_account,
            staker: stake_pool_authority,
            funder,
            stake_account,
            validator: opts.validator_vote,
        },
    )?;
    execution_method.send_instruction(
        cluster,
        config,
        opts.multisig_program_id,
        opts.multisig_address,
        instruction,
    )
}

#[derive(Clap, Debug)]
pub struct ShowSolidoOpts {
    /// The solido instance to show
    #[clap(long)]
    solido_instance: Pubkey,
}

enum ExecutionMethod {
    Multisig(Pubkey),
    Payer(Pubkey),
}

impl ExecutionMethod {
    fn get_pubkey(&self) -> Pubkey {
        match self {
            ExecutionMethod::Multisig(pk) => *pk,
            ExecutionMethod::Payer(pk) => *pk,
        }
    }

    fn send_instruction(
        &self,
        cluster: Cluster,
        config: Config,
        multisig_program_id: Option<Pubkey>,
        multisig_address: Option<Pubkey>,
        instruction: Instruction,
    ) -> Result<Option<ProposeInstructionOutput>, crate::Error> {
        match self {
            ExecutionMethod::Multisig(_) => {
                let program =
                    get_anchor_program(cluster, config.fee_payer, &multisig_program_id.unwrap());

                Ok(Some(propose_instruction(
                    program,
                    multisig_address.unwrap(),
                    instruction,
                )))
            }
            ExecutionMethod::Payer(_) => {
                sign_and_send_transaction(&config, &[instruction], &[&config.fee_payer])?;
                Ok(None)
            }
        }
    }
}

fn get_execution_method(
    payer: Pubkey,
    multisig_program_id: Option<Pubkey>,
    multisig_address: Option<Pubkey>,
) -> ExecutionMethod {
    match multisig_program_id {
        Some(multisig_program_id) => {
            let (program_derived_address, _nonce) =
                get_multisig_program_address(&multisig_program_id, &multisig_address.unwrap());
            ExecutionMethod::Multisig(program_derived_address)
        }
        None => ExecutionMethod::Payer(payer),
    }
}

// TODO: Make `get_solido` and `get_stake_pool` return the structures in a single call to
// `rpc_client.get_multiple_accounts(..)`.
/// Gets the Solido data structure
pub fn get_solido(rpc_client: &RpcClient, solido_address: &Pubkey) -> Result<Lido, crate::Error> {
    let solido_data = rpc_client.get_account_data(solido_address)?;
    let solido = try_from_slice_unchecked::<Lido>(&solido_data)?;
    Ok(solido)
}

/// Gets the Stake Pool and validator list data structures. The validator list
/// is associated with the Stake Pool.
fn get_stake_pool(rpc_client: &RpcClient, stake_pool: &Pubkey) -> Result<StakePool, crate::Error> {
    let stake_pool_data = rpc_client.get_account_data(&stake_pool)?;
    let stake_pool = StakePool::try_from_slice(&stake_pool_data)?;
    Ok(stake_pool)
}
