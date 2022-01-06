import type { PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import type { Solido } from './snapshot';

/**
 * Program addresses for the program deployment
 */
export interface ProgramAddresses {
  solidoProgramId: PublicKey;
  solidoInstanceId: PublicKey;
  stSolMintAddress: PublicKey;
}

/**
 * Balance of SOL account
 */
export class Lamports {
  lamports: BN;

  constructor(lamports: number | string) {
    this.lamports = new BN(lamports);
  }
}

/**
 * Balance of stSOL account
 */
export class StLamports {
  stLamports: BN;

  constructor(stLamports: number | string) {
    this.stLamports = new BN(stLamports);
  }
}

/**
 * Exchange Rate
 */
export interface ExchangeRate {
  solBalance: Lamports;
  stSolSupply: StLamports;
}

/**
 * Snapshot of the Solido stats
 */
export interface Snapshot {
  solido: Solido;
  reserveAccountBalance: Lamports;
  stSolSupply: StLamports;
  stakeAccountRentExemptionBalance: Lamports;
  validatorsStakeAccounts: {
    validatorVoteAddress: PublicKey;
    address: PublicKey;
    balance: Lamports;
  }[];
}
