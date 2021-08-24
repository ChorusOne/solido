// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use solana_program::entrypoint::ProgramResult;
use solana_program::program_option::COption;
use solana_program::program_pack::Pack;
use solana_program::stake::state::StakeAuthorize;
use solana_program::{
    account_info::AccountInfo, borsh::try_from_slice_unchecked, msg, program::invoke,
    program::invoke_signed, program_error::ProgramError, pubkey::Pubkey, rent::Rent,
    stake as stake_program, system_instruction,
};

use crate::STAKE_AUTHORITY;
use crate::{
    error::LidoError,
    instruction::{CollectValidatorFeeInfo, UnstakeAccountsInfo, WithdrawAccountsInfo},
    state::Fees,
    state::Lido,
    token::{Lamports, StLamports},
    MINT_AUTHORITY, RESERVE_ACCOUNT,
};

pub(crate) fn check_rent_exempt(
    rent: &Rent,
    account_info: &AccountInfo,
    account_name: &'static str,
) -> Result<(), ProgramError> {
    if !rent.is_exempt(account_info.lamports(), account_info.data_len()) {
        msg!("{} ({}) is not rent-exempt", account_name, account_info.key);
        return Err(ProgramError::AccountNotRentExempt);
    }
    Ok(())
}

/// Check if the mint program coin supply is zero and the mint authority is set
/// to `mint_authority`.
/// The check has to be done only in Solido's initialization phase, since we
/// store the mint public key in the solido structure, and it never changes.
pub(crate) fn check_mint(
    rent: &Rent,
    mint: &AccountInfo,
    mint_authority: &Pubkey,
) -> Result<(), ProgramError> {
    if !rent.is_exempt(mint.lamports(), mint.data_len()) {
        msg!("Mint is not rent-exempt");
        return Err(ProgramError::AccountNotRentExempt);
    }
    let spl_mint = spl_token::state::Mint::unpack_from_slice(&mint.data.borrow())?;
    if spl_mint.supply != 0 {
        msg!(
            "Mint should not have minted tokens, has {}.",
            spl_mint.supply
        );
        return Err(LidoError::InvalidMint.into());
    }
    if let COption::Some(authority) = spl_mint.mint_authority {
        if &authority != mint_authority {
            msg!(
                "Mint authority should be {}, it's {} instead.",
                mint_authority,
                authority
            );
            return Err(LidoError::InvalidMint.into());
        }
    } else {
        msg!("Mint should have an authority.");
        return Err(LidoError::InvalidMint.into());
    }
    Ok(())
}

/// Subtract the minimum rent-exempt balance from the given reserve balance.
///
/// The rent-exempt amount can never be transferred, or the account would
/// disappear, so we should not treat it as part of Solido's managed SOL.
pub fn get_reserve_available_balance(
    rent: &Rent,
    reserve_account: &AccountInfo,
) -> Result<Lamports, LidoError> {
    let minimum_balance = Lamports(rent.minimum_balance(0));
    match Lamports(reserve_account.lamports()) - minimum_balance {
        Ok(balance) => Ok(balance),
        Err(..) => {
            msg!("The reserve account is not rent-exempt.");
            msg!("Please ensure it holds at least {}.", minimum_balance);
            Err(LidoError::ReserveIsNotRentExempt)
        }
    }
}

pub struct CreateAccountOptions<'a, 'b> {
    /// The amount to transfer from the reserve to the new account.
    pub fund_amount: Lamports,
    /// The size of the data section of the account.
    pub data_size: u64,
    /// Owner of the new account.
    pub owner: Pubkey,
    /// Seeds needed to sign on behalf of the new account.
    pub sign_seeds: &'a [&'a [u8]],
    /// The account to initialize.
    pub account: &'a AccountInfo<'b>,
}

