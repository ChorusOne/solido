// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Entry point for maintenance operations, such as updating the pool balance.

use std::fmt;
use std::io;
use std::time::SystemTime;

use itertools::izip;

use lido::processor::StakeType;
use lido::token;
use lido::REWARDS_WITHDRAW_AUTHORITY;
use serde::Serialize;
use solana_program::program_pack::Pack;
use solana_program::{clock::Clock, pubkey::Pubkey, rent::Rent, stake_history::StakeHistory};
use solana_sdk::account::ReadableAccount;
use solana_sdk::fee_calculator::DEFAULT_TARGET_LAMPORTS_PER_SIGNATURE;
use solana_sdk::{account::Account, instruction::Instruction};
use spl_token::state::Mint;

use lido::token::StLamports;
use lido::{account_map::PubkeyAndEntry, stake_account::StakeAccount, MINT_AUTHORITY};
use lido::{
    stake_account::{deserialize_stake_account, StakeBalance},
    util::serialize_b58,
};
use lido::{
    state::{Lido, Validator},
    token::Lamports,
    MINIMUM_STAKE_ACCOUNT_BALANCE, STAKE_AUTHORITY,
};

use crate::error::MaintenanceError;
use crate::snapshot::Result;
use crate::{config::PerformMaintenanceOpts, SnapshotConfig};

/// A brief description of the maintenance performed. Not relevant functionally,
/// but helpful for automated testing, and just for info.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub enum MaintenanceOutput {
    StakeDeposit {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        stake_account: Pubkey,

        #[serde(rename = "amount_lamports")]
        amount: Lamports,
    },

    UpdateExchangeRate,

    WithdrawInactiveStake {
        /// The vote account of the validator that we want to update.
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,

        /// The expected difference that the update will observe.
        ///
        /// This is only an expected value, because a different transaction might
        /// execute between us observing the state and concluding that there is
        /// a difference, and our `WithdrawInactiveStake` instruction executing.
        #[serde(rename = "expected_difference_stake_lamports")]
        expected_difference_stake: Lamports,

        #[serde(rename = "unstaked_amount_lamports")]
        unstaked_amount: Lamports,
    },

    CollectValidatorFee {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
        #[serde(rename = "fee_rewards_lamports")]
        fee_rewards: Lamports,
    },

    ClaimValidatorFee {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
        #[serde(rename = "fee_rewards_st_lamports")]
        fee_rewards: StLamports,
    },

    MergeStake {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
        #[serde(serialize_with = "serialize_b58")]
        from_stake: Pubkey,
        #[serde(serialize_with = "serialize_b58")]
        to_stake: Pubkey,
        from_stake_seed: u64,
        to_stake_seed: u64,
    },

    UnstakeFromInactiveValidator {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
        #[serde(serialize_with = "serialize_b58")]
        from_stake_account: Pubkey,
        #[serde(serialize_with = "serialize_b58")]
        to_unstake_account: Pubkey,
        from_stake_seed: u64,
        to_unstake_seed: u64,
        amount: Lamports,
    },
}

impl fmt::Display for MaintenanceOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MaintenanceOutput::StakeDeposit {
                validator_vote_account,
                stake_account,
                amount,
            } => {
                writeln!(f, "Staked deposit.")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(f, "  Stake account:          {}", stake_account)?;
                writeln!(f, "  Amount staked:          {}", amount)?;
            }
            MaintenanceOutput::UpdateExchangeRate => {
                writeln!(f, "Updated exchange rate.")?;
            }
            MaintenanceOutput::WithdrawInactiveStake {
                validator_vote_account,
                expected_difference_stake,
                unstaked_amount,
            } => {
                writeln!(f, "Withdrew inactive stake.")?;
                writeln!(
                    f,
                    "  Validator vote account:        {}",
                    validator_vote_account
                )?;
                writeln!(
                    f,
                    "  Expected difference in stake:  {}",
                    expected_difference_stake
                )?;
                writeln!(f, "  Amount withdrawn from unstake: {}", unstaked_amount)?;
            }
            MaintenanceOutput::CollectValidatorFee {
                validator_vote_account,
                fee_rewards,
            } => {
                writeln!(f, "Collected validator fees.")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(f, "  Collected fee rewards:  {}", fee_rewards)?;
            }

            MaintenanceOutput::ClaimValidatorFee {
                validator_vote_account,
                fee_rewards,
            } => {
                writeln!(f, "Claimed validator fees.")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(f, "  Claimed fee:            {}", fee_rewards)?;
            }
            MaintenanceOutput::MergeStake {
                validator_vote_account,
                from_stake,
                to_stake,
                from_stake_seed,
                to_stake_seed,
            } => {
                writeln!(f, "Stake accounts merged")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(
                    f,
                    "  From stake:             {}, seed: {}",
                    from_stake, from_stake_seed
                )?;
                writeln!(
                    f,
                    "  To stake:               {}, seed: {}",
                    to_stake, to_stake_seed
                )?;
            }
            MaintenanceOutput::UnstakeFromInactiveValidator {
                validator_vote_account,
                from_stake_account,
                to_unstake_account,
                from_stake_seed,
                to_unstake_seed,
                amount,
            } => {
                writeln!(f, "Unstake from inactive validator")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(
                    f,
                    "  Stake account:               {}, seed: {}",
                    from_stake_account, from_stake_seed
                )?;
                writeln!(
                    f,
                    "  Unstake account:             {}, seed: {}",
                    to_unstake_account, to_unstake_seed
                )?;
                writeln!(f, "  Amount:              {}", amount)?;
            }
        }
        Ok(())
    }
}

