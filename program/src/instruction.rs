#![allow(clippy::too_many_arguments)]

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    system_program,
    sysvar::{self, stake_history},
};
use spl_stake_pool::{instruction::StakePoolInstruction, stake_program, state::Fee};

use crate::error::LidoError;

#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum LidoInstruction {
    Initialize,
    /// Deposit with amount
    Deposit {
        #[allow(dead_code)] // but it's not
        amount: u64,
    },
    /// Deposit amount to member validator
    DelegateDeposit {
        #[allow(dead_code)] // but it's not
        amount: u64,
    },
    StakePoolDelegate,
    Withdraw {
        #[allow(dead_code)] // but it's not
        amount: u64,
    },
}

macro_rules! accounts_struct_meta {
    ($pubkey:expr, is_signer: $is_signer:expr, is_writable: true, ) => {
        AccountMeta::new($pubkey, $is_signer)
    };
    ($pubkey:expr, is_signer: $is_signer:expr, is_writable: false, ) => {
        AccountMeta::new_readonly($pubkey, $is_signer)
    };
}

/// Generates two structs for passing accounts by name.
///
/// Using this macro has a few advantages over accepting/parsing a list of
/// accounts manually:
///
///  * There is no risk of making a mistake in the ordering of accounts,
///    or forgetting to update one place after modifying a different place.
///
///  * It forces for every account to consider whether it should be writable or
///    not, and it enforces this when the program is called.
///
///  * It has a shorthand for defining accounts that have a statically known
///    address.
///
/// Example:
/// ```
/// accounts_struct! {
///     ExampleAccountsMeta, ExampleAccountsInfo {
///         frobnicator: { is_signer: true, is_writable: false },
///         sysvar_rent = sysvar::rent::id(),
///     }
/// ```
/// This generates two structs:
/// ```
/// struct ExampleAccountsMeta {
///     frobnicator: Pubkey,
/// }
///
/// impl ExampleAccountsMeta {
///     pub fn to_vec(&self) -> Vec<AccountMeta>;
/// }
///
/// struct ExampleAccountsInfo<'a> {
///     frobnicator: &'a AccountInfo<'a>,
///     sysvar_rent: &'a AccountInfo<'a>,
/// }
///
/// impl ExampleAccountsInfo {
///     pub fn try_from_slice(raw: &'a [AccountInfo<'a>]) -> Result<ExampleAccountsInfo, ProgramError>;
/// }
/// ```
/// Such that the accounts returned by `to_vec` are in the same order that
/// `try_from_slice` expects them. `try_from_slice` furthermore validates that
/// `is_signer` and is_writable` match the definition.
macro_rules! accounts_struct {
    {
        $NameAccountMeta:ident, $NameAccountInfo:ident {
            // We prefix the two cases with "pub" and "const", because otherwise
            // the grammar would be locally ambiguous, and Rust doesn't know
            // which of the two cases it is parsing after seeing only the
            // identifier.
            $(
                pub $var_account:ident {
                    is_signer: $is_signer:expr,
                    is_writable: $is_writable:tt,
                }
            ),*
            // This second part with const accounts is optional, so wrap it in
            // a $(...)? block.
            $(
                ,
                $(
                    const $const_account:ident = $const_value:expr
                ),*
                // Allow an optional trailing comma.
                $(,)?
            )?
        }
    } => {
        pub struct $NameAccountMeta {
            $(
                pub $var_account: Pubkey
            ),*
            // Const accounts are not included here, they are not a variable
            // input, they only show up in program, not in the call.
        }

        pub struct $NameAccountInfo<'a> {
            $(
                pub $var_account: &'a AccountInfo<'a>
            ),*
            $(
                ,
                $(
                    pub $const_account: &'a AccountInfo<'a>
                ),*
            )?
        }

        impl $NameAccountMeta {
            pub fn to_vec(&self) -> Vec<AccountMeta> {
                vec![
                    $(
                        accounts_struct_meta!(
                            self.$var_account,
                            is_signer: $is_signer,
                            is_writable: $is_writable,
                        )
                    ),*
                    $(
                        ,
                        $(
                            AccountMeta::new_readonly(
                                $const_value,
                                false /* is_signer */
                            )
                        ),*
                    )?
                ]
            }
        }

        impl $NameAccountInfo<'_> {
            pub fn try_from_slice<'a>(accounts: &'a [AccountInfo<'a>]) -> Result<$NameAccountInfo<'a>, ProgramError> {
                let accounts_iter = &mut accounts.iter();

                // Unpack the accounts from the iterator in the same order that
                // they were provided to the macro. Also verify that is_signer
                // and is_writable match their definitions, and return an error
                // if not.
                $(
                    let $var_account = next_account_info(accounts_iter)?;
                    if $var_account.is_signer != $is_signer
                        || $var_account.is_writable != $is_writable {
                        return Err(LidoError::InvalidAccountInfo.into());
                    }
                )*

                $(
                    $(
                        let $const_account = next_account_info(accounts_iter)?;
                        // Constant accounts (like the system program or rent
                        // sysvar) are never signers or writable.
                        if $const_account.is_signer || $const_account.is_writable {
                            return Err(LidoError::InvalidAccountInfo.into());
                        }
                    )*
                )?

                // Check that there are no excess accounts provided.
                if accounts_iter.next().is_some() {
                    return Err(LidoError::TooManyAccountKeys.into());
                }

                let result = $NameAccountInfo {
                    $( $var_account ),*
                    $(
                        ,
                        $( $const_account ),*
                    )?
                };

                Ok(result)
            }
        }
    }
}

