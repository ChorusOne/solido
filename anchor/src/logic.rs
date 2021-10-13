use crate::{error::AnchorError, token::BLamports, ANCHOR_MINT_AUTHORITY};
use lido::{error::LidoError, state::Lido};
use solana_program::{
    account_info::AccountInfo,
    borsh::try_from_slice_unchecked,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
};

use crate::{instruction::InitializeAccountsInfo, state::Anchor};

pub fn deserialize_anchor(
    program_id: &Pubkey,
    anchor: &AccountInfo,
) -> Result<Anchor, ProgramError> {
    if anchor.owner != program_id {
        msg!(
            "Lido state is owned by {}, but should be owned by the Lido program ({}).",
            anchor.owner,
            program_id
        );
        return Err(LidoError::InvalidOwner.into());
    }
    let lido = try_from_slice_unchecked::<Anchor>(&anchor.data.borrow())?;
    Ok(lido)
}

/// Mint the given amount of bSOL and put it in the recipient's account.
// TODO(#449): Remove underscore once this method is used.
pub fn _mint_b_sol_to<'a>(
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

pub fn create_reserve_account(
    seeds: &[&[&[u8]]],
    accounts: &InitializeAccountsInfo,
    rent: &Rent,
) -> ProgramResult {
    // `system_instruction::create_account` performs the same three steps as we
    // do below, but it additionally has a check to prevent creating an account
    // that has a nonzero balance, which we omit here.
    invoke_signed(
        &system_instruction::allocate(
            accounts.reserve_account.key,
            spl_token::state::Account::LEN as u64,
        ),
        &[
            accounts.reserve_account.clone(),
            accounts.system_program.clone(),
        ],
        seeds,
    )?;
    invoke_signed(
        &system_instruction::assign(accounts.reserve_account.key, &spl_token::id()),
        &[
            accounts.reserve_account.clone(),
            accounts.system_program.clone(),
        ],
        seeds,
    )?;
    invoke_signed(
        &system_instruction::transfer(
            accounts.signer.key,
            accounts.reserve_account.key,
            rent.minimum_balance(spl_token::state::Account::LEN),
        ),
        &[
            accounts.signer.clone(),
            accounts.reserve_account.clone(),
            accounts.system_program.clone(),
        ],
        seeds,
    )?;
    let lido = Lido::deserialize_lido(accounts.lido_program.key, accounts.lido)?;

    // Initialize the reserve account.
    invoke(
        &spl_token::instruction::initialize_account(
            &spl_token::id(),
            accounts.reserve_account.key,
            &lido.st_sol_mint,
            accounts.reserve_authority.key,
        )?,
        &[
            accounts.reserve_account.clone(),
            accounts.st_sol_mint.clone(),
            accounts.reserve_authority.clone(),
            accounts.sysvar_rent.clone(),
        ],
    )
}
