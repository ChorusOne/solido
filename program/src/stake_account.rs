// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Utilities for dealing with stake accounts.

use std::iter::Sum;
use std::ops::Add;

use crate::{error::LidoError, token, token::Lamports};
use solana_program::stake::{self as stake_program, instruction::StakeInstruction, state::Stake};
use solana_program::{
    clock::{Clock, Epoch},
    instruction::AccountMeta,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar,
};
use solana_program::{instruction::Instruction, stake_history::StakeHistory};

/// The balance of a stake account, split into the four states that stake can be in.
///
/// The sum of the four fields is equal to the SOL balance of the stake account.
/// Note that a stake account can have a portion in `inactive` and a portion in
/// `active`, with zero being activating or deactivating.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StakeBalance {
    pub inactive: Lamports,
    pub activating: Lamports,
    pub active: Lamports,
    pub deactivating: Lamports,
}

#[derive(Copy, Clone, Debug)]

pub struct StakeAccount {
    pub balance: StakeBalance,
    pub credits_observed: u64,
    pub activation_epoch: Epoch,
    pub seed: u64,
}

impl StakeBalance {
    pub fn zero() -> StakeBalance {
        StakeBalance {
            inactive: Lamports(0),
            activating: Lamports(0),
            active: Lamports(0),
            deactivating: Lamports(0),
        }
    }

    /// Return the total balance of the stake account.
    pub fn total(&self) -> Lamports {
        self.inactive
            .add(self.activating)
            .expect("Does not overflow: total fitted in u64 before we split it.")
            .add(self.active)
            .expect("Does not overflow: total fitted in u64 before we split it.")
            .add(self.deactivating)
            .expect("Does not overflow: total fitted in u64 before we split it.")
    }
}

/// Consume a 32-byte pubkey from the data start, return it and the remainder.
fn take_pubkey(data: &[u8]) -> (Pubkey, &[u8]) {
    let mut prefix = [0u8; 32];
    prefix.copy_from_slice(&data[..32]);
    (Pubkey::new(&prefix), &data[32..])
}

/// Consume a little-endian `u32` from the data start, return it and the remainder.
fn take_u32_le(data: &[u8]) -> (u32, &[u8]) {
    // It looks like there is something going on here, but this is just to satisfy
    // the typechecker, I sure hope LLVM compiles this to just an unaligned load.
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&data[..4]);
    (u32::from_le_bytes(prefix), &data[4..])
}

/// Consume a little-endian `u64` from the data start, return it and the remainder.
fn take_u64_le(data: &[u8]) -> (u64, &[u8]) {
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&data[..8]);
    (u64::from_le_bytes(prefix), &data[8..])
}

/// Consume a little-endian `f64` from the data start, return it and the remainder.
fn take_f64_le(data: &[u8]) -> (f64, &[u8]) {
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&data[..8]);
    (f64::from_le_bytes(prefix), &data[8..])
}

/// Deserialize the `meta.rent_exempt_reserve` field in a `StakeState::Stake` account.
/// Implemented manually here because `solana_program` does not implement a deserializer.
pub fn deserialize_rent_exempt_reserve(account_data: &[u8]) -> Result<Lamports, ProgramError> {
    let data = account_data;

    if data.len() < 12 {
        return Err(LidoError::InvalidStakeAccount.into());
    }

    let (type_, data) = take_u32_le(data);
    if type_ != 2 {
        msg!("Stake state should have been StakeState::Stake");
        return Err(LidoError::InvalidStakeAccount.into());
    }

    // After the variant tag, the `Meta` struct follows immediately, and the first
    // field is the one we need.
    let (rent_exempt_reserve, _suffix) = take_u64_le(data);

    Ok(Lamports(rent_exempt_reserve))
}

