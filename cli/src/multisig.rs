use std::fmt;

use anchor_client::solana_sdk::bpf_loader_upgradeable;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::signature::{Keypair, Signer};
use anchor_client::solana_sdk::system_instruction;
use anchor_client::solana_sdk::sysvar;
use anchor_client::{Cluster, Program};
use anchor_lang::prelude::{AccountMeta, ToAccountMetas};
use anchor_lang::{Discriminator, InstructionData};
use borsh::de::BorshDeserialize;
use borsh::ser::BorshSerialize;
use clap::Clap;
use lido::instruction::AddMaintainerMeta;
use lido::instruction::AddValidatorMeta;
use lido::instruction::ChangeFeeSpecMeta;
use lido::instruction::CreateValidatorStakeAccountMeta;
use lido::instruction::LidoInstruction;
use lido::instruction::RemoveMaintainerMeta;
use lido::state::FeeDistribution;
use lido::state::FeeRecipients;
use lido::state::Lido;
use multisig::accounts as multisig_accounts;
use multisig::instruction as multisig_instruction;
use serde::Serialize;
use solana_client::rpc_client::RpcClient;

use crate::helpers::get_anchor_program;
use crate::helpers::get_solido;
use crate::util::PubkeyBase58;
use crate::Config;
use crate::{print_output, OutputMode};

#[derive(Clap, Debug)]
pub struct MultisigOpts {
    /// Address of the Multisig program.
    #[clap(long)]
    multisig_program_id: Pubkey,

    #[clap(subcommand)]
    subcommand: SubCommand,
}

#[derive(Clap, Debug)]
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
}

#[derive(Clap, Debug)]
struct CreateMultisigOpts {
    /// How many signatures are needed to approve a transaction.
    #[clap(long)]
    threshold: u64,

    /// The public keys of the multisig owners, who can sign transactions.
    #[clap(long = "owner", required = true)]
    owners: Vec<Pubkey>,
}

impl CreateMultisigOpts {
    /// Perform a few basic checks to rule out nonsensical multisig settings.
    ///
    /// Exits if validation fails.
    fn validate_or_exit(&self) {
        if self.threshold > self.owners.len() as u64 {
            println!("Threshold must be at most the number of owners.");
            std::process::exit(1);
        }
        if self.threshold == 0 {
            println!("Threshold must be at least 1.");
            std::process::exit(1);
        }
    }
}

#[derive(Clap, Debug)]
struct ProposeUpgradeOpts {
    /// The multisig account whose owners should vote for this proposal.
    #[clap(long)]
    multisig_address: Pubkey,

    /// The program id of the program to upgrade.
    #[clap(long)]
    program_address: Pubkey,

    /// The address that holds the new program data.
    #[clap(long)]
    buffer_address: Pubkey,

    /// Account that will receive leftover funds from the buffer account.
    #[clap(long)]
    spill_address: Pubkey,
}

#[derive(Clap, Debug)]
struct ProposeChangeMultisigOpts {
    /// The multisig account to modify.
    #[clap(long)]
    multisig_address: Pubkey,

    // The fields below are the same as for `CreateMultisigOpts`, but we can't
    // just embed a `CreateMultisigOpts`, because Clap does not support that.
    /// How many signatures are needed to approve a transaction.
    #[clap(long)]
    threshold: u64,

    /// The public keys of the multisig owners, who can sign transactions.
    #[clap(long = "owner", required = true)]
    owners: Vec<Pubkey>,
}

impl From<&ProposeChangeMultisigOpts> for CreateMultisigOpts {
    fn from(opts: &ProposeChangeMultisigOpts) -> CreateMultisigOpts {
        CreateMultisigOpts {
            threshold: opts.threshold,
            owners: opts.owners.clone(),
        }
    }
}

#[derive(Clap, Debug)]
struct ShowMultisigOpts {
    /// The multisig account to display.
    #[clap(long)]
    multisig_address: Pubkey,
}

#[derive(Clap, Debug)]
struct ShowTransactionOpts {
    /// The transaction to display.
    #[clap(long)]
    transaction_address: Pubkey,

    /// The transaction to display.
    #[clap(long)]
    solido_program_id: Option<Pubkey>,
}

#[derive(Clap, Debug)]
struct ApproveOpts {
    /// The multisig account whose owners should vote for this proposal.
    // TODO: Can be omitted, we can obtain it from the transaction account.
    #[clap(long)]
    multisig_address: Pubkey,

