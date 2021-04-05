use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction, system_program,
    sysvar::Sysvar,
};
use spl_token::state::Mint;

use crate::{
    error::StakePoolError,
    model::{InitArgs, ValidatorStakeList},
    state::StakePool,
    PROGRAM_VERSION,
};

const PROCESSOR_MIN_RESERVE_BALANCE: u64 = 1000000;
pub struct Processor;
impl Processor {
    /// Suffix for deposit authority seed
    pub const AUTHORITY_DEPOSIT: &'static [u8] = b"deposit";
    /// Suffix for reserve account seed
    pub const AUTHORITY_RESERVE: &'static [u8] = b"reserve";
    /// Suffix for withdraw authority seed
    pub const AUTHORITY_WITHDRAW: &'static [u8] = b"withdraw";
    /// Suffix for temp account
    pub const TEMP_ACCOUNT: &'static [u8] = b"temp";

    pub fn process_deposit(
        program_id: &Pubkey,
        amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        if amount == 0 {
            msg!("Amount must not be zero");
            return Err(ProgramError::InvalidArgument);
        }

        let account_info_iter = &mut accounts.iter();
        // Stake pool
        let stake_pool_info = next_account_info(account_info_iter)?;
        // Stake pool withdraw authority
        let withdraw_info = next_account_info(account_info_iter)?;
        // Reserve account
        let reserve_account_info = next_account_info(account_info_iter)?;
        // User account to transfer SOLs from
        let source_user_info = next_account_info(account_info_iter)?;
        // User account to receive pool tokens
        let dest_user_info = next_account_info(account_info_iter)?;
        // Account to receive pool fee tokens
        let owner_fee_info = next_account_info(account_info_iter)?;
        // Pool token mint account
        let pool_mint_info = next_account_info(account_info_iter)?;
        // Rent sysvar account
        let rent_info = next_account_info(account_info_iter)?;
        let rent = &Rent::from_account_info(rent_info)?;
        // System program id
        let system_program_info = next_account_info(account_info_iter)?;
        // Pool token program id
        let token_program_info = next_account_info(account_info_iter)?;

        let (temp_account_info, native_mint_info) =
            if *source_user_info.owner != system_program::id() {
                (
                    Some(next_account_info(account_info_iter)?),
                    Some(next_account_info(account_info_iter)?),
                )
            } else {
                (None, None)
            };

        if stake_pool_info.owner != program_id {
            msg!(
                "Wrong owner {} for the stake pool {}. Expected {}",
                stake_pool_info.owner,
                stake_pool_info.key,
                program_id
            );
            return Err(StakePoolError::WrongOwner.into());
        }
        // Check program ids
        let mut stake_pool = StakePool::deserialize(&stake_pool_info.data.borrow())?;
        if !stake_pool.is_initialized() {
            return Err(StakePoolError::InvalidState.into());
        }

        stake_pool.check_authority_withdraw(withdraw_info.key, program_id, stake_pool_info.key)?;

        let (expected_reserve, reserve_bump) =
            Self::get_reserve_adderess(program_id, stake_pool_info.key);
        if *reserve_account_info.key != expected_reserve {
            msg!(
                "Expected reserve to be {} but got {}",
                &expected_reserve,
                reserve_account_info.key
            );
            return Err(ProgramError::IncorrectProgramId);
        }

        if stake_pool.owner_fee_account != *owner_fee_info.key {
            return Err(StakePoolError::InvalidFeeAccount.into());
        }
        if stake_pool.token_program_id != *token_program_info.key {
            return Err(ProgramError::IncorrectProgramId);
        }

        // Check stake pool last update epoch
        // if stake_pool.last_update_epoch < clock.epoch {
        // return Err(StakePoolError::StakeListAndPoolOutOfDate.into());
        // }

        let target_balance = **reserve_account_info.lamports.borrow() + amount;
        if target_balance < Self::min_reserve_balance(&rent) {
            return Err(StakePoolError::FirstDepositIsTooSmall.into());
        }

        let pool_amount = stake_pool
            .calc_pool_deposit_amount(amount)
            .ok_or(StakePoolError::CalculationFailure)?;

        let fee_amount = stake_pool
            .calc_fee_amount(pool_amount)
            .ok_or(StakePoolError::CalculationFailure)?;

        let user_amount = pool_amount
            .checked_sub(fee_amount)
            .ok_or(StakePoolError::CalculationFailure)?;

        let withdraw_signer_seeds: &[&[_]] = &[
            &stake_pool_info.key.to_bytes()[..32],
            Self::AUTHORITY_WITHDRAW,
            &[stake_pool.withdraw_bump_seed],
        ];

        // Transfer user's SOLs to reserve
        if let (Some(temp_account_info), Some(native_mint_info)) =
            (temp_account_info, native_mint_info)
        {
            let (expected_temp_address, temp_bump) = Pubkey::find_program_address(
                &[&stake_pool_info.key.to_bytes()[..32], Self::TEMP_ACCOUNT],
                program_id,
            );

            if *temp_account_info.key != expected_temp_address {
                msg!(
                    "Expected temp account {} but got {}",
                    &expected_temp_address,
                    temp_account_info.key
                );
                return Err(ProgramError::InvalidArgument);
            }

            if *native_mint_info.key != spl_token::native_mint::id() {
                msg!("Expected native mint");
                return Err(ProgramError::InvalidArgument);
            }

            let temp_seeds = &[
                &stake_pool_info.key.to_bytes()[..32],
                Self::TEMP_ACCOUNT,
                &[temp_bump],
            ];

            let reserve_signer_seeds: &[&[u8]] = &[
                &stake_pool_info.key.to_bytes()[..32],
                Self::AUTHORITY_RESERVE,
                &[reserve_bump],
            ];

            invoke_signed(
                &system_instruction::create_account(
                    reserve_account_info.key,
                    temp_account_info.key,
                    rent.minimum_balance(spl_token::state::Account::LEN),
                    spl_token::state::Account::LEN as u64,
                    &spl_token::id(),
                ),
                &[
                    reserve_account_info.clone(),
                    temp_account_info.clone(),
                    system_program_info.clone(),
                ],
                &[temp_seeds, reserve_signer_seeds],
            )?;

            invoke(
                &spl_token::instruction::initialize_account(
                    token_program_info.key,
                    temp_account_info.key,
                    &spl_token::native_mint::id(),
                    withdraw_info.key,
                )?,
                &[
                    token_program_info.clone(),
                    temp_account_info.clone(),
                    native_mint_info.clone(),
                    withdraw_info.clone(),
                    rent_info.clone(),
                ],
            )?;

            invoke_signed(
                &spl_token::instruction::transfer(
                    token_program_info.key,
                    source_user_info.key,
                    temp_account_info.key,
                    withdraw_info.key,
                    &[],
                    amount,
                )?,
                &[
                    token_program_info.clone(),
                    source_user_info.clone(),
                    temp_account_info.clone(),
                    withdraw_info.clone(),
                ],
                &[withdraw_signer_seeds],
            )?;

            invoke_signed(
                &spl_token::instruction::close_account(
                    token_program_info.key,
                    temp_account_info.key,
                    reserve_account_info.key,
                    withdraw_info.key,
                    &[],
                )?,
                &[
                    token_program_info.clone(),
                    temp_account_info.clone(),
                    reserve_account_info.clone(),
                    withdraw_info.clone(),
                ],
                &[withdraw_signer_seeds],
            )?;
        } else {
            // Initial deposit must be enough
            if target_balance < PROCESSOR_MIN_RESERVE_BALANCE {
                return Err(StakePoolError::FirstDepositIsTooSmall.into());
            }
            invoke(
                &system_instruction::transfer(
                    source_user_info.key,
                    reserve_account_info.key,
                    amount,
                ),
                &[
                    source_user_info.clone(),
                    reserve_account_info.clone(),
                    system_program_info.clone(),
                ],
            )?;
        }

        Self::token_mint_to(
            stake_pool_info.key,
            token_program_info.clone(),
            pool_mint_info.clone(),
            dest_user_info.clone(),
            withdraw_info.clone(),
            Self::AUTHORITY_WITHDRAW,
            stake_pool.withdraw_bump_seed,
            user_amount,
        )?;

        Self::token_mint_to(
            stake_pool_info.key,
            token_program_info.clone(),
            pool_mint_info.clone(),
            owner_fee_info.clone(),
            withdraw_info.clone(),
            Self::AUTHORITY_WITHDRAW,
            stake_pool.withdraw_bump_seed,
            fee_amount,
        )?;
        stake_pool.pool_total += pool_amount;
        stake_pool.stake_total += amount;
        stake_pool.serialize(&mut stake_pool_info.data.borrow_mut())?;

        Ok(())
    }

