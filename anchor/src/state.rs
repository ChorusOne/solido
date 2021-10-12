use crate::token::BLamports;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::{
    error::LidoError,
    token::{Rational, StLamports},
    util::serialize_b58,
};
use serde::Serialize;
use solana_program::{
    account_info::AccountInfo, clock::Epoch, entrypoint::ProgramResult, msg, pubkey::Pubkey,
};

// Copied from lido::state
#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct ExchangeRate {
    /// The epoch in which we last called `UpdateExchangeRate`.
    pub computed_in_epoch: Epoch,

    /// The amount of stSOL that existed at that time.
    pub b_sol_supply: BLamports,

    /// The amount of BSOL we managed at that time, according to our internal
    /// bookkeeping, so excluding the validation rewards paid at the start of
    /// epoch `computed_in_epoch`.
    pub st_sol_balance: StLamports,
}

impl ExchangeRate {
    /// Convert StLamports to BLamports.
    pub fn exchange_st_sol(&self, amount: StLamports) -> lido::token::Result<BLamports> {
        // The exchange rate starts out at 1:1, if there are no deposits yet.
        // If we minted stSOL but there is no SOL, then also assume a 1:1 rate.
        if self.b_sol_supply == BLamports(0) || self.st_sol_balance == StLamports(0) {
            return Ok(BLamports(amount.0));
        }

        let rate = Rational {
            numerator: self.b_sol_supply.0,
            denominator: self.st_sol_balance.0,
        };

        // The result is in StLamports, because the type system considers Rational
        // dimensionless, but in this case `rate` has dimensions stSOL/SOL, so
        // we need to re-wrap the result in the right type.
        (amount * rate).map(|x| BLamports(x.0))
    }

    /// Convert BLamports to StLamports.
    pub fn exchange_b_sol(&self, amount: BLamports) -> Result<StLamports, LidoError> {
        // If there is no stSOL in existence, it cannot be exchanged.
        if self.b_sol_supply == BLamports(0) {
            msg!("Cannot exchange bSOL for stSOL, because no bSOL has been minted.");
            return Err(LidoError::InvalidAmount);
        }

        let rate = Rational {
            numerator: self.st_sol_balance.0,
            denominator: self.b_sol_supply.0,
        };

        // The result is in BLamports, because the type system considers Rational
        // dimensionless, but in this case `rate` has dimensions SOL/stSOL, so
        // we need to re-wrap the result in the right type.
        Ok((amount * rate).map(|x| StLamports(x.0))?)
    }
}

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Anchor {
    /// The SPL Token mint address for bSOL.
    #[serde(serialize_with = "serialize_b58")]
    pub bsol_mint: Pubkey,

    /// Reserve account, will hold stSOL.
    #[serde(serialize_with = "serialize_b58")]
    pub reserve_account: Pubkey,

    /// Reserve authority for `reserve_account`.
    #[serde(serialize_with = "serialize_b58")]
    pub reserve_authority: Pubkey,

    /// The associated LIDO state address.
    #[serde(serialize_with = "serialize_b58")]
    pub lido: Pubkey,

    /// Exchange rate to use when depositing/withdrawing.
    pub exchange_rate: ExchangeRate,

    /// Bump seeds for signing messages on behalf of the authority.
    pub mint_authority_bump_seed: u8,
    pub reserve_authority_bump_seed: u8,
}

impl Anchor {
    pub fn save(&self, account: &AccountInfo) -> ProgramResult {
        // NOTE: If you ended up here because the tests are failing because the
        // runtime complained that an account's size was modified by a program
        // that wasn't its owner, double check that the name passed to
        // ProgramTest matches the name of the crate.
        BorshSerialize::serialize(self, &mut *account.data.borrow_mut())?;
        Ok(())
    }
}