    /// The transaction to approve.
    #[clap(long)]
    transaction_address: Pubkey,
}

#[derive(Clap, Debug)]
struct ExecuteTransactionOpts {
    /// The multisig account whose owners approved this transaction.
    // TODO: Can be omitted, we can obtain it from the transaction account.
    #[clap(long)]
    multisig_address: Pubkey,

    /// The transaction to execute.
    #[clap(long)]
    transaction_address: Pubkey,
}

pub fn main(
    config: Config,
    cluster: Cluster,
    output_mode: OutputMode,
    multisig_opts: MultisigOpts,
) {
    let program = get_anchor_program(
        cluster,
        config.fee_payer,
        &multisig_opts.multisig_program_id,
    );
    match multisig_opts.subcommand {
        SubCommand::CreateMultisig(cmd_opts) => {
            let output = create_multisig(program, cmd_opts);
            print_output(output_mode, &output);
        }
        SubCommand::ShowMultisig(cmd_opts) => {
            let output = show_multisig(program, cmd_opts);
            print_output(output_mode, &output);
        }
        SubCommand::ShowTransaction(cmd_opts) => {
            let output = show_transaction(program, cmd_opts);
            print_output(output_mode, &output);
        }
        SubCommand::ProposeUpgrade(cmd_opts) => {
            let output = propose_upgrade(program, cmd_opts);
            print_output(output_mode, &output);
        }
        SubCommand::ProposeChangeMultisig(cmd_opts) => {
            let output = propose_change_multisig(program, cmd_opts);
            print_output(output_mode, &output);
        }
        SubCommand::Approve(cmd_opts) => approve(program, cmd_opts),
        SubCommand::ExecuteTransaction(cmd_opts) => execute_transaction(program, cmd_opts),
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
    multisig_address: PubkeyBase58,
    multisig_program_derived_address: PubkeyBase58,
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

fn create_multisig(program: Program, opts: CreateMultisigOpts) -> CreateMultisigOutput {
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
        get_multisig_program_address(&program.id(), &multisig_account.pubkey());

    program
        .request()
        // Create the program-owned account that will hold the multisig data,
        // and fund it from the payer account to make it rent-exempt.
        .instruction(system_instruction::create_account(
            &program.payer(),
            &multisig_account.pubkey(),
            // 352 bytes should be sufficient to hold a multisig state with 10
            // owners. Get the minimum rent-exempt balance for that, and
            // initialize the account with it, funded by the payer.
            // TODO: Ask for confirmation from the user first.
            program
                .rpc()
                .get_minimum_balance_for_rent_exemption(352)
                .expect("Failed to obtain minimum rent-exempt balance."),
            352,
            &program.id(),
        ))
        // Creating the account must be signed by the account itself.
        .signer(&multisig_account)
        .accounts(multisig_accounts::CreateMultisig {
            multisig: multisig_account.pubkey(),
            rent: sysvar::rent::ID,
        })
        .args(multisig_instruction::CreateMultisig {
            owners: opts.owners,
            threshold: opts.threshold,
            nonce,
        })
        .send()
        .expect("Failed to send transaction.");

    CreateMultisigOutput {
        multisig_address: multisig_account.pubkey().into(),
        multisig_program_derived_address: program_derived_address.into(),
    }
}

#[derive(Serialize)]
struct ShowMultisigOutput {
    multisig_program_derived_address: PubkeyBase58,
    threshold: u64,
    owners: Vec<PubkeyBase58>,
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

fn show_multisig(program: Program, opts: ShowMultisigOpts) -> ShowMultisigOutput {
    let multisig: multisig::Multisig = program
        .account(opts.multisig_address)
        .expect("Failed to read multisig state from account.");

    let (program_derived_address, _nonce) =
        get_multisig_program_address(&program.id(), &opts.multisig_address);

    ShowMultisigOutput {
        multisig_program_derived_address: program_derived_address.into(),
        threshold: multisig.threshold,
        owners: multisig.owners.iter().map(PubkeyBase58::from).collect(),
    }
}

#[derive(Serialize)]
struct ShowTransactionSigner {
    owner: PubkeyBase58,
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
        program_to_upgrade: PubkeyBase58,
        program_data_address: PubkeyBase58,
        buffer_address: PubkeyBase58,
        spill_address: PubkeyBase58,
    },
    MultisigChange {
        threshold: u64,
        owners: Vec<PubkeyBase58>,
    },
    SolidoInstruction(SolidoInstruction),
    InvalidSolidoInstruction,
    Unrecognized,
}

#[derive(Serialize)]
enum SolidoInstruction {
    CreateValidatorStakeAccount {
        solido_instance: Pubkey,
        manager: Pubkey,
        stake_pool_program: Pubkey,
        stake_pool: Pubkey,
        funder: Pubkey,
        stake_account: Pubkey,
        validator_vote: Pubkey,
    },
    AddValidator {
        solido_instance: Pubkey,
        manager: Pubkey,
        stake_pool_program: Pubkey,
        stake_pool: Pubkey,
        stake_account: Pubkey,
        validator_token_account: Pubkey,
    },
    AddMaintainer {
        solido_instance: Pubkey,
        manager: Pubkey,
        maintainer: Pubkey,
    },
    RemoveMaintainer {
        solido_instance: Pubkey,
        manager: Pubkey,
        maintainer: Pubkey,
    },
    ChangeFee {
        current_solido: Box<Lido>,
        fee_distribution: FeeDistribution,

        solido_instance: Pubkey,
        manager: Pubkey,
        fee_recipients: FeeRecipients,
    },
}

#[derive(Serialize)]
struct ShowTransactionOutput {
    multisig_address: PubkeyBase58,
    did_execute: bool,
    signers: ShowTransactionSigners,
    // TODO: when using --output-json, the addresses in here get serialized as
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
            ParsedInstruction::MultisigChange { threshold, owners } => {
                writeln!(
                    f,
                    "  This is a multisig::set_owners_and_change_threshold instruction."
                )?;
                writeln!(
                    f,
                    "    New threshold: {} out of {}",
                    threshold,
                    owners.len()
                )?;
                writeln!(f, "    New owners:")?;
                for owner_pubkey in owners {
                    writeln!(f, "      {}", owner_pubkey)?;
                }
            }
            ParsedInstruction::SolidoInstruction(solido_instruction) => {
                write!(f, "  This is a Solido instruction. ")?;
                match solido_instruction {
                    SolidoInstruction::CreateValidatorStakeAccount {
                        solido_instance,
                        manager,
                        stake_pool_program,
                        stake_pool,
                        funder,
                        stake_account,
                        validator_vote,
                    } => {
                        writeln!(f, "It creates a validator stake account.")?;
                        writeln!(f, "    Solido instance:     {}", solido_instance)?;
                        writeln!(f, "    Manager:             {}", manager)?;
                        writeln!(f, "    Stake pool program:  {}", stake_pool_program)?;
                        writeln!(f, "    Stake pool instance: {}", stake_pool)?;
                        writeln!(f, "    Funder:              {}", funder)?;
                        writeln!(f, "    Stake account:       {}", stake_account)?;
                        writeln!(f, "    Validator vote:      {}", validator_vote)?;
                    }
                    SolidoInstruction::AddValidator {
                        solido_instance,
                        manager,
                        stake_account,
                        stake_pool,
                        stake_pool_program,
                        validator_token_account,
                    } => {
                        writeln!(f, "It adds a validator to Solido")?;
                        writeln!(f, "    Solido instance:       {}", solido_instance)?;
                        writeln!(f, "    Manager:               {}", manager)?;
                        writeln!(f, "    Stake pool program:    {}", stake_pool_program)?;
                        writeln!(f, "    Stake pool instance:   {}", stake_pool)?;
                        writeln!(f, "    Stake account:         {}", stake_account)?;
                        writeln!(f, "    Validator fee account: {}", validator_token_account)?;
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
                    SolidoInstruction::ChangeFee {
                        current_solido,
                        fee_distribution,

                        solido_instance,
                        manager,
                        fee_recipients,
                    } => {
                        writeln!(f, "It changes the fee structure and distribution")?;
                        writeln!(f, "    Solido instance:       {}", solido_instance)?;
                        writeln!(f, "    Manager:               {}", manager)?;
                        writeln!(f)?;
                        print_changed_fee(f, &current_solido, &fee_distribution)?;
                        print_changed_recipients(f, &current_solido, &fee_recipients)?;
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
    if before != after {
        writeln!(f, "   {}: {:>5} -> {:>5}", param_name, before, after)?;
    } else {
        writeln!(f, "   {}:          {:>5}", param_name, after)?;
    }
    Ok(())
}

fn print_changed_fee(
    f: &mut fmt::Formatter,
    current_solido: &Lido,
    fee_disribution: &FeeDistribution,
) -> fmt::Result {
    let current_sum = current_solido.fee_distribution.sum();
    let new_sum = fee_disribution.sum();
    changed_fee(
        f,
        current_solido.fee_distribution.insurance_fee,
        fee_disribution.insurance_fee,
        current_sum,
        new_sum,
        "insurance",
    )?;
    changed_fee(
        f,
        current_solido.fee_distribution.treasury_fee,
        fee_disribution.treasury_fee,
        current_sum,
        new_sum,
        "treasury",
    )?;
    changed_fee(
        f,
        current_solido.fee_distribution.validation_fee,
        fee_disribution.validation_fee,
        current_sum,
        new_sum,
        "validation",
    )?;
    changed_fee(
        f,
        current_solido.fee_distribution.manager_fee,
        fee_disribution.manager_fee,
        current_sum,
        new_sum,
        "manager",
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
        &current_solido.fee_recipients.insurance_account,
        &fee_recipients.insurance_account,
        "insurance",
    )?;
    changed_addr(
        f,
        &current_solido.fee_recipients.treasury_account,
        &fee_recipients.treasury_account,
        "treasury",
    )?;
    changed_addr(
        f,
        &current_solido.fee_recipients.manager_account,
        &fee_recipients.manager_account,
        "manager",
    )?;
    Ok(())
}

fn changed_addr(
    f: &mut fmt::Formatter,
    current_addr: &Pubkey,
    new_addr: &Pubkey,
    param_name: &str,
) -> fmt::Result {
    if current_addr != new_addr {
        writeln!(f, "   {}: {} -> {}", param_name, new_addr, current_addr,)?;
    } else {
        writeln!(f, "   {}: {}", param_name, new_addr)?;
    }
    Ok(())
}

fn show_transaction(program: Program, opts: ShowTransactionOpts) -> ShowTransactionOutput {
    let transaction: multisig::Transaction = program
        .account(opts.transaction_address)
        .expect("Failed to read transaction data from account.");

    // Also query the multisig, to get the owner public keys, so we can display
    // exactly who voted.
    // Note: Although these are separate reads, the result will still be
    // consistent, because the transaction account must be owned by the Multisig
    // program, and the multisig program never modifies the
    // `transaction.multisig` field.
    let multisig: multisig::Multisig = program
        .account(transaction.multisig)
        .expect("Failed to read multisig state from account.");

    let signers = if transaction.owner_set_seqno == multisig.owner_set_seqno {
        // If the owners did not change, match up every vote with its owner.
        ShowTransactionSigners::Current {
            signers: multisig
                .owners
                .iter()
                .cloned()
                .zip(transaction.signers.iter())
                .map(|(owner, did_sign)| ShowTransactionSigner {
                    owner: owner.into(),
                    did_sign: *did_sign,
                })
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
            program_data_address: instr.accounts[0].pubkey.into(),
            program_to_upgrade: instr.accounts[1].pubkey.into(),
            buffer_address: instr.accounts[2].pubkey.into(),
            spill_address: instr.accounts[3].pubkey.into(),
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
    else if instr.program_id == program.id()
        && instr.data[..8] == [122, 49, 168, 177, 231, 28, 167, 204]
    {
        if let Ok(instr) =
            multisig_instruction::SetOwnersAndChangeThreshold::try_from_slice(&instr.data[8..])
        {
            ParsedInstruction::MultisigChange {
                threshold: instr.threshold,
                owners: instr.owners.iter().map(PubkeyBase58::from).collect(),
            }
        } else {
            ParsedInstruction::Unrecognized
        }
    } else if Some(instr.program_id) == opts.solido_program_id {
        // Probably a Solido instruction
        match try_parse_solido_instruction(&instr, &program.rpc()) {
            Ok(instr) => instr,
            Err(err) => {
                eprintln!("Warning: failed to parse Solido instruction: {}", err);
                ParsedInstruction::InvalidSolidoInstruction
            }
        }
    } else {
        ParsedInstruction::Unrecognized
    };

    ShowTransactionOutput {
        multisig_address: transaction.multisig.into(),
        did_execute: transaction.did_execute,
        signers,
        instruction: instr,
        parsed_instruction: parsed_instr,
    }
}

fn try_parse_solido_instruction(
    instr: &Instruction,
    rpc_client: &RpcClient,
) -> Result<ParsedInstruction, crate::Error> {
    let instruction: LidoInstruction = BorshDeserialize::deserialize(&mut instr.data.as_slice())?;
    Ok(match instruction {
        LidoInstruction::DistributeFees => todo!(),
        LidoInstruction::ChangeFeeSpec {
            new_fee_distribution,
        } => {
            let accounts = ChangeFeeSpecMeta::try_from_slice(&instr.accounts)?;
            let current_solido = get_solido(&rpc_client, &accounts.lido)?;
            ParsedInstruction::SolidoInstruction(SolidoInstruction::ChangeFee {
                current_solido: Box::new(current_solido),
                fee_distribution: new_fee_distribution,

                solido_instance: accounts.lido,
                manager: accounts.manager,
                fee_recipients: FeeRecipients {
                    insurance_account: accounts.insurance_account,
                    treasury_account: accounts.treasury_account,
                    manager_account: accounts.manager_fee_account,
                },
            })
        }
        LidoInstruction::CreateValidatorStakeAccount => {
            let accounts = CreateValidatorStakeAccountMeta::try_from_slice(&instr.accounts)?;
            ParsedInstruction::SolidoInstruction(SolidoInstruction::CreateValidatorStakeAccount {
                stake_account: accounts.stake_account,
                validator_vote: accounts.validator,
                solido_instance: accounts.lido,
                manager: accounts.manager,
                stake_pool_program: accounts.stake_pool_program,
                stake_pool: accounts.stake_pool,
                funder: accounts.funder,
            })
        }
        LidoInstruction::AddValidator => {
            let accounts = AddValidatorMeta::try_from_slice(&instr.accounts)?;
            ParsedInstruction::SolidoInstruction(SolidoInstruction::AddValidator {
                stake_account: accounts.stake_account,
                solido_instance: accounts.lido,
                manager: accounts.manager,
                stake_pool_program: accounts.stake_pool_program,
                stake_pool: accounts.stake_pool,
                validator_token_account: accounts.validator_token_account,
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

#[derive(Serialize)]
pub struct ProposeInstructionOutput {
    transaction_address: PubkeyBase58,
}

impl fmt::Display for ProposeInstructionOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Transaction address: {}", self.transaction_address)
    }
}

/// Propose the given instruction to be approved and executed by the multisig.
pub fn propose_instruction(
    program: Program,
    multisig_address: Pubkey,
    instruction: Instruction,
) -> ProposeInstructionOutput {
    // The Multisig program expects `multisig::TransactionAccount` instead of
    // `solana_sdk::AccountMeta`. The types are structurally identical,
    // but not nominally, so we need to convert these.
    let accounts: Vec<_> = instruction
        .accounts
        .iter()
        .map(multisig::TransactionAccount::from)
        .collect();

    // The transaction is stored by the Multisig program in yet another account,
    // that we create just for this transaction. We don't save the private key
    // because the account will be owned by the multisig program later; its
    // funds will be locked forever.
    let transaction_account = Keypair::new();

    // We are going to build a dummy version of the `multisig::Transaction`, to
    // compute its size, which we need to allocate an account for it. And to
    // build the dummy transaction, we need to know how many owners the multisig
    // has.
    let multisig: multisig::Multisig = program
        .account(multisig_address)
        .expect("Failed to read multisig state from account.");

    // Build the data that the account will hold, just to measure its size, so
    // we can allocate an account of the right size.
    let dummy_tx = multisig::Transaction {
        multisig: multisig_address,
        program_id: instruction.program_id,
        accounts: accounts.clone(),
        data: instruction.data.clone(),
        signers: multisig
            .owners
            .iter()
            .map(|a| a == &program.payer())
            .collect(),
        did_execute: false,
        owner_set_seqno: multisig.owner_set_seqno,
    };

    // The space used is the serialization of the transaction itself, plus the
    // discriminator that Anchor uses to identify the account type.
    let mut account_bytes = multisig::Transaction::discriminator().to_vec();
    dummy_tx
        .serialize(&mut account_bytes)
        .expect("Failed to serialize dummy transaction.");
    let tx_account_size = account_bytes.len();

    program
        .request()
        // Create the program-owned account that will hold the transaction data,
        // and fund it from the payer account to make it rent-exempt.
        .instruction(system_instruction::create_account(
            &program.payer(),
            &transaction_account.pubkey(),
            // TODO: Ask for confirmation from the user first before funding the
            // account.
            program
                .rpc()
                .get_minimum_balance_for_rent_exemption(tx_account_size)
                .expect("Failed to obtain minimum rent-exempt balance."),
            tx_account_size as u64,
            &program.id(),
        ))
        // Creating the account must be signed by the account itself.
        .signer(&transaction_account)
        .accounts(multisig_accounts::CreateTransaction {
            multisig: multisig_address,
            transaction: transaction_account.pubkey(),
            // For convenience, assume that the party that signs the proposal
            // transaction is a member of the multisig owners, and use it as the
            // proposer.
            proposer: program.payer(),
            rent: sysvar::rent::ID,
        })
        .args(multisig_instruction::CreateTransaction {
            pid: instruction.program_id,
            accs: accounts,
            data: instruction.data,
        })
        .send()
        .expect("Failed to send transaction.");

    ProposeInstructionOutput {
        transaction_address: transaction_account.pubkey().into(),
    }
}

fn propose_upgrade(program: Program, opts: ProposeUpgradeOpts) -> ProposeInstructionOutput {
    let (program_derived_address, _nonce) =
        get_multisig_program_address(&program.id(), &opts.multisig_address);

    let upgrade_instruction = bpf_loader_upgradeable::upgrade(
        &opts.program_address,
        &opts.buffer_address,
        // The upgrade authority is the multisig-derived program address.
        &program_derived_address,
        &opts.spill_address,
    );

    propose_instruction(program, opts.multisig_address, upgrade_instruction)
}

fn propose_change_multisig(
    program: Program,
    opts: ProposeChangeMultisigOpts,
) -> ProposeInstructionOutput {
    // Check that the new settings make sense. This check is shared between a
    // new multisig or altering an existing one.
    CreateMultisigOpts::from(&opts).validate_or_exit();

    let (program_derived_address, _nonce) =
        get_multisig_program_address(&program.id(), &opts.multisig_address);

    let change_data = multisig_instruction::SetOwnersAndChangeThreshold {
        owners: opts.owners,
        threshold: opts.threshold,
    };
    let change_addrs = multisig_accounts::Auth {
        multisig: opts.multisig_address,
        multisig_signer: program_derived_address,
    };

    let override_is_signer = None;
    let change_instruction = Instruction {
        program_id: program.id(),
        data: change_data.data(),
        accounts: change_addrs.to_account_metas(override_is_signer),
    };

    propose_instruction(program, opts.multisig_address, change_instruction)
}

fn approve(program: Program, opts: ApproveOpts) {
    program
        .request()
        .accounts(multisig_accounts::Approve {
            multisig: opts.multisig_address,
            transaction: opts.transaction_address,
            // The owner that signs the multisig proposed transaction, should be
            // the public key that signs the entire approval transaction (which
            // is also the payer).
            owner: program.payer(),
        })
        .args(multisig_instruction::Approve)
        .send()
        .expect("Failed to send transaction.");
}

/// Wrapper type needed to implement `ToAccountMetas`.
struct TransactionAccounts {
    accounts: Vec<multisig::TransactionAccount>,
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
                // instruction that calls the multisig::execute_transaction, the
                // signers of the inner instruction should not be signers of the
                // outer one.
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

fn execute_transaction(program: Program, opts: ExecuteTransactionOpts) {
    let (program_derived_address, _nonce) =
        get_multisig_program_address(&program.id(), &opts.multisig_address);

    // The wrapped instruction can reference additional accounts, that we need
    // to specify in this `multisig::execute_transaction` instruction as well,
    // otherwise `invoke_signed` can fail in `execute_transaction`.
    let transaction: multisig::Transaction = program
        .account(opts.transaction_address)
        .expect("Failed to read transaction data from account.");
    let tx_inner_accounts = TransactionAccounts {
        accounts: transaction.accounts,
        program_id: transaction.program_id,
    };

    program
        .request()
        .accounts(multisig_accounts::ExecuteTransaction {
            multisig: opts.multisig_address,
            multisig_signer: program_derived_address,
            transaction: opts.transaction_address,
        })
        .accounts(tx_inner_accounts)
        .args(multisig_instruction::ExecuteTransaction)
        .send()
        .expect("Failed to send transaction.");
}