/// A snapshot of on-chain accounts relevant to Solido.
pub struct SolidoState {
    /// The time at which we finished querying the Solido state.
    ///
    /// This is used in metrics to assign a timestamp to metrics that we proxy
    /// from the state and then expose to Prometheus. Because the time at which
    /// Prometheus polls is not the time at which the data was obtained, we track
    /// the timestamp to avoid introducing a polling delay in the reported metrics.
    ///
    /// There is also `clock.unix_timestamp`, which is the stake-weighted median
    /// timestamp of the slot that we queried, rather than the time at the machine
    /// that performed the query. However, when you run `solana-test-validator`,
    /// that timestamp goes out of date quickly, so use the actual observed time
    /// instead.
    pub produced_at: SystemTime,

    pub solido_program_id: Pubkey,
    pub solido_address: Pubkey,
    pub solido: Lido,

    /// For each validator, in the same order as in `solido.validators`, holds
    /// the stake balance of the derived stake accounts from the begin seed until
    /// end seed.
    pub validator_stake_accounts: Vec<Vec<(Pubkey, StakeAccount)>>,
    /// Similar to the stake accounts, holds the unstake balance of the derived
    /// unstake accounts from the begin seed until end seed.
    pub validator_unstake_accounts: Vec<Vec<(Pubkey, StakeAccount)>>,
    /// For each validator, in the same order as in `solido.validators`, holds
    /// the number of Lamports of the validator's vote account.
    pub validator_vote_account_balances: Vec<Lamports>,

    /// SPL token mint for stSOL, to know the current supply.
    pub st_sol_mint: Mint,

    pub reserve_address: Pubkey,
    pub reserve_account: Account,
    pub rent: Rent,
    pub clock: Clock,

    /// Public key of the maintainer executing the maintenance.
    /// Must be a member of `solido.maintainers`.
    pub maintainer_address: Pubkey,

    /// Current state of the maintainer account.
    pub maintainer_account: Account,
}

fn get_validator_stake_accounts(
    config: &mut SnapshotConfig,
    solido_program_id: &Pubkey,
    solido_address: &Pubkey,
    clock: &Clock,
    stake_history: &StakeHistory,
    validator: &PubkeyAndEntry<Validator>,
    stake_type: StakeType,
) -> Result<Vec<(Pubkey, StakeAccount)>> {
    let mut result = Vec::new();
    let seeds = match stake_type {
        StakeType::Stake => &validator.entry.stake_seeds,
        StakeType::Unstake => &validator.entry.unstake_seeds,
    };
    for seed in seeds {
        let (addr, _bump_seed) = match stake_type {
            StakeType::Stake => {
                validator.find_stake_account_address(solido_program_id, solido_address, seed)
            }
            StakeType::Unstake => {
                validator.find_unstake_account_address(solido_program_id, solido_address, seed)
            }
        };
        let account = config.client.get_account(&addr)?;
        let stake = deserialize_stake_account(&account.data)
            .expect("Derived stake account contains invalid data.");

        assert_eq!(
            stake.delegation.voter_pubkey, validator.pubkey,
            "Expected the stake account for validator to delegate to that validator."
        );

        let balance = StakeAccount::from_delegated_account(
            Lamports(account.lamports),
            &stake,
            clock,
            stake_history,
            seed,
        );

        result.push((addr, balance));
    }
    Ok(result)
}

fn get_vote_account_balance_except_rent(
    config: &mut SnapshotConfig,
    rent: &Rent,
    validator_vote_account: &Pubkey,
) -> Result<Lamports> {
    let vote_account = config.client.get_account(validator_vote_account)?;
    let vote_rent = rent.minimum_balance(vote_account.data().len());
    Ok(
        (Lamports(vote_account.lamports()) - Lamports(vote_rent)).expect(
            "Shouldn't happen. The vote account balance should be at least its rent-exempt balance.",
        ),
    )
}

