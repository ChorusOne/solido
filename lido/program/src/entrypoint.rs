//! Program entrypoint

#![cfg(all(target_arch = "bpf", not(feature = "no-entrypoint")))]

use crate::processor::Processor;
use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, pubkey::Pubkey,
};

entrypoint!(process_instruction);
fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if let Err(error) = Processor::process(program_id, accounts, instruction_data) {
        Err(error)
    } else {
        Ok(())
    }
}
