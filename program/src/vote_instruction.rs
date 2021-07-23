// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! FIXME copied from the Solana stake program.
//
// Since the Solana vote program cannot be included in a Solana program, for now
// we copy the necessary parts that we need to deal with vote accounts in this
// file.
//
// Solana is licensed under the Apache, version 2, which can be found
// at https://www.apache.org/licenses/LICENSE-2.0.html.
// We copied only the parts that we use, and do not alter the original file in
// any meaningful way.

use serde_derive::{Deserialize, Serialize};
use solana_program::{
    clock::{Slot, UnixTimestamp},
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    vote,
};

// FIXME: copied from the solana vote program.
#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub struct VoteInit {
    pub node_pubkey: Pubkey,
    pub authorized_voter: Pubkey,
    pub authorized_withdrawer: Pubkey,
    pub commission: u8,
}

// FIXME: copied from the solana vote program.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum VoteAuthorize {
    Voter,
    Withdrawer,
}

// FIXME: copied from the solana vote program.
#[derive(Serialize, Default, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Vote {
    /// A stack of votes starting with the oldest vote
    pub slots: Vec<Slot>,
    /// signature of the bank's state at the last slot
    pub hash: Hash,
    /// processing timestamp of last slot
    pub timestamp: Option<UnixTimestamp>,
}

// FIXME: copied from the solana vote program.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum VoteInstruction {
    /// Initialize a vote account
    ///
    /// # Account references
    ///   0. [WRITE] Uninitialized vote account
    ///   1. [] Rent sysvar
    ///   2. [] Clock sysvar
    ///   3. [SIGNER] New validator identity (node_pubkey)
    InitializeAccount(VoteInit),

    /// Authorize a key to send votes or issue a withdrawal
    ///
    /// # Account references
    ///   0. [WRITE] Vote account to be updated with the Pubkey for authorization
    ///   1. [] Clock sysvar
    ///   2. [SIGNER] Vote or withdraw authority
    Authorize(Pubkey, VoteAuthorize),

    /// A Vote instruction with recent votes
    ///
    /// # Account references
    ///   0. [WRITE] Vote account to vote with
    ///   1. [] Slot hashes sysvar
    ///   2. [] Clock sysvar
    ///   3. [SIGNER] Vote authority
    Vote(Vote),

    /// Withdraw some amount of funds
    ///
    /// # Account references
    ///   0. [WRITE] Vote account to withdraw from
    ///   1. [WRITE] Recipient account
    ///   2. [SIGNER] Withdraw authority
    Withdraw(u64),

    /// Update the vote account's validator identity (node_pubkey)
    ///
    /// # Account references
    ///   0. [WRITE] Vote account to be updated with the given authority public key
    ///   1. [SIGNER] New validator identity (node_pubkey)
    ///   2. [SIGNER] Withdraw authority
    UpdateValidatorIdentity,

    /// Update the commission for the vote account
    ///
    /// # Account references
    ///   0. [WRITE] Vote account to be updated
    ///   1. [SIGNER] Withdraw authority
    UpdateCommission(u8),

    /// A Vote instruction with recent votes
    ///
    /// # Account references
    ///   0. [WRITE] Vote account to vote with
    ///   1. [] Slot hashes sysvar
    ///   2. [] Clock sysvar
    ///   3. [SIGNER] Vote authority
    VoteSwitch(Vote, Hash),
}

// FIXME: copied from the solana vote program.
pub fn withdraw(
    vote_pubkey: &Pubkey,
    authorized_withdrawer_pubkey: &Pubkey,
    lamports: u64,
    to_pubkey: &Pubkey,
) -> Instruction {
    let account_metas = vec![
        AccountMeta::new(*vote_pubkey, false),
        AccountMeta::new(*to_pubkey, false),
        AccountMeta::new_readonly(*authorized_withdrawer_pubkey, true),
    ];

    Instruction::new_with_bincode(
        vote::program::id(),
        &VoteInstruction::Withdraw(lamports),
        account_metas,
    )
}
