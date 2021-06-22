//! State transition types

use serde::Serialize;
use std::ops::Sub;

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::borsh::get_instance_packed_len;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    program_pack::Pack, pubkey::Pubkey,
};

use crate::account_map::{AccountMap, AccountSet, PubkeyAndEntry};
use crate::error::LidoError;
use crate::token::{Lamports, Rational, StLamports};
use crate::util::serialize_b58;
use crate::RESERVE_AUTHORITY;
use crate::VALIDATOR_STAKE_ACCOUNT;
use solana_program::clock::Slot;
use spl_stake_pool::solana_program::clock::Epoch;

pub const LIDO_VERSION: u8 = 0;
/// Constant size of header size = 1 version,2 public keys, 1 u64, 2 u8
pub const LIDO_CONSTANT_HEADER_SIZE: usize = 1 + 2 * 32 + 8 + 2;
/// Constant size of fee struct: 2 public keys + 3 u32
pub const LIDO_CONSTANT_FEE_SIZE: usize = 2 * 32 + 3 * 4;
/// Constant size of Lido
pub const LIDO_CONSTANT_SIZE: usize = LIDO_CONSTANT_HEADER_SIZE + LIDO_CONSTANT_FEE_SIZE;

pub type Validators = AccountMap<Validator>;
pub type Maintainers = AccountSet;

/// The exchange rate used for deposits and rewards distribution.
///
/// The exchange rate of SOL to stSOL is determined by the SOL balance of
/// Solido, and the total stSOL supply: every stSOL represents a share of
/// ownership of the SOL pool.
///
/// Deposits do not change the exchange rate: we mint new stSOL proportional to
/// the amount deposited, to keep the exchange rate constant. However, rewards
/// *do* change the exchange rate. This is how rewards get distributed to stSOL
/// holders without any transactions: their stSOL will be worth more SOL.
///
/// Let's call an increase of the SOL balance that mints a proportional amount
/// of stSOL a *deposit*, and an increase of the SOL balance that does not mint
/// any stSOL a *donation*. The ordering of donations relative to one another is
/// not relevant, and the order of deposits relative to one another is not
/// relevant either. But the order of deposits relative to donations is: if you
/// deposit before a donation, you get more stSOL than when you deposit after.
/// If you deposit before, you benefit from the reward, if you deposit after,
/// you do not. In formal terms, *deposit and and donate do not commute*.
///
/// This presents a problem if we want to do rewards distribution in multiple
/// steps (one step per validator). Reward distribution is a combination of a
/// donation (the observed rewards minus fees), and a deposit (the fees, which
/// get paid as stSOL). Because deposit and donate do not commute, different
/// orders of observing validator rewards would lead to different outcomes. We
/// don't want that.
///
/// To resolve this, we use a fixed exchange rate, and update it once per epoch.
/// This means that a donation no longer changes the exchange rate (not
/// instantly at least). That means that we can observe validator rewards in any
/// order we like. A different way of thinking about this, is that by fixing
/// the exchange rate for the duration of the epoch, all the different ways of
/// ordering donations and deposits have the same outcome, so every sequence of
/// deposits and donations is equivalent to one where they all happen
/// simultaneously at the start of the epoch. Time within an epoch ceases to
/// exist, the only thing relevant is the epoch.
///
/// When we update the exchange rate, we set the values to the balance that we
/// inferred by tracking all changes. This does not include any external
/// modifications (validation rewards paid into stake accounts) that were not
/// yet observed at the time of the update.
///
/// When we observe the actual validator balance in `UpdateValidatorBalance`,
/// the difference between the tracked balance and the observed balance, is the
/// reward. We split this reward into a fee part, and a donation part, and
/// update the tracked balances accordingly. This does not by itself change the
/// exchange rate, but the new values will be used in the next exchange rate
/// update.
///
/// `UpdateValidatorBalance` is blocked in a given epoch, until we update the
/// exchange rate in that epoch. Validation rewards are distributed at the start
/// of the epoch. This means that in epoch `i`:
///
/// 1. `UpdateExchangeRate` updates the exchange rate to what it was at the end
///    of epoch `i - 1`.
/// 2. `UpdateValidatorBalance` runs for every validator, and observes the
///    rewards. Deposits (including those for fees) in epoch `i` therefore use
///    the exchange rate at the end of epoch `i - 1`, so deposits in epoch `i`
///    do not benefit from rewards received in epoch `i`.
/// 3. Epoch `i + 1` starts, and validation rewards are paid into stake accounts.
/// 4. `UpdateExchangeRate` updates the exchange rate to what it was at the end
///    of epoch `i`. Everybody who deposited in epoch `i` (users, as well as fee
///    recipients) now benefit from the validation rewards received in epoch `i`.
/// 5. Etc.
#[repr(C)]
#[derive(
  Clone, Debug, Default, BorshDeserialize, BorshSerialize, Eq, PartialEq, Serialize,
)]
struct ExchangeRate {
    /// The epoch in which we last called `UpdateExchangeRate`.
    pub computed_in_epoch: Epoch,

