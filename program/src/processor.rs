//! Program state processor

use std::collections::HashSet;

use crate::{
    error::LidoError,
    state::{Lido, LidoAccountType, LidoMembers},
    AUTHORITY_ID,
};

use {
    bincode::deserialize,
    borsh::{BorshDeserialize, BorshSchema, BorshSerialize},
    num_traits::FromPrimitive,
    solana_program::{
        account_info::next_account_info,
        account_info::AccountInfo,
        clock::Clock,
        decode_error::DecodeError,
        entrypoint::ProgramResult,
        msg,
        native_token::sol_to_lamports,
        program::{invoke, invoke_signed},
        program_error::PrintProgramError,
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        stake_history::StakeHistory,
        system_instruction,
        sysvar::Sysvar,
    },
    spl_stake_pool::borsh::try_from_slice_unchecked,
    spl_token::state::Mint,
};

#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Instruction {
    Initialize {
        stake_pool_account: Pubkey,
        members_list_account: Vec<Pubkey>,
    },
    /// Deposit with amount
    Deposit {
        amount: u64,
    },
    /// Deposit amount to member validator
    DelegateDeposit {
        amount: u64,
        member: Pubkey,
    },
    Withdraw {
        amount: u64,
    },
}

/// Program state handler.
pub struct Processor;
impl Processor {
    pub fn process_initialize(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        stake_pool_account: Pubkey,
        new_members: &[Pubkey],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let lido_info = next_account_info(account_info_iter)?;
        let stakepool_info = next_account_info(account_info_iter)?;
        let owner_info = next_account_info(account_info_iter)?;
        let mint_program_info = next_account_info(account_info_iter)?;
        // let members_list_info = next_account_info(account_info_iter)?;
        let rent_info = next_account_info(account_info_iter)?;

        let rent = &Rent::from_account_info(rent_info)?;

        // let members_list =
        //     try_from_slice_unchecked::<LidoMembers>(&members_list_info.data.borrow())?;

        let mut lido = try_from_slice_unchecked::<Lido>(&lido_info.data.borrow())?;

        // if members_list.is_initialized() {
        //     return Err(LidoError::AlreadyInUse.into());
        // }
        // if !rent.is_exempt(members_list_info.lamports(), members_list_info.data_len()) {
        //     msg!("Members list is not rent-exempt");
        //     return Err(ProgramError::AccountNotRentExempt);
        // }

        // if !rent.is_exempt(lido_info.lamports(), lido_info.data_len()) {
        //     msg!("Lido is not rent-exempt");
        //     return Err(ProgramError::AccountNotRentExempt);
        // }

        // let mut members_list = new_members
        //     .into_iter()
        //     .fold(members_list, |mut acc, member| {
        //         acc.list.push(member.to_owned());
        //         acc
        //     });
        // members_list.account_type = LidoAccountType::Initialized;
        // members_list.serialize(&mut *members_list_info.data.borrow_mut())?;

        // // lido.owner = *owner_info.key;

        // pub stake_pool_account: Pubkey,
        // pub owner: Pubkey,
        // pub lsol_mint_program: Pubkey,
        // pub total_sol: u64,
        // pub lsol_total_shares: u64,
        // pub lido_authority_bump_seed: u8,

        let (_, bump_seed) = Pubkey::find_program_address(
            &[&lido_info.key.to_bytes()[..32], AUTHORITY_ID],
            program_id,
        );

        lido.stake_pool_account = *stakepool_info.key;
        lido.owner = *owner_info.key;
        lido.lsol_mint_program = *mint_program_info.key;
        lido.lido_authority_bump_seed = bump_seed;

        lido.serialize(&mut *lido_info.data.borrow_mut())
            .map_err(|e| e.into())
    }

