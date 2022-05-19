// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use crate::{error::LidoError, find_authority_program_address, REWARDS_WITHDRAW_AUTHORITY};
use solana_program::{account_info::AccountInfo, msg, pubkey::Pubkey};
use std::convert::TryInto;

/// Structure used to read the first 4 fields of a Solana `VoteAccount`.
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
    /// Solido vote accounts should be owned by the vote program, must have a
    /// 100% fee, and have the withdraw authority set to the Solido program
    /// specified as `program_id`.
    pub fn deserialize(
        program_id: &Pubkey,
        lido_address: &Pubkey,
        validator_vote_account: &AccountInfo,
        max_validator_fee: u8,
    ) -> Result<Self, LidoError> {
        if validator_vote_account.owner != &solana_program::vote::program::id() {
            msg!(
                "Expected validator's vote account to be owned by {}, it's owned by {} instead.",
                solana_program::vote::program::id(),
                validator_vote_account.owner
            );
            return Err(LidoError::ValidatorVoteAccountHasDifferentOwner);
        }
        let data = validator_vote_account.data.borrow();
        if data.len() <= PARTIAL_VOTE_STATE_LEN {
            return Err(LidoError::InvalidVoteAccount);
        }
        // Read 4 bytes for u32.
        let version = u32::from_le_bytes(
            data[0..4]
                .try_into()
                .map_err(|_| LidoError::InvalidVoteAccount)?,
        );
        if version != 1 {
            msg!(
                "Vote State account version should be 1, it's {} instead.",
                version
            );
            return Err(LidoError::InvalidVoteAccount);
        }
        let mut pubkey_buf: [u8; 32] = Default::default();
        // Read 32 bytes for Pubkey.
        pubkey_buf.copy_from_slice(&data[4..][..32]);
        let node_pubkey = Pubkey::new_from_array(pubkey_buf);
        // Read 32 bytes for Pubkey.
        pubkey_buf.copy_from_slice(&data[36..][..32]);
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
        if commission > max_validator_fee {
            msg!(
                "Vote Account's commission should be <= {}, is {} instead",
                max_validator_fee,
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
        let mut data = [
            1, 0, 0, 0, 186, 184, 236, 203, 192, 204, 36, 2, 192, 179, 250, 41, 63, 131, 130, 170,
            227, 31, 172, 215, 203, 45, 217, 159, 149, 38, 254, 230, 96, 89, 100, 169, 44, 222, 22,
            204, 119, 148, 166, 154, 32, 195, 245, 215, 117, 57, 183, 164, 68, 73, 97, 66, 223,
            214, 169, 126, 8, 230, 204, 87, 3, 19, 162, 46, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 186, 184, 236, 203, 192, 204, 36, 2, 192,
            179, 250, 41, 63, 131, 130, 170, 227, 31, 172, 215, 203, 45, 217, 159, 149, 38, 254,
            230, 96, 89, 100, 169, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let program_id = Pubkey::from_str("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc").unwrap();
        let lido_address =
            Pubkey::from_str("6ZSSitQ4RqxUcspvS4J1x76J3dAURjU93d3TKU8HSYxs").unwrap();
        let (lido_withdraw_authority, _) =
            find_authority_program_address(&program_id, &lido_address, REWARDS_WITHDRAW_AUTHORITY);

        let acc_key = Pubkey::new_unique();
        let owner = solana_program::vote::program::id();
        let mut lamports = 0;
        let account = AccountInfo::new(
            &acc_key,
            true,
            true,
            &mut lamports,
            &mut data,
            &owner,
            false,
            0,
        );

        let partial_vote =
            PartialVoteState::deserialize(&program_id, &lido_address, &account, 100).unwrap();
        let expected_partial_result = PartialVoteState {
            version: 1,
            node_pubkey: Pubkey::from_str("DZtP4b6tZSY3XWBQDpuATc2mxB8LUh4Pp5t8Jnz9HLWC").unwrap(),
            authorized_withdrawer: lido_withdraw_authority,
            commission: 100,
        };
        assert_eq!(expected_partial_result, partial_vote);
    }

    #[test]
    fn test_less_commission() {
        // excerpt from actual vote account
        let mut data = [
            1, 0, 0, 0, 186, 184, 236, 203, 192, 204, 36, 2, 192, 179, 250, 41, 63, 131, 130, 170,
            227, 31, 172, 215, 203, 45, 217, 159, 149, 38, 254, 230, 96, 89, 100, 169, 44, 222, 22,
            204, 119, 148, 166, 154, 32, 195, 245, 215, 117, 57, 183, 164, 68, 73, 97, 66, 223,
            214, 169, 126, 8, 230, 204, 87, 3, 19, 162, 46, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 186, 184, 236, 203, 192, 204, 36, 2, 192, 179,
            250, 41, 63, 131, 130, 170, 227, 31, 172, 215, 203, 45, 217, 159, 149, 38, 254, 230,
            96, 89, 100, 169, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let program_id = Pubkey::from_str("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc").unwrap();
        let lido_address =
            Pubkey::from_str("DZtP4b6tZSY3XWBQDpuATc2mxB8LUh4Pp5t8Jnz9HLWC").unwrap();

        let acc_key = Pubkey::new_unique();
        let owner = solana_program::vote::program::id();
        let mut lamports = 0;
        let account = AccountInfo::new(
            &acc_key,
            true,
            true,
            &mut lamports,
            &mut data,
            &owner,
            false,
            0,
        );
        assert_eq!(
            PartialVoteState::deserialize(&program_id, &lido_address, &account, 100),
            Err(LidoError::InvalidVoteAccount)
        );
    }
}
