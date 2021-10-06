use borsh::BorshDeserialize;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

use crate::instruction::AnchorInstruction;

/// Processes [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    let instruction = AnchorInstruction::try_from_slice(input)?;
    match instruction {
        AnchorInstruction::Initialize {} => todo!(),
        AnchorInstruction::Deposit { amount } => todo!(),
        AnchorInstruction::Withdraw { amount } => todo!(),
    }
}
