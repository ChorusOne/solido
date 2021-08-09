// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! State transition types

use serde::Serialize;

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::borsh::get_instance_packed_len;
use solana_program::clock::Clock;
use solana_program::{
    account_info::AccountInfo, clock::Epoch, entrypoint::ProgramResult, msg,
    program_error::ProgramError, program_pack::Pack, pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};
use spl_token::state::Mint;

use crate::error::LidoError;
use crate::logic::get_reserve_available_balance;
use crate::metrics::Metrics;
use crate::token::{self, Lamports, Rational, StLamports};
use crate::util::serialize_b58;
use crate::REWARDS_WITHDRAW_AUTHORITY;
use crate::{
    account_map::{AccountMap, AccountSet, EntryConstantSize, PubkeyAndEntry},
    MINIMUM_STAKE_ACCOUNT_BALANCE, MINT_AUTHORITY, RESERVE_ACCOUNT, STAKE_AUTHORITY,
    VALIDATOR_STAKE_ACCOUNT,
};

pub const LIDO_VERSION: u8 = 0;

/// Size of a serialized `Lido` struct excluding validators and maintainers.
///
/// To update this, run the tests and replace the value here with the test output.
pub const LIDO_CONSTANT_SIZE: usize = 357;
pub const VALIDATOR_CONSTANT_SIZE: usize = 65;

pub type Validators = AccountMap<Validator>;

impl Validators {
    pub fn iter_active(&self) -> impl Iterator<Item = &Validator> {
        self.iter_entries().filter(|&v| !v.inactive)
    }
}
pub type Maintainers = AccountSet;

impl EntryConstantSize for Validator {
    const SIZE: usize = VALIDATOR_CONSTANT_SIZE;
}

impl EntryConstantSize for () {
    const SIZE: usize = 0;
}

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
/// When we observe the actual validator balance in `WithdrawInactiveStake`, the
/// difference between the tracked balance and the observed balance, is a
/// donation that will be returned to the reserve account.
///
/// We collect the rewards accumulated by a validator with the
/// `CollectValidatorFee` instruction. This function distributes the accrued
/// rewards paid to the Solido program (as we enforce that 100% of the fees goes
/// to the Solido program).
///
/// `CollectValidatorFee` is blocked in a given epoch, until we update the
/// exchange rate in that epoch. Validation rewards are distributed at the start
/// of the epoch. This means that in epoch `i`:
///
/// 1. `UpdateExchangeRate` updates the exchange rate to what it was at the end
///    of epoch `i - 1`.
/// 2. `CollectValidatorFee` runs for every validator, and observes the
///    rewards. Deposits (including those for fees) in epoch `i` therefore use
///    the exchange rate at the end of epoch `i - 1`, so deposits in epoch `i`
///    do not benefit from rewards received in epoch `i`.
/// 3. Epoch `i + 1` starts, and validation rewards are paid into validator's
/// vote accounts.
/// 4. `UpdateExchangeRate` updates the exchange rate to what it was at the end
///    of epoch `i`. Everybody who deposited in epoch `i` (users, as well as fee
///    recipients) now benefit from the validation rewards received in epoch `i`.
/// 5. Etc.
#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct ExchangeRate {
    /// The epoch in which we last called `UpdateExchangeRate`.
    pub computed_in_epoch: Epoch,

    /// The amount of stSOL that existed at that time.
    pub st_sol_supply: StLamports,

    /// The amount of SOL we managed at that time, according to our internal
    /// bookkeeping, so excluding the validation rewards paid at the start of
    /// epoch `computed_in_epoch`.
    pub sol_balance: Lamports,
}

impl ExchangeRate {
    /// Convert SOL to stSOL.
    pub fn exchange_sol(&self, amount: Lamports) -> token::Result<StLamports> {
        // The exchange rate starts out at 1:1, if there are no deposits yet.
        // If we minted stSOL but there is no SOL, then also assume a 1:1 rate.
        if self.st_sol_supply == StLamports(0) || self.sol_balance == Lamports(0) {
            return Ok(StLamports(amount.0));
        }

        let rate = Rational {
            numerator: self.st_sol_supply.0,
            denominator: self.sol_balance.0,
        };

        // The result is in Lamports, because the type system considers Rational
        // dimensionless, but in this case `rate` has dimensions stSOL/SOL, so
        // we need to re-wrap the result in the right type.
        (amount * rate).map(|x| StLamports(x.0))
    }

