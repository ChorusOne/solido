use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo, borsh::try_from_slice_unchecked, entrypoint::ProgramResult, msg,
    program::invoke_signed, program_pack::Pack, pubkey::Pubkey,
};
use spl_stake_pool::{
    error::StakePoolError,
    instruction::{
        add_validator_to_pool, create_validator_stake_account, remove_validator_from_pool,
    },
    state::StakePool,
};

use crate::{
    error::LidoError,
    instruction::{
        AddValidatorInfo, ChangeFeeSpecInfo, ClaimValidatorFeeInfo,
        CreateValidatorStakeAccountInfo, DistributeFeesInfo, RemoveValidatorInfo,
    },
    logic::{token_mint_to, transfer_to},
    state::{distribute_fees, FeeDistribution, Lido, StLamports, StakePoolTokenLamports},
    FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_AUTHORITY,
};

pub fn process_create_validator_stake_account(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = CreateValidatorStakeAccountInfo::try_from_slice(accounts_raw)?;
    if accounts.stake_pool.owner != accounts.stake_pool_program.key {
        msg!(
            "Stake pool state is owned by {} but should be owned by {}",
            accounts.stake_pool.owner,
            accounts.stake_pool_program.key,
        );
        return Err(LidoError::InvalidOwner.into());
    }
    if accounts.lido.owner != program_id {
        msg!("State has invalid owner");
        return Err(LidoError::InvalidOwner.into());
    }
    let lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;
    lido.check_manager(accounts.manager)?;
    lido.check_stake_pool(accounts.stake_pool)?;
    let (stake_pool_authority, stake_pool_authority_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..], STAKE_POOL_AUTHORITY],
        program_id,
    );
    if &stake_pool_authority != accounts.staker.key {
        msg!("Wrong stake pool staker");
        return Err(LidoError::InvalidStaker.into());
    }

    invoke_signed(
        &create_validator_stake_account(
            accounts.stake_pool_program.key,
            accounts.stake_pool.key,
            accounts.staker.key,
            accounts.funder.key,
            accounts.stake_account.key,
            accounts.validator.key,
        ),
        &[
            accounts.stake_pool_program.clone(),
            accounts.staker.clone(),
            accounts.funder.clone(),
            accounts.stake_account.clone(),
            accounts.validator.clone(),
            accounts.sysvar_rent.clone(),
            accounts.sysvar_clock.clone(),
            accounts.sysvar_stake_history.clone(),
            accounts.stake_program_config.clone(),
            accounts.system_program.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes()[..32],
            STAKE_POOL_AUTHORITY,
            &[stake_pool_authority_bump_seed],
        ]],
    )
}

pub fn process_change_fee_spec(
    program_id: &Pubkey,
    new_fee_distribution: FeeDistribution,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = ChangeFeeSpecInfo::try_from_slice(accounts_raw)?;
    if accounts.lido.owner != program_id {
        msg!("State has invalid owner");
        return Err(LidoError::InvalidOwner.into());
    }

    let mut lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;
    lido.check_manager(accounts.manager)?;

    Lido::check_valid_minter_program(&lido.st_sol_mint_program, accounts.insurance_account)?;
    Lido::check_valid_minter_program(&lido.st_sol_mint_program, accounts.treasury_account)?;
    Lido::check_valid_minter_program(&lido.st_sol_mint_program, accounts.manager_fee_account)?;

    lido.fee_distribution = new_fee_distribution;
    lido.fee_recipients.insurance_account = *accounts.insurance_account.key;
    lido.fee_recipients.treasury_account = *accounts.treasury_account.key;
    lido.fee_recipients.manager_account = *accounts.manager_fee_account.key;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|e| e.into())
}

