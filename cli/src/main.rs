use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use anchor_client::Cluster;
use anchor_client::Program;
use clap::Clap;
use helpers::AddValidatorOpts;
use helpers::CreateValidatorStakeAccountOpts;
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Keypair};

use crate::helpers::command_create_validator_stake_account;
use crate::helpers::{
    command_add_validator, command_create_solido, get_anchor_program, CreateSolidoOpts,
};
use crate::multisig::MultisigOpts;

extern crate lazy_static;
extern crate spl_stake_pool;

mod helpers;
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

    The fees are distributed among the insurance, treasury, validators, and the
    manager, according to the ratio

    «insurance-fee» : «treasury-fee» : «validation-fee» : «manager-fee»

    For example, if all fees are set to 1, then the four parties would each
    receive 25% of the fees. Subsequently, the validation fee is divided equally
    among all validators.
    ")]
    CreateSolido(CreateSolidoOpts),

    /// Add a new validator
    AddValidator(AddValidatorOpts),

    /// Create a Validator Stake Account
    CreateValidatorStakeAccount(CreateValidatorStakeAccountOpts),

    /// Interact with a deployed Multisig program for governance tasks.
    Multisig(MultisigOpts),
}

/// Determines which network to connect to, and who pays the fees.
pub struct Config {
    program: Program,
    // rpc_client: RpcClient,
    fee_payer: Keypair,
    dry_run: bool,
    output_mode: OutputMode,
}

impl Config {
    pub fn rpc(&self) -> RpcClient {
        return self.program.rpc();
    }
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
    let keypair = read_keypair_file(&payer_keypair_path)
        .unwrap_or_else(|_| panic!("Failed to read key pair from {:?}.", payer_keypair_path));

    // TODO: This is a bit hacky :|
    // We need to pass the keypair to Anchor's program by value and to our config by reference.
    // Cluster has to be passed as value as well
    let key_pair_copy =
        Keypair::from_bytes(&keypair.to_bytes()).expect("Keypair returned an invalid secret");
    let config = Config {
        // Set the multisig_program_id to an invalid program, we use the program
        // just to get the rpc client, when we need to use the multisig program,
        // we'll create another instance of it.
        program: get_anchor_program(
            Cluster::from_str(&opts.cluster.to_string()).unwrap(),
            key_pair_copy,
            &Pubkey::new_from_array([255u8; 32]),
        ),
        // For now, we'll assume that the provided key pair fulfils all of these
        // roles. We need a better way to configure keys in the future.
        fee_payer: keypair,
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
        SubCommand::Multisig(cmd_opts) => {
            multisig::main(config, opts.cluster, opts.output_mode, cmd_opts);
        }
        SubCommand::CreateValidatorStakeAccount(cmd_opts) => {
            if let Some(output) =
                command_create_validator_stake_account(config, opts.cluster, cmd_opts)
                    .expect("Failed to create a validator stake account")
            {
                print_output(opts.output_mode, &output);
            }
        }
        SubCommand::AddValidator(cmd_opts) => {
            if let Some(output) = command_add_validator(config, opts.cluster, cmd_opts)
                .expect("Failed to add a validator")
            {
                print_output(opts.output_mode, &output);
            }
        }
    }
}
