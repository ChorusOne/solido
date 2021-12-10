import type { PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import type { Solido } from './snapshot';

export interface ProgramAddresses {
  solidoProgramId: PublicKey;
  solidoInstanceId: PublicKey;
  stSolMintAddress: PublicKey;
}

export class Lamports {
  lamports: BN;

  constructor(lamports: number | string) {
    this.lamports = new BN(lamports);
  }
}

export class StLamports {
  stLamports: BN;

  constructor(stLamports: number | string) {
    this.stLamports = new BN(stLamports);
  }
}

export interface ExchangeRate {
  solBalance: Lamports;
  stSolSupply: StLamports;
}

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
