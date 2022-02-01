import { PublicKey } from '@solana/web3.js';
import type BN from 'bn.js';
import { getExchangeRate } from './stats';
import { Lamports, StLamports } from './types';
import type { ProgramAddresses, Snapshot } from './types';

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
export const exchangeSol = (snapshot: Snapshot, amount: Lamports) => {
  const exchangeRate = getExchangeRate(snapshot);

  return new StLamports(amount.lamports.toNumber() / exchangeRate);
};

/**
 * Exchange stSOL to SOL
 * @param snapshot Snapshot of the Solido stats
 * @param amount stSOL to exchange
 */
export const exchangeStSol = (snapshot: Snapshot, amount: StLamports) => {
  const exchangeRate = getExchangeRate(snapshot);

  return new Lamports(amount.stLamports.toNumber() * exchangeRate);
};
