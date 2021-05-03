//! State transition types

use {
    crate::error::StakePoolError,
    borsh::{BorshDeserialize, BorshSchema, BorshSerialize},
    solana_program::{account_info::AccountInfo, msg, program_error::ProgramError, pubkey::Pubkey},
    spl_math::checked_ceil_div::CheckedCeilDiv,
    std::convert::TryFrom,
};

/// Enum representing the account type managed by the program
#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub enum AccountType {
    /// If the account has not been initialized, the enum will be 0
    Uninitialized,
    /// Stake pool
    StakePool,
    /// Validator stake list
    ValidatorList,
}

impl Default for AccountType {
    fn default() -> Self {
        AccountType::Uninitialized
    }
}

/// Initialized program details.
#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct StakePool {
    /// Account type, must be StakePool currently
    pub account_type: AccountType,

    /// Manager authority, allows for updating the staker, manager, and fee account
    pub manager: Pubkey,

    /// Staker authority, allows for adding and removing validators, and managing stake
    /// distribution
    pub staker: Pubkey,

    /// Deposit authority
    ///
    /// If a depositor pubkey is specified on initialization, then deposits must be
    /// signed by this authority. If no deposit authority is specified,
    /// then the stake pool will default to the result of:
    /// `Pubkey::find_program_address(
    ///     &[&stake_pool_address.to_bytes()[..32], b"deposit"],
    ///     program_id,
    /// )`
    pub deposit_authority: Pubkey,

    /// Withdrawal authority bump seed
    /// for `create_program_address(&[state::StakePool account, "withdrawal"])`
    pub withdraw_bump_seed: u8,

    /// Validator stake list storage account
    pub validator_list: Pubkey,

    /// Reserve stake account, holds deactivated stake
    pub reserve_stake: Pubkey,

    /// Pool Mint
    pub pool_mint: Pubkey,

    /// Manager fee account
    pub manager_fee_account: Pubkey,

    /// Pool token program id
    pub token_program_id: Pubkey,

    /// Total stake under management.
    /// Note that if `last_update_epoch` does not match the current epoch then
    /// this field may not be accurate
    pub total_stake_lamports: u64,

    /// Total supply of pool tokens (should always match the supply in the Pool Mint)
    pub pool_token_supply: u64,

    /// Last epoch the `total_stake_lamports` field was updated
    pub last_update_epoch: u64,

    /// Fee applied to deposits
    pub fee: Fee,
}
impl StakePool {
    /// calculate the pool tokens that should be minted for a deposit of `stake_lamports`
    pub fn calc_pool_tokens_for_deposit(&self, stake_lamports: u64) -> Option<u64> {
        if self.total_stake_lamports == 0 || self.pool_token_supply == 0 {
            return Some(stake_lamports);
        }
        u64::try_from(
            (stake_lamports as u128)
                .checked_mul(self.pool_token_supply as u128)?
                .checked_div(self.total_stake_lamports as u128)?,
        )
        .ok()
    }
    /// calculate the pool tokens that should be burned for a withdrawal of `stake_lamports`
    pub fn calc_pool_tokens_for_withdraw(&self, stake_lamports: u64) -> Option<u64> {
        let (quotient, _) = (stake_lamports as u128)
            .checked_mul(self.pool_token_supply as u128)?
            .checked_ceil_div(self.total_stake_lamports as u128)?;
        u64::try_from(quotient).ok()
    }

    /// calculate lamports amount on withdrawal
    pub fn calc_lamports_withdraw_amount(&self, pool_tokens: u64) -> Option<u64> {
        u64::try_from(
            (pool_tokens as u128)
                .checked_mul(self.total_stake_lamports as u128)?
                .checked_div(self.pool_token_supply as u128)?,
        )
        .ok()
    }
    /// calculate the fee in pool tokens that goes to the manager
    pub fn calc_fee_amount(&self, reward_lamports: u64) -> Option<u64> {
        if self.fee.denominator == 0 {
            return Some(0);
        }
        let pool_amount = self.calc_pool_tokens_for_deposit(reward_lamports)?;
        u64::try_from(
            (pool_amount as u128)
                .checked_mul(self.fee.numerator as u128)?
                .checked_div(self.fee.denominator as u128)?,
        )
        .ok()
    }

    /// Checks that the withdraw or deposit authority is valid
    fn check_authority(
        authority_address: &Pubkey,
        program_id: &Pubkey,
        stake_pool_address: &Pubkey,
        authority_seed: &[u8],
        bump_seed: u8,
    ) -> Result<(), ProgramError> {
        let expected_address = Pubkey::create_program_address(
            &[
                &stake_pool_address.to_bytes()[..32],
                authority_seed,
                &[bump_seed],
            ],
            program_id,
        )?;

        if *authority_address == expected_address {
            Ok(())
        } else {
            msg!(
                "Incorrect authority provided, expected {}, received {}",
                expected_address,
                authority_address
            );
            Err(StakePoolError::InvalidProgramAddress.into())
        }
    }

