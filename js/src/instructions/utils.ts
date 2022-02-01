import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  Token,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token';
import { PublicKey, TransactionInstruction } from '@solana/web3.js';

/**
 * Generates the instruction to create the Associated Token Account for the given mint address
 * @param mintAddress Mint address of the token
 * @param ownerAddress Address of the owner of the token account
 * @returns Instruction to create the Associated Token Account
 */
export const getATAInitializeInstruction = async (
  mintAddress: PublicKey,
  ownerAddress: PublicKey
): Promise<TransactionInstruction> => {
  const associatedTokenAccount = await Token.getAssociatedTokenAddress(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    mintAddress,
    ownerAddress
  );

  return Token.createAssociatedTokenAccountInstruction(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    mintAddress,
    associatedTokenAccount,
    ownerAddress,
    ownerAddress
  );
};
