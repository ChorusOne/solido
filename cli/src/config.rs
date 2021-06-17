use clap::Clap;
use serde::Deserialize;
use serde_json::Value;
use solana_sdk::pubkey::{ParsePubkeyError, Pubkey};
use std::{fs::File, str::FromStr};

fn get_option_from_config<T: FromStr>(
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

macro_rules! cli_opt_struct {
    {
        $name:ident {
            $(
                $(#$properties:tt)?
                $field:ident : $type:ty
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
            pub fn merge_with_config(&mut self, config_file: &Option<ConfigFile>) {
                let mut failed = false;
                $(
                    let str_field = stringify!($field);
                    self.$field = self.$field.take().or(get_option_from_config(str_field, config_file));
                    if self.$field.is_none() {
                        failed = true;
                        eprintln!("Expected --{} to be provided on arguments, or set in config file with key {}.", str_field.replace("_", "-"), str_field);
                    }
                )*
                if failed {
                    std::process::exit(1);
                }
            }

            $(
                #[allow(dead_code)]
                pub fn $field(&self) -> &$type {
                    self.$field.as_ref().unwrap()
                }
            )*
        }
    }
}

#[derive(Debug, Clone)]
pub struct PubkeyVec(pub Vec<Pubkey>);
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

impl ConfigFile {
    pub fn get_pubkey(
        config_file: &Option<ConfigFile>,
        arg_opt: Option<Pubkey>,
        option_name: &str,
        option_arg: &str,
    ) -> Pubkey {
        match (config_file, arg_opt) {
            (None, None) => {
                None
            }
            (None, Some(pubkey)) => Some(pubkey),
            (Some(config_file), None) => match config_file.values.get(option_name) {
                None => None,
                Some(value) => {
                    if let Value::String(pubkey) = value {
                        Some(
                            Pubkey::from_str(pubkey)
                                .expect("Failed to parse public key from file."),
                        )
                    } else {
                        None
                    }
                }
            },
            (Some(_), Some(pubkey)) => {
                eprintln!("Argument {} will override config file", option_arg);
                Some(pubkey)
            },
        }.unwrap_or_else(|| panic!("'{}' was not specified. Either pass '{}', or add a line '{} = \"...\" to the config file", option_name, option_arg, option_name))
    }
}

pub fn read_config(config_path: String) -> ConfigFile {
    let file = File::open(config_path).expect("Failed to open file.");
    let values: Value = serde_json::from_reader(file).expect("Error while reading config.");
    ConfigFile { values }
}

cli_opt_struct! {
    CreateSolidoOpts {
        // Address of the Solido program.
        #[clap(long, value_name = "address")]
        solido_program_id: Pubkey,

        // Address of the SPL stake pool program.
        #[clap(long, value_name = "address")]
        stake_pool_program_id: Pubkey,

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
        // Stake pool program id.
        #[clap(long, value_name = "address")]
        stake_pool_program_id: Pubkey,
        // Address of the validator vote account.
        #[clap(long, value_name = "address")]
        validator_vote: Pubkey,
        // Validator stSol token account.
        #[clap(long, value_name = "address")]
        validator_rewards_address: Pubkey,
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

        // Stake pool program id
        #[clap(long, value_name = "address")]
        stake_pool_program_id: Pubkey,
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

        // Stake pool program id
        #[clap(long, value_name = "address")]
        stake_pool_program_id: Pubkey,
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

impl From<&ProposeChangeMultisigOpts> for CreateMultisigOpts {
    fn from(opts: &ProposeChangeMultisigOpts) -> CreateMultisigOpts {
        CreateMultisigOpts {
            threshold: opts.threshold,
            owners: opts.owners.clone(),
        }
    }
}
