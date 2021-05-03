//! Instruction types

#![allow(clippy::too_many_arguments)]

use {
    crate::{
        find_deposit_authority_program_address, find_stake_program_address,
        find_transient_stake_program_address, stake_program, state::Fee,
    },
    borsh::{BorshDeserialize, BorshSchema, BorshSerialize},
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program_error::ProgramError,
        pubkey::Pubkey,
        system_program, sysvar,
    },
};

/// Instructions supported by the StakePool program.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum StakePoolInstruction {
    ///   Initializes a new StakePool.
    ///
    ///   0. `[w]` New StakePool to create.
    ///   1. `[s]` Manager
    ///   2. `[]` Staker
    ///   3. `[w]` Uninitialized validator stake list storage account
    ///   4. `[]` Reserve stake account must be initialized, have zero balance,
    ///       and staker / withdrawer authority set to pool withdraw authority.
    ///   5. `[]` Pool token mint. Must have zero supply, owned by withdraw authority.
    ///   6. `[]` Pool account to deposit the generated fee for manager.
    ///   7. `[]` Clock sysvar
    ///   8. `[]` Rent sysvar
    ///   9. `[]` Token program id
    ///  10. `[]` (Optional) Deposit authority that must sign all deposits.
    ///      Defaults to the program address generated using
    ///      `find_deposit_authority_program_address`, making deposits permissionless.
    Initialize {
        /// Fee assessed as percentage of perceived rewards
        #[allow(dead_code)] // but it's not
        fee: Fee,
        /// Maximum expected number of validators
        #[allow(dead_code)] // but it's not
        max_validators: u32,
    },

    ///   (Staker only) Creates new program account for accumulating stakes for
    ///   a particular validator
    ///
    ///   0. `[]` Stake pool account this stake will belong to
    ///   1. `[s]` Staker
    ///   2. `[ws]` Funding account (must be a system account)
    ///   3. `[w]` Stake account to be created
    ///   4. `[]` Validator this stake account will vote for
    ///   5. `[]` Rent sysvar
    ///   6. `[]` Stake History sysvar
    ///   7. `[]` Stake Config sysvar
    ///   8. `[]` System program
    ///   9. `[]` Stake program
    CreateValidatorStakeAccount,

    ///   (Staker only) Adds stake account delegated to validator to the pool's
    ///   list of managed validators.
    ///
    ///   The stake account must have the rent-exempt amount plus at least 1 SOL,
    ///   and at most 1.001 SOL.
    ///
    ///   Once we delegate even 1 SOL, it will accrue rewards one epoch later,
    ///   so we'll have more than 1 active SOL at this point.
    ///   At 10% annualized rewards, 1 epoch of 2 days will accrue
    ///   0.000547945 SOL, so we check that it is at least 1 SOL, and at most
    ///   1.001 SOL.
    ///
    ///   0. `[w]` Stake pool
    ///   1. `[s]` Staker
    ///   2. `[]` Stake pool withdraw authority
    ///   3. `[w]` Validator stake list storage account
    ///   4. `[w]` Stake account to add to the pool, its withdraw authority must
    ///      be set to the staker
    ///   5. `[]` Clock sysvar
    ///   6. '[]' Sysvar stake history account
    ///   7. `[]` Stake program
    AddValidatorToPool,

    ///   (Staker only) Removes validator from the pool
    ///
    ///   Only succeeds if the validator stake account has the minimum of 1 SOL
    ///   plus the rent-exempt amount.
    ///
    ///   0. `[w]` Stake pool
    ///   1. `[s]` Staker
    ///   2. `[]` Stake pool withdraw authority
    ///   3. `[]` New withdraw/staker authority to set in the stake account
    ///   4. `[w]` Validator stake list storage account
    ///   5. `[w]` Stake account to remove from the pool
    ///   6. `[]` Transient stake account, to check that that we're not trying to activate
    ///   7. '[]' Sysvar clock
    ///   8. `[]` Stake program id,
    RemoveValidatorFromPool,

    /// (Staker only) Decrease active stake on a validator, eventually moving it to the reserve
    ///
    /// Internally, this instruction splits a validator stake account into its
    /// corresponding transient stake account and deactivates it.
    ///
    /// In order to rebalance the pool without taking custody, the staker needs
    /// a way of reducing the stake on a stake account. This instruction splits
    /// some amount of stake, up to the total activated stake, from the canonical
    /// validator stake account, into its "transient" stake account.
    ///
    /// The instruction only succeeds if the transient stake account does not
    /// exist. The amount of lamports to move must be at least rent-exemption
    /// plus 1 lamport.
    ///
    ///  0. `[]` Stake pool
    ///  1. `[s]` Stake pool staker
    ///  2. `[]` Stake pool withdraw authority
    ///  3. `[]` Validator list
    ///  5. `[w]` Canonical stake account to split from
    ///  5. `[w]` Transient stake account to receive split
    ///  6. `[]` Clock sysvar
    ///  7. `[]` Rent sysvar
    ///  8. `[]` System program
    ///  9. `[]` Stake program
    ///  userdata: amount of lamports to split into the transient stake account
    DecreaseValidatorStake(u64),

    /// (Staker only) Increase stake on a validator from the reserve account
    ///
    /// Internally, this instruction splits reserve stake into a transient stake
    /// account and delegate to the appropriate validator. `UpdateValidatorListBalance`
    /// will do the work of merging once it's ready.
    ///
    /// This instruction only succeeds if the transient stake account does not exist.
    /// The minimum amount to move is rent-exemption plus 1 SOL in order to avoid
    /// issues on credits observed when merging active stakes later.
    ///
    ///  0. `[]` Stake pool
    ///  1. `[s]` Stake pool staker
    ///  2. `[]` Stake pool withdraw authority
    ///  3. `[w]` Validator list
    ///  4. `[w]` Stake pool reserve stake
    ///  5. `[w]` Transient stake account
    ///  6. `[]` Validator vote account to delegate to
    ///  7. '[]' Clock sysvar
    ///  8. '[]' Rent sysvar
    ///  9. `[]` Stake History sysvar
    /// 10. `[]` Stake Config sysvar
    /// 11. `[]` System program
    /// 12. `[]` Stake program
    ///  userdata: amount of lamports to split into the transient stake account
    IncreaseValidatorStake(u64),

    ///  Updates balances of validator and transient stake accounts in the pool
    ///
    ///  While going through the pairs of validator and transient stake accounts,
    ///  if the transient stake is inactive, it is merged into the reserve stake
    ///  account. If the transient stake is active and has matching credits
    ///  observed, it is merged into the canonical validator stake account. In
    ///  all other states, nothing is done, and the balance is simply added to
    ///  the canonical stake account balance.
    ///
    ///  0. `[]` Stake pool
    ///  1. `[]` Stake pool withdraw authority
    ///  2. `[w]` Validator stake list storage account
    ///  3. `[w]` Reserve stake account
    ///  4. `[]` Sysvar clock
    ///  5. `[]` Sysvar stake history
    ///  6. `[]` Stake program
    ///  7. ..7+N ` [] N pairs of validator and transient stake accounts
    UpdateValidatorListBalance {
        /// Index to start updating on the validator list
        #[allow(dead_code)] // but it's not
        start_index: u32,
        /// If true, don't try merging transient stake accounts into the reserve or
        /// validator stake account.  Useful for testing or if a particular stake
        /// account is in a bad state, but we still want to update
        #[allow(dead_code)] // but it's not
        no_merge: bool,
    },

    ///   Updates total pool balance based on balances in the reserve and validator list
    ///
    ///   0. `[w]` Stake pool
    ///   1. `[]` Stake pool withdraw authority
    ///   2. `[]` Validator stake list storage account
    ///   3. `[]` Reserve stake account
    ///   4. `[w]` Account to receive pool fee tokens
    ///   5. `[w]` Pool mint account
    ///   6. `[]` Sysvar clock account
    ///   7. `[]` Pool token program
    UpdateStakePoolBalance,

    ///   Deposit some stake into the pool.  The output is a "pool" token representing ownership
    ///   into the pool. Inputs are converted to the current ratio.
    ///
    ///   0. `[w]` Stake pool
    ///   1. `[w]` Validator stake list storage account
    ///   2. `[]` Stake pool deposit authority
    ///   3. `[]` Stake pool withdraw authority
    ///   4. `[w]` Stake account to join the pool (withdraw authority for the stake account should be first set to the stake pool deposit authority)
    ///   5. `[w]` Validator stake account for the stake account to be merged with
    ///   6. `[w]` User account to receive pool tokens
    ///   8. `[w]` Pool token mint account
    ///   9. '[]' Sysvar clock account (required)
    ///   10. '[]' Sysvar stake history account
    ///   11. `[]` Pool token program id,
    ///   12. `[]` Stake program id,
    Deposit,

    ///   Withdraw the token from the pool at the current ratio.
    ///
    ///   Succeeds if the stake account has enough SOL to cover the desired amount
    ///   of pool tokens, and if the withdrawal keeps the total staked amount
    ///   above the minimum of rent-exempt amount + 1 SOL.
    ///
    ///   A validator stake account can be withdrawn from freely, and the reserve
    ///   can only be drawn from if there is no active stake left, where all
    ///   validator accounts are left with 1 lamport.
    ///
    ///   0. `[w]` Stake pool
    ///   1. `[w]` Validator stake list storage account
    ///   2. `[]` Stake pool withdraw authority
    ///   3. `[w]` Validator or reserve stake account to split
    ///   4. `[w]` Unitialized stake account to receive withdrawal
    ///   5. `[]` User account to set as a new withdraw authority
    ///   6. `[s]` User transfer authority, for pool token account
    ///   7. `[w]` User account with pool tokens to burn from
    ///   8. `[w]` Pool token mint account
    ///   9. `[]` Sysvar clock account (required)
    ///  10. `[]` Pool token program id
    ///  11. `[]` Stake program id,
    ///  userdata: amount of pool tokens to withdraw
    Withdraw(u64),

    ///  (Manager only) Update manager
    ///
    ///  0. `[w]` StakePool
    ///  1. `[s]` Manager
    ///  2. '[]` New manager pubkey
    ///  3. '[]` New manager fee account
    SetManager,

    ///  (Manager only) Update fee
    ///
    ///  0. `[w]` StakePool
    ///  1. `[s]` Manager
    ///  2. `[]` Sysvar clock
    SetFee {
        /// Fee assessed as percentage of perceived rewards
        #[allow(dead_code)] // but it's not
        fee: Fee,
    },

    ///  (Manager or staker only) Update staker
    ///
    ///  0. `[w]` StakePool
    ///  1. `[s]` Manager or current staker
    ///  2. '[]` New staker pubkey
    SetStaker,
}

