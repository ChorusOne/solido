//! Program entrypoint

// #![cfg(all(target_arch = "bpf", not(feature = "no-entrypoint")))]

use crate::error::StakePoolError;
use crate::lido::processor::Processor;
use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult,
    program_error::PrintProgramError, pubkey::Pubkey,
};

pub fn process_lido_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if let Err(error) = Processor::process(program_id, accounts, instruction_data) {
        // catch the error so we can print it
        error.print::<StakePoolError>();
        Err(error)
    } else {
        Ok(())
    }
}
