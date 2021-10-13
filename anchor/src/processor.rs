use borsh::BorshDeserialize;
use lido::{
    error::LidoError,
    state::Lido,
    token::{Lamports, StLamports},
};
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    instruction::{AnchorInstruction, DepositAccountsInfo, InitializeAccountsInfo},
    logic::{deserialize_anchor, mint_b_sol_to},
    state::Anchor,
    token::BLamports,
    ANCHOR_MINT_AUTHORITY, ANCHOR_RESERVE_AUTHORITY,
};

fn process_initialize(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;

    let (_mint_authority, mint_bump_seed) = Pubkey::find_program_address(
        &[&accounts.anchor.key.to_bytes(), ANCHOR_MINT_AUTHORITY],
        program_id,
    );
    // TODO: Check mint authority.

    let (reserve_authority, reserve_authority_bump_seed) = Pubkey::find_program_address(
        &[&accounts.anchor.key.to_bytes(), ANCHOR_RESERVE_AUTHORITY],
        program_id,
    );

    let anchor = Anchor {
        bsol_mint: *accounts.b_sol_mint.key,
        lido: *accounts.lido.key,
        reserve_account: *accounts.reserve_account.key,
        reserve_authority: reserve_authority,
        mint_authority_bump_seed: mint_bump_seed,
        reserve_authority_bump_seed: reserve_authority_bump_seed,
    };

    // TODO: Check the mint program, similar to `lido::logic::check_mint`.

    anchor.save(accounts.anchor)
}

/// Deposit an amount of StLamports and get bSol in return.
fn process_deposit(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
    amount: StLamports,
) -> ProgramResult {
    let accounts = DepositAccountsInfo::try_from_slice(accounts_raw)?;
    if amount == StLamports(0) {
        msg!("Amount must be greater than zero");
        return Err(ProgramError::InvalidArgument);
    }

    let anchor = deserialize_anchor(program_id, accounts.anchor)?;
    if &anchor.reserve_account != accounts.to_reserve_account.key {
        msg!(
            "Reserve account mismatch: expected {}, provided {}.",
            anchor.reserve_account,
            accounts.to_reserve_account.key
        );
        return Err(LidoError::InvalidReserveAccount.into());
    }

    // Transfer `amount` StLamports to the reserve.
    invoke(
        &spl_token::instruction::transfer(
            &spl_token::id(),
            accounts.from_account.key,
            accounts.to_reserve_account.key,
            accounts.user_authority.key,
            &[],
            amount.0,
        )?,
        &[
            accounts.from_account.clone(),
            accounts.to_reserve_account.clone(),
            accounts.user_authority.clone(),
            accounts.spl_token.clone(),
        ],
    )?;

    let lido = Lido::deserialize_lido(program_id, accounts.lido)?;

    // Use Lido's exchange rate (`st_sol_supply / sol_balance`) to compute the
    // amount of BLamports to send.
    let amount = BLamports(lido.exchange_rate.exchange_sol(Lamports(amount.0))?.0);

    mint_b_sol_to(
        &anchor,
        &accounts.anchor.key,
        accounts.spl_token,
        accounts.b_sol_mint,
        accounts.b_sol_mint_authority,
        accounts.b_sol_user_account,
        amount,
    )?;

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
