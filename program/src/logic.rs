use std::fmt::Display;

use solana_program::{
    account_info::AccountInfo, msg, program_error::ProgramError, pubkey::Pubkey, rent::Rent,
};
use spl_stake_pool::state::StakePool;
use std::convert::TryFrom;

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

pub fn calc_stakepool_lamports(
    stake_pool: StakePool,
    pool_to_token_account: spl_token::state::Account,
) -> Result<u64, ProgramError> {
    let stake_pool_lamports = if stake_pool.pool_token_supply != 0 {
        u64::try_from(
            (stake_pool.total_stake_lamports as u128)
                .checked_mul(pool_to_token_account.amount as u128)
                .ok_or(LidoError::CalculationFailure)?
                .checked_div(stake_pool.pool_token_supply as u128)
                .ok_or(LidoError::CalculationFailure)?,
        )
        .map_err(|_| LidoError::CalculationFailure)?
    } else {
        0
    };
    Ok(stake_pool_lamports)
}

pub fn calc_total_lamports(reserve_lamports: u64, stake_pool_lamports: u64) -> u64 {
    reserve_lamports + stake_pool_lamports
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_calc_total_lamports() {
        assert_eq!(calc_total_lamports(0, 0), 0);
        assert_eq!(calc_total_lamports(10, 10), 20);
        assert_eq!(calc_total_lamports(45, 74), 119);
        assert_eq!(calc_total_lamports(95, 37), 132);
    }

    #[test]
    fn test_account_not_rent_exempt() {
        let key = Pubkey::default();
        let mut lamports = 3000;
        let data = &mut [0; 8];
        let mut rent = Rent::default();
        rent.lamports_per_byte_year = 100;
        rent.exemption_threshold = 1.0;
        let account_type = AccountType::StakePool;
        let account = AccountInfo::new(&key, false, false, &mut lamports, data, &key, false, 1);

        let val = rent_exemption(&rent, &account, account_type);

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
        let account_type = AccountType::StakePool;
        let account = AccountInfo::new(&key, false, false, &mut lamports, data, &key, false, 1);

        let val = rent_exemption(&rent, &account, account_type);
        assert!(val.is_ok());
    }

    #[test]
    fn test_calc_stakepool_lamports_with_defaults() {
        let stakepool = StakePool::default();
        let pool = spl_token::state::Account::default();

        let result = calc_stakepool_lamports(stakepool, pool);

        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_calc_stakepool_lamports_with_token_supply_increase() {
        let mut stakepool = StakePool::default();
        stakepool.pool_token_supply = 100;
        stakepool.total_stake_lamports = 50;
        let pool = spl_token::state::Account::default();

        let result = calc_stakepool_lamports(stakepool, pool);

        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_calc_stakepool_lamports_with_token_supply_increase_and_pool_increase() {
        let mut stakepool = StakePool::default();
        stakepool.pool_token_supply = 100;
        stakepool.total_stake_lamports = 50;
        let mut pool = spl_token::state::Account::default();
        pool.amount = 30;

        let result = calc_stakepool_lamports(stakepool, pool);

        assert_eq!(result.unwrap(), 15);
    }

    #[test]
    fn test_calc_stakepool_lamports_with_pool_increase() {
        let mut stakepool = StakePool::default();
        stakepool.pool_token_supply = 100;
        let mut pool = spl_token::state::Account::default();
        pool.amount = 30;

        let result = calc_stakepool_lamports(stakepool, pool);

        assert_eq!(result.unwrap(), 0);
    }
}
