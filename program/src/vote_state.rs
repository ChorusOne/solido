use crate::error::LidoError;
use byteorder::{LittleEndian, ReadBytesExt};
use solana_program::{msg, pubkey::Pubkey};

/// Structure used to read the first 3 fields of a Solana `VoteAccount`.
/// The original `VoteAccount` structure cannot be used in a Solana
/// program due to size constrains.
#[derive(Debug, PartialEq)]
pub struct PartialVoteState {
    /// comes from an enum inside the `VoteState` structure
    /// We only accept if this field is set to 1
    pub version: u32,
    /// the node that votes in this account
    pub node_pubkey: Pubkey,

    /// the signer for withdrawals
    pub authorized_withdrawer: Pubkey,
    /// percentage (0-100) that represents what part of a rewards
    ///  payout should be given to this VoteAccount
    pub commission: u8,
}

impl PartialVoteState {
    /// Deserialize and test if a Vote Account is a Solido valid account.
    /// Solido vote accounts must have a 100% fee and have the withdraw
    /// authority set to the Solido program specified as `program_id`.
    pub fn deserialize(program_id: &Pubkey, data: &[u8]) -> Result<Self, LidoError> {
        if data.len() <= 69 {
            return Err(LidoError::InvalidVoteAccount);
        }
        let version = (&data[0..4])
            .read_u32::<LittleEndian>()
            .map_err(|_| LidoError::InvalidVoteAccount)?;
        if version != 1 {
            msg!(
                "Vote State account version should be 1, it's {} instead.",
                version
            );
            return Err(LidoError::InvalidVoteAccount);
        }
        let mut pubkey_buf: [u8; 32] = Default::default();
        pubkey_buf.copy_from_slice(&data[4..36]);
        let node_pubkey = Pubkey::new_from_array(pubkey_buf);
        pubkey_buf.copy_from_slice(&data[36..68]);
        let authorized_withdrawer = Pubkey::new_from_array(pubkey_buf);
        if &authorized_withdrawer != program_id {
            msg!(
                "Vote Account's withdrawer should be {}, is {} instead.",
                program_id,
                authorized_withdrawer
            );
            return Err(LidoError::InvalidVoteAccount);
        }
        let commission = data[68];
        if commission != 100 {
            msg!(
                "Vote Account's commission should be 100, is {} instead",
                commission
            );
            return Err(LidoError::InvalidVoteAccount);
        }
        Ok(PartialVoteState {
            version,
            node_pubkey,
            authorized_withdrawer,
            commission,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_deserialize() {
        // excerpt from actual vote account
        let data = [
            1, 0, 0, 0, 136, 99, 107, 146, 135, 142, 122, 24, 197, 142, 180, 124, 58, 189, 20, 89,
            26, 0, 48, 141, 5, 176, 45, 58, 21, 245, 13, 42, 159, 41, 182, 16, 40, 202, 229, 244,
            110, 43, 210, 6, 237, 212, 51, 217, 178, 201, 8, 236, 142, 194, 236, 247, 58, 237, 60,
            218, 112, 49, 21, 0, 122, 105, 36, 135, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 136, 99, 107, 146, 135, 142, 122, 24, 197, 142, 180,
            124, 58, 189, 20, 89, 26, 0, 48, 141, 5, 176, 45, 58, 21, 245, 13, 42, 159, 41, 182,
            16, 0, 0, 0, 0, 0,
        ];

        let program_id = Pubkey::from_str("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc").unwrap();
        let partial_vote = PartialVoteState::deserialize(&program_id, &data).unwrap();
        let expected_partial_result = PartialVoteState {
            version: 1,
            node_pubkey: Pubkey::from_str("ABQNjFWnmifA9qjRRtMjN2Ftzbma3gbSVY6sNPNFYtmM").unwrap(),
            authorized_withdrawer: program_id,
            commission: 100,
        };
        assert_eq!(expected_partial_result, partial_vote);
    }

    #[test]
    fn test_less_commission() {
        // excerpt from actual vote account
        let data = [
            1, 0, 0, 0, 136, 99, 107, 146, 135, 142, 122, 24, 197, 142, 180, 124, 58, 189, 20, 89,
            26, 0, 48, 141, 5, 176, 45, 58, 21, 245, 13, 42, 159, 41, 182, 16, 40, 202, 229, 244,
            110, 43, 210, 6, 237, 212, 51, 217, 178, 201, 8, 236, 142, 194, 236, 247, 58, 237, 60,
            218, 112, 49, 21, 0, 122, 105, 36, 135, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 136, 99, 107, 146, 135, 142, 122, 24, 197, 142, 180,
            124, 58, 189, 20, 89, 26, 0, 48, 141, 5, 176, 45, 58, 21, 245, 13, 42, 159, 41, 182,
            16, 0, 0, 0, 0, 0,
        ];

        let program_id = Pubkey::from_str("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc").unwrap();
        assert!(PartialVoteState::deserialize(&program_id, &data).is_err());
    }
}
