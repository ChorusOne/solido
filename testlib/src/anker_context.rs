// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Test context for testing Anker, the Anchor Protocol integration.

use solana_program::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transport;

use anker::instruction;
use anker::token::BLamports;
use lido::token::Lamports;
use lido::token::StLamports;

use crate::solido_context::send_transaction;
use crate::solido_context::{self};

// Program id for the Anker program. Only used for tests.
solana_program::declare_id!("Anker111111111111111111111111111111111111117");

pub struct Context {
    pub solido_context: solido_context::Context,
    pub anker: Pubkey,
    pub b_sol_mint: Pubkey,
    pub b_sol_mint_authority: Pubkey,
    pub reserve: Pubkey,
    pub reserve_authority: Pubkey,
}

const INITIAL_DEPOSIT: Lamports = Lamports(1_000_000_000);

impl Context {
    pub async fn new() -> Context {
        let mut solido_context = solido_context::Context::new_with_maintainer().await;
        let (anker, _seed) = anker::find_instance_address(&id(), &solido_context.solido.pubkey());

        let (reserve, _seed) = anker::find_reserve_account(&id(), &anker);
        let (reserve_authority, _seed) = anker::find_reserve_authority(&id(), &anker);
        let (b_sol_mint_authority, _seed) = anker::find_mint_authority(&id(), &anker);

        let b_sol_mint = solido_context.create_mint(b_sol_mint_authority).await;

        let payer = solido_context.context.payer.pubkey();

        send_transaction(
            &mut solido_context.context,
            &mut solido_context.nonce,
            &[instruction::initialize(
                &id(),
                &instruction::InitializeAccountsMeta {
                    fund_rent_from: payer,
                    anker,
                    solido: solido_context.solido.pubkey(),
                    solido_program: solido_context::id(),
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

        Context {
            solido_context,
            anker,
            b_sol_mint,
            b_sol_mint_authority,
            reserve,
            reserve_authority,
        }
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
                    anker: self.anker,
                    solido: self.solido_context.solido.pubkey(),
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

    /// Create a new stSOL account owned by the user, and withdraw into it.
    pub async fn try_withdraw(
        &mut self,
        user: &Keypair,
        b_sol_account: Pubkey,
        amount: BLamports,
    ) -> transport::Result<Pubkey> {
        let recipient = self
            .solido_context
            .create_st_sol_account(user.pubkey())
            .await;

        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[instruction::withdraw(
                &id(),
                &instruction::WithdrawAccountsMeta {
                    anker: self.anker,
                    solido: self.solido_context.solido.pubkey(),
                    from_b_sol_account: b_sol_account,
                    from_b_sol_authority: user.pubkey().clone(),
                    to_st_sol_account: recipient,
                    reserve_account: self.reserve,
                    reserve_authority: self.reserve_authority,
                    b_sol_mint: self.b_sol_mint,
                },
                amount,
            )],
            vec![user],
        )
        .await?;

        Ok(recipient)
    }

    /// Create a new stSOL account owned by the user, and withdraw into it.
    pub async fn withdraw(
        &mut self,
        user: &Keypair,
        b_sol_account: Pubkey,
        amount: BLamports,
    ) -> Pubkey {
        self.try_withdraw(user, b_sol_account, amount)
            .await
            .expect("Failed to call Withdraw on Anker instance.")
    }

    /// Get the bSOL balance from an SPL token account.
    pub async fn get_b_sol_balance(&mut self, address: Pubkey) -> BLamports {
        let token_account = self.solido_context.get_account(address).await;
        let account_info: spl_token::state::Account =
            spl_token::state::Account::unpack_from_slice(token_account.data.as_slice()).unwrap();

        assert_eq!(account_info.mint, self.b_sol_mint);
        BLamports(account_info.amount)
    }

    /// Return the value of the given amount of stSOL in SOL.
    pub async fn exchange_st_sol(&mut self, amount: StLamports) -> Lamports {
        let solido = self.solido_context.get_solido().await;
        solido.exchange_rate.exchange_st_sol(amount).unwrap()
    }

    /// Return the current amount of bSOL in existence.
    pub async fn get_b_sol_supply(&mut self) -> BLamports {
        let mint_account = self.solido_context.get_account(self.b_sol_mint).await;
        let mint: spl_token::state::Mint =
            spl_token::state::Mint::unpack_from_slice(mint_account.data.as_slice()).unwrap();
        BLamports(mint.supply)
    }
}
