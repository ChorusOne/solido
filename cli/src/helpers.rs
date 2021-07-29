// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::fmt;

use serde::Serialize;
use solana_program::{pubkey::Pubkey, system_instruction};
use solana_sdk::{
    account::ReadableAccount,
    signature::{Keypair, Signer},
};

use lido::{
    balance::get_validator_to_withdraw,
    metrics::LamportsHistogram,
    state::{Lido, RewardDistribution, Validator},
    token::{Lamports, StLamports},
    util::serialize_b58,
    MINT_AUTHORITY, RESERVE_ACCOUNT,
};

use crate::{
    config::{
        AddRemoveMaintainerOpts, AddValidatorOpts, CreateSolidoOpts, DepositOpts, ShowSolidoOpts,
        WithdrawOpts,
    },
    error::MaintenanceError,
};
use crate::{
    multisig::{get_multisig_program_address, propose_instruction, ProposeInstructionOutput},
    snapshot::Result,
    spl_token_utils::{push_create_spl_token_account, push_create_spl_token_mint},
    SnapshotClientConfig, SnapshotConfig,
};

#[derive(Serialize)]
pub struct CreateSolidoOutput {
    /// Account that stores the data for this Solido instance.
    #[serde(serialize_with = "serialize_b58")]
    pub solido_address: Pubkey,

    /// Manages the deposited sol.
    #[serde(serialize_with = "serialize_b58")]
    pub reserve_account: Pubkey,

    /// SPL token mint account for StSol tokens.
    #[serde(serialize_with = "serialize_b58")]
    pub st_sol_mint_address: Pubkey,

    /// stSOL SPL token account that holds the treasury funds.
    #[serde(serialize_with = "serialize_b58")]
    pub treasury_account: Pubkey,

    /// stSOL SPL token account that receives the developer fees.
    #[serde(serialize_with = "serialize_b58")]
    pub developer_account: Pubkey,

    /// Authority for the minting.
    #[serde(serialize_with = "serialize_b58")]
    pub mint_authority: Pubkey,
}

impl fmt::Display for CreateSolidoOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Solido details:")?;
        writeln!(
            f,
            "  Solido address:                {}",
            self.solido_address
        )?;
        writeln!(
            f,
            "  Reserve account:               {}",
            self.reserve_account
        )?;
        writeln!(
            f,
            "  Mint authority:                {}",
            self.mint_authority
        )?;
        writeln!(
            f,
            "  stSOL mint:                    {}",
            self.st_sol_mint_address
        )?;
        writeln!(
            f,
            "  Treasury SPL token account:    {}",
            self.treasury_account
        )?;
        writeln!(
            f,
            "  Developer fee SPL token account: {}",
            self.developer_account
        )?;
        Ok(())
    }
}

