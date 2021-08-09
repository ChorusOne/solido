// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::Clap;
use serde::Deserialize;
use serde_json::Value;
use solana_sdk::pubkey::{ParsePubkeyError, Pubkey};

use lido::token::Lamports;
use lido::token::StLamports;

pub fn get_option_from_config<T: FromStr>(
    name: &'static str,
    config_file: Option<&ConfigFile>,
) -> Option<T> {
    let config_file = config_file?;
    let value = config_file.values.get(name)?;
    if let Value::String(str_value) = value {
        match T::from_str(str_value) {
            Err(_) => {
                eprintln!("Could not convert {} from string.", str_value);
                std::process::exit(1);
            }
            Ok(t) => Some(t),
        }
    } else {
        // TODO: Support numbers
        None
    }
}

pub fn get_option_from_env<T: FromStr>(str_key: &str) -> Option<T> {
    let env_var = (std::env::var(str_key)).ok()?;
    match T::from_str(&env_var) {
        Ok(t) => Some(t),
        Err(_) => {
            eprintln!(
                "Could not convert environment variable {}={} from string.",
                str_key, env_var
            );
            std::process::exit(1);
        }
    }
}
/// Generates a struct that derives `Clap` for usage with a config file.
///
/// This macro avoids code repetition by implementing a function that sweeps
/// every field from the struct and checks if it is set either by argument or by
/// a config file. If the field is set by neither, it will print all the
/// missing fields.
/// It will also implement getters that unwrap every field of the struct,
/// this is necessary to use optional arguments in clap, that will be filled
/// in case they were defined in the configuration file.

/// Optionally, a default value can be passed for the structure with `=> <default>`,
/// If the value is neither passed by argument or by config file, it will be set
/// as `default`.

/// The arguments expected are the same as the names defined in the macro
/// substituting all '_' with '-', in case of the config file, the names are the
/// same as defined in the struct.

/// Example:
/// ```
/// cli_opt_struct! {
///     FooOpts {
///         #[clap(long, value_name = "address")]
///         foo_arg: Pubkey,
///         def_arg: i32 => 3 // argument is 3 by default
///     }
/// }
/// ```
/// This generates the struct:
/// ```
/// struct FooOpts {
///     #[clap(long, value_name = "address")]
///     foo_arg: Option<Pubkey>,
///     def_arg: Option<i32>, // If not present in config, the value will be 3 by default.
/// }
///
/// impl FooOpts {
///     pub fn merge_with_config_and_environment(&mut self, config_file: &Option<ConfigFile>);
/// }
/// ```
/// When `merge_with_config(config_file)` is called, it will set the fields of
/// the config file respecting the order:
///     1. Set by passing `--foo-arg <arg>`.
///     2. Search the `config_file` for a key where all symbols "-" are
///        substituted with "_" from the argument before, e.g., "foo_arg".
///     3. Search for an environmental variable where all the letters of the
///        argument before are capitalized, e.g., "FOO_ARG".
/// search `config_file` for the key 'foo_arg' and sets the field accordingly.
/// The type must implement the `FromStr` trait.
/// In the example, `def_arg` will have value 3 if not present in the config file.