/// Creates an 'initialize' instruction.
pub fn initialize(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    manager: &Pubkey,
    staker: &Pubkey,
    validator_list: &Pubkey,
    reserve_stake: &Pubkey,
    pool_mint: &Pubkey,
    manager_pool_account: &Pubkey,
    token_program_id: &Pubkey,
    deposit_authority: Option<Pubkey>,
    fee: Fee,
    max_validators: u32,
) -> Result<Instruction, ProgramError> {
    let init_data = StakePoolInstruction::Initialize {
        fee,
        max_validators,
    };
    let data = init_data.try_to_vec()?;
    let mut accounts = vec![
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
    ];
    if let Some(deposit_authority) = deposit_authority {
        accounts.push(AccountMeta::new_readonly(deposit_authority, true));
    }
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates `CreateValidatorStakeAccount` instruction (create new stake account for the validator)
pub fn create_validator_stake_account(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    staker: &Pubkey,
    funder: &Pubkey,
    stake_account: &Pubkey,
    validator: &Pubkey,
) -> Result<Instruction, ProgramError> {
    let accounts = vec![
        AccountMeta::new_readonly(*stake_pool, false),
        AccountMeta::new_readonly(*staker, true),
        AccountMeta::new(*funder, true),
        AccountMeta::new(*stake_account, false),
        AccountMeta::new_readonly(*validator, false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(stake_program::config_id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::CreateValidatorStakeAccount.try_to_vec()?,
    })
}

/// Creates `AddValidatorToPool` instruction (add new validator stake account to the pool)
pub fn add_validator_to_pool(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    staker: &Pubkey,
    stake_pool_withdraw: &Pubkey,
    validator_list: &Pubkey,
    stake_account: &Pubkey,
) -> Result<Instruction, ProgramError> {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new_readonly(*staker, true),
        AccountMeta::new_readonly(*stake_pool_withdraw, false),
        AccountMeta::new(*validator_list, false),
        AccountMeta::new(*stake_account, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::AddValidatorToPool.try_to_vec()?,
    })
}

/// Creates `RemoveValidatorFromPool` instruction (remove validator stake account from the pool)
pub fn remove_validator_from_pool(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    staker: &Pubkey,
    stake_pool_withdraw: &Pubkey,
    new_stake_authority: &Pubkey,
    validator_list: &Pubkey,
    stake_account: &Pubkey,
    transient_stake_account: &Pubkey,
) -> Result<Instruction, ProgramError> {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new_readonly(*staker, true),
        AccountMeta::new_readonly(*stake_pool_withdraw, false),
        AccountMeta::new_readonly(*new_stake_authority, false),
        AccountMeta::new(*validator_list, false),
        AccountMeta::new(*stake_account, false),
        AccountMeta::new_readonly(*transient_stake_account, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::RemoveValidatorFromPool.try_to_vec()?,
    })
}

