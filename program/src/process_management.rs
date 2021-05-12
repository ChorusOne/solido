use borsh::BorshSerialize;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    borsh::try_from_slice_unchecked,
    entrypoint::ProgramResult,
    msg,
    pubkey::Pubkey,
};

use crate::{
    error::LidoError,
    state::{FeeDistribution, Lido},
};

pub fn process_change_fee_distribution(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let lido_info = next_account_info(account_info_iter)?;
    let manager = next_account_info(account_info_iter)?;
    let current_fee_distribution_info = next_account_info(account_info_iter)?;
    let new_fee_distribution_info = next_account_info(account_info_iter)?;
    let validator_credit_accounts_info = next_account_info(account_info_iter)?;

    if !manager.is_signer {
        msg!("Message needs to be signed by Lido's manager");
        return Err(LidoError::InvalidOwner.into());
    }
    if lido_info.owner != program_id {
        msg!("State has invalid owner");
        return Err(LidoError::InvalidOwner.into());
    }

    let mut lido = try_from_slice_unchecked::<Lido>(&lido_info.data.borrow())?;
    if &lido.fee_distribution != current_fee_distribution_info.key {
        msg!("Invalid current fee distribution account");
        return Err(LidoError::InvalidFeeDistributionAccount.into());
    }

    let current_fee_distribution =
        try_from_slice_unchecked::<FeeDistribution>(&current_fee_distribution_info.data.borrow())?;

    if &current_fee_distribution.validator_list_account != validator_credit_accounts_info.key {
        msg!("Invalid validators credit accounts");
        return Err(LidoError::InvalidValidatorCreditAccount.into());
    }

    let new_fee_distribution =
        try_from_slice_unchecked::<FeeDistribution>(&new_fee_distribution_info.data.borrow())?;
    new_fee_distribution.check_sum()?;

    if &new_fee_distribution.validator_list_account != validator_credit_accounts_info.key {
        msg!("Validator list account changed! This should not happen");
        return Err(LidoError::ValidatorCreditChanged.into());
    }

    lido.fee_distribution = *new_fee_distribution_info.key;

    lido.serialize(&mut *lido_info.data.borrow_mut())
        .map_err(|e| e.into())
}

pub fn process_add_validator(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let lido_info = next_account_info(account_info_iter)?;
    let manager = next_account_info(account_info_iter)?;
    let current_fee_distribution_info = next_account_info(account_info_iter)?;
    let new_fee_distribution_info = next_account_info(account_info_iter)?;
    let validator_credit_accounts_info = next_account_info(account_info_iter)?;

    unimplemented!()
}

pub fn process_remove_validator(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    unimplemented!()
}

/// TODO
/// Called by the validator, changes the fee account which the validator
/// receives tokens
pub fn process_change_fee_account(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    unimplemented!()
}
