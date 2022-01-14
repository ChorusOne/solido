// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::fmt;
use std::path::PathBuf;

use clap::Clap;
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_remote_wallet::locator::Locator;
use solana_remote_wallet::remote_keypair::generate_remote_keypair;
use solana_remote_wallet::remote_wallet::maybe_wallet_manager;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::derivation_path::DerivationPath;
use solana_sdk::signature::{read_keypair, read_keypair_file};
use solana_sdk::signer::Signer;
use solido_cli_common::error::{Abort, CliError, Error};
use solido_cli_common::snapshot::{Config, OutputMode, SnapshotClient};

use crate::commands_anker::AnkerOpts;
use crate::commands_multisig::MultisigOpts;
use crate::commands_solido::{
    command_add_maintainer, command_add_validator, command_create_solido,
    command_deactivate_validator, command_deposit, command_remove_maintainer, command_show_solido,
    command_show_solido_authorities, command_withdraw,
};
use crate::config::*;

mod anker_state;
mod commands_anker;
mod commands_multisig;
mod commands_solido;
mod config;
mod daemon;
mod maintenance;
mod prometheus;
mod serialization_utils;
mod spl_token_utils;

/// Solido -- Interact with Lido for Solana.
// While it is nice to have Clap handle all inputs, we also want to read
// from a config file and environmental variables. This fields are duplicated
// in the `GeneralOpts` struct, we use a function to merge this from our own
// struct.
// This is due to the inability of our structs to handle Clap's sub-commands.
// Some values are going to be overwritten by `GeneralOpts`, but
// we write the default values on the rustdoc so Clap can print them in help
// messages.
#[derive(Clap, Debug)]
#[clap(after_long_help = r#"CONFIGURATION:
    All of the options of this program can also be provided as an environment
    variable with "SOLIDO_" prefix. E.g. to provide --keypair-path, set the
    SOLIDO_KEYPAIR_PATH environment variable.

    Alternatively, all of the options of this program can also be provided in a
    json config file (the location of which must be provided with --config or
    SOLIDO_CONFIG). This json file must contain an object with one key per
    option. E.g. to provide --cluster and --keypair-path, write the following
    config file:

    {
      "cluster": "https://api.mainnet-beta.solana.com",
      "keypair_path": "/path/to/id.json"
    }"#)]
struct Opts {
    /// The contents of a keypair file to sign and pay with, as json array.
    ///
    /// This is mainly useful when loading a keypair from e.g. Hashicorp
    /// Vault into the SOLIDO_KEYPAIR environment variable. This takes
    /// precedence over --keypair-path.
    ///
    /// Note, when used in the config file, this must be a string that
    /// contains the contents of the keypair file (which itself is a json
    /// array of numbers), it shouldn't be an array directly.
    // Overwritten by `GeneralOpts` if None.
    #[clap(long)]
    keypair: Option<String>,

    /// The keypair to sign and pay with. [default: ~/.config/solana/id.json]
    // Overwritten by `GeneralOpts` if None.
    #[clap(long)]
    keypair_path: Option<PathBuf>,

    /// URL of cluster to connect to (e.g., https://api.devnet.solana.com for solana devnet) [default: http://127.0.0.1:8899]
    // Overwritten by `GeneralOpts` if None.
    #[clap(long)]
    cluster: Option<String>,

    /// Whether to output text or json. [default: "text"]
    // Overwritten by `GeneralOpts` if None.
    #[clap(long = "output", possible_values = &["text", "json"])]
    output_mode: Option<OutputMode>,

    #[clap(subcommand)]
    subcommand: SubCommand,

    /// Optional config path
    #[clap(long)]
    config: Option<PathBuf>,
}

impl Opts {
    fn merge_with_config_and_environment(&mut self) -> Option<ConfigFile> {
        let mut general_opts = GeneralOpts::default();
        let config_file = self.config.as_ref().map(|p| read_config(p.as_path()));
        general_opts.merge_with_config_and_environment(config_file.as_ref());

        self.keypair = self
            .keypair
            .take()
            .or_else(|| Some(general_opts.keypair().to_owned()));
        self.keypair_path = self
            .keypair_path
            .take()
            .or_else(|| Some(general_opts.keypair_path().to_owned()));
        self.cluster = self
            .cluster
            .take()
            .or_else(|| Some(general_opts.cluster().to_owned()));
        self.output_mode = self
            .output_mode
            .take()
            .or_else(|| Some(general_opts.output_mode().to_owned()));
        config_file
    }
}

