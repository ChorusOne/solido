use std::fmt;

use solana_program::pubkey::Pubkey;
use serde::{Serialize, Serializer};

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
        PubkeyBase58(pk.clone())
    }
}

