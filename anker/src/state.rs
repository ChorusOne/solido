// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use crate::instruction::{
    FetchPoolPriceAccountsInfo, SellRewardsAccountsInfo, SendRewardsAccountsInfo,
};
use crate::metrics::Metrics;
use crate::wormhole::{check_wormhole_account, TerraAddress, WormholeTransferArgs};
use crate::{
    error::AnkerError, ANKER_MINT_AUTHORITY, ANKER_RESERVE_AUTHORITY, ANKER_STSOL_RESERVE_ACCOUNT,
    ANKER_UST_RESERVE_ACCOUNT,
};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::state::Lido;
use lido::token::{ArithmeticError, Lamports, Rational, StLamports};
use lido::util::serialize_b58;
use serde::Serialize;
use solana_program::program_error::ProgramError;
use solana_program::{
    account_info::AccountInfo, clock::Slot, entrypoint::ProgramResult, msg, program_pack::Pack,
    pubkey::Pubkey,
};
use spl_token_swap::state::SwapV1;

use crate::token::{self, BLamports, MicroUst};

/// Size of the serialized [`Anker`] struct, in bytes.
pub const ANKER_LEN: usize = 322;
pub const ANKER_VERSION: u8 = 0;

// Next are three constants related to stored stSOL/UST prices. Because Anker is
// permissionless, everybody can call `SellRewards` if there are rewards to sell.
// This means that the caller could sandwich the `SellRewards` between two
// instructions that swap against the same stSOL/UST pool that Anker uses, to
// give us a bad price, and take the difference. To mitigate this risk, we set a
// `min_out` on the swap instruction, but in order to do so, we need a "fair"
// price. For that, we sample 5 past prices, at least some number of slots apart
// (enough that they are produced by different leaders), but also not too old,
// to make sure the price is still fresh. Then we take the median of that as a
// "fair" price and set `min_out` based on that. Now if anybody is trying to
// sandwich us, they would also have to sandwich 3 of those 5 times where we sample
// the price (and they pay swap fees), and they are competing with our honest
// maintenance bot for that (and possibly with others). Also, having a recent
// price ensures that we don't sell rewards at times of extreme volatility.

/// The number of historical stSOL/UST exchange rates we store.
pub const POOL_PRICE_NUM_SAMPLES: usize = 5;

/// The minimum number of slots that must elapse after the most recent stSOL/UST price sample,
/// before we can store a new sample.
pub const POOL_PRICE_MIN_SAMPLE_DISTANCE: Slot = 100;

/// The maximum age of the oldest stSOL/UST price sample where we still allow `SellRewards`.
///
/// This value should be larger than `POOL_PRICE_NUM_SAMPLES * POOL_PRICE_MIN_SAMPLE_DISTANCE`.
///
/// At ~550 ms per slot, 1000 slots is roughly 9 minutes.
pub const POOL_PRICE_MAX_SAMPLE_AGE: Slot = 1000;

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct WormholeParameters {
    /// The Wormhole program associated with this instance.
    pub core_bridge_program_id: Pubkey,
    /// The Wormhole program for token transfers associated with this instance.
    pub token_bridge_program_id: Pubkey,
}

/// The price of 1 stSOL expressed in UST, as observed from the pool in a particular slot.
#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    BorshDeserialize,
    BorshSerialize,
    BorshSchema,
    Eq,
    PartialEq,
    Serialize,
)]
pub struct HistoricalStSolPrice {
    /// The slot in which this price was observed.
    pub slot: Slot,

    /// The price of 1 stSOL (1e9 stLamports).
    pub st_sol_price_in_ust: MicroUst,
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    BorshDeserialize,
    BorshSerialize,
    BorshSchema,
    Eq,
    PartialEq,
    Serialize,
)]
pub struct HistoricalStSolPriceArray(pub [HistoricalStSolPrice; POOL_PRICE_NUM_SAMPLES]);

impl HistoricalStSolPriceArray {
    /// Create new `HistorialStSolPriceArray` with slot 0 and 1 UST in each
    /// position of the array.
    pub fn new() -> Self {
        HistoricalStSolPriceArray(
            [HistoricalStSolPrice {
                slot: 0,
                st_sol_price_in_ust: MicroUst(1_000_000),
            }; 5],
        )
    }

