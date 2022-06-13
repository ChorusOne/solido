import * as BufferLayout from '@solana/buffer-layout';
import { TOKEN_PROGRAM_ID } from '@solana/spl-token';
import { PublicKey, TransactionInstruction } from '@solana/web3.js';
import { ProgramAddresses, StLamports } from '../../types';

/**
 * Returns the instruction to deposit stSOL into the Anker program and mint bSOL tokens
 * @param senderStSolAccountOwnerAddress Address of the owner of the sender's stSOL SPL token account. Must be a signer of the transaction.
 * @param senderStSolAccountAddress Address of the stSOL SPL token account, whose stSOL balance will be decreased
 * @param recipientBSolAccountAddress Address of the recipient bSOL SPL token account
 * @param programAddresses Solido and Anker program addresses
 * @param amount Amount of stSOL to be deposited
 */
export const getDepositInstruction = async (
  senderStSolAccountOwnerAddress: PublicKey,
  senderStSolAccountAddress: PublicKey,
  recipientBSolAccountAddress: PublicKey,
  programAddresses: ProgramAddresses,
  amount: StLamports
) => {
  const [stSolReserveAccountAddress] = await PublicKey.findProgramAddress(
    [
      programAddresses.ankerInstanceId.toBuffer(),
      Buffer.from('st_sol_reserve_account'),
    ],
    programAddresses.ankerProgramId
  );

  const [bSolMintAuthorityAddress] = await PublicKey.findProgramAddress(
    [
      programAddresses.ankerInstanceId.toBuffer(),
      Buffer.from('mint_authority'),
    ],
    programAddresses.ankerProgramId
  );

  const keys = [
    {
      pubkey: programAddresses.ankerInstanceId,
      isSigner: false,
      isWritable: false,
    },
    {
      pubkey: programAddresses.solidoInstanceId,
      isSigner: false,
      isWritable: false,
    },
    { pubkey: senderStSolAccountAddress, isSigner: false, isWritable: true },
    {
      pubkey: senderStSolAccountOwnerAddress,
      isSigner: true,
      isWritable: false,
    },
    {
      pubkey: stSolReserveAccountAddress,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: recipientBSolAccountAddress,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: programAddresses.bSolMintAddress,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: bSolMintAuthorityAddress,
      isSigner: false,
      isWritable: false,
    },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
  ];

  const dataLayout = BufferLayout.struct([
    BufferLayout.u8('instruction'),
    BufferLayout.nu64('amount'),
  ]);

  const data = Buffer.alloc(dataLayout.span);
  dataLayout.encode({ instruction: 1, amount: amount.stLamports }, data);

  return new TransactionInstruction({
    keys,
    programId: programAddresses.ankerProgramId,
    data,
  });
};
