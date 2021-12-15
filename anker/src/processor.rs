// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use borsh::BorshDeserialize;
use lido::token::Lamports;
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_option::COption,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};

use lido::{state::Lido, token::StLamports};

use crate::{
    error::AnkerError,
    find_instance_address, find_mint_authority, find_reserve_authority,
    find_st_sol_reserve_account,
    instruction::{
        AnkerInstruction, ChangeTerraRewardsDestinationAccountsInfo,
        ChangeTokenSwapPoolAccountsInfo, DepositAccountsInfo, InitializeAccountsInfo,
        SellRewardsAccountsInfo, SendRewardsAccountsInfo, WithdrawAccountsInfo,
    },
    logic::{burn_b_sol, deserialize_anker, mint_b_sol_to},
    metrics::Metrics,
    state::{Anker, WormholeParameters},
    token::{BLamports, MicroUst},
    wormhole::{get_wormhole_transfer_instruction, TerraAddress},
};
use crate::{find_ust_reserve_account, ANKER_STSOL_RESERVE_ACCOUNT, ANKER_UST_RESERVE_ACCOUNT};
use crate::{
    logic::{create_account, initialize_spl_account, swap_rewards},
    state::ExchangeRate,
};
use crate::{state::ANKER_LEN, ANKER_RESERVE_AUTHORITY};