accounts_struct! {
    InitializeAccountsMeta, InitializeAccountsInfo {
        pub lido {
            is_signer: true,
            is_writable: true,
        },
        pub stake_pool {
            is_signer: true,
            is_writable: false,
        },
        pub owner {
            is_signer: true,
            is_writable: false,
        },
        pub mint_program {
            is_signer: true,
            is_writable: false,
        },
        pub pool_token_to {
            is_signer: false,
            is_writable: false,
        },
        pub fee_token {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_rent = sysvar::rent::id(),
        const spl_token = spl_token::id(),
    }
}

pub fn initialize(
    program_id: &Pubkey,
    accounts: &InitializeAccountsMeta,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Initialize;
    let data = init_data.try_to_vec()?;
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    })
}

accounts_struct! {
    DepositAccountsMeta, DepositAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub stake_pool {
            is_signer: false,
            is_writable: false,
        },
        pub pool_token_to {
            is_signer: false,
            is_writable: false,
        },
        pub owner {
            is_signer: false,
            is_writable: false,
        },
        pub user {
            is_signer: true,
            is_writable: true,
        },
        pub recipient {
            is_signer: false,
            is_writable: true,
        },
        pub mint_program {
            is_signer: false,
            is_writable: true,
        },
        pub reserve_authority {
            is_signer: false,
            is_writable: true,
        },
        const spl_token = spl_token::id(),
        const system_program = system_program::id(),
        const sysvar_rent = sysvar::rent::id(),
    }
}

pub fn deposit(
    program_id: &Pubkey,
    accounts: &DepositAccountsMeta,
    amount: u64,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Deposit { amount };
    let data = init_data.try_to_vec()?;
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    })
}

accounts_struct! {
    DelegateDepositAccountsMeta, DelegateDepositAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub validator {
            is_signer: false,
            is_writable: true,
        },
        pub reserve {
            is_signer: false,
            is_writable: true,
        },
        pub stake {
            is_signer: false,
            is_writable: true,
        },
        pub deposit_authority {
            is_signer: false,
            is_writable: true,
        },
        const sysvar_clock = sysvar::clock::id(),
        const system_program = system_program::id(),
        const sysvar_rent = sysvar::rent::id(),
        const stake_program = stake_program::id(),
        const stake_history = stake_history::id(),
        const stake_program_config = stake_program::config_id(),
    }
}

