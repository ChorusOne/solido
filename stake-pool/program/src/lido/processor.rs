//! Program state processor

use {
    crate::{
        borsh::try_from_slice_unchecked,
        error::StakePoolError,
        instruction::{Fee, StakePoolInstruction},
        stake_program,
        state::{AccountType, StakePool, ValidatorList, ValidatorStakeInfo},
        AUTHORITY_DEPOSIT, AUTHORITY_WITHDRAW,
    },
    bincode::deserialize,
    borsh::{BorshDeserialize, BorshSchema, BorshSerialize},
    num_traits::FromPrimitive,
    solana_program::{
        account_info::next_account_info,
        account_info::AccountInfo,
        clock::Clock,
        decode_error::DecodeError,
        entrypoint::ProgramResult,
        msg,
        native_token::sol_to_lamports,
        program::{invoke, invoke_signed},
        program_error::PrintProgramError,
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        stake_history::StakeHistory,
        system_instruction,
        sysvar::Sysvar,
    },
    spl_token::state::Mint,
};

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct Lido {
    stakepool: Pubkey,
    members: Pubkey,
}

#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum LidoInstruction {
    Initialize {
        stake_pool: Pubkey,
        member_list: Pubkey,
    },
    Deposit,
    Withdraw(u64),
}
/// Program state handler.
pub struct Processor {}
impl Processor {
    pub fn process_initialize(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        stake_pool: Pubkey,
        member_list: Pubkey,
    ) -> ProgramResult {
        Ok(())
    }
    pub fn process_deposit(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        Ok(())
    }
    pub fn process_withdraw(
        program_id: &Pubkey,
        pool_tokens: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        Ok(())
    }

    /// Processes [Instruction](enum.Instruction.html).
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        let instruction = LidoInstruction::try_from_slice(input)?;
        match instruction {
            LidoInstruction::Initialize {
                stake_pool,
                member_list,
            } => Self::process_initialize(program_id, accounts, stake_pool, member_list),
            LidoInstruction::Deposit {} => Self::process_deposit(program_id, accounts),
            LidoInstruction::Withdraw(amount) => {
                Self::process_withdraw(program_id, amount, accounts)
            }
        }
    }
}
