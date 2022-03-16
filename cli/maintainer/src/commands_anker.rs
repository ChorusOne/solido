// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::fmt;

use anker::state::HistoricalStSolPrice;
use anker::token::{BLamports, MicroUst};
use anker::wormhole::TerraAddress;
use clap::Clap;
use lido::token::{Lamports, StLamports};
use lido::util::serialize_b58;
use serde::Serialize;
use solana_program::pubkey::Pubkey;
use solana_program::system_instruction;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solido_cli_common::error::Abort;
use solido_cli_common::snapshot::{SnapshotClientConfig, SnapshotConfig};
use spl_token_swap::curve::base::{CurveType, SwapCurve};
use spl_token_swap::curve::constant_product::ConstantProductCurve;

use crate::config::{
    AnkerDepositOpts, AnkerWithdrawOpts, ConfigFile, CreateAnkerOpts, CreateTokenPoolOpts,
    ShowAnkerAuthoritiesOpts, ShowAnkerOpts,
};
use crate::print_output;
use crate::serialization_utils::serialize_bech32;
use crate::spl_token_utils::{push_create_spl_token_account, push_create_spl_token_mint};

#[derive(Clap, Debug)]
enum SubCommand {
    /// Create a new Anker instance.
    Create(Box<CreateAnkerOpts>),

    /// Show Anker authorities.
    ShowAuthorities(ShowAnkerAuthoritiesOpts),

    /// Display the details of an Anker instance.
    Show(ShowAnkerOpts),

    /// Create an SPL token swap pool for testing purposes.
    CreateTokenPool(CreateTokenPoolOpts),

    /// Deposit stSOL to Anker to obtain bSOL.
    Deposit(AnkerDepositOpts),

    /// Return bSOL to Anker to redeem stSOL.
    Withdraw(AnkerWithdrawOpts),
}

#[derive(Clap, Debug)]
pub struct AnkerOpts {
    #[clap(subcommand)]
    subcommand: SubCommand,
}

