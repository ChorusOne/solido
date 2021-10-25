// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Test context for testing the Anchor integration.

use solana_program::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transport;

use anchor_integration::instruction;
use anchor_integration::token::BLamports;
use lido::token::Lamports;
use lido::token::StLamports;
use spl_token_swap::curve::base::{CurveType, SwapCurve};
use spl_token_swap::curve::constant_product::ConstantProductCurve;

use crate::solido_context::send_transaction;
use crate::solido_context::{self};

// Program id for the Anchor integration program. Only used for tests.
solana_program::declare_id!("AnchwRMMkz4t63Rr8P6m7mx6qBHetm8yZ4xbeoDSAeQZ");

pub struct Context {
    pub solido_context: solido_context::Context,
    pub anker: Pubkey,
    pub b_sol_mint: Pubkey,
    pub b_sol_mint_authority: Pubkey,
    pub reserve: Pubkey,
}

const INITIAL_DEPOSIT: Lamports = Lamports(1_000_000_000);

impl Context {
    pub async fn new() -> Context {
        let mut solido_context = solido_context::Context::new_with_maintainer().await;
        let (anker, _seed) =
            anchor_integration::find_instance_address(&id(), &solido_context.solido.pubkey());

        let (reserve, _seed) = anchor_integration::find_reserve_account(&id(), &anker);
        let (reserve_authority, _seed) = anchor_integration::find_reserve_authority(&id(), &anker);
        let (b_sol_mint_authority, _seed) = anchor_integration::find_mint_authority(&id(), &anker);

        let b_sol_mint = solido_context.create_mint(b_sol_mint_authority).await;

        let payer = solido_context.context.payer.pubkey();

        send_transaction(
            &mut solido_context.context,
            &mut solido_context.nonce,
            &[instruction::initialize(
                &id(),
                &instruction::InitializeAccountsMeta {
                    fund_rent_from: payer,
                    anchor: anker,
                    lido: solido_context.solido.pubkey(),
                    lido_program: solido_context::id(),
                    st_sol_mint: solido_context.st_sol_mint,
                    b_sol_mint,
                    reserve_account: reserve,
                    reserve_authority,
                },
            )],
            vec![],
        )
        .await
        .expect("Failed to initialize Anker instance.");

        solido_context.deposit(INITIAL_DEPOSIT).await;
        solido_context.advance_to_normal_epoch(0);
        solido_context.update_exchange_rate().await;

        let mut context = Context {
            solido_context,
            anker,
            b_sol_mint,
            b_sol_mint_authority,
            reserve,
        };
        context.initialize_token_pool().await;
        context
    }

    pub async fn new_different_exchange_rate() -> Context {
        let mut context = Context::new().await;
        context
            .solido_context
            .fund(context.solido_context.reserve_address, INITIAL_DEPOSIT)
            .await;
        context.solido_context.advance_to_normal_epoch(1);
        context.solido_context.update_exchange_rate().await;
        context
    }

    /// Create a new SPL token account holding bSOL, return its address.
    pub async fn create_b_sol_account(&mut self, owner: Pubkey) -> Pubkey {
        self.solido_context
            .create_spl_token_account(self.b_sol_mint, owner)
            .await
    }

    /// Deposit some of the stSOL in the `from_st_sol` account, get bSOL.
    ///
    /// Returns the resulting bSOL account.
    pub async fn try_deposit_st_sol(
        &mut self,
        user: &Keypair,
        from_st_sol: Pubkey,
        amount: StLamports,
    ) -> transport::Result<Pubkey> {
        let recipient = self.create_b_sol_account(user.pubkey()).await;

        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[instruction::deposit(
                &id(),
                &instruction::DepositAccountsMeta {
                    anchor: self.anker,
                    lido: self.solido_context.solido.pubkey(),
                    lido_program: solido_context::id(),
                    from_account: from_st_sol,
                    user_authority: user.pubkey(),
                    to_reserve_account: self.reserve,
                    b_sol_user_account: recipient,
                    b_sol_mint: self.b_sol_mint,
                    b_sol_mint_authority: self.b_sol_mint_authority,
                },
                amount,
            )],
            vec![user],
        )
        .await?;

