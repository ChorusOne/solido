use std::{fmt, fs::File};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use solana_program::pubkey::Pubkey;
use std::str::FromStr;

/// Wrapper for `Pubkey` to serialize it as base58 in json, instead of a list of numbers.
pub struct PubkeyBase58(pub Pubkey);

impl fmt::Display for PubkeyBase58 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for PubkeyBase58 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Defer to the Display impl, which formats as base58.
        serializer.collect_str(&self.0)
    }
}

impl From<Pubkey> for PubkeyBase58 {
    fn from(pk: Pubkey) -> PubkeyBase58 {
        PubkeyBase58(pk)
    }
}

impl From<&Pubkey> for PubkeyBase58 {
    fn from(pk: &Pubkey) -> PubkeyBase58 {
        PubkeyBase58(*pk)
    }
}

fn deserialize_b58<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    let pk = Pubkey::from_str(&buf).map_err(serde::de::Error::custom)?;
    Ok(pk)
}

#[derive(Debug, Deserialize)]
pub struct CommonOpts {
    /// Address of the Multisig program.
    #[serde(deserialize_with = "deserialize_b58")]
    pub multisig_program_id: Pubkey,
    /// The solido instance to show
    #[serde(deserialize_with = "deserialize_b58")]
    pub solido_address: Pubkey,
    #[serde(deserialize_with = "deserialize_b58")]
    pub solido_program_id: Pubkey,
}

pub fn read_create_solido_config(config_path: String) -> Result<CommonOpts, crate::Error> {
    let file = File::open(config_path)?;
    let common_opts: CommonOpts = serde_json::from_reader(file)?;
    println!("CONFIG: {:?}", common_opts);
    Ok(common_opts)
}