    /// Get last price from the array.
    pub fn last(&self) -> HistoricalStSolPrice {
        self.0[POOL_PRICE_NUM_SAMPLES - 1]
    }

    /// Insert `st_sol_price_in_ust` at the end of the array and rotate it.
    pub fn insert_and_rotate(&mut self, slot: Slot, st_sol_price_in_ust: MicroUst) {
        // Maintain the invariant that samples are sorted by ascending slot number.
        // The sample at index 0 is the oldest, so we remove it (well, move it to the
        // end to be overwritten), and move everything else closer to the beginning
        // of the array. Then we overwrite the last element with the current price
        // and slot number, and we confirmed above that that slot number is larger
        // than the slot number of the sample before it.
        self.0.rotate_left(1);
        self.0[POOL_PRICE_NUM_SAMPLES - 1].slot = slot;
        self.0[POOL_PRICE_NUM_SAMPLES - 1].st_sol_price_in_ust = st_sol_price_in_ust;
        assert!(self.0[POOL_PRICE_NUM_SAMPLES - 1].slot >= self.0[POOL_PRICE_NUM_SAMPLES - 2].slot);
    }

    /// Calculate the minimum amount we are willing to pay for the `StLamports`
    /// rewards based on the median price from the historical price information.
    pub fn calculate_minimum_price(
        &self,
        rewards: StLamports,
        sell_rewards_min_out_bps: u64,
    ) -> Result<MicroUst, ArithmeticError> {
        let mut sorted_arr = self.0;
        sorted_arr.sort_by_key(|x| x.st_sol_price_in_ust);
        // Get median historical price.
        let median_price = sorted_arr[POOL_PRICE_NUM_SAMPLES / 2];
        let minimum_ust_per_st_sol = (median_price.st_sol_price_in_ust
            * Rational {
                numerator: sell_rewards_min_out_bps,
                denominator: 10_000,
            })?;
        let minimum_price = (rewards
            * Rational {
                numerator: minimum_ust_per_st_sol.0,
                denominator: 1_000_000_000,
            })?;
        Ok(MicroUst(minimum_price.0))
    }
}

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Anker {
    /// Version number for Anker.
    pub version: u8,

    /// The Solido program that owns the `solido` instance.
    #[serde(serialize_with = "serialize_b58")]
    pub solido_program_id: Pubkey,

    /// The associated Solido instance address.
    #[serde(serialize_with = "serialize_b58")]
    pub solido: Pubkey,

    /// The SPL Token mint address for bSOL.
    #[serde(serialize_with = "serialize_b58")]
    pub b_sol_mint: Pubkey,

    /// Token swap data. Used to swap stSOL for UST.
    #[serde(serialize_with = "serialize_b58")]
    pub token_swap_pool: Pubkey,

    /// Destination of the rewards on Terra, paid in UST.
    pub terra_rewards_destination: TerraAddress,

    /// Wormhole parameters associated with this instance.
    pub wormhole_parameters: WormholeParameters,

    /// When we sell rewards, we set the minimum out to stSOL amount times the
    /// median of the recent price samples times a factor alpha. In other words,
    /// this factor alpha is `1 - max_slippage`. Alpha is defined as
    /// `sell_rewards_min_out_bps / 1e4`. The `bps` here means "basis points".
    /// A basis point is 0.01% = 1e-4.)
    pub sell_rewards_min_out_bps: u64,

    /// Metrics for informational purposes.
    pub metrics: Metrics,

    /// Historical stSOL prices, used to prevent sandwiching when we sell rewards.
    ///
    /// Invariant: entries are sorted by ascending slot number (so the oldest
    /// entry is at index 0).
    pub historical_st_sol_prices: HistoricalStSolPriceArray,

    /// Bump seed for the derived address that this Anker instance should live at.
    pub self_bump_seed: u8,

    /// Bump seed for the mint authority derived address.
    pub mint_authority_bump_seed: u8,

    /// Bump seed for the reserve authority (owner of the reserve account) derived address.
    pub reserve_authority_bump_seed: u8,

    /// Bump seed for the reserve account (SPL token account that holds stSOL).
    pub st_sol_reserve_account_bump_seed: u8,

    /// Bump seed for the UST reserve account.
    pub ust_reserve_account_bump_seed: u8,
}

