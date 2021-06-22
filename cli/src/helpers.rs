use std::fmt;

use crate::config::{AddRemoveMaintainerOpts, AddValidatorOpts, CreateSolidoOpts, ShowSolidoOpts};
use lido::{
    state::{FeeDistribution, Lido},
    DEPOSIT_AUTHORITY, RESERVE_AUTHORITY,
};
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_program::{pubkey::Pubkey, system_instruction};
use solana_sdk::{
    borsh::try_from_slice_unchecked,
    instruction::Instruction,
    signature::{Keypair, Signer},
    signer::signers::Signers,
    transaction::Transaction,
};

use crate::{
    error::Error,
    multisig::{get_multisig_program_address, propose_instruction, ProposeInstructionOutput},
    spl_token_utils::{push_create_spl_token_account, push_create_spl_token_mint},
    util::PubkeyBase58,
    Config, OutputMode,
};

pub fn send_transaction(
    config: &Config,
    transaction: Transaction,
) -> solana_client::client_error::Result<()> {
    let _signature = match config.output_mode {
        OutputMode::Text => {
            // In text mode, we can display a spinner.
            config
                .rpc
                .send_and_confirm_transaction_with_spinner(&transaction)?
        }
        OutputMode::Json => {
            // In json mode, printing a spinner to stdout would break the
            // json that we also print to stdout, so opt for the silent
            // version.
            config.rpc.send_and_confirm_transaction(&transaction)?
        }
    };
    Ok(())
}

pub fn sign_and_send_transaction<T: Signers>(
    config: &Config,
    instructions: &[Instruction],
    signers: &T,
) -> solana_client::client_error::Result<()> {
    let mut tx = Transaction::new_with_payer(instructions, Some(&config.signer.pubkey()));

    let (recent_blockhash, _fee_calculator) = config
        .rpc
        .get_recent_blockhash()
        .expect("Failed to get recent blockhash.");

    // Add multisig signer
    tx.sign(signers, recent_blockhash);
    send_transaction(&config, tx)
}

#[derive(Serialize)]
pub struct CreateSolidoOutput {
    /// Account that stores the data for this Solido instance.
    pub solido_address: PubkeyBase58,

    /// Manages the deposited sol and token minting.
    pub reserve_authority: PubkeyBase58,

    /// SPL token mint account for StSol tokens.
    pub st_sol_mint_address: PubkeyBase58,

    /// stSOL SPL token account that holds the treasury funds.
    pub treasury_account: PubkeyBase58,

    /// stSOL SPL token account that receives the developer fees.
    pub developer_account: PubkeyBase58,
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
        writeln!(
            f,
            "  stSOL mint:                    {}",
            self.st_sol_mint_address
        )?;
        writeln!(
            f,
            "  Treasury SPL token account:    {}",
            self.treasury_account
        )?;
        writeln!(
            f,
            "  Developer fee SPL token account: {}",
            self.developer_account
        )?;
        Ok(())
    }
}

