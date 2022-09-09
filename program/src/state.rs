// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! State transition types

use std::convert::TryFrom;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Range;

use serde::Serialize;

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::{
    account_info::AccountInfo,
    borsh::{get_instance_packed_len, try_from_slice_unchecked},
    clock::Clock,
    clock::Epoch,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_memory::sol_memcmp,
    program_pack::Pack,
    program_pack::Sealed,
    pubkey::{Pubkey, PUBKEY_BYTES},
    rent::Rent,
    sysvar::Sysvar,
};
use spl_token::state::Mint;

use crate::big_vec::BigVec;
use crate::error::LidoError;
use crate::logic::{check_account_owner, get_reserve_available_balance};
use crate::metrics::Metrics;
use crate::processor::StakeType;
use crate::token::{self, Lamports, Rational, StLamports};
use crate::util::serialize_b58;
use crate::{
    MINIMUM_STAKE_ACCOUNT_BALANCE, MINT_AUTHORITY, RESERVE_ACCOUNT, STAKE_AUTHORITY,
    VALIDATOR_STAKE_ACCOUNT, VALIDATOR_UNSTAKE_ACCOUNT,
};

/// Types of list entries
/// Uninitialized should always be a first enum field as it catches empty list data errors
#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize, Serialize, BorshSchema)]
pub enum AccountType {
    /// If the account has not been initialized, the enum will be 0
    Uninitialized,
    Lido,
    Validator,
    Maintainer,
}

impl Default for AccountType {
    fn default() -> Self {
        AccountType::Uninitialized
    }
}

/// Storage list for accounts in the pool.
/// It is used to serialize account list on stake pool initialization
#[repr(C)]
#[derive(
    Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema, Serialize,
)]
pub struct AccountList<T> {
    /// Data outside of the list, separated out for cheaper deserializations.
    /// Must be a first field to avoid account confusion.
    #[serde(skip_serializing)]
    pub header: ListHeader<T>,

    /// List of account in the pool
    pub entries: Vec<T>,
}

pub type ValidatorList = AccountList<Validator>;
pub type MaintainerList = AccountList<Maintainer>;

/// Helper type to deserialize just the start of AccountList
#[repr(C)]
#[derive(
    Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema, Serialize,
)]
pub struct ListHeader<T> {
    /// Account type, must be a first field to avoid account confusion
    pub account_type: AccountType,

    /// Version number for the Lido
    pub lido_version: u8,

    /// Maximum allowable number of elements
    pub max_entries: u32,

    phantom: PhantomData<T>,
}

/// Generic element of a list
pub trait ListEntry: Pack + Default + Clone + BorshSerialize + PartialEq + Debug {
    const TYPE: AccountType;

    fn new(pubkey: Pubkey) -> Self;
    fn pubkey(&self) -> &Pubkey;

    /// Performs a very cheap comparison, for checking if entry
    /// info matches the account address.
    /// First PUBKEY_BYTES of a ListEntry data should be the account address
    fn memcmp_pubkey(data: &[u8], pubkey: &[u8]) -> bool {
        sol_memcmp(&data[..PUBKEY_BYTES], pubkey, PUBKEY_BYTES) == 0
    }
}

