use solana_program::{account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, msg, pubkey::Pubkey};

use crate::{instructions::StakePoolInstruction, processor::Processor};

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    input: &[u8],
) -> ProgramResult {
    let instruction = StakePoolInstruction::deserialize(input)?;
    match instruction {
        StakePoolInstruction::Initialize(init) => {
            msg!("Instruction: Init");
            Processor::process_initialize(program_id, init, accounts)
        }
        StakePoolInstruction::Deposit(amount) => {
            msg!("Instruction: Deposit {}", amount);
            Processor::process_deposit(program_id, amount, accounts)
        }
    }
}