impl Anker {
    pub fn save(&self, account: &AccountInfo) -> ProgramResult {
        // NOTE: If you ended up here because the tests are failing because the
        // runtime complained that an account's size was modified by a program
        // that wasn't its owner, double check that the name passed to
        // ProgramTest matches the name of the crate.
        BorshSerialize::serialize(self, &mut *account.data.borrow_mut())?;
        Ok(())
    }

    /// Confirm that the account address is the derived address where the Anker instance should live.
    pub fn check_self_address(
        &self,
        anker_program_id: &Pubkey,
        account_info: &AccountInfo,
    ) -> ProgramResult {
        let address = Pubkey::create_program_address(
            &[self.solido.as_ref(), &[self.self_bump_seed]],
            anker_program_id,
        )
        .expect("Depends only on Anker-controlled values, should not fail.");

        if *account_info.key != address {
            msg!(
                "Expected Anker instance for Solido instance {} to be {}, but found {} instead.",
                self.solido,
                address,
                account_info.key,
            );
            return Err(AnkerError::InvalidDerivedAccount.into());
        }
        Ok(())
    }

    /// Confirm that the derived account address matches the `account_info` adddress.
    fn check_derived_account_address(
        &self,
        name: &'static str,
        seed: &'static [u8],
        bump_seed: u8,
        anker_program_id: &Pubkey,
        anker_instance: &Pubkey,
        account_info: &AccountInfo,
    ) -> ProgramResult {
        let address = Pubkey::create_program_address(
            &[anker_instance.as_ref(), seed, &[bump_seed]],
            anker_program_id,
        )
        .expect("Depends only on Anker-controlled values, should not fail.");

        if *account_info.key != address {
            msg!(
                "Expected {} to be {}, but found {} instead.",
                name,
                address,
                account_info.key,
            );
            return Err(AnkerError::InvalidDerivedAccount.into());
        }
        Ok(())
    }

    /// Confirm that the provided stSOL reserve accounts is the one that
    /// belongs to this instance.
    ///
    /// This does not check that the stSOL reserve is an stSOL account.
    pub fn check_st_sol_reserve_address(
        &self,
        anker_program_id: &Pubkey,
        anker_instance: &Pubkey,
        st_sol_reserve_account_info: &AccountInfo,
    ) -> ProgramResult {
        self.check_derived_account_address(
            "the stSOL reserve account",
            ANKER_STSOL_RESERVE_ACCOUNT,
            self.st_sol_reserve_account_bump_seed,
            anker_program_id,
            anker_instance,
            st_sol_reserve_account_info,
        )
    }

    /// Confirm that the provided UST reserve accounts is the one that
    /// belongs to this instance.
    ///
    /// This does not check that the UST reserve is an UST account.
    pub fn check_ust_reserve_address(
        &self,
        anker_program_id: &Pubkey,
        anker_instance: &Pubkey,
        ust_reserve_account_info: &AccountInfo,
    ) -> ProgramResult {
        self.check_derived_account_address(
            "the UST reserve account",
            ANKER_UST_RESERVE_ACCOUNT,
            self.ust_reserve_account_bump_seed,
            anker_program_id,
            anker_instance,
            ust_reserve_account_info,
        )
    }

    /// Confirm that the provided reserve authority is the one that belongs to this instance.
    pub fn check_reserve_authority(
        &self,
        anker_program_id: &Pubkey,
        anker_instance: &Pubkey,
        reserve_authority_info: &AccountInfo,
    ) -> ProgramResult {
        self.check_derived_account_address(
            "the reserve authority",
            ANKER_RESERVE_AUTHORITY,
            self.reserve_authority_bump_seed,
            anker_program_id,
            anker_instance,
            reserve_authority_info,
        )
    }

    /// Confirm that the provided bSOL mint authority is the one that belongs to this instance.
    pub fn check_mint_authority(
        &self,
        anker_program_id: &Pubkey,
        anker_instance: &Pubkey,
        mint_authority_info: &AccountInfo,
    ) -> ProgramResult {
        self.check_derived_account_address(
            "the bSOL mint authority",
            ANKER_MINT_AUTHORITY,
            self.mint_authority_bump_seed,
            anker_program_id,
            anker_instance,
            mint_authority_info,
        )
    }

