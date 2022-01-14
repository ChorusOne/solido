// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use solana_account_decoder::validator_info;
use solana_client::rpc_client::RpcClient;
use solana_config_program::ConfigKeys;
use solana_sdk::pubkey::Pubkey;

use crate::error::{Error, SerializationError};

type Result<T> = std::result::Result<T, Error>;

/// Validator metadata stored in a config account managed by the config program.
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
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
    account_data: &[u8],
) -> Result<(Pubkey, ValidatorInfo)> {
    let key_list: ConfigKeys = bincode::deserialize(account_data)?;

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

    // The validator identity pubkey lives at index 1.
    let (validator_identity, identity_signed_config) = key_list.keys[1];

    if !identity_signed_config {
        let err = SerializationError {
            context: "Config account is not signed by validator identity.".to_string(),
            cause: None,
            address: config_address,
        };
        return Err(Box::new(err));
    }

    // A config account stores a list of (pubkey, bool) pairs, followed by json
    // data. To figure out where the json data starts, we need to know the size
    // fo the key list. The json data is not stored directly, it is serialized
    // with bincode as a string.
    let key_list_len = bincode::serialized_size(&key_list)
        .expect("We deserialized it, therefore it must be serializable.")
        as usize;
    let json_data: String = bincode::deserialize(&account_data[key_list_len..])?;
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

    // Due to the structure of validator info (config accounts pointing to identity
    // accounts), it is possible for multiple config accounts to describe the same
    // validator. This is invalid, if it happens, we wouldn't know which config
    // account is the right one, so instead of making an arbitrary decision, we
    // ignore all validator infos for that identity.
    let mut bad_identities = HashSet::new();

    for (config_addr, account) in &all_config_accounts {
        if let Ok((validator_identity, _info)) =
            deserialize_validator_info(*config_addr, &account.data)
        {
            // Record the config address for this validator, but ignore the
            // other metadata. We will re-read this later in an snapshot, so
            // we can read it atomically together with other accounts.
            let old_config_addr = mapping.insert(validator_identity, *config_addr);
            if old_config_addr.is_some() {
                bad_identities.insert(validator_identity);
            }
        } else {
            // We ignore errors here: not all config accounts need to contain
            // validator info, so if we fail to deserialize the config account,
            // that is not fatal, it just means this is not an account that
            // we are interested in.
        }
    }

    for bad_identity in &bad_identities {
        mapping.remove(bad_identity);
    }

    Ok(mapping)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_deserialize_requires_signature() {
        // Here's a config account where the second address is not a signer.
        // We should fail to deserialize it.
        let invalid_config_account = b"\x02\x07Q\x97\x01tH\xf2\xac]\xc2<\x9e\xbcz\
        \xc7\x8c\n\'%z\xc6\x14E\x8d\xe0\xa4\xf1o\x80\x00\x00\x00\x00\x86\xadf\x8a\
        \x17\xd5\x90\x82a\x92=\xacV\xe8N\x88\xf0\xc2\x9a\xee\xea\xb0\xba\xc2\x19\
        \x83\x80\xecq\xc6\\\xeb\x00B\x00\x00\x00\x00\x00\x00\x00{\"name\":\"AAAAA\
        AAAAAAAAAAAA\",\"keybaseUsername\":\"aaaaaaaaaaaaaaaaa\"}\x00\x00\x00\x00\x00";

        let result = deserialize_validator_info(Pubkey::new_unique(), invalid_config_account);
        assert!(result.is_err());

        // Flip the "signed" byte to make the config account valid.
        let mut valid_config_account: Vec<u8> = invalid_config_account[..].into();
        valid_config_account[66] = 0x01;

        let result = deserialize_validator_info(Pubkey::new_unique(), &valid_config_account);
        assert_eq!(
            result.ok(),
            Some((
                Pubkey::from_str("A4izJ2gATP6n5P9wXuarbn871beydWZ6mGisfhv8KYd8").unwrap(),
                ValidatorInfo {
                    name: "AAAAAAAAAAAAAAAAA".to_string(),
                    keybase_username: Some("aaaaaaaaaaaaaaaaa".to_string()),
                },
            )),
        )
    }
}
