use lido::{
    id,
    instruction::{self, initialize},
    processor,
    state::{FeeDistribution, Lido, ValidatorCreditAccounts},
    DEPOSIT_AUTHORITY, FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_AUTHORITY,
};
use solana_program::{borsh::get_packed_len, hash::Hash, pubkey::Pubkey, system_instruction};
use solana_program_test::*;
use solana_sdk::{signature::Keypair, transport::TransportError};
use solana_sdk::{signature::Signer, transaction::Transaction};
use spl_stake_pool::borsh::get_instance_packed_len;
use stakepool_account::StakePoolAccounts;

use self::stakepool_account::{create_mint, create_token_account, transfer, ValidatorStakeAccount};

pub mod stakepool_account;
const MAX_VALIDATORS: u32 = 10_000;

pub fn program_test() -> ProgramTest {
    let mut program = ProgramTest::new("lido", id(), processor!(processor::process));
    program.add_program(
        "spl_stake_pool",
        spl_stake_pool::id(),
        processor!(spl_stake_pool::processor::Processor::process),
    );
    program
}

pub struct LidoAccounts {
    pub manager: Keypair,
    pub lido: Keypair,
    pub mint_program: Keypair,
    pub fee_distribution: Keypair,
    pub validator_credit_accounts: Keypair,
    pub pool_token_to: Keypair,

    // Fees
    pub insurance_account: Keypair,
    pub treasury_account: Keypair,
    pub manager_account: Keypair,
    pub fee_structure: FeeDistribution,

    pub reserve_authority: Pubkey,
    pub deposit_authority: Pubkey,
    pub stake_pool_authority: Pubkey,
    pub fee_manager_authority: Pubkey,
    pub stake_pool_accounts: StakePoolAccounts,
}

