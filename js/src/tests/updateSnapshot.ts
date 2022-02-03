import { clusterApiUrl, Connection, PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import fs from 'fs';
import path from 'path';
import util from 'util';
import { MAINNET_PROGRAM_ADDRESSES } from '../constants';
import { getSnapshot } from '../snapshot';

const updateSnapshot = async () => {
  const connection = new Connection(clusterApiUrl('mainnet-beta'));

  const snapshot = await getSnapshot(connection, MAINNET_PROGRAM_ADDRESSES);

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
  rawString = rawString.replace(/\[\w+\]: .+/g, '');

  // Replaced maintainer entry with the correct one
  rawString = rawString.replace(
    /entry: Uint8Array[\w\d\s\[\(\)]+]/g,
    'entry: Uint8Array.from([]),'
  );

  // Added imports for the file
  rawString = `import { PublicKey } from '@solana/web3.js';
  import BN from 'bn.js';
  import { Snapshot } from '../../types';
  
  export const snapshot: Snapshot = ${rawString}`;

  fs.writeFileSync(
    path.join(__dirname, 'data', 'snapshot.ts'),
    rawString,
    'utf-8'
  );
};

updateSnapshot();
