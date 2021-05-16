use lido::{state::FeeDistribution, DEPOSIT_AUTHORITY, FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY};
use solana_program::{
    borsh::get_packed_len, native_token::Sol, program_pack::Pack, pubkey::Pubkey,
    system_instruction,
};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use spl_stake_pool::state::Fee;

use crate::{stake_pool_helpers::command_create_pool, CommandResult, Config, Error};

pub(crate) fn check_fee_payer_balance(config: &Config, required_balance: u64) -> Result<(), Error> {
    let balance = config.rpc_client.get_balance(&config.fee_payer.pubkey())?;
    if balance < required_balance {
        Err(format!(
            "Fee payer, {}, has insufficient balance: {} required, {} available",
            config.fee_payer.pubkey(),
            Sol(required_balance),
            Sol(balance)
        )
        .into())
    } else {
        Ok(())
    }
}

pub(crate) fn send_transaction(
    config: &Config,
    transaction: Transaction,
) -> solana_client::client_error::Result<()> {
    if config.dry_run {
        let result = config.rpc_client.simulate_transaction(&transaction)?;
        println!("Simulate result: {:?}", result);
    } else {
        let signature = config
            .rpc_client
            .send_and_confirm_transaction_with_spinner(&transaction)?;
        println!("Signature: {}", signature);
    }
    Ok(())
}

pub(crate) struct NewStakePoolArgs {
    pub(crate) keypair: Keypair,
    pub(crate) numerator: u64,
    pub(crate) denominator: u64,
    pub(crate) max_validators: u32,
}
pub(crate) enum StakePoolArgs {
    New(NewStakePoolArgs),
    Existing(Pubkey),
}

pub(crate) fn command_create_solido(
    config: &Config,
    stake_pool_args: StakePoolArgs,
) -> CommandResult {
    let lido_keypair = Keypair::new();
    let manager_keypair = Keypair::new();
    let fee_distribution_keypair = Keypair::new();
    let validator_credit_keypair = Keypair::new();
    let insurance_keypair = Keypair::new();
    let treasury_keypair = Keypair::new();
    let manager_fee_keypair = Keypair::new();

    let fee_structure = FeeDistribution {
        insurance_fee_numerator: 2,
        treasury_fee_numerator: 2,
        validators_fee_numerator: 2,
        manager_fee_numerator: 4,
        denominator: 10,
        insurance_account: insurance_keypair.pubkey(),
        treasury_account: treasury_keypair.pubkey(),
        manager_account: manager_fee_keypair.pubkey(),
    };

    println!("Creating lido {}", lido_keypair.pubkey());

    let (reserve_authority, _) = lido::find_authority_program_address(
        &lido::id(),
        &lido_keypair.pubkey(),
        RESERVE_AUTHORITY,
    );

    let (fee_authority, _) = lido::find_authority_program_address(
        &lido::id(),
        &lido_keypair.pubkey(),
        FEE_MANAGER_AUTHORITY,
    );

    let stake_pool_pubkey = match stake_pool_args {
        StakePoolArgs::New(NewStakePoolArgs {
            keypair,
            numerator,
            denominator,
            max_validators,
        }) => {
            let (deposit_authority, _) = lido::find_authority_program_address(
                &lido::id(),
                &lido_keypair.pubkey(),
                DEPOSIT_AUTHORITY,
            );
            let stake_pool_public_key = keypair.pubkey();
            command_create_pool(
                &config,
                &deposit_authority,
                Fee {
                    denominator,
                    numerator,
                },
                max_validators,
                Some(keypair),
                None,
            )?;
            stake_pool_public_key
        }
        StakePoolArgs::Existing(stake_pool_pubkey) => stake_pool_pubkey,
    };

    let mint_keypair = Keypair::new();
    println!("Creating mint {}", mint_keypair.pubkey());
    let fee_keypair = Keypair::new();
    println!("Creating fee account {}", fee_keypair.pubkey());

    let pool_token_to = Keypair::new();
    println!(
        "Creating lido's account in stake pool: {}",
        pool_token_to.pubkey()
    );

    let mint_account_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?;
    let lido_size = get_packed_len::<lido::state::Lido>();
    let lido_account_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(lido_size)?;

    let fee_token_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)?;
    let pool_token_balance = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)?;

    let total_rent_free_balances =
        mint_account_balance + lido_account_balance + fee_token_balance + pool_token_balance;

    let default_decimals = spl_token::native_mint::DECIMALS;
    let mut lido_transaction = Transaction::new_with_payer(
        &[
            // Account for lido lsol mint
            system_instruction::create_account(
                &config.fee_payer.pubkey(),
                &mint_keypair.pubkey(),
                mint_account_balance,
                spl_token::state::Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &mint_keypair.pubkey(),
                &reserve_authority,
                None,
                default_decimals,
            )?,
            system_instruction::create_account(
                &config.fee_payer.pubkey(),
                &lido_keypair.pubkey(),
                lido_account_balance,
                lido_size as u64,
                &lido::id(),
            ),
            // Account for the pool fee accumulation
            system_instruction::create_account(
                &config.fee_payer.pubkey(),
                &fee_keypair.pubkey(),
                fee_token_balance,
                spl_token::state::Account::LEN as u64,
                &spl_token::id(),
            ),
            // Initialize fee receiver account
            spl_token::instruction::initialize_account(
                &spl_token::id(),
                &fee_keypair.pubkey(),
                &mint_keypair.pubkey(),
                &fee_authority,
            )?,
            lido::instruction::initialize(
                &lido::id(),
                fee_structure,
                10_000,
                &lido::instruction::InitializeAccountsMeta {
                    lido: lido_keypair.pubkey(),
                    stake_pool: stake_pool_pubkey,
                    manager: manager_keypair.pubkey(),
                    mint_program: mint_keypair.pubkey(),
                    pool_token_to: pool_token_to.pubkey(), // to define
                    fee_token: fee_keypair.pubkey(),
                    fee_distribution: fee_distribution_keypair.pubkey(),
                    validator_credit_accounts: validator_credit_keypair.pubkey(),
                    insurance_account: insurance_keypair.pubkey(),
                    treasury_account: treasury_keypair.pubkey(),
                    manager_fee_account: manager_fee_keypair.pubkey(),
                },
            )?,
        ],
        Some(&config.fee_payer.pubkey()),
    );

    let (recent_blockhash, fee_calculator) = config.rpc_client.get_recent_blockhash()?;
    check_fee_payer_balance(
        config,
        total_rent_free_balances + fee_calculator.calculate_fee(&lido_transaction.message()),
    )?;
    let signers = vec![config.fee_payer.as_ref(), &mint_keypair, &lido_keypair];
    lido_transaction.sign(&signers, recent_blockhash);
    send_transaction(&config, lido_transaction)?;

    Ok(())
}
