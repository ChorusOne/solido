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
    pub lsol_mint_program: Pubkey,
    /// Total Lido tokens in circulation
    pub lsol_total_shares: u64,
    /// Holder of tokens in Lido's underlying stake pool
    pub pool_token_to: Pubkey,
    /// Fee distribution state, set and modified by the manager
    pub fee_distribution: Pubkey,
    /// Token program id associated with Lido's token
    pub token_program_id: Pubkey,

    /// Bumb seeds for signing messages on behalf of the authority
    pub sol_reserve_authority_bump_seed: u8,
    pub deposit_authority_bump_seed: u8,
    pub token_reserve_authority_bump_seed: u8,
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
                .checked_mul(self.lsol_total_shares as u128)?
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
        lsol_mint_key: &Pubkey,
    ) -> ProgramResult {
        if &self.manager != manager_key {
            return Err(LidoError::InvalidOwner.into());
        }
        if &self.stake_pool_account != stakepool_key {
            return Err(LidoError::InvalidStakePool.into());
        }

        if &self.lsol_mint_program != lsol_mint_key {
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
    address: Pubkey,
    amount: u64,
}

impl ValidatorCreditAccounts {
    fn new(max_validators: u32) -> Self {
        Self {
            max_validators,
            validator_accounts: vec![ValidatorCredit::default(); max_validators as usize],
        }
    }
    pub fn maximum_byte_capacity(buffer_size: usize) -> usize {
        return buffer_size.saturating_sub(8) / 40;
    }
    fn add(&mut self, address: Pubkey) -> Result<(), LidoError> {
        if self.validator_accounts.len() == self.max_validators as usize {
            return Err(LidoError::MaximumValidatorsExceeded);
        }
        self.validator_accounts.push(ValidatorCredit {
            address: address,
            amount: 0,
        });
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct FeeDistribution {
    pub insurance_fee_numerator: u64,
    pub treasure_fee_numerator: u64,
    pub validator_fee_numerator: u64,
    pub manager_fee_numerator: u64,
    pub denominator: u64,

    pub insurance_account: Pubkey,
    pub treasure_account: Pubkey,
    pub manager_account: Pubkey,
    pub validator_list_account: Pubkey,
}

impl FeeDistribution {
    /// Checks
    pub fn check_sum(&self) -> Result<(), LidoError> {
        if self.insurance_fee_numerator
            + self.treasure_fee_numerator
            + self.validator_fee_numerator
            + self.manager_fee_numerator
            != self.denominator
            || self.denominator == 0
        {
            msg!("Fee numerators do not add up to denominator or denominator is 0");
            return Err(LidoError::InvalidFeeAmount);
        }
        Ok(())
    }
}

#[test]
fn test_n_val() {
    let n_validators: u64 = 10000;
    let size = get_instance_packed_len(&ValidatorCreditAccounts::new(n_validators as u32)).unwrap();

    assert_eq!(
        ValidatorCreditAccounts::maximum_byte_capacity(size) as u64,
        n_validators
    );
}

#[cfg(test)]
mod test_lido {
    use super::*;
    use solana_program::program_error::ProgramError;
    use solana_sdk::signature::{Keypair, Signer};

    #[test]
    fn test_lido_members_initialized() {
        let mut members = LidoMembers::new(10);
        assert!(!members.is_initialized());
        members.account_type = LidoAccountType::Initialized;

        assert!(members.is_initialized())
    }

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
    fn test_pool_tokens_when_lsol_total_shares_is_default() {
        let lido = Lido::default();

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(200, 100);

        assert_eq!(pool_tokens_for_deposit, Some(0));
    }

    #[test]
    fn test_pool_tokens_when_lsol_total_shares_is_increased() {
        let mut lido = Lido::default();
        lido.lsol_total_shares = 120;

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(200, 40);

        assert_eq!(pool_tokens_for_deposit, Some(600));
    }

    #[test]
    fn test_pool_tokens_when_stake_lamports_is_zero() {
        let mut lido = Lido::default();
        lido.lsol_total_shares = 120;

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
            &lido.lsol_mint_program,
        );

        let expect: ProgramError = LidoError::InvalidOwner.into();
        assert_eq!(err.err(), Some(expect));
    }

    #[test]
    fn test_lido_for_deposit_wrong_stakepool() {
        let lido = Lido::default();
        let other_stakepool = Keypair::new();

        let err = lido.check_lido_for_deposit(
            &lido.owner,
            &other_stakepool.pubkey(),
            &lido.lsol_mint_program,
        );

        let expect: ProgramError = LidoError::InvalidStakePool.into();
        assert_eq!(expect, err.err().unwrap());
    }

    #[test]
    fn test_lido_for_deposit_wrong_mint_program() {
        let lido = Lido::default();
        let other_mint = Keypair::new();

        let err = lido.check_lido_for_deposit(
            &lido.owner,
            &lido.stake_pool_account,
            &other_mint.pubkey(),
        );

        let expect: ProgramError = LidoError::InvalidToken.into();
        assert_eq!(expect, err.err().unwrap());
    }
}
