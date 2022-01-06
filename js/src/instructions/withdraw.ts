import * as BufferLayout from '@solana/buffer-layout';
import { TOKEN_PROGRAM_ID } from '@solana/spl-token';
import {
  LAMPORTS_PER_SOL,
  PublicKey,
  StakeProgram,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
  TransactionInstruction,
} from '@solana/web3.js';
import BN from 'bn.js';
import { StLamports, Lamports } from '../types';
import type { ProgramAddresses, Snapshot } from '../types';
import {
  findAuthorityProgramAddress,
  getHeaviestValidatorStakeAccount,
} from '../utils';

/**
 * Generates the instructions to unstake from the Solido Program
 * @param snapshot Snapshot of the Solido stats
 * @param programAddresses Program addresses for the deployed Solido Program
 * @param stSolAccountOwnerAddress Address of the owner of the sender's stSOL Account
 * @param senderStSolAccountAddress Address of the stSOL Account to be unstaked
 * @param recipientStakeAccountAddress Address of the NEW Account
 * @param amount Amount of stSOL to unstake
 * @returns Instructions to unstake stSOL
 */
export const getWithdrawInstruction = async (
  snapshot: Snapshot,
  programAddresses: ProgramAddresses,
  stSolAccountOwnerAddress: PublicKey,
  senderStSolAccountAddress: PublicKey,
  recipientStakeAccountAddress: PublicKey,
  amount: StLamports
) => {
  const stakeAuthorityAddress = await findAuthorityProgramAddress(
    programAddresses,
    'stake_authority'
  );
  const heaviestValidatorStakeAccount =
    getHeaviestValidatorStakeAccount(snapshot);

  const { exchange_rate } = snapshot.solido;

  const withdrawAmountInSol = new Lamports(
    amount.stLamports.mul(
      exchange_rate.sol_balance.div(exchange_rate.st_sol_supply)
    )
  );

  if (
    withdrawAmountInSol.lamports.lte(
      snapshot.stakeAccountRentExemptionBalance.lamports
    )
  ) {
    throw new Error('Amount must be greater than the rent exemption balance');
  }

  const maxWithdrawAmount = new Lamports(
    heaviestValidatorStakeAccount.balance.lamports
      .div(new BN(10))
      .add(new BN(10 * LAMPORTS_PER_SOL))
  );

  if (withdrawAmountInSol.lamports.gte(maxWithdrawAmount.lamports)) {
    throw new Error('Amount must be less than the maximum withdrawal amount');
  }

  const keys = [
    {
      pubkey: programAddresses.solidoProgramId,
      isSigner: false,
      isWritable: true,
    },
    { pubkey: stSolAccountOwnerAddress, isSigner: true, isWritable: false },
    {
      pubkey: senderStSolAccountAddress,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: programAddresses.stSolMintAddress,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: heaviestValidatorStakeAccount.validatorVoteAddress,
      isSigner: false,
      isWritable: false,
    },
    {
      pubkey: heaviestValidatorStakeAccount.address,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: recipientStakeAccountAddress,
      isSigner: true,
      isWritable: true,
    },
    { pubkey: stakeAuthorityAddress, isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: StakeProgram.programId, isSigner: false, isWritable: false },
  ];

  // Reference: Withdraw instruction at https://github.com/ChorusOne/solido/blob/main/program/src/instruction.rs#L45-L52
  const dataLayout = BufferLayout.struct([
    BufferLayout.u8('instruction'),
    BufferLayout.nu64('amount'),
  ]);

  const data = Buffer.alloc(dataLayout.span);
  dataLayout.encode(
    {
      instruction: 2,
      amount: amount.stLamports,
    },
    data
  );

  return new TransactionInstruction({
    keys,
    data,
    programId: programAddresses.solidoProgramId,
  });
};
