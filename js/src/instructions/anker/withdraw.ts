import * as BufferLayout from '@solana/buffer-layout';
import { TOKEN_PROGRAM_ID } from '@solana/spl-token';
import { PublicKey, TransactionInstruction } from '@solana/web3.js';
import { BLamports, ProgramAddresses } from '../../types';

/**
 * Returns the instruction to withdraw stSOL from the Anker program
 * @param senderBSolAccountOwnerAddress Address of the owner of the sender's bSOL SPL token account. Must be a signer of the transaction.
 * @param senderBSolAccountAddress Address of the bSOL SPL token account, whose bSOL balance will be decreased
 * @param recipientStSolAccountAddress Address of the recipient stSOL SPL token account, whose stSOL balance will be increased
 * @param programAddresses Solido and Anker program addresses
 * @param amount Amount of bSOL to be withdrawn
 */
export const getWithdrawInstruction = async (
  senderBSolAccountOwnerAddress: PublicKey,
  senderBSolAccountAddress: PublicKey,
  recipientStSolAccountAddress: PublicKey,
  programAddresses: ProgramAddresses,
  amount: BLamports
) => {
  const [stSolReserveAccountAddress] = await PublicKey.findProgramAddress(
    [
      programAddresses.ankerInstanceId.toBuffer(),
      Buffer.from('st_sol_reserve_account'),
    ],
    programAddresses.ankerProgramId
  );

  const [reserveAuthorityAddress] = await PublicKey.findProgramAddress(
    [
      programAddresses.ankerInstanceId.toBuffer(),
      Buffer.from('reserve_authority'),
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
    { pubkey: senderBSolAccountAddress, isSigner: false, isWritable: true },
    {
      pubkey: senderBSolAccountOwnerAddress,
      isSigner: true,
      isWritable: false,
    },
    {
      pubkey: recipientStSolAccountAddress,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: stSolReserveAccountAddress,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: reserveAuthorityAddress,
      isSigner: false,
      isWritable: true,
    },
    {
      pubkey: programAddresses.bSolMintAddress,
      isSigner: false,
      isWritable: true,
    },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
  ];

  const dataLayout = BufferLayout.struct([
    BufferLayout.u8('instruction'),
    BufferLayout.nu64('amount'),
  ]);

  const data = Buffer.alloc(dataLayout.span);
  dataLayout.encode({ instruction: 2, amount: amount.bLamports }, data);

  return new TransactionInstruction({
    keys,
    programId: programAddresses.ankerProgramId,
    data,
  });
};
