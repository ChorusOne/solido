//! State transition types

use crate::error::StakePoolError;
use crate::processor::Processor;
use core::convert::TryInto;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};
use std::convert::TryFrom;
use std::mem::size_of;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StakePool {
    /// Pool version
    pub version:              u8,
    /// Owner authority
    /// allows for updating the staking authority
    pub owner:                Pubkey,
    /// Deposit authority bump seed
    /// for `create_program_address(&[state::StakePool account, "deposit"])`
    pub deposit_bump_seed:    u8,
    /// Withdrawal authority bump seed
    /// for `create_program_address(&[state::StakePool account, "withdrawal"])`
    pub withdraw_bump_seed:   u8,
    /// Validator stake list storage account
    pub validator_stake_list: Pubkey,
    /// Credit list storage account
    pub credit_list:          Pubkey,
    /// Pool Mint
    pub pool_mint:            Pubkey,
    /// Owner fee account
    pub owner_fee_account:    Pubkey,
    /// Credit reserve
    pub credit_reserve:       Pubkey,
    /// Pool token program id
    pub token_program_id:     Pubkey,
    /// total stake in SOL in the pool
    pub stake_total:          u64,
    /// amount of tSOL in the pool
    pub pool_total:           u64,
    /// Last epoch stake_total field was updated
    pub last_update_epoch:    u64,
    /// Fee applied to deposits
    pub fee:                  Fee,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fee {
    /// denominator of the fee ratio
    pub denominator: u64,
    /// numerator of the fee ratio
    pub numerator:   u64,
}


impl StakePool {
    /// Length of state data when serialized
    pub const LEN: usize = size_of::<StakePool>();

    /// calculate the pool tokens that should be minted
    pub fn calc_pool_deposit_amount(&self, stake_lamports: u64) -> Option<u64> {
        if self.stake_total == 0 {
            return Some(stake_lamports);
        }
        self.calc_pool_withdraw_amount(stake_lamports)
    }

    /// calculate the pool tokens that should be withdrawn
    pub fn calc_pool_withdraw_amount(&self, stake_lamports: u64) -> Option<u64> {
        u64::try_from(
            (stake_lamports as u128)
                .checked_mul(self.pool_total as u128)?
                .checked_div(self.stake_total as u128)?,
        )
        .ok()
    }

    /// calculate lamports amount
    pub fn calc_lamports_amount(&self, pool_tokens: u64) -> Option<u64> {
        u64::try_from(
            (pool_tokens as u128)
                .checked_mul(self.stake_total as u128)?
                .checked_div(self.pool_total as u128)?,
        )
        .ok()
    }

    /// calculate the fee in pool tokens that goes to the owner
    pub fn calc_fee_amount(&self, pool_amount: u64) -> Option<u64> {
        if self.fee.denominator == 0 {
            return Some(0);
        }
        u64::try_from(
            (pool_amount as u128)
                .checked_mul(self.fee.numerator as u128)?
                .checked_div(self.fee.denominator as u128)?,
        )
        .ok()
    }

    /// Checks withdraw authority
    pub fn check_authority_withdraw(
        &self,
        authority_to_check: &Pubkey,
        program_id: &Pubkey,
        stake_pool_key: &Pubkey,
    ) -> Result<(), ProgramError> {
        Processor::check_authority(
            authority_to_check,
            program_id,
            stake_pool_key,
            Processor::AUTHORITY_WITHDRAW,
            self.withdraw_bump_seed,
        )
    }

    /// Checks deposit authority
    pub fn check_authority_deposit(
        &self,
        authority_to_check: &Pubkey,
        program_id: &Pubkey,
        stake_pool_key: &Pubkey,
    ) -> Result<(), ProgramError> {
        Processor::check_authority(
            authority_to_check,
            program_id,
            stake_pool_key,
            Processor::AUTHORITY_DEPOSIT,
            self.deposit_bump_seed,
        )
    }

    /// Check owner validity and signature
    pub fn check_owner(&self, owner_info: &AccountInfo) -> ProgramResult {
        if *owner_info.key != self.owner {
            return Err(StakePoolError::WrongOwner.into());
        }
        if !owner_info.is_signer {
            return Err(StakePoolError::SignatureMissing.into());
        }
        Ok(())
    }

    /// Check if StakePool is initialized
    pub fn is_initialized(&self) -> bool {
        self.version > 0
    }

    /// Deserializes a byte buffer into a [StakePool](struct.StakePool.html).
    pub fn deserialize(input: &[u8]) -> Result<StakePool, ProgramError> {
        if input.len() < size_of::<StakePool>() {
            return Err(ProgramError::InvalidAccountData);
        }

        let stake_pool: &StakePool = unsafe { &*(&input[0] as *const u8 as *const StakePool) };

        Ok(*stake_pool)
    }

    /// Serializes [StakePool](struct.StakePool.html) into a byte buffer.
    pub fn serialize(&self, output: &mut [u8]) -> ProgramResult {
        if output.len() < size_of::<StakePool>() {
            return Err(ProgramError::InvalidAccountData);
        }
        #[allow(clippy::cast_ptr_alignment)]
        let value = unsafe { &mut *(&mut output[0] as *mut u8 as *mut StakePool) };
        *value = *self;

        Ok(())
    }
}