impl AnkerOpts {
    pub fn merge_with_config_and_environment(&mut self, config_file: Option<&ConfigFile>) {
        match &mut self.subcommand {
            SubCommand::Create(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::Show(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::CreateTokenPool(opts) => {
                opts.merge_with_config_and_environment(config_file)
            }
            SubCommand::Deposit(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::Withdraw(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::ShowAuthorities(opts) => {
                opts.merge_with_config_and_environment(config_file)
            }
        }
    }
}

pub fn main(config: &mut SnapshotClientConfig, anker_opts: &AnkerOpts) {
    match &anker_opts.subcommand {
        SubCommand::Create(opts) => {
            let result = config.with_snapshot(|config| command_create_anker(config, opts));
            let output = result.ok_or_abort_with("Failed to create Anker instance.");
            print_output(config.output_mode, &output);
        }
        SubCommand::Show(opts) => {
            let result = config.with_snapshot(|config| command_show_anker(config, opts));
            let output = result.ok_or_abort_with("Failed to show Anker instance.");
            print_output(config.output_mode, &output);
        }
        SubCommand::CreateTokenPool(opts) => {
            let result = config.with_snapshot(|config| command_create_token_pool(config, opts));
            let output = result.ok_or_abort_with("Failed to create Token Pool instance.");
            print_output(config.output_mode, &output);
        }
        SubCommand::Deposit(opts) => {
            let result = config.with_snapshot(|config| command_deposit(config, opts));
            let output = result.ok_or_abort_with("Failed to deposit into Anker.");
            print_output(config.output_mode, &output);
        }
        SubCommand::Withdraw(opts) => {
            let result = config.with_snapshot(|config| command_withdraw(config, opts));
            let output = result.ok_or_abort_with("Failed to withdraw from Anker.");
            print_output(config.output_mode, &output);
        }
        SubCommand::ShowAuthorities(opts) => {
            let result = config.with_snapshot(|_| command_show_anker_authorities(opts));
            let output = result.ok_or_abort_with("Failed to show Anker authorities.");
            print_output(config.output_mode, &output);
        }
    }
}

#[derive(Serialize)]
struct CreateAnkerOutput {
    /// Account that stores the data for this Anker instance.
    #[serde(serialize_with = "serialize_b58")]
    pub anker_address: Pubkey,

    /// Manages the deposited stSOL.
    #[serde(serialize_with = "serialize_b58")]
    pub st_sol_reserve_account: Pubkey,

    /// Holds the UST proceeds until they are sent to Terra.
    #[serde(serialize_with = "serialize_b58")]
    pub ust_reserve_account: Pubkey,

    /// SPL token mint account for bSOL tokens.
    #[serde(serialize_with = "serialize_b58")]
    pub b_sol_mint_address: Pubkey,
}

impl fmt::Display for CreateAnkerOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Anker details:")?;
        writeln!(f, "  Anker address:           {}", self.anker_address)?;
        writeln!(
            f,
            "  Reserve account (stSOL): {}",
            self.st_sol_reserve_account
        )?;
        writeln!(f, "  Reserve account (UST):   {}", self.ust_reserve_account)?;
        writeln!(f, "  bSOL mint:               {}", self.b_sol_mint_address)?;
        Ok(())
    }
}

fn command_create_anker(
    config: &mut SnapshotConfig,
    opts: &CreateAnkerOpts,
) -> solido_cli_common::Result<CreateAnkerOutput> {
    let solido = config.client.get_solido(opts.solido_address())?;

    let (anker_address, _bump_seed) =
        anker::find_instance_address(opts.anker_program_id(), opts.solido_address());
    let (st_sol_reserve_account, _bump_seed) =
        anker::find_st_sol_reserve_account(opts.anker_program_id(), &anker_address);
    let (ust_reserve_account, _bump_seed) =
        anker::find_ust_reserve_account(opts.anker_program_id(), &anker_address);
    let (reserve_authority, _bump_seed) =
        anker::find_reserve_authority(opts.anker_program_id(), &anker_address);

    let instructions = [anker::instruction::initialize(
        opts.anker_program_id(),
        &anker::instruction::InitializeAccountsMeta {
            fund_rent_from: config.signer.pubkey(),
            anker: anker_address,
            solido: *opts.solido_address(),
            solido_program: *opts.solido_program_id(),
            st_sol_mint: solido.st_sol_mint,
            b_sol_mint: *opts.b_sol_mint_address(),
            st_sol_reserve_account,
            ust_reserve_account,
            reserve_authority,
            wormhole_core_bridge_program_id: *opts.wormhole_core_bridge_program_id(),
            wormhole_token_bridge_program_id: *opts.wormhole_token_bridge_program_id(),
            ust_mint: *opts.ust_mint_address(),
            token_swap_pool: *opts.token_swap_pool(),
        },
        opts.terra_rewards_address().clone(),
        *opts.sell_rewards_min_out_bps(),
    )];

    config.sign_and_send_transaction(&instructions[..], &[config.signer])?;

    let result = CreateAnkerOutput {
        anker_address,
        st_sol_reserve_account,
        ust_reserve_account,
        b_sol_mint_address: *opts.b_sol_mint_address(),
    };

    Ok(result)
}

#[derive(Serialize)]
struct ShowAnkerOutput {
    #[serde(serialize_with = "serialize_b58")]
    anker_address: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    anker_program_id: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    solido_address: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    solido_program_id: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    b_sol_mint: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    b_sol_mint_authority: Pubkey,

    #[serde(serialize_with = "serialize_bech32")]
    terra_rewards_destination: TerraAddress,

    sell_rewards_min_out_bps: u64,

    #[serde(serialize_with = "serialize_b58")]
    reserve_authority: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    st_sol_reserve: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    ust_reserve: Pubkey,

    #[serde(rename = "ust_reserve_balance_micro_ust")]
    ust_reserve_balance: MicroUst,

    #[serde(rename = "st_sol_reserve_balance_st_lamports")]
    st_sol_reserve_balance: StLamports,

    #[serde(rename = "st_sol_reserve_value_lamports")]
    st_sol_reserve_value: Option<Lamports>,

    #[serde(rename = "b_sol_supply_b_lamports")]
    b_sol_supply: BLamports,

    historical_st_sol_price: Vec<HistoricalStSolPrice>,
}

impl fmt::Display for ShowAnkerOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Anker address:          {}", self.anker_address)?;
        writeln!(f, "Anker program id:       {}", self.anker_program_id)?;
        writeln!(f, "Solido address:         {}", self.solido_address)?;
        writeln!(f, "Solido program id:      {}", self.solido_program_id)?;
        writeln!(
            f,
            "Rewards destination:    {}",
            self.terra_rewards_destination
        )?;
        if self.sell_rewards_min_out_bps <= 9999 {
            writeln!(f,
                     "Sell rewards min out:   {}.{:>02}% of the expected amount ({}.{:>02}% slippage + fees)",
                     self.sell_rewards_min_out_bps / 100,
                     self.sell_rewards_min_out_bps % 100,
                    (10000 - self.sell_rewards_min_out_bps) / 100,
                     (10000 - self.sell_rewards_min_out_bps) % 100,
            )?;
        } else {
            writeln!(
                f,
                "Sell rewards min out:   {}.{:>02}% of the expected amount \
                     (Warning! Getting >100% out is unlikely to ever happen.)",
                self.sell_rewards_min_out_bps / 100,
                self.sell_rewards_min_out_bps % 100,
            )?;
        }
        writeln!(f, "bSOL mint:              {}", self.b_sol_mint)?;
        writeln!(f, "bSOL mint authority:    {}", self.b_sol_mint_authority)?;
        writeln!(f, "bSOL supply:            {}", self.b_sol_supply)?;
        writeln!(f, "Reserve authority:      {}", self.reserve_authority)?;
        writeln!(f, "stSOL reserve address:  {}", self.st_sol_reserve)?;
        writeln!(f, "stSOL reserve balance:  {}", self.st_sol_reserve_balance)?;
        write!(f, "stSOL reserve value:    ")?;
        match self.st_sol_reserve_value {
            Some(sol_value) => writeln!(f, "{}", sol_value),
            None => writeln!(f, "Undefined; does Solido have nonzero deposits?"),
        }?;
        writeln!(f, "UST reserve address:    {}", self.ust_reserve)?;
        writeln!(f, "UST reserve balance:    {}", self.ust_reserve_balance)?;
        writeln!(f, "Historical stSOL price:")?;
        for x in &self.historical_st_sol_price {
            writeln!(f, "  Slot {}: {} per stSOL", x.slot, x.st_sol_price_in_ust)?;
        }
        Ok(())
    }
}