    /// Convert stSOL to SOL.
    pub fn exchange_st_sol(&self, amount: StLamports) -> Result<Lamports, LidoError> {
        // If there is no stSOL in existence, it cannot be exchanged.
        if self.st_sol_supply == StLamports(0) {
            msg!("Cannot exchange stSOL for SOL, because no stSTOL has been minted.");
            return Err(LidoError::InvalidAmount);
        }

        let rate = Rational {
            numerator: self.sol_balance.0,
            denominator: self.st_sol_supply.0,
        };

        // The result is in StLamports, because the type system considers Rational
        // dimensionless, but in this case `rate` has dimensions SOL/stSOL, so
        // we need to re-wrap the result in the right type.
        Ok((amount * rate).map(|x| Lamports(x.0))?)
    }
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
    pub sol_reserve_account_bump_seed: u8,
    pub stake_authority_bump_seed: u8,
    pub mint_authority_bump_seed: u8,
    pub rewards_withdraw_authority_bump_seed: u8,

    /// How rewards are distributed.
    pub reward_distribution: RewardDistribution,

    /// Accounts of the fee recipients.
    pub fee_recipients: FeeRecipients,

    /// Metrics for informational purposes.
    ///
    /// Metrics are only written to, no program logic should depend on these values.
    /// An off-chain program can load a snapshot of the `Lido` struct, and expose
    /// these metrics.
    pub metrics: Metrics,

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

    /// Check if the passed treasury fee account is the one configured.
    ///
    /// Also confirm that the recipient is still an stSOL account.
    pub fn check_treasury_fee_st_sol_account(&self, st_sol_account: &AccountInfo) -> ProgramResult {
        if &self.fee_recipients.treasury_account != st_sol_account.key {
            msg!("Invalid treasury fee stSOL account, not the same as the one stored in state.");
            return Err(LidoError::InvalidFeeRecipient.into());
        }
        self.check_is_st_sol_account(st_sol_account)
    }

