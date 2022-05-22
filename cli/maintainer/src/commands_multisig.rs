// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use anchor_lang::prelude::{AccountMeta, ToAccountMetas};
use anchor_lang::{Discriminator, InstructionData};
use anker::instruction::{
    ChangeSellRewardsMinOutBpsAccountsMeta, ChangeTerraRewardsDestinationAccountsMeta,
    ChangeTokenSwapPoolAccountsMeta,
};
use anker::wormhole::TerraAddress;
use borsh::de::BorshDeserialize;
use borsh::ser::BorshSerialize;
use clap::Parser;
use serde::Serialize;
use serum_multisig::accounts as multisig_accounts;
use serum_multisig::instruction as multisig_instruction;
use solana_sdk::bpf_loader_upgradeable;
use solana_sdk::instruction::Instruction;
use solana_sdk::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature, Signer};
use solana_sdk::system_instruction;
use solana_sdk::sysvar;

use lido::{
    instruction::{
        AddMaintainerMeta, AddValidatorMeta, ChangeRewardDistributionMeta, DeactivateValidatorMeta,
        LidoInstruction, RemoveMaintainerMeta,
    },
    state::{FeeRecipients, Lido, RewardDistribution},
    util::{serialize_b58, serialize_b58_slice},
};
use solido_cli_common::error::Abort;
use solido_cli_common::snapshot::SnapshotError;
use solido_cli_common::{
    snapshot::{SnapshotClientConfig, SnapshotConfig},
    Result,
};

use crate::config::{
    ApproveBatchOpts, ApproveOpts, ConfigFile, CreateMultisigOpts, ExecuteTransactionOpts,
    ProposeChangeMultisigOpts, ProposeUpgradeOpts, ShowMultisigOpts, ShowTransactionOpts,
    TransferTokenOpts,
};
use crate::print_output;

/// The address of Wormhole-wrapped Lido DAO token (wLDO) on mainnet-beta.
mod wormhole_ldo_token {
    use solana_sdk::declare_id;
    declare_id!("HZRCwxP2Vq9PCpPXooayhJ2bxTpo5xfpQrwB1svh332p");
}

#[derive(Parser, Debug)]
pub struct MultisigOpts {
    #[clap(subcommand)]
    subcommand: SubCommand,
}

