use lido::*;
use solana_program::{hash::Hash, pubkey::Pubkey};
use solana_program_test::*;
use solana_sdk::signature::Signer;
use solana_sdk::{signature::Keypair, transport::TransportError};
use stakepool_account::StakePoolAccounts;

pub mod stakepool_account;

pub fn program_test() -> ProgramTest {
    let mut program = ProgramTest::new("lido", id(), processor!(processor::Processor::process));

    program.add_program(
        "spl_stake_pool",
        spl_stake_pool::id(),
        processor!(spl_stake_pool::processor::Processor::process),
    );

    let program = ProgramTest::new(
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
}

impl LidoAccounts {
    pub fn new() -> Self {
        let stake_pool = Keypair::new();
        let owner = Keypair::new();
        let lido = Keypair::new();
        let mint_program = Keypair::new();

        let (authority, _) =
            Pubkey::find_program_address(&[&lido.to_bytes()[..32], AUTHORITY_ID], &id());
        Self {
            stake_pool,
            owner,
            lido,
            mint_program,
            authority,
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
        println!("cocococo");
        self.stake_pool = stake_pool.stake_pool;

        Ok(())
    }
}
