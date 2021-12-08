use std::str::FromStr;

use crate::{error::AnkerError, token::MicroUst};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use hex::FromHexError;
use serde::Serialize;
use solana_program::{
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    msg,
    pubkey::Pubkey,
};

/// Wormhole's Terra chain id.
pub const WORMHOLE_CHAIN_ID_TERRA: u16 = 3;

/// Position of the native transfer code at the Wormhole project:
/// https://github.com/certusone/wormhole
/// solana/modules/token_bridge/program/src/lib.rs
const WORMHOLE_NATIVE_TRANSFER_CODE: u8 = 5;

#[repr(C)]
#[derive(
    Clone, Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct ForeignAddress([u8; 32]);

impl FromStr for ForeignAddress {
    type Err = FromHexError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut foreign_address = [0; 32];
        // Allow for array starting with 0x
        let array_vec = if s.len() >= 2 && &s[..2] == "0x" {
            hex::decode(&s[2..])
        } else {
            hex::decode(s)
        }?;
        if array_vec.len() > 32 {
            return Err(FromHexError::InvalidStringLength);
        }
        let start = 32 - array_vec.len();
        foreign_address[start..].clone_from_slice(&array_vec[..(32 - start)]);
        Ok(ForeignAddress(foreign_address))
    }
}

/// Payload copied and modified from the Wormhole project.
#[repr(C)]
#[derive(BorshSerialize, BorshSchema)]
pub struct Payload {
    pub nonce: u32,
    pub amount: MicroUst,
    pub fee: u64,
    pub foreign_address: ForeignAddress,
    pub target_chain: u16,
}

impl Payload {
    pub fn new(nonce: u32, amount: MicroUst, foreign_address: ForeignAddress) -> Payload {
        Payload {
            nonce,
            amount,
            fee: 0,
            foreign_address,
            target_chain: WORMHOLE_CHAIN_ID_TERRA,
        }
    }
}

pub fn check_wormhole_account(
    msg: &'static str,
    expected: &Pubkey,
    provided: &Pubkey,
) -> ProgramResult {
    if expected != provided {
        msg!(
            "Wrong Wormhole {}. Expected {}, but found {}",
            msg,
            expected,
            provided
        );
        return Err(AnkerError::InvalidSendRewardsParameters.into());
    }
    Ok(())
}

pub struct WormholeTransferArgs {
    pub token_bridge_program_id: Pubkey,
    pub core_bridge_program_id: Pubkey,
    pub payer: Pubkey,
    pub config_key: Pubkey,
    pub from: Pubkey,
    pub mint: Pubkey,
    pub custody_key: Pubkey,
    pub authority_signer_key: Pubkey,
    pub custody_signer_key: Pubkey,
    pub bridge_config: Pubkey,
    pub message: Pubkey,
    pub emitter_key: Pubkey,
    pub sequence_key: Pubkey,
    pub fee_collector_key: Pubkey,
}

impl WormholeTransferArgs {
    pub fn new(
        token_bridge_program_id: Pubkey,
        core_bridge_program_id: Pubkey,
        mint: Pubkey,
        payer: Pubkey,
        from: Pubkey,
        message: Pubkey,
    ) -> Self {
        let (config_key, _) = Pubkey::find_program_address(&[b"config"], &token_bridge_program_id);
        let (custody_key, _) =
            Pubkey::find_program_address(&[&mint.to_bytes()], &token_bridge_program_id);
        let (authority_signer_key, _) =
            Pubkey::find_program_address(&[b"authority_signer"], &token_bridge_program_id);
        let (custody_signer_key, _) =
            Pubkey::find_program_address(&[b"custody_signer"], &token_bridge_program_id);
        let (bridge_config, _) =
            Pubkey::find_program_address(&[b"Bridge"], &core_bridge_program_id);
        let (emitter_key, _) =
            Pubkey::find_program_address(&[b"emitter"], &token_bridge_program_id);
        let (sequence_key, _) = Pubkey::find_program_address(
            &[b"Sequence", &emitter_key.to_bytes()],
            &core_bridge_program_id,
        );
        let (fee_collector_key, _) =
            Pubkey::find_program_address(&[b"fee_collector"], &core_bridge_program_id);

        WormholeTransferArgs {
            token_bridge_program_id,
            core_bridge_program_id,
            config_key,
            mint,
            custody_key,
            authority_signer_key,
            custody_signer_key,
            bridge_config,
            emitter_key,
            sequence_key,
            fee_collector_key,
            payer,
            from,
            message,
        }
    }
}