pub fn command_create_solido(
    config: &Config,
    opts: &CreateSolidoOpts,
) -> Result<CreateSolidoOutput, Error> {
    let lido_keypair = Keypair::new();

    let (reserve_authority, _) = lido::find_authority_program_address(
        opts.solido_program_id(),
        &lido_keypair.pubkey(),
        RESERVE_AUTHORITY,
    );

    let (manager, _nonce) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());

    let lido_size = Lido::calculate_size(*opts.max_validators(), *opts.max_maintainers());
    let lido_account_balance = config
        .rpc
        .get_minimum_balance_for_rent_exemption(lido_size)?;

    let mut instructions = Vec::new();

    // We need to fund Lido's reserve account so it is rent-exempt, otherwise it
    // might disappear.
    let min_balance_empty_data_account = config.rpc.get_minimum_balance_for_rent_exemption(0)?;
    instructions.push(system_instruction::transfer(
        &config.signer.pubkey(),
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
    let signers = &[&st_sol_mint_keypair, config.signer];
    sign_and_send_transaction(&config, &instructions[..], signers)?;
    instructions.clear();
    eprintln!("Did send mint init.");

    // Set up the SPL token account that receive the fees in stSOL.
    let treasury_keypair = push_create_spl_token_account(
        &config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        opts.treasury_account_owner(),
    )?;
    let developer_keypair = push_create_spl_token_account(
        &config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        opts.developer_account_owner(),
    )?;
    sign_and_send_transaction(
        &config,
        &instructions[..],
        &vec![config.signer, &treasury_keypair, &developer_keypair],
    )?;
    instructions.clear();
    eprintln!("Did send SPL account inits.");

    // Create the account that holds the Solido instance itself.
    instructions.push(system_instruction::create_account(
        &config.signer.pubkey(),
        &lido_keypair.pubkey(),
        lido_account_balance,
        lido_size as u64,
        opts.solido_program_id(),
    ));

    instructions.push(lido::instruction::initialize(
        opts.solido_program_id(),
        FeeDistribution {
            treasury_fee: *opts.treasury_fee(),
            validation_fee: *opts.validation_fee(),
            developer_fee: *opts.developer_fee(),
        },
        *opts.max_validators(),
        *opts.max_maintainers(),
        &lido::instruction::InitializeAccountsMeta {
            lido: lido_keypair.pubkey(),
            st_sol_mint: st_sol_mint_keypair.pubkey(),
            manager,
            treasury_account: treasury_keypair.pubkey(),
            developer_account: developer_keypair.pubkey(),
            reserve_account: reserve_authority,
        },
    )?);

    sign_and_send_transaction(&config, &instructions[..], &[config.signer, &lido_keypair])?;
    eprintln!("Did send Lido init.");

    let result = CreateSolidoOutput {
        solido_address: lido_keypair.pubkey().into(),
        reserve_authority: reserve_authority.into(),
        st_sol_mint_address: st_sol_mint_keypair.pubkey().into(),
        treasury_account: treasury_keypair.pubkey().into(),
        developer_account: developer_keypair.pubkey().into(),
    };
    Ok(result)
}

/// Command to add a validator to Solido.
pub fn command_add_validator(
    config: &Config,
    opts: &AddValidatorOpts,
) -> Result<ProposeInstructionOutput, Error> {
    let (multisig_address, _) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());

    let instruction = lido::instruction::add_validator(
        opts.solido_program_id(),
        &lido::instruction::AddValidatorMeta {
            lido: *opts.solido_address(),
            manager: multisig_address,
            validator_vote_account: *opts.validator_vote_account(),
            validator_fee_st_sol_account: *opts.validator_fee_account(),
        },
    )?;
    Ok(propose_instruction(
        &config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        instruction,
    ))
}

/// Command to add a validator to Solido.
pub fn command_add_maintainer(
    config: &Config,
    opts: &AddRemoveMaintainerOpts,
) -> Result<ProposeInstructionOutput, Error> {
    let (multisig_address, _) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());
    let instruction = lido::instruction::add_maintainer(
        opts.solido_program_id(),
        &lido::instruction::AddMaintainerMeta {
            lido: *opts.solido_address(),
            manager: multisig_address,
            maintainer: *opts.maintainer_address(),
        },
    )?;
    Ok(propose_instruction(
        &config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        instruction,
    ))
}

/// Command to add a validator to Solido.
pub fn command_remove_maintainer(
    config: &Config,
    opts: &AddRemoveMaintainerOpts,
) -> Result<ProposeInstructionOutput, Error> {
    let (multisig_address, _) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());
    let instruction = lido::instruction::remove_maintainer(
        opts.solido_program_id(),
        &lido::instruction::RemoveMaintainerMeta {
            lido: *opts.solido_address(),
            manager: multisig_address,
            maintainer: *opts.maintainer_address(),
        },
    )?;
    Ok(propose_instruction(
        &config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        instruction,
    ))
}

#[derive(Serialize)]
pub struct ShowSolidoOutput {
    pub solido_program_id: PubkeyBase58,
    pub solido_address: PubkeyBase58,
    pub solido: Lido,
    pub reserve_authority: PubkeyBase58,
    pub deposit_authority: PubkeyBase58,
}

