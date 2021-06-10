use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use anchor_client::Cluster;
use clap::Clap;
use helpers::AddRemoveMaintainerOpts;
use helpers::AddValidatorOpts;
use helpers::CreateValidatorStakeAccountOpts;
use helpers::ShowSolidoOpts;
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_remote_wallet::locator::Locator;
use solana_remote_wallet::remote_keypair::generate_remote_keypair;
use solana_remote_wallet::remote_wallet::maybe_wallet_manager;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::derivation_path::DerivationPath;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::read_keypair_file;
use solana_sdk::signer::Signer;

use crate::helpers::command_add_maintainer;
use crate::helpers::command_create_validator_stake_account;
use crate::helpers::command_remove_maintainer;
use crate::helpers::command_show_solido;
use crate::helpers::{command_add_validator, command_create_solido, CreateSolidoOpts};
use crate::maintenance::PerformMaintenanceOpts;
use crate::multisig::MultisigOpts;

extern crate lazy_static;
extern crate spl_stake_pool;

mod helpers;
mod maintenance;
mod multisig;
mod spl_token_utils;
mod stake_pool_helpers;
mod util;

type Error = Box<dyn std::error::Error>;

#[derive(Copy, Clone, Debug)]
pub enum OutputMode {
    /// Output human-readable text to stdout.
    Text,

    /// Output machine-readable json to stdout.
    Json,
}

impl FromStr for OutputMode {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<OutputMode, &'static str> {
        match s {
            "text" => Ok(OutputMode::Text),
            "json" => Ok(OutputMode::Json),
            _ => Err("Invalid output mode, expected 'text' or 'json'."),
        }
    }
}

/// Solido -- Interact with Lido for Solana.
#[derive(Clap, Debug)]
struct Opts {
    /// The keypair to sign and pay with. [default: ~/.config/solana/id.json]
    #[clap(long)]
    keypair_path: Option<PathBuf>,

    /// Address of the Multisig program.
    #[clap(long)]
    multisig_program_id: Pubkey,

    /// Cluster to connect to (mainnet, testnet, devnet, localnet, or url).
    #[clap(long, default_value = "localnet")]
    // Although we don't use Anchor here, we use it’s `Cluster` type because
    // it has a convenient `FromStr` implementation.
    cluster: Cluster,

    /// Whether to output text or json.
    #[clap(long = "output", default_value = "text", possible_values = &["text", "json"])]
    output_mode: OutputMode,

    #[clap(subcommand)]
    subcommand: SubCommand,
}

#[derive(Clap, Debug)]
enum SubCommand {
    /// Create a new Lido for Solana instance.
    #[clap(after_help = r"ACCOUNTS:

    This sets up a few things:

    * An SPL token mint for stake pool tokens.
    * An SPL token mint for stSOL.
    * stSOL-denominated SPL token accounts for fee receivers.
    * The stake pool managed by this Solido instance.
    * The Solido instance itself.

FEES:

    Of the validation rewards that the stake pool receives, a fraction
    «fee-numerator» / «fee-denominator» gets paid out as fees. The remaining
    rewards get distributed implicitly to stSOL holders because they now own
    a share of a larger pool of SOL.

    The fees are distributed among the treasury, validators, and the
    developer, according to the ratio

    «treasury-fee» : «validation-fee» : «developer-fee»

    For example, if the fees are set to a 1 : 2 : 1 proportion, then the
    treasury and developers would receive 50% of the fees, and the validation
    would receive the remaining 50%. Subsequently, the validation fee is divided
    equally among all validators.
    ")]
    CreateSolido(CreateSolidoOpts),

    /// Adds a new validator
    AddValidator(AddValidatorOpts),
    /// Adds a maintainer to the Solido instance
    AddMaintainer(AddRemoveMaintainerOpts),
    /// Adds a maintainer to the Solido instance
    RemoveMaintainer(AddRemoveMaintainerOpts),
    /// Create a Validator Stake Account
    CreateValidatorStakeAccount(CreateValidatorStakeAccountOpts),

    /// Show an instance of solido in detail
    ShowSolido(ShowSolidoOpts),

    /// Execute periodic maintenance logic.
    PerformMaintenance(PerformMaintenanceOpts),

    /// Interact with a deployed Multisig program for governance tasks.
    Multisig(MultisigOpts),
}

/// Determines which network to connect to, and who pays the fees.
pub struct Config<'a> {
    /// Address of the Multisig program.
    multisig_program_id: Pubkey,
    /// Program instance, so we can call RPC methods.
    rpc: RpcClient,
    /// Reference to a signer, can be a keypair or ledger device.
    signer: &'a dyn Signer,
    /// TODO: Not used.
    dry_run: bool,
    /// output mode, can be json or text.
    output_mode: OutputMode,
}

