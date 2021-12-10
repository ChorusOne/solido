import { PublicKey } from '@solana/web3.js';
import type BN from 'bn.js';
import type { ProgramAddresses, Snapshot } from './types';

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

export const getHeaviestValidatorStakeAccount = (
  snapshot: Snapshot
): Snapshot['validatorsStakeAccounts'][0] => {
  const sortedValidatorStakeAccounts = snapshot.validatorsStakeAccounts.sort(
    (stakeAccountA, stakeAccountB) =>
      stakeAccountB.balance.lamports
        .sub(stakeAccountA.balance.lamports)
        .toNumber()
  );

  const heaviestValidatorStakeAccount = sortedValidatorStakeAccounts[0];

  return heaviestValidatorStakeAccount;
};
