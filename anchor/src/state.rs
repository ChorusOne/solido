use crate::instruction::ClaimRewardsAccountsInfo;
use crate::{error::AnchorError, ANCHOR_RESERVE_ACCOUNT};
use crate::{find_reserve_account, find_reserve_authority, ANCHOR_RESERVE_AUTHORITY};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::{token::StLamports, util::serialize_b58};
use serde::Serialize;
use solana_program::program::invoke_signed;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_pack::Pack, pubkey::Pubkey,
};

/// Size of the serialized [`Anchor`] struct, in bytes.
pub const ANKER_LEN: usize = 164;

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Anchor {
    /// The SPL Token mint address for bSOL.
    #[serde(serialize_with = "serialize_b58")]
    pub bsol_mint: Pubkey,

    /// Reserve authority for `reserve_account`.
    #[serde(serialize_with = "serialize_b58")]
    pub reserve_authority: Pubkey,

    /// The associated LIDO state address.
    #[serde(serialize_with = "serialize_b58")]
    pub lido: Pubkey,

    /// Token swap data. Used to swap stSOL for UST.
    #[serde(serialize_with = "serialize_b58")]
    pub token_swap_instance: Pubkey,

    /// Destination of the rewards, paid in UST.
    #[serde(serialize_with = "serialize_b58")]
    pub rewards_destination: Pubkey,

    /// Bump seeds for signing messages on behalf of the authority.
    pub mint_authority_bump_seed: u8,
    pub reserve_authority_bump_seed: u8,
    pub reserve_account_bump_seed: u8,
    pub token_swap_bump_seed: u8,
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

    pub fn check_is_b_sol_account(&self, token_account_info: &AccountInfo) -> ProgramResult {
        if token_account_info.owner != &spl_token::id() {
            msg!(
                "Expected SPL token account to be owned by {}, but it's owned by {} instead.",
                spl_token::id(),
                token_account_info.owner
            );
            return Err(AnchorError::InvalidBSolAccountOwner.into());
        }
        let token_account =
            match spl_token::state::Account::unpack_from_slice(&token_account_info.data.borrow()) {
                Ok(account) => account,
                Err(..) => {
                    msg!(
                        "Expected an SPL token account at {}.",
                        token_account_info.key
                    );
                    return Err(AnchorError::InvalidBSolAccount.into());
                }
            };

        if token_account.mint != self.bsol_mint {
            msg!(
                "Expected mint of {} to be our bSOL mint ({}), but found {}.",
                token_account_info.key,
                self.bsol_mint,
                token_account.mint,
            );
            return Err(AnchorError::InvalidBSolMint.into());
        }
        Ok(())
    }

    pub fn check_reserve_account(
        &self,
        program_id: &Pubkey,
        anchor_state: &Pubkey,
        provided_reserve: &Pubkey,
    ) -> ProgramResult {
        let reserve_account = Pubkey::create_program_address(
            &[
                &anchor_state.to_bytes(),
                ANCHOR_RESERVE_ACCOUNT,
                &[self.reserve_account_bump_seed],
            ],
            program_id,
        )?;

        if &reserve_account != provided_reserve {
            msg!(
                "Invalid reserve account, expected {}, but found {}.",
                reserve_account,
                provided_reserve,
            );
            return Err(AnchorError::InvalidReserveAccount.into());
        }
        Ok(())
    }

    pub fn check_mint(&self, provided_mint: &Pubkey) -> ProgramResult {
        if &self.bsol_mint != provided_mint {
            msg!(
                "Invalid mint account, expected {}, but found {}.",
                self.bsol_mint,
                provided_mint,
            );
            return Err(AnchorError::InvalidBSolMint.into());
        }
        Ok(())
    }

    pub fn check_lido(&self, provided_lido: &Pubkey) -> ProgramResult {
        if &self.lido != provided_lido {
            msg!(
                "Invalid Lido account, expected {}, but found {}.",
                self.lido,
                provided_lido,
            );
            return Err(AnchorError::WrongLidoInstance.into());
        }
        Ok(())
    }

    pub fn swap_st_sol_for_ust_tokens(
        &self,
        program_id: &Pubkey,
        amount: StLamports,
        accounts: &ClaimRewardsAccountsInfo,
    ) -> ProgramResult {
        let (token_pool_authority, _) = Pubkey::find_program_address(
            &[&accounts.token_swap_instance.key.to_bytes()[..]],
            &spl_token_swap::id(),
        );
        let (st_sol_reserve, _bump_seed) =
            find_reserve_account(program_id, &accounts.token_swap_instance.key);

        let (reserve_authority, _bump_seed) =
            find_reserve_authority(program_id, &accounts.token_swap_instance.key);
        let swap_instruction = spl_token_swap::instruction::swap(
            accounts.spl_token_swap.key,
            accounts.spl_token.key,
            &accounts.token_swap_instance.key,
            &token_pool_authority,
            &reserve_authority,
            &st_sol_reserve,
            &accounts.st_sol_token.key,
            &accounts.ust_token.key,
            &self.rewards_destination,
            &accounts.pool_mint.key,
            &accounts.pool_fee_account.key,
            None,
            spl_token_swap::instruction::Swap {
                amount_in: amount.0,
                minimum_amount_out: u64::MAX,
            },
        )?;
        invoke_signed(
            &swap_instruction,
            &[
                accounts.token_swap_instance.clone(),
                accounts.token_pool_authority.clone(),
                accounts.reserve_authority.clone(),
                accounts.st_sol_reserve.clone(),
                accounts.st_sol_token.clone(),
                accounts.ust_token.clone(),
                accounts.rewards_destination.clone(),
                accounts.pool_mint.clone(),
                accounts.pool_fee_account.clone(),
                accounts.spl_token.clone(),
                accounts.spl_token_swap.clone(),
            ],
            &[&[
                &accounts.anchor.key.to_bytes(),
                ANCHOR_RESERVE_AUTHORITY,
                &[self.reserve_authority_bump_seed],
            ]],
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_anker_len() {
        let instance = Anchor::default();
        let mut writer = Vec::new();
        BorshSerialize::serialize(&instance, &mut writer).unwrap();
        assert_eq!(writer.len(), ANKER_LEN);
    }
}
