import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  Token,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token';
import { PublicKey, TransactionInstruction } from '@solana/web3.js';
import { MEMO_PROGRAM_ID } from '../constants';

/**
 * Generates the instruction to create the Associated Token Account for the given mint address
 * @param mintAddress Mint address of the token
 * @param ownerAddress Address of the owner of the token account
 * @param associatedTokenAccountAddress Address of the (Uninitialized)Associated Token Account
 * @returns Instruction to create the Associated Token Account
 */
export const getATAInitializeInstruction = async (
  mintAddress: PublicKey,
  ownerAddress: PublicKey,
  associatedTokenAccountAddress?: PublicKey
): Promise<TransactionInstruction> => {
  if (!associatedTokenAccountAddress) {
    associatedTokenAccountAddress = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      mintAddress,
      ownerAddress
    );
  }

  return Token.createAssociatedTokenAccountInstruction(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    mintAddress,
    associatedTokenAccountAddress,
    ownerAddress,
    ownerAddress
  );
};

/**
 * Generates the instruction to add a memo instruction to the given transaction
 * @param memo Any JSON object to be stored in the memo instruction of the transaction
 * @param feePayerAddress Address of the fee payer for the transaction
 * @returns Instruction to store the memo in the transaction
 */
export const getMemoInstruction = (
  memo: JSON,
  feePayerAddress: PublicKey
): TransactionInstruction => {
  const instruction = new TransactionInstruction({
    programId: MEMO_PROGRAM_ID,
    data: Buffer.from(JSON.stringify(memo)),
    keys: [{ isSigner: true, isWritable: false, pubkey: feePayerAddress }],
  });

  return instruction;
};
