#![allow(clippy::too_many_arguments)]

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    system_program,
    sysvar::{self, stake_history},
};
use spl_stake_pool::{instruction::StakePoolInstruction, stake_program, state::Fee};

#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum LidoInstruction {
    Initialize,
    /// Deposit with amount
    Deposit {
        #[allow(dead_code)] // but it's not
        amount: u64,
    },
    /// Deposit amount to member validator
    DelegateDeposit {
        #[allow(dead_code)] // but it's not
        amount: u64,
    },
    StakePoolDelegate,
    Withdraw {
        #[allow(dead_code)] // but it's not
        amount: u64,
    },
}

pub fn initialize(
    program_id: &Pubkey,
    lido: &Pubkey,
    stake_pool: &Pubkey,
    owner: &Pubkey,
    mint_program: &Pubkey,
    pool_token_to: &Pubkey,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Initialize;
    let data = init_data.try_to_vec()?;
    let accounts = vec![
        AccountMeta::new(*lido, true),
        AccountMeta::new_readonly(*stake_pool, false),
        AccountMeta::new_readonly(*owner, false),
        AccountMeta::new(*mint_program, false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(*pool_token_to, false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

pub fn deposit(
    program_id: &Pubkey,
    lido: &Pubkey,
    stake_pool: &Pubkey,
    pool_token_to: &Pubkey,
    owner: &Pubkey,
    user: &Pubkey,
    recipient: &Pubkey,
    mint_program: &Pubkey,
    reserve_authority: &Pubkey,
    amount: u64,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Deposit { amount };
    let data = init_data.try_to_vec()?;
    let accounts = vec![
        AccountMeta::new(*lido, false),
        AccountMeta::new_readonly(*stake_pool, false),
        AccountMeta::new_readonly(*pool_token_to, false),
        AccountMeta::new_readonly(*owner, false),
        AccountMeta::new(*user, true),
        AccountMeta::new(*recipient, false),
        AccountMeta::new(*mint_program, false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new(*reserve_authority, false),
        AccountMeta::new_readonly(system_program::id(), false),
        // AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

pub fn delegate_deposit(
    program_id: &Pubkey,
    lido: &Pubkey,
    validator: &Pubkey,
    reserve: &Pubkey,
    stake: &Pubkey,
    deposit_authority: &Pubkey,

    amount: u64,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::DelegateDeposit { amount };
    let data = init_data.try_to_vec()?;
    let accounts = vec![
        AccountMeta::new(*lido, false),
        AccountMeta::new(*validator, false),
        AccountMeta::new(*reserve, false),
        AccountMeta::new(*stake, false),
        AccountMeta::new(*deposit_authority, false),
        // Sys
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(stake_program::id(), false),
        AccountMeta::new_readonly(stake_history::id(), false),
        AccountMeta::new_readonly(stake_program::config_id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

pub fn stake_pool_delegate(
    program_id: &Pubkey,
    lido: &Pubkey,
    validator: &Pubkey,
    stake: &Pubkey,
    deposit_authority: &Pubkey,
    pool_token: &Pubkey,
    // Stake pool
    stake_pool_program: &Pubkey,
    stake_pool: &Pubkey,
    stake_pool_validator_list: &Pubkey,
    stake_pool_withdraw_authority: &Pubkey,
    stake_pool_validator_stake_account: &Pubkey,
    stake_pool_mint: &Pubkey,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::StakePoolDelegate;
    let data = init_data.try_to_vec()?;
    let accounts = vec![
        AccountMeta::new(*lido, false),
        AccountMeta::new(*validator, false),
        AccountMeta::new(*stake, false),
        AccountMeta::new(*deposit_authority, false),
        AccountMeta::new(*pool_token, false),
        // Stake Pool
        AccountMeta::new_readonly(*stake_pool_program, false),
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new(*stake_pool_validator_list, false),
        AccountMeta::new_readonly(*stake_pool_withdraw_authority, false),
        AccountMeta::new(*stake_pool_validator_stake_account, false),
        AccountMeta::new(*stake_pool_mint, false),
        // Sys
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(stake_history::id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(stake_program::id(), false),
        // AccountMeta::new_readonly(stake_history::id(), false),
        // AccountMeta::new_readonly(stake_program::config_id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

pub fn stake_pool_deposit(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    validator_list_storage: &Pubkey,
    deposit_authority: &Pubkey,
    stake_pool_withdraw_authority: &Pubkey,
    deposit_stake_address: &Pubkey,
    validator_stake_accont: &Pubkey,
    pool_tokens_to: &Pubkey,
    pool_mint: &Pubkey,
    token_program_id: &Pubkey,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new(*validator_list_storage, false),
        AccountMeta::new_readonly(*deposit_authority, true),
        AccountMeta::new_readonly(*stake_pool_withdraw_authority, false),
        AccountMeta::new(*deposit_stake_address, false),
        AccountMeta::new(*validator_stake_accont, false),
        AccountMeta::new(*pool_tokens_to, false),
        AccountMeta::new(*pool_mint, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(*token_program_id, false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::Deposit.try_to_vec().unwrap(),
    }
}

pub fn initialize_stake_pool_with_authority(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    manager: &Pubkey,
    staker: &Pubkey,
    validator_list: &Pubkey,
    reserve_stake: &Pubkey,
    pool_mint: &Pubkey,
    manager_pool_account: &Pubkey,
    token_program_id: &Pubkey,
    deposit_authority: &Pubkey,
    fee: Fee,
    max_validators: u32,
) -> Result<Instruction, ProgramError> {
    let init_data = StakePoolInstruction::Initialize {
        fee,
        max_validators,
    };
    let data = init_data.try_to_vec()?;
    let accounts = vec![
        AccountMeta::new(*stake_pool, true),
        AccountMeta::new_readonly(*manager, true),
        AccountMeta::new_readonly(*staker, false),
        AccountMeta::new(*validator_list, false),
        AccountMeta::new_readonly(*reserve_stake, false),
        AccountMeta::new_readonly(*pool_mint, false),
        AccountMeta::new_readonly(*manager_pool_account, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(*token_program_id, false),
        AccountMeta::new_readonly(*deposit_authority, false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}