pub fn delegate_deposit(
    program_id: &Pubkey,
    accounts: &DelegateDepositAccountsMeta,
    amount: u64,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::DelegateDeposit { amount };
    let data = init_data.try_to_vec()?;
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    })
}

accounts_struct! {
    StakePoolDelegateAccountsMeta, StakePoolDelegateAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub validator {
            is_signer: false,
            is_writable: true,
        },
        pub stake {
            is_signer: false,
            is_writable: true,
        },
        pub deposit_authority {
            is_signer: false,
            is_writable: true,
        },
        pub pool_token_to {
            is_signer: false,
            is_writable: true,
        },
        pub stake_pool_program {
            is_signer: false,
            is_writable: false,
        },
        pub stake_pool {
            is_signer: false,
            is_writable: true,
        },
        pub stake_pool_validator_list {
            is_signer: false,
            is_writable: true,
        },
        pub stake_pool_withdraw_authority {
            is_signer: false,
            is_writable: false,
        },
        pub stake_pool_validator_stake_account {
            is_signer: false,
            is_writable: true,
        },
        pub stake_pool_mint {
            is_signer: false,
            is_writable: true,
        },
        const sysvar_clock = sysvar::clock::id(),
        const stake_history = stake_history::id(),
        const system_program = system_program::id(),
        const sysvar_rent = sysvar::rent::id(),
        const spl_token = spl_token::id(),
        const stake_program = stake_program::id(),
    }
}

pub fn stake_pool_delegate(
    program_id: &Pubkey,
    accounts: &StakePoolDelegateAccountsMeta,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::StakePoolDelegate;
    let data = init_data.try_to_vec()?;
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    })
}

accounts_struct! {
    StakePoolDepositAccountsMeta, StakePoolDepositAccountsInfo {
        pub stake_pool {
            is_signer: false,
            is_writable: true,
        },
        pub validator_list_storage {
            is_signer: false,
            is_writable: true,
        },
        pub deposit_authority {
            is_signer: true,
            is_writable: false,
        },
        pub stake_pool_withdraw_authority {
            is_signer: false,
            is_writable: false,
        },
        pub deposit_stake_address {
            is_signer: false,
            is_writable: true,
        },
        pub validator_stake_account {
            is_signer: false,
            is_writable: true,
        },
        pub pool_tokens_to {
            is_signer: false,
            is_writable: true,
        },
        pub pool_mint {
            is_signer: false,
            is_writable: true,
        },
        const spl_token = spl_token::id(),
        const sysvar_clock = sysvar::clock::id(),
        const sysvar_stake_history = sysvar::stake_history::id(),
        const stake_program = stake_program::id(),
    }
}

pub fn stake_pool_deposit(
    program_id: &Pubkey,
    accounts: &StakePoolDepositAccountsMeta,
) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: StakePoolInstruction::Deposit.try_to_vec().unwrap(),
    }
}

pub fn initialize_stake_pool_with_authority(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    manager: &Pubkey,
    staker: &Pubkey,
    validator_list: &Pubkey,
    reserve_stake: &Pubkey,
    pool_mint: &Pubkey,
    manager_pool_account: &Pubkey,
    token_program_id: &Pubkey,
    deposit_authority: &Pubkey,
    fee: Fee,
    max_validators: u32,
) -> Result<Instruction, ProgramError> {
    let init_data = StakePoolInstruction::Initialize {
        fee,
        max_validators,
    };
    let data = init_data.try_to_vec()?;
    let accounts = vec![
        AccountMeta::new(*stake_pool, true),
        AccountMeta::new_readonly(*manager, true),
        AccountMeta::new_readonly(*staker, false),
        AccountMeta::new(*validator_list, false),
        AccountMeta::new_readonly(*reserve_stake, false),
        AccountMeta::new_readonly(*pool_mint, false),
        AccountMeta::new_readonly(*manager_pool_account, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(*token_program_id, false),
        AccountMeta::new_readonly(*deposit_authority, false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}
