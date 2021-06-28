#![allow(clippy::too_many_arguments)]

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};

use solana_program::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    system_program,
    sysvar::{self, stake_history},
};
use spl_stake_pool::stake_program;

use crate::{
    error::LidoError,
    state::FeeDistribution,
    token::{Lamports, StLamports},
};

#[repr(C)]
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum LidoInstruction {
    Initialize {
        #[allow(dead_code)] // but it's not
        fee_distribution: FeeDistribution,
        #[allow(dead_code)] // but it's not
        max_validators: u32,
        #[allow(dead_code)] // but it's not
        max_maintainers: u32,
    },
    /// Deposit a given amount of SOL.
    ///
    /// This can be called by anybody.
    Deposit {
        #[allow(dead_code)] // but it's not
        amount: Lamports,
    },
    /// Move deposits into a new stake account and delegate it to a member validator.
    ///
    /// This does not yet make the new stake account part of the stake pool;
    /// must be followed up by [`DepositActiveStakeToPool`].
    StakeDeposit {
        #[allow(dead_code)] // but it's not
        amount: Lamports,
    },
    /// Update the exchange rate, at the beginning of the epoch.
    ///
    /// This can be called by anybody.
    UpdateExchangeRate,
    Withdraw {
        #[allow(dead_code)] // but it's not
        amount: StLamports,
    },
    DistributeFees,
    ClaimValidatorFees,
    ChangeFeeSpec {
        #[allow(dead_code)] // but it's not
        new_fee_distribution: FeeDistribution,
    },
    AddValidator,
    RemoveValidator,
    AddMaintainer,
    RemoveMaintainer,
    MergeStake {
        #[allow(dead_code)] // but it's not
        from_seed: u64,
        #[allow(dead_code)] // but it's not
        to_seed: u64,
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
///     pub fn try_from_slice<'a, 'b: 'a>(raw: &'b [AccountInfo<'a>]) -> Result<ExampleAccountsInfo, ProgramError>;
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
            )?
            // Per accounts struct you can have one variadic field,
            // prefixed with an ellipsis.
            $(
                ,
                pub ... $multi_account:ident {
                    is_signer: $multi_is_signer:expr,
                    is_writable: $multi_is_writable:tt,
                }
            )?
            // Require a trailing comma.
            ,
        }
    } => {
        #[derive(Debug)]
        pub struct $NameAccountMeta {
            $(
                pub $var_account: Pubkey
            ),*
            // Const accounts are not included here, they are not a variable
            // input, they only show up in program, not in the call.
            $(
                pub $multi_account: Vec<Pubkey>,
            )?
        }

        #[derive(Debug)]
        pub struct $NameAccountInfo<'a, 'b> {
            $(
                pub $var_account: &'a AccountInfo<'b>
            ),*
            $(
                ,
                $(
                    pub $const_account: &'a AccountInfo<'b>
                ),*
            )?
            $(
                pub $multi_account: &'a [&'a AccountInfo<'b>],
            )?
        }

        impl $NameAccountMeta {
            #[must_use]
            pub fn to_vec(&self) -> Vec<AccountMeta> {
                // The mut is used depending on whether we have a variadic account at the end.
                #[allow(unused_mut)]
                let mut result = vec![
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
                ];
                $(
                    for pubkey in &self.$multi_account {
                        result.push(accounts_struct_meta!(
                            pubkey,
                            is_signer: $multi_is_signer,
                            is_writable: $multi_is_writable,
                        ));
                    }
                )?
                result
            }

            pub fn try_from_slice(accounts: &[AccountMeta]) -> Result<$NameAccountMeta, ProgramError> {
                let mut accounts_iter = accounts.iter();

                // Unpack the accounts from the iterator in the same order that
                // they were provided to the macro. Also verify that is_signer
                // and is_writable match their definitions, and return an error
                // if not.
                $(
                    let account = accounts_iter.next().ok_or(ProgramError::NotEnoughAccountKeys)?;
                    if (($is_signer && !account.is_signer)
                        || ($is_writable && !account.is_writable)) {
                        return Err(LidoError::InvalidAccountInfo.into());
                    }
                    let $var_account = account.pubkey;
                )*

                // The const accounts we only skip over, they are not part of
                // the *Meta struct, only of the *Info struct used in the
                // on-chain program.
                $(
                    $(
                        // We need to reference $const_account for macro
                        // expansion to work, but if we do we get an unused
                        // variable warning, so also assign to _ afterwards.
                        let $const_account = accounts_iter.next().ok_or(ProgramError::NotEnoughAccountKeys)?;
                        let _ = $const_account;
                    )*
                )?

                // Check that there are no excess accounts provided.
                if accounts_iter.next().is_some() {
                    return Err(LidoError::TooManyAccountKeys.into());
                }

                let result = $NameAccountMeta {
                    $( $var_account ),*
                };

                Ok(result)
            }
        }

        impl<'a, 'b> $NameAccountInfo<'a, 'b> {
            pub fn try_from_slice(accounts: &'a [AccountInfo<'b>]) -> Result<$NameAccountInfo<'a, 'b>, ProgramError> {
                use solana_program::msg;
                let mut accounts_iter = accounts.iter();

                // Unpack the accounts from the iterator in the same order that
                // they were provided to the macro. Also verify that is_signer
                // and is_writable match their definitions, and return an error
                // if not.
                $(
                    let $var_account = match accounts_iter.next() {
                        Some(account) => account,
                        None => {
                            msg!(
                                "Not enough accounts provided. Expected {}.",
                                stringify!($var_account),
                            );
                            return Err(ProgramError::NotEnoughAccountKeys);
                        }
                    };
                    if $is_signer && !$var_account.is_signer {
                        msg!(
                            "Expected {} ({}) to be a signer, but it is not.",
                            stringify!($var_account),
                            $var_account.key,
                        );
                        return Err(LidoError::InvalidAccountInfo.into());
                    }
                    if $is_writable && !$var_account.is_writable {
                        msg!(
                            "Expected {} ({}) to be writable, but it is not.",
                            stringify!($var_account),
                            $var_account.key,
                        );
                        return Err(LidoError::InvalidAccountInfo.into());
                    }
                )*

                $(
                    $(
                        let $const_account = match accounts_iter.next() {
                            Some(account) => account,
                            None => {
                                msg!(
                                    "Not enough accounts provided. Expected {}.",
                                    stringify!($const_account),
                                );
                                return Err(ProgramError::NotEnoughAccountKeys);
                            }
                        };
                        // Constant accounts (like the system program or rent
                        // sysvar) are never signers or writable.
                        if $const_account.is_signer || $const_account.is_writable {
                            msg!(
                                "Account {} ({}) is unexpectedly writable or signer.",
                                stringify!($const_account),
                                $const_account.key,
                            );
                            return Err(LidoError::InvalidAccountInfo.into());
                        }
                    )*
                )?

                // Check that there are no excess accounts provided.
                if let Some(account) = accounts_iter.next() {
                    msg!(
                        "Instruction was passed more accounts than needed, did not expect {}.",
                        account.key,
                    );
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

#[cfg(test)]
mod test{
    use super::*;
    use solana_program::{account_info::AccountInfo, instruction::AccountMeta};

    #[test]
    fn accounts_struct_only_pub() {
        accounts_struct! {
            TestAccountsMeta, TestAccountsInfo {
                pub s0_w0 { is_signer: false, is_writable: false, },
                pub s1_w0 { is_signer: true, is_writable: false, },
                pub s0_w1 { is_signer: false, is_writable: true, },
                pub s1_w1 { is_signer: true, is_writable: true, },
            }
        }

        let input = TestAccountsMeta {
            s0_w0: Pubkey::new_unique(),
            s1_w0: Pubkey::new_unique(),
            s0_w1: Pubkey::new_unique(),
            s1_w1: Pubkey::new_unique(),
        };
        let account_metas: Vec<AccountMeta> = input.to_vec();

        // Accounts should be serialized in the order that they were declared.
        assert_eq!(account_metas[0].pubkey, input.s0_w0);
        assert_eq!(account_metas[1].pubkey, input.s1_w0);
        assert_eq!(account_metas[2].pubkey, input.s0_w1);
        assert_eq!(account_metas[3].pubkey, input.s1_w1);

        // Signer and write bits should be set correctly.
        assert_eq!(account_metas[0].is_signer, false);
        assert_eq!(account_metas[0].is_writable, false);

        assert_eq!(account_metas[1].is_signer, true);
        assert_eq!(account_metas[1].is_writable, false);

        assert_eq!(account_metas[2].is_signer, false);
        assert_eq!(account_metas[2].is_writable, true);

        assert_eq!(account_metas[3].is_signer, true);
        assert_eq!(account_metas[3].is_writable, true);

        // The `try_from_slice` on the `AccountsMeta` struct should round-trip.
        let roundtripped = TestAccountsMeta::try_from_slice(&account_metas).unwrap();
        assert_eq!(roundtripped.s0_w0, input.s0_w0);
        assert_eq!(roundtripped.s1_w0, input.s1_w0);
        assert_eq!(roundtripped.s0_w1, input.s0_w1);
        assert_eq!(roundtripped.s1_w1, input.s1_w1);

        let mut lamports = vec![0; account_metas.len()];
        let mut datas = vec![vec![]; account_metas.len()];
        let owner = Pubkey::new_unique();
        let executable = false;
        let rent_epoch = 0;
        let mut account_infos: Vec<AccountInfo> = account_metas
            .iter()
            .zip(lamports.iter_mut())
            .zip(datas.iter_mut())
            .map(|((meta, lamports), data)| AccountInfo::new(
                &meta.pubkey,
                meta.is_signer,
                meta.is_writable,
                lamports,
                data,
                &owner,
                executable,
                rent_epoch,
            )).collect();

        let output = TestAccountsInfo::try_from_slice(&account_infos[..]).unwrap();
        assert_eq!(output.s0_w0.key, &input.s0_w0);
        assert_eq!(output.s1_w0.key, &input.s1_w0);
        assert_eq!(output.s0_w1.key, &input.s0_w1);

        // If an account is required to be a signer, but it is not, then parsing should fail.
        account_infos[1].is_signer = false;
        assert_eq!(
            TestAccountsInfo::try_from_slice(&account_infos[..]).err().unwrap(),
            LidoError::InvalidAccountInfo.into(),
        );
        account_infos[1].is_signer = true;

        // If an account is required to be writable, but it is not, then parsing should fail.
        account_infos[2].is_writable = false;
        assert_eq!(
            TestAccountsInfo::try_from_slice(&account_infos[..]).err().unwrap(),
            LidoError::InvalidAccountInfo.into(),
        );
        account_infos[2].is_writable = true;

        // If an account is not required to be a signer or writable, it is fine
        // for the account to still be that though.
        account_infos[0].is_signer = true;
        account_infos[0].is_writable = true;
        assert!(TestAccountsInfo::try_from_slice(&account_infos[..]).is_ok());
    }

    #[test]
    fn accounts_struct_with_const() {
        use solana_program::sysvar::clock;
        accounts_struct! {
            TestAccountsMeta, TestAccountsInfo {
                pub not_sysvar { is_signer: false, is_writable: false, },
                const clock = clock::id(),
            }
        }

        let input = TestAccountsMeta {
            not_sysvar: Pubkey::new_unique(),
        };
        let account_metas: Vec<AccountMeta> = input.to_vec();

        assert_eq!(account_metas[0].pubkey, input.not_sysvar);
        assert_eq!(account_metas[1].pubkey, clock::id());

        // Sysvars are never writable or signers.
        assert_eq!(account_metas[1].is_signer, false);
        assert_eq!(account_metas[1].is_writable, false);
    }
}

accounts_struct! {
    InitializeAccountsMeta, InitializeAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: false,
            is_writable: false,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: false,
        },
        pub treasury_account {
            is_signer: false,
            is_writable: false,
        },
        pub developer_account {
            is_signer: false,
            is_writable: false,
        },
        pub reserve_account {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_rent = sysvar::rent::id(),
        const spl_token = spl_token::id(),
    }
}

pub fn initialize(
    program_id: &Pubkey,
    fee_distribution: FeeDistribution,
    max_validators: u32,
    max_maintainers: u32,
    accounts: &InitializeAccountsMeta,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::Initialize {
        fee_distribution,
        max_validators,
        max_maintainers,
    };
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
            // TODO(glottologist): This will need to be writable again once we
            // start storing metrics about deposits in the Solido state.
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
        pub st_sol_mint {
            is_signer: false,
            is_writable: true,
        },
        pub reserve_account {
            is_signer: false,
            is_writable: true,
        },
        const spl_token = spl_token::id(),
        const system_program = system_program::id(),
    }
}

pub fn deposit(
    program_id: &Pubkey,
    accounts: &DepositAccountsMeta,
    amount: Lamports,
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
    StakeDepositAccountsMeta, StakeDepositAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub maintainer {
            is_signer: true,
            is_writable: false,
        },
        pub reserve {
            is_signer: false,
            is_writable: true,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        // Must be set to the program-derived stake account for the given
        // validator, with seed `stake_accounts_seed_end`.
        pub stake_account_end {
            is_signer: false,
            is_writable: true,
        },
        pub stake_authority {
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

pub fn stake_deposit(
    program_id: &Pubkey,
    accounts: &StakeDepositAccountsMeta,
    amount: Lamports,
) -> Result<Instruction, ProgramError> {
    let init_data = LidoInstruction::StakeDeposit { amount };
    let data = init_data.try_to_vec()?;
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    })
}

accounts_struct! {
    UpdateExchangeRateAccountsMeta, UpdateExchangeRateAccountsInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub reserve {
            is_signer: false,
            is_writable: false,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_clock = sysvar::clock::id(),
        const sysvar_rent = sysvar::rent::id(),
    }
}

pub fn update_exchange_rate(
    program_id: &Pubkey,
    accounts: &UpdateExchangeRateAccountsMeta,
) -> Instruction {
    // There is no reason why `try_to_vec` should fail here.
    let data = LidoInstruction::UpdateExchangeRate.try_to_vec().unwrap();
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data,
    }
}

// Changes the Fee spec
// The new Fee structure is passed by argument and the recipients are passed here
accounts_struct! {
    ChangeFeeSpecMeta, ChangeFeeSpecInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub treasury_account {
            is_signer: false,
            is_writable: false,
        },
        pub developer_account {
            is_signer: false,
            is_writable: false,
        },
    }
}

