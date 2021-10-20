use borsh::BorshDeserialize;
use lido::token::StLamports;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

use crate::{
    instruction::{AnchorInstruction, DepositAccountsInfo, InitializeAccountsInfo},
    logic::deserialize_anchor,
    state::Anchor,
    ANCHOR_MINT_AUTHORITY,
};

fn process_initialize(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;

    let (_mint_authority, mint_bump_seed) = Pubkey::find_program_address(
        &[&accounts.anchor.key.to_bytes(), ANCHOR_MINT_AUTHORITY],
        program_id,
    );
    // TODO: Check mint authority.

    let anchor = Anchor {
        bsol_mint: *accounts.b_sol_mint.key,
        lido: *accounts.lido.key,
        mint_authority_bump_seed: mint_bump_seed,
    };

    // TODO: Check the mint program, similar to `lido::logic::check_mint`.

    anchor.save(accounts.anchor)
}

/// Deposit an amount of StLamports and get bSol in return.
fn process_deposit(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
    _amount: StLamports,
) -> ProgramResult {
    let accounts = DepositAccountsInfo::try_from_slice(accounts_raw)?;
    let _anchor = deserialize_anchor(program_id, accounts.anchor)?;

    Ok(())
}

/// Processes [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    let instruction = AnchorInstruction::try_from_slice(input)?;
    match instruction {
        AnchorInstruction::Initialize => process_initialize(program_id, accounts),
        AnchorInstruction::Deposit { amount } => process_deposit(program_id, accounts, amount),
        AnchorInstruction::Withdraw { amount } => todo!("{}", amount),
        AnchorInstruction::ClaimRewards => todo!(),
    }
}
