use crate::{error::AnkerError, token::BLamports, ANKER_MINT_AUTHORITY};
use lido::{state::Lido, token::Lamports};
use solana_program::{
    account_info::AccountInfo,
    borsh::try_from_slice_unchecked,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
};

use crate::instruction::DepositAccountsInfo;
use crate::{instruction::InitializeAccountsInfo, state::Anker};

/// Deserialize the Solido and Anker state.
///
/// Check the following things for consistency:
/// * The Solido state should be owned by the Solido program stored in Anker.
/// * The Solido state should live at the address stored in Anker.
/// * The reserve should live at the address derived from Anker.
/// * The reserve should be valid stSOL account.
///
/// The following things are not checked, because these accounts are not always
/// needed:
/// * The mint address should match the address stored in Anker.
/// * The mint authority should match the address derived from Anker.
/// * The reserve authority should match the address derived from Anker.
///
/// Note, the address of the Anker instance is a program-derived address that
/// derived from the Anker program address, and the Solido instance address of
/// the Solido instance that this Anker instance belongs to. This ensures that
/// for a given deployment of the Anker program, there exists a unique Anker
/// instance address per Solido instance.
pub fn deserialize_anker(
    anker_program_id: &Pubkey,
    anker_account: &AccountInfo,
    solido_account: &AccountInfo,
    reserve_account: &AccountInfo,
) -> Result<(Lido, Anker), ProgramError> {
    if anker_account.owner != anker_program_id {
        msg!(
            "Anker state is owned by {}, but should be owned by the Anker program ({}).",
            anker_account.owner,
            anker_program_id
        );
        return Err(AnkerError::InvalidOwner.into());
    }

    let anker = try_from_slice_unchecked::<Anker>(&anker_account.data.borrow())?;

    anker.check_self_address(anker_program_id, anker_account)?;

    if *solido_account.owner != anker.solido_program_id {
        msg!(
            "Anker state is associated with Solido program at {}, but Solido state is owned by {}.",
            anker.solido_program_id,
            solido_account.owner,
        );
        return Err(AnkerError::InvalidOwner.into());
    }

    if *solido_account.key != anker.solido {
        msg!(
            "Anker state is associated with Solido instance at {}, but found {}.",
            anker.solido,
            solido_account.owner,
        );
        return Err(AnkerError::InvalidSolidoInstance.into());
    }

    let solido = Lido::deserialize_lido(&anker.solido_program_id, solido_account)?;

    anker.check_reserve_address(anker_program_id, anker_account.key, reserve_account)?;
    anker.check_is_st_sol_account(&solido, reserve_account)?;

    Ok((solido, anker))
}

/// Mint the given amount of bSOL and put it in the recipient's account.
pub fn mint_b_sol_to(
    anker_program_id: &Pubkey,
    anker: &Anker,
    accounts: &DepositAccountsInfo,
    amount: BLamports,
) -> ProgramResult {
    // Check if the mint account is the same as the one stored in Anker.
    anker.check_mint(accounts.b_sol_mint)?;
    anker.check_mint_authority(
        anker_program_id,
        accounts.anker.key,
        accounts.b_sol_mint_authority,
    )?;

    anker.check_is_b_sol_account(accounts.b_sol_user_account)?;

    let authority_signature_seeds = [
        &accounts.anker.key.to_bytes(),
        ANKER_MINT_AUTHORITY,
        &[anker.mint_authority_bump_seed],
    ];
    let signers = [&authority_signature_seeds[..]];

    // The SPL token program supports multisig-managed mints, but we do not
    // use those.
    let mint_to_signers = [];
    let instruction = spl_token::instruction::mint_to(
        accounts.spl_token.key,
        accounts.b_sol_mint.key,
        accounts.b_sol_user_account.key,
        accounts.b_sol_mint_authority.key,
        &mint_to_signers,
        amount.0,
    )?;

    invoke_signed(
        &instruction,
        &[
            accounts.b_sol_mint.clone(),
            accounts.b_sol_user_account.clone(),
            accounts.b_sol_mint_authority.clone(),
            accounts.spl_token.clone(),
        ],
        &signers,
    )
}

/// Burn
pub fn burn_b_sol<'a>(
    anker: &Anker,
    spl_token_program: &AccountInfo<'a>,
    b_sol_mint: &AccountInfo<'a>,
    burn_from: &AccountInfo<'a>,
    burn_from_authority: &AccountInfo<'a>,
    amount: BLamports,
) -> ProgramResult {
    anker.check_mint(b_sol_mint)?;
    anker.check_is_b_sol_account(burn_from)?;

    // The SPL token program supports multisig-managed mints, but we do not use those.
    let burn_signers = [];
    let instruction = spl_token::instruction::burn(
        spl_token_program.key,
        burn_from.key,
        b_sol_mint.key,
        burn_from_authority.key,
        &burn_signers,
        amount.0,
    )?;

    invoke(
        &instruction,
        &[
            burn_from.clone(),
            b_sol_mint.clone(),
            burn_from_authority.clone(),
            spl_token_program.clone(),
        ],
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