    /// Confirm that the provided mint account is the one stored in this instance.
    pub fn check_mint(&self, provided_mint: &AccountInfo) -> ProgramResult {
        if *provided_mint.owner != spl_token::id() {
            msg!(
                "Expected bSOL mint to be owned by the SPL token program ({}), but found {}.",
                spl_token::id(),
                provided_mint.owner,
            );
            return Err(AnkerError::InvalidTokenMint.into());
        }

        if self.b_sol_mint != *provided_mint.key {
            msg!(
                "Invalid mint account, expected {}, but found {}.",
                self.b_sol_mint,
                provided_mint.key,
            );
            return Err(AnkerError::InvalidTokenMint.into());
        }
        Ok(())
    }

    fn check_is_spl_token_account(
        mint_name: &'static str,
        mint_address: &Pubkey,
        token_account_info: &AccountInfo,
    ) -> ProgramResult {
        if token_account_info.owner != &spl_token::id() {
            msg!(
                "Expected SPL token account to be owned by {}, but it's owned by {} instead.",
                spl_token::id(),
                token_account_info.owner
            );
            return Err(AnkerError::InvalidTokenAccountOwner.into());
        }

        let token_account =
            match spl_token::state::Account::unpack_from_slice(&token_account_info.data.borrow()) {
                Ok(account) => account,
                Err(..) => {
                    msg!(
                        "Expected an SPL token account at {}.",
                        token_account_info.key
                    );
                    return Err(AnkerError::InvalidTokenAccount.into());
                }
            };

        if token_account.mint != *mint_address {
            msg!(
                "Expected mint of {} to be {} mint ({}), but found {}.",
                token_account_info.key,
                mint_name,
                mint_address,
                token_account.mint,
            );
            return Err(AnkerError::InvalidTokenMint.into());
        }

        Ok(())
    }

    /// Confirm that the account is an SPL token account that holds bSOL.
    pub fn check_is_b_sol_account(&self, token_account_info: &AccountInfo) -> ProgramResult {
        Anker::check_is_spl_token_account("our bSOL", &self.b_sol_mint, token_account_info)
    }

    /// Confirm that the account is an SPL token account that holds stSOL.
    pub fn check_is_st_sol_account(
        &self,
        solido: &Lido,
        token_account_info: &AccountInfo,
    ) -> ProgramResult {
        Anker::check_is_spl_token_account("Solido's stSOL", &solido.st_sol_mint, token_account_info)
    }

    /// Get an instance of the Token Swap V1 from the provided account info.
    pub fn get_token_swap_instance(
        &self,
        token_swap_account: &AccountInfo,
        token_swap_program_id: &Pubkey,
    ) -> Result<spl_token_swap::state::SwapV1, ProgramError> {
        self.check_token_swap_pool(token_swap_account)?;

        // We do not check the owner of the `token_swap_account`. Since we store
        // this address in Anker's state, and we also trust the manager that changes
        // this address, we don't verify the account's owner. This also allows us to
        // test different token swap programs ids on different clusters.
        // However, we *should* check that the program we are going to call later to
        // do the token swap, is actually the intended token swap program.
        if token_swap_account.owner != token_swap_program_id {
            msg!(
                "Encountered wrong token swap program; expected {} but found {}.",
                token_swap_account.owner,
                token_swap_program_id,
            );
            return Err(AnkerError::WrongSplTokenSwap.into());
        }

        // Check that version byte corresponds to V1 version byte.
        if token_swap_account.data.borrow().len() != spl_token_swap::state::SwapVersion::LATEST_LEN
        {
            msg!(
                "Length of the Token Swap is invalid, expected {}, found {}",
                spl_token_swap::state::SwapVersion::LATEST_LEN,
                token_swap_account.data.borrow().len()
            );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }
        if token_swap_account.data.borrow()[0] != 1u8 {
            msg!(
            "Token Swap instance version is different from what we expect, expected 1, found {}",
            token_swap_account.data.borrow()[0]
        );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }
        // We should ignore the version 1st byte for the unpack.
        spl_token_swap::state::SwapV1::unpack(&token_swap_account.data.borrow()[1..])
    }

