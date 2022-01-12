use anker::wormhole::TerraAddress;
use serde::Serializer;

/// Serde serializer that serializes a Terra address as bech32 string, for use in json.
pub fn serialize_bech32<S: Serializer>(x: &TerraAddress, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&x.to_string())
}