pub fn command_create_solido(
    config: &mut SnapshotConfig,
    opts: &CreateSolidoOpts,
) -> Result<CreateSolidoOutput> {
    let lido_keypair = Keypair::new();

    let (reserve_account, _) = lido::find_authority_program_address(
        opts.solido_program_id(),
        &lido_keypair.pubkey(),
        RESERVE_ACCOUNT,
    );

    let (mint_authority, _) = lido::find_authority_program_address(
        opts.solido_program_id(),
        &lido_keypair.pubkey(),
        MINT_AUTHORITY,
    );

    let (manager, _nonce) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());

    let lido_size = Lido::calculate_size(*opts.max_validators(), *opts.max_maintainers());
    let lido_account_balance = config
        .client
        .get_minimum_balance_for_rent_exemption(lido_size)?;

    let mut instructions = Vec::new();

    // We need to fund Lido's reserve account so it is rent-exempt, otherwise it
    // might disappear.
    let min_balance_empty_data_account = config.client.get_minimum_balance_for_rent_exemption(0)?;
    instructions.push(system_instruction::transfer(
        &config.signer.pubkey(),
        &reserve_account,
        min_balance_empty_data_account.0,
    ));

    // Set up the Lido stSOL SPL token mint account.
    let st_sol_mint_keypair =
        push_create_spl_token_mint(config, &mut instructions, &mint_authority)?;

    // Ideally we would set up the entire instance in a single transaction, but
    // Solana transaction size limits are so low that we need to break our
    // instructions down into multiple transactions. So set up the mint first,
    // then continue.
    let signers = &[&st_sol_mint_keypair, config.signer];
    config.sign_and_send_transaction(&instructions[..], signers)?;
    instructions.clear();
    eprintln!("Did send mint init.");

    // Set up the SPL token account that receive the fees in stSOL.
    let treasury_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        opts.treasury_account_owner(),
    )?;
    let developer_keypair = push_create_spl_token_account(
        config,
        &mut instructions,
        &st_sol_mint_keypair.pubkey(),
        opts.developer_account_owner(),
    )?;
    config.sign_and_send_transaction(
        &instructions[..],
        &vec![config.signer, &treasury_keypair, &developer_keypair],
    )?;
    instructions.clear();
    eprintln!("Did send SPL account inits.");

    // Create the account that holds the Solido instance itself.
    instructions.push(system_instruction::create_account(
        &config.signer.pubkey(),
        &lido_keypair.pubkey(),
        lido_account_balance.0,
        lido_size as u64,
        opts.solido_program_id(),
    ));

    instructions.push(lido::instruction::initialize(
        opts.solido_program_id(),
        RewardDistribution {
            treasury_fee: *opts.treasury_fee_share(),
            validation_fee: *opts.validation_fee_share(),
            developer_fee: *opts.developer_fee_share(),
            st_sol_appreciation: *opts.st_sol_appreciation_share(),
        },
        *opts.max_validators(),
        *opts.max_maintainers(),
        &lido::instruction::InitializeAccountsMeta {
            lido: lido_keypair.pubkey(),
            st_sol_mint: st_sol_mint_keypair.pubkey(),
            manager,
            treasury_account: treasury_keypair.pubkey(),
            developer_account: developer_keypair.pubkey(),
            reserve_account,
        },
    ));

    config.sign_and_send_transaction(&instructions[..], &[config.signer, &lido_keypair])?;
    eprintln!("Did send Lido init.");

    let result = CreateSolidoOutput {
        solido_address: lido_keypair.pubkey(),
        reserve_account,
        mint_authority,
        st_sol_mint_address: st_sol_mint_keypair.pubkey(),
        treasury_account: treasury_keypair.pubkey(),
        developer_account: developer_keypair.pubkey(),
    };
    Ok(result)
}

/// Command to add a validator to Solido.
pub fn command_add_validator(
    config: &mut SnapshotConfig,
    opts: &AddValidatorOpts,
) -> Result<ProposeInstructionOutput> {
    let (multisig_address, _) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());

    let instruction = lido::instruction::add_validator(
        opts.solido_program_id(),
        opts.weight().clone(),
        &lido::instruction::AddValidatorMeta {
            lido: *opts.solido_address(),
            manager: multisig_address,
            validator_vote_account: *opts.validator_vote_account(),
            validator_fee_st_sol_account: *opts.validator_fee_account(),
        },
    );
    propose_instruction(
        config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        instruction,
    )
}

/// Command to add a validator to Solido.
pub fn command_add_maintainer(
    config: &mut SnapshotConfig,
    opts: &AddRemoveMaintainerOpts,
) -> Result<ProposeInstructionOutput> {
    let (multisig_address, _) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());
    let instruction = lido::instruction::add_maintainer(
        opts.solido_program_id(),
        &lido::instruction::AddMaintainerMeta {
            lido: *opts.solido_address(),
            manager: multisig_address,
            maintainer: *opts.maintainer_address(),
        },
    );
    propose_instruction(
        config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        instruction,
    )
}

/// Command to add a validator to Solido.
pub fn command_remove_maintainer(
    config: &mut SnapshotConfig,
    opts: &AddRemoveMaintainerOpts,
) -> Result<ProposeInstructionOutput> {
    let (multisig_address, _) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());
    let instruction = lido::instruction::remove_maintainer(
        opts.solido_program_id(),
        &lido::instruction::RemoveMaintainerMeta {
            lido: *opts.solido_address(),
            manager: multisig_address,
            maintainer: *opts.maintainer_address(),
        },
    );
    propose_instruction(
        config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        instruction,
    )
}

#[derive(Serialize)]
pub struct ShowSolidoOutput {
    pub solido: Lido,

    #[serde(serialize_with = "serialize_b58")]
    pub solido_program_id: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub solido_address: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub reserve_account: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub stake_authority: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub mint_authority: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    pub rewards_withdraw_authority: Pubkey,
}

