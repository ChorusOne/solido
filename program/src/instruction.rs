// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![allow(clippy::too_many_arguments)]

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};

use solana_program::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    stake as stake_program, system_program,
    sysvar::{self, stake_history},
};

use crate::{
    accounts_struct, accounts_struct_meta,
    error::LidoError,
    state::RewardDistribution,
    token::{Lamports, StLamports},
};

#[repr(C)]
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum LidoInstruction {
    Initialize {
        #[allow(dead_code)] // but it's not
        reward_distribution: RewardDistribution,
        #[allow(dead_code)] // but it's not
        max_validators: u32,
        #[allow(dead_code)] // but it's not
        max_maintainers: u32,
        #[allow(dead_code)] // but it's not
        max_validator_fee: u8,
    },

    /// Deposit a given amount of SOL.
    ///
    /// This can be called by anybody.
    Deposit {
        #[allow(dead_code)] // but it's not
        amount: Lamports,
    },

    /// Withdraw a given amount of stSOL.
    ///
    /// Caller provides some `amount` of StLamports that are to be burned in
    /// order to withdraw SOL.
    Withdraw {
        #[allow(dead_code)] // but it's not
        amount: StLamports,
    },

    /// Move deposits from the reserve into a stake account and delegate it to a member validator.
    StakeDeposit {
        #[allow(dead_code)] // but it's not
        amount: Lamports,
    },
    /// Unstake from a validator to a new stake account.
    Unstake {
        #[allow(dead_code)] // but it's not
        amount: Lamports,
    },
    /// Update the exchange rate, at the beginning of the epoch.
    ///
    /// This can be called by anybody.
    UpdateExchangeRate,

    /// Observe any external changes in the balances of a validator's stake accounts.
    ///
    /// If there is inactive balance in stake accounts, withdraw this back to the reserve.
    /// Distribute fees.
    WithdrawInactiveStake,

    ChangeRewardDistribution {
        #[allow(dead_code)] // but it's not
        new_reward_distribution: RewardDistribution,
    },

    /// Add a new validator to the validator set.
    ///
    /// Requires the manager to sign.
    AddValidator,

    /// Set the `active` flag to false for a given validator.
    ///
    /// Requires the manager to sign.
    ///
    /// Deactivation initiates the validator removal process:
    ///
    /// * It prevents new funds from being staked with the validator.
    /// * It signals to the maintainer bot to start unstaking from this validator.
    ///
    /// Once there are no more delegations to this validator, and it has no
    /// unclaimed fee credits, then the validator can be removed.
    DeactivateValidator,

    RemoveValidator,
    AddMaintainer,
    RemoveMaintainer,
    MergeStake,
}

impl LidoInstruction {
    pub fn to_vec(&self) -> Vec<u8> {
        // `BorshSerialize::try_to_vec` returns a Result, because it uses
        // `Borsh::serialize`, which takes an arbitrary writer, and which can
        // therefore return an IoError. But when serializing to a vec, there
        // is no IO, so for this particular writer, it should never fail.
        self.try_to_vec() // actually it can fail on OutOfMemory error
            .expect("Serializing an Instruction to Vec<u8> does not fail.")
    }
}

accounts_struct! {
    InitializeAccountsMeta, InitializeAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: false,
            is_writable: false,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: false,
        },
        pub treasury_account {
            is_signer: false,
            is_writable: false,
        },
        pub developer_account {
            is_signer: false,
            is_writable: false,
        },
        pub reserve_account {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_rent = sysvar::rent::id(),
        const spl_token = spl_token::id(),
    }
}

pub fn initialize(
    program_id: &Pubkey,
    reward_distribution: RewardDistribution,
    max_validators: u32,
    max_maintainers: u32,
    max_validator_fee: u8,
    accounts: &InitializeAccountsMeta,
) -> Instruction {
    let data = LidoInstruction::Initialize {
        reward_distribution,
        max_validators,
        max_maintainers,
        max_validator_fee,
    };
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: data.to_vec(),
    }
}