    /// Check if the passed developer fee account is the one configured.
    ///
    /// Also confirm that the recipient is still an stSOL account.
    pub fn check_developer_fee_st_sol_account(
        &self,
        st_sol_account: &AccountInfo,
    ) -> ProgramResult {
        if &self.fee_recipients.developer_account != st_sol_account.key {
            msg!("Invalid developer fee stSOL account, not the same as the one stored in state.");
            return Err(LidoError::InvalidFeeRecipient.into());
        }
        self.check_is_st_sol_account(st_sol_account)
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
                RESERVE_ACCOUNT,
                &[self.sol_reserve_account_bump_seed],
            ],
            program_id,
        )
        .map_err(|_| LidoError::InvalidReserveAccount.into())
    }

    /// Confirm that the reserve account belongs to this Lido instance, return
    /// the reserve address.
    pub fn check_reserve_account(
        &self,
        program_id: &Pubkey,
        solido_address: &Pubkey,
        reserve_account_info: &AccountInfo,
    ) -> Result<Pubkey, ProgramError> {
        let reserve_id = self.get_reserve_account(program_id, solido_address)?;
        if reserve_id != *reserve_account_info.key {
            msg!("Invalid reserve account");
            return Err(LidoError::InvalidReserveAccount.into());
        }
        Ok(reserve_id)
    }

    /// Return the address of the stake authority, the program-derived address
    /// that can sign staking instructions.
    pub fn get_stake_authority(
        &self,
        program_id: &Pubkey,
        solido_address: &Pubkey,
    ) -> Result<Pubkey, ProgramError> {
        Pubkey::create_program_address(
            &[
                &solido_address.to_bytes()[..],
                STAKE_AUTHORITY,
                &[self.stake_authority_bump_seed],
            ],
            program_id,
        )
        .map_err(|_| ProgramError::InvalidSeeds)
    }

    /// Confirm that the stake authority belongs to this Lido instance, return
    /// the stake authority address.
    pub fn check_stake_authority(
        &self,
        program_id: &Pubkey,
        solido_address: &Pubkey,
        stake_authority_account_info: &AccountInfo,
    ) -> Result<Pubkey, ProgramError> {
        let authority = self.get_stake_authority(program_id, solido_address)?;
        if &authority != stake_authority_account_info.key {
            msg!(
                "Invalid stake authority, expected {} but got {}.",
                authority,
                stake_authority_account_info.key
            );
            return Err(LidoError::InvalidStakeAuthority.into());
        }
        Ok(authority)
    }

    /// Return the address of the rewards withdraw authority, the
    /// program-derived address that can sign on behalf of vote accounts.
    pub fn get_rewards_withdraw_authority(
        &self,
        program_id: &Pubkey,
        solido_address: &Pubkey,
    ) -> Result<Pubkey, ProgramError> {
        Pubkey::create_program_address(
            &[
                &solido_address.to_bytes()[..],
                REWARDS_WITHDRAW_AUTHORITY,
                &[self.rewards_withdraw_authority_bump_seed],
            ],
            program_id,
        )
        .map_err(|_| ProgramError::InvalidSeeds)
    }

    /// Confirm that the rewards withdraw authority belongs to this Lido
    /// instance, return the rewards authority address.
    pub fn check_rewards_withdraw_authority(
        &self,
        program_id: &Pubkey,
        solido_address: &Pubkey,
        rewards_withdraw_authority_account_info: &AccountInfo,
    ) -> Result<Pubkey, ProgramError> {
        let authority = self.get_rewards_withdraw_authority(program_id, solido_address)?;
        if &authority != rewards_withdraw_authority_account_info.key {
            msg!(
                "Invalid rewards withdraw authority, expected {} but got {}.",
                authority,
                rewards_withdraw_authority_account_info.key
            );
            return Err(LidoError::InvalidRewardsWithdrawAuthority.into());
        }
        Ok(authority)
    }

    pub fn get_mint_authority(
        &self,
        program_id: &Pubkey,
        solido_address: &Pubkey,
    ) -> Result<Pubkey, ProgramError> {
        Pubkey::create_program_address(
            &[
                &solido_address.to_bytes()[..],
                MINT_AUTHORITY,
                &[self.mint_authority_bump_seed],
            ],
            program_id,
        )
        .map_err(|_| ProgramError::InvalidSeeds)
    }

    /// Confirm that the amount to stake is more than the minimum stake amount,
    /// and that we have sufficient SOL in the reserve.
    pub fn check_can_stake_amount(
        &self,
        reserve: &AccountInfo,
        sysvar_rent: &AccountInfo,
        amount: Lamports,
    ) -> Result<(), ProgramError> {
        if amount < MINIMUM_STAKE_ACCOUNT_BALANCE {
            msg!("Trying to stake less than the minimum stake account balance.");
            msg!(
                "Need as least {} but got {}.",
                MINIMUM_STAKE_ACCOUNT_BALANCE,
                amount
            );
            return Err(LidoError::InvalidAmount.into());
        }

        let rent: Rent = Rent::from_account_info(sysvar_rent)?;

        let available_reserve_amount = get_reserve_available_balance(&rent, reserve)?;
        if amount > available_reserve_amount {
            msg!(
                "The requested amount {} is greater than the available amount {}, \
                considering rent-exemption",
                amount,
                available_reserve_amount
            );
            return Err(LidoError::AmountExceedsReserve.into());
        }

        Ok(())
    }

    /// Confirm that `stake_account` is the account at the given seed for the validator.
    ///
    /// Returns the bump seed for the derived address.
    pub fn check_stake_account(
        program_id: &Pubkey,
        solido_address: &Pubkey,
        validator: &PubkeyAndEntry<Validator>,
        stake_account_seed: u64,
        stake_account: &AccountInfo,
    ) -> Result<u8, ProgramError> {
        let (stake_addr, stake_addr_bump_seed) =
            validator.find_stake_account_address(program_id, solido_address, stake_account_seed);
        if &stake_addr != stake_account.key {
            msg!(
                "The derived stake address for seed {} is {}, \
                but the instruction received {} instead.",
                stake_account_seed,
                stake_addr,
                stake_account.key,
            );
            msg!(
                "Note: this can happen during normal operation when instructions \
                race, and one updates the validator's seeds before the other executes."
            );
            return Err(LidoError::InvalidStakeAccount.into());
        }
        Ok(stake_addr_bump_seed)
    }

    pub fn save(&self, account: &AccountInfo) -> ProgramResult {
        // NOTE: If you ended up here because the tests are failing because the
        // runtime complained that an account's size was modified by a program
        // that wasn't its owner, double check that the name passed to
        // ProgramTest matches the name of the crate.
        BorshSerialize::serialize(self, &mut *account.data.borrow_mut())?;
        Ok(())
    }

    /// Compute the total amount of SOL managed by this instance.
    ///
    /// This includes staked as well as non-staked SOL. It excludes SOL in the
    /// reserve that effectively locked because it is needed to keep the reserve
    /// rent-exempt.
    ///
    /// The computation is based on the amount of SOL per validator that we track
    /// ourselves, so if there are any unobserved rewards in the stake accounts,
    /// these will not be included.
    pub fn get_sol_balance(
        &self,
        rent: &Rent,
        reserve: &AccountInfo,
    ) -> Result<Lamports, LidoError> {
        let effective_reserve_balance = get_reserve_available_balance(rent, reserve)?;

        // The remaining SOL managed is all in stake accounts.
        let validator_balance: token::Result<Lamports> = self
            .validators
            .iter_entries()
            .map(|v| v.stake_accounts_balance)
            .sum();

        let result = validator_balance.and_then(|s| s + effective_reserve_balance)?;

        Ok(result)
    }

    /// Return the total amount of stSOL in existence.
    ///
    /// The total is the amount minted so far, plus any unminted rewards that validators
    /// are entitled to, but haven’t claimed yet.
    pub fn get_st_sol_supply(&self, st_sol_mint: &AccountInfo) -> Result<StLamports, ProgramError> {
        self.check_mint_is_st_sol_mint(st_sol_mint)?;

        let st_sol_mint = Mint::unpack_from_slice(&st_sol_mint.data.borrow())?;
        let minted_supply = StLamports(st_sol_mint.supply);

        let credit: token::Result<StLamports> =
            self.validators.iter_entries().map(|v| v.fee_credit).sum();

        let result = credit.and_then(|s| s + minted_supply)?;

        Ok(result)
    }

    pub fn check_exchange_rate_last_epoch(
        &self,
        clock: &Clock,
        method: &str,
    ) -> Result<(), LidoError> {
        if self.exchange_rate.computed_in_epoch < clock.epoch {
            msg!(
                "The exchange rate is outdated, it was last computed in epoch {}, \
                but now it is epoch {}.",
                self.exchange_rate.computed_in_epoch,
                clock.epoch,
            );
            msg!("Please call UpdateExchangeRate before calling {}.", method);
            return Err(LidoError::ExchangeRateNotUpdatedInThisEpoch);
        }
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

    /// Controls if a validator is allowed to have new stake deposits.
    /// When removing a validator, this flag should be set to `true`.
    pub inactive: bool,
}