/// Creates `DecreaseValidatorStake` instruction (rebalance from validator account to
/// transient account)
pub fn decrease_validator_stake(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    staker: &Pubkey,
    stake_pool_withdraw_authority: &Pubkey,
    validator_list: &Pubkey,
    validator_stake: &Pubkey,
    transient_stake: &Pubkey,
    lamports: u64,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new_readonly(*stake_pool, false),
        AccountMeta::new_readonly(*staker, true),
        AccountMeta::new_readonly(*stake_pool_withdraw_authority, false),
        AccountMeta::new_readonly(*validator_list, false),
        AccountMeta::new(*validator_stake, false),
        AccountMeta::new(*transient_stake, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::DecreaseValidatorStake(lamports)
            .try_to_vec()
            .unwrap(),
    }
}

/// Creates `IncreaseValidatorStake` instruction (rebalance from reserve account to
/// transient account)
pub fn increase_validator_stake(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    staker: &Pubkey,
    stake_pool_withdraw_authority: &Pubkey,
    validator_list: &Pubkey,
    reserve_stake: &Pubkey,
    transient_stake: &Pubkey,
    validator: &Pubkey,
    lamports: u64,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new_readonly(*stake_pool, false),
        AccountMeta::new_readonly(*staker, true),
        AccountMeta::new_readonly(*stake_pool_withdraw_authority, false),
        AccountMeta::new(*validator_list, false),
        AccountMeta::new(*reserve_stake, false),
        AccountMeta::new(*transient_stake, false),
        AccountMeta::new_readonly(*validator, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(stake_program::config_id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::IncreaseValidatorStake(lamports)
            .try_to_vec()
            .unwrap(),
    }
}

/// Creates `UpdateValidatorListBalance` instruction (update validator stake account balances)
pub fn update_validator_list_balance(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    stake_pool_withdraw_authority: &Pubkey,
    validator_list: &Pubkey,
    reserve_stake: &Pubkey,
    validator_vote_accounts: &[Pubkey],
    start_index: u32,
    no_merge: bool,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(*stake_pool, false),
        AccountMeta::new_readonly(*stake_pool_withdraw_authority, false),
        AccountMeta::new(*validator_list, false),
        AccountMeta::new(*reserve_stake, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    accounts.append(
        &mut validator_vote_accounts
            .iter()
            .flat_map(|vote_account_address| {
                let (validator_stake_account, _) =
                    find_stake_program_address(program_id, vote_account_address, stake_pool);
                let (transient_stake_account, _) = find_transient_stake_program_address(
                    program_id,
                    vote_account_address,
                    stake_pool,
                );
                vec![
                    AccountMeta::new(validator_stake_account, false),
                    AccountMeta::new(transient_stake_account, false),
                ]
            })
            .collect::<Vec<AccountMeta>>(),
    );
    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::UpdateValidatorListBalance {
            start_index,
            no_merge,
        }
        .try_to_vec()
        .unwrap(),
    }
}

/// Creates `UpdateStakePoolBalance` instruction (pool balance from the stake account list balances)
pub fn update_stake_pool_balance(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    withdraw_authority: &Pubkey,
    validator_list_storage: &Pubkey,
    reserve_stake: &Pubkey,
    manager_fee_account: &Pubkey,
    stake_pool_mint: &Pubkey,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new_readonly(*withdraw_authority, false),
        AccountMeta::new(*validator_list_storage, false),
        AccountMeta::new_readonly(*reserve_stake, false),
        AccountMeta::new(*manager_fee_account, false),
        AccountMeta::new(*stake_pool_mint, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ];
    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::UpdateStakePoolBalance
            .try_to_vec()
            .unwrap(),
    }
}

