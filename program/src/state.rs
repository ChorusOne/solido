//! State transition types

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::{entrypoint::ProgramResult, msg, pubkey::Pubkey};
use spl_stake_pool::borsh::get_instance_packed_len;
use std::convert::TryFrom;

use crate::error::LidoError;

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct Lido {
    /// Stake pool account associated with Lido
    pub stake_pool_account: Pubkey,
    /// Manager of the Lido program, able to execute administrative functions
    pub manager: Pubkey,
    /// Program in charge of minting Lido tokens
    pub st_sol_mint_program: Pubkey,
    /// Total Lido tokens in circulation
    pub st_sol_total_shares: u64,
    /// Holder of tokens in Lido's underlying stake pool
    pub stake_pool_token_holder: Pubkey,
    /// Fee distribution state, set and modified by the manager
    pub fee_distribution: Pubkey,
    /// Validator credits to take from the fee
    pub validator_credit_accounts: Pubkey,
    /// Token program id associated with Lido's token
    pub token_program_id: Pubkey,

    /// Bump seeds for signing messages on behalf of the authority
    pub sol_reserve_authority_bump_seed: u8,
    pub deposit_authority_bump_seed: u8,
    pub stake_pool_authority_bump_seed: u8,
    pub fee_manager_bump_seed: u8,
}

impl Lido {
    pub fn calc_pool_tokens_for_deposit(
        &self,
        stake_lamports: u64,
        total_lamports: u64,
    ) -> Option<u64> {
        if total_lamports == 0 {
            return Some(stake_lamports);
        }
        u64::try_from(
            (stake_lamports as u128)
                .checked_mul(self.st_sol_total_shares as u128)?
                .checked_div(total_lamports as u128)?,
        )
        .ok()
    }

    pub fn is_initialized(&self) -> ProgramResult {
        if self.stake_pool_account != Pubkey::default() {
            msg!("Provided lido already in use");
            Err(LidoError::AlreadyInUse.into())
        } else {
            Ok(())
        }
    }

    pub fn check_lido_for_deposit(
        &self,
        manager_key: &Pubkey,
        stakepool_key: &Pubkey,
        st_sol_mint_key: &Pubkey,
    ) -> ProgramResult {
        if &self.manager != manager_key {
            return Err(LidoError::InvalidOwner.into());
        }
        if &self.stake_pool_account != stakepool_key {
            return Err(LidoError::InvalidStakePool.into());
        }

        if &self.st_sol_mint_program != lsol_mint_key {
            return Err(LidoError::InvalidTokenMinter.into());
        }
        Ok(())
    }

