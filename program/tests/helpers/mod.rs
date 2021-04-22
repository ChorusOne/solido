use lido::*;
use solana_program::{
    borsh::get_packed_len, hash::Hash, program_pack::Pack, pubkey::Pubkey, system_instruction,
};
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
    pub stake_pool: Keypair,
    pub mint_program: Keypair,
    pub authority: Pubkey,
    pub reserve: Keypair,
}

impl LidoAccounts {
    pub fn new() -> Self {
        let stake_pool = Keypair::new();
        let owner = Keypair::new();
        let lido = Keypair::new();
        let mint_program = Keypair::new();
        let reserve = Keypair::new();

        let (authority, _) =
            Pubkey::find_program_address(&[&lido.pubkey().to_bytes()[..32], AUTHORITY_ID], &id());
        Self {
            stake_pool,
            owner,
            lido,
            mint_program,
            authority,
            reserve,
        }
    }

    pub async fn initialize_lido(
        &mut self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Result<(), TransportError> {
        let stake_pool = StakePoolAccounts::new();
        stake_pool
            .initialize_stake_pool(banks_client, payer, recent_blockhash)
            .await?;
        self.stake_pool = stake_pool.stake_pool;

        create_mint(
            banks_client,
            payer,
            recent_blockhash,
            &self.mint_program,
            &self.authority,
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
                    &self.stake_pool.pubkey(),
                    &self.owner.pubkey(),
                    &self.mint_program.pubkey(),
                )
                .unwrap(),
            ],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[payer, &self.lido, &self.stake_pool], *recent_blockhash);
        banks_client.process_transaction(transaction).await?;

        Ok(())
    }
}
