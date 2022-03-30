// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use anker::{
    find_instance_address, find_reserve_authority, find_st_sol_reserve_account,
    find_ust_reserve_account,
    state::Anker,
    token::{BLamports, MicroUst},
};
use lido::{state::Lido, token::StLamports};
use solana_program::{instruction::Instruction, program_pack::Pack};
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;
use solido_cli_common::{
    error::{Error, SerializationError},
    snapshot::SnapshotConfig,
};
use spl_token_swap::curve::{constant_product::ConstantProductCurve, fees};

#[derive(Default)]
pub struct AnkerState {
    pub anker: Anker,
    pub anker_program_id: Pubkey,
    pub token_swap_program_id: Pubkey,

    pub b_sol_total_supply_amount: BLamports,
    pub pool_st_sol_account: Pubkey,
    pub pool_ust_account: Pubkey,
    pub pool_st_sol_balance: StLamports,
    pub pool_ust_balance: MicroUst,

    pub constant_product_calculator: ConstantProductCurve,
    pub pool_fees: fees::Fees,
    pub ust_mint: Pubkey,
    pub pool_mint: Pubkey,
    pub pool_fee_account: Pubkey,
    pub ust_reserve_balance: MicroUst,
    pub st_sol_reserve_balance: StLamports,
}

impl AnkerState {
    pub fn new(
        config: &mut SnapshotConfig,
        anker_program_id: &Pubkey,
        anker_address: &Pubkey,
        solido: &Lido,
    ) -> solido_cli_common::Result<Self> {
        let anker = config.client.get_anker(anker_address)?;

        let token_swap_account = config.client.get_account(&anker.token_swap_pool)?;
        let token_swap_version = token_swap_account.data()[0];
        if token_swap_version != 1 {
            let error: Error = Box::new(SerializationError {
                context: "Expected the token swap version to be 1, but found something else."
                    .to_string(),
                cause: None,
                address: anker.token_swap_pool,
            });
            return Err(error.into());
        }
        let token_swap = spl_token_swap::state::SwapV1::unpack(&token_swap_account.data()[1..])?;
        let token_swap_program_id = token_swap_account.owner;

        let (anker_ust_reserve, _anker_ust_reserve_bump_seed) =
            find_ust_reserve_account(anker_program_id, anker_address);
        let ust_reserve_balance =
            MicroUst(config.client.get_spl_token_balance(&anker_ust_reserve)?);
        let ust_account: spl_token::state::Account =
            config.client.get_unpack(&anker_ust_reserve)?;

        let (anker_st_sol_reserve, _anker_st_sol_reserve_bump_seed) =
            find_st_sol_reserve_account(anker_program_id, anker_address);
        let st_sol_reserve_balance =
            StLamports(config.client.get_spl_token_balance(&anker_st_sol_reserve)?);

        let b_sol_mint_account = config.client.get_spl_token_mint(&anker.b_sol_mint)?;
        let b_sol_total_supply_amount = BLamports(b_sol_mint_account.supply);

        let (pool_ust_account, pool_st_sol_account) =
            if token_swap.token_a_mint == solido.st_sol_mint {
                (token_swap.token_b, token_swap.token_a)
            } else {
                (token_swap.token_a, token_swap.token_b)
            };

        let pool_st_sol_balance =
            StLamports(config.client.get_spl_token_balance(&pool_st_sol_account)?);
        let pool_ust_balance = MicroUst(config.client.get_spl_token_balance(&pool_ust_account)?);

        Ok(AnkerState {
            anker_program_id: *anker_program_id,
            anker,
            b_sol_total_supply_amount,
            pool_st_sol_account,
            pool_ust_account,
            pool_st_sol_balance,
            pool_ust_balance,
            constant_product_calculator: ConstantProductCurve::default(),
            pool_fees: token_swap.fees,
            ust_mint: ust_account.mint,
            pool_mint: token_swap.pool_mint,
            pool_fee_account: token_swap.pool_fee_account,
            ust_reserve_balance,
            st_sol_reserve_balance,
            token_swap_program_id,
        })
    }

    pub fn get_fetch_pool_price_instruction(&self, solido_address: Pubkey) -> Instruction {
        let (anker_instance, _anker_bump_seed) =
            find_instance_address(&self.anker_program_id, &solido_address);

        anker::instruction::fetch_pool_price(
            &self.anker_program_id,
            &anker::instruction::FetchPoolPriceAccountsMeta {
                anker: anker_instance,
                solido: solido_address,
                token_swap_pool: self.anker.token_swap_pool,
                pool_st_sol_account: self.pool_st_sol_account,
                pool_ust_account: self.pool_ust_account,
            },
        )
    }