    /// Check if we can change the token swap account.
    pub fn check_change_token_swap_pool(
        &self,
        solido: &Lido,
        current_token_swap: SwapV1,
        new_token_swap: SwapV1,
    ) -> ProgramResult {
        // We don't check that the old pool's owner is the same as the new
        // pool's owner. It's the manager's responsibility to replace the token
        // pool swap with a valid one. This also allows us to change the pool
        // program, if necessary.
        // Check if the token swap account is the same one as the stored in the instance.

        // Get stSOL and UST mint. We trust that the UST mint stored in the current instance is right.
        let (st_sol_mint, ust_mint) = if current_token_swap.token_a_mint == solido.st_sol_mint {
            (
                current_token_swap.token_a_mint,
                current_token_swap.token_b_mint,
            )
        } else {
            (
                current_token_swap.token_b_mint,
                current_token_swap.token_a_mint,
            )
        };

        // Get the stSOL and UST pool token, and verify that the minters are right.
        if new_token_swap.token_a_mint == st_sol_mint {
            // token_a_mint is stSOL mint.
            if new_token_swap.token_b_mint != ust_mint {
                // token_b_mint should be ust_mint.
                msg!(
                    "token_b_mint is expected to be the UST mint ({}), but is {}",
                    ust_mint,
                    new_token_swap.token_b_mint
                );
                return Err(AnkerError::WrongSplTokenSwapParameters.into());
            }
        } else if new_token_swap.token_a_mint == ust_mint {
            // token_a is UST.
            if new_token_swap.token_b_mint != st_sol_mint {
                // token_b_mint should be ust_mint.
                msg!(
                    "token_b_mint is expected to be the stSOL mint ({}), but is {}",
                    st_sol_mint,
                    new_token_swap.token_b_mint
                );
                return Err(AnkerError::WrongSplTokenSwapParameters.into());
            }
        } else {
            // token_a_mint is wrong.
            msg!(
                "token_a_mint is expected to be either stSOL mint ({}), or UST mint ({}) but is {}",
                st_sol_mint,
                ust_mint,
                new_token_swap.token_a_mint
            );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        };

        Ok(())
    }

    fn check_token_swap_pool(&self, token_swap_account: &AccountInfo) -> ProgramResult {
        if &self.token_swap_pool != token_swap_account.key {
            msg!(
                "Invalid Token Swap instance, expected {}, found {}",
                self.token_swap_pool,
                token_swap_account.key
            );
            return Err(AnkerError::WrongSplTokenSwap.into());
        }
        Ok(())
    }

    /// Confirm that the passed accounts match those stored in the pool.
    pub fn check_token_swap_before_fetch_price(
        &self,
        accounts: &FetchPoolPriceAccountsInfo,
    ) -> ProgramResult {
        // Check if the token swap account is the same one as the stored in the instance.
        let token_swap_program_id = accounts.token_swap_pool.owner;
        let token_swap =
            self.get_token_swap_instance(accounts.token_swap_pool, token_swap_program_id)?;

        // Check that the pool still has token
        let (pool_st_sol_account, pool_ust_account) = if &token_swap.token_a
            == accounts.pool_st_sol_account.key
        {
            Ok((token_swap.token_a, token_swap.token_b))
        } else if &token_swap.token_a == accounts.pool_ust_account.key {
            Ok((token_swap.token_b, token_swap.token_a))
        } else {
            msg!(
                    "Could not find a match for token swap account {}, candidates were the stSol account {} or UST account {}",
                    token_swap.token_a,
                    accounts.pool_st_sol_account.key,
                    accounts.pool_ust_account.key
                );
            Err(AnkerError::WrongSplTokenSwapParameters)
        }?;

        if &pool_st_sol_account != accounts.pool_st_sol_account.key {
            msg!(
                "Token swap stSol token is different from what is stored in the instance, expected {}, found {}",
                pool_st_sol_account,
                accounts.pool_st_sol_account.key
            );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }
        if &pool_ust_account != accounts.pool_ust_account.key {
            msg!(
                "Token swap UST token is different from what is stored in the instance, expected {}, found {}",
                pool_ust_account,
                accounts.pool_ust_account.key
            );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }

        Ok(())
    }

