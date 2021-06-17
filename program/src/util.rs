use serde::Serializer;
use solana_program::pubkey::Pubkey;

/// Function to use when serializing a public key, to print it using base58.
pub fn serialize_b58<S: Serializer>(x: &Pubkey, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&x.to_string())
}