impl MultisigOpts {
    pub fn merge_with_config_and_environment(&mut self, config_file: Option<&ConfigFile>) {
        match &mut self.subcommand {
            SubCommand::CreateMultisig(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::ShowMultisig(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::ShowTransaction(opts) => {
                opts.merge_with_config_and_environment(config_file)
            }
            SubCommand::ProposeUpgrade(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::ProposeChangeMultisig(opts) => {
                opts.merge_with_config_and_environment(config_file)
            }
            SubCommand::Approve(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::ExecuteTransaction(opts) => {
                opts.merge_with_config_and_environment(config_file)
            }
            SubCommand::ApproveBatch(opts) => opts.merge_with_config_and_environment(config_file),
            SubCommand::TokenTransfer(opts) => opts.merge_with_config_and_environment(config_file),
        }
    }
}

#[derive(Parser, Debug)]
enum SubCommand {
    /// Create a new multisig address.
    CreateMultisig(CreateMultisigOpts),

    /// Show the owners and threshold of the given multisig.
    ShowMultisig(ShowMultisigOpts),

    /// Show the details of a transaction.
    ShowTransaction(ShowTransactionOpts),

    /// Propose replacing a program with that in the given buffer account.
    ProposeUpgrade(ProposeUpgradeOpts),

    /// Propose replacing the set of owners or threshold of this multisig.
    ProposeChangeMultisig(ProposeChangeMultisigOpts),

    /// Approve a proposed transaction.
    Approve(ApproveOpts),

    /// Execute a transaction that has enough approvals.
    ExecuteTransaction(ExecuteTransactionOpts),

    /// Approve a batch of multisig transactions one by one.
    ApproveBatch(ApproveBatchOpts),

    /// Transfer SPL tokens.
    TokenTransfer(TransferTokenOpts),
}

pub fn main(config: &mut SnapshotClientConfig, multisig_opts: MultisigOpts) {
    let output_mode = config.output_mode;
    match multisig_opts.subcommand {
        SubCommand::CreateMultisig(cmd_opts) => {
            let result = config.with_snapshot(|config| create_multisig(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to create multisig.");
            print_output(output_mode, &output);
        }
        SubCommand::ShowMultisig(cmd_opts) => {
            let result = config.with_snapshot(|config| show_multisig(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to read multisig.");
            print_output(output_mode, &output);
        }
        SubCommand::ShowTransaction(cmd_opts) => {
            let result = config.with_snapshot(|config| {
                let anker_program_id = if cmd_opts.anker_program_id() == &Pubkey::default() {
                    None
                } else {
                    Some(*cmd_opts.anker_program_id())
                };
                show_transaction(
                    config,
                    cmd_opts.transaction_address(),
                    cmd_opts.multisig_program_id(),
                    cmd_opts.solido_program_id(),
                    anker_program_id,
                )
            });
            let output = result.ok_or_abort_with("Failed to read multisig.");
            print_output(output_mode, &output);
        }
        SubCommand::ProposeUpgrade(cmd_opts) => {
            let result = config.with_snapshot(|config| propose_upgrade(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to propose upgrade.");
            print_output(output_mode, &output);
        }
        SubCommand::ProposeChangeMultisig(cmd_opts) => {
            let result = config.with_snapshot(|config| propose_change_multisig(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to propose multisig change.");
            print_output(output_mode, &output);
        }
        SubCommand::Approve(cmd_opts) => {
            let result = approve(
                config,
                cmd_opts.transaction_address(),
                cmd_opts.multisig_program_id(),
                cmd_opts.multisig_address(),
            );
            let output = result.ok_or_abort_with("Failed to approve multisig transaction.");
            print_output(output_mode, &output);
        }
        SubCommand::ExecuteTransaction(cmd_opts) => {
            let result = config.with_snapshot(|config| {
                execute_transaction(
                    config,
                    cmd_opts.transaction_address(),
                    cmd_opts.multisig_program_id(),
                    cmd_opts.multisig_address(),
                )
            });
            let output = result.ok_or_abort_with("Failed to execute multisig transaction.");
            print_output(output_mode, &output);
        }
        SubCommand::ApproveBatch(cmd_opts) => {
            let result = approve_batch(config, &cmd_opts);
            result.ok_or_abort_with("Failed to batch-approve multisig transactions.");
        }
        SubCommand::TokenTransfer(cmd_opts) => {
            let result = config.with_snapshot(|config| transfer_token(config, &cmd_opts));
            let output = result.ok_or_abort_with("Failed to transfer token.");
            print_output(output_mode, &output);
        }
    }
}

pub fn get_multisig_program_address(
    program_address: &Pubkey,
    multisig_pubkey: &Pubkey,
) -> (Pubkey, u8) {
    let seeds = [multisig_pubkey.as_ref()];
    Pubkey::find_program_address(&seeds, program_address)
}

#[derive(Serialize)]
struct CreateMultisigOutput {
    #[serde(serialize_with = "serialize_b58")]
    multisig_address: Pubkey,

    #[serde(serialize_with = "serialize_b58")]
    multisig_program_derived_address: Pubkey,
}

impl fmt::Display for CreateMultisigOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Multisig address:        {}", self.multisig_address)?;
        writeln!(
            f,
            "Program derived address: {}",
            self.multisig_program_derived_address
        )?;
        writeln!(f, "The multisig can sign on behalf of the derived address.")?;
        Ok(())
    }
}

fn create_multisig(
    config: &mut SnapshotConfig,
    opts: &CreateMultisigOpts,
) -> Result<CreateMultisigOutput> {
    // Enforce a few basic sanity checks.
    opts.validate_or_exit();

    // Before we can make the Multisig program initialize a new multisig
    // account, we need to have a program-owned account to used for that.
    // We generate a temporary key pair for this; after the account is
    // constructed, we no longer need to manipulate it (it is managed by the
    // Multisig program). We don't save the private key because the account will
    // be owned by the Multisig program later anyway. Its funds will be locked
    // up forever.
    let multisig_account = Keypair::new();

    // The Multisig program will sign transactions on behalf of a derived
    // account. Return this derived account, so it can be used to set as e.g.
    // the upgrade authority for a program. Because not every derived address is
    // valid, a bump seed is appended to the seeds. It is stored in the `nonce`
    // field in the multisig account, and the Multisig program includes it when
    // deriving its program address.
    let (program_derived_address, nonce) =
        get_multisig_program_address(opts.multisig_program_id(), &multisig_account.pubkey());

    let create_instruction = system_instruction::create_account(
        &config.signer.pubkey(),
        &multisig_account.pubkey(),
        // 352 bytes should be sufficient to hold a multisig state with 10
        // owners. Get the minimum rent-exempt balance for that, and
        // initialize the account with it, funded by the payer.
        // TODO(#180)
        // Ask for confirmation from the user first.
        config.client.get_minimum_balance_for_rent_exemption(352)?.0,
        352,
        opts.multisig_program_id(),
    );

    let multisig_instruction = Instruction {
        program_id: *opts.multisig_program_id(),
        data: multisig_instruction::CreateMultisig {
            owners: opts.owners().clone().0,
            threshold: *opts.threshold(),
            nonce,
        }
        .data(),
        accounts: multisig_accounts::CreateMultisig {
            multisig: multisig_account.pubkey(),
            rent: sysvar::rent::ID,
        }
        .to_account_metas(None),
    };

    config.sign_and_send_transaction(
        &[create_instruction, multisig_instruction],
        &[&multisig_account, config.signer],
    )?;

    let result = CreateMultisigOutput {
        multisig_address: multisig_account.pubkey(),
        multisig_program_derived_address: program_derived_address,
    };
    Ok(result)
}

#[derive(Serialize)]
struct ShowMultisigOutput {
    #[serde(serialize_with = "serialize_b58")]
    multisig_program_derived_address: Pubkey,

    threshold: u64,

    #[serde(serialize_with = "serialize_b58_slice")]
    owners: Vec<Pubkey>,
}

impl fmt::Display for ShowMultisigOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "Program derived address: {}",
            self.multisig_program_derived_address
        )?;
        writeln!(
            f,
            "Threshold: {} out of {}",
            self.threshold,
            self.owners.len()
        )?;
        writeln!(f, "Owners:")?;
        for owner_pubkey in &self.owners {
            writeln!(f, "  {}", owner_pubkey)?;
        }
        Ok(())
    }
}

fn show_multisig(
    config: &mut SnapshotConfig,
    opts: &ShowMultisigOpts,
) -> Result<ShowMultisigOutput> {
    let multisig: serum_multisig::Multisig = config
        .client
        .get_account_deserialize(opts.multisig_address())?;

    let (program_derived_address, _nonce) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());

    let result = ShowMultisigOutput {
        multisig_program_derived_address: program_derived_address,
        threshold: multisig.threshold,
        owners: multisig.owners,
    };
    Ok(result)
}

#[derive(Serialize)]
struct ShowTransactionSigner {
    #[serde(serialize_with = "serialize_b58")]
    owner: Pubkey,
    did_sign: bool,
}

#[derive(Serialize)]
enum ShowTransactionSigners {
    /// The current owners of the multisig are the same as in the transaction,
    /// and these are the owners and whether they signed.
    Current { signers: Vec<ShowTransactionSigner> },

    /// The owners of the multisig have changed since this transaction, so we
    /// cannot know who the signers were any more, only how many signatures it
    /// had.
    Outdated {
        num_signed: usize,
        num_owners: usize,
    },
}

/// If an `Instruction` is a known one, this contains its details.
#[derive(Serialize)]
enum ParsedInstruction {
    BpfLoaderUpgrade {
        #[serde(serialize_with = "serialize_b58")]
        program_to_upgrade: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        program_data_address: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        buffer_address: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        spill_address: Pubkey,
    },
    MultisigChange {
        old_threshold: u64,

        #[serde(serialize_with = "serialize_b58_slice")]
        old_owners: Vec<Pubkey>,

        new_threshold: u64,

        #[serde(serialize_with = "serialize_b58_slice")]
        new_owners: Vec<Pubkey>,
    },
    SolidoInstruction(SolidoInstruction),
    AnkerInstruction(AnkerInstruction),
    TokenInstruction(TokenInstruction),
    InvalidSolidoInstruction,
    InvalidAnkerInstruction,
    Unrecognized,
}

#[derive(Serialize)]
enum SolidoInstruction {
    AddValidator {
        #[serde(serialize_with = "serialize_b58")]
        solido_instance: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        manager: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        validator_fee_st_sol_account: Pubkey,
    },
    DeactivateValidator {
        #[serde(serialize_with = "serialize_b58")]
        solido_instance: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        manager: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        validator_vote_account: Pubkey,
    },
    AddMaintainer {
        #[serde(serialize_with = "serialize_b58")]
        solido_instance: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        manager: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        maintainer: Pubkey,
    },
    RemoveMaintainer {
        #[serde(serialize_with = "serialize_b58")]
        solido_instance: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        manager: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        maintainer: Pubkey,
    },
    ChangeRewardDistribution {
        current_solido: Box<Lido>,
        reward_distribution: RewardDistribution,

        #[serde(serialize_with = "serialize_b58")]
        solido_instance: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        manager: Pubkey,

        fee_recipients: FeeRecipients,
    },
}

#[allow(clippy::enum_variant_names)]
#[derive(Serialize)]
enum AnkerInstruction {
    ChangeTerraRewardsDestination {
        #[serde(serialize_with = "serialize_b58")]
        anker_instance: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        manager: Pubkey,

        old_terra_rewards_destination: TerraAddress,

        new_terra_rewards_destination: TerraAddress,
    },
    ChangeTokenSwapPool {
        #[serde(serialize_with = "serialize_b58")]
        anker_instance: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        manager: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        old_token_swap_pool: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        new_token_swap_pool: Pubkey,
    },
    ChangeSellRewardsMinOutBps {
        #[serde(serialize_with = "serialize_b58")]
        anker_instance: Pubkey,

        #[serde(serialize_with = "serialize_b58")]
        manager: Pubkey,

        old_sell_rewards_min_out_bps: u64,

        new_sell_rewards_min_out_bps: u64,
    },
}

#[derive(Serialize)]
enum TokenInstruction {
    Transfer {
        #[serde(serialize_with = "serialize_b58")]
        from_address: Pubkey,
        #[serde(serialize_with = "serialize_b58")]
        to_address: Pubkey,
        #[serde(serialize_with = "serialize_b58")]
        token_address: Pubkey,
        amount: u64,
    },
    Unsupported,
}

#[derive(Serialize)]
struct ShowTransactionOutput {
    #[serde(serialize_with = "serialize_b58")]
    multisig_address: Pubkey,
    did_execute: bool,
    signers: ShowTransactionSigners,
    // TODO(#180)
    // when using --output-json, the addresses in here get serialized as
    // arrays of numbers instead of base58 strings, because this uses the
    // regular Solana `Pubkey` types. But I don't feel like creating an
    // `Instruction` duplicate just for this purpose right now, we can create
    // one when needed.
    instruction: Instruction,
    parsed_instruction: ParsedInstruction,
}

impl fmt::Display for ShowTransactionOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Multisig: {}", self.multisig_address)?;
        writeln!(f, "Did execute: {}", self.did_execute)?;

        match &self.signers {
            ShowTransactionSigners::Current { signers } => {
                writeln!(f, "\nSigners:")?;
                for signer in signers {
                    writeln!(
                        f,
                        "  [{}] {}",
                        if signer.did_sign { 'x' } else { ' ' },
                        signer.owner,
                    )?;
                }
            }
            ShowTransactionSigners::Outdated {
                num_signed,
                num_owners,
            } => {
                writeln!(
                    f,
                    "\nThe owners of the multisig have changed since this transaction was created,"
                )?;
                writeln!(f, "therefore we cannot show the identities of the signers.")?;
                writeln!(
                    f,
                    "The transaction had {} out of {} signatures.",
                    num_signed, num_owners,
                )?;
            }
        }

        writeln!(f, "\nInstruction:")?;
        writeln!(f, "  Program to call: {}", self.instruction.program_id)?;
        writeln!(f, "  Accounts:\n")?;
        for account in &self.instruction.accounts {
            writeln!(
                f,
                "    * {}\n      signer: {}, writable: {}\n",
                account.pubkey, account.is_signer, account.is_writable,
            )?;
        }

        match &self.parsed_instruction {
            ParsedInstruction::BpfLoaderUpgrade {
                program_to_upgrade,
                program_data_address,
                buffer_address,
                spill_address,
            } => {
                writeln!(
                    f,
                    "  This is a bpf_loader_upgradeable::upgrade instruction."
                )?;
                writeln!(f, "    Program to upgrade:      {}", program_to_upgrade)?;
                writeln!(f, "    Program data address:    {}", program_data_address)?;
                writeln!(f, "    Buffer with new program: {}", buffer_address)?;
                writeln!(f, "    Spill address:           {}", spill_address)?;
            }
            ParsedInstruction::MultisigChange {
                old_threshold,
                old_owners,
                new_threshold,
                new_owners,
            } => {
                writeln!(
                    f,
                    "  This is a serum_multisig::set_owners_and_change_threshold instruction."
                )?;
                print_diff_multisig(f, *old_threshold, *new_threshold, old_owners, new_owners)?;
            }
            ParsedInstruction::SolidoInstruction(solido_instruction) => {
                write!(f, "  This is a Solido instruction. ")?;
                match solido_instruction {
                    SolidoInstruction::AddValidator {
                        solido_instance,
                        manager,
                        validator_vote_account,
                        validator_fee_st_sol_account,
                    } => {
                        writeln!(f, "It adds a validator to Solido")?;
                        writeln!(f, "    Solido instance:        {}", solido_instance)?;
                        writeln!(f, "    Manager:                {}", manager)?;
                        writeln!(f, "    Validator vote account: {}", validator_vote_account)?;
                        writeln!(
                            f,
                            "    Validator fee account:  {}",
                            validator_fee_st_sol_account
                        )?;
                    }
                    SolidoInstruction::DeactivateValidator {
                        solido_instance,
                        manager,
                        validator_vote_account,
                    } => {
                        writeln!(f, "It deactivates a validator.")?;
                        writeln!(f, "    Solido instance:        {}", solido_instance)?;
                        writeln!(f, "    Manager:                {}", manager)?;
                        writeln!(f, "    Validator vote account: {}", validator_vote_account)?;
                    }
                    SolidoInstruction::AddMaintainer {
                        solido_instance,
                        manager,
                        maintainer,
                    } => {
                        writeln!(f, "It adds a maintainer")?;
                        writeln!(f, "    Solido instance: {}", solido_instance)?;
                        writeln!(f, "    Manager:         {}", manager)?;
                        writeln!(f, "    Maintainer:      {}", maintainer)?;
                    }
                    SolidoInstruction::RemoveMaintainer {
                        solido_instance,
                        manager,
                        maintainer,
                    } => {
                        writeln!(f, "It removes a maintainer")?;
                        writeln!(f, "    Solido instance: {}", solido_instance)?;
                        writeln!(f, "    Manager:         {}", manager)?;
                        writeln!(f, "    Maintainer:      {}", maintainer)?;
                    }
                    SolidoInstruction::ChangeRewardDistribution {
                        current_solido,
                        reward_distribution,
                        solido_instance,
                        manager,
                        fee_recipients,
                    } => {
                        writeln!(f, "It changes the reward distribution")?;
                        writeln!(f, "    Solido instance:       {}", solido_instance)?;
                        writeln!(f, "    Manager:               {}", manager)?;
                        writeln!(f)?;
                        print_changed_reward_distribution(f, current_solido, reward_distribution)?;
                        print_changed_recipients(f, current_solido, fee_recipients)?;
                    }
                }
            }
            ParsedInstruction::Unrecognized => {
                writeln!(f, "  Unrecognized instruction. Provide --solido-program-id <address> parameter to parse a Solido instruction")?;
            }
            ParsedInstruction::InvalidSolidoInstruction => {
                writeln!(
                    f,
                    "  Tried to deserialize a Solido instruction, but failed."
                )?;
            }
            ParsedInstruction::TokenInstruction(token_instruction) => {
                write!(f, "  This is an SPL token instruction. ")?;
                match token_instruction {
                    TokenInstruction::Transfer {
                        from_address,
                        to_address,
                        token_address,
                        amount,
                    } => {
                        writeln!(f, "It transfers tokens.")?;
                        writeln!(f, "    Token address: {}", token_address)?;
                        writeln!(f, "    From address:  {}", from_address)?;
                        writeln!(f, "    To address:    {}", to_address)?;
                        if *token_address == wormhole_ldo_token::id() {
                            // If the token happens to be Wormhole LDO, then we can format it better
                            // by placing the decimal point and by clarifying the token.
                            // TODO(#485): Use the impl_token! macro to make a type for wLDO,
                            // then use that to format here.
                            writeln!(
                                f,
                                "    Amount:        {}.{:>08} wLDO (Wormhole-wrapped Lido DAO token)",
                                amount / 1_0000_0000,
                                amount % 1_0000_0000,
                            )?;
                        } else {
                            writeln!(
                                f,
                                "    Amount:        {}, of the token's smallest denomination",
                                amount
                            )?;
                        }
                    }
                    TokenInstruction::Unsupported => {
                        writeln!(f, "The instruction is currently unsupported.")?;
                    }
                }
            }
            ParsedInstruction::AnkerInstruction(anker_instruction) => match anker_instruction {
                AnkerInstruction::ChangeTerraRewardsDestination {
                    anker_instance,
                    manager,
                    old_terra_rewards_destination,
                    new_terra_rewards_destination,
                } => {
                    writeln!(f, "It changes the Terra rewards destination in Anker")?;
                    writeln!(f, "    Anker instance:                {}", anker_instance)?;
                    writeln!(f, "    Manager:                       {}", manager)?;
                    writeln!(
                        f,
                        "    Old Terra rewards destination: {}",
                        old_terra_rewards_destination
                    )?;
                    writeln!(
                        f,
                        "    New Terra rewards destination: {}",
                        new_terra_rewards_destination
                    )?;
                }
                AnkerInstruction::ChangeTokenSwapPool {
                    anker_instance,
                    manager,
                    old_token_swap_pool,
                    new_token_swap_pool,
                } => {
                    writeln!(f, "It changes the Token Swap Pool in Anker")?;
                    writeln!(f, "    Anker instance:      {}", anker_instance)?;
                    writeln!(f, "    Manager:             {}", manager)?;
                    writeln!(f, "    Old Token Swap Pool: {}", old_token_swap_pool)?;
                    writeln!(f, "    New Token Swap Pool: {}", new_token_swap_pool)?;
                }
                AnkerInstruction::ChangeSellRewardsMinOutBps {
                    anker_instance,
                    manager,
                    old_sell_rewards_min_out_bps,
                    new_sell_rewards_min_out_bps,
                } => {
                    writeln!(f, "It changes the sell rewards min bps in Anker")?;
                    writeln!(f, "    Anker instance:           {}", anker_instance)?;
                    writeln!(f, "    Manager:                  {}", manager)?;
                    writeln!(
                        f,
                        "    Old sell rewards min bps: {}",
                        old_sell_rewards_min_out_bps
                    )?;
                    writeln!(
                        f,
                        "    New sell rewards min bps: {}",
                        new_sell_rewards_min_out_bps
                    )?;
                }
            },
            ParsedInstruction::InvalidAnkerInstruction => {
                writeln!(
                    f,
                    "  Tried to deserialize an Anker instruction, but failed."
                )?;
            }
        }

        Ok(())
    }
}

fn changed_fee(
    f: &mut fmt::Formatter,
    current_param: u32,
    new_param: u32,
    current_sum: u64,
    new_sum: u64,
    param_name: &str,
) -> fmt::Result {
    let before = format!("{}/{}", current_param, current_sum);
    let after = format!("{}/{}", new_param, new_sum);
    if before == after {
        writeln!(f, "   {}:          {:>5}", param_name, after)?;
    } else {
        writeln!(f, "   {}: {:>5} -> {:>5}", param_name, before, after)?;
    }
    Ok(())
}

fn print_changed_reward_distribution(
    f: &mut fmt::Formatter,
    current_solido: &Lido,
    reward_distribution: &RewardDistribution,
) -> fmt::Result {
    let current_sum = current_solido.reward_distribution.sum();
    let new_sum = reward_distribution.sum();
    changed_fee(
        f,
        current_solido.reward_distribution.treasury_fee,
        reward_distribution.treasury_fee,
        current_sum,
        new_sum,
        "treasury",
    )?;
    changed_fee(
        f,
        current_solido.reward_distribution.developer_fee,
        reward_distribution.developer_fee,
        current_sum,
        new_sum,
        "developer",
    )?;
    changed_fee(
        f,
        current_solido.reward_distribution.st_sol_appreciation,
        reward_distribution.st_sol_appreciation,
        current_sum,
        new_sum,
        "stSOL appreciation",
    )?;
    Ok(())
}

fn print_changed_recipients(
    f: &mut fmt::Formatter,
    current_solido: &Lido,
    fee_recipients: &FeeRecipients,
) -> fmt::Result {
    changed_addr(
        f,
        &current_solido.fee_recipients.treasury_account,
        &fee_recipients.treasury_account,
        "treasury",
    )?;
    changed_addr(
        f,
        &current_solido.fee_recipients.developer_account,
        &fee_recipients.developer_account,
        "developer",
    )?;
    Ok(())
}

fn changed_addr(
    f: &mut fmt::Formatter,
    current_addr: &Pubkey,
    new_addr: &Pubkey,
    param_name: &str,
) -> fmt::Result {
    if current_addr == new_addr {
        writeln!(f, "   {}: {}", param_name, new_addr)?;
    } else {
        writeln!(f, "   {}: {} -> {}", param_name, new_addr, current_addr,)?;
    }
    Ok(())
}

fn print_diff_multisig(
    f: &mut fmt::Formatter,
    old_threshold: u64,
    new_threshold: u64,
    old_owners: &[Pubkey],
    new_owners: &[Pubkey],
) -> fmt::Result {
    if (old_threshold, old_owners.len()) == (new_threshold, new_owners.len()) {
        writeln!(
            f,
            "    Threshold (unchanged): {} of {}",
            new_threshold,
            new_owners.len(),
        )?;
    } else {
        writeln!(
            f,
            "    Threshold (changed): {} of {} -> {} of {}",
            old_threshold,
            old_owners.len(),
            new_threshold,
            new_owners.len(),
        )?;
    }
    let old_owners_set: HashSet<_> = old_owners.iter().collect();
    let new_owners_set: HashSet<_> = new_owners.iter().collect();
    if old_owners_set == new_owners_set {
        writeln!(f, "    Owners (unchanged):")?;
    } else {
        writeln!(f, "    Owners (changed):")?;
    }

    for owner in old_owners_set.intersection(&new_owners_set) {
        writeln!(f, "        {}", owner)?;
    }
    for removed_owner in old_owners_set.difference(&new_owners_set) {
        writeln!(f, "      - {}", removed_owner)?;
    }
    for added_owner in new_owners_set.difference(&old_owners_set) {
        writeln!(f, "      + {}", added_owner)?;
    }
    Ok(())
}

fn show_transaction(
    config: &mut SnapshotConfig,
    transaction_address: &Pubkey,
    multisig_program_id: &Pubkey,
    solido_program_id: &Pubkey,
    anker_program_id: Option<Pubkey>,
) -> Result<ShowTransactionOutput> {
    let transaction: serum_multisig::Transaction =
        config.client.get_account_deserialize(transaction_address)?;

    // Also query the multisig, to get the owner public keys, so we can display
    // exactly who voted.
    let multisig: serum_multisig::Multisig = config
        .client
        .get_account_deserialize(&transaction.multisig)?;

    let signers = if transaction.owner_set_seqno == multisig.owner_set_seqno {
        // If the owners did not change, match up every vote with its owner.
        ShowTransactionSigners::Current {
            signers: multisig
                .owners
                .iter()
                .cloned()
                .zip(transaction.signers.iter())
                .map(|(owner, &did_sign)| ShowTransactionSigner { owner, did_sign })
                .collect(),
        }
    } else {
        // If the owners did change, we no longer know who voted. The best we
        // can do is report how many signatures there were.
        ShowTransactionSigners::Outdated {
            num_signed: transaction
                .signers
                .iter()
                .filter(|&did_sign| *did_sign)
                .count(),
            num_owners: transaction.signers.len(),
        }
    };

    let instr = Instruction::from(&transaction);

    let parsed_instr = if instr.program_id == bpf_loader_upgradeable::ID
        && bpf_loader_upgradeable::is_upgrade_instruction(&instr.data[..])
    {
        // Account meaning, according to
        // https://docs.rs/solana-sdk/1.5.19/solana_sdk/loader_upgradeable_instruction/enum.UpgradeableLoaderInstruction.html#variant.Upgrade
        ParsedInstruction::BpfLoaderUpgrade {
            program_data_address: instr.accounts[0].pubkey,
            program_to_upgrade: instr.accounts[1].pubkey,
            buffer_address: instr.accounts[2].pubkey,
            spill_address: instr.accounts[3].pubkey,
        }
    }
    // Try to deserialize the known multisig instructions. The instruction
    // data starts with an 8-byte tag derived from the name of the function,
    // and then the struct data itself, so we need to skip the first 8 bytes
    // when deserializing. See also `anchor_lang::InstructionData::data()`.
    // There doesn't appear to be a way to access the tag through code
    // currently (https://github.com/project-serum/anchor/issues/243), so we
    // hard-code the tag here (it is stable as long as the namespace and
    // function name do not change).
    else if instr.program_id == *multisig_program_id
        && instr.data[..8] == [55, 144, 74, 245, 249, 230, 14, 53]
    {
        if let Ok(instr) =
            multisig_instruction::SetOwnersAndChangeThreshold::try_from_slice(&instr.data[8..])
        {
            ParsedInstruction::MultisigChange {
                old_threshold: multisig.threshold,
                old_owners: multisig.owners,
                new_threshold: instr.threshold,
                new_owners: instr.owners,
            }
        } else {
            ParsedInstruction::Unrecognized
        }
    } else if &instr.program_id == solido_program_id {
        // Probably a Solido instruction
        match try_parse_solido_instruction(config, &instr) {
            Ok(instr) => instr,
            Err(SnapshotError::MissingAccount) => return Err(SnapshotError::MissingAccount),
            Err(SnapshotError::MissingValidatorIdentity(addr)) => {
                return Err(SnapshotError::MissingValidatorIdentity(addr))
            }
            Err(SnapshotError::OtherError(err)) => {
                println!("Warning: Failed to parse Solido instruction.");
                err.print_pretty();
                ParsedInstruction::InvalidSolidoInstruction
            }
        }
    } else if instr.program_id == spl_token::id() {
        match try_parse_token_instruction(config, &instr) {
            Ok(instr) => instr,
            Err(SnapshotError::MissingAccount) => return Err(SnapshotError::MissingAccount),
            Err(SnapshotError::MissingValidatorIdentity(addr)) => {
                return Err(SnapshotError::MissingValidatorIdentity(addr))
            }
            Err(SnapshotError::OtherError(err)) => {
                println!("Warning: Failed to parse Token instruction.");
                err.print_pretty();
                ParsedInstruction::InvalidSolidoInstruction
            }
        }
    } else if anker_program_id == Some(instr.program_id) {
        match try_parse_anker_instruction(config, &instr) {
            Ok(instr) => instr,
            Err(SnapshotError::MissingAccount) => return Err(SnapshotError::MissingAccount),
            Err(SnapshotError::MissingValidatorIdentity(addr)) => {
                return Err(SnapshotError::MissingValidatorIdentity(addr))
            }
            Err(SnapshotError::OtherError(err)) => {
                println!("Warning: Failed to parse Anker instruction.");
                err.print_pretty();
                ParsedInstruction::InvalidAnkerInstruction
            }
        }
    } else {
        ParsedInstruction::Unrecognized
    };

    let result = ShowTransactionOutput {
        multisig_address: transaction.multisig,
        did_execute: transaction.did_execute,
        signers,
        instruction: instr,
        parsed_instruction: parsed_instr,
    };
    Ok(result)
}

fn try_parse_solido_instruction(
    config: &mut SnapshotConfig,
    instr: &Instruction,
) -> Result<ParsedInstruction> {
    let instruction: LidoInstruction = BorshDeserialize::deserialize(&mut instr.data.as_slice())?;
    Ok(match instruction {
        LidoInstruction::ChangeRewardDistribution {
            new_reward_distribution,
        } => {
            let accounts = ChangeRewardDistributionMeta::try_from_slice(&instr.accounts)?;
            let current_solido = config.client.get_solido(&accounts.lido)?;
            ParsedInstruction::SolidoInstruction(SolidoInstruction::ChangeRewardDistribution {
                current_solido: Box::new(current_solido),
                reward_distribution: new_reward_distribution,
                solido_instance: accounts.lido,
                manager: accounts.manager,
                fee_recipients: FeeRecipients {
                    treasury_account: accounts.treasury_account,
                    developer_account: accounts.developer_account,
                },
            })
        }
        LidoInstruction::AddValidator => {
            let accounts = AddValidatorMeta::try_from_slice(&instr.accounts)?;
            ParsedInstruction::SolidoInstruction(SolidoInstruction::AddValidator {
                solido_instance: accounts.lido,
                manager: accounts.manager,
                validator_vote_account: accounts.validator_vote_account,
                validator_fee_st_sol_account: accounts.validator_fee_st_sol_account,
            })
        }
        LidoInstruction::DeactivateValidator => {
            let accounts = DeactivateValidatorMeta::try_from_slice(&instr.accounts)?;
            ParsedInstruction::SolidoInstruction(SolidoInstruction::DeactivateValidator {
                solido_instance: accounts.lido,
                manager: accounts.manager,
                validator_vote_account: accounts.validator_vote_account_to_deactivate,
            })
        }
        LidoInstruction::AddMaintainer => {
            let accounts = AddMaintainerMeta::try_from_slice(&instr.accounts)?;
            ParsedInstruction::SolidoInstruction(SolidoInstruction::AddMaintainer {
                solido_instance: accounts.lido,
                manager: accounts.manager,
                maintainer: accounts.maintainer,
            })
        }
        LidoInstruction::RemoveMaintainer => {
            let accounts = RemoveMaintainerMeta::try_from_slice(&instr.accounts)?;
            ParsedInstruction::SolidoInstruction(SolidoInstruction::RemoveMaintainer {
                solido_instance: accounts.lido,
                manager: accounts.manager,
                maintainer: accounts.maintainer,
            })
        }
        _ => ParsedInstruction::InvalidSolidoInstruction,
    })
}

fn try_parse_token_instruction(
    config: &mut SnapshotConfig,
    instr: &Instruction,
) -> Result<ParsedInstruction> {
    let instruction = spl_token::instruction::TokenInstruction::unpack(instr.data.as_slice())?;

    // Get the from account and deserialize it to an `spl_token`. This is done
    // to get the mint address for the token. If the mint addresses differ, the
    // instruction simulation will fail when proposing.
    let from_account = config.client.get_account(&instr.accounts[0].pubkey)?;
    let spl_token_from = spl_token::state::Account::unpack(&from_account.data)?;
    match instruction {
        spl_token::instruction::TokenInstruction::Transfer { amount } => Ok(
            ParsedInstruction::TokenInstruction(TokenInstruction::Transfer {
                from_address: instr.accounts[0].pubkey,
                to_address: instr.accounts[1].pubkey,
                token_address: spl_token_from.mint,
                amount,
            }),
        ),
        _ => Ok(ParsedInstruction::TokenInstruction(
            TokenInstruction::Unsupported,
        )),
    }
}

fn try_parse_anker_instruction(
    config: &mut SnapshotConfig,
    instr: &Instruction,
) -> Result<ParsedInstruction> {
    let instruction: anker::instruction::AnkerInstruction =
        BorshDeserialize::deserialize(&mut instr.data.as_slice())?;
    Ok(match instruction {
        anker::instruction::AnkerInstruction::ChangeTerraRewardsDestination {
            terra_rewards_destination,
        } => {
            let accounts =
                ChangeTerraRewardsDestinationAccountsMeta::try_from_slice(&instr.accounts)?;
            let current_anker = config.client.get_anker(&accounts.anker)?;
            ParsedInstruction::AnkerInstruction(AnkerInstruction::ChangeTerraRewardsDestination {
                anker_instance: accounts.anker,
                manager: accounts.manager,
                old_terra_rewards_destination: current_anker.terra_rewards_destination,
                new_terra_rewards_destination: terra_rewards_destination,
            })
        }
        anker::instruction::AnkerInstruction::ChangeTokenSwapPool => {
            let accounts = ChangeTokenSwapPoolAccountsMeta::try_from_slice(&instr.accounts)?;
            ParsedInstruction::AnkerInstruction(AnkerInstruction::ChangeTokenSwapPool {
                anker_instance: accounts.anker,
                manager: accounts.manager,
                old_token_swap_pool: accounts.current_token_swap_pool,
                new_token_swap_pool: accounts.new_token_swap_pool,
            })
        }
        anker::instruction::AnkerInstruction::ChangeSellRewardsMinOutBps {
            sell_rewards_min_out_bps,
        } => {
            let accounts = ChangeSellRewardsMinOutBpsAccountsMeta::try_from_slice(&instr.accounts)?;
            let current_anker = config.client.get_anker(&accounts.anker)?;
            ParsedInstruction::AnkerInstruction(AnkerInstruction::ChangeSellRewardsMinOutBps {
                anker_instance: accounts.anker,
                manager: accounts.manager,
                old_sell_rewards_min_out_bps: current_anker.sell_rewards_min_out_bps,
                new_sell_rewards_min_out_bps: sell_rewards_min_out_bps,
            })
        }
        _ => ParsedInstruction::InvalidAnkerInstruction,
    })
}

#[derive(Serialize)]
pub struct ProposeInstructionOutput {
    #[serde(serialize_with = "serialize_b58")]
    transaction_address: Pubkey,
}

impl fmt::Display for ProposeInstructionOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Transaction address: {}", self.transaction_address)
    }
}

/// Propose the given instruction to be approved and executed by the multisig.
pub fn propose_instruction(
    config: &mut SnapshotConfig,
    multisig_program_id: &Pubkey,
    multisig_address: Pubkey,
    instruction: Instruction,
) -> Result<ProposeInstructionOutput> {
    // The transaction is stored by the Multisig program in yet another account,
    // that we create just for this transaction. We don't save the private key
    // because the account will be owned by the multisig program later; its
    // funds will be locked forever.
    let transaction_account = Keypair::new();

    // The Multisig program expects `serum_multisig::TransactionAccount` instead
    // of `solana_sdk::AccountMeta`. The types are structurally identical, but
    // not nominally, so we need to convert these.
    let accounts: Vec<_> = instruction
        .accounts
        .iter()
        .map(serum_multisig::TransactionAccount::from)
        .collect();

    // We are going to build a dummy version of the `serum_multisig::Transaction`,
    // to compute its size, which we need to allocate an account for it. And to
    // build the dummy transaction, we need to know how many owners the multisig
    // has.
    let multisig: serum_multisig::Multisig =
        config.client.get_account_deserialize(&multisig_address)?;

    // Build the data that the account will hold, just to measure its size, so
    // we can allocate an account of the right size.
    let dummy_tx = serum_multisig::Transaction {
        multisig: multisig_address,
        program_id: instruction.program_id,
        accounts,
        data: instruction.data.clone(),
        signers: multisig
            .owners
            .iter()
            .map(|a| a == &config.signer.pubkey())
            .collect(),
        did_execute: false,
        owner_set_seqno: multisig.owner_set_seqno,
    };

    // The space used is the serialization of the transaction itself, plus the
    // discriminator that Anchor uses to identify the account type.
    let mut account_bytes = serum_multisig::Transaction::discriminator().to_vec();
    dummy_tx
        .serialize(&mut account_bytes)
        .expect("Failed to serialize dummy transaction.");
    let tx_account_size = account_bytes.len();

    let create_instruction = system_instruction::create_account(
        &config.signer.pubkey(),
        &transaction_account.pubkey(),
        // TODO(#180)
        // Ask for confirmation from the user first before funding the
        // account.
        config
            .client
            .get_minimum_balance_for_rent_exemption(tx_account_size)?
            .0,
        tx_account_size as u64,
        multisig_program_id,
    );

    // The Multisig program expects `serum_multisig::TransactionAccount` instead
    // of `solana_sdk::AccountMeta`. The types are structurally identical,
    // but not nominally, so we need to convert these.
    let accounts: Vec<_> = instruction
        .accounts
        .iter()
        .map(serum_multisig::TransactionAccount::from)
        .collect();

    let multisig_accounts = multisig_accounts::CreateTransaction {
        multisig: multisig_address,
        transaction: transaction_account.pubkey(),
        // For convenience, assume that the party that signs the proposal
        // transaction is a member of the multisig owners, and use it as the
        // proposer.
        proposer: config.signer.pubkey(),
        rent: sysvar::rent::ID,
    }
    .to_account_metas(None);
    let multisig_ins = multisig_instruction::CreateTransaction {
        pid: instruction.program_id,
        accs: accounts,
        data: instruction.data,
    };

    let multisig_instruction = Instruction {
        program_id: *multisig_program_id,
        data: multisig_ins.data(),
        accounts: multisig_accounts,
    };

    config.sign_and_send_transaction(
        &[create_instruction, multisig_instruction],
        &[config.signer, &transaction_account],
    )?;

    let result = ProposeInstructionOutput {
        transaction_address: transaction_account.pubkey(),
    };
    Ok(result)
}

fn propose_upgrade(
    config: &mut SnapshotConfig,
    opts: &ProposeUpgradeOpts,
) -> Result<ProposeInstructionOutput> {
    let (program_derived_address, _nonce) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());

    let upgrade_instruction = bpf_loader_upgradeable::upgrade(
        opts.program_address(),
        opts.buffer_address(),
        // The upgrade authority is the multisig-derived program address.
        &program_derived_address,
        opts.spill_address(),
    );

    propose_instruction(
        config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        upgrade_instruction,
    )
}

fn propose_change_multisig(
    config: &mut SnapshotConfig,
    opts: &ProposeChangeMultisigOpts,
) -> Result<ProposeInstructionOutput> {
    // Check that the new settings make sense. This check is shared between a
    // new multisig or altering an existing one.
    CreateMultisigOpts::from(opts).validate_or_exit();

    let (program_derived_address, _nonce) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());