impl SolidoState {
    // Set the minimum withdraw from stake accounts and validator's vote
    // accounts, the cost of validating signatures seems to dominate the
    // transaction cost.
    const MINIMUM_WITHDRAW_AMOUNT: Lamports = Lamports(DEFAULT_TARGET_LAMPORTS_PER_SIGNATURE * 100);
    /// Read the state from the on-chain data.
    pub fn new(
        config: &mut SnapshotConfig,
        solido_program_id: &Pubkey,
        solido_address: &Pubkey,
    ) -> Result<SolidoState> {
        let solido = config.client.get_solido(solido_address)?;

        let reserve_address = solido.get_reserve_account(solido_program_id, solido_address)?;
        let reserve_account = config.client.get_account(&reserve_address)?;

        let st_sol_mint_account = config.client.get_account(&solido.st_sol_mint)?;
        let st_sol_mint = Mint::unpack(&st_sol_mint_account.data)?;

        let rent = config.client.get_rent()?;
        let clock = config.client.get_clock()?;
        let stake_history = config.client.get_stake_history()?;

        let mut validator_stake_accounts = Vec::new();
        let mut validator_unstake_accounts = Vec::new();
        let mut validator_vote_account_balances = Vec::new();
        for validator in solido.validators.entries.iter() {
            validator_vote_account_balances.push(get_vote_account_balance_except_rent(
                config,
                &rent,
                &validator.pubkey,
            )?);

            validator_stake_accounts.push(get_validator_stake_accounts(
                config,
                solido_program_id,
                solido_address,
                &clock,
                &stake_history,
                validator,
                StakeType::Stake,
            )?);
            validator_unstake_accounts.push(get_validator_stake_accounts(
                config,
                solido_program_id,
                solido_address,
                &clock,
                &stake_history,
                validator,
                StakeType::Unstake,
            )?);
        }

        // The entity executing the maintenance transactions, is the maintainer.
        // We don't verify here if it is part of the maintainer set, the on-chain
        // program does that anyway.
        let maintainer_address = config.signer.pubkey();
        let maintainer_account = config.client.get_account(&maintainer_address)?;

        Ok(SolidoState {
            produced_at: SystemTime::now(),
            solido_program_id: *solido_program_id,
            solido_address: *solido_address,
            solido,
            validator_stake_accounts,
            validator_unstake_accounts,
            validator_vote_account_balances,
            reserve_address,
            reserve_account: reserve_account.clone(),
            st_sol_mint,
            rent,
            clock,
            maintainer_address,
            maintainer_account: maintainer_account.clone(),
        })
    }

    /// Return the amount of SOL in the reserve account that could be spent
    /// while still keeping the reserve account rent-exempt.
    pub fn get_effective_reserve(&self) -> Lamports {
        Lamports(
            self.reserve_account
                .lamports
                .saturating_sub(self.rent.minimum_balance(0)),
        )
    }

    /// If there is a deposit that can be staked, return the instructions to do so.
    pub fn try_stake_deposit(&self) -> Option<(Instruction, MaintenanceOutput)> {
        // We can only stake if there is an active validator. If there is none,
        // this will short-circuit and return None.
        self.solido.validators.iter_active().next()?;

        let reserve_balance = self.get_effective_reserve();

        // If there is enough reserve, we can make a deposit. To keep the pool
        // balanced, find the validator furthest below its target balance, and
        // deposit to that validator. If we get here there is at least one active
        // validator, so computing the target balance should not fail.
        let undelegated_lamports = reserve_balance;
        let targets =
            lido::balance::get_target_balance(undelegated_lamports, &self.solido.validators)
                .expect("Failed to compute target balance.");

        let (validator_index, amount_below_target) =
            lido::balance::get_validator_furthest_below_target(
                &self.solido.validators,
                &targets[..],
            );
        let validator = &self.solido.validators.entries[validator_index];

        let (stake_account_end, _bump_seed_end) = validator.find_stake_account_address(
            &self.solido_program_id,
            &self.solido_address,
            validator.entry.stake_seeds.end,
        );

        // Top up the validator to at most its target. If that means we don't use the full
        // reserve, a future maintenance run will stake the remainder with the next validator.
        let mut amount_to_deposit = amount_below_target.min(reserve_balance);

        // However, if the amount needed to bring the validator to its target is
        // less than the minimum stake account balance, then we would have to wait
        // until there is `MINIMUM_STAKE_ACCOUNT_BALANCE * num_validators` in the
        // reserve (assuming they are currently balanced) before we stake anything,
        // which would be wasteful. In this case, we rather overshoot the target
        // temporarily, and future deposits will restore the balance.
        amount_to_deposit = amount_to_deposit.max(MINIMUM_STAKE_ACCOUNT_BALANCE);

        // The minimum stake account balance might be more than what's in the
        // reserve. If so, we cannot stake.
        if amount_to_deposit > reserve_balance {
            return None;
        }

        // When we stake a deposit, if possible, we create a new stake account
        // temporarily, but then immediately merge it into the preceding account.
        // This is possible if there is a preceding account, and if it was
        // activated in the current epoch. If merging is not possible, then we
        // set `account_merge_into` to the same account as `end`, to signal that
        // we shouldn't merge.
        let account_merge_into = match self.validator_stake_accounts[validator_index].last() {
            Some((addr, account)) if account.activation_epoch == self.clock.epoch => *addr,
            _ => stake_account_end,
        };

        let instruction = lido::instruction::stake_deposit(
            &self.solido_program_id,
            &lido::instruction::StakeDepositAccountsMeta {
                lido: self.solido_address,
                maintainer: self.maintainer_address,
                reserve: self.reserve_address,
                validator_vote_account: validator.pubkey,
                stake_account_merge_into: account_merge_into,
                stake_account_end,
                stake_authority: self.get_stake_authority(),
            },
            amount_to_deposit,
        );
        let task = MaintenanceOutput::StakeDeposit {
            validator_vote_account: validator.pubkey,
            amount: amount_to_deposit,
            stake_account: stake_account_end,
        };
        Some((instruction, task))
    }

