pub mod entrypoint;
pub mod error;
pub mod processor;
pub mod state;

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    decode_error::DecodeError,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::{PrintProgramError, ProgramError},
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    system_program,
    sysvar::Sysvar,
};

use std::convert::TryFrom;
use std::mem::size_of;

use crate::{error::StakePoolError, state::StakePool};
