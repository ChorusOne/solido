use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use anchor_client::Cluster;
use clap::Clap;
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::{read_keypair_file, Keypair};

use crate::helpers::{command_create_solido, CreateSolidoOpts};
use crate::multisig::MultisigOpts;

extern crate lazy_static;
extern crate spl_stake_pool;

mod helpers;
mod multisig;
mod stake_pool_helpers;
type Error = Box<dyn std::error::Error>;

#[derive(Debug)]
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
    CreateSolido(CreateSolidoOpts),

    /// Interact with a deployed Multisig program for governance tasks.
    Multisig(MultisigOpts),
}

/// Determines which network to connect to, and who pays the fees.
pub struct Config<'a> {
    rpc_client: RpcClient,
    manager: &'a Keypair,
    staker: &'a Keypair,
    fee_payer: &'a Keypair,
    dry_run: bool,
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
    let keypair = read_keypair_file(&payer_keypair_path).expect(&format!(
        "Failed to read key pair from {:?}.",
        payer_keypair_path
    ));

    let config = Config {
        rpc_client: RpcClient::new_with_commitment(
            opts.cluster.url().to_string(),
            CommitmentConfig::confirmed(),
        ),
        // For now, we'll assume that the provided key pair fulfils all of these
        // roles. We need a better way to configure keys in the future.
        manager: &keypair,
        staker: &keypair,
        fee_payer: &keypair,
        // TODO: Do we want a dry-run option in the MVP at all?
        dry_run: false,
    };

    match opts.subcommand {
        SubCommand::CreateSolido(cmd_opts) => {
            let output = command_create_solido(&config, cmd_opts)
                .expect("Failed to create Solido instance.");
            print_output(opts.output_mode, &output);
        }
        SubCommand::Multisig(cmd_opts) => {
            let payer = keypair;
            multisig::main(payer, opts.cluster, opts.output_mode, cmd_opts);
        }
    }
}