    /// Checks that the withdraw authority is valid
    pub(crate) fn check_authority_withdraw(
        &self,
        withdraw_authority: &Pubkey,
        program_id: &Pubkey,
        stake_pool_address: &Pubkey,
    ) -> Result<(), ProgramError> {
        Self::check_authority(
            withdraw_authority,
            program_id,
            stake_pool_address,
            crate::AUTHORITY_WITHDRAW,
            self.withdraw_bump_seed,
        )
    }
    /// Checks that the deposit authority is valid
    pub(crate) fn check_deposit_authority(
        &self,
        deposit_authority: &Pubkey,
    ) -> Result<(), ProgramError> {
        if self.deposit_authority == *deposit_authority {
            Ok(())
        } else {
            Err(StakePoolError::InvalidProgramAddress.into())
        }
    }

    /// Check staker validity and signature
    pub(crate) fn check_mint(&self, mint_info: &AccountInfo) -> Result<(), ProgramError> {
        if *mint_info.key != self.pool_mint {
            Err(StakePoolError::WrongPoolMint.into())
        } else {
            Ok(())
        }
    }

    /// Check manager validity and signature
    pub(crate) fn check_manager(&self, manager_info: &AccountInfo) -> Result<(), ProgramError> {
        if *manager_info.key != self.manager {
            msg!(
                "Incorrect manager provided, expected {}, received {}",
                self.manager,
                manager_info.key
            );
            return Err(StakePoolError::WrongManager.into());
        }
        if !manager_info.is_signer {
            msg!("Manager signature missing");
            return Err(StakePoolError::SignatureMissing.into());
        }
        Ok(())
    }

    /// Check staker validity and signature
    pub(crate) fn check_staker(&self, staker_info: &AccountInfo) -> Result<(), ProgramError> {
        if *staker_info.key != self.staker {
            msg!(
                "Incorrect staker provided, expected {}, received {}",
                self.staker,
                staker_info.key
            );
            return Err(StakePoolError::WrongStaker.into());
        }
        if !staker_info.is_signer {
            msg!("Staker signature missing");
            return Err(StakePoolError::SignatureMissing.into());
        }
        Ok(())
    }

    /// Check the validator list is valid
    pub fn check_validator_list(
        &self,
        validator_list_info: &AccountInfo,
    ) -> Result<(), ProgramError> {
        if *validator_list_info.key != self.validator_list {
            msg!(
                "Invalid validator list provided, expected {}, received {}",
                self.validator_list,
                validator_list_info.key
            );
            Err(StakePoolError::InvalidValidatorStakeList.into())
        } else {
            Ok(())
        }
    }

    /// Check the validator list is valid
    pub fn check_reserve_stake(
        &self,
        reserve_stake_info: &AccountInfo,
    ) -> Result<(), ProgramError> {
        if *reserve_stake_info.key != self.reserve_stake {
            msg!(
                "Invalid reserve stake provided, expected {}, received {}",
                self.reserve_stake,
                reserve_stake_info.key
            );
            Err(StakePoolError::InvalidProgramAddress.into())
        } else {
            Ok(())
        }
    }

    /// Check if StakePool is actually initialized as a stake pool
    pub fn is_valid(&self) -> bool {
        self.account_type == AccountType::StakePool
    }

    /// Check if StakePool is currently uninitialized
    pub fn is_uninitialized(&self) -> bool {
        self.account_type == AccountType::Uninitialized
    }
}

/// Storage list for all validator stake accounts in the pool.
#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct ValidatorList {
    /// Account type, must be ValidatorList currently
    pub account_type: AccountType,

    /// Maximum allowable number of validators
    pub max_validators: u32,

    /// List of stake info for each validator in the pool
    pub validators: Vec<ValidatorStakeInfo>,
}

/// Status of the stake account in the validator list, for accounting
#[derive(Copy, Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub enum StakeStatus {
    /// Stake account is active, there may be a transient stake as well
    Active,
    /// Only transient stake account exists, when a transient stake is
    /// deactivating during validator removal
    DeactivatingTransient,
    /// No more validator stake accounts exist, entry ready for removal during
    /// `UpdateStakePoolBalance`
    ReadyForRemoval,
}

impl Default for StakeStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Information about the singe validator stake account
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct ValidatorStakeInfo {
    /// Status of the validator stake account
    pub status: StakeStatus,

    /// Validator vote account address
    pub vote_account_address: Pubkey,

    /// Amount of stake delegated to this validator
    /// Note that if `last_update_epoch` does not match the current epoch then this field may not
    /// be accurate
    pub stake_lamports: u64,

    /// Last epoch the `stake_lamports` field was updated
    pub last_update_epoch: u64,
}

impl ValidatorList {
    /// Create an empty instance containing space for `max_validators`
    pub fn new(max_validators: u32) -> Self {
        Self {
            account_type: AccountType::ValidatorList,
            max_validators,
            validators: vec![ValidatorStakeInfo::default(); max_validators as usize],
        }
    }

