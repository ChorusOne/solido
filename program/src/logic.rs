use solana_program::{
    account_info::AccountInfo, borsh::try_from_slice_unchecked, msg, program::invoke_signed,
    program_error::ProgramError, pubkey::Pubkey, rent::Rent,
};
use spl_stake_pool::state::StakePool;
use crate::{error::LidoError, token::{Lamports, Rational, StLamports}, RESERVE_AUTHORITY, state::Lido};
use std::fmt::Display;

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
        &[&lido_info.key.to_bytes()[..32], RESERVE_AUTHORITY],
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
    ReserveAccount,
}

impl Display for AccountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let printable = match *self {
            Self::StakePool => "Stake pool",
            Self::Lido => "Lido",
            Self::ReserveAccount => "Reserve account",
        };
        write!(f, "{}", printable)
    }
}

pub fn calc_stakepool_lamports(
    stake_pool: &StakePool,
    pool_to_token_account: &spl_token::state::Account,
) -> Result<Lamports, LidoError> {
    if stake_pool.pool_token_supply == 0 {
        Some(Lamports(0))
    } else {
        Lamports(stake_pool.total_stake_lamports) * Rational {
            numerator: pool_to_token_account.amount,
            denominator: stake_pool.pool_token_supply,
        }
    }
    .ok_or(LidoError::CalculationFailure)
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
    stake_pool: &StakePool,
    program_share_of_stake_pool: &spl_token::state::Account,
    reserve_account: &AccountInfo,
    sysvar_rent: &Rent,
) -> Result<Lamports, LidoError> {
    let reserve_lamports = get_reserve_available_amount(reserve_account, sysvar_rent)?;
    // Get the total available lamports in the stake pool
    let stake_pool_lamports = calc_stakepool_lamports(stake_pool, program_share_of_stake_pool)?;

    (reserve_lamports + stake_pool_lamports).ok_or(LidoError::CalculationFailure)
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
    let authority_signature_seeds = [&me_bytes[..32], authority_type, &[bump_seed]];
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

#[allow(clippy::too_many_arguments)]
pub fn transfer_to<'a>(
    lido: &Pubkey,
    token_program: AccountInfo<'a>,
    source: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    authority: AccountInfo<'a>,
    authority_type: &[u8],
    bump_seed: u8,
    amount: u64,
) -> Result<(), ProgramError> {
    let me_bytes = lido.to_bytes();
    let authority_signature_seeds = [&me_bytes[..32], authority_type, &[bump_seed]];
    let signers = &[&authority_signature_seeds[..]];

    let ix = spl_token::instruction::transfer(
        token_program.key,
        source.key,
        destination.key,
        authority.key,
        &[],
        amount,
    )?;

    invoke_signed(
        &ix,
        &[source, destination, authority, token_program],
        signers,
    )
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

    #[test]
    fn test_calc_total_lamports() {
        let rent = &Rent::default();
        let mut stake_pool = StakePool::default();
        stake_pool.pool_token_supply = 1;
        let mut token_account = spl_token::state::Account::default();
        token_account.amount = 1;
        let key = Pubkey::default();
        let mut amount = rent.minimum_balance(0);
        let mut reserve_account =
            AccountInfo::new(&key, true, true, &mut amount, &mut [], &key, false, 0);

        assert_eq!(
            calc_total_lamports(&stake_pool, &token_account, &reserve_account, rent).unwrap(),
            Lamports(0)
        );
        let mut new_amount = rent.minimum_balance(0) + 10;
        reserve_account.lamports = Rc::new(RefCell::new(&mut new_amount));
        stake_pool.total_stake_lamports = 34;
        assert_eq!(
            calc_total_lamports(&stake_pool, &token_account, &reserve_account, rent).unwrap(),
            Lamports(44)
        );

        stake_pool.total_stake_lamports = u64::MAX;

        assert_eq!(
            calc_total_lamports(&stake_pool, &token_account, &reserve_account, rent),
            Err(LidoError::CalculationFailure)
        );
        let mut new_amount = u64::MAX;
        reserve_account.lamports = Rc::new(RefCell::new(&mut new_amount));
        stake_pool.total_stake_lamports = rent.minimum_balance(0) + 1;

        assert_eq!(
            calc_total_lamports(&stake_pool, &token_account, &reserve_account, rent),
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

        let result = calc_stakepool_lamports(&stakepool, &pool);

        assert_eq!(result.unwrap(), Lamports(0));
    }

    #[test]
    fn test_calc_stakepool_lamports_with_token_supply_increase() {
        let mut stakepool = StakePool::default();
        stakepool.pool_token_supply = 100;
        stakepool.total_stake_lamports = 50;
        let pool = spl_token::state::Account::default();

        let result = calc_stakepool_lamports(&stakepool, &pool);

        assert_eq!(result.unwrap(), Lamports(0));
    }

    #[test]
    fn test_calc_stakepool_lamports_with_token_supply_increase_and_pool_increase() {
        let mut stakepool = StakePool::default();
        stakepool.pool_token_supply = 100;
        stakepool.total_stake_lamports = 50;
        let mut pool = spl_token::state::Account::default();
        pool.amount = 30;

        let result = calc_stakepool_lamports(&stakepool, &pool);

        assert_eq!(result.unwrap(), Lamports(15));
    }

    #[test]
    fn test_calc_stakepool_lamports_with_pool_increase() {
        let mut stakepool = StakePool::default();
        stakepool.pool_token_supply = 100;
        let mut pool = spl_token::state::Account::default();
        pool.amount = 30;

        let result = calc_stakepool_lamports(&stakepool, &pool);

        assert_eq!(result.unwrap(), Lamports(0));
    }
}