#[derive(Clap, Debug)]
enum SubCommand {
    /// Create a new Lido for Solana instance.
    #[clap(after_help = r"ACCOUNTS

    This sets up a few things:

    * An SPL token mint for stSOL.
    * stSOL-denominated SPL token accounts for fee receivers.
    * The Solido instance itself.

REWARDS

    Solido takes a fraction of the rewards that it receives as fees. The
    remainder gets distributed implicitly to stSOL holders because they now own
    a share of a larger pool of SOL.

    The SOL rewards get split according to the ratio T : V : D : A, where

      T: Treasury fee share
      V: Validation fee share (this is for all validators combined)
      D: Developer fee share
      A: stSOL value appreciation share

    For example, if the reward distribution is set to '5 : 3 : 2 : 90', then 90%
    of the rewards go to stSOL value appreciation, and 10% go to fees. Of those
    fees, 50% go to the treasury, 30% are divided among validators, and 20% goes
    to the developer.
    ")]
    CreateSolido(CreateSolidoOpts),

    /// Adds a new validator.
    AddValidator(AddValidatorOpts),

    /// Deactivates a validator and initiates the removal process.
    DeactivateValidator(DeactivateValidatorOpts),

    /// Adds a maintainer to the Solido instance.
    AddMaintainer(AddRemoveMaintainerOpts),

    /// Removes a maintainer from the Solido instance.
    RemoveMaintainer(AddRemoveMaintainerOpts),

    /// Deposit some SOL, receive stSOL in return.
    ///
    /// The recipient will be set to the associated token account for the signer.
    /// If the associated token account does not yet exist, it will be created.
    Deposit(DepositOpts),

    /// Withdraw stSOL, receive a delegated stake account in return.
    ///
    /// The amount of SOL is calculated and stored in the returned stake.
    Withdraw(WithdrawOpts),

    /// Show an instance of Solido in detail
    ShowSolido(ShowSolidoOpts),

    /// Show Solido authorities, even if the instance is not initialized.
    ///
    /// This is useful for testing, and when setting up a token mint ahead of
    /// time, to be used later when initializing the Solido instance.
    ShowAuthorities(ShowSolidoAuthoritiesOpts),

    /// Execute one iteration of periodic maintenance logic.
    ///
    /// This is mainly useful for testing. To perform maintenance continuously,
    /// use 'run-maintainer' instead.
    PerformMaintenance(PerformMaintenanceOpts),

    /// Start the maintainer daemon.
    RunMaintainer(RunMaintainerOpts),

    /// Interact with a deployed Multisig program for governance tasks.
    Multisig(MultisigOpts),

    /// Interact with the Anker (Anchor Protocol integration) program.
    Anker(AnkerOpts),
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
    let mut opts = Opts::parse();
    let config_file = opts.merge_with_config_and_environment();

    solana_logger::setup_with_default("solana=info");

    // Note, the unwraps below are safe, because `merge_with_config_and_environment`
    // ensures that all values are provided; it’s just that for the derived Clap
    // parser, the options are all optional.
    let signer = if opts.keypair.as_ref().unwrap() == "" {
        let payer_keypair_path = opts.keypair_path;
        get_signer_from_path(payer_keypair_path.unwrap())
            .ok_or_abort_with("Failed to load signer keypair.")
    } else {
        get_signer_from_key(opts.keypair.unwrap())
    };

    // Use commitment level "confirmed" as a middle ground between getting a recent
    // state, and not having to wait for a long time for transactions to be confirmed.
    // This means that we can sometimes read states that do not get finalized, when
    // there is a reorg. We considered using "finalized" commitment level to avoid
    // this, but then we have to wait 32 slots after every transaction, and on top
    // of that we base transactions on old states, which increases the likelihood
    // of them failing when executed. So we go back to "confirmed" after all. See
    // also https://github.com/ChorusOne/solido/pull/437.
    let rpc_client =
        RpcClient::new_with_commitment(opts.cluster.unwrap(), CommitmentConfig::confirmed());
    let snapshot_client = SnapshotClient::new(rpc_client);

    let output_mode = opts.output_mode.unwrap();
    let mut config = Config {
        client: snapshot_client,
        signer: &*signer,
        output_mode,
    };