/// Create a new account and fund it from the reserve.
///
/// Unlike `system_instruction::create_account`, this will not fail if the account
/// already exists. This is important, because if account creation would fail for
/// stake accounts, then someone could transfer a small amount to the next stake
/// account for a validator, and that would prevent us from delegating more stake
/// to that validator.
pub fn create_account_overwrite_if_exists<'a, 'b>(
    solido_address: &Pubkey,
    options: CreateAccountOptions<'a, 'b>,
    reserve: &AccountInfo<'b>,
    reserve_account_bump_seed: u8,
    system_program: &AccountInfo<'b>,
) -> ProgramResult {
    let reserve_account_bump_seed = [reserve_account_bump_seed];
    let reserve_account_seeds = &[
        solido_address.as_ref(),
        RESERVE_ACCOUNT,
        &reserve_account_bump_seed[..],
    ][..];

    // `system_instruction::create_account` performs the same three steps as we
    // do below, but it additionally has a check to prevent creating an account
    // that has a nonzero balance, which we omit here.
    invoke_signed(
        &system_instruction::allocate(options.account.key, options.data_size),
        &[options.account.clone(), system_program.clone()],
        &[options.sign_seeds],
    )?;
    invoke_signed(
        &system_instruction::assign(options.account.key, &options.owner),
        &[options.account.clone(), system_program.clone()],
        &[options.sign_seeds],
    )?;
    invoke_signed(
        &system_instruction::transfer(reserve.key, options.account.key, options.fund_amount.0),
        &[
            reserve.clone(),
            options.account.clone(),
            system_program.clone(),
        ],
        &[reserve_account_seeds, options.sign_seeds],
    )?;
    Ok(())
}

/// Call the stake program to initialize the account, but do not yet delegate it.
pub fn initialize_stake_account_undelegated<'a>(
    stake_authority: &Pubkey,
    stake_account: &AccountInfo<'a>,
    sysvar_rent: &AccountInfo<'a>,
    stake_program: &AccountInfo<'a>,
) -> ProgramResult {
    invoke(
        &stake_program::instruction::initialize(
            stake_account.key,
            &stake_program::state::Authorized {
                staker: *stake_authority,
                withdrawer: *stake_authority,
            },
            &stake_program::state::Lockup::default(),
        ),
        &[
            stake_account.clone(),
            sysvar_rent.clone(),
            stake_program.clone(),
        ],
    )
}

/// Mint the given amount of stSOL and put it in the recipient's account.
///
/// * The stSOL mint must be the one configured in the Solido instance.
/// * The recipient account must be an stSOL SPL token account.
pub fn mint_st_sol_to<'a>(
    solido: &Lido,
    solido_address: &Pubkey,
    spl_token_program: &AccountInfo<'a>,
    st_sol_mint: &AccountInfo<'a>,
    mint_authority: &AccountInfo<'a>,
    recipient: &AccountInfo<'a>,
    amount: StLamports,
) -> ProgramResult {
    solido.check_mint_is_st_sol_mint(st_sol_mint)?;
    solido.check_is_st_sol_account(recipient)?;

    let solido_address_bytes = solido_address.to_bytes();
    let authority_signature_seeds = [
        &solido_address_bytes[..],
        MINT_AUTHORITY,
        &[solido.mint_authority_bump_seed],
    ];
    let signers = [&authority_signature_seeds[..]];

    // The SPL token program supports multisig-managed mints, but we do not
    // use those.
    let mint_to_signers = [];

    let instruction = spl_token::instruction::mint_to(
        spl_token_program.key,
        st_sol_mint.key,
        recipient.key,
        mint_authority.key,
        &mint_to_signers,
        amount.0,
    )?;

    invoke_signed(
        &instruction,
        &[
            st_sol_mint.clone(),
            recipient.clone(),
            mint_authority.clone(),
            spl_token_program.clone(),
        ],
        &signers,
    )
}

/// Burns the given amount of stSOL.
///
/// * The stSOL mint must be the one configured in the Solido instance.
/// * The account account must be an stSOL SPL token account.
pub fn burn_st_sol<'a, 'b>(
    solido: &Lido,
    accounts: &WithdrawAccountsInfo<'a, 'b>,
    amount: StLamports,
) -> ProgramResult {
    solido.check_mint_is_st_sol_mint(accounts.st_sol_mint)?;
    solido.check_is_st_sol_account(accounts.st_sol_account)?;

    let st_sol_account: spl_token::state::Account =
        spl_token::state::Account::unpack_from_slice(&accounts.st_sol_account.data.borrow())?;

    // Check if the user is the account owner.
    if &st_sol_account.owner != accounts.st_sol_account_owner.key {
        msg!(
            "Token is owned by {}, but provided owner is {}.",
            st_sol_account.owner,
            accounts.st_sol_account_owner.key,
        );
        return Err(LidoError::InvalidTokenOwner.into());
    }

    // The SPL token program supports multisig-managed mints, but we do not
    // use those.
    let burn_signers = [];
    let instruction = spl_token::instruction::burn(
        accounts.spl_token.key,
        accounts.st_sol_account.key,
        accounts.st_sol_mint.key,
        accounts.st_sol_account_owner.key,
        &burn_signers,
        amount.0,
    )?;

    invoke(
        &instruction,
        &[
            accounts.st_sol_account.clone(),
            accounts.st_sol_mint.clone(),
            accounts.st_sol_account_owner.clone(),
            accounts.spl_token.clone(),
        ],
    )
}
// Set the stake and withdraw authority of the destination stake account to the
// user’s pubkey.
pub fn transfer_stake_authority(
    accounts: &WithdrawAccountsInfo,
    stake_authority_bump_seed: u8,
) -> ProgramResult {
    invoke_signed(
        &solana_program::stake::instruction::authorize(
            accounts.destination_stake_account.key,
            accounts.stake_authority.key,
            accounts.st_sol_account_owner.key,
            StakeAuthorize::Withdrawer,
            None,
        ),
        &[
            accounts.destination_stake_account.clone(),
            accounts.sysvar_clock.clone(),
            accounts.stake_authority.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes(),
            STAKE_AUTHORITY,
            &[stake_authority_bump_seed],
        ]],
    )?;
    invoke_signed(
        &solana_program::stake::instruction::authorize(
            accounts.destination_stake_account.key,
            accounts.stake_authority.key,
            accounts.st_sol_account_owner.key,
            StakeAuthorize::Staker,
            None,
        ),
        &[
            accounts.destination_stake_account.clone(),
            accounts.sysvar_clock.clone(),
            accounts.stake_authority.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes(),
            STAKE_AUTHORITY,
            &[stake_authority_bump_seed],
        ]],
    )
}