impl Validator {
    pub fn new(fee_address: Pubkey) -> Validator {
        Validator {
            fee_address,
            ..Default::default()
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

impl PubkeyAndEntry<Validator> {
    pub fn find_stake_account_address(
        &self,
        program_id: &Pubkey,
        solido_account: &Pubkey,
        seed: u64,
    ) -> (Pubkey, u8) {
        Validator::find_stake_account_address(program_id, solido_account, &self.pubkey, seed)
    }
}

/// Determines how rewards are split up among these parties, represented as the
/// number of parts of the total. For example, if each party has 1 part, then
/// they all get an equal share of the reward.
#[derive(
    Clone, Default, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema, Serialize,
)]
pub struct RewardDistribution {
    pub treasury_fee: u32,
    pub validation_fee: u32,
    pub developer_fee: u32,
    pub st_sol_appreciation: u32,
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

impl RewardDistribution {
    pub fn sum(&self) -> u64 {
        // These adds don't overflow because we widen from u32 to u64 first.
        self.treasury_fee as u64
            + self.validation_fee as u64
            + self.developer_fee as u64
            + self.st_sol_appreciation as u64
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

    pub fn developer_fraction(&self) -> Rational {
        Rational {
            numerator: self.developer_fee as u64,
            denominator: self.sum(),
        }
    }

    /// Split the reward according to the distribution defined in this instance.
    ///
    /// Fees are all rounded down, and the remainder goes to stSOL appreciation.
    /// This means that the outputs may not sum to the input, even when
    /// `st_sol_appreciation` is 0.
    ///
    /// Returns the fee amounts in SOL. stSOL should be minted for those when
    /// they get distributed. This acts like a deposit: it is like the fee
    /// recipients received their fee in SOL outside of Solido, and then
    /// deposited it. The remaining SOL, which is not taken as a fee, acts as a
    /// donation to the pool, and makes the SOL value of stSOL go up. It is not
    /// included in the output, as nothing needs to be done to handle it.
    pub fn split_reward(&self, amount: Lamports, num_validators: u64) -> token::Result<Fees> {
        use std::ops::Add;

        let treasury_amount = (amount * self.treasury_fraction())?;
        let developer_amount = (amount * self.developer_fraction())?;

        // The actual amount that goes to validation can be a tiny bit lower
        // than the target amount, when the number of validators does not divide
        // the target amount. The loss is at most `num_validators` Lamports.
        let validation_amount = (amount * self.validation_fraction())?;
        let reward_per_validator = (validation_amount / num_validators)?;

        // Sanity check: We should not produce more fees than we had to split in
        // the first place.
        let total_fees = Lamports(0)
            .add(treasury_amount)?
            .add(developer_amount)?
            .add((reward_per_validator * num_validators)?)?;
        assert!(total_fees <= amount);

        let st_sol_appreciation_amount = (amount - total_fees)?;

        let result = Fees {
            treasury_amount,
            reward_per_validator,
            developer_amount,
            st_sol_appreciation_amount,
        };

        Ok(result)
    }
}

/// The result of [`RewardDistribution::split_reward`].
///
/// It contains only the fees. The amount that goes to stSOL value appreciation
/// is implicitly the remainder.
#[derive(Debug, PartialEq, Eq)]
pub struct Fees {
    pub treasury_amount: Lamports,
    pub reward_per_validator: Lamports,
    pub developer_amount: Lamports,

