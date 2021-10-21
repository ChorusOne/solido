use borsh::BorshDeserialize;
use lido::token::Lamports;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program::invoke,
    program_error::ProgramError, program_pack::Pack, pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};

use lido::{state::Lido, token::StLamports};

use crate::logic::{create_account, initialize_reserve_account};
use crate::state::ANKER_LEN;
use crate::{
    error::AnchorError,
    find_instance_address, find_mint_authority, find_reserve_account, find_reserve_authority,
    instruction::{
        AnchorInstruction, ClaimRewardsAccountsInfo, DepositAccountsInfo, InitializeAccountsInfo,
    },
    logic::{deserialize_anchor, mint_b_sol_to},
    state::Anchor,
    token::BLamports,
    ANCHOR_RESERVE_ACCOUNT,
};

fn process_initialize(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;
    let rent = Rent::from_account_info(accounts.sysvar_rent)?;

    let (anker_address, anker_bump_seed) = find_instance_address(program_id, accounts.lido.key);

    if anker_address != *accounts.anchor.key {
        msg!(
            "Expected to initialize instance at {}, but {} was provided.",
            anker_address,
            accounts.anchor.key,
        );
        // TODO: Return a proper error.
        panic!();
    }

    let solido = Lido::deserialize_lido(accounts.lido_program.key, accounts.lido)?;
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
        return Err(AnchorError::InvalidReserveAccount.into());
    }

    let anker_seeds = [accounts.lido.key.as_ref(), &[anker_bump_seed]];
    create_account(
        program_id,
        &accounts,
        accounts.anchor,
        &rent,
        ANKER_LEN,
        &anker_seeds,
    )?;

    // Create and initialize an stSOL SPL token account for the reserve.
    let reserve_account_seeds = [
        anker_address.as_ref(),
        ANCHOR_RESERVE_ACCOUNT,
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

    let anchor = Anchor {
        bsol_mint: *accounts.b_sol_mint.key,
        lido: *accounts.lido.key,
        reserve_authority,
        mint_authority_bump_seed: mint_bump_seed,
        reserve_authority_bump_seed,
        reserve_account_bump_seed,
    };

    // TODO: Check the mint program, similar to `lido::logic::check_mint`.

    anchor.save(accounts.anchor)
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

    let anchor = deserialize_anchor(
        program_id,
        accounts.anchor,
        accounts.lido.key,
        accounts.to_reserve_account.key,
    )?;

    // Check if the mint account is the same as the one stored in Anchor.
    anchor.check_mint(accounts.b_sol_mint.key)?;

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

    let lido = Lido::deserialize_lido(accounts.lido_program.key, accounts.lido)?;

    // Use Lido's exchange rate (`sol_balance / sol_supply`) to compute the
    // amount of BLamports to mint.
    let sol_value = lido.exchange_rate.exchange_st_sol(amount)?;
    let b_sol_amount = BLamports(sol_value.0);

    mint_b_sol_to(
        &anchor,
        accounts.anchor.key,
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

/// Claim Anker rewards
fn process_claim_rewards(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = ClaimRewardsAccountsInfo::try_from_slice(accounts_raw)?;
    let anchor = deserialize_anchor(
        program_id,
        accounts.anchor,
        accounts.lido.key,
        accounts.reserve_account.key,
    )?;
    anchor.check_mint(accounts.b_sol_mint.key)?;
    let lido = Lido::deserialize_lido(accounts.lido_program.key, accounts.lido)?;

    let token_mint_state =
        spl_token::state::Mint::unpack_from_slice(&accounts.b_sol_mint.data.borrow())?;
    let b_sol_supply = token_mint_state.supply;

    let st_sol_reserve_state =
        spl_token::state::Account::unpack_from_slice(&accounts.reserve_account.data.borrow())?;
    let reserve_st_sol = StLamports(st_sol_reserve_state.amount);

    // Get StLamports corresponding to the amount of b_sol minted.
    let st_sol_amount = lido.exchange_rate.exchange_sol(Lamports(b_sol_supply))?;

    // If `reserve_st_sol` < `st_sol_amount` something went wrong, and we abort the transaction.
    let rewards = (reserve_st_sol - st_sol_amount)?;

    Ok(())
}

/// Processes [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    let instruction = AnchorInstruction::try_from_slice(input)?;
    match instruction {
        AnchorInstruction::Initialize => process_initialize(program_id, accounts),
        AnchorInstruction::Deposit { amount } => process_deposit(program_id, accounts, amount),
        AnchorInstruction::Withdraw { amount } => todo!("{}", amount),
        AnchorInstruction::ClaimRewards => process_claim_rewards(program_id, accounts),
    }
}
