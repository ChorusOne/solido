//! State transition types

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_pack::Pack, pubkey::Pubkey,
};
use std::{
    convert::TryFrom,
    fmt,
    ops::{Add, Div, Mul, Sub},
};

use crate::error::LidoError;

#[derive(
    Default,
    Copy,
    Clone,
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
    BorshDeserialize,
    BorshSerialize,
    BorshSchema,
)]
pub struct StLamports(pub u64);
pub struct StakePoolTokenLamports(pub u64);

impl fmt::Debug for StLamports {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}.{:0>9} stSOL",
            self.0 / 1_000_000_000,
            self.0 % 1_000_000_000,
        )
    }
}

#[derive(Copy, Clone)]
pub struct Rational {
    pub numerator: u64,
    pub denominator: u64,
}

impl Mul<Rational> for StLamports {
    type Output = Option<StLamports>;
    fn mul(self, other: Rational) -> Option<StLamports> {
        // This multiplication cannot overflow, because we expand the u64s
        // into u128, and u64::MAX * u64::MAX < u128::MAX.
        let result_u128 = ((self.0 as u128) * (other.numerator as u128))
            .checked_div(other.denominator as u128)?;
        Some(StLamports(u64::try_from(result_u128).ok()?))
    }
}

impl Mul<u64> for StLamports {
    type Output = Option<StLamports>;
    fn mul(self, other: u64) -> Option<StLamports> {
        Some(StLamports(self.0.checked_mul(other)?))
    }
}

impl Div<u64> for StLamports {
    type Output = Option<StLamports>;
    fn div(self, other: u64) -> Option<StLamports> {
        Some(StLamports(self.0.checked_div(other)?))
    }
}

impl Sub<StLamports> for StLamports {
    type Output = Option<StLamports>;
    fn sub(self, other: StLamports) -> Option<StLamports> {
        Some(StLamports(self.0.checked_sub(other.0)?))
    }
}

impl Add<StLamports> for StLamports {
    type Output = Option<StLamports>;
    fn add(self, other: StLamports) -> Option<StLamports> {
        Some(StLamports(self.0.checked_add(other.0)?))
    }
}

/// Constant size of header size = 5 public keys, 1 u64, 4 u8
pub const LIDO_CONSTANT_HEADER_SIZE: usize = 5 * 32 + 8 + 4;
/// Constant size of fee struct: 3 public keys + 4 u32
pub const LIDO_CONSTANT_FEE_SIZE: usize = 3 * 32 + 4 * 4;
/// Constant size of Lido
pub const LIDO_CONSTANT_SIZE: usize = LIDO_CONSTANT_HEADER_SIZE + LIDO_CONSTANT_FEE_SIZE;
#[repr(C)]
#[derive(Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct Lido {
    /// Stake pool account associated with Lido
    pub stake_pool_account: Pubkey,
    /// Manager of the Lido program, able to execute administrative functions
    pub manager: Pubkey,
    /// Program in charge of minting Lido tokens
    pub st_sol_mint_program: Pubkey,
    /// Total Lido tokens in circulation
    pub st_sol_total_shares: StLamports,
    /// Holder of tokens in Lido's underlying stake pool
    pub stake_pool_token_holder: Pubkey,
    /// Token program id associated with Lido's token
    pub token_program_id: Pubkey,

    /// Bump seeds for signing messages on behalf of the authority
    pub sol_reserve_authority_bump_seed: u8,
    pub deposit_authority_bump_seed: u8,
    pub stake_pool_authority_bump_seed: u8,
    pub fee_manager_bump_seed: u8,

    /// Fees
    pub fee_distribution: FeeDistribution,
    pub fee_recipients: FeeRecipients,
}

