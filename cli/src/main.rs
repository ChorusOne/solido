use std::process::exit;

use clap::{
    crate_description, crate_name, crate_version, value_t, value_t_or_exit, App, AppSettings, Arg,
    SubCommand,
};

use solana_clap_utils::{
    input_parsers::pubkey_of,
    input_validators::{is_keypair, is_parsable, is_pubkey, is_url},
    keypair::signer_from_path,
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{Keypair, Signer},
};

use crate::helpers::{command_create_solido, NewStakePoolArgs, StakePoolArgs};

extern crate lazy_static;
extern crate spl_stake_pool;

mod helpers;
mod stake_pool_helpers;
type Error = Box<dyn std::error::Error>;
type CommandResult = Result<(), Error>;

struct Config {
    rpc_client: RpcClient,
    verbose: bool,
    manager: Box<dyn Signer>,
    staker: Box<dyn Signer>,
    fee_payer: Box<dyn Signer>,
    dry_run: bool,
}

fn main() {
    solana_logger::setup_with_default("solana=info");

    let matches = App::new(crate_name!())
        .about(crate_description!())
        .version(crate_version!())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg({
            let arg = Arg::with_name("config_file")
                .short("C")
                .long("config")
                .value_name("PATH")
                .takes_value(true)
                .global(true)
                .help("Configuration file to use");
            if let Some(ref config_file) = *solana_cli_config::CONFIG_FILE {
                arg.default_value(&config_file)
            } else {
                arg
            }
        })
        .arg(
            Arg::with_name("verbose")
                .long("verbose")
                .short("v")
                .takes_value(false)
                .global(true)
                .help("Show additional information"),
        )
        .arg(
            Arg::with_name("dry_run")
                .long("dry-run")
                .takes_value(false)
                .global(true)
                .help("Simulate transaction instead of executing"),
        )
        .arg(
            Arg::with_name("json_rpc_url")
                .long("url")
                .value_name("URL")
                .takes_value(true)
                .validator(is_url)
                .help("JSON RPC URL for the cluster.  Default from the configuration file."),
        )
        .arg(
            Arg::with_name("staker")
                .long("staker")
                .value_name("KEYPAIR")
                .validator(is_keypair)
                .takes_value(true)
                .help(
                    "Specify the stake pool staker. \
                     This may be a keypair file, the ASK keyword. \
                     Defaults to the client keypair.",
                ),
        )
        .arg(
            Arg::with_name("manager")
                .long("manager")
                .value_name("KEYPAIR")
                .validator(is_keypair)
                .takes_value(true)
                .help(
                    "Specify the stake pool manager. \
                     This may be a keypair file, the ASK keyword. \
                     Defaults to the client keypair.",
                ),
        )
        .arg(
            Arg::with_name("fee_payer")
                .long("fee-payer")
                .value_name("KEYPAIR")
                .validator(is_keypair)
                .takes_value(true)
                .help(
                    "Specify the fee-payer account. \
                     This may be a keypair file, the ASK keyword. \
                     Defaults to the client keypair.",
                ),
        )
        .subcommand(
            SubCommand::with_name("create")
                .about("Create a new lido")
                .arg(
                    Arg::with_name("stake-pool")
                        .long("stake-pool")
                        .short("s")
                        .validator(is_pubkey)
                        .value_name("STAKE-POOL")
                        .takes_value(true)
                        .help("Specifies a stake pool. If none is specified, one is created."),
                )
                .arg(
                    Arg::with_name("fee-numerator")
                        .long("fee-numerator")
                        .validator(is_parsable::<u64>)
                        .value_name("NUMBER")
                        .takes_value(true)
                        .help("Fee numerator, fee amount is numerator divided by denominator."),
                )
                .arg(
                    Arg::with_name("fee-denominator")
                        .long("fee-denominator")
                        .validator(is_parsable::<u64>)
                        .value_name("NUMBER")
                        .takes_value(true)
                        .help("Fee denominator, fee amount is numerator divided by denominator."),
                )
                .arg(
                    Arg::with_name("max-validators")
                        .long("max-validators")
                        .validator(is_parsable::<u64>)
                        .value_name("NUMBER")
                        .takes_value(true)
                        .help("Max number of validators included in the stake pool"),
                ),
        )
        .subcommand(SubCommand::with_name("deposit").about("Deposits to lido"))
        .get_matches();

    let mut wallet_manager = None;
    let config = {
        let cli_config = if let Some(config_file) = matches.value_of("config_file") {
            solana_cli_config::Config::load(config_file).unwrap_or_default()
        } else {
            solana_cli_config::Config::default()
        };
        let json_rpc_url = value_t!(matches, "json_rpc_url", String)
            .unwrap_or_else(|_| cli_config.json_rpc_url.clone());

        let staker = signer_from_path(
            &matches,
            &cli_config.keypair_path,
            "staker",
            &mut wallet_manager,
        )
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });
        let manager = signer_from_path(
            &matches,
            &cli_config.keypair_path,
            "manager",
            &mut wallet_manager,
        )
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });
        let fee_payer = signer_from_path(
            &matches,
            &cli_config.keypair_path,
            "fee_payer",
            &mut wallet_manager,
        )
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });
        let verbose = matches.is_present("verbose");
        let dry_run = matches.is_present("dry_run");

        Config {
            rpc_client: RpcClient::new_with_commitment(json_rpc_url, CommitmentConfig::confirmed()),
            verbose,
            manager,
            staker,
            fee_payer,
            dry_run,
        }
    };

    let _ = match matches.subcommand() {
        ("create", Some(arg_matches)) => {
            let stake_pool = pubkey_of(arg_matches, "stake-pool");
            let stake_pool_args = match stake_pool {
                Some(pubkey) => StakePoolArgs::Existing(pubkey),
                None => {
                    let keypair = Keypair::new();
                    println!("Creating stake pool {}", &keypair.pubkey());
                    let numerator = value_t_or_exit!(arg_matches, "fee-numerator", u64);
                    let denominator = value_t_or_exit!(arg_matches, "fee-denominator", u64);
                    let max_validators = value_t_or_exit!(arg_matches, "max-validators", u32);
                    StakePoolArgs::New(NewStakePoolArgs {
                        keypair,
                        numerator,
                        denominator,
                        max_validators,
                    })
                }
            };
            command_create_solido(&config, stake_pool_args)
        }

        _ => unreachable!(),
    }
    .map_err(|err| {
        eprintln!("{}", err);
        exit(1);
    });
}