    pub fn get_sell_rewards_instruction(
        &self,
        solido_address: Pubkey,
        st_sol_mint: Pubkey,
    ) -> Instruction {
        let (anker_instance, _anker_bump_seed) =
            find_instance_address(&self.anker_program_id, &solido_address);
        let (anker_ust_reserve_account, _ust_reserve_bump_seed) =
            find_ust_reserve_account(&self.anker_program_id, &anker_instance);

        let (st_sol_reserve_account, _st_sol_reserve_bump_seed) =
            find_st_sol_reserve_account(&self.anker_program_id, &anker_instance);

        let (reserve_authority, _reserve_authority_bump_seed) =
            find_reserve_authority(&self.anker_program_id, &anker_instance);

        let (token_swap_authority, _authority_bump_seed) = Pubkey::find_program_address(
            &[&self.anker.token_swap_pool.to_bytes()[..]],
            &self.token_swap_program_id,
        );

        anker::instruction::sell_rewards(
            &self.anker_program_id,
            &anker::instruction::SellRewardsAccountsMeta {
                anker: anker_instance,
                solido: solido_address,
                st_sol_reserve_account,
                b_sol_mint: self.anker.b_sol_mint,
                token_swap_pool: self.anker.token_swap_pool,
                pool_st_sol_account: self.pool_st_sol_account,
                pool_ust_account: self.pool_ust_account,
                ust_reserve_account: anker_ust_reserve_account,
                pool_mint: self.pool_mint,
                st_sol_mint,
                ust_mint: self.ust_mint,
                pool_fee_account: self.pool_fee_account,
                token_swap_authority,
                reserve_authority,
                token_swap_program_id: self.token_swap_program_id,
            },
        )
    }

    /// Build the instruction to send rewards through Wormhole.
    ///
    /// Returns the instruction and one additional signer.
    pub fn get_send_rewards_instruction(
        &self,
        solido_address: Pubkey,
        maintainer_address: Pubkey,
        wormhole_nonce: u32,
    ) -> (Instruction, Keypair) {
        // In our test transaction [1], before the call to Wormhole,
        // there is a transfer of 0.000_000_010 SOL to _some_ account ... but
        // then the Wormhole call also transfers that amount. So it seems the
        // first one is a kind of tip? Can we skip it?
        // TODO(#489): // Also, we shouldn't transfer out of an account which may have more
        // balance than we need to spend, because Wormhole may steal it.
        // [1]: https://explorer.solana.com/tx/5tSRA1CYLd51sjf7Dd2ZRkLspcqiR8NH51oTd3K34sNc3PZG9uF7euE2AHE95KurrcfKYf2sCQqsEbSRmzQq8oDg?cluster=devnet
        let (anker_instance, _anker_bump_seed) =
            find_instance_address(&self.anker_program_id, &solido_address);

        let (ust_reserve_account, _ust_reserve_bump_seed) =
            find_ust_reserve_account(&self.anker_program_id, &anker_instance);

        let (reserve_authority, _reserve_authority_bump_seed) =
            find_reserve_authority(&self.anker_program_id, &anker_instance);

        // Wormhole requires allocating a new "message" account for every
        // Wormhole transaction.
        let message = Keypair::new();

        // The maintainer who is submitting this transaction pays for the Wormhole fees.
        let payer = maintainer_address;

        let transfer_args = anker::wormhole::WormholeTransferArgs::new(
            self.anker.wormhole_parameters.token_bridge_program_id,
            self.anker.wormhole_parameters.core_bridge_program_id,
            self.ust_mint,
            payer,
            ust_reserve_account,
            reserve_authority,
            message.pubkey(),
        );

        let instruction = anker::instruction::send_rewards(
            &self.anker_program_id,
            &anker::instruction::SendRewardsAccountsMeta {
                anker: anker_instance,
                solido: solido_address,
                reserve_authority,
                wormhole_token_bridge_program_id: transfer_args.token_bridge_program_id,
                wormhole_core_bridge_program_id: transfer_args.core_bridge_program_id,
                payer: transfer_args.payer,
                config_key: transfer_args.config_key,
                ust_reserve_account,
                wrapped_meta_key: transfer_args.wrapped_meta_key,
                ust_mint: self.ust_mint,
                authority_signer_key: transfer_args.authority_signer_key,
                bridge_config: transfer_args.bridge_config,
                message: message.pubkey(),
                emitter_key: transfer_args.emitter_key,
                sequence_key: transfer_args.sequence_key,
                fee_collector_key: transfer_args.fee_collector_key,
            },
            wormhole_nonce,
        );

        (instruction, message)
    }
}