    /// Remainder of the reward.
    ///
    /// This is not a fee, and it is not paid out explicitly, but when summed
    /// with the other fields in this struct, that totals the input amount.
    pub st_sol_appreciation_amount: Lamports,
}

#[cfg(test)]
mod test_lido {
    use super::*;
    use solana_program::program_error::ProgramError;

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
        let validator = get_instance_packed_len(&Validator::default()).unwrap();
        assert_eq!(validator, Validator::SIZE);
        let one_len = get_instance_packed_len(&Validators::new_fill_default(1)).unwrap();
        let two_len = get_instance_packed_len(&Validators::new_fill_default(2)).unwrap();
        assert_eq!(one_len, Validators::required_bytes(1));
        assert_eq!(two_len, Validators::required_bytes(2));
        assert_eq!(
            two_len - one_len,
            std::mem::size_of::<Pubkey>() + Validator::SIZE
        );
    }

    #[test]
    fn test_lido_constant_size() {
        // The minimal size of the struct is its size without any validators and
        // maintainers.
        let minimal = Lido::default();
        let mut data = Vec::new();
        BorshSerialize::serialize(&minimal, &mut data).unwrap();

        let num_entries = 0;
        let size_validators = Validators::required_bytes(num_entries);
        let size_maintainers = Maintainers::required_bytes(num_entries);

        assert_eq!(
            data.len() - size_validators - size_maintainers,
            LIDO_CONSTANT_SIZE
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
            exchange_rate: ExchangeRate {
                computed_in_epoch: 11,
                sol_balance: Lamports(13),
                st_sol_supply: StLamports(17),
            },
            sol_reserve_account_bump_seed: 1,
            stake_authority_bump_seed: 2,
            mint_authority_bump_seed: 3,
            rewards_withdraw_authority_bump_seed: 4,
            reward_distribution: RewardDistribution {
                treasury_fee: 2,
                validation_fee: 3,
                developer_fee: 4,
                st_sol_appreciation: 7,
            },
            fee_recipients: FeeRecipients {
                treasury_account: Pubkey::new_unique(),
                developer_account: Pubkey::new_unique(),
            },
            metrics: Metrics::new(),
            validators: validators,
            maintainers: maintainers,
        };
        let mut data = Vec::new();
        BorshSerialize::serialize(&lido, &mut data).unwrap();

        let lido_restored = try_from_slice_unchecked(&data[..]).unwrap();
        assert_eq!(lido, lido_restored);
    }

    #[test]
    fn test_exchange_when_balance_and_supply_are_zero() {
        let rate = ExchangeRate {
            computed_in_epoch: 0,
            sol_balance: Lamports(0),
            st_sol_supply: StLamports(0),
        };
        assert_eq!(rate.exchange_sol(Lamports(123)), Ok(StLamports(123)));
    }

