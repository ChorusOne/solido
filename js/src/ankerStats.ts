import { AnkerSnapshot, Lamports, StLamports } from './types';
import { exchangeStSol } from './utils';

/**
 * Get the total value locked in the Anker program
 * @param snapshot Snapshot of the Anker state
 * @returns Total value locked in Anker program in SOL and stSOL
 */
export const getTotalValueLocked = (
  snapshot: AnkerSnapshot
): {
  sol: Lamports;
  stSol: StLamports;
} => {
  const stSolReserveAccountBalance = snapshot.stSolReserveAccountBalance;

  return {
    sol: exchangeStSol(snapshot.solido, stSolReserveAccountBalance),
    stSol: stSolReserveAccountBalance,
  };
};