    merge_with_config_and_environment(&mut opts.subcommand, config_file.as_ref());
    match opts.subcommand {
        SubCommand::Anker(cmd_opts) => commands_anker::main(&mut config, &cmd_opts),
        SubCommand::CreateSolido(cmd_opts) => {
            let result = config.with_snapshot(|config| command_create_solido(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to create Solido instance.");
            print_output(output_mode, &output);
        }
        SubCommand::Multisig(cmd_opts) => commands_multisig::main(&mut config, cmd_opts),
        SubCommand::PerformMaintenance(cmd_opts) => {
            // This command only performs one iteration, `RunMaintainer` runs continuously.
            let result = config
                .with_snapshot(|config| maintenance::run_perform_maintenance(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to perform maintenance.");
            match (output_mode, output) {
                (OutputMode::Text, None) => {
                    println!("Nothing done, there was no maintenance to perform.")
                }
                (OutputMode::Json, None) => println!("null"),

                (mode, Some(output)) => print_output(mode, &output),
            }
        }
        SubCommand::RunMaintainer(cmd_opts) => {
            daemon::main(&mut config, &cmd_opts);
        }
        SubCommand::AddValidator(cmd_opts) => {
            let result = config.with_snapshot(|config| command_add_validator(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to add validator.");
            print_output(output_mode, &output);
        }
        SubCommand::DeactivateValidator(cmd_opts) => {
            let result =
                config.with_snapshot(|config| command_deactivate_validator(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to deactivate validator.");
            print_output(output_mode, &output);
        }
        SubCommand::AddMaintainer(cmd_opts) => {
            let result = config.with_snapshot(|config| command_add_maintainer(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to add maintainer.");
            print_output(output_mode, &output);
        }
        SubCommand::RemoveMaintainer(cmd_opts) => {
            let result =
                config.with_snapshot(|config| command_remove_maintainer(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to remove maintainer.");
            print_output(output_mode, &output);
        }
        SubCommand::ShowSolido(cmd_opts) => {
            let result = config.with_snapshot(|config| command_show_solido(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to show Solido data.");
            print_output(output_mode, &output);
        }
        SubCommand::ShowAuthorities(solido_pubkey) => {
            let result =
                config.with_snapshot(|_config| command_show_solido_authorities(&solido_pubkey));
            let output =
                result.ok_or_abort_with("Failed to show authorities for Solido public key.");
            print_output(output_mode, &output);
        }
        SubCommand::Deposit(cmd_opts) => {
            let result = command_deposit(&mut config, &cmd_opts);
            let output = result.ok_or_abort_with("Failed to deposit.");
            print_output(output_mode, &output);
        }
        SubCommand::Withdraw(cmd_opts) => {
            let result = command_withdraw(&mut config, &cmd_opts);
            let output = result.ok_or_abort_with("Failed to withdraw.");
            print_output(output_mode, &output);
        }
    }
}

fn merge_with_config_and_environment(
    subcommand: &mut SubCommand,
    config_file: Option<&ConfigFile>,
) {
    match subcommand {
        SubCommand::Anker(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::CreateSolido(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::AddValidator(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::DeactivateValidator(opts) => {
            opts.merge_with_config_and_environment(config_file)
        }
        SubCommand::AddMaintainer(opts) | SubCommand::RemoveMaintainer(opts) => {
            opts.merge_with_config_and_environment(config_file)
        }
        SubCommand::Deposit(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::Withdraw(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::ShowSolido(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::ShowAuthorities(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::PerformMaintenance(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::Multisig(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::RunMaintainer(opts) => opts.merge_with_config_and_environment(config_file),
    }
}

/// Parse a keypair path of the form "usb://ledger?key=0".
pub fn parse_remote_wallet_details(uri: &str) -> Result<(DerivationPath, Locator), Error> {
    use std::convert::TryFrom;
    use uriparse::uri_reference::URIReference;

    let uri_invalid_msg =
        "Failed to parse usb:// keypair path. It must be of the form 'usb://ledger?key=0'.";

    let uri_ref =
        URIReference::try_from(uri).map_err(|err| CliError::with_cause(uri_invalid_msg, err))?;

    let derivation_path = DerivationPath::from_uri_key_query(&uri_ref)
        .map_err(|err| CliError::with_cause(uri_invalid_msg, err))?
        // If there is no ?key= query parameter, then use the default derivation path.
        .unwrap_or_default();

    let locator = Locator::new_from_uri(&uri_ref)
        .map_err(|err| CliError::with_cause(uri_invalid_msg, err))?;

    Ok((derivation_path, locator))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_remote_wallet_details() {
        use derivation_path::ChildIndex;

        let (derivation_path, _) = parse_remote_wallet_details("usb://ledger").ok().unwrap();
        // /'44/'501 is added by default for all Solana derivation paths.
        assert_eq!(
            derivation_path.path(),
            [ChildIndex::Hardened(44), ChildIndex::Hardened(501)]
        );

        let (derivation_path, _) = parse_remote_wallet_details("usb://ledger?key=0")
            .ok()
            .unwrap();
        assert_eq!(
            derivation_path.path(),
            [
                ChildIndex::Hardened(44),
                ChildIndex::Hardened(501),
                ChildIndex::Hardened(0)
            ]
        );

        let (derivation_path, _) = parse_remote_wallet_details("usb://ledger?key=0/1")
            .ok()
            .unwrap();
        assert_eq!(
            derivation_path.path(),
            [
                ChildIndex::Hardened(44),
                ChildIndex::Hardened(501),
                ChildIndex::Hardened(0),
                ChildIndex::Hardened(1)
            ]
        );

        let (derivation_path, _) = parse_remote_wallet_details(
            "usb://ledger/BsNsvfXqQTtJnagwFWdBS7FBXgnsK8VZ5CmuznN85swK",
        )
        .ok()
        .unwrap();
        assert_eq!(
            derivation_path.path(),
            [ChildIndex::Hardened(44), ChildIndex::Hardened(501)]
        );

        let (derivation_path, _) = parse_remote_wallet_details(
            "usb://ledger/BsNsvfXqQTtJnagwFWdBS7FBXgnsK8VZ5CmuznN85swK?key=2",
        )
        .ok()
        .unwrap();
        assert_eq!(
            derivation_path.path(),
            [
                ChildIndex::Hardened(44),
                ChildIndex::Hardened(501),
                ChildIndex::Hardened(2)
            ]
        );

        let (derivation_path, _) = parse_remote_wallet_details(
            "usb://ledger/BsNsvfXqQTtJnagwFWdBS7FBXgnsK8VZ5CmuznN85swK?key=2/3",
        )
        .ok()
        .unwrap();
        assert_eq!(
            derivation_path.path(),
            [
                ChildIndex::Hardened(44),
                ChildIndex::Hardened(501),
                ChildIndex::Hardened(2),
                ChildIndex::Hardened(3)
            ]
        );

        assert!(parse_remote_wallet_details("usb://ledger?key=not-an-integer").is_err());
        assert!(parse_remote_wallet_details("usb://ledger?foo=bar").is_err());
        assert!(parse_remote_wallet_details("usb://ledger/not-a-key").is_err());
    }
}

fn get_signer_from_key(key_json: String) -> Box<dyn Signer> {
    use std::io::Cursor;
    let mut cursor = Cursor::new(key_json.as_bytes());
    Box::new(
        read_keypair(&mut cursor)
            .expect("Failed to deserialize keypair. Is it a json array of numbers?"),
    )
}

// Get a boxed signer that lives long enough for us to use it in the Config.
fn get_signer_from_path(payer_keypair_path: PathBuf) -> Result<Box<dyn Signer>, Error> {
    let boxed_signer: Box<dyn Signer> = if payer_keypair_path.starts_with("usb://") {
        let uri = payer_keypair_path
            .into_os_string()
            .into_string()
            .map_err(|_| {
                CliError::new("A keypair path that starts with usb:// must be valid UTF-8.")
            })?;
        // Parse the uri before we try to connect, so we can diagnose uri format issues early.
        let (derivation_path, locator) = parse_remote_wallet_details(&uri)?;
        let hw_wallet = maybe_wallet_manager()
            .map_err(|err| CliError::with_cause("Remote wallet found, but failed to establish protocol. Maybe the Solana app is not open.", err))?
            .ok_or_else(|| CliError::new("Failed to find a remote wallet, maybe Ledger is not connected or locked."))?;

        // When using a Ledger hardware wallet, confirm the public key of the
        // key to sign with on its display, so users can be sure that they
        // selected the right key.
        let confirm_public_key = true;

        Box::new(
            generate_remote_keypair(
                locator,
                derivation_path,
                &hw_wallet,
                confirm_public_key,
                "Solido", /* When multiple wallets are connected, used to display a hint */
            )
            .expect("Failed to contact remote wallet"),
        )
    } else {
        Box::new(
            read_keypair_file(&payer_keypair_path).expect("Failed to read key pair from file."),
        )
    };
    Ok(boxed_signer)
}