    #[test]
    fn test_exchange_when_rate_is_one_to_two() {
        let rate = ExchangeRate {
            computed_in_epoch: 0,
            sol_balance: Lamports(2),
            st_sol_supply: StLamports(1),
        };
        // If every stSOL is worth 1 SOL, I should get half my SOL amount in stSOL.
        assert_eq!(rate.exchange_sol(Lamports(44)), Ok(StLamports(22)));
    }

    #[test]
    fn test_exchange_when_one_balance_is_zero() {
        // This case can occur when we donate some SOL to Lido, instead of
        // depositing it. There will not be any stSOL, but there will be SOL.
        // In this case it doesn't matter which exchange rate we use, the first
        // deposits will mint some stSOL, and that stSOL will own all of the
        // pool. The rate we choose is only nominal, it controls the initial
        // stSOL:SOL rate, and we choose it to be 1:1.
        let rate = ExchangeRate {
            computed_in_epoch: 0,
            sol_balance: Lamports(100),
            st_sol_supply: StLamports(0),
        };
        assert_eq!(rate.exchange_sol(Lamports(123)), Ok(StLamports(123)));

        // This case should not occur in the wild, but in any case, use a 1:1 rate here too.
        let rate = ExchangeRate {
            computed_in_epoch: 0,
            sol_balance: Lamports(0),
            st_sol_supply: StLamports(100),
        };
        assert_eq!(rate.exchange_sol(Lamports(123)), Ok(StLamports(123)));
    }

    #[test]
    fn test_exchange_sol_to_st_sol_to_sol_roundtrips() {
        // There are many cases where depositing some amount of SOL and then
        // exchanging it back, does not actually roundtrip. There can be small
        // losses due to integer arithmetic rounding, but there can even be large
        // losses, if the sol_balance and st_sol_supply are very different. For
        // example, if sol_balance = 10, st_sol_supply = 1, then if you deposit
        // 9 Lamports, you are entitled to 0.1 stLamports, which gets rounded
        // down to 0, and you lose your full 9 Lamports.
        // So here we test a few of those cases as a sanity check, but it's not
        // a general roundtripping test.
        let rate = ExchangeRate {
            computed_in_epoch: 0,
            sol_balance: Lamports(100),
            st_sol_supply: StLamports(50),
        };
        let sol_1 = Lamports(10);
        let st_sol = rate.exchange_sol(sol_1).unwrap();
        let sol_2 = rate.exchange_st_sol(st_sol).unwrap();
        assert_eq!(sol_2, sol_1);

        // In this case, one Lamport is lost in a rounding error, because
        // `amount * st_sol_supply` is not a multiple of `sol_balance`.
        let rate = ExchangeRate {
            computed_in_epoch: 0,
            sol_balance: Lamports(110_000),
            st_sol_supply: StLamports(100_000),
        };
        let sol_1 = Lamports(1_000);
        let st_sol = rate.exchange_sol(sol_1).unwrap();
        let sol_2 = rate.exchange_st_sol(st_sol).unwrap();
        assert_eq!(sol_2, Lamports(999));
    }

    #[test]
    fn test_lido_for_deposit_wrong_mint() {
        let mut lido = Lido::default();
        lido.st_sol_mint = Pubkey::new_unique();

        let pubkey = Pubkey::new_unique();
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
    fn test_get_sol_balance() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let rent = &Rent::default();
        let mut lido = Lido::default();
        let key = Pubkey::default();
        let mut amount = rent.minimum_balance(0);
        let mut reserve_account =
            AccountInfo::new(&key, true, true, &mut amount, &mut [], &key, false, 0);

        assert_eq!(
            lido.get_sol_balance(&rent, &reserve_account),
            Ok(Lamports(0))
        );

        let mut new_amount = rent.minimum_balance(0) + 10;
        reserve_account.lamports = Rc::new(RefCell::new(&mut new_amount));

        assert_eq!(
            lido.get_sol_balance(&rent, &reserve_account),
            Ok(Lamports(10))
        );

        lido.validators.maximum_entries = 1;
        lido.validators
            .add(Pubkey::new_unique(), Validator::new(Pubkey::new_unique()))
            .unwrap();
        lido.validators.entries[0].entry.stake_accounts_balance = Lamports(37);
        assert_eq!(
            lido.get_sol_balance(&rent, &reserve_account),
            Ok(Lamports(10 + 37))
        );

        lido.validators.entries[0].entry.stake_accounts_balance = Lamports(u64::MAX);

        assert_eq!(
            lido.get_sol_balance(&rent, &reserve_account),
            Err(LidoError::CalculationFailure)
        );

        let mut new_amount = u64::MAX;
        reserve_account.lamports = Rc::new(RefCell::new(&mut new_amount));
        // The amount here is more than the rent exemption that gets discounted
        // from the reserve, causing an overflow.
        lido.validators.entries[0].entry.stake_accounts_balance = Lamports(5_000_000);

        assert_eq!(
            lido.get_sol_balance(&rent, &reserve_account),
            Err(LidoError::CalculationFailure)
        );
    }

