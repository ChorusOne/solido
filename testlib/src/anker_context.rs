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
use spl_token_swap::instruction::Swap;

use crate::solido_context::send_transaction;
use crate::solido_context::{self};
use anker::{find_reserve_account, find_reserve_authority};

// Program id for the Anker program. Only used for tests.
solana_program::declare_id!("Anker111111111111111111111111111111111111117");

pub struct TokenPoolContext {
    pub swap_account: Keypair,
    pub mint_address: Pubkey,
    pub token_address: Pubkey,
    pub fee_address: Pubkey,
    pub st_sol_address: Pubkey,
    pub ust_address: Pubkey,

    pub ust_mint_authority: Keypair,
    pub ust_mint_address: Pubkey,
}

impl TokenPoolContext {
    pub fn get_token_pool_authority(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[&self.swap_account.pubkey().to_bytes()[..]],
            &spl_token_swap::id(),
        )
    }

    /// Mint `amount` UST to `account`.
    pub async fn mint_ust(
        &self,
        solido_context: &mut solido_context::Context,
        account: &Pubkey,
        amount: u64,
    ) {
        let mint_instruction = spl_token::instruction::mint_to(
            &spl_token::id(),
            &self.ust_mint_address,
            account,
            &self.ust_mint_authority.pubkey(),
            &[],
            amount,
        )
        .expect("Failed to generate UST mint instruction.");
        send_transaction(
            &mut solido_context.context,
            &mut solido_context.nonce,
            &[mint_instruction],
            vec![&self.ust_mint_authority],
        )
        .await
        .expect("Failed to mint UST tokens.");
    }
}

pub struct Context {
    pub solido_context: solido_context::Context,
    pub anker: Pubkey,
    pub b_sol_mint: Pubkey,
    pub b_sol_mint_authority: Pubkey,
    pub reserve: Pubkey,

    pub token_pool_context: TokenPoolContext,
    pub rewards_owner: Keypair,
    pub ust_rewards_account: Pubkey,
}

const INITIAL_DEPOSIT: Lamports = Lamports(1_000_000_000);

impl Context {
    pub async fn new() -> Self {
        let mut solido_context = solido_context::Context::new_with_maintainer().await;
        let (anker, _seed) = anker::find_instance_address(&id(), &solido_context.solido.pubkey());

        let (reserve, _seed) = anker::find_reserve_account(&id(), &anker);
        let (reserve_authority, _seed) = anker::find_reserve_authority(&id(), &anker);
        let (b_sol_mint_authority, _seed) = anker::find_mint_authority(&id(), &anker);

        let b_sol_mint = solido_context.create_mint(b_sol_mint_authority).await;
        let payer = solido_context.context.payer.pubkey();

        let token_pool_context = initialize_token_pool(&mut solido_context).await;

        let rewards_owner = solido_context.deterministic_keypair.new_keypair();
        let ust_rewards_account = solido_context
            .create_spl_token_account(token_pool_context.ust_mint_address, rewards_owner.pubkey())
            .await;

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
                    token_swap_instance: token_pool_context.swap_account.pubkey(),
                    rewards_destination: ust_rewards_account,
                },
            )],
            vec![],
        )
        .await
        .expect("Failed to initialize Anker instance.");

        solido_context.deposit(INITIAL_DEPOSIT).await;
        solido_context.advance_to_normal_epoch(0);
        solido_context.update_exchange_rate().await;

        Self {
            solido_context,
            anker,
            b_sol_mint,
            b_sol_mint_authority,
            reserve,
            token_pool_context,
            rewards_owner,
            ust_rewards_account,
        }
    }

    pub async fn new_different_exchange_rate(amount: Lamports) -> Context {
        let mut context = Context::new().await;
        context
            .solido_context
            .fund(context.solido_context.reserve_address, amount)
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

    /// Get the bSOL balance from an SPL token account.
    pub async fn get_b_sol_balance(&mut self, address: Pubkey) -> BLamports {
        let token_account = self.solido_context.get_account(address).await;
        let account_info: spl_token::state::Account =
            spl_token::state::Account::unpack_from_slice(token_account.data.as_slice()).unwrap();

        assert_eq!(account_info.mint, self.b_sol_mint);
        BLamports(account_info.amount)
    }

    /// Swap StSol for UST
    pub async fn swap_st_sol_for_ust(
        &mut self,
        source: &Pubkey,
        destination: &Pubkey,
        authority: &Keypair,
        amount_in: u64,
        minimum_amount_out: u64,
    ) {
        let swap_instruction = spl_token_swap::instruction::swap(
            &spl_token_swap::id(),
            &spl_token::id(),
            &self.token_pool_context.swap_account.pubkey(),
            &self.token_pool_context.get_token_pool_authority().0,
            &authority.pubkey(),
            source,
            &self.token_pool_context.st_sol_address,
            &self.token_pool_context.ust_address,
            destination,
            &self.token_pool_context.mint_address,
            &self.token_pool_context.fee_address,
            None,
            Swap {
                amount_in,
                minimum_amount_out,
            },
        )
        .expect("Could not create swap instruction.");
        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[swap_instruction],
            vec![authority],
        )
        .await
        .expect("Failed to swap StSol for UST tokens.");
    }

    pub async fn claim_rewards(&mut self) {
        let (reserve_account, _reserve_account_bump_seed) =
            find_reserve_account(&id(), &self.anker);
        let (reserve_authority, _reserve_authority_bump_seed) =
            find_reserve_authority(&id(), &self.anker);
        let (token_pool_authority, _token_pool_authority_bump_seed) =
            self.token_pool_context.get_token_pool_authority();
        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[instruction::claim_rewards(
                &id(),
                &instruction::ClaimRewardsAccountsMeta {
                    anker: self.anker,
                    solido: self.solido_context.solido.pubkey(),
                    reserve_account,
                    b_sol_mint: self.b_sol_mint,
                    token_swap_instance: self.token_pool_context.swap_account.pubkey(),
                    st_sol_token: self.token_pool_context.st_sol_address,
                    ust_token: self.token_pool_context.ust_address,
                    pool_mint: self.token_pool_context.mint_address,
                    st_sol_mint: self.solido_context.st_sol_mint,
                    ust_mint: self.token_pool_context.ust_mint_address,
                    pool_fee_account: self.token_pool_context.fee_address,
                    token_pool_authority,
                    reserve_authority,
                    rewards_destination: self.ust_rewards_account,
                },
            )],
            vec![],
        )
        .await
        .expect("Failed to claim rewards.");
    }
}

