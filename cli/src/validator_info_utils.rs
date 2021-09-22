// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::collections::HashMap;

use bincode;
use serde;
use serde::{Deserialize, Serialize};
use solana_account_decoder::validator_info;
use solana_client::rpc_client::RpcClient;
use solana_config_program::ConfigKeys;
use solana_sdk::account::Account;
use solana_sdk::pubkey::Pubkey;

use crate::error::{Error, SerializationError};

type Result<T> = std::result::Result<T, Error>;

/// Validator metadata stored in a config account managed by the config program.
#[derive(Debug, Deserialize, Serialize)]
pub struct ValidatorInfo {
    pub name: String,

    // Rename the field because this is how Solana stores it, it needs to have
    // this name to be able to deserialize.
    #[serde(rename = "keybaseUsername")]
    pub keybase_username: Option<String>,
    // Other keys that can be present in the json object are "details" and
    // "website", but we have no need for those at this point.
}

/// Deserialize a config account that contains validator info.
///
/// Returns the validator identity account address, and the validator info for
/// that validator. The config address is not related to the validator identity
/// address or any other validator property; to find the config account for a
/// given validator, we have to deserialize all config accounts that exist, and
/// filter for the one that belongs to a given identity account. See
/// [`get_validator_info_accounts`] for that.
pub fn deserialize_validator_info(
    config_address: Pubkey,
    account: &Account,
) -> Result<(Pubkey, ValidatorInfo)> {
    let key_list: ConfigKeys = bincode::deserialize(&account.data)?;

    // I don't know the meaning of the boolean here, but this is what `solana validator-info get`
    // uses to check if a config account contains validator info.
    if !key_list.keys.contains(&(validator_info::id(), false)) {
        let err = SerializationError {
            context: "Config account is not a validator info account.".to_string(),
            cause: None,
            address: config_address,
        };
        return Err(Box::new(err));
    }

    // The validator identity pubkey lives at index 1. The meaning of the
    // boolean is unclear.
    let (validator_identity, _) = key_list.keys[1];

    // A config account stores a list of (pubkey, bool) pairs, followed by json
    // data. To figure out where the json data starts, we need to know the size
    // fo the key list. The json data is not stored directly, it is serialized
    // with bincode as a string.
    let key_list_len = bincode::serialized_size(&key_list)
        .expect("We deserialized it, therefore it must be serializable.")
        as usize;
    let json_data: String = bincode::deserialize(&account.data[key_list_len..])?;
    let validator_info: ValidatorInfo = serde_json::from_str(&json_data)?;

    Ok((validator_identity, validator_info))
}

/// Return a map from validator identity account to config account.
///
/// To get the validator info (the validator metadata, such as name and Keybase
/// username), we have to extract that from the config account that stores the
/// validator info for a particular validator. But there is no way a priori to
/// know the address of the config account for a given validator; the only way
/// is to enumerate all config accounts and then find the one you are looking
/// for. This function builds a map from identity account to config account, so
/// we only have to enumerate once.
pub fn get_validator_info_accounts(rpc_client: &mut RpcClient) -> Result<HashMap<Pubkey, Pubkey>> {
    use solana_sdk::config::program as config_program;

    let all_config_accounts = rpc_client.get_program_accounts(&config_program::id())?;
    let mut mapping = HashMap::new();

    for (config_addr, account) in &all_config_accounts {
        match deserialize_validator_info(*config_addr, account) {
            Ok((validator_identity, _info)) => {
                // Record the config address for this validator, but ignore the
                // other metadata. We will re-read this later in an snapshot, so
                // we can read it atomically together with other accounts.
                mapping.insert(validator_identity, *config_addr);
            }
            Err(_) => {
                // We ignore errors here: not all config accounts need to contain
                // validator info, so if we fail to deserialize the config account,
                // that is not fatal, it just means this is not an account that
                // we are interested in.
            }
        }
    }

    Ok(mapping)
}
