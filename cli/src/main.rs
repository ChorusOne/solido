// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::fmt;
use std::path::PathBuf;

use clap::Clap;
use helpers::command_show_solido_authorities;
use helpers::command_withdraw;
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_remote_wallet::locator::Locator;
use solana_remote_wallet::remote_keypair::generate_remote_keypair;
use solana_remote_wallet::remote_wallet::maybe_wallet_manager;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::derivation_path::DerivationPath;
use solana_sdk::instruction::Instruction;
use solana_sdk::signature::{read_keypair_file, Signature};
use solana_sdk::signer::Signer;
use solana_sdk::signers::Signers;
use solana_sdk::transaction::Transaction;

use crate::config::*;
use crate::error::{Abort, CliError, Error};
use crate::helpers::{
    command_add_maintainer, command_add_validator, command_create_solido, command_deposit,
    command_remove_maintainer, command_show_solido,
};
use crate::multisig::MultisigOpts;
use crate::snapshot::{Snapshot, SnapshotClient};

mod config;
mod daemon;
mod error;
mod helpers;
mod maintenance;
mod multisig;
mod prometheus;
mod snapshot;
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
struct Opts {
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

    /// Adds a new validator
    AddValidator(AddValidatorOpts),

    /// Adds a maintainer to the Solido instance
    AddMaintainer(AddRemoveMaintainerOpts),

    /// Adds a maintainer to the Solido instance
    RemoveMaintainer(AddRemoveMaintainerOpts),

    /// Deposit some SOL, receive stSOL in return.
    ///
    /// The recipient will be set to the associated token account for the signer.
    /// If the associated token account does not yet exist, it will be created.
    Deposit(DepositOpts),

    /// Withdraw StSOL, receive a delegated stake account in return.
    ///
    /// The amount of SOL is calculated and stored in the returned stake.
    Withdraw(WithdrawOpts),

    /// Show an instance of solido in detail
    ShowSolido(ShowSolidoOpts),

    /// Show Solido authorities, useful for testing and when specifying an existing token mint.
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
}

/// Determines which network to connect to, and who pays the fees.
pub struct Config<'a, T> {
    /// RPC client augmented with snapshot functionality.
    client: T,
    /// Reference to a signer, can be a keypair or ledger device.
    signer: &'a dyn Signer,
    /// output mode, can be json or text.
    output_mode: OutputMode,
}

/// Program configuration, and a snapshot of accounts.
///
/// Accept this in functions that just want to read from a consistent chain
/// state, without handling retry logic.
pub type SnapshotConfig<'a> = Config<'a, Snapshot<'a>>;

/// Program configuration, and a client for making snapshots.
///
/// Accept this in functions that need to take a snapshot of the on-chain state
/// at different times. In practice, that's only the long-running maintenance
/// daemon.
pub type SnapshotClientConfig<'a> = Config<'a, SnapshotClient>;

impl<'a> SnapshotClientConfig<'a> {
    pub fn with_snapshot<F, T>(&mut self, mut f: F) -> Result<T, Error>
    where
        F: FnMut(&mut SnapshotConfig) -> snapshot::Result<T>,
    {
        let signer = self.signer;
        let output_mode = self.output_mode;
        self.client.with_snapshot(|snapshot| {
            let mut config = SnapshotConfig {
                client: snapshot,
                signer,
                output_mode,
            };
            f(&mut config)
        })
    }
}

impl<'a> SnapshotConfig<'a> {
    pub fn sign_transaction<T: Signers>(
        &mut self,
        instructions: &[Instruction],
        signers: &T,
    ) -> snapshot::Result<Transaction> {
        let mut tx = Transaction::new_with_payer(instructions, Some(&self.signer.pubkey()));
        let recent_blockhash = self.client.get_recent_blockhash()?;
        tx.sign(signers, recent_blockhash);
        Ok(tx)
    }

    pub fn sign_and_send_transaction<T: Signers>(
        &mut self,
        instructions: &[Instruction],
        signers: &T,
    ) -> snapshot::Result<Signature> {
        let transaction = self.sign_transaction(instructions, signers)?;
        let signature_result = match self.output_mode {
            OutputMode::Text => {
                // In text mode, we can display a spinner.
                self.client
                    .send_and_confirm_transaction_with_spinner(&transaction)
            }
            OutputMode::Json => {
                // In json mode, printing a spinner to stdout would break the
                // json that we also print to stdout, so opt for the silent
                // version.
                self.client.send_and_confirm_transaction(&transaction)
            }
        };

        // Warn the user for one particular footgun.
        match signature_result {
            Err(ref err) if error::might_have_executed(err) => {
                eprintln!(
                    "Warning: The RPC returned an error, but the transaction \
                     might still have been executed. Check the signer's address \
                     on a block explorer before continuing. Beware that timestamps \
                     shown on explorer.solana.com can be off by weeks, check the slot \
                     number to confirm whether a transaction is recent."
                );
            }
            _ => {}
        }

        Ok(signature_result?)
    }
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

    let payer_keypair_path = opts.keypair_path;
    let signer = &*get_signer(payer_keypair_path.unwrap())
        .ok_or_abort_with("Failed to load signer keypair.");

    let rpc_client =
        RpcClient::new_with_commitment(opts.cluster.unwrap(), CommitmentConfig::confirmed());
    let snapshot_client = SnapshotClient::new(rpc_client);

    let output_mode = opts.output_mode.unwrap();
    let mut config = Config {
        client: snapshot_client,
        signer,
        output_mode,
    };

    merge_with_config_and_environment(&mut opts.subcommand, config_file.as_ref());
    match opts.subcommand {
        SubCommand::CreateSolido(cmd_opts) => {
            let result = config.with_snapshot(|config| command_create_solido(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to create Solido instance.");
            print_output(output_mode, &output);
        }
        SubCommand::Multisig(cmd_opts) => multisig::main(&mut config, cmd_opts),
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
        SubCommand::CreateSolido(opts) => opts.merge_with_config_and_environment(config_file),
        SubCommand::AddValidator(opts) => opts.merge_with_config_and_environment(config_file),
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

// Get a boxed signer that lives long enough for us to use it in the Config.
fn get_signer(payer_keypair_path: PathBuf) -> Result<Box<dyn Signer>, Error> {
    use std::convert::TryFrom;
    use uriparse::uri_reference::URIReference;

    let boxed_signer: Box<dyn Signer> = if payer_keypair_path.starts_with("usb://") {
        // Parse the uri before we try to connect, so we can diagnose uri format issues early.
        let uri = payer_keypair_path
            .into_os_string()
            .into_string()
            .map_err(|_| {
                CliError::new("A keypair path that starts with usb:// must be valid UTF-8.")
            })?;

        let uri_invalid_msg =
            "Failed to parse usb:// keypair path. It must be of the form 'usb://ledger?key=0'.";

        let uri_ref = URIReference::try_from(&uri[..])
            .map_err(|err| CliError::with_cause(uri_invalid_msg, err))?;

        let derivation_path = DerivationPath::from_uri_key_query(&uri_ref)
            .map_err(|err| CliError::with_cause(uri_invalid_msg, err))?
            .ok_or_else(|| CliError::new(uri_invalid_msg))?;

        let locator = Locator::new_from_uri(&uri_ref)
            .map_err(|err| CliError::with_cause(uri_invalid_msg, err))?;

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