/// We deserialize the stake account manually here, because `solana_program`
/// does not expose a deserializer for it.
pub fn deserialize_stake_account(account_data: &[u8]) -> Result<Stake, ProgramError> {
    let data = account_data;

    // We read 196 bytes from the data, so check that up front, so that bounds
    // checks can be optimized away below.
    if data.len() < 196 {
        return Err(LidoError::InvalidStakeAccount.into());
    }

    // First: a little-endian 32-bit tag for the variant. We are only
    // interested in `StakeState::Stake`, which has tag 2.
    let (type_, data) = take_u32_le(data);
    if type_ != 2 {
        msg!("Stake state should have been StakeState::Stake");
        return Err(LidoError::InvalidStakeAccount.into());
    }

    // Next up, the 120-byte `Meta`, struct, that we are not interested in.
    let (_meta_bytes, data) = data.split_at(120);

    // Next up, the `Stake` struct.
    // It starts with the `Delegation` struct.
    let (voter_pubkey, data) = take_pubkey(data);
    let (stake, data) = take_u64_le(data);
    let (activation_epoch, data) = take_u64_le(data);
    let (deactivation_epoch, data) = take_u64_le(data);
    let (warmup_cooldown_rate, data) = take_f64_le(data);
    let delegation = stake_program::state::Delegation {
        voter_pubkey,
        stake,
        activation_epoch,
        deactivation_epoch,
        warmup_cooldown_rate,
    };

    // After the `Delegation` is only `credits_observed`.
    let (credits_observed, _suffix) = take_u64_le(data);
    let stake = Stake {
        delegation,
        credits_observed,
    };

    Ok(stake)
}

impl StakeAccount {
    /// Makes an instruction that withdraws from the stake to an account
    pub fn stake_account_withdraw(
        amount: Lamports,
        stake_account: &Pubkey,
        to_account: &Pubkey,
        withdraw_authority: &Pubkey,
    ) -> Instruction {
        let account_metas = vec![
            AccountMeta::new(*stake_account, false),
            AccountMeta::new(*to_account, false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
            AccountMeta::new_readonly(sysvar::stake_history::id(), false),
            AccountMeta::new_readonly(*withdraw_authority, true),
        ];

        Instruction::new_with_bincode(
            stake_program::program::id(),
            &StakeInstruction::Withdraw(amount.0),
            account_metas,
        )
    }
    /// Extract the stake balance from a delegated stake account.
    pub fn from_delegated_account(
        account_lamports: Lamports,
        stake: &Stake,
        clock: &Clock,
        stake_history: &StakeHistory,
        seed: u64,
    ) -> StakeAccount {
        let target_epoch = clock.epoch;
        let history = Some(stake_history);

        let mut state = stake
            .delegation
            .stake_activating_and_deactivating(target_epoch, history);

        // `stake_activating_and_deactivating` counts deactivating stake both as
        // part of the active lamports, and as part of the deactivating
        // lamports, but we want to split the lamports into mutually exclusive
        // categories, so for us, active should not include deactivating
        // lamports. There cannot be more lamports deactivating than there are
        // active lamports, so this does not underflow.
        state.effective -= state.deactivating;
        let inactive_lamports = account_lamports.0
            .checked_sub(state.effective)
            .expect("Active stake cannot be larger than stake account balance.")
            .checked_sub(state.activating)
            .expect("Activating stake cannot be larger than stake account balance - active.")
            .checked_sub(state.deactivating)
            .expect("Deactivating stake cannot be larger than stake account balance - active - activating.");

        StakeAccount {
            balance: StakeBalance {
                inactive: Lamports(inactive_lamports),
                activating: Lamports(state.activating),
                active: Lamports(state.effective),
                deactivating: Lamports(state.deactivating),
            },
            credits_observed: stake.credits_observed,
            activation_epoch: stake.delegation.activation_epoch,
            seed,
        }
    }

    /// Returns `true` if the stake account is active, `false` otherwise.
    pub fn is_active(&self) -> bool {
        self.balance.active > Lamports(0)
            && self.balance.activating == Lamports(0)
            && self.balance.deactivating == Lamports(0)
    }

    /// Returns `true` if the stake account is inactive, `false` otherwise.
    pub fn is_inactive(&self) -> bool {
        self.balance.active == Lamports(0)
            && self.balance.activating == Lamports(0)
            && self.balance.deactivating == Lamports(0)
    }

    /// Returns `true` if the stake account is activating, `false` otherwise.
    pub fn is_activating(&self) -> bool {
        self.balance.activating > Lamports(0)
    }

    /// Returs `true` if `merge_from` can be merged into this stake account, `false` otherwise.
    /// see: https://docs.solana.com/staking/stake-accounts
    pub fn can_merge(&self, merge_from: &Self) -> bool {
        // Two deactivated stakes
        if self.is_inactive() && merge_from.is_inactive() {
            return true;
        }
        // An inactive stake into an activating stake during its activation epoch.
        // Note: although the docs don't say so, merge is symmetric. See also
        // `tests::solana_assumptions`.
        if (merge_from.is_inactive() && self.is_activating())
            || (self.is_inactive() && merge_from.is_activating())
        {
            return true;
        }
        // The voter pubkey and credits observed must match. Voter must be the same by assumption.
        if self.credits_observed == merge_from.credits_observed {
            // Two activated stakes.
            if self.is_active() && merge_from.is_active() {
                return true;
            }
            // Two activating accounts that share an activation epoch, during the activation epoch.
            if self.is_activating()
                && merge_from.is_activating()
                && self.activation_epoch == merge_from.activation_epoch
            {
                return true;
            }
        }
        false
    }
}

impl Add for StakeBalance {
    type Output = token::Result<StakeBalance>;