    let change_data = multisig_instruction::SetOwnersAndChangeThreshold {
        owners: opts.owners().clone().0,
        threshold: *opts.threshold(),
    };
    let change_addrs = multisig_accounts::Auth {
        multisig: *opts.multisig_address(),
        multisig_signer: program_derived_address,
    };

    let override_is_signer = None;
    let change_instruction = Instruction {
        program_id: *opts.multisig_program_id(),
        data: change_data.data(),
        accounts: change_addrs.to_account_metas(override_is_signer),
    };

    propose_instruction(
        config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        change_instruction,
    )
}

#[derive(Serialize)]
struct ApproveOutput {
    pub transaction_id: Signature,
    pub num_approvals: u64,
    pub threshold: u64,
}

impl fmt::Display for ApproveOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Transaction approved.")?;
        writeln!(
            f,
            "Solana transaction id of approval: {}",
            self.transaction_id
        )?;
        writeln!(
            f,
            "Multisig transaction now has {} out of {} required approvals.",
            self.num_approvals, self.threshold,
        )?;
        Ok(())
    }
}

fn approve(
    config: &mut SnapshotClientConfig,
    transaction_address: &Pubkey,
    multisig_program_id: &Pubkey,
    multisig_address: &Pubkey,
) -> std::result::Result<ApproveOutput, crate::Error> {
    // First, do the actual approval.
    let signature = config.with_snapshot(|config| {
        let approve_accounts = multisig_accounts::Approve {
            multisig: *multisig_address,
            transaction: *transaction_address,
            // The owner that signs the multisig proposed transaction, should be
            // the public key that signs the entire approval transaction (which
            // is also the payer).
            owner: config.signer.pubkey(),
        };
        let approve_instruction = Instruction {
            program_id: *multisig_program_id,
            data: multisig_instruction::Approve.data(),
            accounts: approve_accounts.to_account_metas(None),
        };
        config.sign_and_send_transaction(&[approve_instruction], &[config.signer])
    })?;

    // After a successful approval, query the new state of the transaction, so
    // we can show it to the user.
    let result = config.with_snapshot(|config| {
        let multisig: serum_multisig::Multisig =
            config.client.get_account_deserialize(multisig_address)?;

        let transaction: serum_multisig::Transaction =
            config.client.get_account_deserialize(transaction_address)?;

        let result = ApproveOutput {
            transaction_id: signature,
            num_approvals: transaction.signers.iter().filter(|x| **x).count() as u64,
            threshold: multisig.threshold,
        };

        Ok(result)
    })?;

    Ok(result)
}