accounts_struct! {
    DepositAccountsMeta, DepositAccountsInfo {
        pub lido {
            is_signer: false,
            // Needs to be writable for us to update the metrics.
            is_writable: true,
        },
        pub user {
            is_signer: true,
            // Is writable due to transfer (system_instruction::transfer) from user to
            // reserve_account
            is_writable: true,
        },
        pub recipient {
            is_signer: false,
            // Is writable due to mint to (spl_token::instruction::mint_to) recipient from
            // st_sol_mint
            is_writable: true,
        },
        pub st_sol_mint {
            is_signer: false,
            // Is writable due to mint to (spl_token::instruction::mint_to) recipient from
            // st_sol_mint
            is_writable: true,
        },
        pub reserve_account {
            is_signer: false,
            // Is writable due to transfer (system_instruction::transfer) from user to
            // reserve_account
            is_writable: true,
        },
        pub mint_authority {
            is_signer: false,
            is_writable: false,
        },
        const spl_token = spl_token::id(),
        const system_program = system_program::id(),
    }
}

pub fn deposit(
    program_id: &Pubkey,
    accounts: &DepositAccountsMeta,
    amount: Lamports,
) -> Instruction {
    let data = LidoInstruction::Deposit { amount };
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: data.to_vec(),
    }
}

accounts_struct! {
    WithdrawAccountsMeta, WithdrawAccountsInfo {
        pub lido {
            is_signer: false,
            // Needs to be writable for us to update the metrics.
            is_writable: true,
        },
        pub st_sol_account_owner {
            is_signer: true,
            is_writable: false,
        },
        // This should be owned by the user.
        pub st_sol_account {
            is_signer: false,
            // Is writable due to st_sol burn (spl_token::instruction::burn)
            is_writable: true,
        },
        pub st_sol_mint {
            is_signer: false,
            // Is writable due to st_sol burn (spl_token::instruction::burn)
            is_writable: true,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        // Stake account to withdraw from.
        pub source_stake_account {
            is_signer: false,
            // Is writable due to spliti stake (solana_program::stake::instruction::split)
            is_writable: true,
        },
        // Stake where the withdrawn amounts will go.
        pub destination_stake_account {
            is_signer: true,
            // Is writable due to split stake (solana_program::stake::instruction::split) and
            // transfer of stake authority (solana_program::stake::instruction::authorize
            is_writable: true,
        },
        // Used to split stake accounts and burn tokens.
        pub stake_authority {
            is_signer: false,
            is_writable: false,
        },
        const spl_token = spl_token::id(),
        const sysvar_clock = sysvar::clock::id(),
        const system_program = system_program::id(),
        const stake_program = stake_program::program::id(),
    }
}

pub fn withdraw(
    program_id: &Pubkey,
    accounts: &WithdrawAccountsMeta,
    amount: StLamports,
) -> Instruction {
    let data = LidoInstruction::Withdraw { amount };
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: data.to_vec(),
    }
}

accounts_struct! {
    StakeDepositAccountsMeta, StakeDepositAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub maintainer {
            is_signer: true,
            is_writable: false,
        },
        pub reserve {
            is_signer: false,
            // Is writable due to transfer (system_instruction::transfer) from reserve_account to
            // stake_account_end
            is_writable: true,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        // For a `StakeDeposit` where we temporarily create an undelegated
        // account at `stake_account_end`, but immediately merge it into
        // `stake_account_merge_into`, this must be set to the program-derived
        // stake account for the validator, with seed `stake_seed.end
        // - 1`. For a `StakeDeposit` where we create a new stake account, this
        // should be set to the same value as `stake_account_end`.
        pub stake_account_merge_into {
            is_signer: false,
            // Is writable due to merge (stake_program::intruction::merge) of stake_account_end
            // into stake_account_merge_into under the condition that they are not equal
            is_writable: true,
        },
        // Must be set to the program-derived stake account for the given
        // validator, with seed `stake_seeds.end`.
        pub stake_account_end {
            is_signer: false,
            // Is writable due to transfer (system_instruction::transfer) from reserve_account to
            // stake_account_end and stake program being initialized
            // (stake_program::instruction::initialize)
            is_writable: true,
        },
        pub stake_authority {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_clock = sysvar::clock::id(),
        const system_program = system_program::id(),
        const sysvar_rent = sysvar::rent::id(),
        const stake_program = stake_program::program::id(),
        const stake_history = stake_history::id(),
        const stake_program_config = stake_program::config::id(),
    }
}

pub fn stake_deposit(
    program_id: &Pubkey,
    accounts: &StakeDepositAccountsMeta,
    amount: Lamports,
) -> Instruction {
    let data = LidoInstruction::StakeDeposit { amount };
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: data.to_vec(),
    }
}

