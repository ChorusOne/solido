//! Entry point for maintenance operations, such as updating the pool balance.

use std::fmt;
use std::io;

use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_program::{
    clock::Clock, pubkey::Pubkey, rent::Rent, stake_history::StakeHistory, sysvar,
};
use solana_sdk::{account::Account, borsh::try_from_slice_unchecked, instruction::Instruction};
use spl_token::state::Mint;

use lido::account_map::PubkeyAndEntry;
use lido::util::serialize_b58;
use lido::{
    state::{Lido, Validator},
    token::Lamports,
    DEPOSIT_AUTHORITY, MINIMUM_STAKE_ACCOUNT_BALANCE,
};
use spl_stake_pool::stake_program::StakeState;

use crate::config::PerformMaintenanceOpts;
use crate::error::MaintenanceError;
use crate::helpers::{get_solido, sign_and_send_transaction};
use crate::stake_account::StakeBalance;
use crate::{error::Error, Config};
use lido::token::StLamports;
use solana_program::program_pack::Pack;
use std::time::SystemTime;

type Result<T> = std::result::Result<T, Error>;

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
    pub validator_stake_accounts: Vec<Vec<(Pubkey, StakeBalance)>>,

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
    rpc_client: &RpcClient,
    solido_program_id: &Pubkey,
    solido_address: &Pubkey,
    clock: &Clock,
    stake_history: &StakeHistory,
    validator: &PubkeyAndEntry<Validator>,
) -> Result<Vec<(Pubkey, StakeBalance)>> {
    let mut result = Vec::new();
    for seed in validator.entry.stake_accounts_seed_begin..validator.entry.stake_accounts_seed_end {
        let (addr, _bump_seed) = Validator::find_stake_account_address(
            solido_program_id,
            solido_address,
            &validator.pubkey,
            seed,
        );
        let account = rpc_client.get_account(&addr)?;
        let stake_state: StakeState = try_from_slice_unchecked(&account.data)
            .expect("Derived stake account contains invalid data.");
        let delegation = stake_state
            .delegation()
            .expect("Encountered undelegated stake account, this should not happen.");

        assert_eq!(
            delegation.voter_pubkey, validator.pubkey,
            "Expected the stake account for validator to delegate to that validator."
        );

        let balance = StakeBalance::from_delegated_account(
            Lamports(account.lamports),
            &delegation,
            clock,
            stake_history,
        );

        result.push((addr, balance));
    }
    Ok(result)
}