fn approve_batch(
    config: &mut SnapshotClientConfig,
    opts: &ApproveBatchOpts,
) -> std::result::Result<(), crate::Error> {
    use solido_cli_common::snapshot::OutputMode;

    match config.output_mode {
        OutputMode::Json => {
            println!("Json output mode is not supported for batch approval; it requires an interactive TTY.");
            return Ok(());
        }
        OutputMode::Text => { /* This is fine. */ }
    }

    let transaction_addresses = std::fs::read_to_string(opts.transaction_addresses_path())
        .expect("Failed to read transaction addresses from file.");
    for (i, line) in transaction_addresses.lines().enumerate() {
        // Take the first word from the line; the remainder can contain a comment
        // about what the transaction is for.
        match line
            .split_ascii_whitespace()
            .next()
            .and_then(|addr_str| Pubkey::from_str(addr_str).ok())
        {
            Some(addr) => {
                // Now that we know the transaction address is valid, print the
                // full line, to preserve any trailing content. (But trim the
                // newline, println already adds one.)
                println!("\nTransaction {}", line.trim());
                approve_transaction_interactive(config, opts, &addr)?;
            }
            None => {
                println!("\nInvalid transaction address on line {}, skipping.", i + 1);
            }
        }
    }

    Ok(())
}

/// Prompt the user to enter 'Y' or 'N'.
fn ask_user_y_n(prompt: &'static str) -> bool {
    use std::io::{BufRead, Write};
    let mut buf = String::new();
    loop {
        print!("{} [Y/N] ", prompt);
        std::io::stdout()
            .lock()
            .flush()
            .expect("Failed to flush to stdout.");
        std::io::stdin()
            .lock()
            .read_line(&mut buf)
            .expect("Failed to read from stdin.");

        match &buf[..] {
            "Y\n" => return true,
            "N\n" => return false,
            _ => {
                println!("Please enter the character 'Y' or 'N' (case-sensitive).");
                buf.clear();
            }
        }
    }
}

