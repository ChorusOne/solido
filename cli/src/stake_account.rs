//! Utilities for dealing with stake accounts.

use std::iter::Sum;
use std::ops::Add;

use lido::token::Lamports;
use solana_program::clock::Clock;
use solana_program::stake_history::StakeHistory;
use solana_sdk::clock::Epoch;
use spl_stake_pool::stake_program::Stake;

/// The balance of a stake account, split into the four states that stake can be in.
///
/// The sum of the four fields is equal to the SOL balance of the stake account.
/// Note that a stake account can have a portion in `inactive` and a portion in
/// `active`, with zero being activating or deactivating.
#[derive(Copy, Clone)]
pub struct StakeBalance {
    pub inactive: Lamports,
    pub activating: Lamports,
    pub active: Lamports,
    pub deactivating: Lamports,
}

#[derive(Copy, Clone)]

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
}
impl StakeAccount {
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

        // This toggle is a historical quirk in Solana and should always be set
        // to true. See also https://github.com/ChorusOne/solido/issues/184#issuecomment-861653316.
        let fix_stake_deactivate = true;

        let (active_lamports, activating_lamports, deactivating_lamports) = stake
            .delegation
            .stake_activating_and_deactivating(target_epoch, history, fix_stake_deactivate);

        let inactive_lamports = account_lamports.0
            .checked_sub(active_lamports)
            .expect("Active stake cannot be larger than stake account balance.")
            .checked_sub(activating_lamports)
            .expect("Activating stake cannot be larger than stake account balance - active.")
            .checked_sub(deactivating_lamports)
            .expect("Deactivating stake cannot be larger than stake account balance - active - activating.");

        StakeAccount {
            balance: StakeBalance {
                inactive: Lamports(inactive_lamports),
                activating: Lamports(activating_lamports),
                active: Lamports(active_lamports),
                deactivating: Lamports(deactivating_lamports),
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
        if merge_from.is_inactive() && self.is_activating() {
            return true;
        }
        // The voter pubkey and credits observed must match. Voter must be the same by assumption.
        if self.credits_observed == merge_from.credits_observed {
            // Two activated stakes.
            if self.is_active() && merge_from.is_active() {
                return true;
            }
            // Two activating accounts that share an activation epoch, during the activation epoch.
            if self.is_activating() && merge_from.is_activating() {
                return true;
            }
        }
        false
    }
}

impl Add for StakeBalance {
    type Output = Option<StakeBalance>;

    fn add(self, other: StakeBalance) -> Option<StakeBalance> {
        let result = StakeBalance {
            inactive: (self.inactive + other.inactive)?,
            activating: (self.activating + other.activating)?,
            active: (self.active + other.active)?,
            deactivating: (self.deactivating + other.deactivating)?,
        };
        Some(result)
    }
}

// Ideally we would implement this for Option<StakeBalance>, but it isn't allowed
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
