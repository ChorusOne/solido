import { clusterApiUrl, Connection } from '@solana/web3.js';
import fs from 'fs';
import path from 'path';
import { MAINNET_PROGRAM_ADDRESSES } from '../constants';

const updateAccountDump = async () => {
  const connection = new Connection(clusterApiUrl('mainnet-beta'));

  const solidoInstanceAccountInfo = await connection.getAccountInfo(
    MAINNET_PROGRAM_ADDRESSES.solidoInstanceId
  );

  if (solidoInstanceAccountInfo) {
    const updatedSolidoInstaceAccountInfo = {
      ...solidoInstanceAccountInfo,
      data: [...solidoInstanceAccountInfo.data],
    };

    let infoString = JSON.stringify(updatedSolidoInstaceAccountInfo);

    fs.writeFileSync(
      path.join(__dirname, 'data', 'solido_instance_info.json'),
      infoString
    );
  }
};

updateAccountDump();