fn approve_transaction_interactive(
    config: &mut SnapshotClientConfig,
    opts: &ApproveBatchOpts,
    transaction_address: &Pubkey,
) -> std::result::Result<(), crate::Error> {
    config.with_snapshot(|config| {
        let output = show_transaction(
            config,
            transaction_address,
            opts.multisig_program_id(),
            opts.solido_program_id(),
            None,
        )?;
        println!("{}", output);
        Ok(())
    })?;

    if !ask_user_y_n("Sign and submit approval transaction?") {
        println!(
            "Not approving transaction {}, continuing with next transaction if any.",
            transaction_address
        );
        return Ok(());
    }

    let approve_result = approve(
        config,
        transaction_address,
        opts.multisig_program_id(),
        opts.multisig_address(),
    )?;
    println!("{}", approve_result);

    let can_execute = approve_result.num_approvals >= approve_result.threshold;
    if can_execute && ask_user_y_n("Transaction can be executed, sign and submit execution?") {
        config.with_snapshot(|config| {
            let execute_result = execute_transaction(
                config,
                transaction_address,
                opts.multisig_program_id(),
                opts.multisig_address(),
            )?;
            println!("{}", execute_result);
            Ok(())
        })?;
    }

    Ok(())
}

/// Wrapper type needed to implement `ToAccountMetas`.
struct TransactionAccounts {
    accounts: Vec<serum_multisig::TransactionAccount>,
    program_id: Pubkey,
}

