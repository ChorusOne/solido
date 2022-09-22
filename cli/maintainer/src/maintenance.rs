// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Entry point for maintenance operations, such as updating the pool balance.

use std::fmt;
use std::io;
use std::time::SystemTime;

use anker::{
    logic::get_one_st_sol_for_ust_price_from_pool,
    state::{POOL_PRICE_MAX_SAMPLE_AGE, POOL_PRICE_MIN_SAMPLE_DISTANCE},
    token::MicroUst,
};
use itertools::izip;

use serde::Serialize;
use solana_program::{
    clock::{Clock, Slot},
    epoch_schedule::EpochSchedule,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    stake_history::StakeHistory,
};
use solana_sdk::{
    account::{Account, ReadableAccount},
    fee_calculator::DEFAULT_TARGET_LAMPORTS_PER_SIGNATURE,
    instruction::Instruction,
    signer::{keypair::Keypair, Signer},
};
use solana_vote_program::vote_state::VoteState;
use solido_cli_common::{
    error::MaintenanceError, snapshot::SnapshotConfig, validator_info_utils::ValidatorInfo, Result,
};
use spl_token::state::Mint;

use lido::{
    account_map::PubkeyAndEntry,
    processor::StakeType,
    stake_account::StakeAccount,
    stake_account::{deserialize_stake_account, StakeBalance},
    state::{Lido, Validator},
    token::Lamports,
    token::Rational,
    token::StLamports,
    util::serialize_b58,
    MINIMUM_STAKE_ACCOUNT_BALANCE, MINT_AUTHORITY, STAKE_AUTHORITY,
};
use spl_token_swap::curve::calculator::{CurveCalculator, TradeDirection};

use crate::anker_state::AnkerState;
use crate::config::{PerformMaintenanceOpts, StakeTime};

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

        #[serde(rename = "unstake_withdrawn_to_reserve_lamports")]
        unstake_withdrawn_to_reserve: Lamports,
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

    UnstakeFromInactiveValidator(Unstake),
    RemoveValidator {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
    },
    DeactivateValidatorIfCommissionExceedsMax {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
    },
    UnstakeFromActiveValidator(Unstake),

    FetchPoolPrice {
        #[serde(rename = "st_sol_price_in_micro_ust")]
        expected_st_sol_price_in_ust: MicroUst,
    },

    SellRewards {
        #[serde(rename = "st_sol_amount_st_lamports")]
        st_sol_amount: StLamports,
    },
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct Unstake {
    #[serde(serialize_with = "serialize_b58")]
    validator_vote_account: Pubkey,
    #[serde(serialize_with = "serialize_b58")]
    from_stake_account: Pubkey,
    #[serde(serialize_with = "serialize_b58")]
    to_unstake_account: Pubkey,
    from_stake_seed: u64,
    to_unstake_seed: u64,
    amount: Lamports,
}