impl LidoAccounts {
    pub fn new() -> Self {
        let manager = Keypair::new();
        let lido = Keypair::new();
        let mint_program = Keypair::new();
        let fee_distribution = Keypair::new();
        let validator_credit_accounts = Keypair::new();
        let pool_token_to = Keypair::new();

        // Fees
        let insurance_account = Keypair::new();
        let treasury_account = Keypair::new();
        let manager_account = Keypair::new();

        let fee_structure = FeeDistribution {
            insurance_fee_numerator: 2,
            treasury_fee_numerator: 2,
            validators_fee_numerator: 2,
            manager_fee_numerator: 4,
            denominator: 10,

            insurance_account: insurance_account.pubkey(),
            treasury_account: treasury_account.pubkey(),
            manager_account: manager_account.pubkey(),
        };

        let (reserve_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..32], RESERVE_AUTHORITY],
            &id(),
        );

        let (deposit_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..32], DEPOSIT_AUTHORITY],
            &id(),
        );
        let (stake_pool_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..32], STAKE_POOL_AUTHORITY],
            &id(),
        );

        let (fee_manager_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..32], FEE_MANAGER_AUTHORITY],
            &id(),
        );

        let mut stake_pool_accounts = StakePoolAccounts::new(stake_pool_authority);
        stake_pool_accounts.deposit_authority = reserve_authority;
        Self {
            manager,
            lido,
            mint_program,
            fee_distribution,
            validator_credit_accounts,
            pool_token_to,
            insurance_account,
            treasury_account,
            manager_account,
            fee_structure,
            reserve_authority,
            deposit_authority,
            stake_pool_authority,
            fee_manager_authority,
            stake_pool_accounts,
        }
    }

    pub async fn initialize_lido(
        &mut self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Result<(), TransportError> {
        self.stake_pool_accounts.deposit_authority = self.deposit_authority;
        self.stake_pool_accounts
            .initialize_stake_pool(
                banks_client,
                payer,
                &self.manager,
                recent_blockhash,
                1,
                &self.fee_manager_authority,
            )
            .await?;

        create_mint(
            banks_client,
            payer,
            recent_blockhash,
            &self.mint_program,
            &self.reserve_authority,
        )
        .await?;

        create_token_account(
            banks_client,
            &payer,
            &recent_blockhash,
            &self.pool_token_to,
            &self.stake_pool_accounts.pool_mint.pubkey(),
            &self.stake_pool_authority,
        )
        .await
        .unwrap();

        let validator_accounts_len =
            get_instance_packed_len(&ValidatorCreditAccounts::new(MAX_VALIDATORS)).unwrap();
        let rent = banks_client.get_rent().await.unwrap();
        let rent_lido = rent.minimum_balance(get_packed_len::<Lido>());
        let rent_fee_distribution = rent.minimum_balance(get_packed_len::<FeeDistribution>());
        let rent_validator_credit_accounts = rent.minimum_balance(validator_accounts_len);
        let mut transaction = Transaction::new_with_payer(
            &[
                system_instruction::create_account(
                    &payer.pubkey(),
                    &self.lido.pubkey(),
                    rent_lido,
                    get_packed_len::<Lido>() as u64,
                    &id(),
                ),
                system_instruction::create_account(
                    &payer.pubkey(),
                    &self.fee_distribution.pubkey(),
                    rent_fee_distribution,
                    get_packed_len::<FeeDistribution>() as u64,
                    &id(),
                ),
                system_instruction::create_account(
                    &payer.pubkey(),
                    &self.validator_credit_accounts.pubkey(),
                    rent_validator_credit_accounts,
                    validator_accounts_len as u64,
                    &id(),
                ),
                initialize(
                    &id(),
                    self.fee_structure.clone(),
                    MAX_VALIDATORS,
                    &instruction::InitializeAccountsMeta {
                        lido: self.lido.pubkey(),
                        stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                        manager: self.manager.pubkey(),
                        fee_distribution: self.fee_distribution.pubkey(),
                        validator_credit_accounts: self.validator_credit_accounts.pubkey(),
                        mint_program: self.mint_program.pubkey(),
                        pool_token_to: self.pool_token_to.pubkey(),
                        fee_token: self.stake_pool_accounts.pool_fee_account.pubkey(),
                    },
                )
                .unwrap(),
            ],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[
                payer,
                &self.lido,
                &self.fee_distribution,
                &self.validator_credit_accounts,
            ],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await?;

        Ok(())
    }

    pub async fn deposit(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        deposit_amount: u64,
    ) -> Keypair {
        let user = Keypair::new();
        let recipient = Keypair::new();

        create_token_account(
            banks_client,
            payer,
            recent_blockhash,
            &recipient,
            &self.mint_program.pubkey(),
            &user.pubkey(),
        )
        .await
        .unwrap();

        transfer(
            banks_client,
            payer,
            recent_blockhash,
            &user.pubkey(),
            deposit_amount,
        )
        .await;

        let mut transaction = Transaction::new_with_payer(
            &[instruction::deposit(
                &id(),
                &instruction::DepositAccountsMeta {
                    lido: self.lido.pubkey(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    pool_token_to: self.pool_token_to.pubkey(),
                    manager: self.manager.pubkey(),
                    user: user.pubkey(),
                    recipient: recipient.pubkey(),
                    mint_program: self.mint_program.pubkey(),
                    reserve_authority: self.reserve_authority,
                },
                deposit_amount,
            )
            .unwrap()],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[&payer, &user], *recent_blockhash);
        banks_client.process_transaction(transaction).await.unwrap();
        recipient
    }

    pub async fn delegate_deposit(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator: &ValidatorStakeAccount,
        delegate_amount: u64,
    ) -> Pubkey {
        let (stake_account, _) =
            Pubkey::find_program_address(&[&validator.vote.pubkey().to_bytes()[..32]], &id());

        let mut transaction = Transaction::new_with_payer(
            &[instruction::delegate_deposit(
                &id(),
                &instruction::DelegateDepositAccountsMeta {
                    lido: self.lido.pubkey(),
                    validator: validator.vote.pubkey(),
                    reserve: self.reserve_authority,
                    stake: stake_account,
                    deposit_authority: self.deposit_authority,
                },
                delegate_amount,
            )
            .unwrap()],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer], *recent_blockhash);
        banks_client.process_transaction(transaction).await.unwrap();
        stake_account
    }

    pub async fn delegate_stakepool_deposit(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
        validator: &ValidatorStakeAccount,
        stake_account: &Pubkey,
    ) {
        let mut transaction = Transaction::new_with_payer(
            &[instruction::stake_pool_delegate(
                &id(),
                &instruction::StakePoolDelegateAccountsMeta {
                    lido: self.lido.pubkey(),
                    validator: validator.vote.pubkey(),
                    stake: *stake_account,
                    deposit_authority: self.deposit_authority,
                    pool_token_to: self.pool_token_to.pubkey(),
                    stake_pool_program: spl_stake_pool::id(),
                    stake_pool: self.stake_pool_accounts.stake_pool.pubkey(),
                    stake_pool_validator_list: self.stake_pool_accounts.validator_list.pubkey(),
                    stake_pool_withdraw_authority: self.stake_pool_accounts.withdraw_authority,
                    stake_pool_validator_stake_account: validator.stake_account,
                    stake_pool_mint: self.stake_pool_accounts.pool_mint.pubkey(),
                },
            )
            .unwrap()],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer], *recent_blockhash);
        banks_client.process_transaction(transaction).await.unwrap();
    }
}