fn command_show_anker(
    config: &mut SnapshotConfig,
    opts: &ShowAnkerOpts,
) -> solido_cli_common::Result<ShowAnkerOutput> {
    let client = &mut config.client;
    let anker_account = client.get_account(opts.anker_address())?;
    let anker_program_id = anker_account.owner;
    let anker = client.get_anker(opts.anker_address())?;
    let solido = client.get_solido(&anker.solido)?;

    let (mint_authority, _seed) =
        anker::find_mint_authority(&anker_program_id, opts.anker_address());
    let (reserve_authority, _seed) =
        anker::find_reserve_authority(&anker_program_id, opts.anker_address());
    let (st_sol_reserve, _seed) =
        anker::find_st_sol_reserve_account(&anker_program_id, opts.anker_address());
    let (ust_reserve, _seed) =
        anker::find_ust_reserve_account(&anker_program_id, opts.anker_address());

    let st_sol_reserve_balance = StLamports(client.get_spl_token_balance(&st_sol_reserve)?);
    let ust_reserve_balance = MicroUst(client.get_spl_token_balance(&ust_reserve)?);

    let st_sol_reserve_value = solido
        .exchange_rate
        .exchange_st_sol(st_sol_reserve_balance)
        .ok();
    let b_sol_mint = client.get_spl_token_mint(&anker.b_sol_mint)?;
    let b_sol_supply = BLamports(b_sol_mint.supply);

    let result = ShowAnkerOutput {
        anker_address: *opts.anker_address(),
        anker_program_id,

        solido_address: anker.solido,
        solido_program_id: anker.solido_program_id,

        terra_rewards_destination: anker.terra_rewards_destination,
        sell_rewards_min_out_bps: anker.sell_rewards_min_out_bps,

        b_sol_mint: anker.b_sol_mint,
        b_sol_mint_authority: mint_authority,
        reserve_authority,
        st_sol_reserve,
        ust_reserve,

        st_sol_reserve_balance,
        st_sol_reserve_value,

        ust_reserve_balance,
        b_sol_supply,

        historical_st_sol_price: anker.historical_st_sol_prices.0.iter().cloned().collect(),
    };

    Ok(result)
}