impl anchor_lang::ToAccountMetas for TransactionAccounts {
    fn to_account_metas(&self, is_signer: Option<bool>) -> Vec<AccountMeta> {
        assert_eq!(
            is_signer, None,
            "Overriding the signer is not implemented, it is not used by RequestBuilder::accounts.",
        );
        let mut account_metas: Vec<_> = self
            .accounts
            .iter()
            .map(|tx_account| {
                let mut account_meta = AccountMeta::from(tx_account);
                // When the program executes the transaction, it uses the account
                // list with the right signers. But when we build the wrapper
                // instruction that calls the `serum_multisig::execute_transaction,
                // the signers of the inner instruction should not be signers of
                // the outer one.
                account_meta.is_signer = false;
                account_meta
            })
            .collect();

        // Aside from the accounts that the transaction references, we also need
        // to include the id of the program it calls as a referenced account in
        // the outer instruction.
        let program_is_signer = false;
        account_metas.push(AccountMeta::new_readonly(
            self.program_id,
            program_is_signer,
        ));

        account_metas
    }
}

#[derive(Serialize)]
struct ExecuteOutput {
    pub transaction_id: Signature,
}

impl fmt::Display for ExecuteOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Transaction executed.")?;
        writeln!(
            f,
            "Solana transaction id of execution: {}",
            self.transaction_id
        )?;
        Ok(())
    }
}

