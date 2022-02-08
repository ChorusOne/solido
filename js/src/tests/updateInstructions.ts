import {
  clusterApiUrl,
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
} from '@solana/web3.js';
import { writeFileSync } from 'fs';
import path from 'path';
import { format } from 'prettier';
import {
  DEVNET_PROGRAM_ADDRESSES,
  MAINNET_PROGRAM_ADDRESSES,
} from '../constants';
import { getDepositInstruction as getAnkerDepositInstruction } from '../instructions/anker/deposit';
import { getWithdrawInstruction as getAnkerWithdrawInstruction } from '../instructions/anker/withdraw';
import { getDepositInstruction as getSolidoDepositInstruction } from '../instructions/deposit';
import { getWithdrawInstruction as getSolidoWithdrawInstruction } from '../instructions/withdraw';
import { getSnapshot as getSolidoSnapshot } from '../solidoSnapshot';
import { BLamports, Lamports, ProgramAddresses, StLamports } from '../types';

const updateSolidoInstructions = async (
  cluster: 'mainnet' | 'devnet',
  connection: Connection,
  programAddresses: ProgramAddresses
) => {
  const solidoSnapshot = await getSolidoSnapshot(connection, programAddresses);

  let senderAddress = new Keypair().publicKey;
  let stSolAccountAddress = new Keypair().publicKey;
  let stakeAccountAddress = new Keypair().publicKey;
  const amountToDeposit = new Lamports(LAMPORTS_PER_SOL.toString());
  const amountToWithdraw = new StLamports(LAMPORTS_PER_SOL.toString());

  try {
    const { data: depositData } = await getSolidoDepositInstruction(
      senderAddress,
      stSolAccountAddress,
      programAddresses,
      amountToDeposit
    );

    const { data: withdrawData } = await getSolidoWithdrawInstruction(
      solidoSnapshot,
      senderAddress,
      stSolAccountAddress,
      stakeAccountAddress,
      amountToWithdraw
    );

    let rawString = `
        export const deposit = {
          data: Buffer.from([${[...depositData]}]),
        };
        
        export const withdraw = {
          data: Buffer.from([${[...withdrawData]}]),
        }
        `;

    rawString = format(rawString, { parser: 'babel-ts' });

    writeFileSync(
      path.join(__dirname, `./data/${cluster}/solido_instructions.ts`),
      rawString
    );
  } catch (error) {
    console.error('Something went wrong', error);
  }
};

const updateAnkerInstruction = async (
  cluster: 'mainnet' | 'devnet',
  programAddresses: ProgramAddresses
) => {
  let senderAddress = new Keypair().publicKey;
  let stSolAccountAddress = new Keypair().publicKey;
  let bSolAccountAddress = new Keypair().publicKey;

  const amountToDeposit = new StLamports(LAMPORTS_PER_SOL.toString());
  const amountToWithdraw = new BLamports(LAMPORTS_PER_SOL.toString());

  const { data: depositData } = await getAnkerDepositInstruction(
    senderAddress,
    stSolAccountAddress,
    bSolAccountAddress,
    programAddresses,
    amountToDeposit
  );

  const { data: withdrawData } = await getAnkerWithdrawInstruction(
    senderAddress,
    stSolAccountAddress,
    bSolAccountAddress,
    programAddresses,
    amountToWithdraw
  );

  let rawString = `
	export const deposit = {
	  data: Buffer.from([${[...depositData]}]),
	};

	export const withdraw = {
	  data: Buffer.from([${[...withdrawData]}]),
	}
  `;

  rawString = format(rawString, { parser: 'babel-ts' });

  writeFileSync(
    path.join(__dirname, `./data/${cluster}/anker_instructions.ts`),
    rawString
  );
};

const main = async () => {
  const args = process.argv.slice(2);
  const cluster = args[0] as 'mainnet' | 'devnet';

  let connection: Connection;

  switch (cluster) {
    case 'mainnet':
      connection = new Connection(clusterApiUrl('mainnet-beta'));
      await updateSolidoInstructions(
        cluster,
        connection,
        MAINNET_PROGRAM_ADDRESSES
      );
      await updateAnkerInstruction(cluster, MAINNET_PROGRAM_ADDRESSES);
      break;
    case 'devnet':
    default:
      connection = new Connection(clusterApiUrl('devnet'));
      await updateSolidoInstructions(
        'devnet',
        connection,
        DEVNET_PROGRAM_ADDRESSES
      );
      await updateAnkerInstruction('devnet', DEVNET_PROGRAM_ADDRESSES);
      break;
  }
};

main();
