// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Program entrypoint

#![cfg(all(target_arch = "bpf", not(feature = "no-entrypoint")))]

use crate::processor;
use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, pubkey::Pubkey,
};
use crate::state::AnkerProgramId;

entrypoint!(process_instruction);
fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let anker_program_id = AnkerProgramId(*program_id);
    processor::process(&anker_program_id, accounts, instruction_data)
}