impl<T> AccountList<T>
where
    T: Default + Clone + ListEntry + BorshSerialize,
{
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Create an empty instance containing space for `max_entries` with default values
    pub fn new_default(max_entries: u32) -> Self {
        Self {
            header: ListHeader::<T> {
                account_type: T::TYPE,
                max_entries,
                lido_version: Lido::VERSION,
                phantom: PhantomData,
            },
            entries: vec![T::default(); max_entries as usize],
        }
    }

    /// Create a new list of accounts by copying from `data`. Do not use on-chain.
    pub fn from(data: &mut [u8]) -> Result<Self, ProgramError> {
        let (header, big_vec) = ListHeader::<T>::deserialize_vec(data)?;
        Ok(Self {
            header,
            entries: big_vec.iter().cloned().collect(),
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.iter()
    }

    fn header_size() -> usize {
        // + 4 bytes for entries len
        ListHeader::<T>::LEN + std::mem::size_of::<u32>()
    }

    /// Calculate the number of account entries that fit in the provided length
    pub fn calculate_max_entries(buffer_length: usize) -> usize {
        buffer_length.saturating_sub(Self::header_size()) / T::LEN
    }

    /// Calculate the number of bytes required for max_entries
    pub fn required_bytes(max_entries: u32) -> usize {
        Self::header_size() + T::LEN * max_entries as usize
    }

    /// Check if contains an account with particular pubkey
    pub fn find(&self, pubkey: &Pubkey) -> Option<&T> {
        self.entries.iter().find(|x| x.pubkey() == pubkey)
    }

    /// Get index of list entry with pubkey.
    /// Panics if list is too big, not used on-chain
    pub fn position(&self, pubkey: &Pubkey) -> Option<u32> {
        self.entries
            .iter()
            .position(|v| v.pubkey() == pubkey)
            .map(u32::try_from)
            .map(Result::unwrap)
    }

    /// Serialize to AccountInfo data
    pub fn save(&self, account: &AccountInfo) -> ProgramResult {
        BorshSerialize::serialize(self, &mut *account.data.borrow_mut())?;
        Ok(())
    }
}

/// Check Lido version
pub fn check_lido_version(version: u8, account_type: AccountType) -> ProgramResult {
    if version != Lido::VERSION {
        msg!(
            "Lido version mismatch for {:?}. Current version {}, should be {}",
            account_type,
            version,
            Lido::VERSION
        );
        return Err(LidoError::LidoVersionMismatch.into());
    }
    Ok(())
}

/// Represents list of accounts as a view of raw bytes.
/// Main data structure to use on-chain for account lists
pub struct BigVecWithHeader<'data, T> {
    pub header: ListHeader<T>,
    big_vec: BigVec<'data, T>,
}

impl<'data, T: ListEntry> BigVecWithHeader<'data, T> {
    pub fn new(header: ListHeader<T>, big_vec: BigVec<'data, T>) -> Self {
        Self { header, big_vec }
    }

    pub fn len(&self) -> u32 {
        self.big_vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.big_vec.is_empty()
    }

    pub fn iter(&'data self) -> impl Iterator<Item = &'data T> {
        self.big_vec.iter()
    }

    pub fn find(&'data self, pubkey: &Pubkey) -> Result<&'data T, LidoError> {
        self.big_vec
            .find(&pubkey.to_bytes(), T::memcmp_pubkey)
            .ok_or(LidoError::InvalidAccountMember)
    }

    /// Appends to the list only if unique
    pub fn push(&mut self, value: T) -> ProgramResult {
        if self.header.max_entries == self.len() {
            msg!("Can't append to {:?} list as it has no free space", T::TYPE);
            return Err(LidoError::MaximumNumberOfAccountsExceeded.into());
        }

        if self.find(value.pubkey()).is_ok() {
            msg!(
                "Pubkey {} is duplicated in a {:?} list",
                value.pubkey(),
                T::TYPE
            );
            return Err(LidoError::DuplicatedEntry.into());
        };
        self.big_vec.push(value)
    }

    /// Check if list element pubkey matches requested pubkey
    fn check_pubkey(element: &T, pubkey: &Pubkey) -> ProgramResult {
        if element.pubkey() != pubkey {
            msg!(
                "{:?} list index does not match pubkey. Please supply a valid index or try again.",
                T::TYPE
            );
            return Err(LidoError::PubkeyIndexMismatch.into());
        }
        Ok(())
    }

    /// Get element with pubkey at index
    pub fn get_mut(
        &'data mut self,
        index: u32,
        pubkey: &Pubkey,
    ) -> Result<&'data mut T, ProgramError> {
        let element = self.big_vec.get_mut(index)?;
        Self::check_pubkey(element, pubkey)?;
        Ok(element)
    }

    /// Removes element with pubkey at index
    pub fn remove(&'data mut self, index: u32, pubkey: &Pubkey) -> Result<T, ProgramError> {
        let element = self.big_vec.get_mut(index)?;
        Self::check_pubkey(element, pubkey)?;
        self.big_vec.swap_remove(index)
    }
}

impl<T: ListEntry> ListHeader<T> {
    const LEN: usize =
        std::mem::size_of::<u32>() + std::mem::size_of::<AccountType>() + std::mem::size_of::<u8>();

    pub fn deserialize_checked(data: &[u8]) -> Result<Self, ProgramError> {
        let mut data = data;
        let header = Self::deserialize(&mut data)?;

        check_lido_version(header.lido_version, T::TYPE)?;

        // check ListEntryType
        if header.account_type != T::TYPE {
            msg!(
                "Invalid account type when deserializing list header, found {:?}, should be {:?}",
                header.account_type,
                T::TYPE
            );
            return Err(LidoError::InvalidAccountType.into());
        }
        Ok(header)
    }

    /// Extracts the account list into its header and internal BigVec
    pub fn deserialize_vec(data: &mut [u8]) -> Result<(Self, BigVec<T>), ProgramError> {
        let header = Self::deserialize_checked(data)?;
        let big_vec: BigVec<T> =
            BigVec::new(&mut data[Self::LEN..AccountList::<T>::required_bytes(header.max_entries)]);
        Ok((header, big_vec))
    }
}

impl ValidatorList {
    pub fn iter_active(&self) -> impl Iterator<Item = &Validator> {
        self.entries.iter().filter(|&v| v.active)
    }
}

/// NOTE: ORDER IS VERY IMPORTANT HERE, PLEASE DO NOT RE-ORDER THE FIELDS UNLESS
/// THERE'S AN EXTREMELY GOOD REASON.
///
/// To save on BPF instructions, the serialized bytes are reinterpreted with an
/// unsafe pointer cast, which means that this structure cannot have any
/// undeclared alignment-padding in its representation.
#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema, Serialize)]
pub struct Validator {
    /// Validator vote account address.
    /// Do not reorder this field, it should be first in the struct
    #[serde(serialize_with = "serialize_b58")]
    #[serde(rename = "pubkey")]
    pub vote_account_address: Pubkey,

    /// Seeds for active stake accounts.
    pub stake_seeds: SeedRange,
    /// Seeds for inactive stake accounts.
    pub unstake_seeds: SeedRange,

    /// Sum of the balances of the stake accounts and unstake accounts.
    pub stake_accounts_balance: Lamports,

    /// Sum of the balances of the unstake accounts.
    pub unstake_accounts_balance: Lamports,

    /// Effective stake balance is stake_accounts_balance - unstake_accounts_balance.
    /// The result is stored on-chain to optimize compute budget
    pub effective_stake_balance: Lamports,

    /// Controls if a validator is allowed to have new stake deposits.
    /// When removing a validator, this flag should be set to `false`.
    pub active: bool,
}

/// NOTE: ORDER IS VERY IMPORTANT HERE, PLEASE DO NOT RE-ORDER THE FIELDS UNLESS
/// THERE'S AN EXTREMELY GOOD REASON.
///
/// To save on BPF instructions, the serialized bytes are reinterpreted with an
/// unsafe pointer cast, which means that this structure cannot have any
/// undeclared alignment-padding in its representation.
#[repr(C)]
#[derive(
    Clone, Default, Debug, Eq, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema, Serialize,
)]
pub struct Maintainer {
    /// Address of maintainer account.
    /// Do not reorder this field, it should be first in the struct
    #[serde(serialize_with = "serialize_b58")]
    pub pubkey: Pubkey,
}