fn process_initialize(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
    terra_rewards_destination: TerraAddress,
) -> ProgramResult {
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
    let (_reserve_account, st_sol_reserve_account_bump_seed) =
        find_st_sol_reserve_account(program_id, &anker_address);
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
    let st_sol_reserve_account_seeds = [
        anker_address.as_ref(),
        ANKER_STSOL_RESERVE_ACCOUNT,
        &[st_sol_reserve_account_bump_seed],
    ];
    create_account(
        &spl_token::ID,
        &accounts,
        accounts.st_sol_reserve_account,
        &rent,
        spl_token::state::Account::LEN,
        &st_sol_reserve_account_seeds,
    )?;
    initialize_spl_account(
        &accounts,
        &st_sol_reserve_account_seeds,
        accounts.st_sol_reserve_account,
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

    let anker = Anker {
        b_sol_mint: *accounts.b_sol_mint.key,
        solido_program_id: *accounts.solido_program.key,
        solido: *accounts.solido.key,
        token_swap_pool: *accounts.token_swap_pool.key,
        terra_rewards_destination,
        wormhole_parameters: WormholeParameters {
            core_bridge_program_id: *accounts.wormhole_core_bridge_program_id.key,
            token_bridge_program_id: *accounts.wormhole_token_bridge_program_id.key,
        },
        metrics: Metrics::new(),
        self_bump_seed: anker_bump_seed,
        mint_authority_bump_seed: mint_bump_seed,
        reserve_authority_bump_seed,
        st_sol_reserve_account_bump_seed,
        ust_reserve_account_bump_seed,
    };

    anker.check_mint(accounts.b_sol_mint)?;
    anker.check_st_sol_reserve_address(
        program_id,
        &anker_address,
        accounts.st_sol_reserve_account,
    )?;
    anker.check_ust_reserve_address(program_id, &anker_address, accounts.ust_reserve_account)?;
    anker.check_reserve_authority(program_id, &anker_address, accounts.reserve_authority)?;
    anker.check_is_st_sol_account(&solido, accounts.st_sol_reserve_account)?;

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

    let (solido, anker) = deserialize_anker(program_id, accounts.anker, accounts.solido)?;
    anker.check_st_sol_reserve_address(
        program_id,
        accounts.anker.key,
        accounts.to_reserve_account,
    )?;
    anker.check_is_st_sol_account(&solido, accounts.to_reserve_account)?;

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
    let exchange_rate = ExchangeRate::from_solido_pegged(&solido);
    let b_sol_amount = exchange_rate.exchange_st_sol(amount)?;

    mint_b_sol_to(program_id, &anker, &accounts, b_sol_amount)?;

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
    let (solido, mut anker) = deserialize_anker(program_id, accounts.anker, accounts.solido)?;
    anker.check_st_sol_reserve_address(
        program_id,
        accounts.anker.key,
        accounts.st_sol_reserve_account,
    )?;
    anker.check_is_st_sol_account(&solido, accounts.st_sol_reserve_account)?;
    anker.check_mint(accounts.b_sol_mint)?;

    let token_mint_state =
        spl_token::state::Mint::unpack_from_slice(&accounts.b_sol_mint.data.borrow())?;
    let b_sol_supply = token_mint_state.supply;

    let reserve_st_sol = StLamports(Anker::get_token_amount(accounts.st_sol_reserve_account)?);

    // Get StLamports corresponding to the amount of b_sol minted.
    let st_sol_amount = solido.exchange_rate.exchange_sol(Lamports(b_sol_supply))?;

    // If `reserve_st_sol` < `st_sol_amount` something went wrong, and we abort the transaction.
    let rewards = (reserve_st_sol - st_sol_amount)?;

    // Get the amount of UST that we had.
    let ust_before = MicroUst(Anker::get_token_amount(accounts.ust_reserve_account)?);
    swap_rewards(program_id, rewards, &anker, &accounts)?;
    // Get new UST amount.
    let ust_after = MicroUst(Anker::get_token_amount(accounts.ust_reserve_account)?);
    let swapped_ust = (ust_after - ust_before)?;

    msg!("Swapped {} for {}.", st_sol_amount, swapped_ust);

    anker
        .metrics
        .observe_token_swap(st_sol_amount, swapped_ust)?;
    anker.save(accounts.anker)
}

/// Return some bSOL and get back the underlying stSOL.
fn process_withdraw(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
    amount: BLamports,
) -> ProgramResult {
    let accounts = WithdrawAccountsInfo::try_from_slice(accounts_raw)?;

    let (solido, anker) = deserialize_anker(program_id, accounts.anker, accounts.solido)?;
    anker.check_is_st_sol_account(&solido, accounts.reserve_account)?;
    anker.check_mint(accounts.b_sol_mint)?;

    anker.check_mint(accounts.b_sol_mint)?;
    anker.check_reserve_authority(program_id, accounts.anker.key, accounts.reserve_authority)?;

    let mint = match spl_token::state::Mint::unpack_from_slice(&accounts.b_sol_mint.data.borrow()) {
        Ok(mint) => mint,
        _ => {
            msg!("Failed to read the bSOL mint.");
            return Err(AnkerError::InvalidTokenMint.into());
        }
    };

    let reserve =
        match spl_token::state::Account::unpack_from_slice(&accounts.reserve_account.data.borrow())
        {
            Ok(reserve) => reserve,
            _ => {
                msg!("Failed to read the reserve stSOL account.");
                return Err(AnkerError::InvalidReserveAccount.into());
            }
        };

    let b_sol_supply = BLamports(mint.supply);
    let reserve_balance = StLamports(reserve.amount);

    // We have two ways of computing the exchange rate:
    //
    // 1. The inverse exchange rate of what Solido uses.
    // 2. Based on the bSOL supply and stSOL reserve.
    //
    // Option 1 enforces a 1 bSOL = 1 SOL peg, but if for some reason the value
    // of stSOL drops (which is impossible at the time of writing because there
    // is no slashing on Solana, but Solana might introduce this in the future
    // when we are in no position to upgrade this program quickly, so we want to
    // be prepared), then there may not be enough stSOL in the reserve to cover
    // all existing bSOL at a 1 bSOL = 1 SOL rate. This is where the Anker
    // exchange rate comes in: we treat 1 bSOL as a share of 1/supply of the
    // reserve. This ensures that all stSOL can be withdrawn, and it socializes
    // the loss among withdrawers until the 1 bSOL = 1 SOL peg is restored.
    let exchange_rate_solido = ExchangeRate::from_solido_pegged(&solido);
    let exchange_rate_anker = ExchangeRate::from_anker_unpegged(b_sol_supply, reserve_balance);
    let st_sol_solido = exchange_rate_solido.exchange_b_sol(amount)?;
    let st_sol_anker = exchange_rate_anker.exchange_b_sol(amount)?;
    let st_sol_amount = std::cmp::min(st_sol_solido, st_sol_anker);

    // Transfer the stSOL back to the user.
    let reserve_seeds = [
        accounts.anker.key.as_ref(),
        ANKER_RESERVE_AUTHORITY,
        &[anker.reserve_authority_bump_seed],
    ];
    invoke_signed(
        &spl_token::instruction::transfer(
            &spl_token::id(),
            accounts.reserve_account.key,
            accounts.to_st_sol_account.key,
            accounts.reserve_authority.key,
            &[],
            st_sol_amount.0,
        )?,
        &[
            accounts.reserve_account.clone(),
            accounts.to_st_sol_account.clone(),
            accounts.reserve_authority.clone(),
            accounts.spl_token.clone(),
        ],
        &[&reserve_seeds[..]],
    )?;

    burn_b_sol(
        &anker,
        accounts.spl_token,
        accounts.b_sol_mint,
        accounts.from_b_sol_account,
        accounts.from_b_sol_authority,
        amount,
    )?;

    msg!("Anker: Withdrew {} for {}.", amount, st_sol_amount,);

    Ok(())
}

/// Change the Terra rewards destination.
/// Solido's manager needs to sign the transaction.
fn process_change_terra_rewards_destination(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
    terra_rewards_destination: TerraAddress,
) -> ProgramResult {
    let accounts = ChangeTerraRewardsDestinationAccountsInfo::try_from_slice(accounts_raw)?;
    let (solido, mut anker) = deserialize_anker(program_id, accounts.anker, accounts.solido)?;
    solido.check_manager(accounts.manager)?;

    anker.terra_rewards_destination = terra_rewards_destination;
    anker.save(accounts.anker)
}

/// Change the Token Pool instance.
/// Solido's manager needs to sign the transaction.
fn process_change_token_swap_pool(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = ChangeTokenSwapPoolAccountsInfo::try_from_slice(accounts_raw)?;
    let (solido, mut anker) = deserialize_anker(program_id, accounts.anker, accounts.solido)?;
    solido.check_manager(accounts.manager)?;

    let current_token_swap = anker.get_token_swap_instance(accounts.current_token_swap_pool)?;
    // `get_token_swap_instance` compares the account to the one stored in
    // `anker.token_swap_pool`. We assign first so we have the correct value to
    // compare. If the check fails, the transaction will revert.
    anker.token_swap_pool = *accounts.new_token_swap_pool.key;
    let new_token_swap = anker.get_token_swap_instance(accounts.new_token_swap_pool)?;

    anker.check_change_token_swap_pool(&solido, current_token_swap, new_token_swap)?;
    anker.save(accounts.anker)
}

/// Send rewards via Wormhole from the UST reserve address to Terra.
fn process_send_rewards(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
    wormhole_nonce: u32,
) -> ProgramResult {
    let accounts = SendRewardsAccountsInfo::try_from_slice(accounts_raw)?;
    let (_solido, anker) = deserialize_anker(program_id, accounts.anker, accounts.solido)?;
    anker.check_ust_reserve_address(
        program_id,
        accounts.anker.key,
        accounts.ust_reserve_account,
    )?;
    let wormhole_transfer_args = anker.check_send_rewards(&accounts)?;
    let ust_reserve_state =
        spl_token::state::Account::unpack_from_slice(&accounts.ust_reserve_account.data.borrow())?;
    // Check UST mint.
    if &ust_reserve_state.mint != accounts.ust_mint.key {
        return Err(AnkerError::InvalidTokenMint.into());
    }

    let reserve_ust_amount = MicroUst(ust_reserve_state.amount);
    let payload = crate::wormhole::Payload::new(
        wormhole_nonce,
        reserve_ust_amount,
        anker.terra_rewards_destination.to_foreign(),
    );

    // Send UST tokens via Wormhole 🤞.
    let reserve_seeds = [
        accounts.anker.key.as_ref(),
        ANKER_RESERVE_AUTHORITY,
        &[anker.reserve_authority_bump_seed],
    ];

    // The "authority signer key" is the authority that can approve SPL token
    // transfers out of the "from" account and into Wormhole's custody account.
    // If you use SPL token's "delegate"/"approve" feature, then this is the key
    // to which you delegated, but it can also be the owner of the SPL token
    // account directly, which is the reserve authority in our case.
    let authority_signer_key = accounts.reserve_authority.clone();

    // For the order and meaning of the accounts, see also
    // https://github.com/certusone/wormhole/blob/537d56b37aa041a585f2c90515fa3a7ffa5898b5/solana/modules/token_bridge/program/src/instructions.rs#L328-L390.
    invoke_signed(
        &get_wormhole_transfer_instruction(&payload, &wormhole_transfer_args),
        &vec![
            accounts.payer.clone(),
            accounts.config_key.clone(),
            accounts.ust_reserve_account.clone(),
            accounts.ust_mint.clone(),
            accounts.custody_key.clone(),
            authority_signer_key,
            accounts.custody_signer_key.clone(),
            accounts.bridge_config.clone(),
            accounts.message.clone(),
            accounts.emitter_key.clone(),
            accounts.sequence_key.clone(),
            accounts.fee_collector_key.clone(),
            accounts.sysvar_clock.clone(),
            accounts.sysvar_rent.clone(),
            accounts.system_program.clone(),
            accounts.wormhole_core_bridge_program_id.clone(),
            accounts.spl_token.clone(),
        ],
        &[&reserve_seeds[..]],
    )
}

/// Processes [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    let instruction = AnkerInstruction::try_from_slice(input)?;
    match instruction {
        AnkerInstruction::Initialize {
            terra_rewards_destination,
        } => process_initialize(program_id, accounts, terra_rewards_destination),
        AnkerInstruction::Deposit { amount } => process_deposit(program_id, accounts, amount),
        AnkerInstruction::Withdraw { amount } => process_withdraw(program_id, accounts, amount),
        AnkerInstruction::SellRewards => process_sell_rewards(program_id, accounts),
        AnkerInstruction::ChangeTerraRewardsDestination {
            terra_rewards_destination,
        } => process_change_terra_rewards_destination(
            program_id,
            accounts,
            terra_rewards_destination,
        ),
        AnkerInstruction::ChangeTokenSwapPool => {
            process_change_token_swap_pool(program_id, accounts)
        }
        AnkerInstruction::SendRewards { wormhole_nonce } => {
            process_send_rewards(program_id, accounts, wormhole_nonce)
        }
    }
}