/// Creates instructions required to deposit into a stake pool, given a stake
/// account owned by the user.
pub fn deposit(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    validator_list_storage: &Pubkey,
    stake_pool_withdraw_authority: &Pubkey,
    deposit_stake_address: &Pubkey,
    deposit_stake_withdraw_authority: &Pubkey,
    validator_stake_accont: &Pubkey,
    pool_tokens_to: &Pubkey,
    pool_mint: &Pubkey,
    token_program_id: &Pubkey,
) -> Vec<Instruction> {
    let stake_pool_deposit_authority =
        find_deposit_authority_program_address(program_id, stake_pool).0;
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new(*validator_list_storage, false),
        AccountMeta::new_readonly(stake_pool_deposit_authority, false),
        AccountMeta::new_readonly(*stake_pool_withdraw_authority, false),
        AccountMeta::new(*deposit_stake_address, false),
        AccountMeta::new(*validator_stake_accont, false),
        AccountMeta::new(*pool_tokens_to, false),
        AccountMeta::new(*pool_mint, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(*token_program_id, false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    vec![
        stake_program::authorize(
            deposit_stake_address,
            deposit_stake_withdraw_authority,
            &stake_pool_deposit_authority,
            stake_program::StakeAuthorize::Staker,
        ),
        stake_program::authorize(
            deposit_stake_address,
            deposit_stake_withdraw_authority,
            &stake_pool_deposit_authority,
            stake_program::StakeAuthorize::Withdrawer,
        ),
        Instruction {
            program_id: *program_id,
            accounts,
            data: StakePoolInstruction::Deposit.try_to_vec().unwrap(),
        },
    ]
}

/// Creates instructions required to deposit into a stake pool, given a stake
/// account owned by the user. The difference with `deposit()` is that a deposit
/// authority must sign this instruction, which is required for private pools.
pub fn deposit_with_authority(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    validator_list_storage: &Pubkey,
    stake_pool_deposit_authority: &Pubkey,
    stake_pool_withdraw_authority: &Pubkey,
    deposit_stake_address: &Pubkey,
    deposit_stake_withdraw_authority: &Pubkey,
    validator_stake_accont: &Pubkey,
    pool_tokens_to: &Pubkey,
    pool_mint: &Pubkey,
    token_program_id: &Pubkey,
) -> Vec<Instruction> {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new(*validator_list_storage, false),
        AccountMeta::new_readonly(*stake_pool_deposit_authority, true),
        AccountMeta::new_readonly(*stake_pool_withdraw_authority, false),
        AccountMeta::new(*deposit_stake_address, false),
        AccountMeta::new(*validator_stake_accont, false),
        AccountMeta::new(*pool_tokens_to, false),
        AccountMeta::new(*pool_mint, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(*token_program_id, false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    vec![
        stake_program::authorize(
            deposit_stake_address,
            deposit_stake_withdraw_authority,
            stake_pool_deposit_authority,
            stake_program::StakeAuthorize::Staker,
        ),
        stake_program::authorize(
            deposit_stake_address,
            deposit_stake_withdraw_authority,
            stake_pool_deposit_authority,
            stake_program::StakeAuthorize::Withdrawer,
        ),
        Instruction {
            program_id: *program_id,
            accounts,
            data: StakePoolInstruction::Deposit.try_to_vec().unwrap(),
        },
    ]
}

/// Creates a 'withdraw' instruction.
pub fn withdraw(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    validator_list_storage: &Pubkey,
    stake_pool_withdraw: &Pubkey,
    stake_to_split: &Pubkey,
    stake_to_receive: &Pubkey,
    user_stake_authority: &Pubkey,
    user_transfer_authority: &Pubkey,
    user_pool_token_account: &Pubkey,
    pool_mint: &Pubkey,
    token_program_id: &Pubkey,
    amount: u64,
) -> Result<Instruction, ProgramError> {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new(*validator_list_storage, false),
        AccountMeta::new_readonly(*stake_pool_withdraw, false),
        AccountMeta::new(*stake_to_split, false),
        AccountMeta::new(*stake_to_receive, false),
        AccountMeta::new_readonly(*user_stake_authority, false),
        AccountMeta::new_readonly(*user_transfer_authority, true),
        AccountMeta::new(*user_pool_token_account, false),
        AccountMeta::new(*pool_mint, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(*token_program_id, false),
        AccountMeta::new_readonly(stake_program::id(), false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::Withdraw(amount).try_to_vec()?,
    })
}

/// Creates a 'set manager' instruction.
pub fn set_manager(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    manager: &Pubkey,
    new_manager: &Pubkey,
    new_fee_receiver: &Pubkey,
) -> Result<Instruction, ProgramError> {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new_readonly(*manager, true),
        AccountMeta::new_readonly(*new_manager, false),
        AccountMeta::new_readonly(*new_fee_receiver, false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::SetManager.try_to_vec()?,
    })
}

/// Creates a 'set fee' instruction.
pub fn set_fee(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    manager: &Pubkey,
    fee: Fee,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new_readonly(*manager, true),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
    ];
    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::SetFee { fee }.try_to_vec().unwrap(),
    }
}

/// Creates a 'set staker' instruction.
pub fn set_staker(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    set_staker_authority: &Pubkey,
    new_staker: &Pubkey,
) -> Result<Instruction, ProgramError> {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new_readonly(*set_staker_authority, true),
        AccountMeta::new_readonly(*new_staker, false),
    ];
    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::SetStaker.try_to_vec()?,
    })
}