    pub fn authority_id(
        program_id: &Pubkey,
        stake_pool: &Pubkey,
        authority_type: &[u8],
        bump_seed: u8,
    ) -> Result<Pubkey, ProgramError> {
        Pubkey::create_program_address(
            &[&stake_pool.to_bytes()[..32], authority_type, &[bump_seed]],
            program_id,
        )
        .map_err(|_| StakePoolError::InvalidProgramAddress.into())
    }

    /// Generates seed bump for stake pool authorities
    pub fn find_authority_bump_seed(
        program_id: &Pubkey,
        stake_pool: &Pubkey,
        authority_type: &[u8],
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[&stake_pool.to_bytes()[..32], authority_type], program_id)
    }

    /// Checks withdraw or deposit authority
    pub fn check_authority(
        authority_to_check: &Pubkey,
        program_id: &Pubkey,
        stake_pool_key: &Pubkey,
        authority_type: &[u8],
        bump_seed: u8,
    ) -> Result<(), ProgramError> {
        let id = Self::authority_id(program_id, stake_pool_key, authority_type, bump_seed)?;
        if *authority_to_check != id {
            msg!(
                "Check {} authority fails. Expected {} got {}",
                std::str::from_utf8(authority_type).unwrap(),
                id,
                authority_to_check
            );
            return Err(StakePoolError::InvalidProgramAddress.into());
        }
        Ok(())
    }

    /// Issue a spl_token `MintTo` instruction.
    #[allow(clippy::too_many_arguments)]
    pub fn token_mint_to<'a>(
        stake_pool: &Pubkey,
        token_program: AccountInfo<'a>,
        mint: AccountInfo<'a>,
        destination: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        authority_type: &[u8],
        bump_seed: u8,
        amount: u64,
    ) -> Result<(), ProgramError> {
        let me_bytes = stake_pool.to_bytes();
        let authority_signature_seeds = [&me_bytes[..32], authority_type, &[bump_seed]];
        let signers = &[&authority_signature_seeds[..]];

        let ix = spl_token::instruction::mint_to(
            token_program.key,
            mint.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?;

        invoke_signed(&ix, &[mint, destination, authority, token_program], signers)
    }

    /// Get address for reserve
    pub fn get_reserve_adderess(program_id: &Pubkey, stake_pool: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[&stake_pool.to_bytes()[..32], &Self::AUTHORITY_RESERVE],
            program_id,
        )
    }

    fn min_reserve_balance(rent: &Rent) -> u64 {
        PROCESSOR_MIN_RESERVE_BALANCE
            .max(rent.minimum_balance(0) + rent.minimum_balance(spl_token::state::Account::LEN))
    }

    pub fn process_initialize(
        program_id: &Pubkey,
        init: InitArgs,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let stake_pool_info = next_account_info(account_info_iter)?;
        let owner_info = next_account_info(account_info_iter)?;
        let validator_stake_list_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let owner_fee_info = next_account_info(account_info_iter)?;
        // Clock sysvar account
        let clock_info = next_account_info(account_info_iter)?;
        let clock = &Clock::from_account_info(clock_info)?;
        // Rent sysvar account
        let rent_info = next_account_info(account_info_iter)?;
        let rent = &Rent::from_account_info(rent_info)?;
        // Token program ID
        let token_program_info = next_account_info(account_info_iter)?;

        if stake_pool_info.owner != program_id {
            msg!(
                "Wrong owner {} for the stake pool {}. Expected {}",
                stake_pool_info.owner,
                stake_pool_info.key,
                program_id
            );
            return Err(StakePoolError::WrongOwner.into());
        }
        let mut stake_pool = StakePool::deserialize(&stake_pool_info.data.borrow())?;
        // Stake pool account should not be already initialized
        if stake_pool.is_initialized() {
            return Err(StakePoolError::AlreadyInUse.into());
        }

        // Check if transaction was signed by owner
        if !owner_info.is_signer {
            return Err(StakePoolError::SignatureMissing.into());
        }

        // Check if validator stake list storage is unitialized
        let mut validator_stake_list =
            ValidatorStakeList::deserialize(&validator_stake_list_info.data.borrow())?;
        if validator_stake_list.is_initialized() {
            return Err(StakePoolError::AlreadyInUse.into());
        }
        validator_stake_list.version = ValidatorStakeList::VALIDATOR_STAKE_LIST_VERSION;
        validator_stake_list.validators.clear();

        // Check if stake pool account is rent-exempt
        if !rent.is_exempt(stake_pool_info.lamports(), stake_pool_info.data_len()) {
            return Err(StakePoolError::AccountNotRentExempt.into());
        }

        // Check if validator stake list account is rent-exempt
        if !rent.is_exempt(
            validator_stake_list_info.lamports(),
            validator_stake_list_info.data_len(),
        ) {
            return Err(StakePoolError::AccountNotRentExempt.into());
        }

        let (_, deposit_bump_seed) = Self::find_authority_bump_seed(
            program_id,
            stake_pool_info.key,
            Self::AUTHORITY_DEPOSIT,
        );
        let (withdraw_authority_key, withdraw_bump_seed) = Self::find_authority_bump_seed(
            program_id,
            stake_pool_info.key,
            Self::AUTHORITY_WITHDRAW,
        );

        // Numerator should be smaller than or equal to denominator (fee <= 1)
        if init.fee.numerator > init.fee.denominator {
            return Err(StakePoolError::FeeTooHigh.into());
        }

        // Check if fee account's owner the same as token program id
        if owner_fee_info.owner != token_program_info.key {
            msg!(
                "Expexted owner fee's account {} to have {} owner but it has {}",
                owner_fee_info.key,
                token_program_info.key,
                owner_fee_info.owner
            );
            return Err(StakePoolError::InvalidFeeAccount.into());
        }

        // Check pool mint program ID
        if pool_mint_info.owner != token_program_info.key {
            return Err(ProgramError::IncorrectProgramId);
        }

        // Check for owner fee account to have proper mint assigned
        if *pool_mint_info.key
            != spl_token::state::Account::unpack_from_slice(&owner_fee_info.data.borrow())?.mint
        {
            return Err(StakePoolError::WrongAccountMint.into());
        }

        let pool_mint = Mint::unpack_from_slice(&pool_mint_info.data.borrow())?;

        if !pool_mint.mint_authority.contains(&withdraw_authority_key) {
            msg!(
                "Mint authority is {} but need to be {}",
                pool_mint.mint_authority.unwrap_or(Pubkey::new(&[0; 32])),
                withdraw_authority_key
            );
            return Err(StakePoolError::WrongMintingAuthority.into());
        }

        if pool_mint.supply > 0 {
            return Err(StakePoolError::MintHasInitialSupply.into());
        }

        validator_stake_list.serialize(&mut validator_stake_list_info.data.borrow_mut())?;

        msg!("Clock data: {:?}", clock_info.data.borrow());
        msg!("Epoch: {}", clock.epoch);

        stake_pool.version = PROGRAM_VERSION;
        stake_pool.owner = *owner_info.key;
        stake_pool.deposit_bump_seed = deposit_bump_seed;
        stake_pool.withdraw_bump_seed = withdraw_bump_seed;
        stake_pool.validator_stake_list = *validator_stake_list_info.key;
        stake_pool.pool_mint = *pool_mint_info.key;
        stake_pool.owner_fee_account = *owner_fee_info.key;
        stake_pool.token_program_id = *token_program_info.key;
        stake_pool.last_update_epoch = clock.epoch;
        stake_pool.fee = init.fee;

        stake_pool.serialize(&mut stake_pool_info.data.borrow_mut())
    }
}
