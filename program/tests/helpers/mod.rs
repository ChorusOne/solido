use lido::*;
use solana_program::{borsh::get_packed_len, hash::Hash, pubkey::Pubkey, system_instruction};
use solana_program_test::*;
use solana_sdk::{signature::Keypair, transport::TransportError};
use solana_sdk::{signature::Signer, transaction::Transaction};
use stakepool_account::StakePoolAccounts;

use self::stakepool_account::{create_mint, create_token_account, transfer, ValidatorStakeAccount};

pub mod stakepool_account;

pub fn program_test() -> ProgramTest {
    let mut program = ProgramTest::new("lido", id(), processor!(processor::Processor::process));
    program.add_program(
        "spl_stake_pool",
        spl_stake_pool::id(),
        processor!(spl_stake_pool::processor::Processor::process),
    );
    program
}

pub struct LidoAccounts {
    pub owner: Keypair,
    pub lido: Keypair,
    pub mint_program: Keypair,
    pub reserve_authority: Pubkey,
    pub deposit_authority: Pubkey,
    pub stake_pool_token_reserve_authority: Pubkey,
    pub fee_manager_authority: Pubkey,
    pub pool_token_to: Keypair,
    pub stake_pool_accounts: StakePoolAccounts,
}

impl LidoAccounts {
    pub fn new() -> Self {
        let owner = Keypair::new();
        let lido = Keypair::new();
        let mint_program = Keypair::new();
        let pool_token_to = Keypair::new();

        let (reserve_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..32], RESERVE_AUTHORITY_ID],
            &id(),
        );

        let (deposit_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..32], DEPOSIT_AUTHORITY_ID],
            &id(),
        );
        let (stake_pool_token_reserve_authority, _) = Pubkey::find_program_address(
            &[
                &lido.pubkey().to_bytes()[..32],
                STAKE_POOL_TOKEN_RESERVE_AUTHORITY_ID,
            ],
            &id(),
        );
        let (fee_manager_authority, _) = Pubkey::find_program_address(
            &[&lido.pubkey().to_bytes()[..32], FEE_MANAGER_AUTHORITY],
            &id(),
        );

        let mut stake_pool_accounts = StakePoolAccounts::new();
        stake_pool_accounts.deposit_authority = reserve_authority;
        Self {
            owner,
            lido,
            mint_program,
            reserve_authority,
            deposit_authority,
            stake_pool_token_reserve_authority,
            fee_manager_authority,
            pool_token_to,
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
            &self.stake_pool_token_reserve_authority,
        )
        .await
        .unwrap();

        let rent = banks_client.get_rent().await.unwrap();
        let rent_lido = rent.minimum_balance(get_packed_len::<state::Lido>());
        let mut transaction = Transaction::new_with_payer(
            &[
                system_instruction::create_account(
                    &payer.pubkey(),
                    &self.lido.pubkey(),
                    rent_lido,
                    get_packed_len::<state::Lido>() as u64,
                    &id(),
                ),
                instruction::initialize(
                    &id(),
                    &self.lido.pubkey(),
                    &self.stake_pool_accounts.stake_pool.pubkey(),
                    &self.owner.pubkey(),
                    &self.mint_program.pubkey(),
                    &self.pool_token_to.pubkey(),
                    &self.stake_pool_accounts.pool_fee_account.pubkey(),
                )
                .unwrap(),
            ],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer, &self.lido], *recent_blockhash);
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
                &self.lido.pubkey(),
                &self.stake_pool_accounts.stake_pool.pubkey(),
                &self.pool_token_to.pubkey(),
                &self.owner.pubkey(),
                &user.pubkey(),
                &recipient.pubkey(),
                &self.mint_program.pubkey(),
                &self.reserve_authority,
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
                &self.lido.pubkey(),
                &validator.vote.pubkey(),
                &self.reserve_authority,
                &stake_account,
                &self.deposit_authority,
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
                &self.lido.pubkey(),
                &validator.vote.pubkey(),
                &stake_account,
                &self.deposit_authority,
                &self.pool_token_to.pubkey(),
                &spl_stake_pool::id(),
                &self.stake_pool_accounts.stake_pool.pubkey(),
                &self.stake_pool_accounts.validator_list.pubkey(),
                &self.stake_pool_accounts.withdraw_authority,
                &validator.stake_account,
                &self.stake_pool_accounts.pool_mint.pubkey(),
            )
            .unwrap()],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer], *recent_blockhash);
        banks_client.process_transaction(transaction).await.unwrap();
    }
}