    /// Check the if the token swap program is the same as the one stored in the
    /// instance.
    ///
    /// Check all the token swap associated accounts.
    /// Check if the rewards destination is the same as the one stored in Anker.
    pub fn check_token_swap_before_sell(
        &self,
        anker_program_id: &Pubkey,
        accounts: &SellRewardsAccountsInfo,
    ) -> ProgramResult {
        // Check if the token swap account is the same one as the stored in the instance.
        let token_swap = self.get_token_swap_instance(
            accounts.token_swap_pool,
            accounts.token_swap_program_id.key,
        )?;

        // Check token swap instance parameters.
        // Check UST token accounts.
        self.check_ust_reserve_address(
            anker_program_id,
            accounts.anker.key,
            accounts.ust_reserve_account,
        )?;

        // Pool stSOL and UST token could be swapped.
        let (pool_st_sol_account, pool_st_sol_mint, pool_ust_account, pool_ust_mint) =
            if &token_swap.token_a == accounts.pool_st_sol_account.key {
                Ok((
                    token_swap.token_a,
                    token_swap.token_a_mint,
                    token_swap.token_b,
                    token_swap.token_b_mint,
                ))
            } else if &token_swap.token_a == accounts.pool_ust_account.key {
                Ok((
                    token_swap.token_b,
                    token_swap.token_b_mint,
                    token_swap.token_a,
                    token_swap.token_a_mint,
                ))
            } else {
                msg!(
                    "Could not find a match for token swap account {}, candidates were the StSol account {} or UST account {}",
                    token_swap.token_a,
                    accounts.pool_st_sol_account.key,
                    accounts.pool_ust_account.key
                );
                Err(AnkerError::WrongSplTokenSwapParameters)
            }?;

        // Check stSOL token.
        if &pool_st_sol_account != accounts.pool_st_sol_account.key {
            msg!(
            "Token Swap StSol token is different from what is stored in the instance, expected {}, found {}",
            pool_st_sol_account,
            accounts.pool_st_sol_account.key
        );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }
        // Check UST token.
        if &pool_ust_account != accounts.pool_ust_account.key {
            msg!(
            "Token Swap UST token is different from what is stored in the instance, expected {}, found {}",
            pool_ust_account,
            accounts.pool_ust_account.key
        );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }
        // Check pool mint.
        if &token_swap.pool_mint != accounts.pool_mint.key {
            msg!(
            "Token Swap mint is different from what is stored in the instance, expected {}, found {}",
            token_swap.pool_mint,
            accounts.pool_mint.key
        );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }

        // Check stSOL mint.
        if &pool_st_sol_mint != accounts.st_sol_mint.key {
            msg!(
            "Token Swap StSol mint is different from what is stored in the instance, expected {}, found {}",
            pool_st_sol_mint,
            accounts.st_sol_mint.key
        );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }
        // Check UST mint.
        if &pool_ust_mint != accounts.ust_mint.key {
            msg!(
            "Token Swap UST mint is different from what is stored in the instance, expected {}, found {}",
            pool_ust_mint,
            accounts.ust_mint.key
        );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }
        // Check pool fee.
        if &token_swap.pool_fee_account != accounts.pool_fee_account.key {
            msg!(
            "Token Swap fee account is different from what is stored in the instance, expected {}, found {}",
            token_swap.pool_fee_account,
            accounts.pool_fee_account.key
        );
            return Err(AnkerError::WrongSplTokenSwapParameters.into());
        }

        Ok(())
    }

    pub fn check_send_rewards(
        &self,
        accounts: &SendRewardsAccountsInfo,
    ) -> Result<Box<WormholeTransferArgs>, ProgramError> {
        check_wormhole_account(
            "token bridge program",
            &self.wormhole_parameters.token_bridge_program_id,
            accounts.wormhole_token_bridge_program_id.key,
        )?;
        check_wormhole_account(
            "core bridge program",
            &self.wormhole_parameters.core_bridge_program_id,
            accounts.wormhole_core_bridge_program_id.key,
        )?;

        let wormhole_transfer_args = WormholeTransferArgs::new(
            self.wormhole_parameters.token_bridge_program_id,
            self.wormhole_parameters.core_bridge_program_id,
            *accounts.ust_mint.key,
            *accounts.payer.key,
            *accounts.ust_reserve_account.key,
            *accounts.reserve_authority.key,
            *accounts.message.key,
        );

        check_wormhole_account(
            "config key",
            &wormhole_transfer_args.config_key,
            accounts.config_key.key,
        )?;
        check_wormhole_account(
            "wrapped meta key",
            &wormhole_transfer_args.wrapped_meta_key,
            accounts.wrapped_meta_key.key,
        )?;
        check_wormhole_account(
            "authority signer key",
            &wormhole_transfer_args.authority_signer_key,
            accounts.authority_signer_key.key,
        )?;
        check_wormhole_account(
            "bridge config",
            &wormhole_transfer_args.bridge_config,
            accounts.bridge_config.key,
        )?;
        check_wormhole_account(
            "emitter key",
            &wormhole_transfer_args.emitter_key,
            accounts.emitter_key.key,
        )?;
        check_wormhole_account(
            "sequence key",
            &wormhole_transfer_args.sequence_key,
            accounts.sequence_key.key,
        )?;
        check_wormhole_account(
            "fee collector key",
            &wormhole_transfer_args.fee_collector_key,
            accounts.fee_collector_key.key,
        )?;
        Ok(Box::new(wormhole_transfer_args))
    }

