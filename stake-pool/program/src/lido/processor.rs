//! Program state processor

use std::collections::HashSet;

use {
    crate::{
        borsh::try_from_slice_unchecked,
        error::StakePoolError,
        instruction::{Fee, StakePoolInstruction},
        stake_program,
        state::{AccountType, StakePool, ValidatorList, ValidatorStakeInfo},
        AUTHORITY_DEPOSIT, AUTHORITY_WITHDRAW,
    },
    bincode::deserialize,
    borsh::{BorshDeserialize, BorshSchema, BorshSerialize},
    num_traits::FromPrimitive,
    solana_program::{
        account_info::next_account_info,
        account_info::AccountInfo,
        clock::Clock,
        decode_error::DecodeError,
        entrypoint::ProgramResult,
        msg,
        native_token::sol_to_lamports,
        program::{invoke, invoke_signed},
        program_error::PrintProgramError,
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        stake_history::StakeHistory,
        system_instruction,
        sysvar::Sysvar,
    },
    spl_token::state::Mint,
};

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct Lido {
    stake_pool_account: Pubkey,
    members_account: Pubkey,
}

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub enum LidoAccountType {
    Uninitialized,
    Initialized,
}

impl Default for LidoAccountType {
    fn default() -> Self {
        LidoAccountType::Uninitialized
    }
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct LidoMembers {
    /// Account type, must be LidoMembers currently
    pub account_type: LidoAccountType,

    maximum_members: u32,
    list: Vec<Pubkey>,
}

impl LidoMembers {
    pub fn new(maximum_members: u32) -> Self {
        Self {
            account_type: LidoAccountType::Uninitialized,
            maximum_members: maximum_members,
            list: vec![Pubkey::default(); maximum_members as usize],
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.account_type != LidoAccountType::Uninitialized
    }
}

#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum LidoInstruction {
    Initialize {
        stake_pool_account: Pubkey,
        members_list_account: Vec<Pubkey>,
    },
    Deposit,
    Withdraw(u64),
    Upgrade {
        buffer_address: Pubkey,
        spill_address: Pubkey,
    },
}

/// Program state handler.
pub struct Processor;
impl Processor {
    pub fn process_initialize(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        stake_pool_account: Pubkey,
        new_members: &[Pubkey],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let lido_info = next_account_info(account_info_iter)?;
        let members_list_info = next_account_info(account_info_iter)?;
        let rent_info = next_account_info(account_info_iter)?;

        let rent = &Rent::from_account_info(rent_info)?;

        let members_list =
            try_from_slice_unchecked::<LidoMembers>(&members_list_info.data.borrow())?;

        let mut lido = try_from_slice_unchecked::<Lido>(&lido_info.data.borrow())?;

        if members_list.is_initialized() {
            return Err(StakePoolError::AlreadyInUse.into());
        }
        if !rent.is_exempt(members_list_info.lamports(), members_list_info.data_len()) {
            msg!("Members list is not rent-exempt");
            return Err(ProgramError::AccountNotRentExempt);
        }

        if !rent.is_exempt(lido_info.lamports(), lido_info.data_len()) {
            msg!("Lido is not rent-exempt");
            return Err(ProgramError::AccountNotRentExempt);
        }

        let mut members_list = new_members
            .into_iter()
            .fold(members_list, |mut acc, member| {
                acc.list.push(member.to_owned());
                acc
            });
        members_list.account_type = LidoAccountType::Initialized;
        members_list.serialize(&mut *members_list_info.data.borrow_mut())?;

        lido.members_account = *members_list_info.key;
        lido.serialize(&mut *lido_info.data.borrow_mut())?;

        Ok(())
    }

    pub fn process_deposit(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        Ok(())
    }

    pub fn process_upgrade(
        program_id: &Pubkey,
        buffer_address: &Pubkey,
        spill_address: &Pubkey,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let lido_info = next_account_info(account_info_iter)?;
        let members_list_info = next_account_info(account_info_iter)?;

        let signed_accounts = account_info_iter
            .filter(|acc| acc.is_signer)
            .map(|acc| acc.key.to_owned())
            .collect::<HashSet<Pubkey>>();

        let mut lido = try_from_slice_unchecked::<Lido>(&lido_info.data.borrow())?;
        let members_list =
            try_from_slice_unchecked::<LidoMembers>(&members_list_info.data.borrow())?;

        // TODO: Check if lido is valid

        let num_signatures = members_list
            .list
            .iter()
            .filter(|member| signed_accounts.contains(member))
            .count();

        if num_signatures < members_list.list.len() {
            return Err(StakePoolError::SignatureMissing.into());
        }

        let ix = solana_program::bpf_loader_upgradeable::upgrade(
            program_id,
            buffer_address,
            program_id,
            spill_address,
        );
        invoke(&ix, accounts)
    }

    pub fn process_withdraw(
        program_id: &Pubkey,
        pool_tokens: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        Ok(())
    }

    /// Processes [Instruction](enum.Instruction.html).
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        let instruction = LidoInstruction::try_from_slice(input)?;
        match instruction {
            LidoInstruction::Initialize {
                stake_pool_account,
                members_list_account,
            } => Self::process_initialize(
                program_id,
                accounts,
                stake_pool_account,
                &members_list_account,
            ),
            LidoInstruction::Deposit {} => Self::process_deposit(program_id, accounts),
            LidoInstruction::Withdraw(amount) => {
                Self::process_withdraw(program_id, amount, accounts)
            }
            LidoInstruction::Upgrade {
                buffer_address,
                spill_address,
            } => Self::process_upgrade(program_id, &buffer_address, &spill_address, accounts),
        }
    }
}