/// Resolve ~/.config/solana/id.json.
fn get_default_keypair_path() -> PathBuf {
    let home = std::env::var("HOME").expect("Expected $HOME to be set.");
    let mut path = PathBuf::from(home);
    path.push(".config/solana/id.json");
    path
}

fn print_output<Output: fmt::Display + Serialize>(mode: OutputMode, output: &Output) {
    match mode {
        OutputMode::Text => println!("{}", output),
        OutputMode::Json => {
            let json_string =
                serde_json::to_string_pretty(output).expect("Failed to serialize output as json.");
            println!("{}", json_string);
        }
    }
}

fn main() {
    let opts = Opts::parse();
    solana_logger::setup_with_default("solana=info");

    let payer_keypair_path = match opts.keypair_path {
        Some(path) => path,
        None => get_default_keypair_path(),
    };
    // Get a boxed signer that lives long enough for us to use it in the Config.
    let boxed_signer: Box<dyn Signer> = if payer_keypair_path.starts_with("usb://") {
        let hw_wallet = maybe_wallet_manager()
            .unwrap_or_else(|err| panic!("Remote wallet found, but failed to establish protocol. Maybe the Solana app is not open: {}", err))
            .unwrap_or_else(|| panic!("Failed to find a remote wallet, maybe Ledger is not connected or locked."));
        Box::new(
            generate_remote_keypair(
                Locator::new_from_path(
                    payer_keypair_path
                        .into_os_string()
                        .into_string()
                        .expect("Should have failed before"),
                )
                .unwrap_or_else(|err| panic!("Failed reading URL: {}", err)),
                DerivationPath::default(),
                &hw_wallet,
                false,    /* Confirm public key */
                "Solido", /* When multiple wallets are connected, used to display a hint */
            )
            .unwrap_or_else(|err| panic!("Failed to contact remote wallet {}", err)),
        )
    } else {
        Box::new(
            read_keypair_file(&payer_keypair_path).unwrap_or_else(|_| {
                panic!("Failed to read key pair from {:?}.", payer_keypair_path)
            }),
        )
    };
    // Get reference from signer
    let signer = &*boxed_signer;

    let config = Config {
        rpc: RpcClient::new_with_commitment(
            opts.cluster.url().to_string(),
            CommitmentConfig::confirmed(),
        ),
        multisig_program_id: opts.multisig_program_id,
        // For now, we'll assume that the provided key pair fulfils all of these
        // roles. We need a better way to configure keys in the future.
        // fee_payer: keypair,
        signer,
        // TODO: Do we want a dry-run option in the MVP at all?
        dry_run: false,
        output_mode: opts.output_mode,
    };
    match opts.subcommand {
        SubCommand::CreateSolido(cmd_opts) => {
            let output =
                command_create_solido(config, cmd_opts).expect("Failed to create Solido instance.");
            print_output(opts.output_mode, &output);
        }
        SubCommand::Multisig(cmd_opts) => multisig::main(config, opts.output_mode, cmd_opts),
        SubCommand::CreateValidatorStakeAccount(cmd_opts) => {
            let output = command_create_validator_stake_account(config, cmd_opts)
                .expect("Failed to create validator stake account");
            print_output(opts.output_mode, &output);
        }
        SubCommand::PerformMaintenance(cmd_opts) => {
            // For now, this does one maintenance iteration. In the future we
            // might add a daemon mode that runs continuously, and which logs
            // to stdout and exposes Prometheus metrics (also to monitor Solido,
            // not just the maintenance itself).
            maintenance::perform_maintenance(&config, cmd_opts)
                .expect("Failed to perform maintenance.");
        }
        SubCommand::AddValidator(cmd_opts) => {
            let output = command_add_validator(config, cmd_opts).expect("Failed to add validator");
            print_output(opts.output_mode, &output);
        }
        SubCommand::AddMaintainer(cmd_opts) => {
            let output =
                command_add_maintainer(config, cmd_opts).expect("Failed to add maintainer");
            print_output(opts.output_mode, &output);
        }
        SubCommand::RemoveMaintainer(cmd_opts) => {
            let output =
                command_remove_maintainer(config, cmd_opts).expect("Failed to remove maintainer");
            print_output(opts.output_mode, &output);
        }
        SubCommand::ShowSolido(cmd_opts) => {
            let output = command_show_solido(config, cmd_opts).expect("Failed to show Solido data");
            print_output(opts.output_mode, &output);
        }
    }
}