pub fn change_fee_distribution(
    program_id: &Pubkey,
    new_fee_distribution: FeeDistribution,
    accounts: &ChangeFeeSpecMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::ChangeFeeSpec {
            new_fee_distribution,
        }
        .try_to_vec()?,
    })
}

accounts_struct! {
    AddValidatorMeta, AddValidatorInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        pub validator_fee_st_sol_account {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_clock = sysvar::clock::id(),
        const sysvar_stake_history = sysvar::stake_history::id(),
        const sysvar_stake_program = stake_program::id(),
    }
}

pub fn add_validator(
    program_id: &Pubkey,
    accounts: &AddValidatorMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::AddValidator.try_to_vec()?,
    })
}

accounts_struct! {
    RemoveValidatorMeta, RemoveValidatorInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub validator_vote_account_to_remove {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_clock = sysvar::clock::id(),
        const sysvar_stake_program = stake_program::id(),
    }
}

pub fn remove_validator(
    program_id: &Pubkey,
    accounts: &RemoveValidatorMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::RemoveValidator.try_to_vec()?,
    })
}

accounts_struct! {
    DistributeFeesMeta, DistributeFeesInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub maintainer {
            is_signer: true,
            is_writable: false,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: true,
        },
        pub reserve_authority {
            is_signer: false,
            is_writable: false,
        },
        pub treasury_account {
            is_signer: false,
            is_writable: true,
        },
        pub developer_account {
            is_signer: false,
            is_writable: true,
        },
        const spl_token = spl_token::id(),
    }
}

