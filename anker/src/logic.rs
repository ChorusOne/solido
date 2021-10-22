use crate::{error::AnchorError, token::BLamports, ANCHOR_MINT_AUTHORITY};
use lido::{error::LidoError, token::Lamports};
use solana_program::{
    account_info::AccountInfo, borsh::try_from_slice_unchecked, entrypoint::ProgramResult, msg,
    program::invoke_signed, program_error::ProgramError, pubkey::Pubkey, rent::Rent,
    system_instruction,
};

use crate::{instruction::InitializeAccountsInfo, state::Anchor};

/// Deserialize the Anchor state.
/// Checks if the Lido instance is the same as the one stored in Anchor.
/// Checks if the Reserve account is the same as the one stored in Anchor.
pub fn deserialize_anchor(
    program_id: &Pubkey,
    anchor_account: &AccountInfo,
    lido: &Pubkey,
    reserve_account: &Pubkey,
) -> Result<Anchor, ProgramError> {
    if anchor_account.owner != program_id {
        msg!(
            "Anchor state is owned by {}, but should be owned by the Anchor program ({}).",
            anchor_account.owner,
            program_id
        );
        return Err(LidoError::InvalidOwner.into());
    }
    let anchor = try_from_slice_unchecked::<Anchor>(&anchor_account.data.borrow())?;
    anchor.check_lido(lido)?;
    anchor.check_reserve_account(program_id, anchor_account.key, reserve_account)?;
    Ok(anchor)
}

/// Mint the given amount of bSOL and put it in the recipient's account.
pub fn mint_b_sol_to<'a>(
    anchor: &Anchor,
    anchor_address: &Pubkey,
    spl_token_program: &AccountInfo<'a>,
    b_sol_mint: &AccountInfo<'a>,
    mint_authority: &AccountInfo<'a>,
    recipient: &AccountInfo<'a>,
    amount: BLamports,
) -> ProgramResult {
    if &anchor.bsol_mint != b_sol_mint.key {
        msg!(
            "Expected to find our bSOL mint ({}), but got {} instead.",
            anchor.bsol_mint,
            b_sol_mint.key
        );
        return Err(AnchorError::InvalidBSolAccount.into());
    }
    anchor.check_is_b_sol_account(recipient)?;

    let authority_signature_seeds = [
        &anchor_address.to_bytes(),
        ANCHOR_MINT_AUTHORITY,
        &[anchor.mint_authority_bump_seed],
    ];
    let signers = [&authority_signature_seeds[..]];

    // The SPL token program supports multisig-managed mints, but we do not
    // use those.
    let mint_to_signers = [];
    let instruction = spl_token::instruction::mint_to(
        spl_token_program.key,
        b_sol_mint.key,
        recipient.key,
        mint_authority.key,
        &mint_to_signers,
        amount.0,
    )?;

    invoke_signed(
        &instruction,
        &[
            b_sol_mint.clone(),
            recipient.clone(),
            mint_authority.clone(),
            spl_token_program.clone(),
        ],
        &signers,
    )
}

pub fn create_account<'a, 'b>(
    owner: &Pubkey,
    accounts: &InitializeAccountsInfo<'a, 'b>,
    new_account: &'a AccountInfo<'b>,
    sysvar_rent: &Rent,
    data_len: usize,
    seeds: &[&[u8]],
) -> ProgramResult {
    let rent_lamports = sysvar_rent.minimum_balance(data_len);
    let instr_create = system_instruction::create_account(
        accounts.fund_rent_from.key,
        new_account.key,
        rent_lamports,
        data_len as u64,
        owner,
    );
    msg!(
        "Creating account at {}, funded with {} from {}.",
        new_account.key,
        Lamports(rent_lamports),
        accounts.fund_rent_from.key,
    );
    invoke_signed(
        &instr_create,
        &[
            accounts.fund_rent_from.clone(),
            new_account.clone(),
            accounts.system_program.clone(),
        ],
        &[seeds],
    )
}

pub fn initialize_reserve_account(
    accounts: &InitializeAccountsInfo,
    seeds: &[&[u8]],
) -> ProgramResult {
    // Initialize the reserve account.
    invoke_signed(
        &spl_token::instruction::initialize_account(
            &spl_token::id(),
            accounts.reserve_account.key,
            accounts.st_sol_mint.key,
            accounts.reserve_authority.key,
        )?,
        &[
            accounts.reserve_account.clone(),
            accounts.st_sol_mint.clone(),
            accounts.reserve_authority.clone(),
            accounts.sysvar_rent.clone(),
        ],
        &[seeds],
    )
}