    /// Calculate the number of validator entries that fit in the provided length
    pub fn calculate_max_validators(buffer_length: usize) -> usize {
        let header_size = 1 + 4 + 4;
        buffer_length.saturating_sub(header_size) / 49
    }

    /// Check if contains validator with particular pubkey
    pub fn contains(&self, vote_account_address: &Pubkey) -> bool {
        self.validators
            .iter()
            .any(|x| x.vote_account_address == *vote_account_address)
    }

    /// Check if contains validator with particular pubkey
    pub fn find_mut(&mut self, vote_account_address: &Pubkey) -> Option<&mut ValidatorStakeInfo> {
        self.validators
            .iter_mut()
            .find(|x| x.vote_account_address == *vote_account_address)
    }
    /// Check if contains validator with particular pubkey
    pub fn find(&self, vote_account_address: &Pubkey) -> Option<&ValidatorStakeInfo> {
        self.validators
            .iter()
            .find(|x| x.vote_account_address == *vote_account_address)
    }

    /// Check if validator stake list is actually initialized as a validator stake list
    pub fn is_valid(&self) -> bool {
        self.account_type == AccountType::ValidatorList
    }

    /// Check if the validator stake list is uninitialized
    pub fn is_uninitialized(&self) -> bool {
        self.account_type == AccountType::Uninitialized
    }
}

/// Fee rate as a ratio, minted on `UpdateStakePoolBalance` as a proportion of
/// the rewards
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Fee {
    /// denominator of the fee ratio
    pub denominator: u64,
    /// numerator of the fee ratio
    pub numerator: u64,
}

#[cfg(test)]
mod test {
    use {
        super::*,
        crate::borsh::{get_instance_packed_len, try_from_slice_unchecked},
        proptest::prelude::*,
        solana_program::borsh::get_packed_len,
    };

    #[test]
    fn test_state_packing() {
        let max_validators = 10_000;
        let size = get_instance_packed_len(&ValidatorList::new(max_validators)).unwrap();
        // Not initialized
        let stake_list = ValidatorList {
            account_type: AccountType::Uninitialized,
            max_validators: 0,
            validators: vec![],
        };
        let mut byte_vec = vec![0u8; size];
        let mut bytes = byte_vec.as_mut_slice();
        stake_list.serialize(&mut bytes).unwrap();
        let stake_list_unpacked = try_from_slice_unchecked::<ValidatorList>(&byte_vec).unwrap();
        assert_eq!(stake_list_unpacked, stake_list);

        // Empty
        let stake_list = ValidatorList {
            account_type: AccountType::ValidatorList,
            max_validators: 0,
            validators: vec![],
        };
        let mut byte_vec = vec![0u8; size];
        let mut bytes = byte_vec.as_mut_slice();
        stake_list.serialize(&mut bytes).unwrap();
        let stake_list_unpacked = try_from_slice_unchecked::<ValidatorList>(&byte_vec).unwrap();
        assert_eq!(stake_list_unpacked, stake_list);

        // With several accounts
        let stake_list = ValidatorList {
            account_type: AccountType::ValidatorList,
            max_validators,
            validators: vec![
                ValidatorStakeInfo {
                    status: StakeStatus::Active,
                    vote_account_address: Pubkey::new_from_array([1; 32]),
                    stake_lamports: 123456789,
                    last_update_epoch: 987654321,
                },
                ValidatorStakeInfo {
                    status: StakeStatus::DeactivatingTransient,
                    vote_account_address: Pubkey::new_from_array([2; 32]),
                    stake_lamports: 998877665544,
                    last_update_epoch: 11223445566,
                },
                ValidatorStakeInfo {
                    status: StakeStatus::ReadyForRemoval,
                    vote_account_address: Pubkey::new_from_array([3; 32]),
                    stake_lamports: 0,
                    last_update_epoch: 999999999999999,
                },
            ],
        };
        let mut byte_vec = vec![0u8; size];
        let mut bytes = byte_vec.as_mut_slice();
        stake_list.serialize(&mut bytes).unwrap();
        let stake_list_unpacked = try_from_slice_unchecked::<ValidatorList>(&byte_vec).unwrap();
        assert_eq!(stake_list_unpacked, stake_list);
    }

    proptest! {
        #[test]
        fn stake_list_size_calculation(test_amount in 0..=100_000_u32) {
            let validators = ValidatorList::new(test_amount);
            let size = get_instance_packed_len(&validators).unwrap();
            assert_eq!(ValidatorList::calculate_max_validators(size), test_amount as usize);
            assert_eq!(ValidatorList::calculate_max_validators(size.saturating_add(1)), test_amount as usize);
            assert_eq!(ValidatorList::calculate_max_validators(size.saturating_add(get_packed_len::<ValidatorStakeInfo>())), (test_amount + 1)as usize);
            assert_eq!(ValidatorList::calculate_max_validators(size.saturating_sub(1)), (test_amount.saturating_sub(1)) as usize);
        }
    }
}