impl fmt::Display for ShowSolidoOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Manager:                     {}", self.solido.manager)?;
        writeln!(
            f,
            "stSOL mint:                  {}",
            self.solido.st_sol_mint
        )?;

        writeln!(f, "\nExchange rate:")?;
        writeln!(f, "  Computed in epoch: {}", self.solido.exchange_rate.computed_in_epoch)?;
        writeln!(f, "  SOL balance:       {}", self.solido.exchange_rate.sol_balance)?;
        writeln!(f, "  stSOL supply:      {}", self.solido.exchange_rate.st_sol_supply)?;

        writeln!(f, "\nAuthorities (public key, bump seed):")?;
        writeln!(
            f,
            "Reserve:     {}, {}",
            self.reserve_authority, self.solido.sol_reserve_authority_bump_seed
        )?;
        writeln!(
            f,
            "Deposit:     {}, {}",
            self.deposit_authority, self.solido.deposit_authority_bump_seed
        )?;
        writeln!(f, "\nFee distribution:")?;
        writeln!(
            f,
            "Treasury:      {}/{}",
            self.solido.fee_distribution.treasury_fee,
            self.solido.fee_distribution.sum()
        )?;
        writeln!(
            f,
            "Validation:    {}/{}",
            self.solido.fee_distribution.validation_fee,
            self.solido.fee_distribution.sum()
        )?;
        writeln!(
            f,
            "Developer:     {}/{}",
            self.solido.fee_distribution.developer_fee,
            self.solido.fee_distribution.sum()
        )?;
        writeln!(f, "\nFee recipients:")?;
        writeln!(
            f,
            "Treasury SPL token account:      {}",
            self.solido.fee_recipients.treasury_account
        )?;
        writeln!(
            f,
            "Developer fee SPL token account: {}",
            self.solido.fee_recipients.developer_account
        )?;

        writeln!(
            f,
            "\nValidators: {} in use out of {} that the instance can support",
            self.solido.validators.len(),
            self.solido.validators.maximum_entries
        )?;
        for pe in &self.solido.validators.entries {
            writeln!(
                f,
                "  - stake account: {}, rewards address: {}, credit: {}",
                pe.pubkey, pe.entry.fee_address, pe.entry.fee_credit
            )?;
        }
        writeln!(
            f,
            "\nMaintainers: {} in use out of {} that the instance can support",
            self.solido.maintainers.len(),
            self.solido.maintainers.maximum_entries
        )?;
        for pe in &self.solido.maintainers.entries {
            writeln!(f, "  - {}", pe.pubkey)?;
        }
        Ok(())
    }
}

pub fn command_show_solido(
    config: &Config,
    opts: &ShowSolidoOpts,
) -> Result<ShowSolidoOutput, Error> {
    let lido = get_solido(&config.rpc, opts.solido_address())?;
    let reserve_authority = Pubkey::create_program_address(
        &[
            &opts.solido_address().to_bytes(),
            RESERVE_AUTHORITY,
            &[lido.sol_reserve_authority_bump_seed],
        ],
        opts.solido_program_id(),
    )?;

    let deposit_authority = Pubkey::create_program_address(
        &[
            &opts.solido_address().to_bytes(),
            DEPOSIT_AUTHORITY,
            &[lido.deposit_authority_bump_seed],
        ],
        opts.solido_program_id(),
    )?;

    Ok(ShowSolidoOutput {
        solido_program_id: opts.solido_program_id().into(),
        solido_address: opts.solido_address().into(),
        solido: lido,
        reserve_authority: reserve_authority.into(),
        deposit_authority: deposit_authority.into(),
    })
}

// TODO(#181): Make `get_solido` return the structures in a single call to
// `rpc_client.get_multiple_accounts(..)`.
/// Gets the Solido data structure
pub fn get_solido(rpc_client: &RpcClient, solido_address: &Pubkey) -> Result<Lido, Error> {
    let solido_data = rpc_client.get_account_data(solido_address)?;
    let solido = try_from_slice_unchecked::<Lido>(&solido_data)?;
    Ok(solido)
}