    pub fn check_token_program_id(&self, token_program_id: &Pubkey) -> ProgramResult {
        if token_program_id != &self.token_program_id {
            return Err(LidoError::InvalidTokenProgram.into());
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct ValidatorCreditAccounts {
    pub validator_accounts: Vec<ValidatorCredit>,
    pub max_validators: u32,
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct ValidatorCredit {
    pub address: Pubkey,
    pub st_sol_amount: u64,
}

impl ValidatorCreditAccounts {
    pub fn new(max_validators: u32) -> Self {
        Self {
            max_validators,
            validator_accounts: vec![ValidatorCredit::default(); max_validators as usize],
        }
    }
    pub fn maximum_accounts(buffer_size: usize) -> usize {
        return buffer_size.saturating_sub(8) / 40;
    }
    fn add(&mut self, address: Pubkey) -> Result<(), LidoError> {
        if self.validator_accounts.len() == self.max_validators as usize {
            return Err(LidoError::MaximumValidatorsExceeded);
        }
        self.validator_accounts.push(ValidatorCredit {
            address: address,
            st_sol_amount: 0,
        });
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct FeeDistribution {
    pub insurance_fee_numerator: u64,
    pub treasury_fee_numerator: u64,
    pub validators_fee_numerator: u64,
    pub manager_fee_numerator: u64,
    pub denominator: u64,

    pub insurance_account: Pubkey,
    pub treasury_account: Pubkey,
    pub manager_account: Pubkey,
}

pub struct CalculatedTokenFeeAmount {
    pub insurance_amount: u64,
    pub treasury_amount: u64,
    pub each_validator_amount: u64,
    pub manager_amount: u64,
}

impl FeeDistribution {
    /// Checks
    pub fn check_sum(&self) -> Result<(), LidoError> {
        if self.insurance_fee_numerator
            + self.treasury_fee_numerator
            + self.validators_fee_numerator
            + self.manager_fee_numerator
            != self.denominator
            || self.denominator == 0
        {
            msg!("Fee numerators do not add up to denominator or denominator is 0");
            return Err(LidoError::InvalidFeeAmount);
        }
        Ok(())
    }
    /// Returns the amount of each
    pub fn calculate_token_amounts(
        &self,
        total_token_amount: u64,
        number_validators: u32,
    ) -> Result<CalculatedTokenFeeAmount, LidoError> {
        let insurance_amount = (total_token_amount as u128)
            .checked_mul(self.insurance_fee_numerator as u128)
            .ok_or(LidoError::CalculationFailure)?
            .checked_div(self.denominator as u128)
            .ok_or(LidoError::CalculationFailure)? as u64;
        let treasury_amount = (total_token_amount as u128)
            .checked_mul(self.treasury_fee_numerator as u128)
            .ok_or(LidoError::CalculationFailure)?
            .checked_div(self.denominator as u128)
            .ok_or(LidoError::CalculationFailure)? as u64;

        let validators_amount = (total_token_amount as u128)
            .checked_mul(self.validators_fee_numerator as u128)
            .ok_or(LidoError::CalculationFailure)?
            .checked_div(self.denominator as u128)
            .ok_or(LidoError::CalculationFailure)?;

        let each_validator_amount = validators_amount
            .checked_div(number_validators as u128)
            .ok_or(LidoError::CalculationFailure)? as u64;

        let manager_check = (total_token_amount as u128)
            .checked_mul(self.manager_fee_numerator as u128)
            .ok_or(LidoError::CalculationFailure)?
            .checked_div(self.denominator as u128)
            .ok_or(LidoError::CalculationFailure)? as u64;
        let manager_amount = total_token_amount
            - insurance_amount
            - treasury_amount
            - each_validator_amount
                .checked_mul(number_validators as u64)
                .ok_or(LidoError::CalculationFailure)?;
        // This should never happen
        if manager_amount < manager_check {
            msg!(
                "Manager is receiving an incorrect number of tokens {}, should get at least  {}",
                manager_amount,
                manager_check,
            );
            return Err(LidoError::CalculationFailure);
        }
        Ok(CalculatedTokenFeeAmount {
            insurance_amount,
            treasury_amount,
            each_validator_amount,
            manager_amount,
        })
    }
}

#[test]
fn test_n_val() {
    let n_validators: u64 = 10000;
    let size = get_instance_packed_len(&ValidatorCreditAccounts::new(n_validators as u32)).unwrap();

    assert_eq!(
        ValidatorCreditAccounts::maximum_accounts(size) as u64,
        n_validators
    );
}

#[cfg(test)]
mod test_lido {
    use super::*;
    use solana_program::program_error::ProgramError;
    use solana_sdk::signature::{Keypair, Signer};

    #[test]
    fn lido_initialized() {
        let lido = Lido::default();

        assert!(lido.is_initialized().is_ok());
    }

    #[test]
    fn test_pool_tokens_when_total_lamports_is_zero() {
        let lido = Lido::default();

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(123, 0);

        assert_eq!(pool_tokens_for_deposit, Some(123));
    }

    #[test]
    fn test_pool_tokens_when_st_sol_total_shares_is_default() {
        let lido = Lido::default();

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(200, 100);

        assert_eq!(pool_tokens_for_deposit, Some(0));
    }

    #[test]
    fn test_pool_tokens_when_st_sol_total_shares_is_increased() {
        let mut lido = Lido::default();
        lido.st_sol_total_shares = 120;

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(200, 40);

        assert_eq!(pool_tokens_for_deposit, Some(600));
    }

    #[test]
    fn test_pool_tokens_when_stake_lamports_is_zero() {
        let mut lido = Lido::default();
        lido.st_sol_total_shares = 120;

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(0, 40);

        assert_eq!(pool_tokens_for_deposit, Some(0));
    }

    #[test]
    fn test_lido_correct_program_id() {
        let lido = Lido::default();

        assert!(lido.check_token_program_id(&lido.token_program_id).is_ok());
    }

    #[test]
    fn test_lido_wrong_program_id() {
        let lido = Lido::default();
        let prog_id = Keypair::new();

        let err = lido.check_token_program_id(&prog_id.pubkey());
        let expect: ProgramError = LidoError::InvalidTokenProgram.into();
        assert_eq!(expect, err.err().unwrap());
    }

    #[test]
    fn test_lido_for_deposit_wrong_owner() {
        let lido = Lido::default();
        let other_owner = Keypair::new();

        let err = lido.check_lido_for_deposit(
            &other_owner.pubkey(),
            &lido.stake_pool_account,
            &lido.st_sol_mint_program,
        );

        let expect: ProgramError = LidoError::InvalidOwner.into();
        assert_eq!(err.err(), Some(expect));
    }

    #[test]
    fn test_lido_for_deposit_wrong_stakepool() {
        let lido = Lido::default();
        let other_stakepool = Keypair::new();

        let err = lido.check_lido_for_deposit(
            &lido.manager,
            &other_stakepool.pubkey(),
            &lido.st_sol_mint_program,
        );

        let expect: ProgramError = LidoError::InvalidStakePool.into();
        assert_eq!(expect, err.err().unwrap());
    }

    #[test]
    fn test_lido_for_deposit_wrong_mint_program() {
        let lido = Lido::default();
        let other_mint = Keypair::new();

        let err = lido.check_lido_for_deposit(
            &lido.manager,
            &lido.stake_pool_account,
            &other_mint.pubkey(),
        );

        let expect: ProgramError = LidoError::InvalidTokenMinter.into();
        assert_eq!(expect, err.err().unwrap());
    }
    #[test]
    fn test_fee_distribution() {
        let fee_distribution = FeeDistribution {
            insurance_fee_numerator: 3,
            treasury_fee_numerator: 3,
            validators_fee_numerator: 2,
            manager_fee_numerator: 1,
            denominator: 9,

            insurance_account: Pubkey::default(),
            treasury_account: Pubkey::default(),
            manager_account: Pubkey::default(),
        };
        assert!(fee_distribution.check_sum().is_ok());
        let amount: u64 = 1000;
        let number_validators: u32 = 10;

        let insurance_amount =
            (amount * fee_distribution.insurance_fee_numerator) / fee_distribution.denominator;
        let treasury_amount =
            (amount * fee_distribution.treasury_fee_numerator) / fee_distribution.denominator;
        let validators_amount =
            (amount * fee_distribution.validators_fee_numerator) / fee_distribution.denominator;

        let each_validator_amount = validators_amount / number_validators as u64;
        let manager_amount = amount
            - insurance_amount
            - treasury_amount
            - each_validator_amount * number_validators as u64;

        let distributions = fee_distribution
            .calculate_token_amounts(amount, number_validators)
            .unwrap();
        assert_eq!(insurance_amount, distributions.insurance_amount);
        assert_eq!(treasury_amount, distributions.treasury_amount);
        assert_eq!(each_validator_amount, distributions.each_validator_amount);
        assert_eq!(manager_amount, distributions.manager_amount);
        assert_eq!(
            manager_amount
                + each_validator_amount * number_validators as u64
                + treasury_amount
                + insurance_amount,
            amount,
        );
    }
}
