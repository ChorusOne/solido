use std::fmt;

use clap::Clap;
use lido::{state::FeeDistribution, DEPOSIT_AUTHORITY, FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY};
use serde::Serialize;
use solana_program::{
    borsh::get_packed_len, native_token::Sol, program_pack::Pack, pubkey::Pubkey,
    system_instruction,
};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use spl_stake_pool::state::Fee;

use crate::{
    stake_pool_helpers::{command_create_pool, CreatePoolOutput},
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

    /// Account to receive the `insurance_fee` proportion.
    #[clap(long)]
    pub insurance_account: Pubkey,
    /// Account to receive the `treasury_fee` proportion.
    #[clap(long)]
    pub treasury_account: Pubkey,
    /// Account to receive the `manager_fee` proportion.
    #[clap(long)]
    pub manager_fee_account: Pubkey,
}

#[derive(Serialize)]
pub struct CreateSolidoOutput {
    /// Account that stores the data for this Solido instance.
    pub solido_address: Pubkey,

    /// Manages the deposited sol and token minting.
    pub reserve_authority: Pubkey,

    /// Owner of the `fee_address`.
    pub fee_authority: Pubkey,

    /// Address for receiving fees.
    pub fee_address: Pubkey,

    /// SPL token mint account for LSOL tokens.
    pub mint_address: Pubkey,

    /// The only depositor of the stake pool.
    pub pool_token_to: Pubkey,

    /// Details of the stake pool managed by Solido.
    pub stake_pool: CreatePoolOutput,
}

impl fmt::Display for CreateSolidoOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Solido details:")?;
        writeln!(f, "  Solido address:         {}", self.solido_address)?;
        writeln!(f, "  Reserve authority:      {}", self.reserve_authority)?;
        writeln!(f, "  Fee authority:          {}", self.fee_authority)?;
        writeln!(f, "  Fee address:            {}", self.fee_address)?;
        writeln!(f, "  stSOL mint:             {}", self.mint_address)?;
        writeln!(f, "  Solido's pool account:  {}", self.pool_token_to)?;
        writeln!(f, "Stake pool details:\n{}", self.stake_pool)?;
        Ok(())
    }
}

pub fn command_create_solido(
    config: &Config,
    opts: CreateSolidoOpts,
) -> Result<CreateSolidoOutput, crate::Error> {
    let lido_keypair = Keypair::new();

    let (reserve_authority, _) = lido::find_authority_program_address(
        &lido::id(),
        &lido_keypair.pubkey(),
        RESERVE_AUTHORITY,
    );

    let (fee_authority, _) = lido::find_authority_program_address(
        &lido::id(),
        &lido_keypair.pubkey(),
        FEE_MANAGER_AUTHORITY,
    );

    let (deposit_authority, _) = lido::find_authority_program_address(
        &lido::id(),
        &lido_keypair.pubkey(),
        DEPOSIT_AUTHORITY,
    );

    let stake_pool = command_create_pool(
        config,
        &deposit_authority,
        Fee {
            numerator: opts.fee_numerator,
            denominator: opts.fee_denominator,
        },
        opts.max_validators,
    )?;

    let mint_keypair = Keypair::new();
    let fee_keypair = Keypair::new();
    let pool_token_to = Keypair::new();

    let mint_account_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?;
    let lido_size = get_packed_len::<lido::state::Lido>();
    let lido_account_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(lido_size)?;

    let fee_token_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)?;
    let pool_token_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)?;

    let total_rent_free_balances =
        mint_account_balance + lido_account_balance + fee_token_balance + pool_token_balance;

    let default_decimals = spl_token::native_mint::DECIMALS;
    let mut lido_transaction = Transaction::new_with_payer(
        &[
            // Account for lido st_sol mint
            system_instruction::create_account(
                &config.fee_payer.pubkey(),
                &mint_keypair.pubkey(),
                mint_account_balance,
                spl_token::state::Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &mint_keypair.pubkey(),
                &reserve_authority,
                None,
                default_decimals,
            )?,
            system_instruction::create_account(
                &config.fee_payer.pubkey(),
                &lido_keypair.pubkey(),
                lido_account_balance,
                lido_size as u64,
                &lido::id(),
            ),
            // Account for the pool fee accumulation
            system_instruction::create_account(
                &config.fee_payer.pubkey(),
                &fee_keypair.pubkey(),
                fee_token_balance,
                spl_token::state::Account::LEN as u64,
                &spl_token::id(),
            ),
            // Initialize fee receiver account
            spl_token::instruction::initialize_account(
                &spl_token::id(),
                &fee_keypair.pubkey(),
                &mint_keypair.pubkey(),
                &fee_authority,
            )?,
            system_instruction::create_account(
                &config.fee_payer.pubkey(),
                &pool_token_to.pubkey(),
                fee_token_balance,
                spl_token::state::Account::LEN as u64,
                &spl_token::id(),
            ),
            // Initialize Lido's account in Stake Pool
            spl_token::instruction::initialize_account(
                &spl_token::id(),
                &pool_token_to.pubkey(),
                &stake_pool.mint_address,
                &lido::id(),
            )?,
            lido::instruction::initialize(
                &lido::id(),
                FeeDistribution {
                    insurance_fee: opts.insurance_fee,
                    treasury_fee: opts.treasury_fee,
                    validation_fee: opts.validation_fee,
                    manager_fee: opts.manager_fee,
                },
                opts.max_validators,
                &lido::instruction::InitializeAccountsMeta {
                    lido: lido_keypair.pubkey(),
                    stake_pool: stake_pool.stake_pool_address,
                    mint_program: mint_keypair.pubkey(),
                    pool_token_to: pool_token_to.pubkey(),
                    fee_token: fee_keypair.pubkey(), // to define
                    manager: config.staker.pubkey(),
                    insurance_account: opts.insurance_account,
                    treasury_account: opts.treasury_account,
                    manager_fee_account: opts.manager_fee_account,
                },
            )?,
        ],
        Some(&config.fee_payer.pubkey()),
    );

    let (recent_blockhash, fee_calculator) = config.rpc_client.get_recent_blockhash()?;
    check_fee_payer_balance(
        config,
        total_rent_free_balances + fee_calculator.calculate_fee(&lido_transaction.message()),
    )?;
    let signers = vec![
        config.fee_payer,
        &mint_keypair,
        &lido_keypair,
        &pool_token_to,
        &fee_keypair,
    ];
    lido_transaction.sign(&signers, recent_blockhash);
    send_transaction(&config, lido_transaction)?;

    let result = CreateSolidoOutput {
        solido_address: lido_keypair.pubkey(),
        reserve_authority,
        fee_authority,
        mint_address: mint_keypair.pubkey(),
        fee_address: fee_keypair.pubkey(),
        pool_token_to: pool_token_to.pubkey(),
        stake_pool,
    };
    Ok(result)
}
