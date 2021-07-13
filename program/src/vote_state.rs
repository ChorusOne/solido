use crate::{error::LidoError, find_authority_program_address, REWARDS_WITHDRAW_AUTHORITY};
use byteorder::{LittleEndian, ReadBytesExt};
use solana_program::{msg, pubkey::Pubkey};

/// Structure used to read the first 3 fields of a Solana `VoteAccount`.
/// The original `VoteAccount` structure cannot be used in a Solana
/// program due to size constrains.

const PARTIAL_VOTE_STATE_LEN: usize = 69;
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
    pub fn deserialize(
        program_id: &Pubkey,
        lido_address: &Pubkey,
        data: &[u8],
    ) -> Result<Self, LidoError> {
        if data.len() <= PARTIAL_VOTE_STATE_LEN {
            return Err(LidoError::InvalidVoteAccount);
        }
        // Read 4 bytes for u32.
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
        // Read 32 bytes for Pubkey.
        pubkey_buf.copy_from_slice(&data[4..36]);
        let node_pubkey = Pubkey::new_from_array(pubkey_buf);
        // Read 32 bytes for Pubkey.
        pubkey_buf.copy_from_slice(&data[36..68]);
        let authorized_withdrawer = Pubkey::new_from_array(pubkey_buf);

        let (lido_withdraw_authority, _) =
            find_authority_program_address(program_id, lido_address, REWARDS_WITHDRAW_AUTHORITY);
        if authorized_withdrawer != lido_withdraw_authority {
            msg!(
                "Vote Account's withdrawer should be {}, is {} instead.",
                lido_withdraw_authority,
                authorized_withdrawer
            );
            return Err(LidoError::InvalidVoteAccount);
        }
        // Read 1 byte for u8.
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
            1, 0, 0, 0, 180, 53, 218, 159, 38, 49, 69, 73, 80, 121, 168, 248, 205, 118, 6, 230,
            209, 124, 181, 128, 20, 104, 14, 54, 240, 62, 196, 143, 243, 28, 96, 243, 103, 85, 133,
            162, 105, 18, 173, 186, 226, 83, 7, 70, 80, 145, 100, 154, 49, 103, 152, 90, 87, 169,
            112, 255, 7, 37, 148, 29, 13, 170, 162, 35, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 180, 53, 218, 159, 38, 49, 69, 73, 80, 121, 168,
            248, 205, 118, 6, 230, 209, 124, 181, 128, 20, 104, 14, 54, 240, 62, 196, 143, 243, 28,
            96, 243, 0, 0, 0, 0, 0,
        ];
        let program_id = Pubkey::from_str("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc").unwrap();
        let lido_address =
            Pubkey::from_str("HrdD6QcHDXadFe8EQjcGu45GvmnM7ZRuMzpdGz4WtaiN").unwrap();
        let (lido_withdraw_authority, _) =
            find_authority_program_address(&program_id, &lido_address, REWARDS_WITHDRAW_AUTHORITY);

        let partial_vote =
            PartialVoteState::deserialize(&program_id, &lido_address, &data).unwrap();
        let expected_partial_result = PartialVoteState {
            version: 1,
            node_pubkey: Pubkey::from_str("D8U1qEq5DeFTUsHpviJvnKjJBzuhqqJPCGFk85DRTz74").unwrap(),
            authorized_withdrawer: lido_withdraw_authority,
            commission: 100,
        };
        assert_eq!(expected_partial_result, partial_vote);
    }

    #[test]
    fn test_less_commission() {
        // excerpt from actual vote account
        let data = [
            1, 0, 0, 0, 180, 53, 218, 159, 38, 49, 69, 73, 80, 121, 168, 248, 205, 118, 6, 230,
            209, 124, 181, 128, 20, 104, 14, 54, 240, 62, 196, 143, 243, 28, 96, 243, 103, 85, 133,
            162, 105, 18, 173, 186, 226, 83, 7, 70, 80, 145, 100, 154, 49, 103, 152, 90, 87, 169,
            112, 255, 7, 37, 148, 29, 13, 170, 162, 35, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 180, 53, 218, 159, 38, 49, 69, 73, 80, 121, 168,
            248, 205, 118, 6, 230, 209, 124, 181, 128, 20, 104, 14, 54, 240, 62, 196, 143, 243, 28,
            96, 243, 0, 0, 0, 0, 0,
        ];

        let program_id = Pubkey::from_str("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc").unwrap();
        let lido_address =
            Pubkey::from_str("HrdD6QcHDXadFe8EQjcGu45GvmnM7ZRuMzpdGz4WtaiN").unwrap();
        assert!(PartialVoteState::deserialize(&program_id, &lido_address, &data).is_err());
    }
}
