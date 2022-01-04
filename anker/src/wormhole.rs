use std::fmt;
use std::fmt::Formatter;
use std::str::FromStr;

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

/// The constant is 4, because it is the instruction at index 4, starting from 0.
/// https://github.com/certusone/wormhole/blob/94695ee125399f67c3a62f26ebd807cf532567c4/solana/modules/token_bridge/program/src/lib.rs#L80
const WORMHOLE_WRAPPED_TRANSFER_CODE: u8 = 4;

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
            AddressError::HumanReadablePartIsNotTerra => {
                write!(f, "Address does not start with 'terra'.")
            }
            AddressError::LengthNot20Bytes => write!(f, "The address is not 20 bytes long."),
            AddressError::VariantIsNotBech32 => {
                write!(f, "The address variant is not the classic BIP-0173 bech32.")
            }
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
        // Wormhole treats all addresses as bytestrings of length 32. If the
        // address is shorter, it must be left-padded with zeros.
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
    pub payer: Pubkey,
    pub config_key: Pubkey,
    pub from: Pubkey,
    pub from_owner: Pubkey,
    pub wrapped_mint_key: Pubkey,
    pub wrapped_meta_key: Pubkey,
    pub authority_signer_key: Pubkey,
    pub bridge_config: Pubkey,
    pub message: Pubkey,
    pub emitter_key: Pubkey,
    pub sequence_key: Pubkey,
    pub fee_collector_key: Pubkey,
    pub core_bridge_program_id: Pubkey,
    pub token_bridge_program_id: Pubkey,
}

impl WormholeTransferArgs {
    pub fn new(
        token_bridge_program_id: Pubkey,
        core_bridge_program_id: Pubkey,
        wrapped_mint_key: Pubkey,
        payer: Pubkey,
        from: Pubkey,
        from_owner: Pubkey,
        message: Pubkey,
    ) -> Self {
        let (config_key, _) = Pubkey::find_program_address(&[b"config"], &token_bridge_program_id);
        let (wrapped_meta_key, _) = Pubkey::find_program_address(
            &[b"meta", &wrapped_mint_key.to_bytes()],
            &token_bridge_program_id,
        );
        let (authority_signer_key, _) =
            Pubkey::find_program_address(&[b"authority_signer"], &token_bridge_program_id);
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
            payer,
            config_key,
            from,
            from_owner,
            wrapped_mint_key,
            wrapped_meta_key,
            authority_signer_key,
            bridge_config,
            message,
            emitter_key,
            sequence_key,
            fee_collector_key,
            core_bridge_program_id,
            token_bridge_program_id,
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
            AccountMeta::new_readonly(wormhole_transfer_args.from_owner, true),
            AccountMeta::new(wormhole_transfer_args.wrapped_mint_key, false),
            AccountMeta::new_readonly(wormhole_transfer_args.wrapped_meta_key, false),
            AccountMeta::new_readonly(wormhole_transfer_args.authority_signer_key, false),
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
        data: (WORMHOLE_WRAPPED_TRANSFER_CODE, payload)
            .try_to_vec()
            .unwrap(),
    }
}

