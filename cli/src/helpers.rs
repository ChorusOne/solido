use std::fmt;

use clap::Clap;
use lido::{state::FeeDistribution, DEPOSIT_AUTHORITY, FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_AUTHORITY};
use serde::Serialize;
use solana_program::{
    native_token::Sol, program_pack::Pack, pubkey::Pubkey,
    system_instruction,
};
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signer},
    signer::signers::Signers,
    transaction::Transaction,
};
use spl_stake_pool::state::Fee;

use crate::{
    stake_pool_helpers::{command_create_pool, CreatePoolOutput},
    util::PubkeyBase58,
    Config, Error, OutputMode,
};

pub fn check_fee_payer_balance(config: &Config, required_balance: u64) -> Result<(), Error> {
    let balance = config.rpc_client.get_balance(&config.fee_payer.pubkey())?;
    if balance < required_balance {
        Err(format!(
            "Fee payer, {}, has insufficient balance: {} required, {} available",
            config.fee_payer.pubkey(),
            Sol(required_balance),
            Sol(balance)
        )
        .into())
    } else {
        Ok(())
    }
}

pub fn send_transaction(
    config: &Config,
    transaction: Transaction,
) -> solana_client::client_error::Result<()> {
    if config.dry_run {
        config.rpc_client.simulate_transaction(&transaction)?;
    } else {
        let _signature = match config.output_mode {
            OutputMode::Text => {
                // In text mode, we can display a spinner.
                config
                    .rpc_client
                    .send_and_confirm_transaction_with_spinner(&transaction)?
            }
            OutputMode::Json => {
                // In json mode, printing a spinner to stdout would break the
                // json that we also print to stdout, so opt for the silent
                // version.
                config
                    .rpc_client
                    .send_and_confirm_transaction(&transaction)?
            }
        };
    }
    Ok(())
}

#[derive(Clap, Debug)]
pub struct CreateSolidoOpts {
    /// Address of the Solido program.
    #[clap(long)]
    pub solido_program_id: Pubkey,

    /// Numerator of the fee fraction.
    #[clap(long)]
    pub fee_numerator: u64,

    /// Denominator of the fee fraction.
    #[clap(long)]
    pub fee_denominator: u64,

    /// The maximum number of validators that this Solido instance will support.
    #[clap(long)]
    pub max_validators: u32,

    /// Fees are divided proportionally to the sum of all specified fees, for instance,
    /// if all the fees are the same value, they will be divided equally.

    /// Insurance fee proportion
    #[clap(long)]
    pub insurance_fee: u32,
    /// Treasury fee proportion
    #[clap(long)]
    pub treasury_fee: u32,
    /// Validation fee proportion, to be divided equally among validators
    #[clap(long)]
    pub validation_fee: u32,
    /// Manager fee
    #[clap(long)]
    pub manager_fee: u32,

    /// Account who will own the stSOL SPL token account that receives insurance fees.
    #[clap(long)]
    pub insurance_account_owner: Pubkey,
    /// Account who will own the stSOL SPL token account that receives treasury fees.
    #[clap(long)]
    pub treasury_account_owner: Pubkey,
    /// Account who will own the stSOL SPL token account that receives the manager fees.
    #[clap(long)]
    pub manager_fee_account_owner: Pubkey,
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
        writeln!(f, "  Solido address:                {}", self.solido_address)?;
        writeln!(f, "  Reserve authority:             {}", self.reserve_authority)?;
        writeln!(f, "  Fee authority:                 {}", self.fee_authority)?;
        writeln!(f, "  Stake pool authority:          {}", self.stake_pool_authority)?;
        writeln!(f, "  stSOL mint:                    {}", self.st_sol_mint_address)?;
        writeln!(f, "  Solido's pool account:         {}", self.pool_token_to)?;
        writeln!(f, "  Insurance SPL token account:   {}", self.insurance_account)?;
        writeln!(f, "  Treasury SPL token account:    {}", self.treasury_account)?;
        writeln!(f, "  Manager fee SPL token account: {}", self.treasury_account)?;
        writeln!(f, "Stake pool details:\n{}", self.stake_pool)?;
        Ok(())
    }
}

