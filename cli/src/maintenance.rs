//! Entry point for maintenance operations, such as updating the pool balance.

use std::fmt;

use clap::Clap;
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_program::{
    clock::Clock, pubkey::Pubkey, rent::Rent, stake_history::StakeHistory, sysvar,
};
use solana_sdk::{account::Account, borsh::try_from_slice_unchecked, instruction::Instruction};

use lido::account_map::PubkeyAndEntry;
use lido::util::serialize_b58;
use lido::{
    state::{Lido, Validator},
    token::Lamports,
    DEPOSIT_AUTHORITY,
};

use crate::helpers::{get_solido, sign_and_send_transaction};
use crate::{error::Error, Config};
use spl_stake_pool::stake_program::StakeState;

type Result<T> = std::result::Result<T, Error>;

#[derive(Clap, Debug)]
pub struct PerformMaintenanceOpts {
    /// Address of the Solido program.
    #[clap(long)]
    pub solido_program_id: Pubkey,

    /// Account that stores the data for this Solido instance.
    #[clap(long)]
    pub solido_address: Pubkey,
}

/// A brief description of the maintenance performed. Not relevant functionally,
/// but helpful for automated testing, and just for info.
#[derive(Serialize)]
pub enum MaintenanceOutput {
    StakeDeposit {
        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
        #[serde(rename = "amount_lamports")]
        amount: Lamports,
    },
}

impl fmt::Display for MaintenanceOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MaintenanceOutput::StakeDeposit {
                validator_vote_account,
                amount,
            } => {
                writeln!(f, "Staked deposit.")?;
                writeln!(f, "  Validator vote account: {}", validator_vote_account)?;
                writeln!(f, "  Amount staked:          {}", amount)?;
            }
        }
        Ok(())
    }
}

/// A snapshot of on-chain accounts relevant to Solido.
pub struct SolidoState {
    pub solido_program_id: Pubkey,
    pub solido_address: Pubkey,
    pub solido: Lido,

    /// For each validator, in the same order as in `solido.validators`, holds
    /// the stake balance of the derived stake accounts from the begin seed until
    /// end seed.
    pub validator_stake_accounts: Vec<Vec<(Pubkey, StakeBalance)>>,

    pub reserve_address: Pubkey,
    pub reserve_account: Account,
    pub rent: Rent,

    /// Public key of the maintainer executing the maintenance.
    /// Must be a member of `solido.maintainers`.
    pub maintainer_address: Pubkey,
}

/// The balance of a stake account, split into the four states that stake can be in.
///
/// The sum of the four fields is equal to the SOL balance of the stake account.
pub struct StakeBalance {
    pub inactive: Lamports,
    pub activating: Lamports,
    pub active: Lamports,
    pub deactivating: Lamports,
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

        let target_epoch = clock.epoch;
        let history = Some(stake_history);
        // TODO(#184): Confirm the meaning of this.
        let fix_stake_deactivate = true;

        let (active_lamports, activating_lamports, deactivating_lamports) = delegation
            .stake_activating_and_deactivating(target_epoch, history, fix_stake_deactivate);

        let inactive_lamports = account.lamports
            .checked_sub(active_lamports)
            .expect("Active stake cannot be larger than stake account balance.")
            .checked_sub(activating_lamports)
            .expect("Activating stake cannot be larger than stake account balance - active.")
            .checked_sub(deactivating_lamports)
            .expect("Deactivating stake cannot be larger than stake account balance - active - activating.");

        let balance = StakeBalance {
            inactive: Lamports(inactive_lamports),
            activating: Lamports(activating_lamports),
            active: Lamports(active_lamports),
            deactivating: Lamports(deactivating_lamports),
        };

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

        Ok(SolidoState {
            solido_program_id: solido_program_id.clone(),
            solido_address: solido_address.clone(),
            solido,
            validator_stake_accounts,
            reserve_address,
            reserve_account,
            rent,
            // The entity executing the maintenance transactions, is the maintainer.
            // We don't verify here if it is part of the maintainer set, the on-chain
            // program does that anyway.
            maintainer_address: config.signer.pubkey(),
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
    pub fn try_stake_deposit(&self) -> Result<Option<(Instruction, MaintenanceOutput)>> {
        let reserve_balance = self.get_effective_reserve();
        let minimum_stake_account_balance =
            Lamports(self.rent.minimum_balance(std::mem::size_of::<StakeState>()));

        // If there is not enough reserve to create a new stake account, we
        // can't stake the deposit, even if there is some balance.
        if reserve_balance < minimum_stake_account_balance {
            return Ok(None);
        }

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

        let (stake_account_end, _bump_seed) = Validator::find_stake_account_address(
            &self.solido_program_id,
            &self.solido_address,
            &validator.pubkey,
            validator.entry.stake_accounts_seed_end,
        );

        let (deposit_authority, _bump_seed) = lido::find_authority_program_address(
            &self.solido_program_id,
            &self.solido_address,
            DEPOSIT_AUTHORITY,
        );

        // Top up the validator to at most its target. If that means we don't use the full
        // reserve, a future maintenance run will stake the remainder with the next validator.
        let amount_to_deposit = amount_below_target.min(reserve_balance);

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
        };

        Ok(Some((instruction, task)))
    }
}

pub fn try_perform_maintenance(
    config: &Config,
    state: &SolidoState,
) -> Result<Option<MaintenanceOutput>> {
    // Try all of these operations one by one, and select the first one that
    // produces an instruction.
    let instruction: Option<Result<(Instruction, MaintenanceOutput)>> =
        None.or_else(|| state.try_stake_deposit().transpose());

    match instruction {
        Some(Ok((instr, output))) => {
            // For maintenance operations, the maintainer is the only signer,
            // and that should be sufficient.
            sign_and_send_transaction(config, &[instr], &[config.signer])?;
            Ok(Some(output))
        }
        Some(Err(err)) => Err(err),
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
    config: &Config,
    opts: &PerformMaintenanceOpts,
) -> Result<Option<MaintenanceOutput>> {
    let state = SolidoState::new(config, &opts.solido_program_id, &opts.solido_address)?;
    try_perform_maintenance(config, &state)
}