/// Create a new token pool using `CurveType::ConstantProduct`.
///
/// Fund UST and StSOL with 10 * 1e9 Lamports each.
pub async fn initialize_token_pool(
    solido_context: &mut solido_context::Context,
) -> TokenPoolContext {
    let admin = solido_context.deterministic_keypair.new_keypair();

    // When packing the SwapV1 structure, `SwapV1::pack(swap_info, &mut
    // dst[1..])` is called. But the program also wants the size of the data
    // to be `spl_token_swap::state::SwapV1::LEN`. That is why we add the
    // `+1` to the size 🤷 .
    let swap_account = solido_context
        .create_account(
            &spl_token_swap::id(),
            spl_token_swap::state::SwapV1::LEN + 1,
        )
        .await;

    let (authority_pubkey, _authority_bump_seed) = Pubkey::find_program_address(
        &[&swap_account.pubkey().to_bytes()[..]],
        &spl_token_swap::id(),
    );

    let pool_mint_pubkey = solido_context.create_mint(authority_pubkey).await;
    let pool_token_pubkey = solido_context
        .create_spl_token_account(pool_mint_pubkey, admin.pubkey())
        .await;
    let pool_fee_pubkey = solido_context
        .create_spl_token_account(pool_mint_pubkey, admin.pubkey())
        .await;

    // Create UST token
    let ust_mint_authority = solido_context.deterministic_keypair.new_keypair();
    let ust_mint_address = solido_context
        .create_mint(ust_mint_authority.pubkey())
        .await;

    // UST and StSOL token accounts for the pool.
    let ust_account = solido_context
        .create_spl_token_account(ust_mint_address, authority_pubkey)
        .await;
    let st_sol_account = solido_context
        .create_spl_token_account(solido_context.st_sol_mint, authority_pubkey)
        .await;

    let token_pool_context = TokenPoolContext {
        swap_account,
        mint_address: pool_mint_pubkey,
        token_address: pool_token_pubkey,
        fee_address: pool_fee_pubkey,
        st_sol_address: st_sol_account,
        ust_address: ust_account,
        ust_mint_authority,
        ust_mint_address,
    };

    // Transfer some UST and StSOL to the pool.
    token_pool_context
        .mint_ust(solido_context, &ust_account, 10_000_000_000)
        .await;
    let (kp_stsol, token_st_sol) = solido_context.deposit(Lamports(10_000_000_000)).await;
    solido_context
        .transfer_spl_token(&token_st_sol, &st_sol_account, &kp_stsol, 10_000_000_000)
        .await;

    token_pool_context
}