#[derive(Serialize)]
struct CreateTokenPoolOutput {
    #[serde(serialize_with = "serialize_b58")]
    pool_address: Pubkey,
    #[serde(serialize_with = "serialize_b58")]
    pool_authority: Pubkey,
}

impl fmt::Display for CreateTokenPoolOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Pool address:         {}", self.pool_address)?;
        writeln!(f, "Pool authority:       {}", self.pool_authority)?;
        Ok(())
    }
}

/// Create a Token Pool. Used for testing purposes only.
///
/// The pool is created with 0 fees.
/// The pool is `ConstantProduct`, i.e., `token_a * token_b = C`, with C a
/// constant.
fn command_create_token_pool(
    config: &mut SnapshotConfig,
    opts: &CreateTokenPoolOpts,
) -> solido_cli_common::Result<CreateTokenPoolOutput> {
    let client = &mut config.client;
    let mut instructions = Vec::new();

    let token_pool_account = Keypair::new();
    let rent = client.get_rent()?;

    let rent_lamports = rent.minimum_balance(spl_token_swap::state::SwapVersion::LATEST_LEN);
    instructions.push(system_instruction::create_account(
        &config.signer.pubkey(),
        &token_pool_account.pubkey(),
        rent_lamports,
        spl_token_swap::state::SwapVersion::LATEST_LEN as u64,
        opts.token_swap_program_id(),
    ));

    let (authority_pubkey, authority_bump_seed) = Pubkey::find_program_address(
        &[&token_pool_account.pubkey().to_bytes()[..]],
        opts.token_swap_program_id(),
    );

    let pool_mint_keypair =
        push_create_spl_token_mint(config, &mut instructions, &authority_pubkey)?;
    let pool_mint_pubkey = pool_mint_keypair.pubkey();
    let pool_fee_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &pool_mint_pubkey,
        &config.signer.pubkey(),
    )?;
    let pool_token_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &pool_mint_pubkey,
        &config.signer.pubkey(),
    )?;

    // Change the token owner to the pool's authority.
    instructions.push(spl_token::instruction::set_authority(
        &spl_token::id(),
        opts.st_sol_account(),
        Some(&authority_pubkey),
        spl_token::instruction::AuthorityType::AccountOwner,
        &config.signer.pubkey(),
        &[],
    )?);

    // Change the token owner to the pool's authority.
    instructions.push(spl_token::instruction::set_authority(
        &spl_token::id(),
        opts.ust_account(),
        Some(&authority_pubkey),
        spl_token::instruction::AuthorityType::AccountOwner,
        &config.signer.pubkey(),
        &[],
    )?);

    let signers = vec![
        config.signer,
        &token_pool_account,
        &pool_mint_keypair,
        &pool_fee_keypair,
        &pool_token_keypair,
    ];

    let fees = spl_token_swap::curve::fees::Fees {
        trade_fee_numerator: 0,
        trade_fee_denominator: 10,
        owner_trade_fee_numerator: 0,
        owner_trade_fee_denominator: 10,
        owner_withdraw_fee_numerator: 0,
        owner_withdraw_fee_denominator: 10,
        host_fee_numerator: 0,
        host_fee_denominator: 10,
    };

    let swap_curve = SwapCurve {
        curve_type: CurveType::ConstantProduct,
        calculator: Box::new(ConstantProductCurve),
    };

    let initialize_pool_instruction = spl_token_swap::instruction::initialize(
        opts.token_swap_program_id(),
        &spl_token::id(),
        &token_pool_account.pubkey(),
        &authority_pubkey,
        opts.st_sol_account(),
        opts.ust_account(),
        &pool_mint_pubkey,
        &pool_fee_keypair.pubkey(),
        &pool_token_keypair.pubkey(),
        authority_bump_seed,
        fees,
        swap_curve,
    )
    .expect("Failed to create token pool initialization instruction.");
    instructions.push(initialize_pool_instruction);

    config.sign_and_send_transaction(&instructions[..], &signers)?;

    Ok(CreateTokenPoolOutput {
        pool_address: token_pool_account.pubkey(),
        pool_authority: authority_pubkey,
    })
}

