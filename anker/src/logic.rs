use crate::{
    error::AnkerError,
    token::{BLamports, MicroUst},
    ANKER_MINT_AUTHORITY, ANKER_RESERVE_AUTHORITY,
};
use lido::{
    state::Lido,
    token::{ArithmeticError, Lamports, StLamports},
};
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
use spl_token_swap::{
    curve::calculator::{CurveCalculator, TradeDirection},
    instruction::Swap,
};
use std::convert::TryFrom;

use crate::{
    instruction::{DepositAccountsInfo, InitializeAccountsInfo, SellRewardsAccountsInfo},
    state::Anker,
};

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
/// * The StSOL/UST reserve address should match the address derived from Anker.
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

/// Initialize an SPL account with the owner set as the reserve authority.
pub fn initialize_spl_account<'a, 'b>(
    accounts: &InitializeAccountsInfo<'a, 'b>,
    seeds: &[&[u8]],
    account: &'a AccountInfo<'b>,
    mint: &'a AccountInfo<'b>,
) -> ProgramResult {
    // Initialize the reserve account.
    invoke_signed(
        &spl_token::instruction::initialize_account(
            &spl_token::id(),
            account.key,
            mint.key,
            accounts.reserve_authority.key,
        )?,
        &[
            account.clone(),
            mint.clone(),
            accounts.reserve_authority.clone(),
            accounts.sysvar_rent.clone(),
        ],
        &[seeds],
    )
}

/// Swap the `amount` from StSOL to UST
///
/// Sends the UST to the `accounts.ust_reserve`
pub fn swap_rewards(
    program_id: &Pubkey,
    amount: StLamports,
    anker: &Anker,
    accounts: &SellRewardsAccountsInfo,
    minimum_ust_out: MicroUst,
) -> ProgramResult {
    if amount == StLamports(0) {
        msg!("Anker rewards must be greater than zero to be claimable.");
        return Err(AnkerError::ZeroRewardsToClaim.into());
    }
    anker.check_token_swap_before_sell(program_id, accounts)?;

    let swap_instruction = spl_token_swap::instruction::swap(
        accounts.token_swap_program_id.key,
        accounts.spl_token.key,
        accounts.token_swap_pool.key,
        accounts.token_swap_authority.key,
        accounts.reserve_authority.key,
        accounts.st_sol_reserve_account.key,
        accounts.pool_st_sol_account.key,
        accounts.pool_ust_account.key,
        accounts.ust_reserve_account.key,
        accounts.pool_mint.key,
        accounts.pool_fee_account.key,
        None,
        Swap {
            amount_in: amount.0,
            minimum_amount_out: minimum_ust_out.0,
        },
    )?;

    let authority_signature_seeds = [
        &accounts.anker.key.to_bytes(),
        ANKER_RESERVE_AUTHORITY,
        &[anker.reserve_authority_bump_seed],
    ];
    let signers = [&authority_signature_seeds[..]];

    invoke_signed(
        &swap_instruction,
        &[
            accounts.token_swap_pool.clone(),
            accounts.token_swap_authority.clone(),
            accounts.reserve_authority.clone(),
            accounts.st_sol_reserve_account.clone(),
            accounts.pool_st_sol_account.clone(),
            accounts.pool_ust_account.clone(),
            accounts.ust_reserve_account.clone(),
            accounts.pool_mint.clone(),
            accounts.pool_fee_account.clone(),
            accounts.spl_token.clone(),
            accounts.token_swap_program_id.clone(),
        ],
        &signers,
    )
}

/// Get the price for selling 1 stSOL in MicroUst in the token swap pool.
pub fn get_one_st_sol_for_ust_price_from_pool(
    curve_calculator: &dyn CurveCalculator,
    swap_pool_token_a: &Pubkey,
    pool_ust_address: &Pubkey,
    pool_st_sol_balance: StLamports,
    pool_ust_balance: MicroUst,
) -> Result<MicroUst, ProgramError> {
    // To sample the price, we go from stSOL to UST.
    let trade_direction = if swap_pool_token_a == pool_ust_address {
        TradeDirection::BtoA
    } else {
        TradeDirection::AtoB
    };

    // Check how much UST we get out, if we put in 1 stSOL. With a constant-product
    // pool, the amount we get out depends not only on the state of the pool, but
    // also on the amount we put in. We pick 1 stSOL here because it should be
    // large enough that we don't lose precision in the output, but small enough
    // to not move the price by a lot if we did swap that amount.
    let one_st_sol = StLamports(1_000_000_000);
    let swap_result = curve_calculator
        .swap_without_fees(
            one_st_sol.0 as u128,
            pool_st_sol_balance.0 as u128,
            pool_ust_balance.0 as u128,
            trade_direction,
        )
        .ok_or(AnkerError::PoolPriceUndefined)?;
    Ok(MicroUst(
        u64::try_from(swap_result.destination_amount_swapped).map_err(|_| ArithmeticError)?,
    ))
}

#[cfg(test)]
mod test {
    use super::*;
    use spl_token_swap::curve::constant_product::ConstantProductCurve;

    #[test]
    fn test_less_than_one_st_sol_for_ust() {
        // Previously, we had one assert that stated we sold exactly one stSOL,
        // sometimes due to precision errors this assertion might fail. We
        // removed it and put this test that sells `Lamports(999_999_998)`.
        let curve = ConstantProductCurve::default();
        let swap_pool_token_a = Pubkey::new_unique();
        let pool_ust_address = Pubkey::new_unique();
        let result = get_one_st_sol_for_ust_price_from_pool(
            &curve,
            &swap_pool_token_a,
            &pool_ust_address,
            StLamports(500_000_000),
            MicroUst(1_000_000_000),
        );
        assert_eq!(result, Ok(MicroUst(666_666_666)));
    }
}