    /// The amount of stSOL that existed at that time.
    pub st_sol_supply: StLamports,

    /// The amount of SOL we managed at that time, according to our internal
    /// bookkeeping, so excluding the validation rewards paid at the start of
    /// epoch `computed_in_epoch`.
    pub sol_balance: Lamports,
}

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Lido {
    /// Version number for the Lido
    pub lido_version: u8,
    /// Manager of the Lido program, able to execute administrative functions
    #[serde(serialize_with = "serialize_b58")]
    pub manager: Pubkey,

    /// The SPL Token mint address for stSOL.
    #[serde(serialize_with = "serialize_b58")]
    pub st_sol_mint: Pubkey,

    /// Exchange rate to use when depositing.
    pub exchange_rate: ExchangeRate,

    /// Bump seeds for signing messages on behalf of the authority
    pub sol_reserve_authority_bump_seed: u8,
    pub deposit_authority_bump_seed: u8,

    /// Fees
    pub fee_distribution: FeeDistribution,
    pub fee_recipients: FeeRecipients,

    /// Map of enrolled validators, maps their vote account to `Validator` details.
    pub validators: Validators,

    /// The set of maintainers.
    ///
    /// Maintainers are granted low security risk privileges. Maintainers are
    /// expected to run the maintenance daemon, that invokes the maintenance
    /// operations. These are gated on the signer being present in this set.
    /// In the future we plan to make maintenance operations callable by anybody.
    pub maintainers: Maintainers,
}

impl Lido {
    /// Calculates the total size of Lido given two variables: `max_validators`
    /// and `max_maintainers`, the maximum number of maintainers and validators,
    /// respectively. It creates default structures for both and sum its sizes
    /// with Lido's constant size.
    pub fn calculate_size(max_validators: u32, max_maintainers: u32) -> usize {
        let lido_instance = Lido {
            validators: Validators::new_fill_default(max_validators),
            maintainers: Maintainers::new_fill_default(max_maintainers),
            ..Default::default()
        };
        get_instance_packed_len(&lido_instance).unwrap()
    }
    pub fn calc_pool_tokens_for_deposit(
        &self,
        stake_lamports: Lamports,
        total_lamports: Lamports,
    ) -> Option<StLamports> {
        if total_lamports == Lamports(0) {
            return Some(StLamports(stake_lamports.0));
        }
        let ratio = Rational {
            numerator: self.st_sol_total_shares.0,
            denominator: total_lamports.0,
        };
        StLamports(stake_lamports.0) * ratio
    }

    pub fn is_initialized(&self) -> ProgramResult {
        if self.manager != Pubkey::default() {
            msg!("Provided Solido instance already in use.");
            Err(LidoError::AlreadyInUse.into())
        } else {
            Ok(())
        }
    }

    /// Confirm that the given account is Solido's stSOL mint.
    pub fn check_mint_is_st_sol_mint(&self, mint_account_info: &AccountInfo) -> ProgramResult {
        if &self.st_sol_mint != mint_account_info.key {
            msg!(
                "Expected to find our stSOL mint ({}), but got {} instead.",
                self.st_sol_mint,
                mint_account_info.key
            );
            return Err(LidoError::InvalidStSolAccount.into());
        }
        Ok(())
    }

    /// Confirm that the given account is an SPL token account with our stSOL mint as mint.
    pub fn check_is_st_sol_account(&self, token_account_info: &AccountInfo) -> ProgramResult {
        let token_account =
            match spl_token::state::Account::unpack_from_slice(&token_account_info.data.borrow()) {
                Ok(account) => account,
                Err(..) => {
                    msg!(
                        "Expected an SPL token account at {}.",
                        token_account_info.key
                    );
                    return Err(LidoError::InvalidStSolAccount.into());
                }
            };

        if token_account.mint != self.st_sol_mint {
            msg!(
                "Expected mint of {} to be our stSOL mint ({}), but found {}.",
                token_account_info.key,
                self.st_sol_mint,
                token_account.mint,
            );
            return Err(LidoError::InvalidFeeRecipient.into());
        }
        Ok(())
    }

