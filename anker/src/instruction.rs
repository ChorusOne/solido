// TODO(#449): Remove this once Anker functions are all complete.
#![allow(dead_code)]

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};

use solana_program::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    system_program, sysvar,
};

use lido::{accounts_struct, accounts_struct_meta, error::LidoError, token::StLamports};

#[repr(C)]
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum AnkerInstruction {
    Initialize,

    /// Deposit a given amount of StSOL, gets bSOL in return.
    ///
    /// This can be called by anybody.
    Deposit {
        #[allow(dead_code)] // but it's not
        amount: StLamports,
    },

    /// Withdraw a given amount of stSOL.
    ///
    /// Caller provides some `amount` of StLamports that are to be burned in
    /// order to withdraw bSOL.
    Withdraw {
        #[allow(dead_code)] // but it's not
        amount: StLamports,
    },

    /// Claim rewards on Terra.
    ClaimRewards,
}

impl AnkerInstruction {
    pub fn to_vec(&self) -> Vec<u8> {
        // `BorshSerialize::try_to_vec` returns a Result, because it uses
        // `Borsh::serialize`, which takes an arbitrary writer, and which can
        // therefore return an IoError. But when serializing to a vec, there
        // is no IO, so for this particular writer, it should never fail.
        self.try_to_vec()
            .expect("Serializing an Instruction to Vec<u8> does not fail.")
    }
}

accounts_struct! {
    InitializeAccountsMeta, InitializeAccountsInfo {
        pub fund_rent_from {
            is_signer: true,
            is_writable: true, // It pays for the rent of the new accounts.
        },
        pub anker {
            is_signer: false,
            is_writable: true, // Writable because we need to initialize it.
        },
        pub solido {
            is_signer: false,
            is_writable: false,
        },
        pub solido_program {
            is_signer: false,
            is_writable: false,
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: false,
        },
        pub b_sol_mint {
            is_signer: false,
            is_writable: false,
        },
        pub reserve_account {
            is_signer: false,
            is_writable: true, // Writable because we need to initialize it.
        },
        pub reserve_authority {
            is_signer: false,
            is_writable: false,
        },
        // Instance of the token swap data used for trading StSOL for UST.
        pub token_swap_instance {
            is_signer: false,
            is_writable: false,
        },
        pub rewards_destination {
            is_signer: false,
            is_writable: false,
        },
        const sysvar_rent = sysvar::rent::id(),
        const system_program = system_program::id(),
        const spl_token = spl_token::id(),
    }
}

pub fn initialize(program_id: &Pubkey, accounts: &InitializeAccountsMeta) -> Instruction {
    let data = AnkerInstruction::Initialize;
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: data.to_vec(),
    }
}

accounts_struct! {
    DepositAccountsMeta, DepositAccountsInfo {
        pub anker {
            is_signer: false,
            is_writable: false,
        },
        // For reading the stSOL/SOL exchange rate.
        pub solido {
            is_signer: false,
            is_writable: false,
        },
        pub from_account {
            is_signer: false,
            is_writable: true, // We will reduce its balance.
        },
        // Owner of `from_account` SPL token account.
        // Must sign the transaction in order to move tokens.
        pub user_authority {
            is_signer: true,
            is_writable: false,
        },
        pub to_reserve_account {
            is_signer: false,
            is_writable: true, // Needs to be writable to update the account's state.
        },
        // User account that will receive the bSOL tokens, needs to be writable
        // to update the account's state.
        pub b_sol_user_account {
            is_signer: false,
            is_writable: true,
        },
        pub b_sol_mint {
            is_signer: false,
            is_writable: true, // Minting changes the supply, which is stored in the mint.
        },
        pub b_sol_mint_authority {
            is_signer: false,
            is_writable: false,
        },
        const spl_token = spl_token::id(),
    }
}

pub fn deposit(
    program_id: &Pubkey,
    accounts: &DepositAccountsMeta,
    amount: StLamports,
) -> Instruction {
    let data = AnkerInstruction::Deposit { amount };
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: data.to_vec(),
    }
}

accounts_struct! {
    ClaimRewardsAccountsMeta, ClaimRewardsAccountsInfo {
        pub anker {
            is_signer: false,
            is_writable: false,
        },
        pub solido {
            is_signer: false,
            is_writable: false,
        },
        // Needs to be writable so we can sell stSOL.
        pub reserve_account {
            is_signer: false,
            is_writable: true, // Needed to swap tokens.
        },
        pub b_sol_mint {
            is_signer: false,
            is_writable: false,
        },

        // Accounts for token swap
        pub token_swap_instance {
            is_signer: false,
            is_writable: false,
        },
        pub st_sol_token {
            is_signer: false,
            is_writable: true, // Needed to swap tokens.
        },
        pub ust_token {
            is_signer: false,
            is_writable: true, // Needed to swap tokens.
        },
        pub pool_mint {
            is_signer: false,
            is_writable: true, // Needed to swap tokens.
        },
        pub st_sol_mint {
            is_signer: false,
            is_writable: false,
        },
        pub ust_mint {
            is_signer: false,
            is_writable: false,
        },
        pub pool_fee_account {
            is_signer: false,
            is_writable: true, // Needed to swap tokens.
        },
        pub token_pool_authority {
            is_signer: false,
            is_writable: false,
        },
        pub reserve_authority {
            is_signer: false,
            is_writable: false,
        },
        pub rewards_destination {
            is_signer: false,
            is_writable: true, // Needed to swap tokens.
        },
        const spl_token = spl_token::id(),
        const orca_token_swap_v2 = crate::orca_token_swap_v2::id(),
    }
}

pub fn claim_rewards(program_id: &Pubkey, accounts: &ClaimRewardsAccountsMeta) -> Instruction {
    let data = AnkerInstruction::ClaimRewards;
    Instruction {
        program_id: *program_id,
        accounts: accounts.to_vec(),
        data: data.to_vec(),
    }
}