    /// Get the `amount` of tokens from the SPL account defined by `account`.
    /// Does not perform any checks, fails if not able to decode an SPL account.
    pub fn get_token_amount(account: &AccountInfo) -> Result<u64, ProgramError> {
        if account.owner != &spl_token::id() {
            msg!(
                "Token accounts should be owned by {}, it's owned by {}",
                spl_token::id(),
                account.owner
            );
            return Err(AnkerError::InvalidOwner.into());
        }
        let account_state = spl_token::state::Account::unpack_from_slice(&account.data.borrow())?;
        Ok(account_state.amount)
    }
}

/// Exchange rate from bSOL to stSOL.
///
/// This can be computed in different ways, but
pub struct ExchangeRate {
    /// Amount of stSOL that is equal in value to `b_sol_amount`.
    pub st_sol_amount: StLamports,

    /// Amount of bSOL that is equal in value to `st_sol_amount`.
    pub b_sol_amount: BLamports,
}

impl ExchangeRate {
    /// Return the bSOL/stSOL rate that ensures that 1 bSOL = 1 SOL.
    pub fn from_solido_pegged(solido: &Lido) -> ExchangeRate {
        // On mainnet, the Solido instance exists for a while already, and its
        // stSOL supply and SOL balance are nonzero. But for local testing, in
        // the first epoch, the exchange rate stored in the Solido instance is
        // 0 stSOL = 0 SOL. To still enable Anker deposits during that first
        // epoch, we define the initial exchange rate to be 1 stSOL = 1 bSOL,
        // because Solido initially uses 1 SOL = 1 stSOL if the balance is zero.
        if solido.exchange_rate.st_sol_supply == StLamports(0)
            && solido.exchange_rate.sol_balance == Lamports(0)
        {
            ExchangeRate {
                st_sol_amount: StLamports(1),
                b_sol_amount: BLamports(1),
            }
        } else {
            ExchangeRate {
                st_sol_amount: solido.exchange_rate.st_sol_supply,
                // By definition here, we set 1 bSOL equal to 1 SOL.
                b_sol_amount: BLamports(solido.exchange_rate.sol_balance.0),
            }
        }
    }

    /// Return the bSOL/stSOL rate assuming 1 bSOL is a fraction 1/supply of the reserve.
    pub fn from_anker_unpegged(
        b_sol_supply: BLamports,
        reserve_balance: StLamports,
    ) -> ExchangeRate {
        ExchangeRate {
            st_sol_amount: reserve_balance,
            b_sol_amount: b_sol_supply,
        }
    }

    pub fn exchange_st_sol(&self, amount: StLamports) -> token::Result<BLamports> {
        // This swap is only used when depositing, so we should use the exchange
        // rate based on the Solido instance. It should have a non-zero amount
        // of assets under management, so the exchange rate is well-defined.
        assert!(self.b_sol_amount > BLamports(0));
        assert!(self.st_sol_amount > StLamports(0));

        let rate = Rational {
            numerator: self.b_sol_amount.0,
            denominator: self.st_sol_amount.0,
        };

        // The result is in StLamports, because the type system considers Rational
        // dimensionless, but in this case `rate` has dimensions bSOL/stSOL, so
        // we need to re-wrap the result in the right type.
        (amount * rate).map(|x| BLamports(x.0))
    }