macro_rules! cli_opt_struct {
    {
        // Struct name
        $name:ident {
            $(
                // Forward the attributes, such as doc comments or Clap options.
                $(#[$attr:meta])*
                // Field name and type, specify default value
                $field:ident : $type:ty $(=> $default:expr)?
            ),*
            $(,)?
        }
    } => {
        #[derive(Debug, Clap, Default)]
        pub struct $name {
            $(
                $(#[$attr])*
                $field: Option<$type>,
            )*
        }

        impl $name {
            /// Merges the struct with a config file.
            /// Fails if a field is not present (None) in the struct *and* not
            /// present in the config file. When failing, prints all the missing
            /// fields.
            #[allow(dead_code)]
            pub fn merge_with_config_and_environment(&mut self, config_file: Option<&ConfigFile>) {
                let mut failed = false;
                $(
                    let from_cli = self.$field.take();
                    let str_field = stringify!($field);
                    let from_config = get_option_from_config(str_field, config_file);

                    #[allow(unused_mut, unused_assignments)]
                    let mut default = None;
                    $(default = Some($default);)?
                    let env_var_name = format!("SOLIDO_{}", str_field.to_ascii_uppercase());
                    let env_option = get_option_from_env(&env_var_name);
                    // Sets the field with the argument or the config field.
                    self.$field = from_cli.or(from_config).or(env_option).or(default);
                    if self.$field.is_none() {
                        failed = true;
                        eprintln!("Expected --{} to be provided on the command line, set in config file with key \"{}\", or specified in an environment variable with key \"{}\".",
                        str_field.replace("_", "-"), str_field, env_var_name);
                    }
                )*
                if failed {
                    std::process::exit(1);
                }
            }

            $(
                // Implement a getter for every field in the struct
                #[allow(dead_code)]
                pub fn $field(&self) -> &$type {
                    self.$field.as_ref().unwrap()
                }
            )*
        }
    }
}

/// Type to represent a vector of `Pubkey`.
// TODO(#218) Accept an array in the json config file.
#[derive(Debug, Clone)]
pub struct PubkeyVec(pub Vec<Pubkey>);
/// Constructs a `PubkeyVec` from a string by splitting the string by ',' and
/// constructing a Pubkey for each of the tokens
impl FromStr for PubkeyVec {
    type Err = ParsePubkeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let pubkeys = s
            .split(',')
            .map(|key| Pubkey::from_str(key))
            .collect::<Result<Vec<Pubkey>, Self::Err>>()?;
        Ok(PubkeyVec(pubkeys))
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub values: Value,
}

pub fn read_config(config_path: &Path) -> ConfigFile {
    let file_content = std::fs::read(config_path).expect("Failed to open config file.");
    let values: Value = serde_json::from_slice(&file_content).expect("Error while reading config.");
    ConfigFile { values }
}

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

/// Resolve ~/.config/solana/id.json.
fn get_default_keypair_path() -> PathBuf {
    let home = std::env::var("HOME").expect("Expected $HOME to be set.");
    let mut path = PathBuf::from(home);
    path.push(".config/solana/id.json");
    path
}

cli_opt_struct! {
    GeneralOpts {
        /// The keypair to sign and pay with. [default: ~/.config/solana/id.json]
        #[clap(long)]
        keypair_path: PathBuf => get_default_keypair_path(),

        /// URL of cluster to connect to (e.g., https://api.devnet.solana.com for solana devnet)
        #[clap(long)]
        cluster: String => "http://127.0.0.1:8899".to_owned(),

        /// Whether to output text or json.
        #[clap(long = "output", possible_values = &["text", "json"])]
        output_mode: OutputMode => OutputMode::Text,

        /// Optional config path
        #[clap(long)]
        config: PathBuf => PathBuf::default(),
    }
}

cli_opt_struct! {
    CreateSolidoOpts {
        /// Address of the Solido program
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,

        /// The maximum number of validators that this Solido instance will support.
        #[clap(long, value_name = "int")]
        max_validators: u32,

        /// The maximum number of maintainers that this Solido instance will support.
        #[clap(long, value_name = "int")]
        max_maintainers: u32,

        // See also the docs section of `create-solido` in main.rs for a description
        // of the fee shares.
        /// Treasury fee share of the rewards.
        #[clap(long, value_name = "int")]
        treasury_fee_share: u32,

        /// Validation fee share of the rewards.
        #[clap(long, value_name = "int")]
        validation_fee_share: u32,

        /// Developer fee share of the rewards.
        #[clap(long, value_name = "int")]
        developer_fee_share: u32,

        /// Share of the rewards that goes to stSOL appreciation (the non-fee part).
        #[clap(long, value_name = "int")]
        st_sol_appreciation_share: u32,

        /// Account who will own the stSOL SPL token account that receives treasury fees.
        #[clap(long, value_name = "address")]
        treasury_account_owner: Pubkey,

        /// Account who will own the stSOL SPL token account that receives the developer fees.
        #[clap(long, value_name = "address")]
        developer_account_owner: Pubkey,

        /// Optional argument for the mint address, if not passed a random one
        /// will be created.
        #[clap(long)]
        mint_address: Pubkey => Pubkey::default(),

        /// Optional argument for the solido address, if not passed a random one
        /// will be created.
        #[clap(long)]
        solido_key_path: PathBuf => PathBuf::default(),

        /// Used to compute Solido's manager. Multisig instance.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
    DepositOpts {
        /// Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,

        /// Account that stores the data for this Solido instance.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,

        /// Amount to deposit, in SOL, using . as decimal separator.
        #[clap(long, value_name = "sol")]
        amount_sol: Lamports,
    }
}

cli_opt_struct! {
    WithdrawOpts {
         /// Address of the Solido program.
         #[clap(long, value_name = "address")]
         solido_program_id: Pubkey,

         /// Account that stores the data for this Solido instance.
         #[clap(long, value_name = "address")]
         solido_address: Pubkey,

         /// Amount to withdraw in stSOL, using . as decimal separator.
         #[clap(long, value_name = "st_sol")]
         amount_st_sol: StLamports,
    }
}

cli_opt_struct! {
    AddValidatorOpts {
        /// Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
        /// Account that stores the data for this Solido instance.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,

        /// Address of the validator vote account.
        #[clap(long, value_name = "address")]
        validator_vote_account: Pubkey,

        /// Validator stSol token account.
        #[clap(long, value_name = "address")]
        validator_fee_account: Pubkey,

        /// Multisig instance.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
    AddRemoveMaintainerOpts {
        /// Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
        /// Account that stores the data for this Solido instance.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,

        /// Maintainer to add or remove.
        #[clap(long, value_name = "address")]
        maintainer_address: Pubkey,

        /// Multisig instance.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
     ShowSolidoOpts {
        /// The solido instance to show.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,
        /// Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
    }
}

cli_opt_struct! {
    ShowSolidoAuthoritiesOpts {
        /// The solido instance to show authorities.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,
        /// Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
   }
}

cli_opt_struct! {
    PerformMaintenanceOpts {
        /// Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,

        /// Account that stores the data for this Solido instance.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,

        /// Vote account that the maintainer tries to claim validator's fees
        /// from. If none is set, the validator will ignore the instruction to
        /// claim fees.
        #[clap(long)]
        validator_vote_account : Pubkey => Pubkey::default(),
    }
}

// Multisig opts

cli_opt_struct! {
    CreateMultisigOpts {
        /// How many signatures are needed to approve a transaction.
        #[clap(long)]
        threshold: u64,

        /// The public keys of the multisig owners, who can sign transactions.
        /// Addresses are given separated by comma.
        #[clap(long)]
        owners: PubkeyVec,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

impl CreateMultisigOpts {
    /// Perform a few basic checks to rule out nonsensical multisig settings.
    ///
    /// Exits if validation fails.
    pub fn validate_or_exit(&self) {
        if *self.threshold() > self.owners().0.len() as u64 {
            println!("Threshold must be at most the number of owners.");
            std::process::exit(1);
        }
        if *self.threshold() == 0 {
            println!("Threshold must be at least 1.");
            std::process::exit(1);
        }
    }
}

cli_opt_struct! {
ProposeUpgradeOpts {
        /// The multisig account whose owners should vote for this proposal.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        /// The program id of the program to upgrade.
        #[clap(long, value_name = "address")]
        program_address: Pubkey,

        /// The address that holds the new program data.
        #[clap(long, value_name = "address")]
        buffer_address: Pubkey,

        /// Account that will receive leftover funds from the buffer account.
        #[clap(long, value_name = "address")]
        spill_address: Pubkey,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
ShowMultisigOpts {
        /// The multisig account to display.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
    ShowTransactionOpts {
        /// The transaction to display.
        #[clap(long, value_name = "address")]
        transaction_address: Pubkey,

        /// The transaction to display.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
    ApproveOpts {
        // TODO: Can be omitted, we can obtain it from the transaction account.
        /// The multisig account whose owners should vote for this proposal.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        /// The transaction to approve.
        #[clap(long, value_name = "address")]
        transaction_address: Pubkey,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
    ExecuteTransactionOpts {
        // TODO: Can be omitted, we can obtain it from the transaction account.
        /// The multisig account whose owners approved this transaction.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        /// The transaction to execute.
        #[clap(long, value_name = "address")]
        transaction_address: Pubkey,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
    ProposeChangeMultisigOpts {
        /// The multisig account to modify.
        #[clap(long)]
        multisig_address: Pubkey,

        // The fields below are the same as for `CreateMultisigOpts`, but we can't
        // just embed a `CreateMultisigOpts`, because Clap does not support that.
        /// How many signatures are needed to approve a transaction.
        #[clap(long)]
        threshold: u64,

        /// The public keys of the multisig owners, who can sign transactions.
        #[clap(long)]
        owners: PubkeyVec,

        /// Address of the Multisig program.
        #[clap(long)]
        multisig_program_id: Pubkey,
    }
}

cli_opt_struct! {
    RunMaintainerOpts {
        /// Address of the Solido program.
        #[clap(long)]
        solido_program_id: Pubkey,

        /// Account that stores the data for this Solido instance.
        #[clap(long)]
        solido_address: Pubkey,

        /// Listen address and port for the http server that serves a /metrics endpoint. Defaults to 0.0.0.0:8923.
        listen: String => "0.0.0.0:8923".to_owned(),

        // The expected wait time is half the max poll interval. A max poll interval
        // of a few minutes should be plenty fast for a production deployment, but
        // for testing you can reduce this value to make the daemon more responsive,
        // to eliminate some waiting time.
        /// Maximum time to wait in seconds after there was no maintenance to perform, before checking again. Defaults to 120s
        #[clap(long)]
        max_poll_interval_seconds: u64 => 120,

        /// Vote account that the maintainer tries to claim validator's fees
        /// from. If none is set, the validator will ignore the instruction to
        /// claim fees.
        #[clap(long)]
        validator_vote_account : Pubkey => Pubkey::default(),
    }
}

impl From<&ProposeChangeMultisigOpts> for CreateMultisigOpts {
    fn from(opts: &ProposeChangeMultisigOpts) -> CreateMultisigOpts {
        CreateMultisigOpts {
            threshold: opts.threshold,
            owners: opts.owners.clone(),
            multisig_program_id: opts.multisig_program_id,
        }
    }
}