fn execute_transaction(
    config: &mut SnapshotConfig,
    transaction_address: &Pubkey,
    multisig_program_id: &Pubkey,
    multisig_address: &Pubkey,
) -> Result<ExecuteOutput> {
    let (program_derived_address, _nonce) =
        get_multisig_program_address(multisig_program_id, multisig_address);

    let transaction: serum_multisig::Transaction =
        config.client.get_account_deserialize(transaction_address)?;

    let tx_inner_accounts = TransactionAccounts {
        accounts: transaction.accounts,
        program_id: transaction.program_id,
    };

    let mut accounts = multisig_accounts::ExecuteTransaction {
        multisig: *multisig_address,
        multisig_signer: program_derived_address,
        transaction: *transaction_address,
    }
    .to_account_metas(None);
    accounts.append(&mut tx_inner_accounts.to_account_metas(None));

    let multisig_instruction = Instruction {
        program_id: *multisig_program_id,
        data: multisig_instruction::ExecuteTransaction.data(),
        accounts,
    };
    let signature = config.sign_and_send_transaction(&[multisig_instruction], &[config.signer])?;
    let result = ExecuteOutput {
        transaction_id: signature,
    };
    Ok(result)
}

fn transfer_token(
    config: &mut SnapshotConfig,
    opts: &TransferTokenOpts,
) -> Result<ProposeInstructionOutput> {
    let (multisig_address, _) =
        get_multisig_program_address(opts.multisig_program_id(), opts.multisig_address());

    let instruction = spl_token::instruction::transfer(
        &spl_token::id(),
        opts.from_address(),
        opts.to_address(),
        &multisig_address,
        &[],
        *opts.amount(),
    )?;
    propose_instruction(
        config,
        opts.multisig_program_id(),
        *opts.multisig_address(),
        instruction,
    )
}
