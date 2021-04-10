mod helpers;

use {
    borsh::BorshSerialize,
    helpers::*,
    solana_program::{
        borsh::get_packed_len,
        hash::Hash,
        instruction::{AccountMeta, Instruction},
        program_pack::Pack,
        system_instruction, sysvar,
    },
    solana_program_test::*,
    solana_sdk::{
        instruction::InstructionError, signature::Keypair, signature::Signer,
        transaction::Transaction, transaction::TransactionError, transport::TransportError,
    },
    spl_stake_pool::{
        borsh::{get_instance_packed_len, try_from_slice_unchecked},
        error, id, instruction, state,
    },
};

#[tokio::test]
async fn lido() {}