#[derive(Serialize)]
struct DepositOutput {
    /// Recipient account that holds the bSOL.
    #[serde(serialize_with = "serialize_b58")]
    pub b_sol_account: Pubkey,

    /// Whether we had to create the associated bSOL account. False if one existed already.
    pub created_associated_b_sol_account: bool,
}

impl fmt::Display for DepositOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.created_associated_b_sol_account {
            writeln!(f, "Created recipient bSOL account, it did not yet exist.")?;
        } else {
            writeln!(f, "Recipient bSOL account existed already before deposit.")?;
        }
        writeln!(f, "Recipient bSOL account: {}", self.b_sol_account)?;
        Ok(())
    }
}

fn command_deposit(
    config: &mut SnapshotConfig,
    opts: &AnkerDepositOpts,
) -> solido_cli_common::Result<DepositOutput> {
    let client = &mut config.client;
    let anker_account = client.get_account(opts.anker_address())?;
    let anker_program_id = anker_account.owner;
    let anker = client.get_anker(opts.anker_address())?;
    let solido = client.get_solido(&anker.solido)?;

    let mut instructions = Vec::new();
    let mut created_recipient = false;

    // The user can pass in a particular SPL token account to send from, but if
    // none is provided, we use the associated token account of the signer.
    let sender = if opts.from_st_sol_address() == &Pubkey::default() {
        spl_associated_token_account::get_associated_token_address(
            &config.signer.pubkey(),
            &solido.st_sol_mint,
        )
    } else {
        *opts.from_st_sol_address()
    };

    let recipient = spl_associated_token_account::get_associated_token_address(
        &config.signer.pubkey(),
        &anker.b_sol_mint,
    );

    if !config.client.account_exists(&recipient)? {
        let instr = spl_associated_token_account::create_associated_token_account(
            &config.signer.pubkey(),
            &config.signer.pubkey(),
            &anker.b_sol_mint,
        );
        instructions.push(instr);
        created_recipient = true;
    }

    let (st_sol_reserve_account, _bump_seed) =
        anker::find_st_sol_reserve_account(&anker_program_id, opts.anker_address());
    let (b_sol_mint_authority, _bump_seed) =
        anker::find_mint_authority(&anker_program_id, opts.anker_address());

    let instr = anker::instruction::deposit(
        &anker_program_id,
        &anker::instruction::DepositAccountsMeta {
            anker: *opts.anker_address(),
            solido: anker.solido,
            from_account: sender,
            user_authority: config.signer.pubkey(),
            to_reserve_account: st_sol_reserve_account,
            b_sol_user_account: recipient,
            b_sol_mint: anker.b_sol_mint,
            b_sol_mint_authority,
        },
        *opts.amount_st_sol(),
    );
    instructions.push(instr);

    config.sign_and_send_transaction(&instructions[..], &[config.signer])?;

    let result = DepositOutput {
        created_associated_b_sol_account: created_recipient,
        b_sol_account: recipient,
    };
    Ok(result)
}

#[derive(Serialize)]
struct WithdrawOutput {
    /// Sender account whose bSOL balance was decreased.
    #[serde(serialize_with = "serialize_b58")]
    pub from_b_sol_account: Pubkey,

    /// Recipient account whose stSOL balance was increased.
    #[serde(serialize_with = "serialize_b58")]
    pub to_st_sol_account: Pubkey,

    /// Whether we had to create the associated stSOL account. False if one existed already.
    pub created_associated_st_sol_account: bool,
}

impl fmt::Display for WithdrawOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.created_associated_st_sol_account {
            writeln!(f, "Created recipient stSOL account, it did not yet exist.")?;
        } else {
            writeln!(
                f,
                "Recipient stSOL account existed already before withdraw."
            )?;
        }
        writeln!(f, "Sender bSOL account:     {}", self.from_b_sol_account)?;
        writeln!(f, "Recipient stSOL account: {}", self.to_st_sol_account)?;
        Ok(())
    }
}

