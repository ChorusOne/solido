use solana_program::entrypoint::ProgramResult;
use solana_program::{
    account_info::AccountInfo, borsh::try_from_slice_unchecked, msg, program::invoke_signed,
    program_error::ProgramError, pubkey::Pubkey, rent::Rent,
};

use crate::{
    error::LidoError,
    instruction::UpdateValidatorBalanceInfo,
    state::Fees,
    state::Lido,
    token::{Lamports, StLamports},
    MINT_AUTHORITY,
};

pub(crate) fn check_rent_exempt(
    rent: &Rent,
    account_info: &AccountInfo,
    account_name: &'static str,
) -> Result<(), ProgramError> {
    if !rent.is_exempt(account_info.lamports(), account_info.data_len()) {
        msg!("{} ({}) is not rent-exempt", account_name, account_info.key);
        return Err(ProgramError::AccountNotRentExempt);
    }
    Ok(())
}
/// Subtract the minimum rent-exempt balance from the given reserve balance.
///
/// The rent-exempt amount can never be transferred, or the account would
/// disappear, so we should not treat it as part of Solido's managed SOL.
pub fn get_reserve_available_balance(
    rent: &Rent,
    reserve_account: &AccountInfo,
) -> Result<Lamports, LidoError> {
    let minimum_balance = Lamports(rent.minimum_balance(0));
    match Lamports(reserve_account.lamports()) - minimum_balance {
        Some(balance) => Ok(balance),
        None => {
            msg!("The reserve account is not rent-exempt.");
            msg!("Please ensure it holds at least {}.", minimum_balance);
            Err(LidoError::ReserveIsNotRentExempt)
        }
    }
}

/// Mint the given amount of stSOL and put it in the recipient's account.
///
/// * The stSOL mint must be the one configured in the Solido instance.
/// * The recipient account must be an stSOL SPL token account.
pub fn mint_st_sol_to<'a>(
    solido: &Lido,
    solido_address: &Pubkey,
    spl_token_program: &AccountInfo<'a>,
    st_sol_mint: &AccountInfo<'a>,
    mint_authority: &AccountInfo<'a>,
    recipient: &AccountInfo<'a>,
    amount: StLamports,
) -> ProgramResult {
    solido.check_mint_is_st_sol_mint(st_sol_mint)?;
    solido.check_is_st_sol_account(recipient)?;

    let mint_authority = mint_authority;
    let solido_address_bytes = solido_address.to_bytes();
    let authority_signature_seeds = [
        &solido_address_bytes[..],
        MINT_AUTHORITY,
        &[solido.mint_authority_bump_seed],
    ];
    let signers = [&authority_signature_seeds[..]];

    // The SPL token program supports multisig-managed mints, but we do not
    // use those.
    let mint_to_signers = [];

    let instruction = spl_token::instruction::mint_to(
        &spl_token_program.key,
        &st_sol_mint.key,
        &recipient.key,
        &mint_authority.key,
        &mint_to_signers,
        amount.0,
    )?;

    invoke_signed(
        &instruction,
        &[
            st_sol_mint.clone(),
            recipient.clone(),
            mint_authority.clone(),
            spl_token_program.clone(),
        ],
        &signers,
    )
}

/// Mint stSOL for the given fees, and transfer them to the appropriate accounts.
pub fn distribute_fees<'a, 'b>(
    solido: &mut Lido,
    accounts: &UpdateValidatorBalanceInfo<'a, 'b>,
    fees: Fees,
) -> ProgramResult {
    // Convert all fees to stSOL according to the previously updated exchange rate.
    // In the case of fees, the SOL is already part of one of the stake accounts,
    // but we do still need to mint stSOL to represent it.

    let treasury_amount = solido
        .exchange_rate
        .exchange_sol(fees.treasury_amount)
        .ok_or(LidoError::CalculationFailure)?;

    let developer_amount = solido
        .exchange_rate
        .exchange_sol(fees.developer_amount)
        .ok_or(LidoError::CalculationFailure)?;

    let per_validator_amount = solido
        .exchange_rate
        .exchange_sol(fees.reward_per_validator)
        .ok_or(LidoError::CalculationFailure)?;

    // The treasury and developer fee we can mint and pay immediately.
    mint_st_sol_to(
        solido,
        accounts.lido.key,
        accounts.spl_token_program,
        accounts.st_sol_mint,
        accounts.mint_authority,
        accounts.treasury_st_sol_account,
        treasury_amount,
    )?;
    mint_st_sol_to(
        solido,
        accounts.lido.key,
        accounts.spl_token_program,
        accounts.st_sol_mint,
        accounts.mint_authority,
        accounts.developer_st_sol_account,
        developer_amount,
    )?;

    // For the validators, as there can be many of them, we can't pay all of
    // them in a single transaction. Instead, we store how much they are
    // entitled to, and they can later claim it themselves with `ClaimValidatorFees`.
    for validator in solido.validators.iter_entries_mut() {
        validator.fee_credit =
            (validator.fee_credit + per_validator_amount).ok_or(LidoError::CalculationFailure)?;
    }

    Ok(())
}

pub fn deserialize_lido(program_id: &Pubkey, lido: &AccountInfo) -> Result<Lido, ProgramError> {
    if lido.owner != program_id {
        msg!(
            "Lido state is owned by {}, but should be owned by the Lido program ({}).",
            lido.owner,
            program_id
        );
        return Err(LidoError::InvalidOwner.into());
    }
    let lido = try_from_slice_unchecked::<Lido>(&lido.data.borrow())?;
    Ok(lido)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_account_not_rent_exempt() {
        let key = Pubkey::default();
        let mut lamports = 3000;
        let data = &mut [0; 8];
        let mut rent = Rent::default();
        rent.lamports_per_byte_year = 100;
        rent.exemption_threshold = 1.0;
        let account = AccountInfo::new(&key, false, false, &mut lamports, data, &key, false, 1);

        let val = check_rent_exempt(&rent, &account, "dummy account");

        assert_eq!(val.err(), Some(ProgramError::AccountNotRentExempt));
    }

    #[test]
    fn test_account_is_rent_exempt() {
        let key = Pubkey::default();
        let mut lamports = 3000000;
        let data = &mut [0; 8];
        let mut rent = Rent::default();
        rent.lamports_per_byte_year = 100;
        rent.exemption_threshold = 1.0;
        let account = AccountInfo::new(&key, false, false, &mut lamports, data, &key, false, 1);

        let val = check_rent_exempt(&rent, &account, "dummy account");
        assert!(val.is_ok());
    }
}
