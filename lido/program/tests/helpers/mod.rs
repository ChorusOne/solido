use lido::*;
use solana_program::{borsh::get_packed_len, hash::Hash, pubkey::Pubkey, system_instruction};
use solana_program_test::*;
use solana_sdk::{signature::Keypair, transport::TransportError};
use solana_sdk::{signature::Signer, transaction::Transaction};
use stakepool_account::StakePoolAccounts;

use self::stakepool_account::create_mint;

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

    pub stake_pool_accounts: StakePoolAccounts,
}

impl LidoAccounts {
    pub fn new() -> Self {
        let owner = Keypair::new();
        let lido = Keypair::new();
        let mint_program = Keypair::new();

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

        let mut stake_pool_accounts = StakePoolAccounts::new();
        stake_pool_accounts.deposit_authority = reserve_authority;
        Self {
            owner,
            lido,
            mint_program,
            reserve_authority,
            deposit_authority,
            stake_pool_accounts,
            stake_pool_token_reserve_authority,
        }
    }

    pub async fn initialize_lido(
        &mut self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Result<(), TransportError> {
        self.stake_pool_accounts = StakePoolAccounts::new();
        self.stake_pool_accounts.deposit_authority = self.deposit_authority;
        self.stake_pool_accounts
            .initialize_stake_pool(banks_client, payer, recent_blockhash, 1)
            .await?;

        create_mint(
            banks_client,
            payer,
            recent_blockhash,
            &self.mint_program,
            &self.reserve_authority,
        )
        .await?;

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
                )
                .unwrap(),
            ],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[payer, &self.lido, &self.stake_pool_accounts.stake_pool],
            *recent_blockhash,
        );
        banks_client.process_transaction(transaction).await?;

        Ok(())
    }
}