    /// If there is a validator being deactivated, try to unstake its funds.
    pub fn try_unstake_from_inactive_validator(&self) -> Option<(Instruction, MaintenanceOutput)> {
        for (validator, stake_accounts) in self
            .solido
            .validators
            .entries
            .iter()
            .zip(self.validator_stake_accounts.iter())
        {
            // We are only interested in unstaking from inactive validators that
            // have stake accounts.
            if validator.entry.active || stake_accounts.is_empty() {
                continue;
            }
            // Validator already has 3 unstake accounts.
            if validator.entry.unstake_seeds.end - validator.entry.unstake_seeds.begin >= 3 {
                continue;
            }
            let (validator_unstake_account, _) = validator.find_unstake_account_address(
                &self.solido_program_id,
                &self.solido_address,
                validator.entry.unstake_seeds.end,
            );
            let task = MaintenanceOutput::UnstakeFromInactiveValidator {
                validator_vote_account: validator.pubkey,
                from_stake_account: stake_accounts[0].0,
                to_unstake_account: validator_unstake_account,
                from_stake_seed: validator.entry.stake_seeds.begin,
                to_unstake_seed: validator.entry.unstake_seeds.end,
                amount: stake_accounts[0].1.balance.total(),
            };

            return Some((
                lido::instruction::unstake(
                    &self.solido_program_id,
                    &lido::instruction::UnstakeAccountsMeta {
                        lido: self.solido_address,
                        maintainer: self.maintainer_address,
                        validator_vote_account: validator.pubkey,
                        source_stake_account: stake_accounts[0].0,
                        destination_unstake_account: validator_unstake_account,
                        stake_authority: self.get_stake_authority(),
                    },
                    stake_accounts[0].1.balance.total(),
                ),
                task,
            ));
        }
        None
    }

    /// Get an instruction to merge accounts.
    fn get_merge_instruction(
        &self,
        validator: &PubkeyAndEntry<Validator>,
        from_seed: u64,
        to_seed: u64,
    ) -> Instruction {
        // Stake Account created by this transaction.
        let (from_stake, _bump_seed_end) = validator.find_stake_account_address(
            &self.solido_program_id,
            &self.solido_address,
            from_seed,
        );
        // Stake Account created by this transaction.
        let (to_stake, _bump_seed_end) = validator.find_stake_account_address(
            &self.solido_program_id,
            &self.solido_address,
            to_seed,
        );
        lido::instruction::merge_stake(
            &self.solido_program_id,
            &lido::instruction::MergeStakeMeta {
                lido: self.solido_address,
                validator_vote_account: validator.pubkey,
                from_stake,
                to_stake,
                stake_authority: self.get_stake_authority(),
            },
        )
    }

    // Tries to merge accounts from the beginning of the validator's
    // stake accounts.  May return None or one instruction.
    pub fn try_merge_on_all_stakes(&self) -> Option<(Instruction, MaintenanceOutput)> {
        for (validator, stake_accounts) in self
            .solido
            .validators
            .entries
            .iter()
            .zip(self.validator_stake_accounts.iter())
        {
            // Try to merge from beginning
            if stake_accounts.len() > 1 {
                let from_stake = stake_accounts[0];
                let to_stake = stake_accounts[1];
                if to_stake.1.can_merge(&from_stake.1) {
                    let instruction =
                        self.get_merge_instruction(validator, from_stake.1.seed, to_stake.1.seed);
                    let task = MaintenanceOutput::MergeStake {
                        validator_vote_account: validator.pubkey,
                        from_stake: from_stake.0,
                        to_stake: to_stake.0,
                        from_stake_seed: from_stake.1.seed,
                        to_stake_seed: to_stake.1.seed,
                    };
                    return Some((instruction, task));
                }
            }
        }
        None
    }

