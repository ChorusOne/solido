use borsh::BorshDeserialize;
use lido::token::Lamports;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program::invoke,
    program_error::ProgramError, program_option::COption, program_pack::Pack, pubkey::Pubkey,
    rent::Rent, sysvar::Sysvar,
};

use lido::{state::Lido, token::StLamports};

use crate::logic::{create_account, initialize_spl_account, swap_rewards};
use crate::state::ANKER_LEN;
use crate::{
    error::AnkerError,
    find_instance_address, find_mint_authority, find_reserve_authority, find_stsol_reserve_account,
    instruction::{
        AnkerInstruction, DepositAccountsInfo, InitializeAccountsInfo, SellRewardsAccountsInfo,
    },
    logic::{deserialize_anker, mint_b_sol_to},
    state::Anker,
    token::BLamports,
};
use crate::{find_ust_reserve_account, ANKER_STSOL_RESERVE_ACCOUNT, ANKER_UST_RESERVE_ACCOUNT};

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
        return Err(AnkerError::InvalidDerivedAccount.into());
    }

    let solido = Lido::deserialize_lido(accounts.solido_program.key, accounts.solido)?;

    // We generate these addresses here, and then at the end after constructing
    // the Anker instance, we check that these addresses match the provided ones.
    // This way we can re-use the existing checks.
    let (mint_authority, mint_bump_seed) = find_mint_authority(program_id, &anker_address);
    let (_reserve_authority, reserve_authority_bump_seed) =
        find_reserve_authority(program_id, &anker_address);
    let (_reserve_account, stsol_reserve_account_bump_seed) =
        find_stsol_reserve_account(program_id, &anker_address);
    let (_ust_reserve_account, ust_reserve_account_bump_seed) =
        find_ust_reserve_account(program_id, &anker_address);

    // Create an account for the Anker instance.
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
    let stsol_reserve_account_seeds = [
        anker_address.as_ref(),
        ANKER_STSOL_RESERVE_ACCOUNT,
        &[stsol_reserve_account_bump_seed],
    ];
    create_account(
        &spl_token::ID,
        &accounts,
        accounts.stsol_reserve_account,
        &rent,
        spl_token::state::Account::LEN,
        &stsol_reserve_account_seeds,
    )?;
    initialize_spl_account(
        &accounts,
        &stsol_reserve_account_seeds,
        accounts.stsol_reserve_account,
        accounts.st_sol_mint,
    )?;

    // Create and initialize an UST SPL token account for the reserve
    let ust_reserve_account_seeds = [
        anker_address.as_ref(),
        ANKER_UST_RESERVE_ACCOUNT,
        &[ust_reserve_account_bump_seed],
    ];
    create_account(
        &spl_token::ID,
        &accounts,
        accounts.ust_reserve_account,
        &rent,
        spl_token::state::Account::LEN,
        &ust_reserve_account_seeds,
    )?;
    initialize_spl_account(
        &accounts,
        &ust_reserve_account_seeds,
        accounts.ust_reserve_account,
        accounts.ust_mint,
    )?;

    let (_, token_swap_bump_seed) =
        Pubkey::find_program_address(&[&accounts.pool.key.to_bytes()], &spl_token_swap::id());

    let anker = Anker {
        b_sol_mint: *accounts.b_sol_mint.key,
        solido_program_id: *accounts.solido_program.key,
        solido: *accounts.solido.key,
        pool: *accounts.pool.key,
        rewards_destination: *accounts.rewards_destination.key,
        self_bump_seed: anker_bump_seed,
        mint_authority_bump_seed: mint_bump_seed,
        reserve_authority_bump_seed,
        stsol_reserve_account_bump_seed,
        ust_reserve_account_bump_seed,
        token_swap_bump_seed,
    };

    anker.check_mint(accounts.b_sol_mint)?;
    anker.check_stsol_reserve_address(
        program_id,
        &anker_address,
        accounts.stsol_reserve_account,
    )?;
    anker.check_ust_reserve_address(program_id, &anker_address, accounts.ust_reserve_account)?;
    anker.check_reserve_authority(program_id, &anker_address, accounts.reserve_authority)?;
    anker.check_is_st_sol_account(&solido, accounts.stsol_reserve_account)?;

    match spl_token::state::Mint::unpack_from_slice(&accounts.b_sol_mint.data.borrow()) {
        Ok(mint) if mint.mint_authority == COption::Some(mint_authority) => {
            // Ok, we control this mint.
        }
        _ => {
            msg!(
                "Mint authority of bSOL mint {} is not the expected {}.",
                accounts.b_sol_mint.key,
                mint_authority,
            );
            return Err(AnkerError::InvalidTokenMint.into());
        }
    }

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

    let (solido, anker) = deserialize_anker(
        program_id,
        accounts.anker,
        accounts.solido,
        accounts.to_reserve_account,
    )?;

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

    // Use Lido's exchange rate (`sol_balance / sol_supply`) to compute the
    // amount of BLamports to mint.
    let sol_value = solido.exchange_rate.exchange_st_sol(amount)?;
    let b_sol_amount = BLamports(sol_value.0);

    mint_b_sol_to(
        program_id,
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

/// Sell Anker rewards.
fn process_sell_rewards(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = SellRewardsAccountsInfo::try_from_slice(accounts_raw)?;
    let (lido, anker) = deserialize_anker(
        program_id,
        accounts.anker,
        accounts.solido,
        accounts.stsol_reserve_account,
    )?;
    anker.check_mint(accounts.b_sol_mint)?;

    let token_mint_state =
        spl_token::state::Mint::unpack_from_slice(&accounts.b_sol_mint.data.borrow())?;
    let b_sol_supply = token_mint_state.supply;

    let st_sol_reserve_state = spl_token::state::Account::unpack_from_slice(
        &accounts.stsol_reserve_account.data.borrow(),
    )?;
    let reserve_st_sol = StLamports(st_sol_reserve_state.amount);

    // Get StLamports corresponding to the amount of b_sol minted.
    let st_sol_amount = lido.exchange_rate.exchange_sol(Lamports(b_sol_supply))?;

    // If `reserve_st_sol` < `st_sol_amount` something went wrong, and we abort the transaction.
    let rewards = (reserve_st_sol - st_sol_amount)?;
    swap_rewards(program_id, rewards, &anker, &accounts)
}

/// Processes [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    let instruction = AnkerInstruction::try_from_slice(input)?;
    match instruction {
        AnkerInstruction::Initialize => process_initialize(program_id, accounts),
        AnkerInstruction::Deposit { amount } => process_deposit(program_id, accounts, amount),
        AnkerInstruction::Withdraw { amount } => todo!("{}", amount),
        AnkerInstruction::SellRewards => process_sell_rewards(program_id, accounts),
    }
}