/// Get Wormhole transfer instruction.
pub fn get_wormhole_transfer_instruction(
    payload: &Payload,
    wormhole_transfer_args: &WormholeTransferArgs,
) -> Instruction {
    Instruction {
        program_id: wormhole_transfer_args.token_bridge_program_id,
        accounts: vec![
            AccountMeta::new(wormhole_transfer_args.payer, true),
            AccountMeta::new_readonly(wormhole_transfer_args.config_key, false),
            AccountMeta::new(wormhole_transfer_args.from, false),
            AccountMeta::new(wormhole_transfer_args.mint, false),
            AccountMeta::new(wormhole_transfer_args.custody_key, false),
            AccountMeta::new_readonly(wormhole_transfer_args.authority_signer_key, false),
            AccountMeta::new_readonly(wormhole_transfer_args.custody_signer_key, false),
            AccountMeta::new(wormhole_transfer_args.bridge_config, false),
            AccountMeta::new(wormhole_transfer_args.message, true),
            AccountMeta::new_readonly(wormhole_transfer_args.emitter_key, false),
            AccountMeta::new(wormhole_transfer_args.sequence_key, false),
            AccountMeta::new(wormhole_transfer_args.fee_collector_key, false),
            AccountMeta::new_readonly(solana_program::sysvar::clock::id(), false),
            // Dependencies
            AccountMeta::new_readonly(solana_program::sysvar::rent::id(), false),
            AccountMeta::new_readonly(solana_program::system_program::id(), false),
            // Program
            AccountMeta::new_readonly(wormhole_transfer_args.core_bridge_program_id, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: (WORMHOLE_NATIVE_TRANSFER_CODE, payload)
            .try_to_vec()
            .unwrap(),
    }
}

// Tests transaction that locks wrapped Sol and transfers it to Ethereum. Transaction id
// 7cw4gLGZfH2rU5di5xeQbNZ1Nbc8D7i78jkxXtLUvnwyyZbha5E3Ew2izLjLTki56Ek1zQyZn2Ghb1tK4fWeMhE
#[test]
fn test_get_wormhole_instruction() {
    // wormDTUJ6AWPNvk59vGQbDvGJmqbDTdgWgAqcLBCgUb : Wormhole token bridge program id.
    // worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth : Wormhole core bridge program id.

    let wormhole_chain_id_ethereum = 2;
    let ethereum_pubkey =
        ForeignAddress::from_str("29fc5aacd613410b68c9c08d4e1656e3c890e482").unwrap();
    let mut payload = Payload::new(14476, MicroUst(500_000_000), ethereum_pubkey);
    payload.target_chain = wormhole_chain_id_ethereum;
    let payer = Pubkey::new_unique();
    let from = Pubkey::from_str("5F22sMTRuLQtkiuvTKif5WBYnv39cACJ8YcPzKfm1WaM").unwrap();
    let mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let message = Pubkey::new_unique();

    let wormhole_transfer_args = WormholeTransferArgs::new(
        Pubkey::from_str("wormDTUJ6AWPNvk59vGQbDvGJmqbDTdgWgAqcLBCgUb").unwrap(),
        Pubkey::from_str("worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth").unwrap(),
        mint,
        payer,
        from,
        message,
    );
    let instruction = get_wormhole_transfer_instruction(&payload, &wormhole_transfer_args);

    let expected_accounts = vec![
        payer,
        Pubkey::from_str("DapiQYH3BGonhN8cngWcXQ6SrqSm3cwysoznoHr6Sbsx").unwrap(),
        from,
        mint,
        Pubkey::from_str("2nQNF8F9LLWMqdjymiLK2u8HoHMvYa4orCXsp3w65fQ2").unwrap(),
        Pubkey::from_str("7oPa2PHQdZmjSPqvpZN7MQxnC7Dcf3uL4oLqknGLk2S3").unwrap(),
        Pubkey::from_str("GugU1tP7doLeTw9hQP51xRJyS8Da1fWxuiy2rVrnMD2m").unwrap(),
        Pubkey::from_str("2yVjuQwpsvdsrywzsJJVs9Ueh4zayyo5DYJbBNc3DDpn").unwrap(),
        message,
        Pubkey::from_str("Gv1KWf8DT1jKv5pKBmGaTmVszqa56Xn8YGx2Pg7i7qAk").unwrap(),
        Pubkey::from_str("GF2ghkjwsR9CHkGk1RvuZrApPZGBZynxMm817VNi51Nf").unwrap(),
        Pubkey::from_str("9bFNrXNb2WTx8fMHXCheaZqkLZ3YCCaiqTftHxeintHy").unwrap(),
        Pubkey::from_str("SysvarC1ock11111111111111111111111111111111").unwrap(),
        Pubkey::from_str("SysvarRent111111111111111111111111111111111").unwrap(),
        Pubkey::from_str("11111111111111111111111111111111").unwrap(),
        Pubkey::from_str("worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth").unwrap(),
        Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap(),
    ];
    let expected_data = hex::decode("058c3800000065cd1d00000000000000000000000000000000000000000000000029fc5aacd613410b68c9c08d4e1656e3c890e4820200").unwrap();
    let accounts: Vec<Pubkey> = instruction.accounts.iter().map(|acc| acc.pubkey).collect();
    assert_eq!(expected_accounts, accounts);
    assert_eq!(expected_data, instruction.data);
}

#[test]
fn test_foreign_address_conversion() {
    let zero_addr = ForeignAddress::from_str("").unwrap();
    assert_eq!(zero_addr, ForeignAddress([0; 32]));
    let zero_addr_start_0x = ForeignAddress::from_str("0x").unwrap();
    assert_eq!(zero_addr, zero_addr_start_0x);

    let eth_addr =
        ForeignAddress::from_str("29fc5aacd613410b68c9c08d4e1656e3c890e4820200").unwrap();
    assert_eq!(
        eth_addr,
        ForeignAddress([
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 41, 252, 90, 172, 214, 19, 65, 11, 104, 201, 192, 141,
            78, 22, 86, 227, 200, 144, 228, 130, 2, 0
        ])
    );
    let address_32_bytes = ForeignAddress::from_str(
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    )
    .unwrap();

    let address_32_bytes_start_with_0x = ForeignAddress::from_str(
        "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    )
    .unwrap();
    assert_eq!(address_32_bytes_start_with_0x, address_32_bytes);

    assert_eq!(
        address_32_bytes,
        ForeignAddress([
            222, 173, 190, 239, 222, 173, 190, 239, 222, 173, 190, 239, 222, 173, 190, 239, 222,
            173, 190, 239, 222, 173, 190, 239, 222, 173, 190, 239, 222, 173, 190, 239
        ])
    );
    let address_more_32_bytes = ForeignAddress::from_str(
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefde",
    );
    assert_eq!(
        address_more_32_bytes,
        Err(FromHexError::InvalidStringLength)
    );
    let address_invalid_hexa = ForeignAddress::from_str("ka");
    assert_eq!(
        address_invalid_hexa,
        Err(FromHexError::InvalidHexCharacter { c: 'k', index: 0 })
    );
    let address_odd_length_bytes = ForeignAddress::from_str("aaa");
    assert_eq!(address_odd_length_bytes, Err(FromHexError::OddLength));
}