impl Validator {
    /// Return the balance in only the stake accounts, excluding the unstake accounts.
    pub fn compute_effective_stake_balance(&self) -> Lamports {
        (self.stake_accounts_balance - self.unstake_accounts_balance)
            .expect("Unstake balance cannot exceed the validator's total stake balance.")
    }

    pub fn observe_balance(observed: Lamports, tracked: Lamports, info: &str) -> ProgramResult {
        if observed < tracked {
            msg!(
                "{}: observed balance of {} is less than tracked balance of {}.",
                info,
                observed,
                tracked
            );
            msg!("This should not happen, aborting ...");
            return Err(LidoError::ValidatorBalanceDecreased.into());
        }
        Ok(())
    }

    pub fn has_stake_accounts(&self) -> bool {
        self.stake_seeds.begin != self.stake_seeds.end
    }
    pub fn has_unstake_accounts(&self) -> bool {
        self.unstake_seeds.begin != self.unstake_seeds.end
    }

    pub fn check_can_be_removed(&self) -> Result<(), LidoError> {
        if self.active {
            return Err(LidoError::ValidatorIsStillActive);
        }
        if self.has_stake_accounts() {
            return Err(LidoError::ValidatorShouldHaveNoStakeAccounts);
        }
        if self.has_unstake_accounts() {
            return Err(LidoError::ValidatorShouldHaveNoUnstakeAccounts);
        }
        // If not, this is a bug.
        assert_eq!(self.stake_accounts_balance, Lamports(0));
        Ok(())
    }

    pub fn show_removed_error_msg(error: &Result<(), LidoError>) {
        if let Err(err) = error {
            match err {
                LidoError::ValidatorIsStillActive => {
                    msg!(
                                "Refusing to remove validator because it is still active, deactivate it first."
                            );
                }
                LidoError::ValidatorHasUnclaimedCredit => {
                    msg!(
                        "Validator still has tokens to claim. Reclaim tokens before removing the validator"
                    );
                }
                LidoError::ValidatorShouldHaveNoStakeAccounts => {
                    msg!("Refusing to remove validator because it still has stake accounts, unstake them first.");
                }
                LidoError::ValidatorShouldHaveNoUnstakeAccounts => {
                    msg!("Refusing to remove validator because it still has unstake accounts, withdraw them first.");
                }
                _ => {
                    msg!("Invalid error when removing a validator: shouldn't happen.");
                }
            }
        }
    }

    pub fn find_stake_account_address_with_authority(
        &self,
        program_id: &Pubkey,
        solido_account: &Pubkey,
        authority: &[u8],
        seed: u64,
    ) -> (Pubkey, u8) {
        let seeds = [
            &solido_account.to_bytes(),
            &self.vote_account_address.to_bytes(),
            authority,
            &seed.to_le_bytes()[..],
        ];
        Pubkey::find_program_address(&seeds, program_id)
    }

    pub fn find_stake_account_address(
        &self,
        program_id: &Pubkey,
        solido_account: &Pubkey,
        seed: u64,
        stake_type: StakeType,
    ) -> (Pubkey, u8) {
        let authority = match stake_type {
            StakeType::Stake => VALIDATOR_STAKE_ACCOUNT,
            StakeType::Unstake => VALIDATOR_UNSTAKE_ACCOUNT,
        };
        self.find_stake_account_address_with_authority(program_id, solido_account, authority, seed)
    }