accounts_struct! {
    UnstakeAccountsMeta, UnstakeAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub maintainer {
            is_signer: true,
            is_writable: false,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        // Source stake account is the oldest active stake account that we'll try
        // to unstake from.  Determined by the program-derived stake account for
        // the given validator, with seed `stake_seeds.begin`.
        pub source_stake_account {
            is_signer: false,
            // Is writable due to split (`stake_program::intruction::split`).
            is_writable: true,
        },
        // Destination stake account is the oldest unstake stake account that will
        // receive the split of the funds. Determined by the program-derived
        // stake account for the given validator, with seed `unstake_seeds.end`.
        pub destination_unstake_account {
            is_signer: false,
            // Is writable due to the first two instructions from split.
            is_writable: true,
        },
        // Stake authority, to be able to split the stake.
        pub stake_authority {
            is_signer: false,
            is_writable: false,
        },
        // Required to call `solana_program::stake::instruction::deactivate_stake`.
        const sysvar_clock = sysvar::clock::id(),
        // Required to call cross-program.
        const system_program = system_program::id(),
        // Required to call `stake_program::intruction::split`.
        const stake_program = stake_program::program::id(),
    }
}

pub fn unstake(
    program_id: &Pubkey,
    accounts: &UnstakeAccountsMeta,
    amount: Lamports,
) -> Instruction {
    let data = LidoInstruction::Unstake { amount };
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: data.to_vec(),
    }
}

accounts_struct! {
    UpdateExchangeRateAccountsMeta, UpdateExchangeRateAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub reserve {
            is_signer: false,
            is_writable: false,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_clock = sysvar::clock::id(),
        const sysvar_rent = sysvar::rent::id(),
    }
}

pub fn update_exchange_rate(
    program_id: &Pubkey,
    accounts: &UpdateExchangeRateAccountsMeta,
) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::UpdateExchangeRate.to_vec(),
    }
}

accounts_struct! {
    // Note: there are no signers among these accounts, updating validator
    // balance is permissionless, anybody can do it.
    WithdrawInactiveStakeMeta, WithdrawInactiveStakeInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        // The validator to update the balance for.
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },

        // This instruction withdraws any excess stake from the stake accounts
        // back to the reserve. The stake authority needs to sign off on those
        // (but program-derived, so it is not a signer here), and we need access
        // to the reserve.
        pub stake_authority {
            is_signer: false,
            is_writable: false,
        },
        pub reserve {
            is_signer: false,
            // Is writable due to withdraw from stake account to reserve (StakeAccount::stake_account_withdraw)
            is_writable: true,
        },

        // Updating balances also immediately mints rewards, so we need the stSOL
        // mint, and the fee accounts to deposit the stSOL into.
        pub st_sol_mint {
            is_signer: false,
            // Is writable due to fee mint (spl_token::instruction::mint_to)
            is_writable: true,
        },

        // Mint authority is required to mint tokens.
        pub mint_authority {
            is_signer: false,
            is_writable: false,
        },

        pub treasury_st_sol_account {
            is_signer: false,
            // Is writable due to fee mint (spl_token::instruction::mint_to) to treasury
            is_writable: true,
        },
        pub developer_st_sol_account {
            is_signer: false,
            // Is writable due to fee mint (spl_token::instruction::mint_to) to developer
            is_writable: true,
        },

        // Needed for minting rewards.
        const spl_token_program = spl_token::id(),

        // We only allow updating balances if the exchange rate is up to date,
        // so we need to know the current epoch.
        const sysvar_clock = sysvar::clock::id(),

        // Needed to determine if there is excess balance in a stake account.
        const sysvar_rent = sysvar::rent::id(),

        // Needed for the stake program, to withdraw from stake accounts.
        const sysvar_stake_history = sysvar::stake_history::id(),

        // Needed to withdraw from stake accounts.
        const stake_program = stake_program::program::id(),

        // The validator's stake accounts, from the begin seed until (but
        // excluding) the end seed.
        pub ...stake_accounts {
            is_signer: false,
            is_writable: true,
        },
    }
}

