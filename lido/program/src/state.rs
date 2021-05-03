//! State transition types

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::{entrypoint::ProgramResult, msg, pubkey::Pubkey};
use std::convert::TryFrom;

use crate::error::LidoError;

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct Lido {
    pub stake_pool_account: Pubkey,
    pub owner: Pubkey,
    pub lsol_mint_program: Pubkey,
    pub total_sol: u64,
    pub lsol_total_shares: u64,

    pub sol_reserve_authority_bump_seed: u8,
    pub deposit_authority_bump_seed: u8,
    pub token_reserve_authority_bump_seed: u8,
    pub token_program_id: Pubkey,
}

impl Lido {
    pub fn calc_pool_tokens_for_deposit(&self, stake_lamports: u64) -> Option<u64> {
        if self.total_sol == 0 {
            return Some(stake_lamports);
        }
        u64::try_from(
            (stake_lamports as u128)
                .checked_mul(self.lsol_total_shares as u128)?
                .checked_div(self.total_sol as u128)?,
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
        owner_key: &Pubkey,
        stakepool_key: &Pubkey,
        lsol_mint_key: &Pubkey,
    ) -> ProgramResult {
        if &self.owner != owner_key {
            return Err(LidoError::InvalidOwner.into());
        }
        if &self.stake_pool_account != stakepool_key {
            return Err(LidoError::InvalidStakePool.into());
        }

        if &self.lsol_mint_program != lsol_mint_key {
            return Err(LidoError::InvalidToken.into());
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
pub struct Owner {
    members: Pubkey,
}

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub enum LidoAccountType {
    Uninitialized,
    Initialized,
}

impl Default for LidoAccountType {
    fn default() -> Self {
        LidoAccountType::Uninitialized
    }
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct LidoMembers {
    /// Account type, must be LidoMembers currently
    pub account_type: LidoAccountType,

    maximum_members: u32,
    list: Vec<Pubkey>,
}

impl LidoMembers {
    pub fn new(maximum_members: u32) -> Self {
        Self {
            account_type: LidoAccountType::Uninitialized,
            maximum_members,
            list: vec![Pubkey::default(); maximum_members as usize],
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.account_type != LidoAccountType::Uninitialized
    }
}