impl SolidoState {
    /// Read the state from the on-chain data.
    pub fn new(
        config: &Config,
        solido_program_id: &Pubkey,
        solido_address: &Pubkey,
    ) -> Result<SolidoState> {
        let rpc = &config.rpc;

        // TODO(#183): Transactions can execute in between those reads, leading to
        // a torn state. Make a function that re-reads everything with get_multiple_accounts.
        let solido = get_solido(rpc, solido_address)?;

        let reserve_address = solido.get_reserve_account(solido_program_id, solido_address)?;
        let reserve_account = rpc.get_account(&reserve_address)?;

        let st_sol_mint_account = rpc.get_account(&solido.st_sol_mint)?;
        let st_sol_mint = Mint::unpack(&st_sol_mint_account.data)?;

        let rent_account = rpc.get_account(&sysvar::rent::ID)?;
        let rent: Rent = bincode::deserialize(&rent_account.data)?;

        let clock_account = rpc.get_account(&sysvar::clock::ID)?;
        let clock: Clock = bincode::deserialize(&clock_account.data)?;

        let stake_history_account = rpc.get_account(&sysvar::stake_history::ID)?;
        let stake_history: StakeHistory = bincode::deserialize(&stake_history_account.data)?;

        let mut validator_stake_accounts = Vec::new();
        for validator in solido.validators.entries.iter() {
            validator_stake_accounts.push(get_validator_stake_accounts(
                rpc,
                solido_program_id,
                solido_address,
                &clock,
                &stake_history,
                validator,
            )?);
        }

        // The entity executing the maintenance transactions, is the maintainer.
        // We don't verify here if it is part of the maintainer set, the on-chain
        // program does that anyway.
        let maintainer_address = config.signer.pubkey();
        let maintainer_account = rpc.get_account(&maintainer_address)?;

        Ok(SolidoState {
            produced_at: SystemTime::now(),
            solido_program_id: *solido_program_id,
            solido_address: *solido_address,
            solido,
            validator_stake_accounts,
            reserve_address,
            reserve_account,
            st_sol_mint,
            rent,
            clock,
            maintainer_address,
            maintainer_account,
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

    /// If there is a deposit that can be staked, return the instruction to do so.
    pub fn try_stake_deposit(&self) -> Option<(Instruction, MaintenanceOutput)> {
        let reserve_balance = self.get_effective_reserve();

        // If there is enough reserve, we can make a deposit. To keep the pool
        // balanced, find the validator furthest below its target balance, and
        // deposit to that validator.
        let mut targets = vec![Lamports(0); self.solido.validators.len()];

        let undelegated_lamports = reserve_balance;
        lido::balance::get_target_balance(
            undelegated_lamports,
            &self.solido.validators,
            &mut targets[..],
        )
        .expect("Failed to compute target balance.");

        let (validator_index, amount_below_target) =
            lido::balance::get_validator_furthest_below_target(
                &self.solido.validators,
                &targets[..],
            );
        let validator = &self.solido.validators.entries[validator_index];

        let (stake_account_end, _bump_seed_end) = Validator::find_stake_account_address(
            &self.solido_program_id,
            &self.solido_address,
            &validator.pubkey,
            validator.entry.stake_accounts_seed_end,
        );

        let (deposit_authority, _bump_seed_authority) = lido::find_authority_program_address(
            &self.solido_program_id,
            &self.solido_address,
            DEPOSIT_AUTHORITY,
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

        let instruction = lido::instruction::stake_deposit(
            &self.solido_program_id,
            &lido::instruction::StakeDepositAccountsMeta {
                lido: self.solido_address,
                maintainer: self.maintainer_address,
                reserve: self.reserve_address,
                validator_vote_account: validator.pubkey,
                stake_account_end,
                deposit_authority,
            },
            amount_to_deposit,
        )
        .expect("Failed to construct StakeDeposit instruction.");

        let task = MaintenanceOutput::StakeDeposit {
            validator_vote_account: validator.pubkey,
            amount: amount_to_deposit,
            stake_account: stake_account_end,
        };

        Some((instruction, task))
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

    /// Write metrics about the current Solido instance in Prometheus format.
    pub fn write_prometheus<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        use crate::prometheus::{write_metric, Metric, MetricFamily};

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
                metrics: vec![Metric::new(self.maintainer_account.lamports)
                    // Enable 1e-9 factor: the metric is in SOL, but the value in lamports.
                    .nano()
                    .at(self.produced_at)
                    // Include the maintainer address, to prevent any confusion
                    // about which account this is monitoring.
                    .with_label("maintainer_address", self.maintainer_address.to_string())],
            },
        )?;

        // Gather the different components that make up Solido's SOL balance.
        // The values are stored in Lamports (1e-9 SOL), but Prometheus convention
        // is to use base units, so we set `.nano()` to report them in SOL.
        let mut balance_sol_metrics = vec![Metric::new(self.get_effective_reserve().0)
            .nano()
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
            let stake_balance: StakeBalance =
                stake_accounts.iter().map(|(_addr, balance)| *balance).sum();
            let metric = |amount: Lamports, status: &'static str| {
                Metric::new(amount.0)
                    .nano()
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

        let st_sol_supply = (StLamports(self.st_sol_mint.supply) + unclaimed_fees)
            .expect("stSOL supply no longer fits in u64.");
        write_metric(
            out,
            &MetricFamily {
                name: "solido_token_supply_st_sol",
                help: "Amount of stSOL that exists currently.",
                type_: "gauge",
                metrics: vec![
                    // The supply is measured in stSOL lamports (1e-9 stSOL), so set .nano().
                    Metric::new(st_sol_supply.0).nano().at(self.produced_at),
                ],
            },
        )?;

        write_metric(
            out,
            &MetricFamily {
                name: "solido_exchange_rate_supply_st_sol",
                help: "Amount of stSOL that existed at the time of the last exchange rate update.",
                type_: "gauge",
                metrics: vec![
                    // The supply is measured in stSOL lamports (1e-9 stSOL), so set .nano().
                    Metric::new(self.solido.exchange_rate.st_sol_supply.0)
                        .nano()
                        .at(self.produced_at),
                ],
            },
        )?;
        write_metric(
            out,
            &MetricFamily {
                name: "solido_exchange_rate_balance_sol",
                help: "Amount of SOL managed at the time of the last exchange rate update.",
                type_: "gauge",
                metrics: vec![
                    // The balance is measured in SOL lamports (1e-9 stSOL), so set .nano().
                    Metric::new(self.solido.exchange_rate.sol_balance.0)
                        .nano()
                        .at(self.produced_at),
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

        Ok(())
    }
    /// If there are stake accounts that can be merged, return the instruction
    /// to do so. The instruction merges a maximum of two stake accounts.
    pub fn try_merge_stake(&self) -> Result<Option<(Instruction, MaintenanceOutput)>> {
        todo!()
    }
}

fn stake_accounts_can_merge() {}

pub fn try_perform_maintenance(
    config: &Config,
    state: &SolidoState,
) -> Result<Option<MaintenanceOutput>> {
    // To prevent the maintenance transactions failing with mysterious errors
    // that are difficult to debug, before we do any maintenance, do a sanity
    // check to ensure that the maintainer has at least some SOL to pay the
    // transaction fees.
    let minimum_maintainer_balance = Lamports(100_000_000);
    if Lamports(state.maintainer_account.lamports) < minimum_maintainer_balance {
        return Err(Box::new(MaintenanceError {
            message: format!(
                "Balance of the maintainer account {} is less than {}. \
                Please fund the maintainer account.",
                state.maintainer_address, minimum_maintainer_balance,
            ),
        }));
    }

    // Try all of these operations one by one, and select the first one that
    // produces an instruction.
    let instruction: Option<(Instruction, MaintenanceOutput)> = None
        .or_else(|| state.try_update_exchange_rate())
        .or_else(|| state.try_stake_deposit());

    let (instr, description) = match instruction {
        Some(x) => x,
        None => return Ok(None),
    };

    // For maintenance operations, the maintainer is the only signer,
    // and that should be sufficient.
    sign_and_send_transaction(config, &[instr], &[config.signer])?;
    Ok(Some(description))
}

/// Inspect the on-chain Solido state, and if there is maintenance that can be
/// performed, do so. Returns a description of the task performed, if any.
///
/// This takes only one step, there might be more work left to do after this
/// function returns. Call it in a loop until it returns `None`. (And then still
/// call it in a loop, because the on-chain state might change.)
pub fn run_perform_maintenance(
    config: &Config,
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

        // Put enough SOL in the reserve that we can stake half of the deposit
        // with each of the validators, and still be above the minimum stake
        // balance.
        state.reserve_account.lamports += 4 * MINIMUM_STAKE_ACCOUNT_BALANCE.0;

        let stake_account_0 = Validator::find_stake_account_address(
            &state.solido_program_id,
            &state.solido_address,
            &state.solido.validators.entries[0].pubkey,
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

        let stake_account_1 = Validator::find_stake_account_address(
            &state.solido_program_id,
            &state.solido_address,
            &state.solido.validators.entries[1].pubkey,
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