impl fmt::Display for Unstake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "  Validator vote account: {}",
            self.validator_vote_account
        )?;
        writeln!(
            f,
            "  Stake account:               {}, seed: {}",
            self.from_stake_account, self.from_stake_seed
        )?;
        writeln!(
            f,
            "  Unstake account:             {}, seed: {}",
            self.to_unstake_account, self.to_unstake_seed
        )?;
        writeln!(f, "  Amount:              {}", self.amount)?;
        Ok(())
    }
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
                unstake_withdrawn_to_reserve,
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
                writeln!(
                    f,
                    "  Amount withdrawn from unstake: {}",
                    unstake_withdrawn_to_reserve
                )?;
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
            MaintenanceOutput::UnstakeFromInactiveValidator(unstake) => {
                writeln!(f, "Unstake from inactive validator\n{}", unstake)?;
            }
            MaintenanceOutput::UnstakeFromActiveValidator(unstake) => {
                writeln!(f, "Unstake from active validator\n{}", unstake)?;
            }
            MaintenanceOutput::RemoveValidator {
                validator_vote_account,
            } => {
                writeln!(f, "Remove validator")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
            }
            MaintenanceOutput::DeactivateValidatorIfCommissionExceedsMax {
                validator_vote_account,
            } => {
                writeln!(f, "Check max commission violation.")?;
                writeln!(
                    f,
                    "Deactivate validator that charges more commission than we allow."
                )?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
            }
            MaintenanceOutput::SellRewards { st_sol_amount } => {
                writeln!(f, "Sell stSOL rewards")?;
                writeln!(f, "  Amount:               {}", st_sol_amount)?;
            }
            MaintenanceOutput::FetchPoolPrice {
                expected_st_sol_price_in_ust,
            } => {
                writeln!(f, "Fetch Pool Price")?;
                writeln!(
                    f,
                    "  Expected amount per stSOL: {}",
                    expected_st_sol_price_in_ust
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct MaintenanceInstruction {
    instruction: Instruction,
    output: MaintenanceOutput,
    additional_signers: Vec<Keypair>,
}

impl MaintenanceInstruction {
    pub fn new(instruction: Instruction, output: MaintenanceOutput) -> MaintenanceInstruction {
        MaintenanceInstruction {
            instruction,
            output,
            additional_signers: Vec::new(),
        }
    }
}

/// A snapshot of on-chain accounts relevant to Solido.
pub struct SolidoState {
    /// The label for the time at which we finished querying the Solido state.
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
    ///
    /// This field holds the current datetime indicated by the OS, which is
    /// useful for communicating that time externally (to Prometheus), but which
    /// is not suitable for measuring durations.
    pub produced_at: SystemTime,

    pub solido_program_id: Pubkey,
    pub solido_address: Pubkey,
    pub solido: Lido,

    /// Anker parameters
    pub anker_state: Option<AnkerState>,

    /// For each validator, in the same order as in `solido.validators`, holds
    /// the stake balance of the derived stake accounts from the begin seed until
    /// end seed.
    pub validator_stake_accounts: Vec<Vec<(Pubkey, StakeAccount)>>,

    /// Similar to the stake accounts, holds the unstaked balance of the derived
    /// unstake accounts from the begin seed until end seed.
    pub validator_unstake_accounts: Vec<Vec<(Pubkey, StakeAccount)>>,

    /// For each validator, in the same order as in `solido.validators`, holds
    /// the number of Lamports of the validator's vote account.
    pub validator_vote_account_balances: Vec<Lamports>,

    /// For each validator, in the same order as in `solido.validators`, holds
    /// the deserialized vote account.
    pub validator_vote_accounts: Vec<VoteState>,

    /// For each validator, in the same order as in `solido.validators`, holds
    /// the balance of the validator's identity account (which pays for the
    /// votes).
    pub validator_identity_account_balances: Vec<Lamports>,

    /// For each validator, in the same order as in `solido.validators`, holds
    /// the validator info (name and Keybase username).
    pub validator_infos: Vec<ValidatorInfo>,

    /// For each maintainer, in the same order as in `solido.maintainers`, holds
    /// the number of Lamports in the maintainer's account.
    pub maintainer_balances: Vec<Lamports>,

    /// SPL token mint for stSOL, to know the current supply.
    pub st_sol_mint: Mint,

    pub reserve_address: Pubkey,
    pub reserve_account: Account,
    pub rent: Rent,
    pub clock: Clock,
    pub epoch_schedule: EpochSchedule,
    pub stake_history: StakeHistory,

    /// Public key of the maintainer executing the maintenance.
    /// Must be a member of `solido.maintainers`.
    pub maintainer_address: Pubkey,

    /// When to unstake/stake.
    /// If set to StakeTime::Anytime, stake and unstake instructions are issued
    /// whenever possible. If set to StakeTime::OnlyNearEpochEnd the
    /// instructions are issued only close to the end of epoch.
    pub stake_time: StakeTime,
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
        let (addr, _bump_seed) = validator.find_stake_account_address(
            solido_program_id,
            solido_address,
            seed,
            stake_type,
        );
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

fn get_account_balance_except_rent(rent: &Rent, account: &Account) -> Lamports {
    let rent_amount = rent.minimum_balance(account.data().len());
    (Lamports(account.lamports()) - Lamports(rent_amount))
        .expect("Shouldn't happen. The account balance should be at least its rent-exempt balance.")
}

/// Given a public validator name, return one suitable for use in metrics.
fn sanitize_validator_name(name: &str) -> String {
    // Lido policy is that validator names should start with "Lido / ", so that
    // adds no information, strip it here to leave more space for graphs in
    // dashboards, and not waste so much space on the redundant part of the name.
    match name.strip_prefix("Lido / ") {
        // Negated range.contains syntax only makes things more cryptic below.
        #[allow(clippy::manual_range_contains)]
        // I don't want distracting emojis in my Grafana dashboards.
        Some(suffix) => suffix
            .chars()
            .filter(|&ch|
                // Remove code points in the Supplementary Multilingual Plane and
                // beyond. This strips most emojis and dingbats while leaving
                // letters and punctuation of all contemporary languages.
                ch < '\u{10000}'
                // Remove variation selectors. These can be used to make code
                // points that are traditionally not emoji, render as emoji.
                && (ch < '\u{fe00}' || ch > '\u{fe0f}')
                // Remove code points from the Miscellaneous Symbols block,
                // which contains dingbats that predate emoji, but nowadays
                // are usually rendered with colored emoji font instead of an
                // outline glyph.
                && (ch < '\u{2600}' || ch > '\u{26ff}'))
            .collect::<String>()
            .trim()
            .to_string(),
        None => format!("INVALID: {}", name),
    }
}

impl SolidoState {
    // Set the minimum withdraw from stake accounts and validator's vote
    // accounts, the cost of validating signatures seems to dominate the
    // transaction cost.
    const MINIMUM_WITHDRAW_AMOUNT: Lamports = Lamports(DEFAULT_TARGET_LAMPORTS_PER_SIGNATURE * 100);

    // Threshold that will trigger unstake on validators.
    const UNBALANCE_THRESHOLD: Rational = Rational {
        numerator: 1,
        denominator: 10,
    };

    /// Threshold for when to consider the end of an epoch.
    /// E.g. if set to 19/20, the end of epoch would be considered if the system
    /// is past 95% of the epoch's time.
    const END_OF_EPOCH_THRESHOLD: Rational = Rational {
        numerator: 19,
        denominator: 20,
    };

    /// Read the state from the on-chain data.
    pub fn new(
        config: &mut SnapshotConfig,
        solido_program_id: &Pubkey,
        anker_program_id: &Pubkey,
        solido_address: &Pubkey,
        stake_time: StakeTime,
    ) -> Result<SolidoState> {
        let solido = config.client.get_solido(solido_address)?;

        let reserve_address = solido.get_reserve_account(solido_program_id, solido_address)?;
        let reserve_account = config.client.get_account(&reserve_address)?;

        let st_sol_mint_account = config.client.get_account(&solido.st_sol_mint)?;
        let st_sol_mint = Mint::unpack(&st_sol_mint_account.data)?;

        let rent = config.client.get_rent()?;
        let clock = config.client.get_clock()?;
        let epoch_schedule = config.client.get_epoch_schedule()?;
        let stake_history = config.client.get_stake_history()?;

        let mut validator_stake_accounts = Vec::new();
        let mut validator_unstake_accounts = Vec::new();
        let mut validator_vote_account_balances = Vec::new();
        let mut validator_identity_account_balances = Vec::new();
        let mut validator_vote_accounts = Vec::new();
        let mut validator_infos = Vec::new();
        for validator in solido.validators.entries.iter() {
            let vote_account = config.client.get_account(&validator.pubkey)?;
            let vote_state = config.client.get_vote_account(&validator.pubkey)?;
            let validator_info = config.client.get_validator_info(&vote_state.node_pubkey)?;
            let identity_account = config.client.get_account(&vote_state.node_pubkey)?;
            validator_vote_accounts.push(vote_state);
            validator_vote_account_balances
                .push(get_account_balance_except_rent(&rent, vote_account));
            validator_identity_account_balances
                .push(get_account_balance_except_rent(&rent, identity_account));
            validator_infos.push(validator_info);

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

        let mut maintainer_balances = Vec::new();
        for maintainer in solido.maintainers.entries.iter() {
            maintainer_balances.push(Lamports(
                config.client.get_account(&maintainer.pubkey)?.lamports,
            ));
        }

        // The entity executing the maintenance transactions, is the maintainer.
        // We don't verify here if it is part of the maintainer set, the on-chain
        // program does that anyway.
        let maintainer_address = config.signer.pubkey();

        let anker_state = if anker_program_id == &Pubkey::default() {
            None
        } else {
            let (anker_address, _bump_seed) =
                anker::find_instance_address(anker_program_id, solido_address);
            Some(AnkerState::new(
                config,
                anker_program_id,
                &anker_address,
                &solido,
            )?)
        };

        Ok(SolidoState {
            produced_at: SystemTime::now(),
            solido_program_id: *solido_program_id,
            solido_address: *solido_address,
            solido,
            anker_state,
            validator_stake_accounts,
            validator_unstake_accounts,
            validator_vote_account_balances,
            validator_vote_accounts,
            validator_identity_account_balances,
            validator_infos,
            maintainer_balances,
            reserve_address,
            reserve_account: reserve_account.clone(),
            st_sol_mint,
            rent,
            clock,
            epoch_schedule,
            stake_history,
            maintainer_address,
            stake_time,
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
    pub fn try_stake_deposit(&self) -> Option<MaintenanceInstruction> {
        self.confirm_should_stake_unstake_in_current_slot()?;
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
            lido::balance::get_minimum_stake_validator_index_amount(
                &self.solido.validators,
                &targets[..],
            );

        let validator = &self.solido.validators.entries[validator_index];

        let (stake_account_end, _bump_seed_end) = validator.find_stake_account_address(
            &self.solido_program_id,
            &self.solido_address,
            validator.entry.stake_seeds.end,
            StakeType::Stake,
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
        Some(MaintenanceInstruction::new(instruction, task))
    }

    /// Returns a tuple with the unstake account address and the instruction to
    /// unstake `amount` from it.
    pub fn get_unstake_instruction(
        &self,
        validator: &PubkeyAndEntry<Validator>,
        stake_account: &(Pubkey, StakeAccount),
        amount: Lamports,
    ) -> (Pubkey, Instruction) {
        let (validator_unstake_account, _) = validator.find_stake_account_address(
            &self.solido_program_id,
            &self.solido_address,
            validator.entry.unstake_seeds.end,
            StakeType::Unstake,
        );
        let (stake_account_address, _) = stake_account;
        (
            validator_unstake_account,
            lido::instruction::unstake(
                &self.solido_program_id,
                &lido::instruction::UnstakeAccountsMeta {
                    lido: self.solido_address,
                    maintainer: self.maintainer_address,
                    validator_vote_account: validator.pubkey,
                    source_stake_account: *stake_account_address,
                    destination_unstake_account: validator_unstake_account,
                    stake_authority: self.get_stake_authority(),
                },
                amount,
            ),
        )
    }

    /// If there is a validator being deactivated, try to unstake its funds.
    pub fn try_unstake_from_inactive_validator(&self) -> Option<MaintenanceInstruction> {
        for (validator, stake_accounts) in self
            .solido
            .validators
            .entries
            .iter()
            .zip(self.validator_stake_accounts.iter())
        {
            // We are only interested in unstaking from inactive validators that
            // have stake accounts.
            if validator.entry.active {
                continue;
            }
            // Validator already has 3 unstake accounts.
            if validator.entry.unstake_seeds.end - validator.entry.unstake_seeds.begin
                >= lido::MAXIMUM_UNSTAKE_ACCOUNTS
            {
                continue;
            }
            // No stake account to unstake from.
            if stake_accounts.first().is_none() {
                continue;
            }
            let (stake_account_address, stake_account_balance) = stake_accounts[0];
            let (unstake_account, unstake_instruction) = self.get_unstake_instruction(
                validator,
                &stake_accounts[0],
                stake_account_balance.balance.total(),
            );
            let task = MaintenanceOutput::UnstakeFromInactiveValidator(Unstake {
                validator_vote_account: validator.pubkey,
                from_stake_account: stake_account_address,
                to_unstake_account: unstake_account,
                from_stake_seed: validator.entry.stake_seeds.begin,
                to_unstake_seed: validator.entry.unstake_seeds.end,
                amount: stake_account_balance.balance.total(),
            });

            return Some(MaintenanceInstruction::new(unstake_instruction, task));
        }
        None
    }

    /// If there is a validator which exceeded commission limit, try to deactivate it.
    pub fn try_deactivate_validator_if_commission_exceeds_max(
        &self,
    ) -> Option<MaintenanceInstruction> {
        for (validator, vote_state) in self
            .solido
            .validators
            .entries
            .iter()
            .zip(self.validator_vote_accounts.iter())
        {
            // We are only interested in validators that violate commission limit
            if !validator.entry.active
                || vote_state.commission <= self.solido.max_commission_percentage
            {
                continue;
            }

            let task = MaintenanceOutput::DeactivateValidatorIfCommissionExceedsMax {
                validator_vote_account: validator.pubkey,
            };

            let instruction = lido::instruction::deactivate_validator_if_commission_exceeds_max(
                &self.solido_program_id,
                &lido::instruction::DeactivateValidatorIfCommissionExceedsMaxMeta {
                    lido: self.solido_address,
                    validator_vote_account_to_deactivate: validator.pubkey,
                },
            );
            return Some(MaintenanceInstruction::new(instruction, task));
        }
        None
    }

    /// If there is a validator ready for removal, try to remove it.
    pub fn try_remove_validator(&self) -> Option<MaintenanceInstruction> {
        for validator in &self.solido.validators.entries {
            // We are only interested in validators that can be removed.
            if validator.entry.check_can_be_removed().is_err() {
                continue;
            }
            let task = MaintenanceOutput::RemoveValidator {
                validator_vote_account: validator.pubkey,
            };

            let instruction = lido::instruction::remove_validator(
                &self.solido_program_id,
                &lido::instruction::RemoveValidatorMeta {
                    lido: self.solido_address,
                    validator_vote_account_to_remove: validator.pubkey,
                },
            );
            return Some(MaintenanceInstruction::new(instruction, task));
        }
        None
    }

    /// Get the amount of rewards we can sell in Anker.
    fn get_anker_rewards(&self) -> Option<StLamports> {
        let anker_state = self.anker_state.as_ref()?;
        let reserve_st_sol = anker_state.st_sol_reserve_balance;
        let st_sol_amount = self
            .solido
            .exchange_rate
            .exchange_sol(Lamports(anker_state.b_sol_total_supply_amount.0))
            .expect("It will not overflow because we always have less than the total amount of minted Sol.");

        (reserve_st_sol - st_sol_amount).ok()
    }

    /// Try to sell the extra stSOL rewards for UST tokens or
    /// to update the historical pool price exchange rate to protect us
    /// against sandwiching attacks.
    pub fn try_sell_anker_rewards(&self) -> Option<MaintenanceInstruction> {
        let anker_state = self.anker_state.as_ref()?;

        let rewards = self.get_anker_rewards()?;
        let min_rewards_to_sell = self
            .solido
            .exchange_rate
            .exchange_sol(Self::MINIMUM_WITHDRAW_AMOUNT)
            .expect("The price of a signature should be small enough that it doesn't overflow.");
        // We should not call the instruction if the rewards are 0, or if the rewards are so small
        // that the transaction cost is a significant portion of the rewards.
        if rewards < min_rewards_to_sell {
            return None;
        }

        // Fees as in the `spl_token_swap` `SwapCurve::swap` calculation.
        let trade_fee = anker_state.pool_fees.trading_fee(rewards.0 as u128)?;
        let owner_fee = anker_state.pool_fees.owner_trading_fee(rewards.0 as u128)?;

        let total_fees = trade_fee.checked_add(owner_fee)?;
        let rewards_minus_fees = (rewards.0 as u128).checked_sub(total_fees)?;

        let expected_proceeds = anker_state
            .constant_product_calculator
            .swap_without_fees(
                rewards_minus_fees,
                anker_state.pool_st_sol_balance.0 as u128,
                anker_state.pool_ust_balance.0 as u128,
                TradeDirection::AtoB,
            )?
            .destination_amount_swapped;
        let expected_proceeds = MicroUst(expected_proceeds as u64);

        // We want at least 0.01 UST out if we are going to do the swap at all.
        let min_proceeds = MicroUst(10_000);
        if expected_proceeds < min_proceeds {
            return None;
        }

        // Check if we can sell the rewards with the preset slippage tolerance.
        // Note that this might change when the instruction gets included in the block.
        let minimum_ust_amount_for_rewards = anker_state
            .anker
            .historical_st_sol_prices
            .minimum_ust_swap_amount(rewards, anker_state.anker.sell_rewards_min_out_bps)
            .ok()?;
        if expected_proceeds < minimum_ust_amount_for_rewards {
            return None;
        }

        let oldest_price_sample = anker_state.anker.historical_st_sol_prices.first();
        let slots_elapsed_since_oldest_sample =
            self.clock.slot.saturating_sub(oldest_price_sample.slot);

        let youngest_sample = anker_state.anker.historical_st_sol_prices.last();
        let slots_elapsed_since_youngest_sample =
            self.clock.slot.saturating_sub(youngest_sample.slot);

        // If the youngest sample is too recent, we are not yet allowed to sell
        // rewards or update the price.
        if slots_elapsed_since_youngest_sample < POOL_PRICE_MIN_SAMPLE_DISTANCE {
            return None;
        }

        // Time to update the historical price
        if slots_elapsed_since_oldest_sample > POOL_PRICE_MAX_SAMPLE_AGE
            || oldest_price_sample.slot == 0
        {
            let expected_st_sol_price_in_ust = get_one_st_sol_for_ust_price_from_pool(
                &anker_state.constant_product_calculator,
                &anker_state.pool_st_sol_account,
                &anker_state.pool_ust_account,
                anker_state.pool_st_sol_balance,
                anker_state.pool_ust_balance,
            )
            .ok()?;
            Some(MaintenanceInstruction::new(
                anker_state.get_fetch_pool_price_instruction(self.solido_address),
                MaintenanceOutput::FetchPoolPrice {
                    expected_st_sol_price_in_ust,
                },
            ))
        } else {
            Some(MaintenanceInstruction::new(
                anker_state
                    .get_sell_rewards_instruction(self.solido_address, self.solido.st_sol_mint),
                MaintenanceOutput::SellRewards {
                    st_sol_amount: anker_state.st_sol_reserve_balance,
                },
            ))
        }
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
            StakeType::Stake,
        );
        // Stake Account created by this transaction.
        let (to_stake, _bump_seed_end) = validator.find_stake_account_address(
            &self.solido_program_id,
            &self.solido_address,
            to_seed,
            StakeType::Stake,
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
    pub fn try_merge_on_all_stakes(&self) -> Option<MaintenanceInstruction> {
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
                    return Some(MaintenanceInstruction::new(instruction, task));
                }
            }
        }
        None
    }

    /// If a new epoch started, and we haven't updated the exchange rate yet, do so.
    pub fn try_update_exchange_rate(&self) -> Option<MaintenanceInstruction> {
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

        Some(MaintenanceInstruction::new(instruction, task))
    }

    /// Check if any validator's balance is outdated, and if so, update it.
    ///
    /// Merging stakes generates inactive stake that could be withdrawn with this transaction,
    /// or if some joker donates to one of the stake accounts we can use the same function
    /// to claim these rewards back to the reserve account so they can be re-staked.
    pub fn try_update_stake_account_balance(&self) -> Option<MaintenanceInstruction> {
        for (validator, stake_accounts, unstake_accounts) in izip!(
            self.solido.validators.entries.iter(),
            self.validator_stake_accounts.iter(),
            self.validator_unstake_accounts.iter()
        ) {
            let current_stake_balance = stake_accounts
                .iter()
                .map(|(_addr, detail)| detail.balance.total())
                .sum::<lido::token::Result<Lamports>>()
                .expect("If this overflows, there would be more than u64::MAX staked.");

            let expected_difference_stake =
                if current_stake_balance > validator.entry.effective_stake_balance() {
                    (current_stake_balance - validator.entry.effective_stake_balance())
                        .expect("Does not overflow because current > entry.balance.")
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

            // If the expected difference is less than some defined amount
            // of Lamports, we don't bother withdrawing. We try to do this
            // so we don't pay more for fees than the amount that we'll
            // withdraw. Or if we have stake to remove from unstake accounts.
            // If we're dealing with an inactive validator, we try to withraw
            // all inactive stake.
            let minimum_withdraw_amount = if validator.entry.active {
                SolidoState::MINIMUM_WITHDRAW_AMOUNT
            } else {
                Lamports(0)
            };

            if expected_difference_stake > minimum_withdraw_amount || removed_unstake > Lamports(0)
            {
                // The balance of this validator is not up to date, try to update it.
                let mut stake_account_addrs = Vec::new();
                stake_account_addrs.extend(stake_accounts.iter().map(|(addr, _)| *addr));
                stake_account_addrs.extend(unstake_accounts.iter().map(|(addr, _)| *addr));
                let instruction = lido::instruction::update_stake_account_balance(
                    &self.solido_program_id,
                    &lido::instruction::UpdateStakeAccountBalanceMeta {
                        lido: self.solido_address,
                        validator_vote_account: validator.pubkey,
                        stake_accounts: stake_account_addrs,
                        reserve: self.reserve_address,
                        stake_authority: self.get_stake_authority(),
                        mint_authority: self.get_mint_authority(),
                        st_sol_mint: self.solido.st_sol_mint,
                        treasury_st_sol_account: self.solido.fee_recipients.treasury_account,
                        developer_st_sol_account: self.solido.fee_recipients.developer_account,
                    },
                );
                let task = MaintenanceOutput::WithdrawInactiveStake {
                    validator_vote_account: validator.pubkey,
                    expected_difference_stake,
                    unstake_withdrawn_to_reserve: removed_unstake,
                };
                return Some(MaintenanceInstruction::new(instruction, task));
            }
        }

        None
    }

    /// Unstake from active validators in order to rebalance validators.
    pub fn try_unstake_from_active_validators(&self) -> Option<MaintenanceInstruction> {
        self.confirm_should_stake_unstake_in_current_slot()?;
        // Return None if there's no active validator to unstake from.
        self.solido.validators.iter_active().next()?;

        // Get the target for each validator. Undelegated Lamports can be
        // sent when staking with validators.
        let targets = lido::balance::get_target_balance(
            self.get_effective_reserve(),
            &self.solido.validators,
        )
        .expect("Failed to compute target balance.");

        let (validator_index, unstake_amount) = lido::balance::get_unstake_validator_index(
            &self.solido.validators,
            &targets,
            SolidoState::UNBALANCE_THRESHOLD,
        )?;
        let validator = &self.solido.validators.entries[validator_index];
        let stake_account = &self.validator_stake_accounts[validator_index][0];

        let maximum_unstake = (stake_account.1.balance.total() - MINIMUM_STAKE_ACCOUNT_BALANCE)
            .expect("Stake account should always have the minimum amount.");
        // Get the maximum that can be unstaked from the stake account.  The
        // minimum amongst the value to be unstaked, and the maximum that can be
        // unstaked from the validator.
        let amount = unstake_amount.min(maximum_unstake);

        // If the amount unstaked would leave a stake account with less than
        // `MINIMUM_STAKE_ACCOUNT_BALANCE` we shouldn't unstake it.
        if amount < MINIMUM_STAKE_ACCOUNT_BALANCE {
            return None;
        }

        let (unstake_account, instruction) =
            self.get_unstake_instruction(validator, stake_account, amount);
        let task = MaintenanceOutput::UnstakeFromActiveValidator(Unstake {
            validator_vote_account: validator.pubkey,
            from_stake_account: stake_account.0,
            to_unstake_account: unstake_account,
            from_stake_seed: validator.entry.stake_seeds.begin,
            to_unstake_seed: validator.entry.unstake_seeds.end,
            amount,
        });
        Some(MaintenanceInstruction::new(instruction, task))
    }

    /// Write metrics about the current Solido instance in Prometheus format.
    pub fn write_prometheus<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        use solido_cli_common::prometheus::{
            write_anker_metrics_as_prometheus, write_metric, write_solido_metrics_as_prometheus,
            Metric, MetricFamily,
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
        write_metric(
            out,
            &MetricFamily {
                name: "solido_solana_epoch",
                help: "Solana epoch that the slot at solido_solana_block_height falls in.",
                type_: "gauge",
                metrics: vec![Metric::new(self.clock.epoch).at(self.produced_at)],
            },
        )?;
        write_metric(
            out,
            &MetricFamily {
                name: "solido_solana_epoch_start_slot",
                help: "Slot number of the first slot of the current Solana epoch.",
                type_: "gauge",
                metrics: vec![Metric::new(
                    self.epoch_schedule
                        .get_first_slot_in_epoch(self.clock.epoch),
                )
                .at(self.produced_at)],
            },
        )?;
        write_metric(
            out,
            &MetricFamily {
                name: "solido_solana_slots_per_epoch",
                help: "Number of slots in the current Solana epoch.",
                type_: "gauge",
                metrics: vec![Metric::new(self.epoch_schedule.slots_per_epoch).at(self.produced_at)],
            },
        )?;

        // https://docs.solana.com/developing/runtime-facilities/sysvars#stakehistory says that the
        // stake history sysvar is updated at the start of every epoch. If you try to query it for
        // the current epoch, it returns None. So unfortunately we can only get the state of the
        // previous epoch. If the total amount staked changes slowly, we can still used it to
        // roughly estimate Solido's share of the stake.
        if let Some(entry) = self.stake_history.get(self.clock.epoch - 1) {
            write_metric(
                out,
                &MetricFamily {
                    name: "solido_solana_stake_sol",
                    help: "Amount of SOL staked for the *previous* epoch, across the entire Solana network.",
                    type_: "gauge",
                    metrics: vec![
                        Metric::new_sol(Lamports(entry.activating))
                            .at(self.produced_at)
                            .with_label("status", "activating".to_string()),
                        Metric::new_sol(Lamports(entry.effective))
                            .at(self.produced_at)
                            .with_label("status", "active".to_string()),
                        Metric::new_sol(Lamports(entry.deactivating))
                            .at(self.produced_at)
                            .with_label("status", "deactivating".to_string()),
                    ],
                },
            )?;
        }

        // Include the maintainer balance, so maintainers can alert on it getting too low.
        write_metric(
            out,
            &MetricFamily {
                name: "solido_maintainer_balance_sol",
                help: "Balance of the maintainer accounts, in SOL.",
                type_: "gauge",
                metrics: self
                    .solido
                    .maintainers
                    .entries
                    .iter()
                    .zip(&self.maintainer_balances)
                    .map(|(maintainer, balance)| {
                        Metric::new_sol(*balance)
                            .at(self.produced_at)
                            .with_label("maintainer_address", maintainer.pubkey.to_string())
                    })
                    .collect(),
            },
        )?;

        // Gather the different components that make up Solido's SOL balance.
        let mut balance_sol_metrics = vec![Metric::new_sol(self.get_effective_reserve())
            .at(self.produced_at)
            .with_label("status", "reserve".to_string())];

        let mut last_voted_slot_metrics = Vec::new();
        let mut last_voted_timestamp_metrics = Vec::new();
        let mut identity_account_balance_metrics = Vec::new();
        let mut vote_credits_metrics = Vec::new();

        for (
            (
                (((validator, stake_accounts), unstake_accounts), vote_account),
                identity_account_balance,
            ),
            info,
        ) in self
            .solido
            .validators
            .entries
            .iter()
            .zip(self.validator_stake_accounts.iter())
            .zip(self.validator_unstake_accounts.iter())
            .zip(self.validator_vote_accounts.iter())
            .zip(self.validator_identity_account_balances.iter())
            .zip(self.validator_infos.iter())
        {
            // Helper struct to add the right labels to our metrics. Ideally we
            // would do this in a closure, but it's not possible to add the required
            // lifetime annotations that way, so we manually define the closure
            // struct here.
            struct MetricAnnotator {
                produced_at: SystemTime,
                vote_account: String,
                name: String,
                keybase_username: String,
            }

            impl MetricAnnotator {
                fn add_labels<'a>(&self, metric: Metric<'a>) -> Metric<'a> {
                    metric
                        .at(self.produced_at)
                        .with_label("vote_account", self.vote_account.clone())
                        .with_label("validator_name", self.name.clone())
                        .with_label("validator_keybase", self.keybase_username.clone())
                }
            }

            let annotator = MetricAnnotator {
                produced_at: self.produced_at,
                vote_account: validator.pubkey.to_string(),
                name: sanitize_validator_name(&info.name),
                keybase_username: info
                    .keybase_username
                    .as_ref()
                    .expect("All Lido validators should have a Keybase username set.")
                    .to_string(),
            };

            let stake_balance: StakeBalance = stake_accounts
                .iter()
                .chain(unstake_accounts.iter())
                .map(|(_addr, stake_account)| stake_account.balance)
                .sum();

            let metric = |amount: Lamports, status: &'static str| {
                annotator
                    .add_labels(Metric::new_sol(amount))
                    .with_label("status", status.to_string())
            };
            balance_sol_metrics.push(metric(stake_balance.inactive, "inactive"));
            balance_sol_metrics.push(metric(stake_balance.activating, "activating"));
            balance_sol_metrics.push(metric(stake_balance.active, "active"));
            balance_sol_metrics.push(metric(stake_balance.deactivating, "deactivating"));

            last_voted_slot_metrics
                .push(annotator.add_labels(Metric::new(vote_account.last_timestamp.slot)));
            last_voted_timestamp_metrics.push(
                annotator.add_labels(Metric::new(vote_account.last_timestamp.timestamp as u64)),
            );
            identity_account_balance_metrics
                .push(annotator.add_labels(Metric::new_sol(*identity_account_balance)));
            vote_credits_metrics.push(annotator.add_labels(Metric::new(vote_account.credits())));
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

        write_metric(
            out,
            &MetricFamily {
                name: "solido_validator_last_voted_slot",
                help: "The slot that the validator last voted on.",
                type_: "gauge",
                metrics: last_voted_slot_metrics,
            },
        )?;

        write_metric(
            out,
            &MetricFamily {
                name: "solido_validator_last_voted_timestamp",
                help: "The Unix timestamp (in seconds) that the validator included \
                    in its last vote. Note that this is provided by the validator itself, \
                    it may not be accurate.",
                type_: "gauge",
                metrics: last_voted_timestamp_metrics,
            },
        )?;

        write_metric(
            out,
            &MetricFamily {
                name: "solido_validator_identity_account_balance_sol",
                help: "Balance of the validator's identity account (that pays for votes) minus rent-exempt amount.",
                type_: "gauge",
                metrics: identity_account_balance_metrics,
            },
        )?;

        write_metric(
            out,
            &MetricFamily {
                name: "solido_validator_vote_credits_total",
                help: "Vote credits in the validator's vote account. \
                       On-chain this value can only increase, but decreases in the observed value can \
                       happen due to reorgs.",
                type_: "gauge",
                metrics: vote_credits_metrics,
            },
        )?;

        let st_sol_supply = StLamports(self.st_sol_mint.supply);

        write_metric(
            out,
            &MetricFamily {
                name: "solido_token_supply_st_sol",
                help: "Amount of stSOL that exists currently.",
                type_: "gauge",
                metrics: vec![Metric::new_st_sol(st_sol_supply)
                    .at(self.produced_at)
                    .with_label("status", "minted".to_string())],
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
        if let Some(anker_state) = &self.anker_state {
            write_metric(
                out,
                &MetricFamily {
                    name: "anker_token_supply_b_sol",
                    help: "Amount of bSOL that exists currently.",
                    type_: "gauge",
                    metrics: vec![Metric::new_b_sol(anker_state.b_sol_total_supply_amount)
                        .at(self.produced_at)],
                },
            )?;

            write_metric(
                out,
                &MetricFamily {
                    name: "anker_reserve_st_sol",
                    help: "Amount of stSOL in reserve.",
                    type_: "gauge",
                    metrics: vec![
                        Metric::new_st_sol(anker_state.st_sol_reserve_balance).at(self.produced_at)
                    ],
                },
            )?;

            write_metric(
                out,
                &MetricFamily {
                    name: "anker_reserve_ust",
                    help: "Amount of UST in reserve.",
                    type_: "gauge",
                    metrics: vec![
                        Metric::new_ust(anker_state.ust_reserve_balance).at(self.produced_at)
                    ],
                },
            )?;

            write_anker_metrics_as_prometheus(&anker_state.anker.metrics, self.produced_at, out)?;
        }

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

    fn get_mint_authority(&self) -> Pubkey {
        let (mint_authority, _bump_seed_authority) = lido::find_authority_program_address(
            &self.solido_program_id,
            &self.solido_address,
            MINT_AUTHORITY,
        );
        mint_authority
    }

    /// The number of slots between the start of a duty slice and the start of
    /// the next duty slice, see [`get_current_maintainer_duty`].
    const MAINTAINER_DUTY_SLICE_LENGTH: Slot = 100;

    /// The number of slots before the start of the duty slice where no maintainer
    /// is on duty, see [`get_current_maintainer_duty`].
    const MAINTAINER_DUTY_PAUSE_LENGTH: Slot = 10;

    /// Return the maintainer who is currently on "maintainer duty".
    ///
    /// The maintenance tasks need to be executed by somebody. Every task
    /// preferably needs to be executed by a single maintainer. For most
    /// operations, the on-chain program either guarantees that in case of
    /// racing transactions, only one succeeds, or the operation is idempotent.
    /// But for some operations (like unstake) we don't have this protection
    /// yet, and besides, it is wasteful for all maintainers to create the same
    /// transaction and have all but one of them fail.
    ///
    /// The solution to this is to introduce _maintainer duty_. We divide time
    /// into slices based on slot number, and every maintainer will only perform
    /// maintenance during their assigned slice. This assumes that the maintainers
    /// all cooperate and run this version of the maintainer software, but the
    /// worst thing that will happen if they don't is that some transactions may
    /// get submitted by multiple maintainers. All maintainers can agree about
    /// who is on duty, because this is a pure function of the current Solido
    /// state and clock sysvar.
    ///
    /// One decision to make, is the duration of the duty slice.
    ///
    /// * Shorter cycles mean more risk of races (because a maintainer can
    ///   create a transaction just before the end of its duty, and it may not
    ///   yet have executed at the start of the next maintainer's duty, so the
    ///   next maintainer performs the same operation). To mitigate this, we
    ///   can leave a small gap between duty slices.
    /// * Larger slices avoid races, but increase the latency when the
    ///   maintainer that's on duty is offline.
    ///
    /// As an middle ground between those two, we take 100 slots, which at a
    /// block time of 550ms is a little under a minute per maintainer. If only
    /// one maintainer is offline, this means maintenance operations get delayed
    /// by at most ~55s.
    pub fn get_current_maintainer_duty(&self) -> Option<Pubkey> {
        if self.solido.maintainers.entries.is_empty() {
            return None;
        }

        let duty_slice = self.clock.slot / Self::MAINTAINER_DUTY_SLICE_LENGTH;
        let slot_in_duty_slice = self.clock.slot % Self::MAINTAINER_DUTY_SLICE_LENGTH;

        // In the last few slots of the slice, nobody is on duty, to minimize
        // races at the duty slice boundary.
        if slot_in_duty_slice
            >= Self::MAINTAINER_DUTY_SLICE_LENGTH - Self::MAINTAINER_DUTY_PAUSE_LENGTH
        {
            return None;
        }

        let maintainer_index = duty_slice % self.solido.maintainers.len() as u64;
        Some(self.solido.maintainers.entries[maintainer_index as usize].pubkey)
    }

    /// Return the slot at which the given maintainer's next duty slice starts.
    ///
    /// If the maintainer is currently on duty, this returns the start of its
    /// next duty slice, not the start of the current duty slice.
    ///
    /// See also [`get_current_maintainer_duty`].
    pub fn get_next_maintainer_duty_slot(&self, maintainer: &Pubkey) -> Option<Slot> {
        if self.solido.maintainers.entries.is_empty() {
            return None;
        }

        // Compute the start of the current "cycle", where in every cycle, every
        // maintainer has a single duty slice.
        let cycle_length =
            self.solido.maintainers.entries.len() as u64 * Self::MAINTAINER_DUTY_SLICE_LENGTH;
        let current_cycle_start_slot = (self.clock.slot / cycle_length) * cycle_length;

        // Compute the start of our slice within the current cycle.
        let self_index = self
            .solido
            .maintainers
            .entries
            .iter()
            .position(|m| m.pubkey == *maintainer)? as u64;
        let self_slice_start_slot =
            current_cycle_start_slot + self_index * Self::MAINTAINER_DUTY_SLICE_LENGTH;

        if self_slice_start_slot <= self.clock.slot {
            // It might be that we already had our duty in the current cycle.
            // In that case, the next duty is in the next cycle.
            Some(self_slice_start_slot + cycle_length)
        } else {
            Some(self_slice_start_slot)
        }
    }

    /// Return None if we observe we moved past `1 -
    /// SolidoState::END_OF_EPOCH_THRESHOLD`%. Return Some(()) if the above
    /// condition fails or `self.stake_unstake_any_time` is set to
    /// `true`.
    pub fn confirm_should_stake_unstake_in_current_slot(&self) -> Option<()> {
        match self.stake_time {
            StakeTime::Anytime => Some(()),
            StakeTime::OnlyNearEpochEnd => {
                // Get the slot that the current epoch started.
                let slots_epoch_begin = self
                    .epoch_schedule
                    .get_first_slot_in_epoch(self.clock.epoch);
                let next_epoch_begin = self
                    .epoch_schedule
                    .get_first_slot_in_epoch(self.clock.epoch + 1);
                let slots_per_epoch = next_epoch_begin
                    .checked_sub(slots_epoch_begin)
                    .expect("Next epoch's slot should be always greater than previous.");
                let slot_past_epoch = self.clock.slot.checked_sub(slots_epoch_begin).expect(
        "Current slot is less than the beginning of the epoch's slot. This shouldn't happen.",
    );
                let ratio = Rational {
                    numerator: slot_past_epoch,
                    denominator: slots_per_epoch,
                };
                if ratio > SolidoState::END_OF_EPOCH_THRESHOLD {
                    Some(())
                } else {
                    None
                }
            }
        }
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
    match state
        .solido
        .maintainers
        .entries
        .iter()
        .zip(&state.maintainer_balances)
        .filter(|(m, _)| m.pubkey == state.maintainer_address)
        .map(|(_, balance)| balance)
        .next()
    {
        Some(balance) if balance < &minimum_maintainer_balance => {
            return Err(MaintenanceError::new(format!(
                "Balance of the maintainer account {} is less than {}. \
                Please fund the maintainer account.",
                state.maintainer_address, minimum_maintainer_balance,
            ))
            .into())
        }
        _ => {}
    }

    // Try all of these operations one by one, and select the first one that
    // produces an instruction.
    let instruction_output: Option<MaintenanceInstruction> = None
        // Merging stake accounts goes before updating validator balance, to
        // ensure that the balance update needs to reference as few accounts
        // as possible.
        .or_else(|| state.try_merge_on_all_stakes())
        .or_else(|| state.try_update_exchange_rate())
        .or_else(|| state.try_update_stake_account_balance())
        .or_else(|| state.try_unstake_from_inactive_validator())
        // Collecting validator fees goes after updating the exchange rate,
        // because it may be rejected if the exchange rate is outdated.
        // Same for updating the validator balance.
        .or_else(|| state.try_deactivate_validator_if_commission_exceeds_max())
        .or_else(|| state.try_stake_deposit())
        .or_else(|| state.try_unstake_from_active_validators())
        .or_else(|| state.try_remove_validator())
        .or_else(|| state.try_sell_anker_rewards());

    match instruction_output {
        Some(maintenance_instruction) => {
            // Usually the maintainer is the only signer, but in some cases we
            // need to generate a new fresh keypair, which then also is a signer.
            let mut signers: Vec<&dyn Signer> = vec![config.signer];
            for keypair in &maintenance_instruction.additional_signers {
                signers.push(keypair);
            }
            config.sign_and_send_transaction(&[maintenance_instruction.instruction], &signers)?;
            Ok(Some(maintenance_instruction.output))
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
    let state = SolidoState::new(
        config,
        opts.solido_program_id(),
        opts.anker_program_id(),
        opts.solido_address(),
        *opts.stake_time(),
    )?;
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
            anker_state: Some(AnkerState::default()),
            validator_stake_accounts: vec![],
            validator_unstake_accounts: vec![],
            validator_vote_account_balances: vec![],
            validator_vote_accounts: vec![],
            validator_identity_account_balances: vec![],
            validator_infos: vec![],
            maintainer_balances: vec![],
            st_sol_mint: Mint::default(),
            reserve_address: Pubkey::new_unique(),
            reserve_account: Account::default(),
            rent: Rent::default(),
            clock: Clock::default(),
            epoch_schedule: EpochSchedule::default(),
            stake_history: StakeHistory::default(),
            maintainer_address: Pubkey::new_unique(),
            stake_time: StakeTime::Anytime,
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
            .add(Pubkey::new_unique(), Validator::new())
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
            .add(Pubkey::new_unique(), Validator::new())
            .unwrap();
        state
            .solido
            .validators
            .add(Pubkey::new_unique(), Validator::new())
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
            StakeType::Stake,
        );

        // The first attempt should stake with the first validator.
        assert_eq!(
            state.try_stake_deposit().unwrap().output,
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
            StakeType::Stake,
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
            state.try_stake_deposit().unwrap().output,
            MaintenanceOutput::StakeDeposit {
                validator_vote_account: state.solido.validators.entries[1].pubkey,
                amount: (MINIMUM_STAKE_ACCOUNT_BALANCE * 2).unwrap(),
                stake_account: stake_account_1.0,
            }
        );
    }

    #[test]
    fn next_maintainer_duty_slot_agrees_with_current_duty() {
        for num_maintainers in 1..10 {
            let mut state = new_empty_solido();
            state.solido.maintainers.maximum_entries = num_maintainers;
            for _ in 0..num_maintainers {
                state
                    .solido
                    .maintainers
                    .add(Pubkey::new_unique(), ())
                    .unwrap();
            }

            let maintainer_keys: Vec<Pubkey> = state
                .solido
                .maintainers
                .entries
                .iter()
                .map(|p| p.pubkey)
                .collect();

            // Check the next slot in forward order but also reverse order. With
            // forward order, the duty start slots all fall in the same cycle,
            // so by iterating backwards, we also test the other branch.
            let mut maintainers = Vec::new();
            maintainers.extend(maintainer_keys.iter().rev());
            maintainers.extend(maintainer_keys.iter());

            // Don't start at slot 0, to avoid wrapping below.
            state.clock.slot = SolidoState::MAINTAINER_DUTY_SLICE_LENGTH;

            for maintainer in &maintainers {
                let start_slot = state
                    .get_next_maintainer_duty_slot(maintainer)
                    .expect("The maintainer is part of the set, it should have a next duty.");
                for slot in start_slot - SolidoState::MAINTAINER_DUTY_PAUSE_LENGTH..start_slot {
                    state.clock.slot = slot;
                    assert_eq!(
                        state.get_current_maintainer_duty(),
                        None,
                        "In slot {}, during the pause before slot {}, no maintainer has duty.",
                        slot,
                        start_slot
                    );
                }
                for slot in start_slot
                    ..start_slot + SolidoState::MAINTAINER_DUTY_SLICE_LENGTH
                        - SolidoState::MAINTAINER_DUTY_PAUSE_LENGTH
                {
                    state.clock.slot = slot;
                    assert_eq!(
                        state.get_current_maintainer_duty(),
                        Some(*maintainer),
                        "Maintainer should have duty in slot {} (slice starting at slot {}).",
                        slot,
                        start_slot
                    );
                }
            }

            let not_maintainer = Pubkey::new_unique();
            assert_eq!(state.get_next_maintainer_duty_slot(&not_maintainer), None);
        }
    }

    #[test]
    fn next_maintainer_duty_returns_slot_greater_than_current_slot() {
        let mut state = new_empty_solido();
        let maintainer = Pubkey::new_unique();
        state.solido.maintainers.maximum_entries = 1;
        state.solido.maintainers.add(maintainer, ()).unwrap();

        for _ in 0..10 {
            let next_slot = state.get_next_maintainer_duty_slot(&maintainer).unwrap();
            assert!(next_slot > state.clock.slot);
            state.clock.slot = next_slot;
        }
    }

    #[test]
    fn test_below_epoch_threshold() {
        let mut state = new_empty_solido();
        state.stake_time = StakeTime::OnlyNearEpochEnd;
        // Epoch 1 starts at slot 32 and ends at slot 63
        // At slot 33 is at 1.5% of epoch.
        state.clock.slot = 33;
        state.clock.epoch = 1;
        assert_eq!(state.confirm_should_stake_unstake_in_current_slot(), None);
    }

    #[test]
    fn test_above_epoch_threshold() {
        let mut state = new_empty_solido();
        state.stake_time = StakeTime::OnlyNearEpochEnd;
        // Epoch 1 starts at slot 32 and ends at slot 63
        // At slot 32 + 62 is at 96.8% of epoch.
        state.clock.slot = 32 + 62;
        state.clock.epoch = 1;
        assert_eq!(
            state.confirm_should_stake_unstake_in_current_slot(),
            Some(())
        );
        // At slot 32 + 61 is at 95.3% of epoch.
        state.clock.slot = 32 + 61;
        assert_eq!(
            state.confirm_should_stake_unstake_in_current_slot(),
            Some(())
        );
    }

    #[test]
    fn test_respect_stake_time_config() {
        let mut state = new_empty_solido();
        state.stake_time = StakeTime::Anytime;
        state.clock.slot = 32;
        state.clock.epoch = 1;
        assert_eq!(
            state.confirm_should_stake_unstake_in_current_slot(),
            Some(())
        );
    }
}