    #[test]
    fn test_get_st_sol_supply() {
        use solana_program::program_option::COption;

        let mint = Mint {
            mint_authority: COption::None,
            supply: 200_000,
            decimals: 9,
            is_initialized: true,
            freeze_authority: COption::None,
        };
        let mut data = [0_u8; 128];
        mint.pack_into_slice(&mut data);

        let mut lido = Lido::default();
        let mint_address = Pubkey::default();
        let mut amount = 0;
        let is_signer = false;
        let is_writable = false;
        let executable = false;
        let rent_epoch = 0;
        let st_sol_mint = AccountInfo::new(
            &mint_address,
            is_signer,
            is_writable,
            &mut amount,
            &mut data,
            &mint_address,
            executable,
            rent_epoch,
        );

        lido.st_sol_mint = mint_address;

        assert_eq!(
            lido.get_st_sol_supply(&st_sol_mint),
            Ok(StLamports(200_000)),
        );

        lido.validators.maximum_entries = 1;
        lido.validators
            .add(Pubkey::new_unique(), Validator::new(Pubkey::new_unique()))
            .unwrap();
        lido.validators.entries[0].entry.fee_credit = StLamports(37);
        assert_eq!(
            lido.get_st_sol_supply(&st_sol_mint),
            Ok(StLamports(200_000 + 37))
        );

        lido.st_sol_mint = Pubkey::new_unique();

        assert_eq!(
            lido.get_st_sol_supply(&st_sol_mint),
            Err(LidoError::InvalidStSolAccount.into())
        );
    }

    #[test]
    fn test_split_reward() {
        let mut spec = RewardDistribution {
            treasury_fee: 3,
            validation_fee: 2,
            developer_fee: 1,
            st_sol_appreciation: 0,
        };

        assert_eq!(
            // In this case the amount can be split exactly,
            // there is no remainder.
            spec.split_reward(Lamports(600), 1).unwrap(),
            Fees {
                treasury_amount: Lamports(300),
                reward_per_validator: Lamports(200),
                developer_amount: Lamports(100),
                st_sol_appreciation_amount: Lamports(0),
            },
        );

        assert_eq!(
            // In this case the amount cannot be split exactly, all fees are
            // rounded down.
            spec.split_reward(Lamports(1_000), 4).unwrap(),
            Fees {
                treasury_amount: Lamports(500),
                reward_per_validator: Lamports(83),
                developer_amount: Lamports(166),
                st_sol_appreciation_amount: Lamports(2),
            },
        );

        // If we use 3%, 2%, 1% fee, and the remaining 94% go to stSOL appreciation,
        // we should see 3%, 2%, and 1% fee.
        spec.st_sol_appreciation = 94;
        assert_eq!(
            spec.split_reward(Lamports(100), 1).unwrap(),
            Fees {
                treasury_amount: Lamports(3),
                reward_per_validator: Lamports(2),
                developer_amount: Lamports(1),
                st_sol_appreciation_amount: Lamports(94),
            },
        );

        let spec_coprime = RewardDistribution {
            treasury_fee: 17,
            validation_fee: 23,
            developer_fee: 19,
            st_sol_appreciation: 0,
        };
        assert_eq!(
            spec_coprime.split_reward(Lamports(1_000), 1).unwrap(),
            Fees {
                treasury_amount: Lamports(288),
                reward_per_validator: Lamports(389),
                developer_amount: Lamports(322),
                st_sol_appreciation_amount: Lamports(1),
            },
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
