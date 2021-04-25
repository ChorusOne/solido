use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    system_program, sysvar,
};

#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum LidoInstruction {
    Initialize,
    /// Deposit with amount
    Deposit {
        amount: u64,
    },
    /// Deposit amount to member validator
    DelegateDeposit {
        amount: u64,
    },
    Withdraw {
        amount: u64,
    },
}

pub fn initialize(
    program_id: &Pubkey,
    lido: &Pubkey,
    stake_pool: &Pubkey,
    owner: &Pubkey,
    mint_program: &Pubkey,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Initialize;
    let data = init_data.try_to_vec()?;
    let accounts = vec![
        AccountMeta::new(*lido, true),
        AccountMeta::new_readonly(*stake_pool, true),
        AccountMeta::new_readonly(*owner, false),
        AccountMeta::new(*mint_program, false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

// let account_info_iter = &mut accounts.iter();
//         // Lido
//         let lido_info = next_account_info(account_info_iter)?;
//         // Stake pool
//         let stake_pool = next_account_info(account_info_iter)?;
//         // Owner program
//         let owner_info = next_account_info(account_info_iter)?;
//         // User account
//         let user_info = next_account_info(account_info_iter)?;
//         // Recipient account
//         let lsol_recipient_info = next_account_info(account_info_iter)?;
//         // Token minter
//         let lsol_mint_info = next_account_info(account_info_iter)?;
//         // Token program account (SPL Token Program)
//         let token_program_info = next_account_info(account_info_iter)?;
//         // Lido authority account
//         let authority_info = next_account_info(account_info_iter)?;
//         // Reserve account
//         let reserve_account_info = next_account_info(account_info_iter)?;
//         // System program
//         let system_program_info = next_account_info(account_info_iter)?;

pub fn deposit(
    program_id: &Pubkey,
    lido: &Pubkey,
    stake_pool: &Pubkey,
    owner: &Pubkey,
    user: &Pubkey,
    recipient: &Pubkey,
    mint_program: &Pubkey,
    authority: &Pubkey,
    reserve_account: &Pubkey,
    amount: u64,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Deposit { amount: amount };
    let data = init_data.try_to_vec()?;
    let accounts = vec![
        AccountMeta::new(*lido, false),
        AccountMeta::new_readonly(*stake_pool, false),
        AccountMeta::new_readonly(*owner, false),
        AccountMeta::new(*user, true),
        AccountMeta::new(*recipient, false),
        AccountMeta::new(*mint_program, false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(*authority, false),
        AccountMeta::new(*reserve_account, false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}
