use crate::{error::AnkerError, ANKER_RESERVE_ACCOUNT, ANKER_RESERVE_AUTHORITY, ANKER_MINT_AUTHORITY};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::util::serialize_b58;
use lido::state::Lido;
use serde::Serialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_pack::Pack, pubkey::Pubkey,
};

/// Size of the serialized [`Anker`] struct, in bytes.
pub const ANKER_LEN: usize = 100;

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

    /// Bump seed for the derived address that this Anker instance should live at.
    pub self_bump_seed: u8,

    /// Bump seed for the mint authority derived address.
    pub mint_authority_bump_seed: u8,

    /// Bump seed for the reserve authority (owner of the reserve account) derived address.
    pub reserve_authority_bump_seed: u8,

    /// Bump seed for the reserve account (SPL token account that holds stSOL).
    pub reserve_account_bump_seed: u8,
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

    /// Confirm that the account address is the derived address where Anker instance should live.
    pub fn check_self_address(&self, anker_program_id: &Pubkey, account_info: &AccountInfo) -> ProgramResult {
        let address = Pubkey::create_program_address(
            &[
                self.solido.as_ref(),
                &[self.self_bump_seed],
            ],
            anker_program_id,
        ).expect("Depends only on Anker-controlled values, should not fail.");

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
    fn check_derived_account_address(&self, name: &'static str, seed: &'static [u8], bump_seed: u8, anker_program_id: &Pubkey, anker_instance: &Pubkey, account_info: &AccountInfo) -> ProgramResult {
        let address = Pubkey::create_program_address(
            &[
                anker_instance.as_ref(),
                seed,
                &[bump_seed],
            ],
            anker_program_id,
        ).expect("Depends only on Anker-controlled values, should not fail.");

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

    /// Confirm that the provided reserve account is the one that belongs to this instance.
    ///
    /// This does not check that the reserve is an stSOL account.
    pub fn check_reserve_address(&self, anker_program_id: &Pubkey, anker_instance: &Pubkey, reserve_account_info: &AccountInfo) -> ProgramResult {
        self.check_derived_account_address(
            "the reserve account",
            ANKER_RESERVE_ACCOUNT,
            self.reserve_account_bump_seed,
            anker_program_id,
            anker_instance,
            reserve_account_info,
        )
    }

    /// Confirm that the provided reserve authority is the one that belongs to this instance.
    pub fn check_reserve_authority(&self, anker_program_id: &Pubkey, anker_instance: &Pubkey, reserve_authority_info: &AccountInfo) -> ProgramResult {
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
    pub fn check_mint_authority(&self, anker_program_id: &Pubkey, anker_instance: &Pubkey, mint_authority_info: &AccountInfo) -> ProgramResult {
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
        Anker::check_is_spl_token_account(
            "our bSOL",
            &self.b_sol_mint,
            token_account_info,
        )
    }

    /// Confirm that the account is an SPL token account that holds stSOL.
    pub fn check_is_st_sol_account(&self, solido: &Lido, token_account_info: &AccountInfo) -> ProgramResult {
        Anker::check_is_spl_token_account(
            "Solido's stSOL",
            &solido.st_sol_mint,
            token_account_info,
        )
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