    /// Checks if the passed manager is the same as the one stored in the state
    pub fn check_manager(&self, manager: &AccountInfo) -> ProgramResult {
        if &self.manager != manager.key {
            msg!("Invalid manager, not the same as the one stored in state");
            return Err(LidoError::InvalidManager.into());
        }
        Ok(())
    }

    /// Checks if the passed maintainer belong to the list of maintainers
    pub fn check_maintainer(&self, maintainer: &AccountInfo) -> ProgramResult {
        if !&self.maintainers.entries.contains(&PubkeyAndEntry {
            pubkey: *maintainer.key,
            entry: (),
        }) {
            msg!(
                "Invalid maintainer, account {} is not present in the maintainers list.",
                maintainer.key
            );

            return Err(LidoError::InvalidMaintainer.into());
        }
        Ok(())
    }

    /// Return the address of the reserve account, the account where SOL gets
    /// deposited into.
    pub fn get_reserve_account(
        &self,
        program_id: &Pubkey,
        solido_address: &Pubkey,
    ) -> Result<Pubkey, ProgramError> {
        Pubkey::create_program_address(
            &[
                &solido_address.to_bytes()[..],
                RESERVE_AUTHORITY,
                &[self.sol_reserve_authority_bump_seed],
            ],
            program_id,
        )
        .map_err(|_| LidoError::InvalidReserveAuthority.into())
    }

    /// Confirm that the reserve authority belongs to this Lido instance, return
    /// the reserve address.
    pub fn check_reserve_authority(
        &self,
        program_id: &Pubkey,
        solido_address: &Pubkey,
        reserve_authority_info: &AccountInfo,
    ) -> Result<Pubkey, ProgramError> {
        let reserve_id = self.get_reserve_account(program_id, solido_address)?;
        if reserve_id != *reserve_authority_info.key {
            msg!("Invalid reserve authority");
            return Err(LidoError::InvalidReserveAuthority.into());
        }
        Ok(reserve_id)
    }

    pub fn save(&self, account: &AccountInfo) -> ProgramResult {
        // NOTE: If you ended up here because the tests are failing because the
        // runtime complained that an account's size was modified by a program
        // that wasn't its owner, double check that the name passed to
        // ProgramTest matches the name of the crate.
        BorshSerialize::serialize(self, &mut *account.data.borrow_mut())?;
        Ok(())
    }
}

#[repr(C)]
#[derive(
    Clone, Debug, Default, Eq, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema, Serialize,
)]
pub struct Validator {
    /// Fees in stSOL that the validator is entitled too, but hasn't claimed yet.
    pub fee_credit: StLamports,

    /// SPL token account denominated in stSOL to transfer fees to when claiming them.
    #[serde(serialize_with = "serialize_b58")]
    pub fee_address: Pubkey,

    /// Start (inclusive) of the seed range for currently active stake accounts.
    ///
    /// When we stake deposited SOL, we take it out of the reserve account, and
    /// transfer it to a stake account. The stake account address is a derived
    /// address derived from a.o. the validator address, and a seed. After
    /// creation, it takes one or more epochs for the stake to become fully
    /// activated. While stake is activating, we may want to activate additional
    /// stake, so we need a new stake account. Therefore we have a range of
    /// seeds. When we need a new stake account, we bump `end`. When the account
    /// with seed `begin` is 100% active, we deposit that stake account into the
    /// pool and bump `begin`. Accounts are not reused.
    ///
    /// The program enforces that creating new stake accounts is only allowed at
    /// the `_end` seed, and depositing active stake is only allowed from the
    /// `_begin` seed. This ensures that maintainers don’t race and accidentally
    /// stake more to this validator than intended. If the seed has changed
    /// since the instruction was created, the transaction fails.
    pub stake_accounts_seed_begin: u64,

    /// End (exclusive) of the seed range for currently active stake accounts.
    pub stake_accounts_seed_end: u64,

    /// Sum of the balances of the stake accounts.
    pub stake_accounts_balance: Lamports,
}

impl Validator {
    pub fn new(fee_address: Pubkey) -> Validator {
        Validator {
            fee_address,
            fee_credit: StLamports(0),
            stake_accounts_seed_begin: 0,
            stake_accounts_seed_end: 0,
            stake_accounts_balance: Lamports(0),
        }
    }