    /// If a new epoch started, and we haven't updated the exchange rate yet, do so.
    pub fn try_update_exchange_rate(&self) -> Option<(Instruction, MaintenanceOutput)> {
        if self.solido.exchange_rate.computed_in_epoch >= self.clock.epoch {
            // The exchange rate has already been updated in this epoch, nothing to do.
            return None;
        }

        let instruction = lido::instruction::update_exchange_rate(
            &self.solido_program_id,
            &lido::instruction::UpdateExchangeRateAccountsMeta {
                lido: self.solido_address,
                reserve: self.reserve_address,
                st_sol_mint: self.solido.st_sol_mint,
            },
        );
        let task = MaintenanceOutput::UpdateExchangeRate;

        Some((instruction, task))
    }

    /// Check if any validator's balance is outdated, and if so, update it.
    ///
    /// Merging stakes generates inactive stake that could be withdrawn with this transaction,
    /// or if some joker donates to one of the stake accounts we can use the same function
    /// to claim these rewards back to the reserve account so they can be re-staked.
    pub fn try_withdraw_inactive_stake(&self) -> Option<(Instruction, MaintenanceOutput)> {
        for (validator, stake_accounts, unstake_accounts) in izip!(
            self.solido.validators.entries.iter(),
            self.validator_stake_accounts.iter(),
            self.validator_unstake_accounts.iter()
        ) {
            let current_stake_balance = stake_accounts
                .iter()
                .map(|(_addr, detail)| detail.balance.total())
                .sum::<token::Result<Lamports>>()
                .expect("If this overflows, there would be more than u64::MAX staked.");

            let expected_difference_stake =
                if current_stake_balance > validator.entry.stake_accounts_balance {
                    let expected_difference = (current_stake_balance
                        - validator.entry.stake_accounts_balance)
                        .expect("Does not overflow because current > entry.balance.");
                    // If the expected difference is less than some defined amount
                    // of Lamports, we don't bother withdrawing. We try to do this
                    // so we don't pay more for fees than the amount that we'll
                    // withdraw.
                    if expected_difference >= SolidoState::MINIMUM_WITHDRAW_AMOUNT {
                        expected_difference
                    } else {
                        Lamports(0)
                    }
                } else {
                    Lamports(0)
                };

            let mut removed_unstake = Lamports(0);

            for (_addr, unstake_account) in unstake_accounts.iter() {
                if unstake_account.balance.inactive != unstake_account.balance.total() {
                    break;
                }
                removed_unstake = (removed_unstake + unstake_account.balance.total())
                    .expect("Summing unstake accounts should not overflow.");
            }

            if expected_difference_stake > Lamports(0) || removed_unstake > Lamports(0) {
                // The balance of this validator is not up to date, try to update it.
                let mut stake_account_addrs: Vec<Pubkey> =
                    stake_accounts.iter().map(|(addr, _)| *addr).collect();
                // Try to also withdraw from unstake accounts
                let mut unstake_account_addrs: Vec<Pubkey> =
                    unstake_accounts.iter().map(|(addr, _)| *addr).collect();
                stake_account_addrs.append(&mut unstake_account_addrs);
                let instruction = lido::instruction::withdraw_inactive_stake(
                    &self.solido_program_id,
                    &lido::instruction::WithdrawInactiveStakeMeta {
                        lido: self.solido_address,
                        validator_vote_account: validator.pubkey,
                        stake_accounts: stake_account_addrs,
                        reserve: self.reserve_address,
                        stake_authority: self.get_stake_authority(),
                    },
                );
                let task = MaintenanceOutput::WithdrawInactiveStake {
                    validator_vote_account: validator.pubkey,
                    expected_difference_stake,
                    unstaked_amount: removed_unstake,
                };
                return Some((instruction, task));
            }
        }

        None
    }