fn command_withdraw(
    config: &mut SnapshotConfig,
    opts: &AnkerWithdrawOpts,
) -> solido_cli_common::Result<WithdrawOutput> {
    let client = &mut config.client;
    let anker_account = client.get_account(opts.anker_address())?;
    let anker_program_id = anker_account.owner;
    let anker = client.get_anker(opts.anker_address())?;
    let solido = client.get_solido(&anker.solido)?;

    let mut instructions = Vec::new();
    let mut created_recipient = false;

    // The user can pass in a particular SPL token account to send from, but if
    // none is provided, we use the associated token account of the signer.
    let sender = if opts.from_b_sol_address() == &Pubkey::default() {
        spl_associated_token_account::get_associated_token_address(
            &config.signer.pubkey(),
            &anker.b_sol_mint,
        )
    } else {
        *opts.from_b_sol_address()
    };

    // Also for the recipient, we use the associated token account of the signer
    // if the user did not specify an account.
    let recipient = if opts.to_st_sol_address() == &Pubkey::default() {
        let recipient = spl_associated_token_account::get_associated_token_address(
            &config.signer.pubkey(),
            &solido.st_sol_mint,
        );
        if !config.client.account_exists(&recipient)? {
            let instr = spl_associated_token_account::create_associated_token_account(
                &config.signer.pubkey(),
                &config.signer.pubkey(),
                &solido.st_sol_mint,
            );
            instructions.push(instr);
            created_recipient = true;
        }
        recipient
    } else {
        *opts.to_st_sol_address()
    };

    let (st_sol_reserve_account, _bump_seed) =
        anker::find_st_sol_reserve_account(&anker_program_id, opts.anker_address());
    let (reserve_authority, _bump_seed) =
        anker::find_reserve_authority(&anker_program_id, opts.anker_address());

    let instr = anker::instruction::withdraw(
        &anker_program_id,
        &anker::instruction::WithdrawAccountsMeta {
            anker: *opts.anker_address(),
            solido: anker.solido,
            from_b_sol_account: sender,
            from_b_sol_authority: config.signer.pubkey(),
            to_st_sol_account: recipient,
            reserve_account: st_sol_reserve_account,
            reserve_authority,
            b_sol_mint: anker.b_sol_mint,
        },
        *opts.amount_b_sol(),
    );
    instructions.push(instr);

    config.sign_and_send_transaction(&instructions[..], &[config.signer])?;

    let result = WithdrawOutput {
        created_associated_st_sol_account: created_recipient,
        from_b_sol_account: sender,
        to_st_sol_account: recipient,
    };
    Ok(result)
}

#[derive(Serialize)]
pub struct ShowAnkerAuthoritiesOutput {
    #[serde(serialize_with = "serialize_b58")]
    pub anker_address: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub b_sol_mint_authority: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub st_sol_reserve_account: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub ust_reserve_account: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub reserve_authority: Pubkey,
}

impl fmt::Display for ShowAnkerAuthoritiesOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Anker address:         {}", self.anker_address,)?;
        writeln!(f, "bSOL Mint authority:   {}", self.b_sol_mint_authority)?;
        writeln!(f, "stSOL reserve account: {}", self.st_sol_reserve_account)?;
        writeln!(f, "UST reserve account:   {}", self.ust_reserve_account)?;
        writeln!(f, "Reserve authority:     {}", self.reserve_authority,)?;
        Ok(())
    }
}

/// Show Anker address derived for its Solido instance and Anker authorities.
pub fn command_show_anker_authorities(
    opts: &ShowAnkerAuthoritiesOpts,
) -> solido_cli_common::Result<ShowAnkerAuthoritiesOutput> {
    let (anker_address, _) =
        anker::find_instance_address(opts.anker_program_id(), opts.solido_address());
    let (b_sol_mint_authority, _) =
        anker::find_mint_authority(opts.anker_program_id(), &anker_address);
    let (reserve_authority, _) =
        anker::find_reserve_authority(opts.anker_program_id(), &anker_address);
    let (st_sol_reserve_account, _) =
        anker::find_st_sol_reserve_account(opts.anker_program_id(), &anker_address);
    let (ust_reserve_account, _) =
        anker::find_ust_reserve_account(opts.anker_program_id(), &anker_address);

    Ok(ShowAnkerAuthoritiesOutput {
        anker_address,
        b_sol_mint_authority,
        st_sol_reserve_account,
        ust_reserve_account,
        reserve_authority,
    })
}