        Ok(recipient)
    }

    /// Deposit `amount` into Solido to get stSOL, deposit that into Anker to get bSOL.
    ///
    /// Returns the owner, and the bSOL account.
    pub async fn try_deposit(&mut self, amount: Lamports) -> transport::Result<(Keypair, Pubkey)> {
        // Note, we use `deposit` here, not `try_deposit`, because we assume in these
        // tests that the Solido part does not fail. If we intentionally make a transaction
        // fail, it should fail when calling Anker, not Solido.
        let (user, st_sol_account) = self.solido_context.deposit(amount).await;
        let balance = self.solido_context.get_st_sol_balance(st_sol_account).await;
        let b_sol_account = self
            .try_deposit_st_sol(&user, st_sol_account, balance)
            .await?;
        Ok((user, b_sol_account))
    }

    /// Deposit `amount` into Solido to get stSOL, deposit that into Anker to get bSOL.
    ///
    /// Returns the owner, and the bSOL account.
    pub async fn deposit(&mut self, amount: Lamports) -> (Keypair, Pubkey) {
        self.try_deposit(amount)
            .await
            .expect("Failed to call Deposit on Anker instance.")
    }

    /// Get the bSOL balance from an SPL token account.
    pub async fn get_b_sol_balance(&mut self, address: Pubkey) -> BLamports {
        let token_account = self.solido_context.get_account(address).await;
        let account_info: spl_token::state::Account =
            spl_token::state::Account::unpack_from_slice(token_account.data.as_slice()).unwrap();

        assert_eq!(account_info.mint, self.b_sol_mint);
        BLamports(account_info.amount)
    }

    pub async fn initialize_token_pool(&mut self) {
        let swap_account = self.solido_context.deterministic_keypair.new_keypair();

        let (authority_pubkey, authority_bump_seed) = Pubkey::find_program_address(
            &[&swap_account.pubkey().to_bytes()[..]],
            &spl_token_swap::id(),
        );

        let admin = self.solido_context.deterministic_keypair.new_keypair();

        let pool_mint_account = self.solido_context.create_mint(authority_pubkey).await;
        let pool_token_account = self
            .solido_context
            .create_spl_token_account(pool_mint_account, admin.pubkey())
            .await;
        let pool_fee_account = self
            .solido_context
            .create_spl_token_account(pool_mint_account, admin.pubkey())
            .await;

        let st_sol_account = self
            .solido_context
            .create_spl_token_account(self.solido_context.st_sol_mint, authority_pubkey)
            .await;
        let b_sol_account = self
            .solido_context
            .create_spl_token_account(self.b_sol_mint, authority_pubkey)
            .await;

        let (kp_bsol, _) = self.deposit(Lamports(1_000_000_000)).await;
        let (kp_stsol, _) = self.solido_context.deposit(Lamports(1_000_000_000)).await;

        let fees = spl_token_swap::curve::fees::Fees {
            trade_fee_numerator: 0,
            trade_fee_denominator: 10,
            owner_trade_fee_numerator: 0,
            owner_trade_fee_denominator: 10,
            owner_withdraw_fee_numerator: 0,
            owner_withdraw_fee_denominator: 10,
            host_fee_numerator: 0,
            host_fee_denominator: 10,
        };
        let swap_curve = SwapCurve {
            curve_type: CurveType::ConstantProduct,
            calculator: Box::new(ConstantProductCurve),
        };

        let pool_instruction = spl_token_swap::instruction::initialize(
            &spl_token_swap::id(),
            &spl_token::id(),
            &swap_account.pubkey(),
            &authority_pubkey,
            &st_sol_account,
            &b_sol_account,
            &pool_mint_account,
            &pool_fee_account,
            &pool_token_account,
            authority_bump_seed,
            fees,
            swap_curve,
        )
        .expect("Failed to create token pool initialization instruction.");

        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[pool_instruction],
            vec![&swap_account],
        )
        .await
        .expect("Failed to initialize token pool.");
    }
}