    pub fn exchange_b_sol(&self, amount: BLamports) -> token::Result<StLamports> {
        // We can get the exchange rate either from Solido, or from the reserve + supply.
        // But in either case, neither of the values should be zero when we exchange bSOL
        // back to stSOL: in Solido neither the SOL balance nor the stSOL supply should
        // ever become zero, because we deposited some SOL in it that we do not plan to
        // ever withdraw. And for Anker, if you have bSOL to exchange, the only way in
        // which it could have been created is by locking some stSOL in Anker, so there
        // is a nonzero bSOL supply and nonzero reserve.
        assert!(self.b_sol_amount > BLamports(0));
        assert!(self.st_sol_amount > StLamports(0));

        let rate = Rational {
            numerator: self.st_sol_amount.0,
            denominator: self.b_sol_amount.0,
        };

        // The result is in BLamports, because the type system considers Rational
        // dimensionless, but in this case `rate` has dimensions stSOL/bSOL, so
        // we need to re-wrap the result in the right type.
        (amount * rate).map(|x| StLamports(x.0))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_anker_len() {
        let instance = Anker::default();
        let mut writer = Vec::new();
        BorshSerialize::serialize(&instance, &mut writer).unwrap();
        assert_eq!(writer.len(), ANKER_LEN);
    }

    #[test]
    fn test_version_serialise() {
        use solana_sdk::borsh::try_from_slice_unchecked;

        for i in 0..=255 {
            let anker = Anker {
                version: i,
                ..Anker::default()
            };
            let mut res: Vec<u8> = Vec::new();
            BorshSerialize::serialize(&anker, &mut res).unwrap();

            assert_eq!(res[0], i);

            let anker_recovered = try_from_slice_unchecked(&res[..]).unwrap();
            assert_eq!(anker, anker_recovered);
        }
    }

    #[test]
    fn test_historical_price_array_minimum() {
        let mut price_array = HistoricalStSolPriceArray::new();
        // 100 UST for each StSol.
        for slot in 0..POOL_PRICE_NUM_SAMPLES as u64 {
            price_array.insert_and_rotate(slot, MicroUst(100_000_000));
        }

        // 1 StSol rewards and 1% slippage.
        let minimum_ust = price_array
            .calculate_minimum_price(StLamports(1_000_000_000), 9900)
            .unwrap();
        assert_eq!(minimum_ust, MicroUst(99_000_000));

        // 1 StSol rewards and 2% slippage.
        let minimum_ust = price_array
            .calculate_minimum_price(StLamports(1_000_000_000), 9800)
            .unwrap();
        assert_eq!(minimum_ust, MicroUst(98_000_000));

        // 80 StSol rewards and 5% slippage
        let minimum_ust = price_array
            .calculate_minimum_price(StLamports(80_000_000_000), 9500)
            .unwrap();
        assert_eq!(minimum_ust, MicroUst(7_600_000_000));

        // 331 StSol rewards and 50% slippage
        let minimum_ust = price_array
            .calculate_minimum_price(StLamports(331_000_000_000), 5000)
            .unwrap();
        assert_eq!(minimum_ust, MicroUst(16_550_000_000));
    }

    #[test]
    fn test_different_prices() {
        let mut price_array = HistoricalStSolPriceArray::new();
        // Prices in USD per Sol [100, 90, 95, 105, 101], median: 100
        for (slot, price) in [100, 90, 95, 105, 101].iter().enumerate() {
            price_array.insert_and_rotate(slot as Slot, MicroUst(price * 1_000_000));
        }

        price_array.insert_and_rotate(4, MicroUst(80_000_000));
        // prices: [90, 95, 105, 101, 80], median: 95
        let minimum_ust = price_array
            .calculate_minimum_price(StLamports(331_000_000_000), 5000)
            .unwrap();
        assert_eq!(minimum_ust, MicroUst(15_722_500_000));

        price_array.insert_and_rotate(5, MicroUst(70_000_000));
        price_array.insert_and_rotate(6, MicroUst(85_000_000));
        // prices: [70, 80, 85, 101, 105], median: 85
        let minimum_ust = price_array
            .calculate_minimum_price(StLamports(100_000_000_000), 9800)
            .unwrap();
        assert_eq!(minimum_ust, MicroUst(8_330_000_000));
    }

    #[test]
    fn test_historical_price_array_limits() {
        let mut price_array = HistoricalStSolPriceArray::new();
        // 100 UST for each StSol.
        for slot in 0..POOL_PRICE_NUM_SAMPLES as u64 {
            price_array.insert_and_rotate(slot, MicroUst(100_000_000));
        }

        // 100 StLamports rewards and 1% slippage.
        let minimum_ust = price_array
            .calculate_minimum_price(StLamports(100), 9900)
            .unwrap();
        assert_eq!(minimum_ust, MicroUst(9));
    }
}