    /// Get stake account address that should be merged into another right after creation.
    /// This function should be used to create temporary stake accounts
    /// tied to the epoch that should be merged into another account and destroyed
    /// after a transaction. So that each epoch would have a diferent
    /// generation of stake accounts. This is done for security purpose
    pub fn find_temporary_stake_account_address(
        &self,
        program_id: &Pubkey,
        solido_account: &Pubkey,
        seed: u64,
        epoch: Epoch,
    ) -> (Pubkey, u8) {
        let authority = [VALIDATOR_STAKE_ACCOUNT, &epoch.to_le_bytes()[..]].concat();
        self.find_stake_account_address_with_authority(program_id, solido_account, &authority, seed)
    }
}

impl Sealed for Validator {}

impl Pack for Validator {
    const LEN: usize = 89;
    fn pack_into_slice(&self, data: &mut [u8]) {
        let mut data = data;
        BorshSerialize::serialize(&self, &mut data).unwrap();
    }
    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let unpacked = Self::try_from_slice(src)?;
        Ok(unpacked)
    }
}

impl Default for Validator {
    fn default() -> Self {
        Validator {
            stake_seeds: SeedRange { begin: 0, end: 0 },
            unstake_seeds: SeedRange { begin: 0, end: 0 },
            stake_accounts_balance: Lamports(0),
            unstake_accounts_balance: Lamports(0),
            effective_stake_balance: Lamports(0),
            active: true,
            vote_account_address: Pubkey::default(),
        }
    }
}

impl ListEntry for Validator {
    const TYPE: AccountType = AccountType::Validator;

    fn new(vote_account_address: Pubkey) -> Self {
        Self {
            vote_account_address,
            ..Default::default()
        }
    }

    fn pubkey(&self) -> &Pubkey {
        &self.vote_account_address
    }
}

impl Sealed for Maintainer {}

impl Pack for Maintainer {
    const LEN: usize = PUBKEY_BYTES;
    fn pack_into_slice(&self, data: &mut [u8]) {
        let mut data = data;
        BorshSerialize::serialize(&self, &mut data).unwrap();
    }
    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let unpacked = Self::try_from_slice(src)?;
        Ok(unpacked)
    }
}

impl ListEntry for Maintainer {
    const TYPE: AccountType = AccountType::Maintainer;

    fn new(pubkey: Pubkey) -> Self {
        Self { pubkey }
    }

    fn pubkey(&self) -> &Pubkey {
        &self.pubkey
    }
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
    /// Account type, must be a first field to avoid account confusion
    pub account_type: AccountType,

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

    /// Validator list account
    #[serde(serialize_with = "serialize_b58")]
    pub validator_list: Pubkey,

    /// Maintainer list account
    ///
    /// Maintainers are granted low security risk privileges. Maintainers are
    /// expected to run the maintenance daemon, that invokes the maintenance
    /// operations. These are gated on the signer being present in this set.
    /// In the future we plan to make maintenance operations callable by anybody.
    #[serde(serialize_with = "serialize_b58")]
    pub maintainer_list: Pubkey,

    /// Maximum validation commission percentage in [0, 100]
    pub max_commission_percentage: u8,
}

impl Lido {
    pub const VERSION: u8 = 1;

    /// Size of a serialized `Lido` struct excluding validators and maintainers.
    ///
    /// To update this, run the tests and replace the value here with the test output.
    pub const LEN: usize = 418;

    pub fn deserialize_lido(program_id: &Pubkey, lido: &AccountInfo) -> Result<Lido, ProgramError> {
        check_account_owner(lido, program_id)?;

        let lido = try_from_slice_unchecked::<Lido>(&lido.data.borrow())?;
        if lido.account_type != AccountType::Lido {
            msg!(
                "Lido account type should be {:?}, but is {:?}",
                AccountType::Lido,
                lido.account_type
            );
            return Err(LidoError::InvalidAccountType.into());
        }

        check_lido_version(lido.lido_version, AccountType::Lido)?;

        Ok(lido)
    }

