//! State transition types

use crate::error::StakePoolError;
use crate::processor::Processor;
use solana_program::{account_info::{ AccountInfo}, entrypoint::ProgramResult, msg, program_error::ProgramError, pubkey::Pubkey};
use std::convert::{TryFrom, TryInto};
use std::mem::size_of;

pub const MAX_VALIDATORS: usize = 100;

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
    /// Pool Mint
    pub pool_mint:            Pubkey,
    /// Owner fee account
    pub owner_fee_account:    Pubkey,
    /// Credit reserve
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

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValidatorStakeList {
    /// Validator stake list version
    pub version: u8,
    /// List of all validator stake accounts and their info
    pub validators: Vec<ValidatorStakeInfo>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ValidatorStakeInfo {
    /// Validator account pubkey
    pub validator_account: Pubkey,

    /// Account balance in lamports
    pub balance: u64,

    /// Last epoch balance field was updated
    pub last_update_epoch: u64,

    /// Stake account count
    pub stake_count: u32,
}

impl ValidatorStakeList {
    /// Length of ValidatorStakeList data when serialized
    pub const LEN: usize = Self::HEADER_LEN + ValidatorStakeInfo::LEN * MAX_VALIDATORS;

    /// Header length
    pub const HEADER_LEN: usize = size_of::<u8>() + size_of::<u16>();

    /// Version of validator stake list
    pub const VALIDATOR_STAKE_LIST_VERSION: u8 = 1;

    /// Check if contains validator with particular pubkey
    pub fn contains(&self, validator: &Pubkey) -> bool {
        self.validators
            .iter()
            .any(|x| x.validator_account == *validator)
    }

    /// Check if contains validator with particular pubkey (mutable)
    pub fn find_mut(&mut self, validator: &Pubkey) -> Option<&mut ValidatorStakeInfo> {
        self.validators
            .iter_mut()
            .find(|x| x.validator_account == *validator)
    }
    /// Check if contains validator with particular pubkey (immutable)
    pub fn find(&self, validator: &Pubkey) -> Option<&ValidatorStakeInfo> {
        self.validators
            .iter()
            .find(|x| x.validator_account == *validator)
    }

    /// Check if validator stake list is initialized
    pub fn is_initialized(&self) -> bool {
        self.version > 0
    }

    /// Deserializes a byte buffer into a ValidatorStakeList.
    pub fn deserialize(input: &[u8]) -> Result<Self, ProgramError> {
        if input.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }

        if input[0] == 0 {
            return Ok(ValidatorStakeList {
                version: 0,
                validators: vec![],
            });
        }

        let number_of_validators: usize = u16::from_le_bytes(
            input[1..3]
                .try_into()
                .or(Err(ProgramError::InvalidAccountData))?,
        ) as usize;
        if number_of_validators > MAX_VALIDATORS {
            return Err(ProgramError::InvalidAccountData);
        }
        let mut validators: Vec<ValidatorStakeInfo> = Vec::with_capacity(number_of_validators + 1);

        let mut from = Self::HEADER_LEN;
        let mut to = from + ValidatorStakeInfo::LEN;
        for _ in 0..number_of_validators {
            validators.push(ValidatorStakeInfo::deserialize(&input[from..to])?);
            from += ValidatorStakeInfo::LEN;
            to += ValidatorStakeInfo::LEN;
        }
        Ok(ValidatorStakeList {
            version: input[0],
            validators,
        })
    }

    /// Serializes ValidatorStakeList into a byte buffer.
    pub fn serialize(&self, output: &mut [u8]) -> ProgramResult {
        if output.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        if self.validators.len() > MAX_VALIDATORS {
            return Err(ProgramError::InvalidAccountData);
        }
        output[0] = self.version;
        output[1..3].copy_from_slice(&u16::to_le_bytes(self.validators.len() as u16));
        let mut from = Self::HEADER_LEN;
        let mut to = from + ValidatorStakeInfo::LEN;
        for validator in &self.validators {
            validator.serialize(&mut output[from..to])?;
            from += ValidatorStakeInfo::LEN;
            to += ValidatorStakeInfo::LEN;
        }
        Ok(())
    }
}

impl ValidatorStakeInfo {
    /// Length of ValidatorStakeInfo data when serialized
    pub const LEN: usize = size_of::<ValidatorStakeInfo>();

    /// Deserializes a byte buffer into a ValidatorStakeInfo.
    pub fn deserialize(input: &[u8]) -> Result<Self, ProgramError> {
        if input.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        #[allow(clippy::cast_ptr_alignment)]
        let stake_info: &ValidatorStakeInfo =
            unsafe { &*(&input[0] as *const u8 as *const ValidatorStakeInfo) };
        Ok(*stake_info)
    }

    /// Serializes ValidatorStakeInfo into a byte buffer.
    pub fn serialize(&self, output: &mut [u8]) -> ProgramResult {
        if output.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }

        #[allow(clippy::cast_ptr_alignment)]
        let value = unsafe { &mut *(&mut output[0] as *mut u8 as *mut ValidatorStakeInfo) };
        *value = *self;
        Ok(())
    }

    /// Stake account address for validator
    pub fn stake_address(
        &self,
        program_id: &Pubkey,
        stake_pool: &Pubkey,
        index: u32,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                &self.validator_account.to_bytes()[..32],
                &stake_pool.to_bytes()[..32],
                &unsafe { std::mem::transmute::<u32, [u8; 4]>(index) },
            ],
            program_id,
        )
    }

    /// Checks if validator stake account is a proper program address
    pub fn check_validator_stake_address(
        &self,
        program_id: &Pubkey,
        stake_pool: &Pubkey,
        index: u32,
        stake_account_pubkey: &Pubkey,
    ) -> Result<u8, ProgramError> {
        // Check stake account address validity
        let (expected_stake_address, bump_seed) =
            self.stake_address(&program_id, &stake_pool, index);
        if *stake_account_pubkey != expected_stake_address {
            msg!(
                "Invalid {} stake account {} for validator {}",
                index,
                stake_account_pubkey,
                self.validator_account
            );
            msg!("Expected {}", expected_stake_address);
            return Err(StakePoolError::InvalidStakeAccountAddress.into());
        }
        Ok(bump_seed)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InitArgs {
    /// Fee paid to the owner in pool tokens
    pub fee: Fee,
}