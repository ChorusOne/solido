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
            return Err(LidoError::ReserveIsNotRentExempt);
        }
    }
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