pub fn process_add_validator(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = AddValidatorInfo::try_from_slice(accounts_raw)?;
    if accounts.lido.owner != program_id {
        msg!("Lido state has an invalid owner, should be the Lido program");
        return Err(LidoError::InvalidOwner.into());
    }
    if accounts.stake_pool.owner != accounts.stake_pool_program.key {
        msg!(
            "Stake pool state is owned by {} but should be owned by {}",
            accounts.stake_pool.owner,
            accounts.stake_pool_program.key,
        );
        return Err(LidoError::InvalidOwner.into());
    }
    let mut lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;
    lido.check_manager(accounts.manager)?;
    lido.check_stake_pool(accounts.stake_pool)?;
    if &lido.stake_pool_account != accounts.stake_pool.key {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }

    let validator_token_account = spl_token::state::Account::unpack_from_slice(
        &accounts.validator_token_account.data.borrow(),
    )?;
    if lido.st_sol_mint_program != validator_token_account.mint {
        msg!(
            "Validator account minter should be the same as Lido minter {}",
            lido.st_sol_mint_program
        );
        return Err(LidoError::InvalidTokenMinter.into());
    }

    invoke_signed(
        &add_validator_to_pool(
            accounts.stake_pool_program.key,
            accounts.stake_pool.key,
            accounts.stake_pool_manager_authority.key,
            accounts.stake_pool_withdraw_authority.key,
            accounts.stake_pool_validator_list.key,
            accounts.stake_account.key,
        ),
        &[
            accounts.stake_pool_program.clone(),
            accounts.stake_pool.clone(),
            accounts.stake_pool_manager_authority.clone(),
            accounts.stake_pool_withdraw_authority.clone(),
            accounts.stake_pool_validator_list.clone(),
            accounts.stake_account.clone(),
            accounts.sysvar_clock.clone(),
            accounts.sysvar_stake_history.clone(),
            accounts.sysvar_stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes()[..32],
            STAKE_POOL_AUTHORITY,
            &[lido.stake_pool_authority_bump_seed],
        ]],
    )?;

    // If the condition below is false, the stake pool operation should have failed, but
    // we double check to be sure
    if lido
        .fee_recipients
        .validator_credit_accounts
        .validator_accounts
        .len() as u32
        == lido.fee_recipients.validator_credit_accounts.max_validators
    {
        msg!("Maximum number of validators reached");
        return Err(LidoError::UnexpectedValidatorCreditAccountSize.into());
    }

    lido.fee_recipients.validator_credit_accounts.add(
        *accounts.stake_account.key,
        *accounts.validator_token_account.key,
    )?;
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

/// Removes a validator from the stake pool, notice that the validator might not
/// be immediately removed from the validators list in the stake pool after this
/// instruction is executed, this function requires the validator has no
/// unclaimed fees.
/// The validator stake account to be removed:
/// `accounts::stake_account_to_remove` should have exactly 1 Sol + rent for
/// holding a Stake account, this is checked in `remove_validator_from_pool` from
/// the Stake Pool.
pub fn process_remove_validator(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = RemoveValidatorInfo::try_from_slice(accounts_raw)?;
    if accounts.lido.owner != program_id {
        msg!("Lido state has an invalid owner, should be the Lido program");
        return Err(LidoError::InvalidOwner.into());
    }
    if accounts.stake_pool.owner != accounts.stake_pool_program.key {
        msg!(
            "Stake pool state is owned by {} but should be owned by {}",
            accounts.stake_pool.owner,
            accounts.stake_pool_program.key,
        );
        return Err(LidoError::InvalidOwner.into());
    }
    let mut lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;
    if &lido.stake_pool_account != accounts.stake_pool.key {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }

    invoke_signed(
        &remove_validator_from_pool(
            accounts.stake_pool_program.key,
            accounts.stake_pool.key,
            accounts.stake_pool_manager_authority.key,
            accounts.stake_pool_withdraw_authority.key,
            accounts.new_withdraw_authority.key,
            accounts.stake_pool_validator_list.key,
            accounts.stake_account_to_remove.key,
            accounts.transient_stake.key,
        ),
        &[
            accounts.stake_pool_program.clone(),
            accounts.stake_pool.clone(),
            accounts.stake_pool_manager_authority.clone(),
            accounts.stake_pool_withdraw_authority.clone(),
            accounts.new_withdraw_authority.clone(),
            accounts.stake_pool_validator_list.clone(),
            accounts.stake_account_to_remove.clone(),
            accounts.sysvar_clock.clone(),
            accounts.sysvar_stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes(),
            STAKE_POOL_AUTHORITY,
            &[lido.stake_pool_authority_bump_seed],
        ]],
    )?;

    // finds the validator index, this should never return an error
    let validator_idx = lido
        .fee_recipients
        .validator_credit_accounts
        .validator_accounts
        .iter()
        .position(|v| &v.stake_address == accounts.stake_account_to_remove.key)
        .ok_or(LidoError::ValidatorCreditNotFound)?;

    if lido
        .fee_recipients
        .validator_credit_accounts
        .validator_accounts[validator_idx]
        .st_sol_amount
        != StLamports(0)
    {
        msg!("Validator still has tokens to claim. Reclaim tokens before removing the validator");
        return Err(LidoError::ValidatorHasUnclaimedCredit.into());
    }

    lido.fee_recipients
        .validator_credit_accounts
        .validator_accounts
        .swap_remove(validator_idx);
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

