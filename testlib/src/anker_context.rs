// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Test context for testing Anker, the Anchor Protocol integration.

use solana_program::borsh::try_from_slice_unchecked;
use solana_program::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transport;

use anker::instruction;
use anker::token::{BLamports, MicroUst};
use lido::token::Lamports;
use lido::token::StLamports;
use spl_token_swap::curve::base::{CurveType, SwapCurve};
use spl_token_swap::curve::constant_product::ConstantProductCurve;
use spl_token_swap::instruction::Swap;

use crate::solido_context::send_transaction;
use crate::solido_context::{self};
use anker::{find_reserve_authority, find_st_sol_reserve_account, wormhole::TerraAddress};

// Program id for the Anker program. Only used for tests.
solana_program::declare_id!("Anker111111111111111111111111111111111111117");

pub struct TokenPoolContext {
    pub swap_account: Keypair,
    pub mint_address: Pubkey,
    pub token_address: Pubkey,
    pub fee_address: Pubkey,
    pub token_a: Pubkey,
    pub token_b: Pubkey,

    pub ust_mint_authority: Keypair,
    pub ust_mint_address: Pubkey,
}

impl TokenPoolContext {
    pub fn get_token_pool_authority(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[&self.swap_account.pubkey().to_bytes()[..]],
            &anker::orca_token_swap_v2::id(),
        )
    }

    /// Mint `amount` UST to `account`.
    pub async fn mint_ust(
        &self,
        solido_context: &mut solido_context::Context,
        account: &Pubkey,
        amount: MicroUst,
    ) {
        let mint_instruction = spl_token::instruction::mint_to(
            &spl_token::id(),
            &self.ust_mint_address,
            account,
            &self.ust_mint_authority.pubkey(),
            &[],
            amount.0,
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

    /// Returns the pair of tokens from the token pool that correspond to the
    /// UST and stSOL, respectively.
    pub async fn get_ust_stsol_addresses(
        &self,
        solido_context: &mut solido_context::Context,
    ) -> (Pubkey, Pubkey) {
        let token_a_account = solido_context.get_account(self.token_a).await;
        let token_a =
            spl_token::state::Account::unpack_from_slice(token_a_account.data.as_slice()).unwrap();

        if token_a.mint == self.ust_mint_address {
            (self.token_a, self.token_b)
        } else {
            (self.token_b, self.token_a)
        }
    }

    // Put StSOL and UST to the liquidity provider
    pub async fn provide_liquidity(
        &self,
        solido_context: &mut solido_context::Context,
        st_sol_amount: StLamports,
        ust_amount: MicroUst,
    ) {
        let (ust_address, st_sol_address) = self.get_ust_stsol_addresses(solido_context).await;

        // Transfer some UST and StSOL to the pool.
        self.mint_ust(solido_context, &ust_address, ust_amount)
            .await;

        let solido = solido_context.get_solido().await;
        let sol_amount = solido
            .exchange_rate
            .exchange_st_sol(st_sol_amount)
            .expect("Some StSol should have been minted at this point.");
        let (st_sol_keypair, token_st_sol) = solido_context.deposit(sol_amount).await;
        let balance = solido_context.get_st_sol_balance(token_st_sol).await;
        solido_context
            .transfer_spl_token(&token_st_sol, &st_sol_address, &st_sol_keypair, balance.0)
            .await;
    }

    // Initialize token pool.
    pub async fn initialize_token_pool(&mut self, solido_context: &mut solido_context::Context) {
        self.provide_liquidity(
            solido_context,
            StLamports(10_000_000_000),
            MicroUst(10_000_000_000),
        )
        .await;
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

        let (authority_pubkey, authority_bump_seed) = Pubkey::find_program_address(
            &[&self.swap_account.pubkey().to_bytes()[..]],
            &anker::orca_token_swap_v2::id(),
        );

        let pool_instruction = spl_token_swap::instruction::initialize(
            &anker::orca_token_swap_v2::id(),
            &spl_token::id(),
            &self.swap_account.pubkey(),
            &authority_pubkey,
            &self.token_a,
            &self.token_b,
            &self.mint_address,
            &self.fee_address,
            &self.token_address,
            authority_bump_seed,
            fees,
            swap_curve,
        )
        .expect("Failed to create token pool initialization instruction.");

        send_transaction(
            &mut solido_context.context,
            &mut solido_context.nonce,
            &[pool_instruction],
            vec![&self.swap_account],
        )
        .await
        .expect("Failed to initialize token pool.");
    }

    /// Get the Token Swap Pool authority.
    pub fn get_authority(&self) -> Pubkey {
        let (authority, _bump_seed) = Pubkey::find_program_address(
            &[&self.swap_account.pubkey().to_bytes()[..]],
            &anker::orca_token_swap_v2::id(),
        );
        authority
    }
}

pub struct Context {
    pub solido_context: solido_context::Context,
    pub anker: Pubkey,
    pub b_sol_mint: Pubkey,
    pub b_sol_mint_authority: Pubkey,
    pub st_sol_reserve: Pubkey,
    pub ust_reserve: Pubkey,

    pub token_pool_context: TokenPoolContext,
    pub rewards_owner: Keypair,
    pub terra_rewards_destination: TerraAddress,
    pub reserve_authority: Pubkey,
}

const INITIAL_DEPOSIT: Lamports = Lamports(1_000_000_000);

impl Context {
    pub async fn new_with_undefined_exchange_rate() -> Self {
        let mut solido_context = solido_context::Context::new_with_maintainer().await;
        let (anker, _seed) = anker::find_instance_address(&id(), &solido_context.solido.pubkey());

        let (st_sol_reserve, _seed) = anker::find_st_sol_reserve_account(&id(), &anker);
        let (ust_reserve, _seed) = anker::find_ust_reserve_account(&id(), &anker);
        let (reserve_authority, _seed) = anker::find_reserve_authority(&id(), &anker);
        let (b_sol_mint_authority, _seed) = anker::find_mint_authority(&id(), &anker);

        let b_sol_mint = solido_context.create_mint(b_sol_mint_authority).await;
        let payer = solido_context.context.payer.pubkey();

        let token_pool_context = setup_token_pool(&mut solido_context).await;

        let rewards_owner = solido_context.deterministic_keypair.new_keypair();
        let terra_rewards_destination = TerraAddress::default();

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
                    wormhole_core_bridge_program_id: Pubkey::new_unique(),
                    wormhole_token_bridge_program_id: Pubkey::new_unique(),
                    st_sol_mint: solido_context.st_sol_mint,
                    b_sol_mint,
                    st_sol_reserve_account: st_sol_reserve,
                    ust_reserve_account: ust_reserve,
                    reserve_authority,
                    token_swap_pool: token_pool_context.swap_account.pubkey(),
                    ust_mint: token_pool_context.ust_mint_address,
                },
                terra_rewards_destination.clone(),
            )],
            vec![],
        )
        .await
        .expect("Failed to initialize Anker instance.");

        Self {
            solido_context,
            anker,
            b_sol_mint,
            b_sol_mint_authority,
            st_sol_reserve,
            ust_reserve,
            token_pool_context,
            rewards_owner,
            terra_rewards_destination,
            reserve_authority,
        }
    }

    /// Create a new test context, where Solido has some balance, and the exchange
    /// rate has been updated once.
    pub async fn new() -> Self {
        let mut ctx = Context::new_with_undefined_exchange_rate().await;

        ctx.solido_context.deposit(INITIAL_DEPOSIT).await;
        ctx.solido_context.advance_to_normal_epoch(0);
        ctx.solido_context.update_exchange_rate().await;

        ctx
    }

    // Start a new Anker context with `amount` Lamports donated to Solido's
    // reserve. Also update the exchange rate. Usually used when testing a
    // different 1:1 exchange rate.
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

    pub async fn initialize_token_pool_and_deposit(&mut self, deposit_amount: Lamports) {
        self.token_pool_context
            .initialize_token_pool(&mut self.solido_context)
            .await;
        self.deposit(deposit_amount).await;
        // Donate something to Solido's reserve so we can see some rewards.
        self.solido_context
            .fund(self.solido_context.reserve_address, deposit_amount)
            .await;
        // Update the exchange rate so we see some rewards.
        self.solido_context.advance_to_normal_epoch(1);
        self.solido_context.update_exchange_rate().await;
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
                    to_reserve_account: self.st_sol_reserve,
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
                    from_b_sol_authority: user.pubkey(),
                    to_st_sol_account: recipient,
                    reserve_account: self.st_sol_reserve,
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

    /// Swap StSol for UST
    pub async fn swap_st_sol_for_ust(
        &mut self,
        source: &Pubkey,
        destination: &Pubkey,
        authority: &Keypair,
        amount_in: StLamports,
        minimum_amount_out: MicroUst,
    ) {
        let (ust_address, st_sol_address) = self
            .token_pool_context
            .get_ust_stsol_addresses(&mut self.solido_context)
            .await;
        let swap_instruction = spl_token_swap::instruction::swap(
            &anker::orca_token_swap_v2::id(),
            &spl_token::id(),
            &self.token_pool_context.swap_account.pubkey(),
            &self.token_pool_context.get_token_pool_authority().0,
            &authority.pubkey(),
            source,
            &st_sol_address,
            &ust_address,
            destination,
            &self.token_pool_context.mint_address,
            &self.token_pool_context.fee_address,
            None,
            Swap {
                amount_in: amount_in.0,
                minimum_amount_out: minimum_amount_out.0,
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

    pub async fn sell_rewards(&mut self) {
        self.try_sell_rewards()
            .await
            .expect("Failed to call SellRewards on Anker instance.")
    }

    pub async fn try_sell_rewards(&mut self) -> transport::Result<()> {
        let (st_sol_reserve_account, _reserve_account_bump_seed) =
            find_st_sol_reserve_account(&id(), &self.anker);
        let (reserve_authority, _reserve_authority_bump_seed) =
            find_reserve_authority(&id(), &self.anker);
        let (token_swap_authority, _token_pool_authority_bump_seed) =
            self.token_pool_context.get_token_pool_authority();

        let (ust_address, st_sol_address) = self
            .token_pool_context
            .get_ust_stsol_addresses(&mut self.solido_context)
            .await;

        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[instruction::sell_rewards(
                &id(),
                &instruction::SellRewardsAccountsMeta {
                    anker: self.anker,
                    solido: self.solido_context.solido.pubkey(),
                    st_sol_reserve_account,
                    b_sol_mint: self.b_sol_mint,
                    token_swap_pool: self.token_pool_context.swap_account.pubkey(),
                    pool_st_sol_account: st_sol_address,
                    pool_ust_account: ust_address,
                    pool_mint: self.token_pool_context.mint_address,
                    st_sol_mint: self.solido_context.st_sol_mint,
                    ust_mint: self.token_pool_context.ust_mint_address,
                    pool_fee_account: self.token_pool_context.fee_address,
                    token_swap_authority,
                    reserve_authority,
                    ust_reserve_account: self.ust_reserve,
                    token_swap_program_id: anker::orca_token_swap_v2::id(),
                },
            )],
            vec![],
        )
        .await?;
        Ok(())
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

    /// Return the `MicroUst` balance of the account in `address`.
    pub async fn get_ust_balance(&mut self, address: Pubkey) -> MicroUst {
        let ust_account = self.solido_context.get_account(address).await;
        let ust_spl_account: spl_token::state::Account =
            spl_token::state::Account::unpack_from_slice(ust_account.data.as_slice())
                .expect("UST account does not exist");
        MicroUst(ust_spl_account.amount)
    }

    // Create a new UST token account.
    pub async fn create_ust_token_account(&mut self, owner: Pubkey) -> Pubkey {
        self.solido_context
            .create_spl_token_account(self.token_pool_context.ust_mint_address, owner)
            .await
    }

    pub async fn try_change_terra_rewards_destination(
        &mut self,
        manager: &Keypair,
        terra_rewards_destination: TerraAddress,
    ) -> transport::Result<()> {
        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[instruction::change_terra_rewards_destination(
                &id(),
                &instruction::ChangeTerraRewardsDestinationAccountsMeta {
                    anker: self.anker,
                    solido: self.solido_context.solido.pubkey(),
                    manager: manager.pubkey(),
                },
                terra_rewards_destination,
            )],
            vec![manager],
        )
        .await?;
        Ok(())
    }

    pub async fn try_change_token_swap_pool(
        &mut self,
        token_swap_pool: Pubkey,
    ) -> transport::Result<()> {
        let anker = self.get_anker().await;
        send_transaction(
            &mut self.solido_context.context,
            &mut self.solido_context.nonce,
            &[instruction::change_token_swap_pool(
                &id(),
                &instruction::ChangeTokenSwapPoolAccountsMeta {
                    anker: self.anker,
                    solido: self.solido_context.solido.pubkey(),
                    manager: self.solido_context.manager.pubkey(),
                    current_token_swap_pool: anker.token_swap_pool,
                    new_token_swap_pool: token_swap_pool,
                },
            )],
            vec![&self.solido_context.manager],
        )
        .await?;
        Ok(())
    }

    pub async fn get_anker(&mut self) -> anker::state::Anker {
        let anker_account = self.solido_context.get_account(self.anker).await;
        // This returns a Result because it can cause an IO error, but that should
        // not happen in the test environment. (And if it does, then the test just
        // fails.)
        try_from_slice_unchecked::<anker::state::Anker>(anker_account.data.as_slice()).unwrap()
    }
}

/// Create a new token pool using `CurveType::ConstantProduct`.
///
/// The stake pool is not initialized at the end of this function. To
/// initialize the token swap instance, it requires funded token pairs on the
/// liquidity pool.
/// To get a new Context with an initialized token pool, call
/// `Context::new_with_token_pool_rewards`.
pub async fn setup_token_pool(solido_context: &mut solido_context::Context) -> TokenPoolContext {
    let admin = solido_context.deterministic_keypair.new_keypair();

    // When packing the SwapV1 structure, `SwapV1::pack(swap_info, &mut
    // dst[1..])` is called. But the program also wants the size of the data
    // to be `spl_token_swap::state::SwapV1::LEN`. `LATEST_LEN` is 1 +
    // SwapV1::LEN.
    let swap_account = solido_context
        .create_account(
            &anker::orca_token_swap_v2::id(),
            spl_token_swap::state::SwapVersion::LATEST_LEN,
        )
        .await;

    let (authority_pubkey, _authority_bump_seed) = Pubkey::find_program_address(
        &[&swap_account.pubkey().to_bytes()[..]],
        &anker::orca_token_swap_v2::id(),
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
    let token_a = solido_context
        .create_spl_token_account(ust_mint_address, authority_pubkey)
        .await;
    let token_b = solido_context
        .create_spl_token_account(solido_context.st_sol_mint, authority_pubkey)
        .await;

    TokenPoolContext {
        swap_account,
        mint_address: pool_mint_pubkey,
        token_address: pool_token_pubkey,
        fee_address: pool_fee_pubkey,
        token_a,
        token_b,
        ust_mint_authority,
        ust_mint_address,
    }
}