/// Test transaction that transfers UST on Solana to Terra.
///
/// Based on this transaction: <https://explorer.solana.com/tx/5tSRA1CYLd51sjf7Dd2ZRkLspcqiR8NH51oTd3K34sNc3PZG9uF7euE2AHE95KurrcfKYf2sCQqsEbSRmzQq8oDg?cluster=devnet>.
#[test]
fn test_get_wormhole_instruction() {
    let terra_addr =
        TerraAddress::from_str("terra1z7529lza7elcleyhzj2sfq62uk7rtjgnrqeuxr").unwrap();
    let foreign_addr = terra_addr.to_foreign();

    let payload = Payload::new(0x28fb, MicroUst(1_000_000), foreign_addr);
    let payer = Pubkey::from_str("GUVfssWwwu6oXfKyVQUjKcYxgKDJEPhaEwh16kccZkSq").unwrap();
    let from = Pubkey::from_str("3gHYGmunh7mBWHGQ5YjqgKjy44krwenxNZ5cadZ85DtT").unwrap();
    let from_owner = payer;
    let wrapped_mint_key =
        Pubkey::from_str("5Dmmc5CC6ZpKif8iN5DSY9qNYrWJvEKcX2JrxGESqRMu").unwrap();
    let message = Pubkey::from_str("9yvM539kKjfrowv5yjuJBpTyouuD76X3J8JidobENV9s").unwrap();

    // Testnet addresses: https://docs.wormholenetwork.com/wormhole/contracts#core-bridge-1.
    let token_bridge_id = Pubkey::from_str("DZnkkTmCiFWfYTfT41X3Rd1kDgozqzxWaHqsw6W4x2oe").unwrap();
    let core_bridge_id = Pubkey::from_str("3u8hJUVTA4jH1wYAyUur7FFZVQ8H635K3tSHHF4ssjQ5").unwrap();

    let wormhole_transfer_args = WormholeTransferArgs::new(
        token_bridge_id,
        core_bridge_id,
        wrapped_mint_key,
        payer,
        from,
        from_owner,
        message,
    );
    let instruction = get_wormhole_transfer_instruction(&payload, &wormhole_transfer_args);

    let expected_accounts = vec![
        payer,
        Pubkey::from_str("8PFZNjn19BBYVHNp4H31bEW7eAmu78Yf2RKV8EeA461K").unwrap(),
        from,
        from_owner,
        wrapped_mint_key,
        Pubkey::from_str("GUvmRrbZcB6TkDZDYJ5zbZ1bNdRj9QGfuZQDgkCNhgyA").unwrap(),
        Pubkey::from_str("3VFdJkFuzrcwCwdxhKRETGxrDtUVAipNmYcLvRBDcQeH").unwrap(),
        Pubkey::from_str("6bi4JGDoRwUs9TYBuvoA7dUVyikTJDrJsJU1ew6KVLiu").unwrap(),
        message,
        Pubkey::from_str("4yttKWzRoNYS2HekxDfcZYmfQqnVWpKiJ8eydYRuFRgs").unwrap(),
        Pubkey::from_str("9QzqZZvhxoHzXbNY9y2hyAUfJUzDwyDb7fbDs9RXwH3").unwrap(),
        Pubkey::from_str("7s3a1ycs16d6SNDumaRtjcoyMaTDZPavzgsmS3uUZYWX").unwrap(),
        Pubkey::from_str("SysvarC1ock11111111111111111111111111111111").unwrap(),
        Pubkey::from_str("SysvarRent111111111111111111111111111111111").unwrap(),
        Pubkey::from_str("11111111111111111111111111111111").unwrap(),
        core_bridge_id,
        Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap(),
    ];
    let expected_data = hex::decode("04fb28000040420f0000000000000000000000000000000000000000000000000017a8a2fc5df67f8fe497149504834ae5bc35c9130300").unwrap();
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
            0x17, 0xa8, 0xa2, 0xfc, 0x5d, 0xf6, 0x7f, 0x8f, 0xe4, 0x97, 0x14, 0x95, 0x04, 0x83,
            0x4a, 0xe5, 0xbc, 0x35, 0xc9, 0x13
        ])),
    );
}

#[test]
fn test_terra_address_to_string() {
    // This is the address from the test transaction:
    // https://github.com/ChorusOne/solido/issues/445#issuecomment-988002302.
    assert_eq!(
        TerraAddress([
            0x17, 0xa8, 0xa2, 0xfc, 0x5d, 0xf6, 0x7f, 0x8f, 0xe4, 0x97, 0x14, 0x95, 0x04, 0x83,
            0x4a, 0xe5, 0xbc, 0x35, 0xc9, 0x13
        ])
        .to_string(),
        "terra1z7529lza7elcleyhzj2sfq62uk7rtjgnrqeuxr",
    );
}

#[test]
fn terra_address_to_foreign_left_pads_with_zeros() {
    assert_eq!(
        TerraAddress([
            0x17, 0xa8, 0xa2, 0xfc, 0x5d, 0xf6, 0x7f, 0x8f, 0xe4, 0x97, 0x14, 0x95, 0x04, 0x83,
            0x4a, 0xe5, 0xbc, 0x35, 0xc9, 0x13
        ])
        .to_foreign(),
        ForeignAddress([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x17, 0xa8,
            0xa2, 0xfc, 0x5d, 0xf6, 0x7f, 0x8f, 0xe4, 0x97, 0x14, 0x95, 0x04, 0x83, 0x4a, 0xe5,
            0xbc, 0x35, 0xc9, 0x13
        ])
    );
}
