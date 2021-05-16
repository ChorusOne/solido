#![allow(dead_code)]

use solana_program::sysvar;

use super::{create_token_account, LidoAccounts};

use {
    solana_program::{
        borsh::get_packed_len, hash::Hash, program_pack::Pack, pubkey::Pubkey, system_instruction,
        system_program,
    },
    solana_program_test::*,
    solana_sdk::{
        account::Account,
        signature::{Keypair, Signer},
        transaction::Transaction,
        transport::TransportError,
    },
    solana_vote_program::{
        self, vote_instruction,
        vote_state::{VoteInit, VoteState},
    },
    spl_stake_pool::{
        self,
        borsh::{get_instance_packed_len, try_from_slice_unchecked},
        find_stake_program_address, find_transient_stake_program_address, stake_program, state,
    },
};

pub const TEST_STAKE_AMOUNT: u64 = 1_500_000_000;
pub const MAX_TEST_VALIDATORS: u32 = 10_000;

pub async fn get_account(banks_client: &mut BanksClient, pubkey: &Pubkey) -> Account {
    banks_client
        .get_account(*pubkey)
        .await
        .expect("account not found")
        .expect("account empty")
}

pub async fn create_mint(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    pool_mint: &Keypair,
    manager: &Pubkey,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let mint_rent = rent.minimum_balance(spl_token::state::Mint::LEN);

    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &pool_mint.pubkey(),
                mint_rent,
                spl_token::state::Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &pool_mint.pubkey(),
                &manager,
                None,
                0,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, pool_mint], *recent_blockhash);
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn transfer(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    recipient: &Pubkey,
    amount: u64,
) {
    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::transfer(
            &payer.pubkey(),
            recipient,
            amount,
        )],
        Some(&payer.pubkey()),
        &[payer],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();
}