    /// Check if any validator's vote account is eligible for fee collection, and if
    /// so, collects it.
    ///
    /// As validator's vote accounts accumulate rewards, at the beginning of
    /// every epoch, they should be collected and the fees they've generated
    /// should be spread to the Solido participants.
    pub fn try_collect_validator_fee(&self) -> Option<(Instruction, MaintenanceOutput)> {
        for (validator, vote_account_balance) in self
            .solido
            .validators
            .entries
            .iter()
            .zip(self.validator_vote_account_balances.iter())
        {
            // Need to collect some rewards if the balance is more than
            // the minimum predefined amount.
            if vote_account_balance > &SolidoState::MINIMUM_WITHDRAW_AMOUNT {
                let instruction = lido::instruction::collect_validator_fee(
                    &self.solido_program_id,
                    &lido::instruction::CollectValidatorFeeMeta {
                        lido: self.solido_address,
                        validator_vote_account: validator.pubkey,
                        mint_authority: self.get_mint_authority(),
                        st_sol_mint: self.solido.st_sol_mint,
                        treasury_st_sol_account: self.solido.fee_recipients.treasury_account,
                        developer_st_sol_account: self.solido.fee_recipients.developer_account,
                        reserve: self.reserve_address,
                        rewards_withdraw_authority: self.get_rewards_withdraw_authority(),
                    },
                );
                let task = MaintenanceOutput::CollectValidatorFee {
                    validator_vote_account: validator.pubkey,
                    fee_rewards: *vote_account_balance,
                };
                return Some((instruction, task));
            }
        }

        None
    }

    /// Checks if any of the validators has unclaimed fees in stSOL. If so,
    /// claims it on behalf of the validator.
    pub fn try_claim_validator_fee(&self) -> Option<(Instruction, MaintenanceOutput)> {
        for validator in self.solido.validators.entries.iter() {
            if validator.entry.fee_credit == StLamports(0) {
                continue;
            }

            let instruction = lido::instruction::claim_validator_fee(
                &self.solido_program_id,
                &lido::instruction::ClaimValidatorFeeMeta {
                    lido: self.solido_address,
                    st_sol_mint: self.solido.st_sol_mint,
                    mint_authority: self.get_mint_authority(),
                    validator_fee_st_sol_account: validator.entry.fee_address,
                },
            );
            let task = MaintenanceOutput::ClaimValidatorFee {
                validator_vote_account: validator.pubkey,
                fee_rewards: validator.entry.fee_credit,
            };

            return Some((instruction, task));
        }

        None
    }

    /// Write metrics about the current Solido instance in Prometheus format.
    pub fn write_prometheus<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        use crate::prometheus::{
            write_metric, write_solido_metrics_as_prometheus, Metric, MetricFamily,
        };

        write_metric(
            out,
            &MetricFamily {
                name: "solido_solana_block_height",
                help: "Solana slot that we read the Solido details from.",
                type_: "gauge",
                metrics: vec![Metric::new(self.clock.slot).at(self.produced_at)],
            },
        )?;

        // Include the maintainer balance, so maintainers can alert on it getting too low.
        write_metric(
            out,
            &MetricFamily {
                name: "solido_maintainer_balance_sol",
                help: "Balance of the maintainer account, in SOL.",
                type_: "gauge",
                metrics: vec![Metric::new_sol(Lamports(self.maintainer_account.lamports))
                    .at(self.produced_at)
                    // Include the maintainer address, to prevent any confusion
                    // about which account this is monitoring.
                    .with_label("maintainer_address", self.maintainer_address.to_string())],
            },
        )?;

        // Gather the different components that make up Solido's SOL balance.
        let mut balance_sol_metrics = vec![Metric::new_sol(self.get_effective_reserve())
            .at(self.produced_at)
            .with_label("status", "reserve".to_string())];

        // Track if there are any unclaimed (and therefore unminted) validation
        // fees.
        let mut unclaimed_fees = StLamports(0);

        for (validator, stake_accounts) in self
            .solido
            .validators
            .entries
            .iter()
            .zip(self.validator_stake_accounts.iter())
        {
            let stake_balance: StakeBalance = stake_accounts
                .iter()
                .map(|(_addr, stake_account)| stake_account.balance)
                .sum();
            let metric = |amount: Lamports, status: &'static str| {
                Metric::new_sol(amount)
                    .at(self.produced_at)
                    .with_label("status", status.to_string())
                    .with_label("vote_account", validator.pubkey.to_string())
            };
            balance_sol_metrics.push(metric(stake_balance.inactive, "inactive"));
            balance_sol_metrics.push(metric(stake_balance.activating, "activating"));
            balance_sol_metrics.push(metric(stake_balance.active, "active"));
            balance_sol_metrics.push(metric(stake_balance.deactivating, "deactivating"));

