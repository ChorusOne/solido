use crate::{
    error::LidoError,
    state::Lido,
    token::{Lamports, StLamports},
};
use solana_program::{
    account_info::AccountInfo, borsh::try_from_slice_unchecked, msg, program::invoke_signed,
    program_error::ProgramError, pubkey::Pubkey, rent::Rent,
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

/// Gets the amount of lamports in reserve. The rent is subtracted from the total amount.
/// Fails if the reserve's balance minus rent is < 0.
pub fn get_reserve_available_amount(
    reserve_account: &AccountInfo,
    sysvar_rent: &Rent,
) -> Result<Lamports, LidoError> {
    reserve_account
        .lamports()
        .checked_sub(sysvar_rent.minimum_balance(0))
        .map(Lamports)
        .ok_or(LidoError::ReserveIsNotRentExempt)
}

/// Calculates the sum of lamports available in the reserve and the stake pool
/// Discounts the rent payed
pub fn calc_total_lamports(
    lido: &Lido,
    reserve_account: &AccountInfo,
    sysvar_rent: &Rent,
) -> Result<Lamports, LidoError> {
    // There are three places where we store SOL: the reserve account, the stake
    // pool, and stake accounts with activating stake.
    let reserve_balance = get_reserve_available_amount(reserve_account, sysvar_rent)?;
    let activating_balance: Option<Lamports> = lido
        .validators
        .entries
        .iter()
        .map(|pe| pe.entry.stake_accounts_balance)
        .sum();

    activating_balance
        .and_then(|s| s + reserve_balance)
        .ok_or(LidoError::CalculationFailure)
}

/// Issue a spl_token `MintTo` instruction.
#[allow(clippy::too_many_arguments)]
pub fn token_mint_to<'a>(
    lido: &Pubkey,
    token_program: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    authority: AccountInfo<'a>,
    authority_type: &[u8],
    bump_seed: u8,
    amount: StLamports,
) -> Result<(), ProgramError> {
    let me_bytes = lido.to_bytes();
    let authority_signature_seeds = [&me_bytes, authority_type, &[bump_seed]];
    let signers = &[&authority_signature_seeds[..]];

    let ix = spl_token::instruction::mint_to(
        token_program.key,
        mint.key,
        destination.key,
        authority.key,
        &[],
        amount.0,
    )?;

    invoke_signed(&ix, &[mint, destination, authority, token_program], signers)
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
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use crate::state::{Lido, Validator};

    #[test]
    fn test_calc_total_lamports() {
        let rent = &Rent::default();
        let mut lido = Lido::default();
        let key = Pubkey::default();
        let mut amount = rent.minimum_balance(0);
        let mut reserve_account =
            AccountInfo::new(&key, true, true, &mut amount, &mut [], &key, false, 0);

        assert_eq!(
            calc_total_lamports(&lido, &reserve_account, rent).unwrap(),
            Lamports(0)
        );

        let mut new_amount = rent.minimum_balance(0) + 10;
        reserve_account.lamports = Rc::new(RefCell::new(&mut new_amount));
        assert_eq!(
            calc_total_lamports(&lido, &reserve_account, rent).unwrap(),
            Lamports(10)
        );

        lido.validators.maximum_entries = 1;
        lido.validators
            .add(Pubkey::new_unique(), Validator::new(Pubkey::new_unique()))
            .unwrap();
        lido.validators.entries[0].entry.stake_accounts_balance = Lamports(37);
        assert_eq!(
            calc_total_lamports(&lido, &reserve_account, rent).unwrap(),
            Lamports(10 + 37)
        );

        lido.validators.entries[0].entry.stake_accounts_balance = Lamports(u64::MAX);

        assert_eq!(
            calc_total_lamports(&lido, &reserve_account, rent),
            Err(LidoError::CalculationFailure)
        );

        let mut new_amount = u64::MAX;
        reserve_account.lamports = Rc::new(RefCell::new(&mut new_amount));
        // The amount here is more than the rent exemption that gets discounted
        // from the reserve, causing an overflow.
        lido.validators.entries[0].entry.stake_accounts_balance = Lamports(5_000_000);

        assert_eq!(
            calc_total_lamports(&lido, &reserve_account, rent),
            Err(LidoError::CalculationFailure)
        );
    }

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
