#![allow(clippy::too_many_arguments)]

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};

use solana_program::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    system_program,
    sysvar::{self, stake_history},
};
use spl_stake_pool::stake_program;

use crate::{
    accounts_struct,
    accounts_struct_meta,
    error::LidoError,
    state::FeeDistribution,
    token::{Lamports, StLamports},
};

#[repr(C)]
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum LidoInstruction {
    Initialize {
        #[allow(dead_code)] // but it's not
        fee_distribution: FeeDistribution,
        #[allow(dead_code)] // but it's not
        max_validators: u32,
        #[allow(dead_code)] // but it's not
        max_maintainers: u32,
    },
    /// Deposit a given amount of SOL.
    ///
    /// This can be called by anybody.
    Deposit {
        #[allow(dead_code)] // but it's not
        amount: Lamports,
    },
    /// Move deposits into a new stake account and delegate it to a member validator.
    ///
    /// This does not yet make the new stake account part of the stake pool;
    /// must be followed up by [`DepositActiveStakeToPool`].
    StakeDeposit {
        #[allow(dead_code)] // but it's not
        amount: Lamports,
    },
    /// Update the exchange rate, at the beginning of the epoch.
    ///
    /// This can be called by anybody.
    UpdateExchangeRate,
    Withdraw {
        #[allow(dead_code)] // but it's not
        amount: StLamports,
    },
    DistributeFees,
    ClaimValidatorFees,
    ChangeFeeSpec {
        #[allow(dead_code)] // but it's not
        new_fee_distribution: FeeDistribution,
    },
    AddValidator,
    RemoveValidator,
    AddMaintainer,
    RemoveMaintainer,
    MergeStake {
        #[allow(dead_code)] // but it's not
        from_seed: u64,
        #[allow(dead_code)] // but it's not
        to_seed: u64,
    },
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
    fee_distribution: FeeDistribution,
    max_validators: u32,
    max_maintainers: u32,
    accounts: &InitializeAccountsMeta,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Initialize {
        fee_distribution,
        max_validators,
        max_maintainers,
    };
    let data = init_data.try_to_vec()?;
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    })
}

accounts_struct! {
    DepositAccountsMeta, DepositAccountsInfo {
        pub lido {
            is_signer: false,
            // TODO(glottologist): This will need to be writable again once we
            // start storing metrics about deposits in the Solido state.
            is_writable: false,
        },
        pub user {
            is_signer: true,
            is_writable: true,
        },
        pub recipient {
            is_signer: false,
            is_writable: true,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: true,
        },
        pub reserve_account {
            is_signer: false,
            is_writable: true,
        },
        const spl_token = spl_token::id(),
        const system_program = system_program::id(),
    }
}

pub fn deposit(
    program_id: &Pubkey,
    accounts: &DepositAccountsMeta,
    amount: Lamports,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Deposit { amount };
    let data = init_data.try_to_vec()?;
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    })
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
            is_writable: true,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        // Must be set to the program-derived stake account for the given
        // validator, with seed `stake_accounts_seed_end`.
        pub stake_account_end {
            is_signer: false,
            is_writable: true,
        },
        pub stake_authority {
            is_signer: false,
            is_writable: true,
        },
        const sysvar_clock = sysvar::clock::id(),
        const system_program = system_program::id(),
        const sysvar_rent = sysvar::rent::id(),
        const stake_program = stake_program::id(),
        const stake_history = stake_history::id(),
        const stake_program_config = stake_program::config_id(),
    }
}

pub fn stake_deposit(
    program_id: &Pubkey,
    accounts: &StakeDepositAccountsMeta,
    amount: Lamports,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::StakeDeposit { amount };
    let data = init_data.try_to_vec()?;
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    })
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
    // There is no reason why `try_to_vec` should fail here.
    let data = LidoInstruction::UpdateExchangeRate.try_to_vec().unwrap();
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    }
}

// Changes the Fee spec
// The new Fee structure is passed by argument and the recipients are passed here
accounts_struct! {
    ChangeFeeSpecMeta, ChangeFeeSpecInfo {
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

pub fn change_fee_distribution(
    program_id: &Pubkey,
    new_fee_distribution: FeeDistribution,
    accounts: &ChangeFeeSpecMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::ChangeFeeSpec {
            new_fee_distribution,
        }
        .try_to_vec()?,
    })
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
        const sysvar_clock = sysvar::clock::id(),
        const sysvar_stake_history = sysvar::stake_history::id(),
        const sysvar_stake_program = stake_program::id(),
    }
}

pub fn add_validator(
    program_id: &Pubkey,
    accounts: &AddValidatorMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::AddValidator.try_to_vec()?,
    })
}

accounts_struct! {
    RemoveValidatorMeta, RemoveValidatorInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub validator_vote_account_to_remove {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_clock = sysvar::clock::id(),
        const sysvar_stake_program = stake_program::id(),
    }
}

pub fn remove_validator(
    program_id: &Pubkey,
    accounts: &RemoveValidatorMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::RemoveValidator.try_to_vec()?,
    })
}

accounts_struct! {
    DistributeFeesMeta, DistributeFeesInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub maintainer {
            is_signer: true,
            is_writable: false,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: true,
        },
        pub reserve_authority {
            is_signer: false,
            is_writable: false,
        },
        pub treasury_account {
            is_signer: false,
            is_writable: true,
        },
        pub developer_account {
            is_signer: false,
            is_writable: true,
        },
        const spl_token = spl_token::id(),
    }
}

pub fn distribute_fees(
    program_id: &Pubkey,
    accounts: &DistributeFeesMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::DistributeFees.try_to_vec()?,
    })
}

accounts_struct! {
    ClaimValidatorFeeMeta, ClaimValidatorFeeInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: true,
        },
        pub reserve_authority {
            is_signer: false,
            is_writable: false,
        },
        pub validator_fee_st_sol_account {
            is_signer: false,
            is_writable: true,
        },
        const spl_token = spl_token::id(),
    }
}

pub fn claim_validator_fees(
    program_id: &Pubkey,
    accounts: &ClaimValidatorFeeMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::ClaimValidatorFees.try_to_vec()?,
    })
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

pub fn add_maintainer(
    program_id: &Pubkey,
    accounts: &AddMaintainerMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::AddMaintainer.try_to_vec()?,
    })
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

pub fn remove_maintainer(
    program_id: &Pubkey,
    accounts: &RemoveMaintainerMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::RemoveMaintainer.try_to_vec()?,
    })
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
            is_writable: true,
        },
        pub to_stake {
            is_signer: false,
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
        const stake_program = stake_program::id(),
    }
}

pub fn merge_stake(
    program_id: &Pubkey,
    from_seed: u64,
    to_seed: u64,
    accounts: &MergeStakeMeta,
) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::MergeStake { from_seed, to_seed }
            .try_to_vec()
            .unwrap(), // This should never fail.
    }
}