/// Mint stSOL for the given fees, and transfer them to the appropriate accounts.
pub fn distribute_fees<'a, 'b>(
    solido: &mut Lido,
    accounts: &CollectValidatorFeeInfo<'a, 'b>,
    fees: Fees,
) -> ProgramResult {
    // Convert all fees to stSOL according to the previously updated exchange rate.
    // In the case of fees, the SOL is already part of one of the stake accounts,
    // but we do still need to mint stSOL to represent it.

    let treasury_amount = solido.exchange_rate.exchange_sol(fees.treasury_amount)?;

    let developer_amount = solido.exchange_rate.exchange_sol(fees.developer_amount)?;

    let per_validator_amount = solido
        .exchange_rate
        .exchange_sol(fees.reward_per_validator)?;

    // The treasury and developer fee we can mint and pay immediately.
    mint_st_sol_to(
        solido,
        accounts.lido.key,
        accounts.spl_token_program,
        accounts.st_sol_mint,
        accounts.mint_authority,
        accounts.treasury_st_sol_account,
        treasury_amount,
    )?;
    mint_st_sol_to(
        solido,
        accounts.lido.key,
        accounts.spl_token_program,
        accounts.st_sol_mint,
        accounts.mint_authority,
        accounts.developer_st_sol_account,
        developer_amount,
    )?;

    // For the validators, as there can be many of them, we can't pay all of
    // them in a single transaction. Instead, we store how much they are
    // entitled to, and they can later claim it themselves with `ClaimValidatorFee`.
    let mut fee_validation_sol = Lamports(0);
    let mut fee_validation_st_sol = StLamports(0);
    for validator in solido.validators.iter_entries_mut() {
        validator.fee_credit = (validator.fee_credit + per_validator_amount)?;
        fee_validation_sol = (fee_validation_sol + fees.reward_per_validator)?;
        fee_validation_st_sol = (fee_validation_st_sol + per_validator_amount)?;
    }

    // Also record our rewards in the metrics.
    solido
        .metrics
        .observe_fee_treasury(fees.treasury_amount, treasury_amount)?;
    solido
        .metrics
        .observe_fee_validation(fee_validation_sol, fee_validation_st_sol)?;
    solido
        .metrics
        .observe_fee_developer(fees.developer_amount, developer_amount)?;
    solido
        .metrics
        .observe_reward_st_sol_appreciation(fees.st_sol_appreciation_amount)?;

    Ok(())
}

