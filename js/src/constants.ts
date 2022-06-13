import { PublicKey } from '@solana/web3.js';
import { ProgramAddresses } from './types';

/**
 * Program addresses for the program deployment on mainnet
 */
export const MAINNET_PROGRAM_ADDRESSES: ProgramAddresses = {
  solidoProgramId: new PublicKey(
    'CrX7kMhLC3cSsXJdT7JDgqrRVWGnUpX3gfEfxxU2NVLi'
  ),
  solidoInstanceId: new PublicKey(
    '49Yi1TKkNyYjPAFdR9LBvoHcUjuPX4Df5T5yv39w2XTn'
  ),
  stSolMintAddress: new PublicKey(
    '7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj'
  ),
  ankerProgramId: PublicKey.default,
  ankerInstanceId: PublicKey.default,
  bSolMintAddress: PublicKey.default,
};

export const DEVNET_PROGRAM_ADDRESSES: ProgramAddresses = {
  solidoProgramId: new PublicKey(
    '874qdedig9MnSiinBkErWvafQacAfwzkHjHyE6XTa8kg'
  ),
  solidoInstanceId: new PublicKey(
    'EMtjYGwPnXdtqK5SGL8CWGv4wgdBQN79UPoy53x9bBTJ'
  ),
  stSolMintAddress: new PublicKey(
    'H6L2MwgQPVCoyETqFyqiuJgW3reCxFdesnAb579qzX88'
  ),

  ankerProgramId: new PublicKey('8MT6MtwbSdNyYH655cDxf2MypYSVfmAdx8jXrBWPREzf'),
  ankerInstanceId: new PublicKey(
    'BovX97d8MnVTbpwbBdyjSrEr7RvxN8AHEk3dYwTEx7RD'
  ),
  bSolMintAddress: new PublicKey(
    '3FMBoeddUhtqxepzkrxPrMUV3CL4bZM5QmMoLJfEpirz'
  ),
};

/**
 * Program Id for the Memo program
 */
export const MEMO_PROGRAM_ID = new PublicKey(
  'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr'
);
