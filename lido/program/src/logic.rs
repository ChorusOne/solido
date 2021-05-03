use std::fmt::Display;

use solana_program::{
    account_info::AccountInfo, msg, program_error::ProgramError, pubkey::Pubkey, rent::Rent,
};

use crate::{error::LidoError, RESERVE_AUTHORITY_ID};

pub(crate) fn rent_exemption(
    rent: &Rent,
    account_info: &AccountInfo,
    account_type: AccountType,
) -> Result<(), ProgramError> {
    if !rent.is_exempt(account_info.lamports(), account_info.data_len()) {
        msg!("{} not rent-exempt", account_type);
        return Err(ProgramError::AccountNotRentExempt);
    }
    Ok(())
}

pub fn check_reserve_authority(
    lido_info: &AccountInfo,
    program_id: &Pubkey,
    reserve_authority_info: &AccountInfo,
) -> Result<(), ProgramError> {
    let (reserve_id, _) = Pubkey::find_program_address(
        &[&lido_info.key.to_bytes()[..32], RESERVE_AUTHORITY_ID],
        program_id,
    );
    if reserve_id != *reserve_authority_info.key {
        msg!("Invalid reserve authority");
        return Err(LidoError::InvalidReserveAuthority.into());
    }
    Ok(())
}

pub(crate) enum AccountType {
    StakePool,
    Lido,
}

impl Display for AccountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let printable = match *self {
            Self::StakePool => "Stake pool",
            Self::Lido => "Lido",
        };
        write!(f, "{}", printable)
    }
}