/// Push instructions to create and initialize an SPL token account.
///
/// Returns the keypair for the account. This keypair needs to sign the
/// transaction.
fn push_create_spl_token_account(
    config: &Config,
    instructions: &mut Vec<solana_program::instruction::Instruction>,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<Keypair, crate::Error> {

    let spl_token_min_sol_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)?;

    let keypair = Keypair::new();

    instructions.push(system_instruction::create_account(
        &config.fee_payer.pubkey(),
        &keypair.pubkey(),
        // Deposit enough SOL to make it rent-exempt.
        spl_token_min_sol_balance,
        spl_token::state::Account::LEN as u64,
        // The new account should be owned by the SPL token program.
        &spl_token::id(),
    ));
    instructions.push(spl_token::instruction::initialize_account(
        &spl_token::id(),
        &keypair.pubkey(),
        mint,
        owner,
    )?);

    Ok(keypair)
}

fn sign_and_send_transaction<T: Signers>(config: &Config, instructions: &[Instruction], signers: &T) -> Result<(), crate::Error> {
    let mut tx = Transaction::new_with_payer(instructions,
        Some(&config.fee_payer.pubkey()),
    );

    let (recent_blockhash, _fee_calculator) = config.rpc_client.get_recent_blockhash()?;
    tx.sign(signers, recent_blockhash);
    send_transaction(&config, tx)?;

    Ok(())
}

pub fn command_create_solido(
    config: &Config,
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
        config,
        &stake_pool_authority,
        &deposit_authority,
        Fee {
            numerator: opts.fee_numerator,
            denominator: opts.fee_denominator,
        },
        opts.max_validators,
    )?;

    let st_sol_mint_keypair = Keypair::new();

    let mint_account_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?;
    // TODO(fynn): get_packed_len panics on https://docs.rs/solana-program/1.6.9/src/solana_program/borsh.rs.html#40,
    // so we need to compute the size in a different way.
    let lido_size = 999; //get_packed_len::<lido::state::Lido>();
    let lido_account_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(lido_size)?;

    let mut instructions = Vec::new();
    let default_decimals = spl_token::native_mint::DECIMALS;

    // Set up the Lido stSOL SPL token mint account.
    instructions.push(system_instruction::create_account(
        &config.fee_payer.pubkey(),
        &st_sol_mint_keypair.pubkey(),
        mint_account_balance,
        spl_token::state::Mint::LEN as u64,
        &spl_token::id(),
    ));
    instructions.push(spl_token::instruction::initialize_mint(
        &spl_token::id(),
        &st_sol_mint_keypair.pubkey(),
        &reserve_authority,
        None,
        default_decimals,
    )?);

    // Ideally we would set up the entire instance in a single transaction, but
    // Solana transaction size limits are so low that we need to break our
    // instructions down into multiple transactions. So set up the mint first,
    // then continue.
    sign_and_send_transaction(
        config,
        &instructions[..],
        &[
            config.fee_payer,
            &st_sol_mint_keypair,
        ],
    )?;
    instructions.clear();
    eprintln!("Did send mint init.");

    // Set up the SPL token account that holds Lido's stake pool tokens.
    let pool_token_to_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &stake_pool.mint_address.0,
        &opts.solido_program_id,
    )?;

    sign_and_send_transaction(
        config,
        &instructions[..],
        &vec![
            config.fee_payer,
            &pool_token_to_keypair,
        ]
    )?;
    instructions.clear();
    eprintln!("Did send SPL account inits part 1.");

    // Set up the SPL token account that receive the fees in stSOL.
    let insurance_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        &opts.insurance_account_owner,
    )?;
    let treasury_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        &opts.treasury_account_owner,
    )?;
    let manager_fee_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        &opts.manager_fee_account_owner,
    )?;
    sign_and_send_transaction(
        config,
        &instructions[..],
        &vec![
            config.fee_payer,
            &insurance_keypair,
            &treasury_keypair,
            &manager_fee_keypair,
        ]
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
        &lido::instruction::InitializeAccountsMeta {
            lido: lido_keypair.pubkey(),
            stake_pool: stake_pool.stake_pool_address.0,
            mint_program: st_sol_mint_keypair.pubkey(),
            pool_token_to: pool_token_to_keypair.pubkey(),
            fee_token: stake_pool.fee_address.0,
            manager: stake_pool_authority,
            insurance_account: insurance_keypair.pubkey(),
            treasury_account: treasury_keypair.pubkey(),
            manager_fee_account: manager_fee_keypair.pubkey(),
            reserve_account: reserve_authority,
        },
    )?);

    sign_and_send_transaction(
        config,
        &instructions[..],
        &[
            config.fee_payer,
            &lido_keypair,
        ],
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