    fn add(self, other: StakeBalance) -> token::Result<StakeBalance> {
        let result = StakeBalance {
            inactive: (self.inactive + other.inactive)?,
            activating: (self.activating + other.activating)?,
            active: (self.active + other.active)?,
            deactivating: (self.deactivating + other.deactivating)?,
        };
        Ok(result)
    }
}

// Ideally we would implement this for Result<StakeBalance>, but it isn't allowed
// due to orphan impl rules. Curiously, it does work in our `impl_token!` macro.
// But in any case, overflow should not happen on mainnet, so we can make it
// panic for now. It will make it harder to fuzz later though.
impl Sum for StakeBalance {
    fn sum<I: Iterator<Item = StakeBalance>>(iter: I) -> Self {
        let mut accumulator = StakeBalance::zero();
        for x in iter {
            accumulator = (accumulator + x).expect(
                "Overflow when adding stake balances, this should not happen \
                because there is not that much SOL in the ecosystem.",
            )
        }
        accumulator
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use solana_program::rent::Rent;
    use solana_program::stake::state::Delegation;
    use std::str::FromStr;

    #[test]
    fn test_deserialize_stake_account() {
        // Actual stake account, printed from one of the `solana_program_test` tests.
        let stake_account_data = [
            2_u8, 0, 0, 0, 128, 213, 34, 0, 0, 0, 0, 0, 109, 205, 23, 189, 77, 39, 158, 172, 203,
            232, 104, 67, 226, 58, 21, 243, 188, 167, 146, 138, 219, 130, 169, 165, 102, 229, 186,
            26, 37, 216, 129, 239, 109, 205, 23, 189, 77, 39, 158, 172, 203, 232, 104, 67, 226, 58,
            21, 243, 188, 167, 146, 138, 219, 130, 169, 165, 102, 229, 186, 26, 37, 216, 129, 239,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 123, 78, 181, 133, 59, 177,
            25, 168, 47, 189, 98, 97, 72, 40, 220, 29, 58, 189, 47, 120, 44, 190, 215, 164, 200,
            134, 123, 116, 72, 25, 135, 124, 202, 134, 166, 88, 34, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 208, 63, 1, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0,
        ];

        let actual = deserialize_stake_account(&stake_account_data).unwrap();
        let expected = Stake {
            delegation: Delegation {
                voter_pubkey: Pubkey::from_str("9JLkwJFXQL548xYfspjaZQws9MCXAF3NYYux9AxUxEfd")
                    .unwrap(),
                stake: 1_247_027_824_330,
                activation_epoch: 0,
                deactivation_epoch: u64::MAX,
                warmup_cooldown_rate: 0.25,
            },
            credits_observed: 1,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_deserialize_rent_exempt_reserve() {
        // Actual stake account, printed from one of the `solana_program_test` tests. Therefore, its
        // rent-exempt balance stored in the account, should be equal to the rent-exempt balance for
        // an account of that size.
        let stake_account_data = [
            2_u8, 0, 0, 0, 128, 213, 34, 0, 0, 0, 0, 0, 109, 205, 23, 189, 77, 39, 158, 172, 203,
            232, 104, 67, 226, 58, 21, 243, 188, 167, 146, 138, 219, 130, 169, 165, 102, 229, 186,
            26, 37, 216, 129, 239, 109, 205, 23, 189, 77, 39, 158, 172, 203, 232, 104, 67, 226, 58,
            21, 243, 188, 167, 146, 138, 219, 130, 169, 165, 102, 229, 186, 26, 37, 216, 129, 239,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 123, 78, 181, 133, 59, 177,
            25, 168, 47, 189, 98, 97, 72, 40, 220, 29, 58, 189, 47, 120, 44, 190, 215, 164, 200,
            134, 123, 116, 72, 25, 135, 124, 202, 134, 166, 88, 34, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 208, 63, 1, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0,
        ];

        let actual = deserialize_rent_exempt_reserve(&stake_account_data).unwrap();
        let rent = Rent::default();
        let expected = rent.minimum_balance(stake_account_data.len());
        assert_eq!(actual, Lamports(expected));
    }
}
