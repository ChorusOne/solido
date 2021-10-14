use borsh::BorshDeserialize;
use lido::{
    state::Lido,
    token::{Lamports, StLamports},
};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program::invoke,
    program_error::ProgramError, pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};

use crate::{
    error::AnchorError,
    instruction::{AnchorInstruction, DepositAccountsInfo, InitializeAccountsInfo},
    logic::{create_reserve_account, deserialize_anchor, mint_b_sol_to},
    state::Anchor,
    token::BLamports,
    ANCHOR_MINT_AUTHORITY, ANCHOR_RESERVE_ACCOUNT, ANCHOR_RESERVE_AUTHORITY,
};

fn process_initialize(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;
    let rent = Rent::from_account_info(accounts.sysvar_rent)?;

    let (_mint_authority, mint_bump_seed) = Pubkey::find_program_address(
        &[&accounts.anchor.key.to_bytes(), ANCHOR_MINT_AUTHORITY],
        program_id,
    );
    // TODO: Check mint authority.

    let (reserve_authority, reserve_authority_bump_seed) = Pubkey::find_program_address(
        &[&accounts.anchor.key.to_bytes(), ANCHOR_RESERVE_AUTHORITY],
        program_id,
    );

    let (reserve_account, reserve_account_bump_seed) = Pubkey::find_program_address(
        &[&accounts.anchor.key.to_bytes(), ANCHOR_RESERVE_ACCOUNT],
        program_id,
    );
    if &reserve_account != accounts.reserve_account.key {
        msg!(
            "Invalid reserve account, expected {}, but found {}.",
            reserve_account,
            accounts.reserve_account.key,
        );
        return Err(AnchorError::InvalidReserveAccount.into());
    }

    let reserve_account_seeds = [
        &accounts.anchor.key.to_bytes(),
        ANCHOR_RESERVE_ACCOUNT,
        &[reserve_account_bump_seed],
    ];
    // Create and initialize the reserve account.
    create_reserve_account(&[&reserve_account_seeds], &accounts, &rent)?;

    let anchor = Anchor {
        bsol_mint: *accounts.b_sol_mint.key,
        lido: *accounts.lido.key,
        reserve_authority: reserve_authority,
        mint_authority_bump_seed: mint_bump_seed,
        reserve_authority_bump_seed,
        reserve_account_bump_seed,
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

    let anchor = deserialize_anchor(
        program_id,
        accounts.anchor,
        accounts.lido.key,
        accounts.to_reserve_account.key,
    )?;

    // Check if the mint account is the same as the one stored in Anchor.
    anchor.check_mint(accounts.b_sol_mint.key)?;

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

    let lido = Lido::deserialize_lido(accounts.lido_program.key, accounts.lido)?;

    // Use Lido's exchange rate (`st_sol_supply / sol_balance`) to compute the
    // amount of BLamports to mint.
    let amount = BLamports(lido.exchange_rate.exchange_sol(Lamports(amount.0))?.0);

    mint_b_sol_to(
        &anchor,
        &accounts.anchor.key,
        accounts.spl_token,
        accounts.b_sol_mint,
        accounts.b_sol_mint_authority,
        accounts.b_sol_user_account,
        amount,
    )
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