    /// Calculates the total size of Lido
    pub fn calculate_size() -> usize {
        let lido_instance = Lido {
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
        if token_account_info.owner != &spl_token::id() {
            msg!(
                "Expected SPL token account to be owned by {}, but it's owned by {} instead.",
                spl_token::id(),
                token_account_info.owner
            );
            return Err(LidoError::InvalidStSolAccountOwner.into());
        }
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

        let rent: Rent = Rent::get()?;

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
        validator: &Validator,
        stake_account_seed: u64,
        stake_account: &AccountInfo,
        authority: &[u8],
    ) -> Result<u8, ProgramError> {
        let (stake_addr, stake_addr_bump_seed) = validator
            .find_stake_account_address_with_authority(
                program_id,
                solido_address,
                authority,
                stake_account_seed,
            );
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
    pub fn get_sol_balance<'data, I>(
        validators: I,
        rent: &Rent,
        reserve: &AccountInfo,
    ) -> Result<Lamports, LidoError>
    where
        I: Iterator<Item = &'data Validator>,
    {
        let effective_reserve_balance = get_reserve_available_balance(rent, reserve)?;

        // The remaining SOL managed is all in stake accounts.
        let validator_balance: token::Result<Lamports> =
            validators.map(|v| v.stake_accounts_balance).sum();

        let result = validator_balance.and_then(|s| s + effective_reserve_balance)?;

        Ok(result)
    }

    /// Return the total amount of stSOL in existence.
    ///
    /// The total is the amount minted so far
    pub fn get_st_sol_supply(&self, st_sol_mint: &AccountInfo) -> Result<StLamports, ProgramError> {
        self.check_mint_is_st_sol_mint(st_sol_mint)?;

        let st_sol_mint = Mint::unpack_from_slice(&st_sol_mint.data.borrow())?;
        let minted_supply = StLamports(st_sol_mint.supply);

        Ok(minted_supply)
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

    /// Checks if the maintainer belongs to the list of maintainers
    pub fn check_maintainer(
        &self,
        program_id: &Pubkey,
        maintainer_list: &AccountInfo,
        maintainer_index: u32,
        maintainer: &AccountInfo,
    ) -> ProgramResult {
        let data = &mut *maintainer_list.data.borrow_mut();
        let mut maintainer_list =
            self.deserialize_account_list_info::<Maintainer>(program_id, maintainer_list, data)?;

        if maintainer_list
            .get_mut(maintainer_index, maintainer.key)
            .is_err()
        {
            msg!(
                "Invalid maintainer, account {} is not present in the maintainers list.",
                maintainer.key
            );

            return Err(LidoError::InvalidMaintainer.into());
        }

        Ok(())
    }

    /// Checks if account list belongs to Lido
    pub fn check_account_list_info<T: ListEntry>(
        &self,
        program_id: &Pubkey,
        list_address: &Pubkey,
        account_list_info: &AccountInfo,
    ) -> ProgramResult {
        check_account_owner(account_list_info, program_id)?;

        // check account_list belongs to Lido
        if list_address != account_list_info.key {
            msg!(
                "{:?} list address {} is different from Lido's {}",
                T::TYPE,
                account_list_info.key,
                list_address
            );
            return Err(LidoError::InvalidListAccount.into());
        }

        Ok(())
    }

    /// Check account list info and deserialize the account data
    pub fn deserialize_account_list_info<'data, T: ListEntry>(
        &self,
        program_id: &Pubkey,
        account_list_info: &AccountInfo,
        account_list_data: &'data mut [u8],
    ) -> Result<BigVecWithHeader<'data, T>, ProgramError> {
        let solido_list_address = match T::TYPE {
            AccountType::Validator => self.validator_list,
            AccountType::Maintainer => self.maintainer_list,
            _ => {
                msg!(
                    "Invalid account type {:?} when deserializing account list",
                    T::TYPE,
                );
                return Err(LidoError::InvalidAccountType.into());
            }
        };
        self.check_account_list_info::<T>(program_id, &solido_list_address, account_list_info)?;
        let (header, big_vec) = ListHeader::<T>::deserialize_vec(account_list_data)?;
        Ok(BigVecWithHeader::new(header, big_vec))
    }
}

#[repr(C)]
#[derive(
    Clone, Debug, Default, Eq, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema, Serialize,
)]
pub struct SeedRange {
    /// Start (inclusive) of the seed range for stake accounts.
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
    /// the `end` seed, and depositing active stake is only allowed from the
    /// `begin` seed. This ensures that maintainers don’t race and accidentally
    /// stake more to this validator than intended. If the seed has changed
    /// since the instruction was created, the transaction fails.
    ///
    /// When we unstake SOL, we follow an analogous symmetric mechanism. We
    /// split the validator's stake in two, and retrieve the funds of the second
    /// to the reserve account where it can be re-staked.
    pub begin: u64,

    /// End (exclusive) of the seed range for stake accounts.
    pub end: u64,
}

impl IntoIterator for &SeedRange {
    type Item = u64;
    type IntoIter = Range<u64>;

    fn into_iter(self) -> Self::IntoIter {
        Range {
            start: self.begin,
            end: self.end,
        }
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
        self.treasury_fee as u64 + self.developer_fee as u64 + self.st_sol_appreciation as u64
    }

