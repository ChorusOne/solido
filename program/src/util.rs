// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use solana_program::pubkey::Pubkey;

/// Function to use when serializing a public key, to print it using base58.
pub fn serialize_b58<S: Serializer>(x: &Pubkey, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&x.to_string())
}

/// Serializer that serializes a list of pubkeys as an array of base58 strings.
pub fn serialize_b58_slice<T: AsRef<[Pubkey]>, S: Serializer>(
    pubkeys: T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let pubkeys_slice = pubkeys.as_ref();
    let mut seq = serializer.serialize_seq(Some(pubkeys_slice.len()))?;
    for pubkey in pubkeys_slice {
        seq.serialize_element(&PubkeyBase58(*pubkey))?;
    }
    seq.end()
}

/// Helper for a serializer that serializes a `&[Pubkey]` as a list of strings.
///
/// Because Serde is built around the `Serialize` trait, we need a struct that
/// implements it to be able to call `serialize_element`.
struct PubkeyBase58(Pubkey);

impl Serialize for PubkeyBase58 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Defer to the Display impl, which formats as base58.
        serializer.collect_str(&self.0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::Serialize;
    use serde_json;
    use std::str::FromStr;

    #[test]
    fn test_serialize_b58() {
        #[derive(Serialize)]
        struct Test {
            #[serde(serialize_with = "serialize_b58")]
            pubkey: Pubkey,
        }

        let x = Test {
            pubkey: Pubkey::from_str("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc").unwrap(),
        };
        let json = serde_json::to_string(&x);
        assert_eq!(
            json.unwrap(),
            r#"{"pubkey":"3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc"}"#,
        )
    }

    #[test]
    fn test_serialize_b58_slice() {
        #[derive(Serialize)]
        struct Test {
            #[serde(serialize_with = "serialize_b58_slice")]
            pubkeys: Vec<Pubkey>,
        }

        let x = Test {
            pubkeys: vec![
                Pubkey::from_str("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc").unwrap(),
                Pubkey::from_str("7dwKwaz6gmNCQVjTGSrvZEuouJBuC7tzGw4voDa3iTrk").unwrap(),
            ],
        };
        let json = serde_json::to_string_pretty(&x);
        assert_eq!(
            json.unwrap(),
            r#"{
  "pubkeys": [
    "3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc",
    "7dwKwaz6gmNCQVjTGSrvZEuouJBuC7tzGw4voDa3iTrk"
  ]
}"#,
        )
    }
}
