// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Test context for testing the Anchor integration.

use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transport;

use anchor_integration::instruction;
use lido::token::Lamports;
use lido::token::StLamports;

use crate::solido_context;
use crate::solido_context::send_transaction;

// Program id for the Anchor integration program. Only used for tests.
solana_program::declare_id!("AnchwRMMkz4t63Rr8P6m7mx6qBHetm8yZ4xbeoDSAeQZ");

pub struct Context {
    solido_context: solido_context::Context,
    anker: Pubkey,
    b_sol_mint: Pubkey,
    b_sol_mint_authority: Pubkey,
    reserve: Pubkey,
}

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

        solido_context.deposit(Lamports(1_000_000_000)).await;
        solido_context.advance_to_normal_epoch(1);
        solido_context.update_exchange_rate().await;

        Context {
            solido_context,
            anker,
            b_sol_mint,
            b_sol_mint_authority,
            reserve,
        }
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
        println!("BALANCE {}", balance);
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
}