    pub fn treasury_fraction(&self) -> Rational {
        Rational {
            numerator: self.treasury_fee as u64,
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
    pub fn split_reward(&self, amount: Lamports) -> token::Result<Fees> {
        use std::ops::Add;

        let treasury_amount = (amount * self.treasury_fraction())?;
        let developer_amount = (amount * self.developer_fraction())?;

        // Sanity check: We should not produce more fees than we had to split in
        // the first place.
        let total_fees = Lamports(0).add(treasury_amount)?.add(developer_amount)?;
        assert!(total_fees <= amount);

        let st_sol_appreciation_amount = (amount - total_fees)?;

        let result = Fees {
            treasury_amount,
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
    pub developer_amount: Lamports,

    /// Remainder of the reward.
    ///
    /// This is not a fee, and it is not paid out explicitly, but when summed
    /// with the other fields in this struct, that totals the input amount.
    pub st_sol_appreciation_amount: Lamports,
}

/// The different ways to stake some amount from the reserve.
pub enum StakeDeposit {
    /// Stake into a new stake account, and delegate the new account.
    ///
    /// This consumes the end seed of the validator's stake accounts.
    Append,

    /// Stake into temporary stake account, and immediately merge it.
    ///
    /// This merges into the stake account at `end_seed - 1`.
    Merge,
}

/////////////////////////////////////////////////// OLD STATE ///////////////////////////////////////////////////

/// An entry in `AccountMap`.
#[derive(Clone, Debug, BorshDeserialize, BorshSchema)]
pub struct PubkeyAndEntry<T> {
    pub pubkey: Pubkey,
    pub entry: T,
}

/// A map from public key to `T`, implemented as a vector of key-value pairs.
#[derive(Clone, Debug, BorshDeserialize, BorshSchema)]
pub struct AccountMap<T> {
    pub entries: Vec<PubkeyAndEntry<T>>,
    pub maximum_entries: u32,
}

#[repr(C)]
#[derive(Clone, Debug, BorshDeserialize, BorshSchema)]
pub struct ValidatorV1 {
    pub fee_credit: StLamports,
    pub fee_address: Pubkey,
    pub stake_seeds: SeedRange,
    pub unstake_seeds: SeedRange,
    pub stake_accounts_balance: Lamports,
    pub unstake_accounts_balance: Lamports,
    pub active: bool,
}

#[derive(Clone, Debug, BorshDeserialize, BorshSchema)]
pub struct RewardDistributionV1 {
    pub treasury_fee: u32,
    pub validation_fee: u32,
    pub developer_fee: u32,
    pub st_sol_appreciation: u32,
}

#[repr(C)]
#[derive(Clone, Debug, BorshDeserialize, BorshSchema)]
pub struct LidoV1 {
    pub lido_version: u8,
    pub manager: Pubkey,
    pub st_sol_mint: Pubkey,
    pub exchange_rate: ExchangeRate,
    pub sol_reserve_account_bump_seed: u8,
    pub stake_authority_bump_seed: u8,
    pub mint_authority_bump_seed: u8,
    pub rewards_withdraw_authority_bump_seed: u8,
    pub reward_distribution: RewardDistributionV1,
    pub fee_recipients: FeeRecipients,
    pub metrics: Metrics,
    pub validators: AccountMap<ValidatorV1>,
    pub maintainers: AccountMap<()>,
}

impl LidoV1 {
    pub fn deserialize_lido(
        program_id: &Pubkey,
        lido: &AccountInfo,
    ) -> Result<LidoV1, ProgramError> {
        check_account_owner(lido, program_id)?;
        let lido = try_from_slice_unchecked::<LidoV1>(&lido.data.borrow())?;
        Ok(lido)
    }

    /// Checks if the passed manager is the same as the one stored in the state
    pub fn check_manager(&self, manager: &AccountInfo) -> ProgramResult {
        if &self.manager != manager.key {
            msg!("Invalid manager, not the same as the one stored in state");
            return Err(LidoError::InvalidManager.into());
        }
        Ok(())
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod test_lido {
    use super::*;
    use solana_program::program_error::ProgramError;

    #[test]
    fn test_account_map_required_bytes_relates_to_maximum_entries() {
        for buffer_size in 0..8_000 {
            let max_entries = ValidatorList::calculate_max_entries(buffer_size);
            let needed_size = ValidatorList::required_bytes(max_entries as u32);
            assert!(
                needed_size <= buffer_size || max_entries == 0,
                "Buffer of len {} can fit {} validators which need {} bytes.",
                buffer_size,
                max_entries,
                needed_size,
            );
        }
    }

    #[test]
    fn test_validators_size() {
        let validator = get_instance_packed_len(&Validator::default()).unwrap();
        assert_eq!(validator, Validator::LEN);
        let one_len = get_instance_packed_len(&ValidatorList::new_default(1)).unwrap();
        let two_len = get_instance_packed_len(&ValidatorList::new_default(2)).unwrap();
        assert_eq!(one_len, ValidatorList::required_bytes(1));
        assert_eq!(two_len, ValidatorList::required_bytes(2));
        assert_eq!(two_len - one_len, Validator::LEN);
    }

    #[test]
    fn test_lido_constant_size() {
        // The minimal size of the struct is its size without any validators and
        // maintainers.
        let minimal = Lido::default();
        let mut data = Vec::new();
        BorshSerialize::serialize(&minimal, &mut data).unwrap();
        assert_eq!(data.len(), Lido::LEN);
    }

    #[test]
    fn test_lido_serialization_roundtrips() {
        use solana_sdk::borsh::try_from_slice_unchecked;

        fn test_list<T: ListEntry>() {
            // create empty account list with Vec
            let mut accounts = AccountList::<T>::new_default(0);
            accounts.header.max_entries = 100;

            // allocate space for future elements
            let mut buffer: Vec<u8> =
                vec![0; AccountList::<T>::required_bytes(accounts.header.max_entries)];
            let mut slice = &mut buffer[..];
            // seriaslize empty list to buffer, which serializes a header and lenght
            BorshSerialize::serialize(&accounts, &mut slice).unwrap();

            // deserialize to BigVec
            let slice = &mut buffer[..];
            let (header, big_vec) = ListHeader::<T>::deserialize_vec(slice).unwrap();
            let mut account_list = BigVecWithHeader::new(header, big_vec);

            for _ in 0..accounts.header.max_entries {
                // add same account to both Vec and BigVec
                let new_account = T::new(Pubkey::new_unique());
                account_list.push(new_account.clone()).unwrap();
                accounts.entries.push(new_account);
            }

            // restore from BigVec to Vec and compare
            let slice = &mut buffer[..];
            let accounts_restored = AccountList::<T>::from(slice).unwrap();
            assert_eq!(accounts_restored, accounts);
        }

        test_list::<Validator>();
        test_list::<Maintainer>();

        let lido = Lido {
            lido_version: 0,
            account_type: AccountType::Lido,
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
            reward_distribution: RewardDistribution {
                treasury_fee: 2,
                developer_fee: 4,
                st_sol_appreciation: 7,
            },
            fee_recipients: FeeRecipients {
                treasury_account: Pubkey::new_unique(),
                developer_account: Pubkey::new_unique(),
            },
            metrics: Metrics::new(),
            validator_list: Pubkey::new_unique(),
            maintainer_list: Pubkey::new_unique(),
            max_commission_percentage: 5,
        };
        let mut data = Vec::new();
        BorshSerialize::serialize(&lido, &mut data).unwrap();

        let restored = try_from_slice_unchecked(&data[..]).unwrap();
        assert_eq!(lido, restored);
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
        let mut validators = ValidatorList::new_default(0);
        let key = Pubkey::default();
        let mut amount = rent.minimum_balance(0);
        let mut reserve_account =
            AccountInfo::new(&key, true, true, &mut amount, &mut [], &key, false, 0);

        assert_eq!(
            Lido::get_sol_balance(validators.iter(), &rent, &reserve_account),
            Ok(Lamports(0))
        );

        let mut new_amount = rent.minimum_balance(0) + 10;
        reserve_account.lamports = Rc::new(RefCell::new(&mut new_amount));

        assert_eq!(
            Lido::get_sol_balance(validators.iter(), &rent, &reserve_account),
            Ok(Lamports(10))
        );

        validators.header.max_entries = 1;
        validators
            .entries
            .push(Validator::new(Pubkey::new_unique()));
        validators.entries[0].stake_accounts_balance = Lamports(37);
        assert_eq!(
            Lido::get_sol_balance(validators.iter(), &rent, &reserve_account),
            Ok(Lamports(10 + 37))
        );

        validators.entries[0].stake_accounts_balance = Lamports(u64::MAX);

        assert_eq!(
            Lido::get_sol_balance(validators.iter(), &rent, &reserve_account),
            Err(LidoError::CalculationFailure)
        );

        let mut new_amount = u64::MAX;
        reserve_account.lamports = Rc::new(RefCell::new(&mut new_amount));
        // The amount here is more than the rent exemption that gets discounted
        // from the reserve, causing an overflow.
        validators.entries[0].stake_accounts_balance = Lamports(5_000_000);

        assert_eq!(
            Lido::get_sol_balance(validators.iter(), &rent, &reserve_account),
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
            developer_fee: 1,
            st_sol_appreciation: 0,
        };

        assert_eq!(
            // In this case the amount can be split exactly,
            // there is no remainder.
            spec.split_reward(Lamports(600)).unwrap(),
            Fees {
                treasury_amount: Lamports(450),
                developer_amount: Lamports(150),
                st_sol_appreciation_amount: Lamports(0),
            },
        );

        assert_eq!(
            // In this case the amount cannot be split exactly, all fees are
            // rounded down.
            spec.split_reward(Lamports(1_003)).unwrap(),
            Fees {
                treasury_amount: Lamports(752),
                developer_amount: Lamports(250),
                st_sol_appreciation_amount: Lamports(1),
            },
        );

        // If we use 3%, 2%, 1% fee, and the remaining 94% go to stSOL appreciation,
        // we should see 3%, 2%, and 1% fee.
        spec.st_sol_appreciation = 96;
        assert_eq!(
            spec.split_reward(Lamports(100)).unwrap(),
            Fees {
                treasury_amount: Lamports(3),
                developer_amount: Lamports(1),
                st_sol_appreciation_amount: Lamports(96),
            },
        );

        let spec_coprime = RewardDistribution {
            treasury_fee: 17,
            developer_fee: 19,
            st_sol_appreciation: 0,
        };
        assert_eq!(
            spec_coprime.split_reward(Lamports(1_000)).unwrap(),
            Fees {
                treasury_amount: Lamports(472),
                developer_amount: Lamports(527),
                st_sol_appreciation_amount: Lamports(1),
            },
        );
    }
    #[test]
    fn test_n_val() {
        let n_validators: u64 = 10_000;
        let size =
            get_instance_packed_len(&ValidatorList::new_default(n_validators as u32)).unwrap();

        assert_eq!(
            ValidatorList::calculate_max_entries(size) as u64,
            n_validators
        );
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

            assert_eq!(res[1], i);

            let lido_recovered = try_from_slice_unchecked(&res[..]).unwrap();
            assert_eq!(lido, lido_recovered);
        }
    }

    #[test]
    fn test_check_is_st_sol_account_fails_with_different_owner() {
        let lido = Lido::default();
        let key = Pubkey::new_unique();
        let mut lamports = 0;
        let mut data = [];
        let owner = Pubkey::new_unique();
        let token_account = &AccountInfo::new(
            &key,
            false,
            true,
            &mut lamports,
            &mut data,
            &owner,
            false,
            0,
        );
        let result = lido.check_is_st_sol_account(token_account);
        match result {
            Err(ProgramError::Custom(err_code)) => {
                assert_eq!(err_code, LidoError::InvalidStSolAccountOwner as u32)
            }
            _ => panic!("Should be the InvalidStSolAccountOwner error"),
        }
        assert!(result.is_err());
    }

    #[test]
    fn check_lido_version() {
        // create empty account list with Vec
        let mut accounts = ValidatorList::new_default(1);
        accounts.header.lido_version = 0;

        // allocate space for future elements
        let mut buffer: Vec<u8> =
            vec![0; ValidatorList::required_bytes(accounts.header.max_entries)];
        let mut slice = &mut buffer[..];
        // seriaslize empty list to buffer, which serializes a header and lenght
        BorshSerialize::serialize(&accounts, &mut slice).unwrap();

        // deserialize to BigVec
        let slice = &mut buffer[..];
        let err = ListHeader::<Validator>::deserialize_vec(slice).unwrap_err();
        assert_eq!(err, LidoError::LidoVersionMismatch.into());
    }

    #[test]
    fn check_account_type() {
        // create empty validator list with Vec
        let accounts = ValidatorList::new_default(1);

        // allocate space for future elements
        let mut buffer: Vec<u8> =
            vec![0; ValidatorList::required_bytes(accounts.header.max_entries)];
        let mut slice = &mut buffer[..];
        // seriaslize empty list to buffer, which serializes a header and lenght
        BorshSerialize::serialize(&accounts, &mut slice).unwrap();

        // deserialize to BigVec but with a different account type
        let slice = &mut buffer[..];
        let err = ListHeader::<Maintainer>::deserialize_vec(slice).unwrap_err();
        assert_eq!(err, LidoError::InvalidAccountType.into());
    }

    #[test]
    fn check_deserialize_with_borsh() {
        // create empty validator list with Vec
        let mut accounts = ValidatorList::new_default(1);
        accounts.header.max_entries = 2;

        let mut elem = &mut accounts.entries[0];
        elem.vote_account_address = Pubkey::new_unique();
        elem.effective_stake_balance = Lamports(34453);
        elem.stake_accounts_balance = Lamports(234525);
        elem.active = true;

        // allocate space for future elements
        let mut buffer: Vec<u8> =
            vec![0; ValidatorList::required_bytes(accounts.header.max_entries)];
        let mut slice = &mut buffer[..];
        BorshSerialize::serialize(&accounts, &mut slice).unwrap();

        let slice = &mut buffer[..];
        let (big_vec, header) = ListHeader::<Validator>::deserialize_vec(slice).unwrap();
        let mut bigvec = BigVecWithHeader::new(big_vec, header);

        let elem = Validator {
            vote_account_address: Pubkey::new_unique(),
            stake_seeds: SeedRange {
                begin: 123,
                end: 5455,
            },
            unstake_seeds: SeedRange {
                begin: 555,
                end: 9886,
            },
            stake_accounts_balance: Lamports(1111),
            unstake_accounts_balance: Lamports(3333),
            effective_stake_balance: Lamports(3465468),
            active: false,
        };

        accounts.entries.push(elem.clone());

        bigvec.push(elem).unwrap();

        let mut slice = &buffer[..];
        let accounts2 = BorshDeserialize::deserialize(&mut slice).unwrap();

        // test that BigVec does not break borsh deserialization
        assert_eq!(accounts, accounts2);
    }
}