    pub fn find_stake_account_address(
        program_id: &Pubkey,
        solido_account: &Pubkey,
        validator_vote_account: &Pubkey,
        seed: u64,
    ) -> (Pubkey, u8) {
        let seeds = [
            &solido_account.to_bytes(),
            &validator_vote_account.to_bytes(),
            VALIDATOR_STAKE_ACCOUNT,
            &seed.to_le_bytes()[..],
        ];
        Pubkey::find_program_address(&seeds, program_id)
    }
}

/// Determines how fees are split up among these parties, represented as the
/// number of parts of the total. For example, if each party has 1 part, then
/// they all get an equal share of the fee.
#[derive(
    Clone, Default, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema, Serialize,
)]
pub struct FeeDistribution {
    pub treasury_fee: u32,
    pub validation_fee: u32,
    pub developer_fee: u32,
}

/// Specifies the fee recipients, accounts that should be created by Lido's minter
#[derive(
    Clone, Default, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema, Serialize,
)]
pub struct FeeRecipients {
    #[serde(serialize_with = "serialize_b58")]
    pub treasury_account: Pubkey,
    #[serde(serialize_with = "serialize_b58")]
    pub developer_account: Pubkey,
}

impl FeeDistribution {
    pub fn sum(&self) -> u64 {
        // These adds don't overflow because we widen from u32 to u64 first.
        self.treasury_fee as u64 + self.validation_fee as u64 + self.developer_fee as u64
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

#[derive(Debug, PartialEq, Eq)]
pub struct Fees {
    pub treasury_amount: StLamports,
    pub reward_per_validator: StLamports,
    pub developer_amount: StLamports,
}

pub fn distribute_fees(
    fee_distribution: &FeeDistribution,
    num_validators: u64,
    reward: Lamports,
) -> Option<Fees> {
    let amount = StLamports(reward.0);
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
    let developer_amount = amount.sub(treasury_amount)?.sub(validation_actual)?;

    let result = Fees {
        treasury_amount,
        reward_per_validator,
        developer_amount,
    };

    Some(result)
}

#[cfg(test)]
mod test_lido {
    use super::*;
    use solana_program::program_error::ProgramError;
    use solana_sdk::signature::{Keypair, Signer};

    #[test]
    fn test_account_map_required_bytes_relates_to_maximum_entries() {
        for buffer_size in 0..8_000 {
            let max_entries = Validators::maximum_entries(buffer_size);
            let needed_size = Validators::required_bytes(max_entries);
            assert!(
                needed_size <= buffer_size || max_entries == 0,
                "Buffer of len {} can fit {} validators which need {} bytes.",
                buffer_size,
                max_entries,
                needed_size,
            );

            let max_entries = Maintainers::maximum_entries(buffer_size);
            let needed_size = Maintainers::required_bytes(max_entries);
            assert!(
                needed_size <= buffer_size || max_entries == 0,
                "Buffer of len {} can fit {} maintainers which need {} bytes.",
                buffer_size,
                max_entries,
                needed_size,
            );
        }
    }

    #[test]
    fn test_validators_size() {
        let one_len = get_instance_packed_len(&Validators::new_fill_default(1)).unwrap();
        let two_len = get_instance_packed_len(&Validators::new_fill_default(2)).unwrap();
        assert_eq!(one_len, Validators::required_bytes(1));
        assert_eq!(two_len, Validators::required_bytes(2));
        assert_eq!(
            two_len - one_len,
            std::mem::size_of::<(Pubkey, Validator)>()
        );
    }

    #[test]
    fn test_lido_serialization_roundtrips() {
        use solana_sdk::borsh::try_from_slice_unchecked;

        let mut validators = Validators::new(10_000);
        validators
            .add(Pubkey::new_unique(), Validator::new(Pubkey::new_unique()))
            .unwrap();
        let maintainers = Maintainers::new(1);
        let lido = Lido {
            lido_version: 0,
            manager: Pubkey::new_unique(),
            st_sol_mint: Pubkey::new_unique(),
            st_sol_total_shares: StLamports(1000),
            sol_reserve_authority_bump_seed: 1,
            deposit_authority_bump_seed: 2,
            fee_distribution: FeeDistribution {
                treasury_fee: 2,
                validation_fee: 3,
                developer_fee: 4,
            },
            fee_recipients: FeeRecipients {
                treasury_account: Pubkey::new_unique(),
                developer_account: Pubkey::new_unique(),
            },
            validators: validators,
            maintainers: maintainers,
        };
        let mut data = Vec::new();
        BorshSerialize::serialize(&lido, &mut data).unwrap();

        let lido_restored = try_from_slice_unchecked(&data[..]).unwrap();
        assert_eq!(lido, lido_restored);
    }

    #[test]
    fn lido_initialized() {
        let lido = Lido::default();

        assert!(lido.is_initialized().is_ok());
    }

    #[test]
    fn test_pool_tokens_when_total_lamports_is_zero() {
        let lido = Lido::default();

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(Lamports(123), Lamports(0));

        assert_eq!(pool_tokens_for_deposit, Some(StLamports(123)));
    }

    #[test]
    fn test_pool_tokens_when_st_sol_total_shares_is_default() {
        let lido = Lido::default();

        let pool_tokens_for_deposit =
            lido.calc_pool_tokens_for_deposit(Lamports(200), Lamports(100));

        assert_eq!(pool_tokens_for_deposit, Some(StLamports(0)));
    }

    #[test]
    fn test_pool_tokens_when_st_sol_total_shares_is_increased() {
        let mut lido = Lido::default();
        lido.st_sol_total_shares = StLamports(120);

        let pool_tokens_for_deposit =
            lido.calc_pool_tokens_for_deposit(Lamports(200), Lamports(40));

        assert_eq!(pool_tokens_for_deposit, Some(StLamports(600)));
    }

    #[test]
    fn test_pool_tokens_when_stake_lamports_is_zero() {
        let mut lido = Lido::default();
        lido.st_sol_total_shares = StLamports(120);

        let pool_tokens_for_deposit = lido.calc_pool_tokens_for_deposit(Lamports(0), Lamports(40));

        assert_eq!(pool_tokens_for_deposit, Some(StLamports(0)));
    }

    #[test]
    fn test_lido_for_deposit_wrong_mint() {
        let mut lido = Lido::default();
        lido.st_sol_mint = Keypair::new().pubkey();

        let other_mint = Keypair::new();
        let pubkey = other_mint.pubkey();
        let mut lamports = 100;
        let mut data = [0_u8];
        let is_signer = false;
        let is_writable = false;
        let owner = spl_token::id();
        let executable = false;
        let rent_epoch = 1;
        let fake_mint_account = AccountInfo::new(
            &pubkey,
            is_signer,
            is_writable,
            &mut lamports,
            &mut data,
            &owner,
            executable,
            rent_epoch,
        );
        let result = lido.check_mint_is_st_sol_mint(&fake_mint_account);

        let expected_error: ProgramError = LidoError::InvalidStSolAccount.into();
        assert_eq!(result, Err(expected_error));
    }

    #[test]
    fn test_fee_distribution() {
        let spec = FeeDistribution {
            treasury_fee: 3,
            validation_fee: 2,
            developer_fee: 1,
        };
        assert_eq!(
            Fees {
                treasury_amount: StLamports(300),
                reward_per_validator: StLamports(200),
                developer_amount: StLamports(100),
            },
            // Test no rounding errors
            distribute_fees(&spec, 1, Lamports(600)).unwrap()
        );

        assert_eq!(
            Fees {
                treasury_amount: StLamports(500),
                reward_per_validator: StLamports(83),
                developer_amount: StLamports(168),
            },
            // Test rounding errors going to manager
            distribute_fees(&spec, 4, Lamports(1_000)).unwrap()
        );
        let spec_coprime = FeeDistribution {
            treasury_fee: 17,
            validation_fee: 23,
            developer_fee: 19,
        };
        assert_eq!(
            Fees {
                treasury_amount: StLamports(288),
                reward_per_validator: StLamports(389),
                developer_amount: StLamports(323),
            },
            distribute_fees(&spec_coprime, 1, Lamports(1_000)).unwrap()
        );
    }
    #[test]
    fn test_n_val() {
        let n_validators: u64 = 10_000;
        let size =
            get_instance_packed_len(&Validators::new_fill_default(n_validators as u32)).unwrap();

        assert_eq!(Validators::maximum_entries(size) as u64, n_validators);
    }

    #[test]
    fn test_version_serialise() {
        use solana_sdk::borsh::try_from_slice_unchecked;

        for i in 0..=255 {
            let lido = Lido {
                lido_version: i,
                ..Lido::default()
            };
            let mut res: Vec<u8> = Vec::new();
            BorshSerialize::serialize(&lido, &mut res).unwrap();

            assert_eq!(res[0], i);

            let lido_recovered = try_from_slice_unchecked(&res[..]).unwrap();
            assert_eq!(lido, lido_recovered);
        }
    }
}
