import { BLamports, StLamports } from './types';
import { Solido } from './solidoSnapshot';

/**
 * Get amount of stSOL to be received when withdrawn from the Anker Program
 * @param exchangeRate Solido state from the snapshot
 * @param amount Amount of bSOL to convert
 * @returns Amount of stSOL to be received after conversion
 */
export const convertBSolToStSol = (
  exchangeRate: Solido['exchange_rate'],
  amount: BLamports
) => {
  return new StLamports(
    (amount.bLamports.toNumber() * exchangeRate.st_sol_supply.toNumber()) /
      exchangeRate.sol_balance.toNumber()
  );
};

/**
 * Get amount of bSOL to be received when deposited to the Anker Program
 * @param exchangeRate Solido state from the snapshot
 * @param amount Amount of stSOL to convert
 * @returns Amount of bSOL to be received after conversion
 */
export const convertStSolToBSol = (
  exchangeRate: Solido['exchange_rate'],
  amount: StLamports
) => {
  if (exchangeRate.st_sol_supply.toString() === '0') {
    return new BLamports(amount.stLamports.toString());
  }

  return new BLamports(
    (amount.stLamports.toNumber() * exchangeRate.sol_balance.toNumber()) /
      exchangeRate.st_sol_supply.toNumber()
  );
};