impl fmt::Display for ShowSolidoOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Manager:                     {}", self.solido.manager)?;
        writeln!(
            f,
            "stSOL mint:                  {}",
            self.solido.st_sol_mint
        )?;

        writeln!(f, "\nExchange rate:")?;
        writeln!(
            f,
            "  Computed in epoch: {}",
            self.solido.exchange_rate.computed_in_epoch
        )?;
        writeln!(
            f,
            "  SOL balance:       {}",
            self.solido.exchange_rate.sol_balance
        )?;
        writeln!(
            f,
            "  stSOL supply:      {}",
            self.solido.exchange_rate.st_sol_supply
        )?;

        writeln!(f, "\nAuthorities (public key, bump seed):")?;
        writeln!(
            f,
            "Stake authority:            {}, {}",
            self.stake_authority, self.solido.stake_authority_bump_seed
        )?;
        writeln!(
            f,
            "Mint authority:             {}, {}",
            self.mint_authority, self.solido.mint_authority_bump_seed
        )?;
        writeln!(
            f,
            "Rewards withdraw authority: {}, {}",
            self.rewards_withdraw_authority, self.solido.rewards_withdraw_authority_bump_seed,
        )?;
        writeln!(
            f,
            "Reserve:                    {}, {}",
            self.reserve_account, self.solido.sol_reserve_account_bump_seed
        )?;
        writeln!(f, "\nReward distribution:")?;
        let mut print_reward = |name, get: fn(&RewardDistribution) -> u32| {
            writeln!(
                f,
                "  {:4}/{:4} => {}",
                get(&self.solido.reward_distribution),
                self.solido.reward_distribution.sum(),
                name,
            )
        };
        print_reward("stSOL appreciation", |d| d.st_sol_appreciation)?;
        print_reward("Treasury", |d| d.treasury_fee)?;
        print_reward("Validation fee", |d| d.validation_fee)?;
        print_reward("Developer fee", |d| d.developer_fee)?;

        writeln!(f, "\nFee recipients:")?;
        writeln!(
            f,
            "Treasury SPL token account:      {}",
            self.solido.fee_recipients.treasury_account
        )?;
        writeln!(
            f,
            "Developer fee SPL token account: {}",
            self.solido.fee_recipients.developer_account
        )?;

        writeln!(f, "\nMetrics:")?;
        writeln!(
            f,
            "  Total treasury fee:       {}, valued at {} when it was paid",
            self.solido.metrics.fee_treasury_st_sol_total,
            self.solido.metrics.fee_treasury_sol_total,
        )?;
        writeln!(
            f,
            "  Total validation fee:     {}, valued at {} when it was paid",
            self.solido.metrics.fee_validation_st_sol_total,
            self.solido.metrics.fee_validation_sol_total,
        )?;
        writeln!(
            f,
            "  Total developer fee:      {}, valued at {} when it was paid",
            self.solido.metrics.fee_developer_st_sol_total,
            self.solido.metrics.fee_developer_sol_total,
        )?;
        writeln!(
            f,
            "  Total stSOL appreciation: {}",
            self.solido.metrics.st_sol_appreciation_sol_total
        )?;
        writeln!(
            f,
            "  Total deposited:          {}",
            self.solido.metrics.deposit_amount.total
        )?;
        for (count, upper_bound) in self
            .solido
            .metrics
            .deposit_amount
            .counts
            .iter()
            .zip(&LamportsHistogram::BUCKET_UPPER_BOUNDS)
        {
            writeln!(f, "  Number of deposits ≤ {}: {}", upper_bound, count)?;
        }

        writeln!(
            f,
            "\nValidators: {} in use out of {} that the instance can support",
            self.solido.validators.len(),
            self.solido.validators.maximum_entries
        )?;
        for pe in &self.solido.validators.entries {
            writeln!(
                f,
                "\n  - \
                Vote account:  {}\n    \
                Fee address:   {}\n    \
                Unclaimed fee: {}\n    \
                Stake:         {}\n    \
                Stake accounts (seed, address):",
                pe.pubkey,
                pe.entry.fee_address,
                pe.entry.fee_credit,
                pe.entry.stake_accounts_balance,
            )?;
            for seed in pe.entry.stake_accounts_seed_begin..pe.entry.stake_accounts_seed_end {
                writeln!(
                    f,
                    "      - {}: {}",
                    seed,
                    pe.find_stake_account_address(
                        &self.solido_program_id,
                        &self.solido_address,
                        seed
                    )
                    .0
                )?;
            }
        }
        writeln!(
            f,
            "\nMaintainers: {} in use out of {} that the instance can support\n",
            self.solido.maintainers.len(),
            self.solido.maintainers.maximum_entries
        )?;
        for pe in &self.solido.maintainers.entries {
            writeln!(f, "  - {}", pe.pubkey)?;
        }
        Ok(())
    }
}