            unclaimed_fees = (unclaimed_fees + validator.entry.fee_credit)
                .expect("There shouldn't be so many fees to cause stSOL overflow.");
        }

        write_metric(
            out,
            &MetricFamily {
                name: "solido_balance_sol",
                help: "Amount of SOL currently managed by Solido.",
                type_: "gauge",
                metrics: balance_sol_metrics,
            },
        )?;

        let st_sol_supply = StLamports(self.st_sol_mint.supply);

        write_metric(
            out,
            &MetricFamily {
                name: "solido_token_supply_st_sol",
                help: "Amount of stSOL that exists currently.",
                type_: "gauge",
                metrics: vec![
                    Metric::new_st_sol(st_sol_supply)
                        .at(self.produced_at)
                        .with_label("status", "minted".to_string()),
                    Metric::new_st_sol(unclaimed_fees)
                        .at(self.produced_at)
                        .with_label("status", "unclaimed_fee".to_string()),
                ],
            },
        )?;

        write_metric(
            out,
            &MetricFamily {
                name: "solido_exchange_rate_supply_st_sol",
                help: "Amount of stSOL that existed at the time of the last exchange rate update.",
                type_: "gauge",
                metrics: vec![Metric::new_st_sol(self.solido.exchange_rate.st_sol_supply)
                    .at(self.produced_at)],
            },
        )?;
        write_metric(
            out,
            &MetricFamily {
                name: "solido_exchange_rate_balance_sol",
                help: "Amount of SOL managed at the time of the last exchange rate update.",
                type_: "gauge",
                metrics: vec![
                    Metric::new_sol(self.solido.exchange_rate.sol_balance).at(self.produced_at)
                ],
            },
        )?;
        write_metric(
            out,
            &MetricFamily {
                name: "solido_exchange_rate_computed_epoch",
                help: "The epoch in which the exchange rate was last computed.",
                type_: "gauge",
                metrics: vec![
                    Metric::new(self.solido.exchange_rate.computed_in_epoch).at(self.produced_at)
                ],
            },
        )?;

        write_solido_metrics_as_prometheus(&self.solido.metrics, self.produced_at, out)?;

        Ok(())
    }
    fn get_stake_authority(&self) -> Pubkey {
        let (stake_authority, _bump_seed_authority) = lido::find_authority_program_address(
            &self.solido_program_id,
            &self.solido_address,
            STAKE_AUTHORITY,
        );
        stake_authority
    }

    fn get_rewards_withdraw_authority(&self) -> Pubkey {
        let (rewards_withdraw_authority, _bump_seed_authority) =
            lido::find_authority_program_address(
                &self.solido_program_id,
                &self.solido_address,
                REWARDS_WITHDRAW_AUTHORITY,
            );
        rewards_withdraw_authority
    }

    fn get_mint_authority(&self) -> Pubkey {
        let (mint_authority, _bump_seed_authority) = lido::find_authority_program_address(
            &self.solido_program_id,
            &self.solido_address,
            MINT_AUTHORITY,
        );
        mint_authority
    }
}

pub fn try_perform_maintenance(
    config: &mut SnapshotConfig,
    state: &SolidoState,
) -> Result<Option<MaintenanceOutput>> {
    // To prevent the maintenance transactions failing with mysterious errors
    // that are difficult to debug, before we do any maintenance, do a sanity
    // check to ensure that the maintainer has at least some SOL to pay the
    // transaction fees.
    let minimum_maintainer_balance = Lamports(100_000_000);
    if Lamports(state.maintainer_account.lamports) < minimum_maintainer_balance {
        return Err(MaintenanceError::new(format!(
            "Balance of the maintainer account {} is less than {}. \
            Please fund the maintainer account.",
            state.maintainer_address, minimum_maintainer_balance,
        ))
        .into());
    }

    // Try all of these operations one by one, and select the first one that
    // produces an instruction.
    let instruction_output: Option<(Instruction, MaintenanceOutput)> = None
        // Merging stake accounts goes before updating validator balance, to
        // ensure that the balance update needs to reference as few accounts
        // as possible.
        .or_else(|| state.try_merge_on_all_stakes())
        .or_else(|| state.try_update_exchange_rate())
        .or_else(|| state.try_unstake_from_inactive_validator())
        // Collecting validator fees goes after updating the exchange rate,
        // because it may be rejected if the exchange rate is outdated.
        .or_else(|| state.try_collect_validator_fee())
        // Same for updating the validator balance.
        .or_else(|| state.try_withdraw_inactive_stake())
        .or_else(|| state.try_stake_deposit())
        .or_else(|| state.try_claim_validator_fee());

    match instruction_output {
        Some((instruction, output)) => {
            // For maintenance operations, the maintainer is the only signer,
            // and that should be sufficient.
            config.sign_and_send_transaction(&[instruction], &[config.signer])?;
            Ok(Some(output))
        }
        None => Ok(None),
    }
}

/// Inspect the on-chain Solido state, and if there is maintenance that can be
/// performed, do so. Returns a description of the task performed, if any.
///
/// This takes only one step, there might be more work left to do after this
/// function returns. Call it in a loop until it returns `None`. (And then still
/// call it in a loop, because the on-chain state might change.)
pub fn run_perform_maintenance(
    config: &mut SnapshotConfig,
    opts: &PerformMaintenanceOpts,
) -> Result<Option<MaintenanceOutput>> {
    let state = SolidoState::new(config, opts.solido_program_id(), opts.solido_address())?;
    try_perform_maintenance(config, &state)
}

