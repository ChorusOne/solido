//! Program state processor

use std::collections::HashSet;

use spl_stake_pool::stake_program;

use crate::{
    error::LidoError,
    instruction::LidoInstruction,
    state::{Lido, LidoAccountType, LidoMembers},
    DEPOSIT_AUTHORITY_ID, RESERVE_AUTHORITY_ID, TOKEN_RESERVE_AUTHORITY_ID,
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
    std::convert::TryFrom,
};

/// Program state handler.
pub struct Processor;
impl Processor {
    pub fn process_initialize(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
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

        let (_, reserve_bump_seed) = Pubkey::find_program_address(
            &[&lido_info.key.to_bytes()[..32], RESERVE_AUTHORITY_ID],
            program_id,
        );

        let (_, deposit_bump_seed) = Pubkey::find_program_address(
            &[&lido_info.key.to_bytes()[..32], DEPOSIT_AUTHORITY_ID],
            program_id,
        );

        let (_, token_reserve_bump_seed) = Pubkey::find_program_address(
            &[&lido_info.key.to_bytes()[..32], TOKEN_RESERVE_AUTHORITY_ID],
            program_id,
        );

        lido.stake_pool_account = *stakepool_info.key;
        lido.owner = *owner_info.key;
        lido.lsol_mint_program = *mint_program_info.key;
        lido.sol_reserve_authority_bump_seed = reserve_bump_seed;
        lido.deposit_authority_bump_seed = deposit_bump_seed;
        lido.token_reserve_authority_bump_seed = token_reserve_bump_seed;

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
        // Recipient account
        let lsol_recipient_info = next_account_info(account_info_iter)?;
        // Token minter
        let lsol_mint_info = next_account_info(account_info_iter)?;
        // Token program account (SPL Token Program)
        let token_program_info = next_account_info(account_info_iter)?;
        // Lido authority account
        let authority_info = next_account_info(account_info_iter)?;
        // Reserve account
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

        let lsol_amount = lido
            .calc_pool_tokens_for_deposit(amount)
            .ok_or(LidoError::CalculationFailure)?;

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
            RESERVE_AUTHORITY_ID,
            &[lido.sol_reserve_authority_bump_seed],
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
        amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let lido_info = next_account_info(account_info_iter)?;
        let stake_pool_program_info = next_account_info(account_info_iter)?;
        let stake_pool_info = next_account_info(account_info_iter)?;
        let validator_list_info = next_account_info(account_info_iter)?;
        let deposit_authority_info = next_account_info(account_info_iter)?;
        let withdraw_authority_info = next_account_info(account_info_iter)?;
        let stake_info = next_account_info(account_info_iter)?;
        let reserve_info = next_account_info(account_info_iter)?;
        let validator_stake_account_info = next_account_info(account_info_iter)?;
        // let dest_user_info = next_account_info(account_info_iter)?;
        let pool_tokens_authority = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let clock_info = next_account_info(account_info_iter)?;
        let clock = &Clock::from_account_info(clock_info)?;
        let stake_history_info = next_account_info(account_info_iter)?;
        let token_program_info = next_account_info(account_info_iter)?;
        let stake_program_info = next_account_info(account_info_iter)?;
        let rent_info = next_account_info(account_info_iter)?;

        let mut lido = Lido::try_from_slice(&lido_info.data.borrow())?;

        let (to_pubkey, _) = Pubkey::find_program_address(
            &[&validator_stake_account_info.key.to_bytes()[..32]],
            program_id,
        );
        if &to_pubkey != stake_info.key {
            return Err(LidoError::InvalidStaker.into());
        }

        let stake_account_ix = system_instruction::create_account(
            reserve_info.key,
            stake_info.key,
            amount,
            std::mem::size_of::<stake_program::StakeState>() as u64,
            &stake_program::id(),
        );

        let authority_signature_seeds = [
            &RESERVE_AUTHORITY_ID[..],
            &[lido.sol_reserve_authority_bump_seed],
        ];
        let signers = &[&authority_signature_seeds[..]];

        invoke_signed(
            &stake_account_ix,
            &[reserve_info.clone(), stake_info.clone()],
            signers,
        )?;

        let stake_init_ix = stake_program::initialize(
            stake_info.key,
            &stake_program::Authorized {
                staker: *deposit_authority_info.key,
                withdrawer: *deposit_authority_info.key,
            },
            &stake_program::Lockup::default(),
        );

        invoke(&stake_init_ix, &[stake_info.clone(), rent_info.clone()])?;

        let deposit_ixs = spl_stake_pool::instruction::deposit_with_authority(
            &stake_pool_program_info.key,
            &stake_pool_info.key,
            &validator_list_info.key,
            &deposit_authority_info.key,
            &withdraw_authority_info.key,
            &stake_info.key,
            &deposit_authority_info.key,
            &validator_stake_account_info.key,
            &pool_tokens_authority.key,
            &pool_mint_info.key,
            &token_program_info.key,
        );

        let auth_ix = deposit_ixs.get(0).unwrap();
        invoke_signed(auth_ix, &[], signers)?;

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
        let instruction = LidoInstruction::try_from_slice(input)?;
        match instruction {
            LidoInstruction::Initialize => Self::process_initialize(program_id, accounts),
            LidoInstruction::Deposit { amount } => {
                Self::process_deposit(program_id, amount, accounts)
            }
            LidoInstruction::DelegateDeposit { amount } => {
                Self::process_delegate_deposit(program_id, amount, accounts)
            }
            LidoInstruction::Withdraw { amount } => {
                Self::process_withdraw(program_id, amount, accounts)
            }
        }
    }
}