pub async fn mint_tokens(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    mint: &Pubkey,
    account: &Pubkey,
    mint_authority: &Keypair,
    amount: u64,
) -> Result<(), TransportError> {
    let transaction = Transaction::new_signed_with_payer(
        &[spl_token::instruction::mint_to(
            &spl_token::id(),
            mint,
            account,
            &mint_authority.pubkey(),
            &[],
            amount,
        )
        .unwrap()],
        Some(&payer.pubkey()),
        &[payer, mint_authority],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn get_token_balance(banks_client: &mut BanksClient, token: &Pubkey) -> u64 {
    let token_account = banks_client.get_account(*token).await.unwrap().unwrap();
    let account_info: spl_token::state::Account =
        spl_token::state::Account::unpack_from_slice(token_account.data.as_slice()).unwrap();
    account_info.amount
}

pub async fn get_token_supply(banks_client: &mut BanksClient, mint: &Pubkey) -> u64 {
    let mint_account = banks_client.get_account(*mint).await.unwrap().unwrap();
    let account_info =
        spl_token::state::Mint::unpack_from_slice(mint_account.data.as_slice()).unwrap();
    account_info.supply
}

pub async fn delegate_tokens(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    account: &Pubkey,
    manager: &Keypair,
    delegate: &Pubkey,
    amount: u64,
) {
    let transaction = Transaction::new_signed_with_payer(
        &[spl_token::instruction::approve(
            &spl_token::id(),
            &account,
            &delegate,
            &manager.pubkey(),
            &[],
            amount,
        )
        .unwrap()],
        Some(&payer.pubkey()),
        &[payer, manager],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();
}

#[allow(clippy::too_many_arguments)]
pub async fn create_stake_pool(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake_pool: &Keypair,
    validator_list: &Keypair,
    reserve_stake: &Pubkey,
    pool_mint: &Pubkey,
    pool_token_account: &Pubkey,
    manager: &Keypair,
    staker: &Pubkey,
    deposit_authority: &Pubkey,
    fee: &state::Fee,
    max_validators: u32,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let rent_stake_pool = rent.minimum_balance(get_packed_len::<state::StakePool>());
    let validator_list_size =
        get_instance_packed_len(&state::ValidatorList::new(max_validators)).unwrap();
    let rent_validator_list = rent.minimum_balance(validator_list_size);

    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &stake_pool.pubkey(),
                rent_stake_pool,
                get_packed_len::<state::StakePool>() as u64,
                &spl_stake_pool::id(),
            ),
            system_instruction::create_account(
                &payer.pubkey(),
                &validator_list.pubkey(),
                rent_validator_list,
                validator_list_size as u64,
                &spl_stake_pool::id(),
            ),
            lido::instruction::initialize_stake_pool_with_authority(
                &spl_stake_pool::id(),
                &lido::instruction::InitializeStakePoolWithAuthorityAccountsMeta {
                    stake_pool: stake_pool.pubkey(),
                    manager: manager.pubkey(),
                    staker: *staker,
                    validator_list: validator_list.pubkey(),
                    reserve_stake: *reserve_stake,
                    pool_mint: *pool_mint,
                    manager_fee_account: *pool_token_account,
                    deposit_authority: *deposit_authority,
                    sysvar_clock: sysvar::clock::id(),
                    sysvar_rent: sysvar::rent::id(),
                    sysvar_token: spl_token::id(),
                },
                *fee,
                max_validators,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(
        &vec![payer, stake_pool, validator_list, manager],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn create_vote(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    validator: &Keypair,
    vote: &Keypair,
) {
    let rent = banks_client.get_rent().await.unwrap();
    let rent_voter = rent.minimum_balance(VoteState::size_of());

    let mut instructions = vec![system_instruction::create_account(
        &payer.pubkey(),
        &validator.pubkey(),
        42,
        0,
        &system_program::id(),
    )];
    instructions.append(&mut vote_instruction::create_account(
        &payer.pubkey(),
        &vote.pubkey(),
        &VoteInit {
            node_pubkey: validator.pubkey(),
            authorized_voter: validator.pubkey(),
            ..VoteInit::default()
        },
        rent_voter,
    ));

    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[validator, vote, payer],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();
}

pub async fn create_independent_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Keypair,
    authorized: &stake_program::Authorized,
    lockup: &stake_program::Lockup,
    stake_amount: u64,
) -> u64 {
    let rent = banks_client.get_rent().await.unwrap();
    let lamports =
        rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()) + stake_amount;

    let transaction = Transaction::new_signed_with_payer(
        &stake_program::create_account(
            &payer.pubkey(),
            &stake.pubkey(),
            authorized,
            lockup,
            lamports,
        ),
        Some(&payer.pubkey()),
        &[payer, stake],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    lamports
}

pub async fn create_blank_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Keypair,
) -> u64 {
    let rent = banks_client.get_rent().await.unwrap();
    let lamports = rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()) + 1;

    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::create_account(
            &payer.pubkey(),
            &stake.pubkey(),
            lamports,
            std::mem::size_of::<stake_program::StakeState>() as u64,
            &stake_program::id(),
        )],
        Some(&payer.pubkey()),
        &[payer, stake],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    lamports
}

pub async fn delegate_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Pubkey,
    authorized: &Keypair,
    vote: &Pubkey,
) {
    let mut transaction = Transaction::new_with_payer(
        &[stake_program::delegate_stake(
            &stake,
            &authorized.pubkey(),
            &vote,
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, authorized], *recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();
}

pub async fn authorize_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Pubkey,
    authorized: &Keypair,
    new_authorized: &Pubkey,
    stake_authorize: stake_program::StakeAuthorize,
) {
    let mut transaction = Transaction::new_with_payer(
        &[stake_program::authorize(
            &stake,
            &authorized.pubkey(),
            &new_authorized,
            stake_authorize,
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, authorized], *recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();
}

pub struct ValidatorStakeAccount {
    pub stake_account: Pubkey,
    pub transient_stake_account: Pubkey,
    pub vote: Keypair,
    pub validator: Keypair,
    pub stake_pool: Pubkey,
    pub validator_token_account: Keypair,
}

impl ValidatorStakeAccount {
    pub fn new(stake_pool: &Pubkey) -> Self {
        let validator = Keypair::new();
        let vote = Keypair::new();
        let validator_token_account = Keypair::new();
        let (stake_account, _) =
            find_stake_program_address(&spl_stake_pool::id(), &vote.pubkey(), stake_pool);
        let (transient_stake_account, _) =
            find_transient_stake_program_address(&spl_stake_pool::id(), &vote.pubkey(), stake_pool);
        ValidatorStakeAccount {
            stake_account,
            transient_stake_account,
            vote,
            validator,
            stake_pool: *stake_pool,
            validator_token_account,
        }
    }

    pub async fn create_and_delegate(
        &self,
        mut banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        lido_accounts: &LidoAccounts,
        staker: &Pubkey,
    ) {
        create_vote(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.validator,
            &self.vote,
        )
        .await;

        lido_accounts
            .create_validator_stake_account(
                banks_client,
                payer,
                recent_blockhash,
                staker,
                &self.stake_account,
                &self.vote.pubkey(),
            )
            .await;
    }
}

pub struct StakePoolAccounts {
    pub stake_pool: Keypair,
    pub validator_list: Keypair,
    pub reserve_stake: Keypair,
    pub pool_mint: Keypair,
    pub pool_fee_account: Keypair,
    pub staker: Pubkey,
    pub withdraw_authority: Pubkey,
    pub deposit_authority: Pubkey,
    pub deposit_authority_keypair: Option<Keypair>,
    pub fee: state::Fee,
    pub max_validators: u32,
}

impl StakePoolAccounts {
    pub fn new(staker: Pubkey) -> Self {
        let stake_pool = Keypair::new();
        let validator_list = Keypair::new();
        let stake_pool_address = &stake_pool.pubkey();
        let (withdraw_authority, _) = Pubkey::find_program_address(
            &[&stake_pool_address.to_bytes()[..32], b"withdraw"],
            &spl_stake_pool::id(),
        );
        let (deposit_authority, _) = Pubkey::find_program_address(
            &[&stake_pool_address.to_bytes()[..32], b"deposit"],
            &spl_stake_pool::id(),
        );
        let reserve_stake = Keypair::new();
        let pool_mint = Keypair::new();
        let pool_fee_account = Keypair::new();

        Self {
            stake_pool,
            validator_list,
            reserve_stake,
            pool_mint,
            pool_fee_account,
            staker,
            withdraw_authority,
            deposit_authority,
            deposit_authority_keypair: None,
            fee: state::Fee {
                numerator: 10,
                denominator: 100,
            },
            max_validators: MAX_TEST_VALIDATORS,
        }
    }

    pub fn new_with_deposit_authority(staker: Pubkey, deposit_authority: Keypair) -> Self {
        let mut stake_pool_accounts = Self::new(staker);
        stake_pool_accounts.deposit_authority = deposit_authority.pubkey();
        stake_pool_accounts.deposit_authority_keypair = Some(deposit_authority);
        stake_pool_accounts
    }

    pub fn calculate_fee(&self, amount: u64) -> u64 {
        amount * self.fee.numerator / self.fee.denominator
    }

    pub async fn initialize_stake_pool(
        &self,
        mut banks_client: &mut BanksClient,
        payer: &Keypair,
        manager: &Keypair,
        recent_blockhash: &Hash,
        reserve_lamports: u64,
        fee_manager: &Pubkey,
    ) -> Result<(), TransportError> {
        create_mint(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.pool_mint,
            &self.withdraw_authority,
        )
        .await?;
        create_token_account(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.pool_fee_account,
            &self.pool_mint.pubkey(),
            &fee_manager,
        )
        .await?;
        create_independent_stake_account(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.reserve_stake,
            &stake_program::Authorized {
                staker: self.withdraw_authority,
                withdrawer: self.withdraw_authority,
            },
            &stake_program::Lockup::default(),
            reserve_lamports,
        )
        .await;
        create_stake_pool(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.stake_pool,
            &self.validator_list,
            &self.reserve_stake.pubkey(),
            &self.pool_mint.pubkey(),
            &self.pool_fee_account.pubkey(),
            manager,
            &self.staker,
            &self.deposit_authority,
            &self.fee,
            self.max_validators,
        )
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deposit_stake(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        stake: &Pubkey,
        pool_account: &Pubkey,
        validator_stake_account: &Pubkey,
        current_staker: &Keypair,
    ) -> Result<(), TransportError> {
        let mut signers = vec![payer, current_staker];
        let instructions = if let Some(deposit_authority) = self.deposit_authority_keypair.as_ref()
        {
            signers.push(deposit_authority);
            spl_stake_pool::instruction::deposit_with_authority(
                &spl_stake_pool::id(),
                &self.stake_pool.pubkey(),
                &self.validator_list.pubkey(),
                &self.deposit_authority,
                &self.withdraw_authority,
                stake,
                &current_staker.pubkey(),
                validator_stake_account,
                pool_account,
                &self.pool_mint.pubkey(),
                &spl_token::id(),
            )
        } else {
            spl_stake_pool::instruction::deposit(
                &spl_stake_pool::id(),
                &self.stake_pool.pubkey(),
                &self.validator_list.pubkey(),
                &self.withdraw_authority,
                stake,
                &current_staker.pubkey(),
                validator_stake_account,
                pool_account,
                &self.pool_mint.pubkey(),
                &spl_token::id(),
            )
        };
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&payer.pubkey()),
            &signers,
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn withdraw_stake(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        stake_recipient: &Pubkey,
        user_transfer_authority: &Keypair,
        pool_account: &Pubkey,
        validator_stake_account: &Pubkey,
        recipient_new_authority: &Pubkey,
        amount: u64,
    ) -> Option<TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[spl_stake_pool::instruction::withdraw(
                &spl_stake_pool::id(),
                &self.stake_pool.pubkey(),
                &self.validator_list.pubkey(),
                &self.withdraw_authority,
                validator_stake_account,
                stake_recipient,
                recipient_new_authority,
                &user_transfer_authority.pubkey(),
                pool_account,
                &self.pool_mint.pubkey(),
                &spl_token::id(),
                amount,
            )
            .unwrap()],
            Some(&payer.pubkey()),
            &[payer, user_transfer_authority],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.err()
    }

    pub async fn update_validator_list_balance(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator_vote_accounts: &[Pubkey],
        no_merge: bool,
    ) -> Option<TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[spl_stake_pool::instruction::update_validator_list_balance(
                &spl_stake_pool::id(),
                &self.stake_pool.pubkey(),
                &self.withdraw_authority,
                &self.validator_list.pubkey(),
                &self.reserve_stake.pubkey(),
                validator_vote_accounts,
                0,
                no_merge,
            )],
            Some(&payer.pubkey()),
            &[payer],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.err()
    }

    pub async fn update_stake_pool_balance(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Option<TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[spl_stake_pool::instruction::update_stake_pool_balance(
                &spl_stake_pool::id(),
                &self.stake_pool.pubkey(),
                &self.withdraw_authority,
                &self.validator_list.pubkey(),
                &self.reserve_stake.pubkey(),
                &self.pool_fee_account.pubkey(),
                &self.pool_mint.pubkey(),
            )],
            Some(&payer.pubkey()),
            &[payer],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.err()
    }

    pub async fn update_all(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator_vote_accounts: &[Pubkey],
        no_merge: bool,
    ) -> Option<TransportError> {
        let transaction = Transaction::new_signed_with_payer(
            &[
                spl_stake_pool::instruction::update_validator_list_balance(
                    &spl_stake_pool::id(),
                    &self.stake_pool.pubkey(),
                    &self.withdraw_authority,
                    &self.validator_list.pubkey(),
                    &self.reserve_stake.pubkey(),
                    validator_vote_accounts,
                    0,
                    no_merge,
                ),
                spl_stake_pool::instruction::update_stake_pool_balance(
                    &spl_stake_pool::id(),
                    &self.stake_pool.pubkey(),
                    &self.withdraw_authority,
                    &self.validator_list.pubkey(),
                    &self.reserve_stake.pubkey(),
                    &self.pool_fee_account.pubkey(),
                    &self.pool_mint.pubkey(),
                ),
            ],
            Some(&payer.pubkey()),
            &[payer],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await.err()
    }

    // pub async fn remove_validator_from_pool(
    //     &self,
    //     banks_client: &mut BanksClient,
    //     payer: &Keypair,
    //     recent_blockhash: &Hash,
    //     new_authority: &Pubkey,
    //     validator_stake: &Pubkey,
    //     transient_stake: &Pubkey,
    // ) -> Option<TransportError> {
    //     let transaction = Transaction::new_signed_with_payer(
    //         &[spl_stake_pool::instruction::remove_validator_from_pool(
    //             &spl_stake_pool::id(),
    //             &self.stake_pool.pubkey(),
    //             &self.staker.pubkey(),
    //             &self.withdraw_authority,
    //             &new_authority,
    //             &self.validator_list.pubkey(),
    //             validator_stake,
    //             transient_stake,
    //         )
    //         .unwrap()],
    //         Some(&payer.pubkey()),
    //         &[payer, &self.staker],
    //         *recent_blockhash,
    //     );
    //     banks_client.process_transaction(transaction).await.err()
    // }

    //     pub async fn decrease_validator_stake(
    //         &self,
    //         banks_client: &mut BanksClient,
    //         payer: &Keypair,
    //         recent_blockhash: &Hash,
    //         validator_stake: &Pubkey,
    //         transient_stake: &Pubkey,
    //         lamports: u64,
    //     ) -> Option<TransportError> {
    //         let transaction = Transaction::new_signed_with_payer(
    //             &[spl_stake_pool::instruction::decrease_validator_stake(
    //                 &spl_stake_pool::id(),
    //                 &self.stake_pool.pubkey(),
    //                 &self.staker.pubkey(),
    //                 &self.withdraw_authority,
    //                 &self.validator_list.pubkey(),
    //                 validator_stake,
    //                 transient_stake,
    //                 lamports,
    //             )],
    //             Some(&payer.pubkey()),
    //             &[payer, &self.staker],
    //             *recent_blockhash,
    //         );
    //         banks_client.process_transaction(transaction).await.err()
    //     }

    //     pub async fn increase_validator_stake(
    //         &self,
    //         banks_client: &mut BanksClient,
    //         payer: &Keypair,
    //         recent_blockhash: &Hash,
    //         transient_stake: &Pubkey,
    //         validator: &Pubkey,
    //         lamports: u64,
    //     ) -> Option<TransportError> {
    //         let transaction = Transaction::new_signed_with_payer(
    //             &[spl_stake_pool::instruction::increase_validator_stake(
    //                 &spl_stake_pool::id(),
    //                 &self.stake_pool.pubkey(),
    //                 &self.staker.pubkey(),
    //                 &self.withdraw_authority,
    //                 &self.validator_list.pubkey(),
    //                 &self.reserve_stake.pubkey(),
    //                 transient_stake,
    //                 validator,
    //                 lamports,
    //             )],
    //             Some(&payer.pubkey()),
    //             &[payer, &self.staker],
    //             *recent_blockhash,
    //         );
    //         banks_client.process_transaction(transaction).await.err()
    //     }
}

#[derive(Debug)]
pub struct DepositStakeAccount {
    pub authority: Keypair,
    pub stake: Keypair,
    pub pool_account: Keypair,
    pub stake_lamports: u64,
    pub pool_tokens: u64,
    pub vote_account: Pubkey,
    pub validator_stake_account: Pubkey,
}

impl DepositStakeAccount {
    pub fn new_with_vote(
        vote_account: Pubkey,
        validator_stake_account: Pubkey,
        stake_lamports: u64,
    ) -> Self {
        let authority = Keypair::new();
        let stake = Keypair::new();
        let pool_account = Keypair::new();
        Self {
            authority,
            stake,
            pool_account,
            vote_account,
            validator_stake_account,
            stake_lamports,
            pool_tokens: 0,
        }
    }

    pub async fn create_and_delegate(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) {
        let lockup = stake_program::Lockup::default();
        let authorized = stake_program::Authorized {
            staker: self.authority.pubkey(),
            withdrawer: self.authority.pubkey(),
        };
        create_independent_stake_account(
            banks_client,
            payer,
            recent_blockhash,
            &self.stake,
            &authorized,
            &lockup,
            self.stake_lamports,
        )
        .await;
        delegate_stake_account(
            banks_client,
            payer,
            recent_blockhash,
            &self.stake.pubkey(),
            &self.authority,
            &self.vote_account,
        )
        .await;
    }

    pub async fn deposit(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        stake_pool_accounts: &StakePoolAccounts,
    ) {
        // make pool token account
        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &self.pool_account,
            &stake_pool_accounts.pool_mint.pubkey(),
            &self.authority.pubkey(),
        )
        .await
        .unwrap();

        stake_pool_accounts
            .deposit_stake(
                banks_client,
                payer,
                recent_blockhash,
                &self.stake.pubkey(),
                &self.pool_account.pubkey(),
                &self.validator_stake_account,
                &self.authority,
            )
            .await
            .unwrap();
    }
}

pub async fn simple_deposit(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake_pool_accounts: &StakePoolAccounts,
    validator_stake_account: &ValidatorStakeAccount,
    stake_lamports: u64,
) -> Option<DepositStakeAccount> {
    let authority = Keypair::new();
    // make stake account
    let stake = Keypair::new();
    let lockup = stake_program::Lockup::default();
    let authorized = stake_program::Authorized {
        staker: authority.pubkey(),
        withdrawer: authority.pubkey(),
    };
    create_independent_stake_account(
        banks_client,
        payer,
        recent_blockhash,
        &stake,
        &authorized,
        &lockup,
        stake_lamports,
    )
    .await;
    let vote_account = validator_stake_account.vote.pubkey();
    delegate_stake_account(
        banks_client,
        payer,
        recent_blockhash,
        &stake.pubkey(),
        &authority,
        &vote_account,
    )
    .await;
    // make pool token account
    let pool_account = Keypair::new();
    create_token_account(
        banks_client,
        payer,
        recent_blockhash,
        &pool_account,
        &stake_pool_accounts.pool_mint.pubkey(),
        &authority.pubkey(),
    )
    .await
    .unwrap();

    let validator_stake_account = validator_stake_account.stake_account;
    stake_pool_accounts
        .deposit_stake(
            banks_client,
            payer,
            recent_blockhash,
            &stake.pubkey(),
            &pool_account.pubkey(),
            &validator_stake_account,
            &authority,
        )
        .await
        .ok()?;

    let pool_tokens = get_token_balance(banks_client, &pool_account.pubkey()).await;

    Some(DepositStakeAccount {
        authority,
        stake,
        pool_account,
        stake_lamports,
        pool_tokens,
        vote_account,
        validator_stake_account,
    })
}

pub async fn get_validator_list_sum(
    banks_client: &mut BanksClient,
    reserve_stake: &Pubkey,
    validator_list: &Pubkey,
) -> u64 {
    let validator_list = banks_client
        .get_account(*validator_list)
        .await
        .unwrap()
        .unwrap();
    let validator_list =
        try_from_slice_unchecked::<state::ValidatorList>(validator_list.data.as_slice()).unwrap();
    let reserve_stake = banks_client
        .get_account(*reserve_stake)
        .await
        .unwrap()
        .unwrap();

    let validator_sum: u64 = validator_list
        .validators
        .iter()
        .map(|info| info.stake_lamports)
        .sum();
    let rent = banks_client.get_rent().await.unwrap();
    let rent = rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>());
    validator_sum + reserve_stake.lamports - rent - 1
}
