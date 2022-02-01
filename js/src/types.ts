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
 *
 * 1 lamport = 1e-9 SOL, and is the smallest possible amount of SOL
 */
export class Lamports {
  lamports: BN;

  constructor(lamports: number | string | BN) {
    this.lamports = new BN(lamports);
  }
}

/**
 * Balance of stSOL account
 *
 * 1 stLamport = 1e-9 stSOL, and is the smallest possible amount of stSOL
 */
export class StLamports {
  stLamports: BN;

  constructor(stLamports: number | string | BN) {
    this.stLamports = new BN(stLamports);
  }
}

/**
 * Snapshot of the Solido stats
 *
 * Snapshot of all Solido-related accounts at a given slot.
 *
 * From the snapshot we can query all Solido stats, and it is also the starting point for constructing transactions.
 *
 * There are multiple accounts that are relevant to the Solido program, aside from the main instance.
 * For example, the validators’ stake accounts.
 * To be able to get a consistent view of those accounts, we read them atomically with the `getMultipleAccounts` RPC call.
 * The snapshot holds the parsed results.
 */
export interface Snapshot {
  solido: Solido;
  programAddresses: ProgramAddresses;
  reserveAccountBalance: Lamports;
  stSolSupply: StLamports;
  stakeAccountRentExemptionBalance: Lamports;
  validatorsStakeAccounts: {
    validatorVoteAddress: PublicKey;
    address: PublicKey;
    balance: Lamports;
  }[];
}
