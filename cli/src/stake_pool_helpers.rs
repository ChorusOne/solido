use solana_program::sysvar;
use std::fmt;
use {
    crate::helpers::{send_transaction, sign_and_send_transaction},
    crate::spl_token_utils::{push_create_spl_token_account, push_create_spl_token_mint},
    crate::util::PubkeyBase58,
    crate::Config,
    serde::Serialize,
    solana_program::{borsh::get_packed_len, pubkey::Pubkey},
    solana_sdk::{
        signature::{Keypair, Signer},
        system_instruction,
        transaction::Transaction,
    },
    spl_stake_pool::{
        self,
        borsh::get_instance_packed_len,
        find_withdraw_authority_program_address,
        stake_program::{self},
        state::{Fee, StakePool, ValidatorList},
    },
};

const STAKE_STATE_LEN: usize = 200;

#[derive(Serialize)]
pub struct CreatePoolOutput {
    /// Account that holds the stake pool data structure.
    pub stake_pool_address: PubkeyBase58,

    /// Reserve account holds Lamports used when increasing or decreasing the validator's stake.
    pub reserve_stake_address: PubkeyBase58,

    /// SPL token mint account for stake pool tokens.
    pub mint_address: PubkeyBase58,

    /// SPL token account that collected fees get deposited into, in stake pool tokens.
    pub fee_address: PubkeyBase58,

    /// Account that stores the validator list data structure.
    pub validator_list_address: PubkeyBase58,

    /// Program-derived account that can mint stake pool tokens.
    pub withdraw_authority: PubkeyBase58,
}

impl fmt::Display for CreatePoolOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // All output is indented by two spaces because we expect to call this
        // from `CreateSolidoOutput::fmt`.
        writeln!(f, "  Stake pool address:     {}", self.stake_pool_address)?;
        writeln!(
            f,
            "  Reserve stake address:  {}",
            self.reserve_stake_address
        )?;
        writeln!(
            f,
            "  Stake pool token mint:  {}",
            self.reserve_stake_address
        )?;
        writeln!(f, "  Fee deposit address:    {}", self.fee_address)?;
        writeln!(
            f,
            "  Validator list address: {}",
            self.validator_list_address
        )?;
        writeln!(f, "  Withdraw authority:     {}", self.withdraw_authority)?;
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn command_create_pool(
    config: &Config,
    stake_pool_program_id: &Pubkey,
    stake_pool_authority: &Pubkey,
    deposit_authority: &Pubkey,
    fee_authority: &Pubkey,
    manager: &Pubkey,
    fee: Fee,
    max_validators: u32,
) -> Result<CreatePoolOutput, crate::Error> {
    let reserve_stake = Keypair::new();
    let stake_pool_keypair = Keypair::new();
    let validator_list = Keypair::new();

    let reserve_stake_balance = config
        .rpc
        .get_minimum_balance_for_rent_exemption(STAKE_STATE_LEN)?
        + 1;
    let stake_pool_account_lamports = config
        .rpc
        .get_minimum_balance_for_rent_exemption(get_packed_len::<StakePool>())?;
    let empty_validator_list = ValidatorList::new(max_validators);
    let validator_list_size = get_instance_packed_len(&empty_validator_list)?;
    let validator_list_balance = config
        .rpc
        .get_minimum_balance_for_rent_exemption(validator_list_size)?;

    // Calculate withdraw authority used for minting pool tokens
    let (withdraw_authority, _) = find_withdraw_authority_program_address(
        stake_pool_program_id,
        &stake_pool_keypair.pubkey(),
    );

    let mut instructions = vec![
        // Account for the stake pool reserve
        system_instruction::create_account(
            &config.signer.pubkey(),
            &reserve_stake.pubkey(),
            reserve_stake_balance,
            STAKE_STATE_LEN as u64,
            &stake_program::id(),
        ),
        stake_program::initialize(
            &reserve_stake.pubkey(),
            &stake_program::Authorized {
                staker: withdraw_authority,
                withdrawer: withdraw_authority,
            },
            &stake_program::Lockup::default(),
        ),
    ];

    let mint_keypair = push_create_spl_token_mint(config, &mut instructions, &withdraw_authority)?;

    // Set up the SPL token account that will receive the fees.
    let pool_fee_account_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &mint_keypair.pubkey(),
        fee_authority,
    )?;
    sign_and_send_transaction(
        config,
        &instructions[..],
        &[
            config.signer,
            &reserve_stake,
            &mint_keypair,
            &pool_fee_account_keypair,
        ],
    );

    let mut initialize_transaction = Transaction::new_with_payer(
        &[
            // Validator stake account list storage
            system_instruction::create_account(
                &config.signer.pubkey(),
                &validator_list.pubkey(),
                validator_list_balance,
                validator_list_size as u64,
                stake_pool_program_id,
            ),
            // Account for the stake pool
            system_instruction::create_account(
                &config.signer.pubkey(),
                &stake_pool_keypair.pubkey(),
                stake_pool_account_lamports,
                get_packed_len::<StakePool>() as u64,
                stake_pool_program_id,
            ),
            // Initialize stake pool
            lido::instruction::initialize_stake_pool_with_authority(
                stake_pool_program_id,
                &lido::instruction::InitializeStakePoolWithAuthorityAccountsMeta {
                    stake_pool: stake_pool_keypair.pubkey(),
                    manager: *manager,
                    staker: *stake_pool_authority,
                    validator_list: validator_list.pubkey(),
                    reserve_stake: reserve_stake.pubkey(),
                    pool_mint: mint_keypair.pubkey(),
                    sysvar_clock: sysvar::clock::id(),
                    sysvar_rent: sysvar::rent::id(),
                    sysvar_token: spl_token::id(),
                    deposit_authority: *deposit_authority,
                    fee_account: pool_fee_account_keypair.pubkey(),
                },
                fee,
                max_validators,
            )?,
        ],
        Some(&config.signer.pubkey()),
    );

    let (recent_blockhash, _fee_calculator) = config.rpc.get_recent_blockhash()?;

    let initialize_signers = vec![config.signer, &stake_pool_keypair, &validator_list];
    initialize_transaction.sign(&initialize_signers, recent_blockhash);
    send_transaction(&config, initialize_transaction)?;

    let result = CreatePoolOutput {
        stake_pool_address: stake_pool_keypair.pubkey().into(),
        reserve_stake_address: reserve_stake.pubkey().into(),
        mint_address: mint_keypair.pubkey().into(),
        fee_address: pool_fee_account_keypair.pubkey().into(),
        validator_list_address: validator_list.pubkey().into(),
        withdraw_authority: withdraw_authority.into(),
    };
    Ok(result)
}