#[cfg(test)]
mod test {

    use super::*;

    /// Produce a new state with `default` Solido instance in it, and random pubkeys.
    fn new_empty_solido() -> SolidoState {
        let mut state = SolidoState {
            produced_at: SystemTime::UNIX_EPOCH,
            solido_program_id: Pubkey::new_unique(),
            solido_address: Pubkey::new_unique(),
            solido: Lido::default(),
            validator_stake_accounts: vec![],
            validator_unstake_accounts: vec![],
            validator_vote_account_balances: vec![],
            st_sol_mint: Mint::default(),
            reserve_address: Pubkey::new_unique(),
            reserve_account: Account::default(),
            rent: Rent::default(),
            clock: Clock::default(),
            maintainer_address: Pubkey::new_unique(),
            maintainer_account: Account::default(),
        };

        // The reserve should be rent-exempt.
        state.reserve_account.lamports = state.rent.minimum_balance(0);

        state
    }

    /// This is a regression test. In the past we checked for the minimum stake
    /// balance before capping it at the amount below target, which meant that
    /// if there was enough in the reserve, but the amount below target was less
    /// than that of a minimum stake account, we would still try to deposit it,
    /// which would fail. Later though we changed it to stake the minimum stake
    /// amount if possible, even if it leaves validators unbalanced.
    #[test]
    fn stake_deposit_does_not_stake_less_than_the_minimum() {
        let mut state = new_empty_solido();

        // Add a validators, without any stake accounts yet.
        state.solido.validators.maximum_entries = 1;
        state
            .solido
            .validators
            .add(Pubkey::new_unique(), Validator::new(Pubkey::new_unique()))
            .unwrap();
        state.validator_stake_accounts.push(vec![]);
        // Put some SOL in the reserve, but not enough to stake.
        state.reserve_account.lamports += MINIMUM_STAKE_ACCOUNT_BALANCE.0 - 1;

        assert_eq!(
            state.try_stake_deposit(),
            None,
            "Should not try to stake, this is not enough for a stake account.",
        );

        // If we add a bit more, then we can fund two stake accounts, and that
        // should be enough to trigger a StakeDeposit.
        state.reserve_account.lamports += 1;

        assert!(state.try_stake_deposit().is_some());
    }

    #[test]
    fn stake_deposit_splits_evenly_if_possible() {
        use std::ops::Add;

        let mut state = new_empty_solido();

        // Add two validators, both without any stake account yet.
        state.solido.validators.maximum_entries = 2;
        state
            .solido
            .validators
            .add(Pubkey::new_unique(), Validator::new(Pubkey::new_unique()))
            .unwrap();
        state
            .solido
            .validators
            .add(Pubkey::new_unique(), Validator::new(Pubkey::new_unique()))
            .unwrap();
        state.validator_stake_accounts = vec![vec![], vec![]];

        // Put enough SOL in the reserve that we can stake half of the deposit
        // with each of the validators, and still be above the minimum stake
        // balance.
        state.reserve_account.lamports += 4 * MINIMUM_STAKE_ACCOUNT_BALANCE.0;

        let stake_account_0 = state.solido.validators.entries[0].find_stake_account_address(
            &state.solido_program_id,
            &state.solido_address,
            0,
        );

        // The first attempt should stake with the first validator.
        assert_eq!(
            state.try_stake_deposit().unwrap().1,
            MaintenanceOutput::StakeDeposit {
                validator_vote_account: state.solido.validators.entries[0].pubkey,
                amount: (MINIMUM_STAKE_ACCOUNT_BALANCE * 2).unwrap(),
                stake_account: stake_account_0.0,
            }
        );

        let stake_account_1 = state.solido.validators.entries[1].find_stake_account_address(
            &state.solido_program_id,
            &state.solido_address,
            0,
        );

        // Pretend that the amount was actually staked.
        state.reserve_account.lamports -= 2 * MINIMUM_STAKE_ACCOUNT_BALANCE.0;
        let validator = &mut state.solido.validators.entries[0].entry;
        validator.stake_accounts_balance = validator
            .stake_accounts_balance
            .add((MINIMUM_STAKE_ACCOUNT_BALANCE * 2).unwrap())
            .unwrap();

        // The second attempt should stake with the second validator, and the amount
        // should be the same as before.
        assert_eq!(
            state.try_stake_deposit().unwrap().1,
            MaintenanceOutput::StakeDeposit {
                validator_vote_account: state.solido.validators.entries[1].pubkey,
                amount: (MINIMUM_STAKE_ACCOUNT_BALANCE * 2).unwrap(),
                stake_account: stake_account_1.0,
            }
        );
    }
}
