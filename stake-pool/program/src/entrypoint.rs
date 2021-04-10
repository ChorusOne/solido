//! Program entrypoint

// #![cfg(all(target_arch = "bpf", not(feature = "no-entrypoint")))]

use crate::{
    lido::processor::Processor as LidoProcessor,
    error::StakePoolError, processor::Processor,
};
use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult,
    program_error::PrintProgramError, pubkey::Pubkey,
};

entrypoint!(process_instruction);
fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if let Err(error) = Processor::process(program_id, accounts, instruction_data) {
        if let Ok(lido) = LidoProcessor::process(program_id, accounts, instruction_data) {
            // process lido somehow
            Ok(lido)
        } else {
            error.print::<StakePoolError>();
            Err(error)
        }

    } else {
        Ok(())
    }
}