impl Lido {
    pub fn calc_pool_tokens_for_deposit(
        &self,
        stake_lamports: u64,
        total_lamports: u64,
    ) -> Option<StLamports> {
        if total_lamports == 0 {
            return Some(StLamports(stake_lamports));
        }
        let ratio = Rational {
            numerator: self.st_sol_total_shares.0,
            denominator: total_lamports,
        };
        StLamports(stake_lamports) * ratio
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

        if &self.st_sol_mint_program != st_sol_mint_key {
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

    /// Checks if the token is minted by the `minter_program`
    pub fn check_valid_minter_program(
        minter_program: &Pubkey,
        token_account_info: &AccountInfo,
    ) -> Result<(), LidoError> {
        if &spl_token::state::Account::unpack_from_slice(&token_account_info.data.borrow())
            .map_err(|_| LidoError::InvalidFeeRecipient)?
            .mint
            != minter_program
        {
            return Err(LidoError::InvalidFeeRecipient);
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct ValidatorCreditAccounts {
    pub max_validators: u32,
    pub validator_accounts: Vec<ValidatorCredit>,
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct ValidatorCredit {
    pub stake_address: Pubkey,
    pub token_address: Pubkey,
    pub st_sol_amount: StLamports,
}

impl ValidatorCreditAccounts {
    pub fn new(max_validators: u32) -> Self {
        Self {
            max_validators,
            validator_accounts: vec![ValidatorCredit::default(); max_validators as usize],
        }
    }
    pub fn maximum_accounts(buffer_size: usize) -> usize {
        // 8 bytes: 4 bytes for `max_validators` + 4 bytes for number of validators in vec
        // 32*2+8 bytes for each validator = 2 public keys + amount in StLamports
        buffer_size.saturating_sub(8) / (32 * 2 + 8)
    }
    pub fn add(&mut self, stake_address: Pubkey, token_address: Pubkey) -> Result<(), LidoError> {
        if self.validator_accounts.len() == self.max_validators as usize {
            return Err(LidoError::MaximumValidatorsExceeded);
        }
        // Adds if vote account is different
        if !self
            .validator_accounts
            .iter()
            .any(|v| v.stake_address == stake_address)
        {
            self.validator_accounts.push(ValidatorCredit {
                stake_address,
                token_address,
                st_sol_amount: StLamports(0),
            });
        }
        Ok(())
    }
}

/// Determines how fees are split up among these parties, represented as the
/// number of parts of the total. For example, if each party has 1 part, then
/// they all get an equal share of the fee.
#[derive(Clone, Default, PartialEq, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct FeeDistribution {
    pub insurance_fee: u32,
    pub treasury_fee: u32,
    pub validation_fee: u32,
    pub manager_fee: u32,
}

/// Specifies the fee recipients, accounts that should be created by Lido's minter
#[derive(Clone, Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct FeeRecipients {
    pub insurance_account: Pubkey,
    pub treasury_account: Pubkey,
    pub manager_account: Pubkey,
    pub validator_credit_accounts: ValidatorCreditAccounts,
}

impl FeeDistribution {
    pub fn sum(&self) -> u64 {
        // These adds don't overflow because we widen from u32 to u64 first.
        self.insurance_fee as u64
            + self.treasury_fee as u64
            + self.validation_fee as u64
            + self.manager_fee as u64
    }
    pub fn insurance_fraction(&self) -> Rational {
        Rational {
            numerator: self.insurance_fee as u64,
            denominator: self.sum(),
        }
    }
    pub fn treasury_fraction(&self) -> Rational {
        Rational {
            numerator: self.treasury_fee as u64,
            denominator: self.sum(),
        }
    }
    pub fn validation_fraction(&self) -> Rational {
        Rational {
            numerator: self.validation_fee as u64,
            denominator: self.sum(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Fees {
    pub insurance_amount: StLamports,
    pub treasury_amount: StLamports,
    pub reward_per_validator: StLamports,
    pub manager_amount: StLamports,
}

pub fn distribute_fees(
    fee_distribution: &FeeDistribution,
    num_validators: u64,
    amount_spt: StakePoolTokenLamports,
) -> Option<Fees> {
    let amount = StLamports(amount_spt.0);
    let insurance_amount = (amount * fee_distribution.insurance_fraction())?;
    let treasury_amount = (amount * fee_distribution.treasury_fraction())?;

    // The actual amount that goes to validation can be a tiny bit lower
    // than the target amount, when the number of validators does not divide
    // the target amount. The loss is at most `num_validators` stLamports.
    let validation_target = (amount * fee_distribution.validation_fraction())?;
    let reward_per_validator = (validation_target / num_validators)?;
    let validation_actual = (reward_per_validator * num_validators)?;

    // The leftovers are for the manager. Rather than computing the fraction,
    // we compute the leftovers, to ensure that the output amount equals the
    // input amount.
    let manager_amount = amount
        .sub(insurance_amount)?
        .sub(treasury_amount)?
        .sub(validation_actual)?;

    let result = Fees {
        insurance_amount,
        treasury_amount,
        reward_per_validator,
        manager_amount,
    };

    Some(result)
}

#[cfg(test)]
mod test_lido {
    use super::*;
    use solana_program::program_error::ProgramError;
    use solana_sdk::signature::{Keypair, Signer};
    use spl_stake_pool::borsh::get_instance_packed_len;

    #[test]
    fn test_validator_credit_size() {
        let one_val = get_instance_packed_len(&ValidatorCreditAccounts::new(1)).unwrap();
        let two_val = get_instance_packed_len(&ValidatorCreditAccounts::new(2)).unwrap();
        assert_eq!(two_val - one_val, 72);
    }
    #[test]
    fn test_lido_serialization() {
        let mut data = Vec::new();
        let lido = Lido {
            stake_pool_account: Pubkey::new_unique(),
            manager: Pubkey::new_unique(),
            st_sol_mint_program: Pubkey::new_unique(),
            st_sol_total_shares: StLamports(1000),
            stake_pool_token_holder: Pubkey::new_unique(),
            token_program_id: Pubkey::new_unique(),
            sol_reserve_authority_bump_seed: 1,
            deposit_authority_bump_seed: 2,
            stake_pool_authority_bump_seed: 3,
            fee_manager_bump_seed: 4,
            fee_distribution: FeeDistribution {
                insurance_fee: 1,
                treasury_fee: 2,
                validation_fee: 3,
                manager_fee: 4,
            },
            fee_recipients: FeeRecipients {
                insurance_account: Pubkey::new_unique(),
                treasury_account: Pubkey::new_unique(),
                manager_account: Pubkey::new_unique(),
                validator_credit_accounts: ValidatorCreditAccounts {
                    max_validators: 10000,
                    validator_accounts: vec![ValidatorCredit {
                        stake_address: Pubkey::new_unique(),
                        token_address: Pubkey::new_unique(),
                        st_sol_amount: StLamports(10000),
                    }],
                },
            },
        };
        let validator_accounts_len =
            get_instance_packed_len(&ValidatorCreditAccounts::new(10000)).unwrap();
        assert_eq!(validator_accounts_len, 10000 * (32 * 2 + 8) + 8);
        lido.serialize(&mut data).unwrap();
        assert_eq!(data.len(), LIDO_CONSTANT_SIZE + (32 * 2 + 8) + 8);
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

        assert_eq!(pool_tokens_for_deposit, Some(StLamports(123)));
    }

    #[test]
    fn test_pool_tokens_when_st_sol_total_shares_is_default() {
        let lido = Lido::default();

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(200, 100);

        assert_eq!(pool_tokens_for_deposit, Some(StLamports(0)));
    }

    #[test]
    fn test_pool_tokens_when_st_sol_total_shares_is_increased() {
        let mut lido = Lido::default();
        lido.st_sol_total_shares = StLamports(120);

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(200, 40);

        assert_eq!(pool_tokens_for_deposit, Some(StLamports(600)));
    }

    #[test]
    fn test_pool_tokens_when_stake_lamports_is_zero() {
        let mut lido = Lido::default();
        lido.st_sol_total_shares = StLamports(120);

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(0, 40);

        assert_eq!(pool_tokens_for_deposit, Some(StLamports(0)));
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
        let spec = FeeDistribution {
            insurance_fee: 3,
            treasury_fee: 3,
            validation_fee: 2,
            manager_fee: 1,
        };
        assert_eq!(
            Fees {
                insurance_amount: StLamports(333),
                treasury_amount: StLamports(333),
                reward_per_validator: StLamports(222),
                manager_amount: StLamports(111),
            },
            // Test no rounding errors
            distribute_fees(&spec, 1, StakePoolTokenLamports(999)).unwrap()
        );

        assert_eq!(
            Fees {
                insurance_amount: StLamports(333),
                treasury_amount: StLamports(333),
                reward_per_validator: StLamports(55),
                manager_amount: StLamports(114),
            },
            // Test rounding errors going to manager
            distribute_fees(&spec, 4, StakePoolTokenLamports(1_000)).unwrap()
        );
        let spec_coprime = FeeDistribution {
            insurance_fee: 13,
            treasury_fee: 17,
            validation_fee: 23,
            manager_fee: 19,
        };
        assert_eq!(
            Fees {
                insurance_amount: StLamports(180),
                treasury_amount: StLamports(236),
                reward_per_validator: StLamports(319),
                manager_amount: StLamports(265),
            },
            distribute_fees(&spec_coprime, 1, StakePoolTokenLamports(1_000)).unwrap()
        );
    }
    #[test]
    fn test_n_val() {
        let n_validators: u64 = 10000;
        let size =
            get_instance_packed_len(&ValidatorCreditAccounts::new(n_validators as u32)).unwrap();

        assert_eq!(
            ValidatorCreditAccounts::maximum_accounts(size) as u64,
            n_validators
        );
    }
}
