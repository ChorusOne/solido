import { clusterApiUrl, Connection } from '@solana/web3.js';
import fs from 'fs';
import path from 'path';
import { ProgramAddresses } from '../types';
import {
  MAINNET_PROGRAM_ADDRESSES,
  DEVNET_PROGRAM_ADDRESSES,
} from '../constants';
import { format } from 'prettier';

const updateSolidoAccountDump = async (
  cluster: 'mainnet' | 'devnet',
  connection: Connection,
  programAddresses: ProgramAddresses
) => {
  const solidoInstanceAccountInfo = await connection.getAccountInfo(
    programAddresses.solidoInstanceId
  );

  if (solidoInstanceAccountInfo) {
    const updatedSolidoInstaceAccountInfo = {
      ...solidoInstanceAccountInfo,
      data: [...solidoInstanceAccountInfo.data],
    };

    let infoString = JSON.stringify(updatedSolidoInstaceAccountInfo);

    infoString = format(infoString, { parser: 'babel-ts' });

    fs.writeFileSync(
      path.join(__dirname, 'data', cluster, 'solido_instance_info.json'),
      infoString
    );
  }
};

const updateAnkerAccountDump = async (
  cluster: 'mainnet' | 'devnet',
  connection: Connection,
  programAddresses: ProgramAddresses
) => {
  const ankerInstanceAccountInfo = await connection.getAccountInfo(
    programAddresses.ankerInstanceId
  );

  if (ankerInstanceAccountInfo) {
    const updatedAnkerInstaceAccountInfo = {
      ...ankerInstanceAccountInfo,
      data: [...ankerInstanceAccountInfo.data],
    };

    let infoString = JSON.stringify(updatedAnkerInstaceAccountInfo);

    infoString = format(infoString, { parser: 'babel-ts' });

    fs.writeFileSync(
      path.join(__dirname, 'data', cluster, 'anker_instance_info.json'),
      infoString
    );
  }
};

const main = async () => {
  const args = process.argv.slice(2);
  const cluster = args[0] as 'mainnet' | 'devnet';

  let connection: Connection;

  switch (cluster) {
    case 'mainnet':
      connection = new Connection(clusterApiUrl('mainnet-beta'));
      await updateSolidoAccountDump(
        cluster,
        connection,
        MAINNET_PROGRAM_ADDRESSES
      );
      await updateAnkerAccountDump(
        cluster,
        connection,
        MAINNET_PROGRAM_ADDRESSES
      );
      break;
    case 'devnet':
    default:
      connection = new Connection(clusterApiUrl('devnet'));
      await updateSolidoAccountDump(
        'devnet',
        connection,
        DEVNET_PROGRAM_ADDRESSES
      );
      await updateAnkerAccountDump(
        'devnet',
        connection,
        DEVNET_PROGRAM_ADDRESSES
      );
      break;
  }
};

main();