pub fn process_claim_validator_fee(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = ClaimValidatorFeeInfo::try_from_slice(accounts_raw)?;

    if accounts.lido.owner != program_id {
        msg!("Lido has an invalid owner");
        return Err(LidoError::InvalidOwner.into());
    }

    let mut lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;

    let validator_account = lido
        .fee_recipients
        .validator_credit_accounts
        .validator_accounts
        .iter_mut()
        .find(|vc| &vc.token_address == accounts.validator_token.key)
        .ok_or(LidoError::InvalidValidatorCreditAccount)?;
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.mint_program.clone(),
        accounts.validator_token.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        validator_account.st_sol_amount,
    )?;
    validator_account.st_sol_amount = StLamports(0);
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

pub fn process_distribute_fees(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = DistributeFeesInfo::try_from_slice(accounts_raw)?;
    if accounts.lido.owner != program_id {
        msg!("Lido state has an invalid owner, should be the Lido program");
        return Err(LidoError::InvalidOwner.into());
    }

    let mut lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;
    lido.check_stake_pool(accounts.stake_pool)?;

    let stake_pool = StakePool::try_from_slice(&accounts.stake_pool.data.borrow())?;
    if &stake_pool.manager_fee_account != accounts.stake_pool_fee_account.key {
        msg!("Invalid fee account from StakePool");
        return Err(StakePoolError::InvalidFeeAccount.into());
    }
    let stake_pool_fee_account = spl_token::state::Account::unpack_from_slice(
        &accounts.stake_pool_fee_account.data.borrow(),
    )?;

    let token_shares = distribute_fees(
        &lido.fee_distribution,
        lido.fee_recipients
            .validator_credit_accounts
            .validator_accounts
            .len() as u64,
        StakePoolTokenLamports(stake_pool_fee_account.amount),
    )
    .ok_or(LidoError::CalculationFailure)?;

    // Send all tokens to Lido token holder
    transfer_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.stake_pool_fee_account.clone(),
        accounts.token_holder_stake_pool.clone(),
        accounts.stake_pool_manager_fee_account.clone(),
        FEE_MANAGER_AUTHORITY,
        lido.fee_manager_bump_seed,
        stake_pool_fee_account.amount,
    )?;

    // Mint tokens for insurance
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.mint_program.clone(),
        accounts.insurance_account.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.insurance_amount,
    )?;
    // Mint tokens for treasury
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.mint_program.clone(),
        accounts.treasury_account.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.treasury_amount,
    )?;
    // Mint tokens for manager
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.mint_program.clone(),
        accounts.manager_fee_account.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.manager_amount,
    )?;

    // Update validator list that can be claimed at a later time
    for vc in lido
        .fee_recipients
        .validator_credit_accounts
        .validator_accounts
        .iter_mut()
    {
        vc.st_sol_amount = (vc.st_sol_amount + token_shares.reward_per_validator)
            .ok_or(LidoError::CalculationFailure)?;
    }
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

/// TODO
/// Called by the validator, changes the fee account which the validator
/// receives tokens
pub fn _process_change_validator_fee_account(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
) -> ProgramResult {
    unimplemented!()
}
