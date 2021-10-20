// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Test context for testing the Anchor integration.

use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transport;

use lido::token::Lamports;
use lido::token::StLamports;

use crate::solido_context;

// Program id for the Anchor integration program. Only used for tests.
solana_program::declare_id!("AnchwRMMkz4t63Rr8P6m7mx6qBHetm8yZ4xbeoDSAeQZ");

pub struct Context {
    solido_context: solido_context::Context,
    _anker: Pubkey,
    b_sol_mint: Pubkey,
}

impl Context {
    pub async fn new() -> Context {
        let solido_context = solido_context::Context::new_with_maintainer().await;
        let (anker, _seed) =
            anchor_integration::find_instance_address(&id(), &solido_context.solido.pubkey());

        Context {
            solido_context,
            // TODO: Make this field used.
            _anker: anker,
            // TODO: Initialize mint properly.
            b_sol_mint: Pubkey::new_unique(),
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
        _from_st_sol: Pubkey,
        _amount: StLamports,
    ) -> transport::Result<Pubkey> {
        let recipient = self.create_b_sol_account(user.pubkey()).await;

        /* TODO: Actually send deposit transaction.
        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[instruction::deposit(
                &id(),
                &instruction::DepositAccountsMeta {
                    lido: self.solido.pubkey(),
                    user: user.pubkey(),
                    recipient: recipient,
                    st_sol_mint: self.st_sol_mint,
                    reserve_account: self.reserve_address,
                    mint_authority: self.mint_authority,
                },
                amount,
            )],
            vec![&user],
        )
            .await?;
        */

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
}
