use crate::error::StakePoolError;
use crate::state::Fee;
use solana_program::{entrypoint::ProgramResult, msg, program_error::ProgramError, pubkey::Pubkey};
use std::convert::TryInto;
use std::mem::size_of;

pub const MAX_VALIDATORS: usize = 100;

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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_list_contains_validator() {
        let key =
            Pubkey::create_with_seed(&Pubkey::default(), "some seed", &Pubkey::default()).unwrap();
        let validator_stake_info = ValidatorStakeInfo {
            validator_account: key,
            balance: 300,
            last_update_epoch: 10,
            stake_count: 50,
        };

        let validator_list = ValidatorStakeList {
            version: 1,
            validators: vec![validator_stake_info],
        };

        assert!(validator_list.contains(&key));
        assert!(!validator_list.contains(&Pubkey::default()));
        assert!(validator_list.is_initialized())
    }

    #[test]
    fn test_list_find_validator() {
        let key =
            Pubkey::create_with_seed(&Pubkey::default(), "some seed", &Pubkey::default()).unwrap();
        let validator_stake_info = ValidatorStakeInfo {
            validator_account: key,
            balance: 300,
            last_update_epoch: 10,
            stake_count: 50,
        };

        let validator_list = ValidatorStakeList {
            version: 1,
            validators: vec![validator_stake_info],
        };

        assert!(validator_list.find(&key).is_some());
        assert!(validator_list.find(&Pubkey::default()).is_none());
    }

    #[test]
    fn test_find_stake_address() {
        let key =
            Pubkey::create_with_seed(&Pubkey::default(), "some seed", &Pubkey::default()).unwrap();
        let validator_stake_info = ValidatorStakeInfo {
            validator_account: key,
            balance: 300,
            last_update_epoch: 10,
            stake_count: 50,
        };

        let addr = validator_stake_info.stake_address(&Pubkey::default(), &Pubkey::default(), 32);

        let key = format!("{:?}", addr.0);
        assert_eq!(addr.1, 254);
        assert_eq!(key, "13P22UfKcDDa2N8PsFvsRHJGYD2SQSMJNN5SkpFJ2UV7");
    }
}
