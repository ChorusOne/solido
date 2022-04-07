import { clusterApiUrl, Connection, PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import fs from 'fs';
import path from 'path';
import util from 'util';
import {
  MAINNET_PROGRAM_ADDRESSES,
  DEVNET_PROGRAM_ADDRESSES,
} from '../constants';
import { getSnapshot as getSolidoSnapshot } from '../solidoSnapshot';
import { getSnapshot as getAnkerSnapshot } from '../ankerSnapshot';
import type { ProgramAddresses } from '../types';
import { format } from 'prettier';

const updateSolidoSnapshot = async (
  cluster: 'mainnet' | 'devnet',
  connection: Connection,
  programAddresses: ProgramAddresses
) => {
  const snapshot = await getSolidoSnapshot(connection, programAddresses);

  let rawString = util.inspect(snapshot, true, 10, false);

  // Replace the ugly <BN: ...> public keys with readable ones
  rawString = rawString.replace(
    /PublicKey {[\s]+_bn: <BN: (\w+)>[\s]+}/g,
    (_match, tag: string) => {
      const updatedString = `new PublicKey("${new PublicKey(
        new BN(tag.trim(), 'hex')
      ).toString()}")`;
      return updatedString;
    }
  );

  // Removed all type inferences
  // eg., reward_distribution: RewardDistribution {...}
  rawString = rawString.replace(/: [\w]+ {/g, () => ': {');

  // Replaced all big numbers to their values
  // <BN: 1> => new BN("1")
  rawString = rawString.replace(/<BN: ([\w]+)>/g, (_, tag: string) => {
    const parsedNumber = new BN(tag.trim(), 'hex').toString();
    return `new BN("${parsedNumber}")`;
  });

  // Removed unnecessary types
  rawString = rawString.replace(/MaintainerPubKeyAndEntry/g, '');
  rawString = rawString.replace(/ValidatorPubKeyAndEntry/g, '');

  // Removed all lengths of arrays
  // [length]: 4
  rawString = rawString.replace(/\[\w+\]: [\S]+/g, '');

  // Replaced maintainer entry with the correct one
  rawString = rawString.replace(
    /entry: Uint8Array[\w\:\s\d\n\}\(\)\[]+]/g,
    'entry: Uint8Array.from([]),'
  );

  // Replace token values with the correct class instances
  rawString = rawString.replace(
    /{ lamports: new BN\(([\d\"]+)\) }/gi,
    (_, tag: string) => `new Lamports(${tag.trim()})`
  );
  rawString = rawString.replace(
    /{ stLamports: new BN\(([\d\"]+)\) }/gi,
    (_, tag: string) => `new StLamports(${tag.trim()})`
  );

  // Added imports for the file
  rawString = `import { PublicKey } from '@solana/web3.js';
  import BN from 'bn.js';
  import { Snapshot, Lamports, StLamports } from '../../../types';
  
  export const snapshot: Snapshot = ${rawString}`;

  rawString = format(rawString, { parser: 'babel-ts' });

  fs.writeFileSync(
    path.join(__dirname, 'data', cluster, 'solido_snapshot.ts'),
    rawString,
    'utf-8'
  );
};

const updateAnkerSnapshot = async (
  cluster: 'mainnet' | 'devnet',
  connection: Connection,
  programAddresses: ProgramAddresses
) => {
  const snapshot = await getAnkerSnapshot(connection, programAddresses);

  snapshot.solido.maintainers.entries = snapshot.solido.maintainers.entries.map(
    (e) => ({ pubkey: e.pubkey, entry: Uint8Array.from([]) })
  );

  let rawString = util.inspect(snapshot, true, 10, false);

  // Replace the ugly <BN: ...> public keys with readable ones
  rawString = rawString.replace(
    /PublicKey {[\s]+_bn: <BN: (\w+)>[\s]+}/g,
    (_match, tag: string) => {
      const updatedString = `new PublicKey("${new PublicKey(
        new BN(tag.trim(), 'hex')
      ).toString()}")`;
      return updatedString;
    }
  );

  // Removed all type inferences
  // eg., reward_distribution: RewardDistribution {...}
  rawString = rawString.replace(/: [\w]+ {/g, () => ': {');

  // Replaced all big numbers to their values
  // <BN: 1> => new BN("1")
  rawString = rawString.replace(/<BN: ([\w]+)>/g, (_, tag: string) => {
    const parsedNumber = new BN(tag.trim(), 'hex').toString();
    return `new BN("${parsedNumber}")`;
  });

  // Removed unnecessary types
  rawString = rawString.replace(/MaintainerPubKeyAndEntry/g, '');
  rawString = rawString.replace(/ValidatorPubKeyAndEntry/g, '');

  // Removed all lengths of arrays
  // [length]: 4
  rawString = rawString.replace(/\[\w+\]: [\S]+/g, '');

  // Replaced maintainer entry with the correct one
  rawString = rawString.replace(
    /entry: Uint8Array[\w\:\s\d\n\}\(\)\[]+]/g,
    'entry: Uint8Array.from([]),'
  );

  // Replace token values with the correct class instances
  rawString = rawString.replace(
    /{ lamports: new BN\(([\d\"]+)\) }/gi,
    (_, tag: string) => `new Lamports(${tag.trim()})`
  );
  rawString = rawString.replace(
    /{ stLamports: new BN\(([\d\"]+)\) }/gi,
    (_, tag: string) => `new StLamports(${tag.trim()})`
  );

  // Added imports for the file
  rawString = `import { PublicKey } from '@solana/web3.js';
  import BN from 'bn.js';
  import { AnkerSnapshot, Lamports, StLamports } from '../../../types';
  
  export const snapshot: AnkerSnapshot = ${rawString}`;

  rawString = format(rawString, {
    parser: 'babel-ts',
  });

  fs.writeFileSync(
    path.join(__dirname, 'data', cluster, 'anker_snapshot.ts'),
    rawString,
    'utf-8'
  );
};

const main = async () => {
  const args = process.argv.slice(2);
  const cluster = args[0] as 'mainnet' | 'devnet';

  let connection: Connection;

  switch (cluster) {
    case 'mainnet':
      connection = new Connection(clusterApiUrl('mainnet-beta'));
      await updateSolidoSnapshot(
        cluster,
        connection,
        MAINNET_PROGRAM_ADDRESSES
      );
      await updateAnkerSnapshot(cluster, connection, MAINNET_PROGRAM_ADDRESSES);
      break;
    case 'devnet':
    default:
      connection = new Connection(clusterApiUrl('devnet'));
      await updateSolidoSnapshot(
        'devnet',
        connection,
        DEVNET_PROGRAM_ADDRESSES
      );
      await updateAnkerSnapshot('devnet', connection, DEVNET_PROGRAM_ADDRESSES);
      break;
  }
};

main();