pub fn command_show_solido(
    config: &mut SnapshotConfig,
    opts: &ShowSolidoOpts,
) -> Result<ShowSolidoOutput> {
    let lido = config.client.get_solido(opts.solido_address())?;
    let reserve_account =
        lido.get_reserve_account(opts.solido_program_id(), opts.solido_address())?;
    let stake_authority =
        lido.get_stake_authority(opts.solido_program_id(), opts.solido_address())?;
    let mint_authority =
        lido.get_mint_authority(opts.solido_program_id(), opts.solido_address())?;
    let rewards_withdraw_authority =
        lido.get_rewards_withdraw_authority(opts.solido_program_id(), opts.solido_address())?;

    Ok(ShowSolidoOutput {
        solido_program_id: *opts.solido_program_id(),
        solido_address: *opts.solido_address(),
        solido: lido,
        reserve_account,
        stake_authority,
        mint_authority,
        rewards_withdraw_authority,
    })
}

#[derive(Serialize)]
pub struct DepositOutput {
    #[serde(serialize_with = "serialize_b58")]
    pub recipient: Pubkey,

    /// Amount of stSOL we expected to receive based on the exchange rate at the time of the deposit.
    ///
    /// This can differ from the actual amount, when a deposit happens close to
    /// an epoch boundary, and an `UpdateExchangeRate` instruction executed before
    /// our deposit, but after we checked the exchange rate.
    #[serde(rename = "expected_st_lamports")]
    pub expected_st_sol: StLamports,

    /// The difference in stSOL balance before and after our deposit.
    ///
    /// If no other transactions touch the recipient account, then this is the
    /// amount of stSOL we got. However, the stSOL account balance might change
    /// for other reasons than just the deposit, if another transaction touched
    /// the account in the same block.
    #[serde(rename = "st_lamports_balance_increase")]
    pub st_sol_balance_increase: StLamports,

    /// Whether we had to create the associated stSOL account. False if one existed already.
    pub created_associated_st_sol_account: bool,
}

impl fmt::Display for DepositOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.created_associated_st_sol_account {
            writeln!(f, "Created recipient stSOL account, it did not yet exist.")?;
        } else {
            writeln!(f, "Recipient stSOL account existed already before deposit.")?;
        }
        writeln!(f, "Recipient stSOL account: {}", self.recipient)?;
        writeln!(f, "Expected stSOL amount:   {}", self.expected_st_sol)?;
        writeln!(
            f,
            "stSOL balance increase:  {}",
            self.st_sol_balance_increase
        )?;
        Ok(())
    }
}

