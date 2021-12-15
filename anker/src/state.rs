// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use crate::instruction::{SellRewardsAccountsInfo, SendRewardsAccountsInfo};
use crate::metrics::Metrics;
use crate::wormhole::{check_wormhole_account, TerraAddress, WormholeTransferArgs};
use crate::{
    error::AnkerError, ANKER_MINT_AUTHORITY, ANKER_RESERVE_AUTHORITY, ANKER_STSOL_RESERVE_ACCOUNT,
    ANKER_UST_RESERVE_ACCOUNT,
};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::state::Lido;
use lido::token::{Rational, StLamports};
use lido::util::serialize_b58;
use serde::Serialize;
use solana_program::program_error::ProgramError;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_pack::Pack, pubkey::Pubkey,
};
use spl_token_swap::state::SwapV1;

use crate::token::{self, BLamports};

/// Size of the serialized [`Anker`] struct, in bytes.
pub const ANKER_LEN: usize = 233;

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

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Anker {
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

    /// Metrics for informational purposes.
    pub metrics: Metrics,

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
    ) -> Result<spl_token_swap::state::SwapV1, ProgramError> {
        self.check_token_swap_pool(token_swap_account)?;
        // We do not check the owner of the `token_swap_account`. Since we store
        // this address in Anker's state, and we also trust the manager that changes
        // this address, we don't verify the account's owner. This also allows us to
        // test different token swap programs ids on different clusters.
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

    /// Check the if the token swap program is the same as the one stored in the
    /// instance.
    ///
    /// Check all the token swap associated accounts.
    /// Check if the rewards destination is the same as the one stored in Anker.
    pub fn check_token_swap(
        &self,
        anker_program_id: &Pubkey,
        accounts: &SellRewardsAccountsInfo,
    ) -> ProgramResult {
        // Check if the token swap account is the same one as the stored in the instance.
        let token_swap = self.get_token_swap_instance(accounts.token_swap_pool)?;

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
            *accounts.message.key,
        );

        check_wormhole_account(
            "config key",
            &wormhole_transfer_args.config_key,
            accounts.config_key.key,
        )?;
        check_wormhole_account(
            "custody key",
            &wormhole_transfer_args.custody_key,
            accounts.custody_key.key,
        )?;
        check_wormhole_account(
            "authority signer key",
            &wormhole_transfer_args.authority_signer_key,
            accounts.authority_signer_key.key,
        )?;
        check_wormhole_account(
            "custody signer key",
            &wormhole_transfer_args.custody_signer_key,
            accounts.custody_signer_key.key,
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
        ExchangeRate {
            st_sol_amount: solido.exchange_rate.st_sol_supply,
            // By definition here, we set 1 bSOL equal to 1 SOL.
            b_sol_amount: BLamports(solido.exchange_rate.sol_balance.0),
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
}
