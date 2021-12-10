use std::str::FromStr;
use std::fmt;
use std::fmt::Formatter;

use bech32::{FromBase32, ToBase32};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::Serialize;
use solana_program::{
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    msg,
    pubkey::Pubkey,
};

use crate::{error::AnkerError, token::MicroUst};

/// Wormhole's Terra chain id.
pub const WORMHOLE_CHAIN_ID_TERRA: u16 = 3;

/// Position of the native transfer code at the Wormhole project:
/// https://github.com/certusone/wormhole/blob/05425a96df6e5841f05e7be5e7f4c45be01985a6/solana/modules/token_bridge/program/src/lib.rs
const WORMHOLE_NATIVE_TRANSFER_CODE: u8 = 5;

#[repr(C)]
#[derive(
    Clone, Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct ForeignAddress([u8; 32]);

#[derive(Debug, Eq, PartialEq)]
pub enum AddressError {
    /// Bech32 decoding failed.
    Bech32(bech32::Error),

    /// The human-readable part of the address is not "terra".
    HumanReadablePartIsNotTerra,

    /// The address is either too long or too short.
    LengthNot20Bytes,

    /// The variant is not the classic BIP-0173 bech32.
    VariantIsNotBech32,
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AddressError::Bech32(err) => write!(f, "Invalid bech32 format: {}", err),
            AddressError::HumanReadablePartIsNotTerra => write!(f, "Address does not start with 'terra'."),
            AddressError::LengthNot20Bytes => write!(f, "The address is not 20 bytes long."),
            AddressError::VariantIsNotBech32 => write!(f, "The address variant is not the classic BIP-0173 bech32."),
        }
    }

}

#[repr(C)]
#[derive(
    Clone, Default, Debug, BorshSerialize, BorshDeserialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct TerraAddress([u8; 20]);

impl TerraAddress {
    pub fn to_foreign(&self) -> ForeignAddress {
        let mut foreign = [0_u8; 32];
        foreign[12..].copy_from_slice(&self.0[..]);
        ForeignAddress(foreign)
    }
}

impl FromStr for TerraAddress {
    type Err = AddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {

        let (hrp, data_u5, variant) = bech32::decode(s).map_err(AddressError::Bech32)?;
        if hrp != "terra" {
            return Err(AddressError::HumanReadablePartIsNotTerra);
        }
        if variant != bech32::Variant::Bech32 {
            return Err(AddressError::VariantIsNotBech32);
        }

        let data_bytes = Vec::<u8>::from_base32(&data_u5).map_err(AddressError::Bech32)?;
        if data_bytes.len() != 20 {
            return Err(AddressError::LengthNot20Bytes);
        }

        let mut address = [0; 20];
        address.copy_from_slice(&data_bytes);

        Ok(TerraAddress(address))
    }
}

impl fmt::Display for TerraAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        bech32::encode_to_fmt(f, "terra", self.0.to_base32(), bech32::Variant::Bech32)
            .expect("The HRP is hard-coded and known to be fine, it should not fail.")
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
    let ethereum_pubkey = ForeignAddress([
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x29, 0xfc, 0x5a, 0xac,
        0xd6, 0x13, 0x41, 0x0b, 0x68, 0xc9, 0xc0, 0x8d,
        0x4e, 0x16, 0x56, 0xe3, 0xc8, 0x90, 0xe4, 0x82,
    ]);
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
fn test_terra_address_from_string() {
    // This is the address from the test transaction:
    // https://github.com/ChorusOne/solido/issues/445#issuecomment-988002302.
    assert_eq!(
        TerraAddress::from_str("terra1z7529lza7elcleyhzj2sfq62uk7rtjgnrqeuxr"),
        Ok(TerraAddress([
            0x17, 0xa8, 0xa2, 0xfc,
            0x5d, 0xf6, 0x7f, 0x8f, 0xe4, 0x97, 0x14, 0x95,
            0x04, 0x83, 0x4a, 0xe5, 0xbc, 0x35, 0xc9, 0x13
        ])),
    );
}

#[test]
fn test_terra_address_to_string() {
    // This is the address from the test transaction:
    // https://github.com/ChorusOne/solido/issues/445#issuecomment-988002302.
    assert_eq!(
        TerraAddress([
            0x17, 0xa8, 0xa2, 0xfc,
            0x5d, 0xf6, 0x7f, 0x8f, 0xe4, 0x97, 0x14, 0x95,
            0x04, 0x83, 0x4a, 0xe5, 0xbc, 0x35, 0xc9, 0x13
        ]).to_string(),
        "terra1z7529lza7elcleyhzj2sfq62uk7rtjgnrqeuxr",
    );
}

#[test]
fn terra_address_to_foreign_left_pads_with_zeros() {
    assert_eq!(
        TerraAddress([
            0x17, 0xa8, 0xa2, 0xfc,
            0x5d, 0xf6, 0x7f, 0x8f, 0xe4, 0x97, 0x14, 0x95,
            0x04, 0x83, 0x4a, 0xe5, 0xbc, 0x35, 0xc9, 0x13
        ]).to_foreign(),
        ForeignAddress([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x17, 0xa8, 0xa2, 0xfc,
            0x5d, 0xf6, 0x7f, 0x8f, 0xe4, 0x97, 0x14, 0x95,
            0x04, 0x83, 0x4a, 0xe5, 0xbc, 0x35, 0xc9, 0x13
        ])
    );
}