pub fn command_deposit(
    config: &mut SnapshotClientConfig,
    opts: &DepositOpts,
) -> std::result::Result<DepositOutput, crate::error::Error> {
    let (recipient, created_recipient) = config.with_snapshot(|config| {
        let solido = config.client.get_solido(opts.solido_address())?;

        let recipient = spl_associated_token_account::get_associated_token_address(
            &config.signer.pubkey(),
            &solido.st_sol_mint,
        );

        if !config.client.account_exists(&recipient)? {
            let instr = spl_associated_token_account::create_associated_token_account(
                &config.signer.pubkey(),
                &config.signer.pubkey(),
                &solido.st_sol_mint,
            );

            config.sign_and_send_transaction(&[instr], &[config.signer])?;

            Ok((recipient, true))
        } else {
            Ok((recipient, false))
        }
    })?;

    let (balance_before, exchange_rate) = config.with_snapshot(|config| {
        let balance_before = config
            .client
            .get_spl_token_balance(&recipient)
            .map(StLamports)?;
        let solido = config.client.get_solido(opts.solido_address())?;
        let reserve =
            solido.get_reserve_account(opts.solido_program_id(), opts.solido_address())?;
        let mint_authority =
            solido.get_mint_authority(opts.solido_program_id(), opts.solido_address())?;

        let instr = lido::instruction::deposit(
            opts.solido_program_id(),
            &lido::instruction::DepositAccountsMeta {
                lido: *opts.solido_address(),
                user: config.signer.pubkey(),
                recipient,
                st_sol_mint: solido.st_sol_mint,
                mint_authority,
                reserve_account: reserve,
            },
            *opts.amount_sol(),
        );

        config.sign_and_send_transaction(&[instr], &[config.signer])?;

        Ok((balance_before, solido.exchange_rate))
    })?;

    let balance_after = config.with_snapshot(|config| {
        config
            .client
            .get_spl_token_balance(&recipient)
            .map(StLamports)
    })?;

    let st_sol_balance_increase = StLamports(balance_after.0.saturating_sub(balance_before.0));
    let expected_st_sol = exchange_rate
        .exchange_sol(*opts.amount_sol())
        // If this is not an `Ok`, the transaction should have failed, but if
        // the transaction did not fail, then we do want to show the output; we
        // don't want the user to think that the deposit failed.
        .unwrap_or(StLamports(0));

    let result = DepositOutput {
        recipient,
        expected_st_sol,
        st_sol_balance_increase,
        created_associated_st_sol_account: created_recipient,
    };
    Ok(result)
}

#[derive(Serialize)]
pub struct WithdrawOutput {
    #[serde(serialize_with = "serialize_b58")]
    pub from_token_address: Pubkey,

    /// Amount of SOL that was withdrawn.
    pub withdrawn_sol: Lamports,

    /// Newly created stake account, where the source stake account will be
    /// split to.
    #[serde(serialize_with = "serialize_b58")]
    pub new_stake_account: Pubkey,
}

impl fmt::Display for WithdrawOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Withdrawn from:          {}", self.from_token_address)?;
        writeln!(f, "Total SOL withdrawn:     {}", self.withdrawn_sol)?;
        writeln!(f, "New stake account:       {}", self.new_stake_account)?;
        Ok(())
    }
}

pub fn command_withdraw(
    config: &mut SnapshotClientConfig,
    opts: &WithdrawOpts,
) -> std::result::Result<WithdrawOutput, crate::error::Error> {
    let (st_sol_address, new_stake_account) = config.with_snapshot(|config| {
        let solido = config.client.get_solido(opts.solido_address())?;

        let st_sol_address = spl_associated_token_account::get_associated_token_address(
            &config.signer.pubkey(),
            &solido.st_sol_mint,
        );

        let stake_authority =
            solido.get_stake_authority(opts.solido_program_id(), opts.solido_address())?;

        // Get heaviest validator.
        let heaviest_validator = get_validator_to_withdraw(&solido.validators).map_err(|_| {
            MaintenanceError::new(
                "The instance has no active validators to withdraw from.".to_owned(),
            )
        })?;

        let (stake_address, _bump_seed) = Validator::find_stake_account_address(
            opts.solido_program_id(),
            opts.solido_address(),
            &heaviest_validator.pubkey,
            heaviest_validator.entry.stake_accounts_seed_begin,
        );

        let destination_stake_account = Keypair::new();

        let instr = lido::instruction::withdraw(
            opts.solido_program_id(),
            &lido::instruction::WithdrawAccountsMeta {
                lido: *opts.solido_address(),
                st_sol_mint: solido.st_sol_mint,
                st_sol_account_owner: config.signer.pubkey(),
                st_sol_account: st_sol_address,
                validator_vote_account: heaviest_validator.pubkey,
                source_stake_account: stake_address,
                destination_stake_account: destination_stake_account.pubkey(),
                stake_authority,
            },
            *opts.amount_st_sol(),
        );
        config.sign_and_send_transaction(&[instr], &[config.signer, &destination_stake_account])?;

        Ok((st_sol_address, destination_stake_account))
    })?;

    let stake_sol = config.with_snapshot(|config| {
        let stake_account = config.client.get_account(&new_stake_account.pubkey())?;
        Ok(Lamports(stake_account.lamports()))
    })?;
    let result = WithdrawOutput {
        from_token_address: st_sol_address,
        withdrawn_sol: stake_sol,
        new_stake_account: new_stake_account.pubkey(),
    };
    Ok(result)
}
