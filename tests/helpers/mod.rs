use solana_program::{hash::Hash, program_pack::Pack, pubkey::Pubkey, system_instruction};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    signature::{Keypair, Signer},
    transaction::Transaction,
    transport::TransportError,
};
use solido::{entrypoint::process_instruction, instructions::StakePoolInstruction, model, state};
solana_program::declare_id!("5QuBzCtUC6pHgFEQJ5d2qX7ktyyHba9HVXLQVUEiAf7d");

pub fn program_test() -> ProgramTest {
    ProgramTest::new("solido", id(), processor!(process_instruction))
}

pub struct StakePoolAccounts {
    pub stake_pool: Keypair,
    pub validator_stake_list: Keypair,
    pub pool_mint: Keypair,
    pub pool_fee_account: Keypair,
    pub owner: Keypair,
    pub withdraw_authority: Pubkey,
    pub deposit_authority: Pubkey,
    pub fee: state::Fee,
}

impl StakePoolAccounts {
    pub fn new() -> Self {
        let stake_pool = Keypair::new();
        let validator_stake_list = Keypair::new();
        let stake_pool_address = &stake_pool.pubkey();
        let (withdraw_authority, _) = Pubkey::find_program_address(
            &[&stake_pool_address.to_bytes()[..32], b"withdraw"],
            &id(),
        );
        let (deposit_authority, _) = Pubkey::find_program_address(
            &[&stake_pool_address.to_bytes()[..32], b"deposit"],
            &id(),
        );
        let pool_mint = Keypair::new();
        let pool_fee_account = Keypair::new();
        let owner = Keypair::new();

        Self {
            stake_pool,
            validator_stake_list,
            pool_mint,
            pool_fee_account,
            owner,
            withdraw_authority,
            deposit_authority,
            fee: state::Fee {
                numerator: 1,
                denominator: 100,
            },
        }
    }
    pub async fn initialize_stake_pool(
        &self,
        mut banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
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
            &self.owner.pubkey(),
        )
        .await?;
        create_stake_pool(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &self.stake_pool,
            &self.validator_stake_list,
            &self.pool_mint.pubkey(),
            &self.pool_fee_account.pubkey(),
            &self.owner,
            &self.fee,
        )
        .await?;
        Ok(())
    }
}

pub async fn create_token_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    account: &Keypair,
    pool_mint: &Pubkey,
    owner: &Pubkey,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let account_rent = rent.minimum_balance(spl_token::state::Account::LEN);

    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &account.pubkey(),
                account_rent,
                spl_token::state::Account::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_account(
                &spl_token::id(),
                &account.pubkey(),
                pool_mint,
                owner,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, account], *recent_blockhash);
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn create_mint(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    pool_mint: &Keypair,
    owner: &Pubkey,
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
                &owner,
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

pub async fn create_stake_pool(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake_pool: &Keypair,
    validator_stake_list: &Keypair,
    pool_mint: &Pubkey,
    pool_token_account: &Pubkey,
    owner: &Keypair,
    fee: &state::Fee,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let rent_stake_pool = rent.minimum_balance(state::StakePool::LEN);
    let rent_validator_stake_list = rent.minimum_balance(model::ValidatorStakeList::LEN);
    let init_args = model::InitArgs { fee: *fee };

    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &stake_pool.pubkey(),
                rent_stake_pool,
                state::StakePool::LEN as u64,
                &id(),
            ),
            system_instruction::create_account(
                &payer.pubkey(),
                &validator_stake_list.pubkey(),
                rent_validator_stake_list,
                model::ValidatorStakeList::LEN as u64,
                &id(),
            ),
            StakePoolInstruction::initialize(
                &id(),
                &stake_pool.pubkey(),
                &owner.pubkey(),
                &validator_stake_list.pubkey(),
                pool_mint,
                pool_token_account,
                &spl_token::id(),
                init_args,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(
        &[payer, stake_pool, validator_stake_list, owner],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await?;
    Ok(())
}

pub async fn get_account(banks_client: &mut BanksClient, pubkey: &Pubkey) -> Account {
    banks_client
        .get_account(*pubkey)
        .await
        .expect("account not found")
        .expect("account empty")
}
