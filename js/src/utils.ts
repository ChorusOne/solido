import { PublicKey } from '@solana/web3.js';
import type BN from 'bn.js';
import type { ProgramAddresses, Snapshot } from './types';
import { Lamports, StLamports } from './types';

/**
 * Derives the addresses from seed and solido program
 * @param programAddresses Addresses of the program deployment
 * @param additionalSeedString Seed string
 * @returns Program derived address
 */
export const findAuthorityProgramAddress = async (
  programAddresses: ProgramAddresses,
  additionalSeedString: string
) => {
  const bufferArray = [
    programAddresses.solidoInstanceId.toBuffer(),
    Buffer.from(additionalSeedString),
  ];

  return (
    await PublicKey.findProgramAddress(
      bufferArray,
      programAddresses.solidoProgramId
    )
  )[0];
};

/**
 * Derives the stake account address from seed and validator's vote account address
 * @param solidoInstanceId Solido instance account address
 * @param solidoProgramId Solido program address
 * @param validatorVoteAccount Vote account for the validator
 * @param seed Seed to derive the address
 * @returns Stake account address
 */
export const calculateStakeAccountAddress = async (
  solidoInstanceId: PublicKey,
  solidoProgramId: PublicKey,
  validatorVoteAccount: PublicKey,
  seed: BN
) => {
  const bufferArray = [
    solidoInstanceId.toBuffer(),
    validatorVoteAccount.toBuffer(),
    Buffer.from('validator_stake_account'),
    Buffer.from(seed.toArray('le', 8)),
  ];

  const [stakeAccountAddress] = await PublicKey.findProgramAddress(
    bufferArray,
    solidoProgramId
  );

  return stakeAccountAddress;
};

/**
 * Get the stake account that has the most amount of SOL staked (heaviest)
 * @param snapshot Snapshot of the Solido stats
 * @returns Heaviest stake account
 */
export const getHeaviestValidatorStakeAccount = (
  snapshot: Snapshot
): Snapshot['validatorsStakeAccounts'][0] => {
  let heaviestValidatorStakeAccount = snapshot.validatorsStakeAccounts[0];

  snapshot.validatorsStakeAccounts.forEach((validatorStakeAccount) => {
    if (
      validatorStakeAccount.balance.lamports.gt(
        heaviestValidatorStakeAccount.balance.lamports
      )
    ) {
      heaviestValidatorStakeAccount = validatorStakeAccount;
    }
  });

  return heaviestValidatorStakeAccount;
};

/**
 * Exchange SOL to stSOL
 * @param snapshot Snapshot of the Solido stats
 * @param amount SOL to exchange
 */
export const exchangeSol = (
  snapshot: Snapshot,
  amount: Lamports
): StLamports => {
  const exchangeRate = snapshot.solido.exchange_rate;

  // The stSOL/SOL ratio is 1:1 for a fresh deployment(i.e., stSolSupply is 0)
  // So the user would get same amount of stSOL as SOL deposited
  if (exchangeRate.st_sol_supply.toString() === '0') {
    return new StLamports(amount.lamports.toString());
  }

  return new StLamports(
    (amount.lamports.toNumber() * exchangeRate.st_sol_supply.toNumber()) /
      exchangeRate.sol_balance.toNumber()
  );
};

/**
 * Exchange stSOL to SOL
 * @param snapshot Snapshot of the Solido stats
 * @param amount stSOL to exchange
 */
export const exchangeStSol = (
  snapshot: Snapshot,
  amount: StLamports
): Lamports => {
  const exchangeRate = snapshot.solido.exchange_rate;

  return new Lamports(
    (amount.stLamports.toNumber() * exchangeRate.sol_balance.toNumber()) /
      exchangeRate.st_sol_supply.toNumber()
  );
};