pub fn distribute_fees(
    program_id: &Pubkey,
    accounts: &DistributeFeesMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::DistributeFees.try_to_vec()?,
    })
}

accounts_struct! {
    ClaimValidatorFeeMeta, ClaimValidatorFeeInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: true,
        },
        pub reserve_authority {
            is_signer: false,
            is_writable: false,
        },
        pub validator_fee_st_sol_account {
            is_signer: false,
            is_writable: true,
        },
        const spl_token = spl_token::id(),
    }
}

pub fn claim_validator_fees(
    program_id: &Pubkey,
    accounts: &ClaimValidatorFeeMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::ClaimValidatorFees.try_to_vec()?,
    })
}

accounts_struct! {
    AddMaintainerMeta, AddMaintainerInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub maintainer {
            is_signer: false,
            is_writable: false,
        },
    }
}

pub fn add_maintainer(
    program_id: &Pubkey,
    accounts: &AddMaintainerMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::AddMaintainer.try_to_vec()?,
    })
}

accounts_struct! {
    RemoveMaintainerMeta, RemoveMaintainerInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub manager {
            is_signer: true,
            is_writable: false,
        },
        pub maintainer {
            is_signer: false,
            is_writable: false,
        },
    }
}

pub fn remove_maintainer(
    program_id: &Pubkey,
    accounts: &RemoveMaintainerMeta,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::RemoveMaintainer.try_to_vec()?,
    })
}

accounts_struct! {
    MergeStakeMeta, MergeStakeInfo {
        pub lido {
            is_signer: false,
            is_writable: true,
        },
        pub validator_vote_account {
            is_signer: false,
            is_writable: false,
        },
        pub from_stake {
            is_signer: false,
            is_writable: true,
        },
        pub to_stake {
            is_signer: false,
            is_writable: true,
        },
        // This instruction doesn’t reference the authority directly, but it
        // invokes one a `MergeStake` instruction that needs the deposit
        // authority to sign.
        pub stake_authority {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_clock = sysvar::clock::id(),
        const stake_history = stake_history::id(),
        const stake_program = stake_program::id(),
    }
}

pub fn merge_stake(
    program_id: &Pubkey,
    from_seed: u64,
    to_seed: u64,
    accounts: &MergeStakeMeta,
) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: LidoInstruction::MergeStake { from_seed, to_seed }
            .try_to_vec()
            .unwrap(), // This should never fail.
    }
}