/// Checks if the stake accounts on `accounts` correspond to the ones generated
/// by the validator's seeds. Returns the destination bump seed.
pub fn check_unstake_accounts(
    program_id: &Pubkey,
    lido: &Lido,
    accounts: &UnstakeAccountsInfo,
) -> Result<u8, ProgramError> {
    let validator = lido.validators.get(accounts.validator_vote_account.key)?;

    // If a validator doesn't have a stake account, it cannot be unstaked.
    if validator.entry.stake_seeds.begin == validator.entry.stake_seeds.end {
        msg!(
            "Attempting to unstake from a validator {} that has no stake accounts.",
            validator.pubkey
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }
    let source_stake_seed = validator.entry.stake_seeds.begin;
    let destination_stake_seed = validator.entry.unstake_seeds.end;

    let (source_stake_account, _) =
        validator.find_stake_account_address(program_id, accounts.lido.key, source_stake_seed);

    if &source_stake_account != accounts.source_stake_account.key {
        msg!(
            "Source stake account differs from the one calculated by seed {}, should be {}, is {}.",
            source_stake_seed,
            source_stake_account,
            accounts.source_stake_account.key
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }

    let (destination_stake_account, destination_bump_seed) = validator
        .find_unstake_account_address(program_id, accounts.lido.key, destination_stake_seed);
    if &destination_stake_account != accounts.destination_stake_account.key {
        msg!(
            "Destination stake account differs from the one calculated by seed {}, should be {}, is {}.",
            destination_stake_seed,
            destination_stake_account,
            accounts.destination_stake_account.key
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }
    Ok(destination_bump_seed)
}

pub struct SplitStakeAccounts<'a, 'b> {
    pub source_stake_account: &'a AccountInfo<'b>,
    pub destination_stake_account: &'a AccountInfo<'b>,
    pub authority: &'a AccountInfo<'b>,
    pub system_program: &'a AccountInfo<'b>,
    pub stake_program: &'a AccountInfo<'b>,
}

/// Splits `amount` Lamports from the stake in `accounts.source_stake_account`
/// to the stake in `accounts.destination_stake_account`.
///
/// Issue 3 transactions with `invoke_signed` signed with seeds specified by `seeds`:
///   - Allocates space in the `accounts.destination_stake_account`.
///   - Assigns the owner of the `accounts.destination_stake_account` to the stake program.
///   - Splits the stake.
pub fn split_stake_account(
    lido_address: &Pubkey,
    lido: &Lido,
    accounts: &SplitStakeAccounts,
    amount: Lamports,
    seeds: &[&[&[u8]]],
) -> ProgramResult {
    // The Split instruction returns three instructions:
    //   0 - Allocate instruction.
    //   1 - Assign owner instruction.
    //   2 - Split stake instruction.
    let split_instructions = solana_program::stake::instruction::split(
        accounts.source_stake_account.key,
        accounts.authority.key,
        amount.0,
        accounts.destination_stake_account.key,
    );
    assert_eq!(split_instructions.len(), 3);

    let (allocate_instruction, assign_instruction, split_instruction) = (
        &split_instructions[0],
        &split_instructions[1],
        &split_instructions[2],
    );

    invoke_signed(
        allocate_instruction,
        &[
            accounts.destination_stake_account.clone(),
            accounts.system_program.clone(),
        ],
        seeds,
    )?;
    invoke_signed(
        assign_instruction,
        &[
            accounts.destination_stake_account.clone(),
            accounts.system_program.clone(),
        ],
        seeds,
    )?;

    invoke_signed(
        split_instruction,
        &[
            accounts.source_stake_account.clone(),
            accounts.destination_stake_account.clone(),
            accounts.authority.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &lido_address.to_bytes(),
            STAKE_AUTHORITY,
            &[lido.stake_authority_bump_seed],
        ]],
    )?;
    Ok(())
}

pub fn deserialize_lido(program_id: &Pubkey, lido: &AccountInfo) -> Result<Lido, ProgramError> {
    if lido.owner != program_id {
        msg!(
            "Lido state is owned by {}, but should be owned by the Lido program ({}).",
            lido.owner,
            program_id
        );
        return Err(LidoError::InvalidOwner.into());
    }
    let lido = try_from_slice_unchecked::<Lido>(&lido.data.borrow())?;
    Ok(lido)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_account_not_rent_exempt() {
        let key = Pubkey::default();
        let mut lamports = 3000;
        let data = &mut [0; 8];
        let mut rent = Rent::default();
        rent.lamports_per_byte_year = 100;
        rent.exemption_threshold = 1.0;
        let account = AccountInfo::new(&key, false, false, &mut lamports, data, &key, false, 1);

        let val = check_rent_exempt(&rent, &account, "dummy account");

        assert_eq!(val.err(), Some(ProgramError::AccountNotRentExempt));
    }

    #[test]
    fn test_account_is_rent_exempt() {
        let key = Pubkey::default();
        let mut lamports = 3000000;
        let data = &mut [0; 8];
        let mut rent = Rent::default();
        rent.lamports_per_byte_year = 100;
        rent.exemption_threshold = 1.0;
        let account = AccountInfo::new(&key, false, false, &mut lamports, data, &key, false, 1);

        let val = check_rent_exempt(&rent, &account, "dummy account");
        assert!(val.is_ok());
    }
}