    pub fn process_deposit(
        program_id: &Pubkey,
        amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        if amount == 0 {
            msg!("Amount must be greater than zero");
            return Err(ProgramError::InvalidArgument);
        }

        let account_info_iter = &mut accounts.iter();
        // Lido
        let lido_info = next_account_info(account_info_iter)?;
        // Stake pool
        let stake_pool = next_account_info(account_info_iter)?;
        // Owner program
        let owner_info = next_account_info(account_info_iter)?;
        // User account
        let user_info = next_account_info(account_info_iter)?;
        // User account
        let lsol_recipient_info = next_account_info(account_info_iter)?;
        // User account
        let lsol_mint_info = next_account_info(account_info_iter)?;
        // Token program account
        let token_program_info = next_account_info(account_info_iter)?;
        // Lido authority account
        let authority_info = next_account_info(account_info_iter)?;
        // Lido authority account
        let reserve_account_info = next_account_info(account_info_iter)?;
        // System program
        let system_program_info = next_account_info(account_info_iter)?;

        if user_info.lamports() < amount {
            return Err(LidoError::InvalidAmount.into());
        }

        let mut lido = Lido::try_from_slice(&lido_info.data.borrow())?;
        if &lido.owner != owner_info.key {
            return Err(LidoError::InvalidOwner.into());
        }
        if &lido.stake_pool_account != stake_pool.key {
            return Err(LidoError::InvalidStakePool.into());
        }

        if &lido.lsol_mint_program != lsol_mint_info.key {
            return Err(LidoError::InvalidToken.into());
        }

        // Overflow will never happen because we check that user has `amount` in its account
        // user_info.lamports.borrow_mut().checked_sub(amount);

        invoke(
            &system_instruction::transfer(user_info.key, reserve_account_info.key, amount),
            &[
                user_info.clone(),
                reserve_account_info.clone(),
                system_program_info.clone(),
            ],
        )?;
        // if total_sol == 0
        // lsol_amount = amount
        // else
        // lsol_amount = amount * total_lsol / total_sol

        // TODO  : Check if there are standard Solana functions for this
        let lsol_amount = if lido.total_sol == 0 {
            amount
        } else {
            amount * lido.lsol_total_shares / lido.total_sol
        };

        let total_lsol = lido.total_sol + lsol_amount;
        let total_sol = lido.lsol_total_shares + amount;

        let ix = spl_token::instruction::mint_to(
            token_program_info.key,
            lsol_mint_info.key,
            lsol_recipient_info.key,
            authority_info.key,
            &[],
            lsol_amount,
        )?;

        let me_bytes = lido_info.key.to_bytes();
        let authority_signature_seeds = [
            &me_bytes[..32],
            AUTHORITY_ID,
            &[lido.lido_authority_bump_seed],
        ];
        let signers = &[&authority_signature_seeds[..]];
        invoke_signed(
            &ix,
            &[
                lsol_mint_info.clone(),
                lsol_recipient_info.clone(),
                authority_info.clone(),
                token_program_info.clone(),
            ],
            signers,
        )?;

        // How to check if lido members is initialized?

        /*

        Step 1 : Load Relevant Accounts and Parse them into Rust Structures

        Step 2 : Make Checks

        - Boilerplate Checks
        - Logic Specific Checks
            - a) Deposit Amount > 0
            -

        Step 3 : Logic
            a) Take User's SOL and put it in deposit pool
            b) Calculate LSOL to mint : User's SOL = Total LSOL Minted Already / Total SOL in Pool
            c) Mint LSOL, Transfer to user
            d - maybe) Update Lido State Info : Total SOL in Pool = Total SOL + What's just deposited
            e) Update Lido State Info : Total LSOL = Total LSOL + what's just minted

        */

        lido.lsol_total_shares = total_lsol;
        lido.total_sol = total_sol;

        lido.serialize(&mut *lido_info.data.borrow_mut())
            .map_err(|e| e.into())
    }

    pub fn process_delegate_deposit(
        program_id: &Pubkey,
        member: &Pubkey,
        delegate_amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        // Deposit pool
        let deposit_pool_info = next_account_info(account_info_iter)?;

        Ok(())

        /*

        Step 1 : Load Relevant Accounts and Parse them into Rust Structures

        Step 2 : Make Checks

        - Boilerplate Checks
        - Logic Specific Checks

        Step 3: Logic

        */
    }

    pub fn process_withdraw(
        program_id: &Pubkey,
        pool_tokens: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        Ok(())
    }

    /// Processes [Instruction](enum.Instruction.html).
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        let instruction = Instruction::try_from_slice(input)?;
        match instruction {
            Instruction::Initialize {
                stake_pool_account,
                members_list_account,
            } => Self::process_initialize(
                program_id,
                accounts,
                stake_pool_account,
                &members_list_account,
            ),
            Instruction::Deposit { amount } => Self::process_deposit(program_id, amount, accounts),
            Instruction::DelegateDeposit { amount, member } => {
                Self::process_delegate_deposit(program_id, &member, amount, accounts)
            }
            Instruction::Withdraw { amount } => {
                Self::process_withdraw(program_id, amount, accounts)
            }
        }
    }
}
