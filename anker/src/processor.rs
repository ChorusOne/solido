use borsh::BorshDeserialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program::invoke,
    program_error::ProgramError, program_pack::Pack, pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};

use lido::{state::Lido, token::StLamports};

use crate::logic::{create_account, initialize_reserve_account};
use crate::state::ANKER_LEN;
use crate::{
    error::AnkerError,
    find_instance_address, find_mint_authority, find_reserve_account, find_reserve_authority,
    instruction::{AnkerInstruction, DepositAccountsInfo, InitializeAccountsInfo},
    logic::{deserialize_anker, mint_b_sol_to},
    state::Anker,
    token::BLamports,
    ANKER_RESERVE_ACCOUNT,
};

fn process_initialize(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;
    let rent = Rent::from_account_info(accounts.sysvar_rent)?;

    let (anker_address, anker_bump_seed) = find_instance_address(program_id, accounts.solido.key);

    if anker_address != *accounts.anker.key {
        msg!(
            "Expected to initialize instance at {}, but {} was provided.",
            anker_address,
            accounts.anker.key,
        );
        // TODO: Return a proper error.
        panic!();
    }

    let solido = Lido::deserialize_lido(accounts.solido_program.key, accounts.solido)?;
    if solido.st_sol_mint != *accounts.st_sol_mint.key {
        msg!(
            "Expected stSOL mint to be Soldido's mint {}, but got {}.",
            solido.st_sol_mint,
            accounts.st_sol_mint.key,
        );
        // TODO: Return a proper error.
        panic!();
    }

    let (_mint_authority, mint_bump_seed) = find_mint_authority(program_id, &anker_address);
    // TODO: Check mint authority.

    let (reserve_authority, reserve_authority_bump_seed) =
        find_reserve_authority(program_id, &anker_address);
    let (reserve_account, reserve_account_bump_seed) =
        find_reserve_account(program_id, &anker_address);
    if &reserve_account != accounts.reserve_account.key {
        msg!(
            "Invalid reserve account, expected {}, but found {}.",
            reserve_account,
            accounts.reserve_account.key,
        );
        return Err(AnkerError::InvalidReserveAccount.into());
    }

    let anker_seeds = [accounts.solido.key.as_ref(), &[anker_bump_seed]];
    create_account(
        program_id,
        &accounts,
        accounts.anker,
        &rent,
        ANKER_LEN,
        &anker_seeds,
    )?;

    // Create and initialize an stSOL SPL token account for the reserve.
    let reserve_account_seeds = [
        anker_address.as_ref(),
        ANKER_RESERVE_ACCOUNT,
        &[reserve_account_bump_seed],
    ];
    create_account(
        &spl_token::ID,
        &accounts,
        accounts.reserve_account,
        &rent,
        spl_token::state::Account::LEN,
        &reserve_account_seeds,
    )?;
    initialize_reserve_account(&accounts, &reserve_account_seeds)?;

    let anker = Anker {
        bsol_mint: *accounts.b_sol_mint.key,
        lido: *accounts.solido.key,
        reserve_authority,
        mint_authority_bump_seed: mint_bump_seed,
        reserve_authority_bump_seed,
        reserve_account_bump_seed,
    };

    // TODO: Check the mint program, similar to `lido::logic::check_mint`.

    anker.save(accounts.anker)
}

/// Deposit an amount of StLamports and get bSol in return.
fn process_deposit(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
    amount: StLamports,
) -> ProgramResult {
    let accounts = DepositAccountsInfo::try_from_slice(accounts_raw)?;
    if amount == StLamports(0) {
        msg!("Amount must be greater than zero");
        return Err(ProgramError::InvalidArgument);
    }

    let anker = deserialize_anker(
        program_id,
        accounts.anker,
        accounts.solido.key,
        accounts.to_reserve_account.key,
    )?;

    // Check if the mint account is the same as the one stored in Anker.
    anker.check_mint(accounts.b_sol_mint.key)?;

    // Transfer `amount` StLamports to the reserve.
    invoke(
        &spl_token::instruction::transfer(
            &spl_token::id(),
            accounts.from_account.key,
            accounts.to_reserve_account.key,
            accounts.user_authority.key,
            &[],
            amount.0,
        )?,
        &[
            accounts.from_account.clone(),
            accounts.to_reserve_account.clone(),
            accounts.user_authority.clone(),
            accounts.spl_token.clone(),
        ],
    )?;

    let solido = Lido::deserialize_lido(accounts.solido_program.key, accounts.solido)?;

    // Use Lido's exchange rate (`sol_balance / sol_supply`) to compute the
    // amount of BLamports to mint.
    let sol_value = solido.exchange_rate.exchange_st_sol(amount)?;
    let b_sol_amount = BLamports(sol_value.0);

    mint_b_sol_to(
        &anker,
        accounts.anker.key,
        accounts.spl_token,
        accounts.b_sol_mint,
        accounts.b_sol_mint_authority,
        accounts.b_sol_user_account,
        b_sol_amount,
    )?;

    msg!(
        "Anker: Deposited {}, minted {} in return.",
        amount,
        b_sol_amount,
    );

    Ok(())
}

/// Processes [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    let instruction = AnkerInstruction::try_from_slice(input)?;
    match instruction {
        AnkerInstruction::Initialize => process_initialize(program_id, accounts),
        AnkerInstruction::Deposit { amount } => process_deposit(program_id, accounts, amount),
        AnkerInstruction::Withdraw { amount } => todo!("{}", amount),
        AnkerInstruction::ClaimRewards => todo!(),
    }
}
