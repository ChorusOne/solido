use std::convert::TryInto;

use solana_program::{
    account_info::AccountInfo,
    entrypoint,
    entrypoint::ProgramResult,
    pubkey::Pubkey,
};

use crate::processor::Processor;

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    _data: &[u8],
) -> ProgramResult {
    let amount = u64::from_be_bytes(_data.try_into().unwrap());
    Processor::process_deposit(program_id, amount, accounts)
}