pub fn withdraw_inactive_stake(
    program_id: &Pubkey,
    accounts: &WithdrawInactiveStakeMeta,
) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::WithdrawInactiveStake.to_vec(),
    }
}

// Changes the Fee spec
// The new Fee structure is passed by argument and the recipients are passed here
accounts_struct! {
    ChangeRewardDistributionMeta, ChangeRewardDistributionInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub treasury_account {
            is_signer: false,
            is_writable: false,
        },
        pub developer_account {
            is_signer: false,
            is_writable: false,
        },
    }
}

pub fn change_reward_distribution(
    program_id: &Pubkey,
    new_reward_distribution: RewardDistribution,
    accounts: &ChangeRewardDistributionMeta,
) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::ChangeRewardDistribution {
            new_reward_distribution,
        }
        // Serializing the instruction should never fail.
        .try_to_vec()
        .unwrap(),
    }
}

accounts_struct! {
    AddValidatorMeta, AddValidatorInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        pub validator_fee_st_sol_account {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_rent = sysvar::rent::id(),
    }
}

pub fn add_validator(program_id: &Pubkey, accounts: &AddValidatorMeta) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::AddValidator.to_vec(),
    }
}

accounts_struct! {
    RemoveValidatorMeta, RemoveValidatorInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub validator_vote_account_to_remove {
            is_signer: false,
            is_writable: false,
        },
    }
}

pub fn remove_validator(program_id: &Pubkey, accounts: &RemoveValidatorMeta) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::RemoveValidator.to_vec(),
    }
}

accounts_struct! {
    DeactivateValidatorMeta, DeactivateValidatorInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub validator_vote_account_to_deactivate {
            is_signer: false,
            is_writable: false,
        },
    }
}

pub fn deactivate_validator(
    program_id: &Pubkey,
    accounts: &DeactivateValidatorMeta,
) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::DeactivateValidator.to_vec(),
    }
}

accounts_struct! {
    ClaimValidatorFeeMeta, ClaimValidatorFeeInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub st_sol_mint {
            is_signer: false,
            // Is writable due to fee mint (spl_token::instruction::mint_to) to validator fee
            // st_sol account
            is_writable: true,
        },
        pub mint_authority {
            is_signer: false,
            is_writable: false,
        },
        pub validator_fee_st_sol_account {
            is_signer: false,
            // Is writable due to fee mint (spl_token::instruction::mint_to) to validator fee
            // st_sol account
            is_writable: true,
        },
        const spl_token = spl_token::id(),
    }
}

accounts_struct! {
    AddMaintainerMeta, AddMaintainerInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub maintainer {
            is_signer: false,
            is_writable: false,
        },
    }
}

pub fn add_maintainer(program_id: &Pubkey, accounts: &AddMaintainerMeta) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::AddMaintainer.to_vec(),
    }
}

accounts_struct! {
    RemoveMaintainerMeta, RemoveMaintainerInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub maintainer {
            is_signer: false,
            is_writable: false,
        },
    }
}

pub fn remove_maintainer(program_id: &Pubkey, accounts: &RemoveMaintainerMeta) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::RemoveMaintainer.to_vec(),
    }
}

accounts_struct! {
    MergeStakeMeta, MergeStakeInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        pub from_stake {
            is_signer: false,
            // Is writable due to merge (solana_program::stake::instruction::merge)
            is_writable: true,
        },
        pub to_stake {
            is_signer: false,
            // Is writable due to merge (solana_program::stake::instruction::merge)
            is_writable: true,
        },
        // This instruction doesn’t reference the authority directly, but it
        // invokes one a `MergeStake` instruction that needs the deposit
        // authority to sign.
        pub stake_authority {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_clock = sysvar::clock::id(),
        const stake_history = stake_history::id(),
        const stake_program = stake_program::program::id(),
    }
}

pub fn merge_stake(program_id: &Pubkey, accounts: &MergeStakeMeta) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        // this can fail on OutOfMemory
        data: LidoInstruction::MergeStake.try_to_vec().unwrap(), // This should never fail.
    }
}
