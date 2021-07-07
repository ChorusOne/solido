//! Contains a utility macro that makes it safer to work with lists of accounts.

/// Implementation detail of [`accounts_struct`].
#[macro_export]
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
/// The macro accepts three types of field:
///
///  * User-specified accounts, with `pub`.
///
///  * Optionally, one or more accounts with a fixed address, with `const`. These
///    are not part of the `Meta` struct, because their address is known, so the
///    caller does not need to provide it, but they are part of the `Info` struct,
///    because the program does need to access them.
///
///  * Optionally, a vector with a variable number of accounts, with `pub ...`.
///
/// Example:
/// ```
/// # use lido::{accounts_struct, error::LidoError};
/// # use lido::{accounts_struct_meta};
/// # use solana_program::{pubkey::Pubkey, account_info::AccountInfo, instruction::AccountMeta, program_error::ProgramError, sysvar};
/// accounts_struct! {
///     ExampleAccountsMeta, ExampleAccountsInfo {
///         pub frobnicator { is_signer: true, is_writable: false, },
///         const sysvar_rent = sysvar::rent::id(),
///         pub ...widgets { is_signer: false, is_writable: true, },
///     }
/// }
/// ```
/// This generates two structs:
/// ```
/// # use solana_program::{pubkey::Pubkey, account_info::AccountInfo, instruction::AccountMeta, program_error::ProgramError};
/// struct ExampleAccountsMeta {
///     frobnicator: Pubkey,
///     widgets: Vec<Pubkey>,
/// }
///
/// impl ExampleAccountsMeta {
///     pub fn to_vec(&self) -> Vec<AccountMeta> {
///         # unimplemented!("Body omitted in example.")
///     }
/// }
///
/// struct ExampleAccountsInfo<'a> {
///     frobnicator: &'a AccountInfo<'a>,
///     sysvar_rent: &'a AccountInfo<'a>,
///     widgets: &'a [AccountInfo<'a>],
/// }
///
/// impl<'a> ExampleAccountsInfo<'a> {
///     pub fn try_from_slice<'b: 'a>(raw: &'b [AccountInfo<'a>]) -> Result<ExampleAccountsInfo<'a>, ProgramError> {
///         # unimplemented!("Body omitted in example.")
///     }
/// }
/// ```
/// Such that the accounts returned by `to_vec` are in the same order that
/// `try_from_slice` expects them. `try_from_slice` furthermore validates that
/// `is_signer` and `is_writable` match the definition.
#[macro_export]
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
                    // For now, only allow non-signer writable accounts in
                    // the variadic parts, so we don't have to implement
                    // verification checks. We can add that when we need it.
                    is_signer: false,
                    is_writable: true,
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
                ,
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
                ,
                pub $multi_account: &'a [AccountInfo<'b>],
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
                            *pubkey,
                            is_signer: false,
                            is_writable: true,
                        ));
                    }
                )?
                result
            }

            // The `AccountsMeta::try_from_slice` function is not always used, we
            // have the pair of `AccountsMeta::to_vec` and `AccountsInfo::try_from_slice`
            // that should always be used. This one doesn’t have to be used.
            #[allow(dead_code)]
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

                $(
                    // Collect all remaining pubkeys in a vector.
                    let mut $multi_account = Vec::new();
                    while let Some(meta) = accounts_iter.next() {
                        $multi_account.push(meta.pubkey);
                    }
                )?

                // Check that there are no excess accounts provided.
                if accounts_iter.next().is_some() {
                    return Err(LidoError::TooManyAccountKeys.into());
                }

                let result = $NameAccountMeta {
                    $( $var_account ),*
                    $( , $multi_account )?
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
                        // Also confirm that the account passed is the hard-coded
                        // one that we expected.
                        if *$const_account.key != $const_value {
                            msg!(
                                "Account {} was expected to be set to {}, but found {} instead.",
                                stringify!($const_account),
                                $const_value,
                                $const_account.key,
                            );
                            return Err(LidoError::InvalidAccountInfo.into());
                        }
                    )*
                )?

                $(
                    // Collect all remaining AccountInfos in a slice.
                    let $multi_account = accounts_iter.as_slice();

                    // Confirm that they are writable.
                    for account in $multi_account {
                        if !account.is_writable {
                            msg!(
                                "Account {} ({}) should have been writable.",
                                stringify!($multi_account),
                                account.key,
                            );
                            return Err(LidoError::InvalidAccountInfo.into());
                        }
                    }

                    // Also consume the iterator, so the no-excess-accounts check
                    // below does not trigger.
                    for _ in 0..$multi_account.len() {
                        accounts_iter.next();
                    }
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
                    $( , $multi_account )?
                };

                Ok(result)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::error::LidoError;
    use solana_program::{
        account_info::AccountInfo, instruction::AccountMeta, program_error::ProgramError,
        pubkey::Pubkey,
    };

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
            .map(|((meta, lamports), data)| {
                AccountInfo::new(
                    &meta.pubkey,
                    meta.is_signer,
                    meta.is_writable,
                    lamports,
                    data,
                    &owner,
                    executable,
                    rent_epoch,
                )
            })
            .collect();

        let output = TestAccountsInfo::try_from_slice(&account_infos[..]).unwrap();
        assert_eq!(output.s0_w0.key, &input.s0_w0);
        assert_eq!(output.s1_w0.key, &input.s1_w0);
        assert_eq!(output.s0_w1.key, &input.s0_w1);

        // If an account is required to be a signer, but it is not, then parsing should fail.
        account_infos[1].is_signer = false;
        assert_eq!(
            TestAccountsInfo::try_from_slice(&account_infos[..])
                .err()
                .unwrap(),
            LidoError::InvalidAccountInfo.into(),
        );
        account_infos[1].is_signer = true;

        // If an account is required to be writable, but it is not, then parsing should fail.
        account_infos[2].is_writable = false;
        assert_eq!(
            TestAccountsInfo::try_from_slice(&account_infos[..])
                .err()
                .unwrap(),
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
                const sysvar_clock = clock::id(),
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

        let key0 = Pubkey::new_unique();
        let key_clock = clock::id();
        let is_signer = false;
        let is_writable = false;
        let mut lamports0 = 0;
        let mut lamports1 = 0;
        let mut data0 = vec![];
        let mut data1 = vec![];
        let owner = Pubkey::new_unique();
        let executable = false;
        let rent_epoch = 0;
        let mut account_infos = vec![
            AccountInfo::new(
                &key0,
                is_signer,
                is_writable,
                &mut lamports0,
                &mut data0,
                &owner,
                executable,
                rent_epoch,
            ),
            AccountInfo::new(
                &key_clock,
                is_signer,
                is_writable,
                &mut lamports1,
                &mut data1,
                &owner,
                executable,
                rent_epoch,
            ),
        ];
        let output = TestAccountsInfo::try_from_slice(&account_infos).unwrap();
        assert_eq!(output.not_sysvar.key, account_infos[0].key);
        assert_eq!(output.sysvar_clock.key, &clock::id());

        // `try_from_slice` should verify that we passed the correct public key;
        // if we try to pass in a different one than the hard-coded expected one,
        // it should fail.
        let key1 = Pubkey::new_unique();
        account_infos[1].key = &key1;
        assert_eq!(
            TestAccountsInfo::try_from_slice(&account_infos)
                .err()
                .unwrap(),
            LidoError::InvalidAccountInfo.into(),
        );
    }

    #[test]
    fn accounts_struct_variadic() {
        accounts_struct! {
            TestAccountsMeta, TestAccountsInfo {
                pub single { is_signer: false, is_writable: false, },
                pub ...remainder { is_signer: false, is_writable: true, },
            }
        }

        let input_0 = TestAccountsMeta {
            single: Pubkey::new_unique(),
            remainder: vec![],
        };
        let account_metas: Vec<AccountMeta> = input_0.to_vec();
        assert_eq!(account_metas.len(), 1);

        let input_1 = TestAccountsMeta {
            single: Pubkey::new_unique(),
            remainder: vec![Pubkey::new_unique()],
        };
        let account_metas: Vec<AccountMeta> = input_1.to_vec();
        assert_eq!(account_metas.len(), 2);
        assert_eq!(account_metas[0].pubkey, input_1.single);
        assert_eq!(account_metas[1].pubkey, input_1.remainder[0]);

        let input_2 = TestAccountsMeta {
            single: Pubkey::new_unique(),
            remainder: vec![Pubkey::new_unique(), Pubkey::new_unique()],
        };
        let account_metas: Vec<AccountMeta> = input_2.to_vec();
        assert_eq!(account_metas.len(), 3);
        assert_eq!(account_metas[0].pubkey, input_2.single);
        assert_eq!(account_metas[1].pubkey, input_2.remainder[0]);
        assert_eq!(account_metas[2].pubkey, input_2.remainder[1]);

        let pubkeys = vec![
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        ];
        let is_signer = false;
        let is_writable = true;
        let mut lamports = vec![0; 3];
        let mut datas = vec![vec![]; 3];
        let owner = Pubkey::new_unique();
        let executable = false;
        let rent_epoch = 0;
        let account_infos: Vec<AccountInfo> = pubkeys
            .iter()
            .zip(lamports.iter_mut())
            .zip(datas.iter_mut())
            .map(|((pubkey, lamports), data)| {
                AccountInfo::new(
                    pubkey,
                    is_signer,
                    is_writable,
                    lamports,
                    data,
                    &owner,
                    executable,
                    rent_epoch,
                )
            })
            .collect();

        let output = TestAccountsInfo::try_from_slice(&account_infos).unwrap();
        assert_eq!(output.single.key, &pubkeys[0]);
        assert_eq!(output.remainder.len(), 2);
        assert_eq!(output.remainder[0].key, &pubkeys[1]);
        assert_eq!(output.remainder[1].key, &pubkeys[2]);
    }
}
