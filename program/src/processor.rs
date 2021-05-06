//! Program state processor

use solana_program::program_pack::Pack;
use spl_stake_pool::{stake_program, state::StakePool};

use crate::{
    error::LidoError,
    instruction::{stake_pool_deposit, LidoInstruction},
    logic::{check_reserve_authority, rent_exemption, AccountType},
    state::Lido,
    DEPOSIT_AUTHORITY_ID, RESERVE_AUTHORITY_ID, STAKE_POOL_TOKEN_RESERVE_AUTHORITY_ID,
};

use {
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::next_account_info,
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        msg,
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        pubkey::Pubkey,
        rent::Rent,
        system_instruction,
        sysvar::Sysvar,
    },
    spl_stake_pool::borsh::try_from_slice_unchecked,
    std::convert::TryFrom,
};

fn get_stake_state(
    stake_account_info: &AccountInfo,
) -> Result<(stake_program::Meta, stake_program::Stake), ProgramError> {
    let stake_state =
        try_from_slice_unchecked::<stake_program::StakeState>(&stake_account_info.data.borrow())?;
    match stake_state {
        stake_program::StakeState::Stake(meta, stake) => Ok((meta, stake)),
        _ => Err(LidoError::WrongStakeState.into()),
    }
}

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
        // Token program account (SPL Token Program)
        let token_program_info = next_account_info(account_info_iter)?;
        let pool_token_to_info = next_account_info(account_info_iter)?;

        let rent = &Rent::from_account_info(rent_info)?;
        rent_exemption(rent, stakepool_info, AccountType::StakePool)?;
        rent_exemption(rent, lido_info, AccountType::Lido)?;

        let mut lido = try_from_slice_unchecked::<Lido>(&lido_info.data.borrow())?;
        lido.is_initialized()?;

        let stake_pool = StakePool::try_from_slice(&stakepool_info.data.borrow())?;
        if stake_pool.is_uninitialized() {
            msg!("Provided stake pool not initialized");
            return Err(LidoError::InvalidStakePool.into());
        }

        let (_, reserve_bump_seed) = Pubkey::find_program_address(
            &[&lido_info.key.to_bytes()[..32], RESERVE_AUTHORITY_ID],
            program_id,
        );

        let (_, deposit_bump_seed) = Pubkey::find_program_address(
            &[&lido_info.key.to_bytes()[..32], DEPOSIT_AUTHORITY_ID],
            program_id,
        );

        let (_, token_reserve_bump_seed) = Pubkey::find_program_address(
            &[
                &lido_info.key.to_bytes()[..32],
                STAKE_POOL_TOKEN_RESERVE_AUTHORITY_ID,
            ],
            program_id,
        );

        let (pool_token_to_pubkey, _) = Pubkey::find_program_address(
            &[&pool_token_to_info.key.to_bytes()[..32], RESERVE_AUTHORITY_ID],
            program_id,
        );

        lido.stake_pool_account = *stakepool_info.key;
        lido.owner = *owner_info.key;
        lido.lsol_mint_program = *mint_program_info.key;
        lido.sol_reserve_authority_bump_seed = reserve_bump_seed;
        lido.deposit_authority_bump_seed = deposit_bump_seed;
        lido.token_reserve_authority_bump_seed = token_reserve_bump_seed;
        lido.token_program_id = *token_program_info.key;
        lido.pool_token_to = pool_token_to_pubkey;

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
        let stake_pool_info = next_account_info(account_info_iter)?;
        // Recipient of tokens from the stake pool
        let pool_token_to_info = next_account_info(account_info_iter)?;
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
        let reserve_authority_info = next_account_info(account_info_iter)?;
        // System program
        let system_program_info = next_account_info(account_info_iter)?;

        if user_info.lamports() < amount {
            return Err(LidoError::InvalidAmount.into());
        }

        let mut lido = Lido::try_from_slice(&lido_info.data.borrow())?;

        lido.check_lido_for_deposit(owner_info.key, stake_pool_info.key, lsol_mint_info.key)?;
        lido.check_token_program_id(token_program_info.key)?;
        check_reserve_authority(lido_info, program_id, reserve_authority_info)?;

        if &lido.stake_pool_account != stake_pool_info.key {
            return Err(LidoError::InvalidStakePool.into());
        }
        let stake_pool = StakePool::try_from_slice(&stake_pool_info.data.borrow())?;
        if !stake_pool.is_valid() {
            return Err(LidoError::InvalidStakePool.into());
        }
        if &stake_pool.token_program_id != token_program_info.key {
            return Err(LidoError::InvalidTokenProgram.into());
        }
        // if &lido.pool_token_to != pool_token_to_info.key {
        //     return Err(LidoError::InvalidToken.into());
        // }

        let reserve_lamports = reserve_authority_info.lamports();

        let pool_to_token_account =
            spl_token::state::Account::unpack_from_slice(&pool_token_to_info.data.borrow())?;

        // stake_pool_total_sol * stake_pool_token(pool_token_to_info)/stake_pool_total_tokens
        let stake_pool_lamports = if stake_pool.pool_token_supply != 0 {
            u64::try_from(
                (stake_pool.total_stake_lamports as u128)
                    .checked_mul(pool_to_token_account.amount as u128)
                    .ok_or(LidoError::CalculationFailure)?
                    .checked_div(stake_pool.pool_token_supply as u128)
                    .ok_or(LidoError::CalculationFailure)?,
            )
            .or_else(|_| Err::<u64, ProgramError>(LidoError::CalculationFailure.into()))?
        } else {
            0
        };

        let total_lamports = reserve_lamports + stake_pool_lamports;
        invoke(
            &system_instruction::transfer(user_info.key, reserve_authority_info.key, amount),
            &[
                user_info.clone(),
                reserve_authority_info.clone(),
                system_program_info.clone(),
            ],
        )?;

        let lsol_amount = lido
            .calc_pool_tokens_for_deposit(amount, total_lamports)
            .ok_or(LidoError::CalculationFailure)?;

        let total_lsol = lido.lsol_total_shares + lsol_amount;

        let ix = spl_token::instruction::mint_to(
            token_program_info.key,
            lsol_mint_info.key,
            lsol_recipient_info.key,
            reserve_authority_info.key,
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
                reserve_authority_info.clone(),
                token_program_info.clone(),
            ],
            signers,
        )?;

        lido.lsol_total_shares = total_lsol;

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
        let validator_info = next_account_info(account_info_iter)?;
        let reserve_info = next_account_info(account_info_iter)?;
        let stake_info = next_account_info(account_info_iter)?;
        let deposit_authority_info = next_account_info(account_info_iter)?;

        // Sys
        let clock_info = next_account_info(account_info_iter)?;
        let system_program_info = next_account_info(account_info_iter)?;
        let rent_info = next_account_info(account_info_iter)?;
        let stake_program_info = next_account_info(account_info_iter)?;
        let stake_history_info = next_account_info(account_info_iter)?;
        let stake_config_info = next_account_info(account_info_iter)?;

        let rent = &Rent::from_account_info(rent_info)?;
        let lido = Lido::try_from_slice(&lido_info.data.borrow())?;

        let (to_pubkey, stake_bump_seed) =
            Pubkey::find_program_address(&[&validator_info.key.to_bytes()[..32]], program_id);
        if &to_pubkey != stake_info.key {
            return Err(LidoError::InvalidStaker.into());
        }

        let me_bytes = lido_info.key.to_bytes();
        let reserve_authority_seed: &[&[_]] = &[&me_bytes, RESERVE_AUTHORITY_ID][..];
        let (reserve_authority, _) =
            Pubkey::find_program_address(reserve_authority_seed, program_id);

        if reserve_info.key != &reserve_authority {
            return Err(LidoError::InvalidReserveAuthority.into());
        }

        if amount < rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()) {
            return Err(LidoError::InvalidAmount.into());
        }

        // TODO: Reference more validators

        let authority_signature_seeds: &[&[_]] = &[
            &me_bytes,
            &RESERVE_AUTHORITY_ID,
            &[lido.sol_reserve_authority_bump_seed],
        ];

        let validator_stake_seeds: &[&[_]] =
            &[&validator_info.key.to_bytes()[..32], &[stake_bump_seed]];

        // Check if the stake_info exists
        if get_stake_state(stake_info).is_ok() {
            return Err(LidoError::WrongStakeState.into());
        }

        invoke_signed(
            &system_instruction::create_account(
                reserve_info.key,
                stake_info.key,
                amount,
                std::mem::size_of::<stake_program::StakeState>() as u64,
                &stake_program::id(),
            ),
            // &[reserve_info.clone(), stake_info.clone()],
            &[
                reserve_info.clone(),
                stake_info.clone(),
                system_program_info.clone(),
            ],
            &[&authority_signature_seeds, &validator_stake_seeds],
        )?;

        invoke(
            &stake_program::initialize(
                stake_info.key,
                &stake_program::Authorized {
                    staker: *deposit_authority_info.key,
                    withdrawer: *deposit_authority_info.key,
                },
                &stake_program::Lockup::default(),
            ),
            &[
                stake_info.clone(),
                rent_info.clone(),
                stake_program_info.clone(),
            ],
        )?;

        invoke_signed(
            &stake_program::delegate_stake(
                stake_info.key,
                deposit_authority_info.key,
                validator_info.key,
            ),
            &[
                stake_info.clone(),
                validator_info.clone(),
                clock_info.clone(),
                stake_history_info.clone(),
                stake_config_info.clone(),
                deposit_authority_info.clone(),
            ],
            &[&[
                &lido_info.key.to_bytes()[..32],
                DEPOSIT_AUTHORITY_ID,
                &[lido.deposit_authority_bump_seed],
            ]],
        )
    }

    pub fn process_stake_pool_delegate(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let lido_info = next_account_info(account_info_iter)?;
        let validator_info = next_account_info(account_info_iter)?;
        let stake_info = next_account_info(account_info_iter)?;
        let deposit_authority_info = next_account_info(account_info_iter)?;
        let pool_token_to_info = next_account_info(account_info_iter)?;

        // Stake pool
        let stake_pool_program_info = next_account_info(account_info_iter)?;
        let stake_pool_info = next_account_info(account_info_iter)?;
        let stake_pool_validator_list_info = next_account_info(account_info_iter)?;
        let stake_pool_withdraw_authority_info = next_account_info(account_info_iter)?;
        let stake_pool_validator_stake_account_info = next_account_info(account_info_iter)?;
        let stake_pool_mint_info = next_account_info(account_info_iter)?;

        // Sys
        let _clock_info = next_account_info(account_info_iter)?;
        let _stake_history_info = next_account_info(account_info_iter)?;
        let _system_program_info = next_account_info(account_info_iter)?;
        let rent_info = next_account_info(account_info_iter)?;
        let token_program_info = next_account_info(account_info_iter)?;
        let _stake_program_info = next_account_info(account_info_iter)?;

        let _rent = &Rent::from_account_info(rent_info)?;
        let lido = Lido::try_from_slice(&lido_info.data.borrow())?;

        let (to_pubkey, _) =
            Pubkey::find_program_address(&[&validator_info.key.to_bytes()[..32]], program_id);

        let (stake_pool_token_reserve_authority, _) = Pubkey::find_program_address(
            &[
                &lido_info.key.to_bytes()[..32],
                STAKE_POOL_TOKEN_RESERVE_AUTHORITY_ID,
            ],
            program_id,
        );

        if &to_pubkey != stake_info.key {
            return Err(LidoError::InvalidStaker.into());
        }

        let pool_token_account =
            spl_token::state::Account::unpack_from_slice(&pool_token_to_info.data.borrow())?;

        // TODO: Pass lido as account and check pool_token_to_info is equal to the save done
        // TODO: Check if this is enough
        if stake_pool_token_reserve_authority != pool_token_account.owner {
            msg!(
                "Wrong stake pool reserve authority: {}",
                pool_token_account.owner
            );
            return Err(LidoError::InvalidOwner.into());
        }

        invoke_signed(
            &stake_pool_deposit(
                &stake_pool_program_info.key,
                &stake_pool_info.key,
                &stake_pool_validator_list_info.key,
                &deposit_authority_info.key,
                &stake_pool_withdraw_authority_info.key,
                &stake_info.key,
                &stake_pool_validator_stake_account_info.key,
                &pool_token_to_info.key,
                &stake_pool_mint_info.key,
                &token_program_info.key,
            ),
            &[
                stake_pool_program_info.clone(),
                stake_pool_info.clone(),
                stake_pool_validator_list_info.clone(),
                deposit_authority_info.clone(),
                stake_pool_withdraw_authority_info.clone(),
                stake_info.clone(),
                stake_pool_validator_stake_account_info.clone(),
                pool_token_to_info.clone(),
                stake_pool_mint_info.clone(),
                token_program_info.clone(),
            ],
            &[&[
                &lido_info.key.to_bytes()[..32],
                DEPOSIT_AUTHORITY_ID,
                &[lido.deposit_authority_bump_seed],
            ]],
        )?;
        Ok(())
    }

    pub fn process_withdraw(
        _program_id: &Pubkey,
        _pool_tokens: u64,
        _accounts: &[AccountInfo],
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
            LidoInstruction::StakePoolDelegate => {
                Self::process_stake_pool_delegate(program_id, accounts)
            }
            LidoInstruction::Withdraw { amount } => {
                Self::process_withdraw(program_id, amount, accounts)
            }
        }
    }
}
