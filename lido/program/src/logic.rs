use std::fmt::Display;

use solana_program::{account_info::AccountInfo, msg, program_error::ProgramError, rent::Rent};

pub (crate) fn rent_exemption(rent: &Rent, account_info: &AccountInfo, account_type: AccountType) -> Option<Result<(), ProgramError>> {
    if !rent.is_exempt(account_info.lamports(), account_info.data_len()) {
        msg!("{} not rent-exempt", account_type);
        return Some(Err(ProgramError::AccountNotRentExempt));
    }
    None
}

pub (crate) enum AccountType {
    StakePool,
    Lido
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