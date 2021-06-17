use clap::Clap;
use serde::Deserialize;
use serde_json::Value;
use solana_sdk::pubkey::{ParsePubkeyError, Pubkey};
use std::{fs::File, str::FromStr};

pub fn get_option_from_config<T: FromStr>(
    name: &'static str,
    config_file: &Option<ConfigFile>,
) -> Option<T> {
    // let config_file = config_file?;
    match config_file {
        Some(config_file) => {
            let value = config_file.values.get(name)?;
            if let Value::String(str_value) = value {
                match T::from_str(str_value) {
                    Err(_) => {
                        eprintln!("Could not convert {} from string", str_value);
                        None
                    }
                    Ok(pubkey) => Some(pubkey),
                }
            } else {
                // TODO: Support numbers
                None
            }
        }
        None => None,
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
///         Address of the Solido program.
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
///     pub fn merge_with_config(&mut self, config_file: &Option<ConfigFile>);
/// }
/// ```
/// When `merge_with_config(config_file)` is called, if the `foo` field has a
/// value (set by passing `--foo-arg <pubkey>`) it does nothing, otherwise,
/// search `config_file` for the key 'foo_arg' and sets the field accordingly.
/// The type must implement the `FromStr` trait.
/// In the example, `def_arg` will have value 3 if not present in the config file.

macro_rules! cli_opt_struct {
    {
        // Struct name
        $name:ident {
            $(
                // Foward the properties
                $(#$properties:tt)?
                // Field name and type, specify default value
                $field:ident : $type:ty $(=> $default:expr)?
            ),*
            $(,)?
        }
    } => {
        #[derive(Debug, Clap)]
        pub struct $name {
            $(
                $(#$properties)?
                $field: Option<$type>,
            )*
        }

        impl $name {
            /// Merges the struct with a config file.
            /// Fails if a field is not present (None) in the struct *and* not
            /// present in the config file. When failing, prints all the missing
            /// fields.
            #[allow(dead_code)]
            pub fn merge_with_config(&mut self, config_file: &Option<ConfigFile>) {
                let mut failed = false;
                $(
                    let str_field = stringify!($field);
                    // Sets the field with the argument or the config field.
                    self.$field = self.$field.take().or(get_option_from_config(str_field, config_file));
                    #[allow(unused_mut, unused_assignments)]
                    let mut is_optional = false;
                    $(
                        // If a default value was passed and the field is None, sets the default value.
                        self.$field = self.$field.take().or(Some($default));
                        is_optional = true;
                    )?
                    // If field is still none, prints an error, this will fail in the end of the function.
                    // If the value has a default, let it be None
                    if !is_optional && self.$field.is_none() {
                        failed = true;
                        eprintln!("Expected --{} to be provided on arguments, or set in config file with key {}.", str_field.replace("_", "-"), str_field);
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

/// Type to represent a vector of `Pubkey`
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

pub fn read_config(config_path: String) -> ConfigFile {
    let file = File::open(config_path).expect("Failed to open file.");
    let values: Value = serde_json::from_reader(file).expect("Error while reading config.");
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

cli_opt_struct! {
    CreateSolidoOpts {
        // Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,

        // Numerator of the fee fraction.
        #[clap(long, value_name = "int")]
        fee_numerator: u64,

        // Denominator of the fee fraction.
        #[clap(long, value_name = "int")]
        fee_denominator: u64,

        // The maximum number of validators that this Solido instance will support.
        #[clap(long, value_name = "int")]
        max_validators: u32,

        // The maximum number of maintainers that this Solido instance will support.
        #[clap(long, value_name = "int")]
        max_maintainers: u32,

        // Fees are divided proportionally to the sum of all specified fees, for instance,
        // if all the fees are the same value, they will be divided equally.
        // Treasury fee share
        #[clap(long, value_name = "int")]
        treasury_fee: u32,
        // Validation fee share, to be divided equally among validators
        #[clap(long, value_name = "int")]
        validation_fee: u32,
        // Developer fee share
        #[clap(long, value_name = "int")]
        developer_fee: u32,

        // Account who will own the stSOL SPL token account that receives treasury fees.
        #[clap(long, value_name = "address")]
        treasury_account_owner: Pubkey,
        // Account who will own the stSOL SPL token account that receives the developer fees.
        #[clap(long, value_name = "address")]
        developer_account_owner: Pubkey,

        // Used to compute Solido's manager.
        // Multisig instance.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,
    }
}

cli_opt_struct! {
    AddValidatorOpts {
        // Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
        // Account that stores the data for this Solido instance.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,
        // Address of the validator vote account.
        #[clap(long, value_name = "address")]
        validator_vote_account: Pubkey,
        // Validator stSol token account.
        #[clap(long, value_name = "address")]
        validator_fee_account: Pubkey,
        // Multisig instance.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,
    }
}

cli_opt_struct! {
    AddRemoveMaintainerOpts {
        // Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
        // Account that stores the data for this Solido instance.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,

        // Maintainer to add or remove.
        #[clap(long, value_name = "address")]
        maintainer_address: Pubkey,

        // Multisig instance.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,
    }
}

cli_opt_struct! {
    CreateValidatorStakeAccountOpts {
        // Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
        // Account that stores the data for this Solido instance.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,

        // Address of the validator vote account.
        #[clap(long, value_name = "address")]
        validator_vote: Pubkey,

        // Multisig instance.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,
    }
}

cli_opt_struct! {
     ShowSolidoOpts {
        // The solido instance to show
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
    }
}

cli_opt_struct! {
    PerformMaintenanceOpts {
        // Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,

        // Account that stores the data for this Solido instance.
        #[clap(long, value_name = "address")]
        solido_address: Pubkey,
    }
}

// Multisig opts

cli_opt_struct! {
    CreateMultisigOpts {
        // How many signatures are needed to approve a transaction.
        #[clap(long)]
        threshold: u64,

        // The public keys of the multisig owners, who can sign transactions.
        #[clap(long = "owner")]
        owners: PubkeyVec,
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
        // The multisig account whose owners should vote for this proposal.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        // The program id of the program to upgrade.
        #[clap(long, value_name = "address")]
        program_address: Pubkey,

        // The address that holds the new program data.
        #[clap(long, value_name = "address")]
        buffer_address: Pubkey,

        // Account that will receive leftover funds from the buffer account.
        #[clap(long, value_name = "address")]
        spill_address: Pubkey,
    }
}

cli_opt_struct! {
ShowMultisigOpts {
        // The multisig account to display.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,
    }
}

cli_opt_struct! {
    ShowTransactionOpts {
        // The transaction to display.
        #[clap(long, value_name = "address")]
        transaction_address: Pubkey,

        // The transaction to display.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,
    }
}

cli_opt_struct! {
    ApproveOpts {
        // The multisig account whose owners should vote for this proposal.
        // TODO: Can be omitted, we can obtain it from the transaction account.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        // The transaction to approve.
        #[clap(long, value_name = "address")]
        transaction_address: Pubkey,
    }
}

cli_opt_struct! {
    ExecuteTransactionOpts {
        // The multisig account whose owners approved this transaction.
        // TODO: Can be omitted, we can obtain it from the transaction account.
        #[clap(long, value_name = "address")]
        multisig_address: Pubkey,

        // The transaction to execute.
        #[clap(long, value_name = "address")]
        transaction_address: Pubkey,
    }
}

cli_opt_struct! {
    ProposeChangeMultisigOpts {
        // The multisig account to modify.
        #[clap(long)]
        multisig_address: Pubkey,

        // The fields below are the same as for `CreateMultisigOpts`, but we can't
        // just embed a `CreateMultisigOpts`, because Clap does not support that.
        // How many signatures are needed to approve a transaction.
        #[clap(long)]
        threshold: u64,

        // The public keys of the multisig owners, who can sign transactions.
        #[clap(long = "owner")]
        owners: PubkeyVec,
    }
}

cli_opt_struct! {
    RunMaintainerOpts {
        // Address of the Solido program.
        #[clap(long)]
        solido_program_id: Pubkey,

        // Account that stores the data for this Solido instance.
        #[clap(long)]
         solido_address: Pubkey,

        // Listen address and port for the http server that serves a /metrics endpoint.
        #[clap(long)]
        listen: String => "0.0.0.0:8923".to_owned(),

        // Maximum time to wait after there was no maintenance to perform, before checking again.
        //
        // The expected wait time is half the max poll interval. A max poll interval
        // of a few minutes should be plenty fast for a production deployment, but
        // for testing you can reduce this value to make the daemon more responsive,
        // to eliminate some waiting time.
        #[clap(long)]
        max_poll_interval_seconds: u64 => 120,
    }
}

impl From<&ProposeChangeMultisigOpts> for CreateMultisigOpts {
    fn from(opts: &ProposeChangeMultisigOpts) -> CreateMultisigOpts {
        CreateMultisigOpts {
            threshold: opts.threshold,
            owners: opts.owners.clone(),
        }
    }
}
