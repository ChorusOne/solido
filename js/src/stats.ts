import { TOKEN_PROGRAM_ID } from '@solana/spl-token';
import { Connection, PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import { Lamports, Snapshot, StLamports } from './types';

/**
 * Get total value locked in the solido program
 * @param snapshot Solido snapshot
 * @returns Total value locked
 */
export const getTotalValueLocked = (snapshot: Snapshot): Lamports => {
  const validatorsStakeAccountsBalanceInLamports =
    snapshot.solido.validators.entries
      .map((pubKeyAndEntry) => pubKeyAndEntry.entry)
      .map((validator) => validator.stake_accounts_balance)
      .reduce((acc, current) => acc.add(current), new BN(0));

  return {
    lamports: snapshot.reserveAccountBalance.lamports.add(
      validatorsStakeAccountsBalanceInLamports
    ),
  };
};

/**
 * Get the total stSOL supply
 * @param snapshot Solido snapshot
 * @param type 'totalcoins' or 'circulating'
 *
 * - With type=circulating, we return the stSOL supply according to the SPL token mint. It’s the amount of tokens that exists at the moment, excluding tokens that we already know will be minted in the future, but which haven’t been minted yet at this time.
 * - With type=totalcoins, we also include fees that have been earned, for which stSOL will be minted soon, but which hasn’t been minted yet at this time.
 *
 * In practice, the two values are almost always equal. They can differ briefly at the start of the epoch, when fees are distributed.
 *
 * @returns stSOL supply
 */
export const getStSolSupply = (
  snapshot: Snapshot,
  type: 'totalcoins' | 'circulating'
): StLamports => {
  const totalFeeCredits = snapshot.solido.validators.entries.reduce(
    (acc, eachValidator) => acc.add(eachValidator.entry.fee_credit),
    new BN(0)
  );

  switch (type) {
    case 'circulating': {
      return snapshot.stSolSupply;
    }
    case 'totalcoins': {
      return {
        stLamports: snapshot.stSolSupply.stLamports.add(totalFeeCredits),
      };
    }
  }
};

/**
 * Get exchange rate for the current epoch in terms of SOL and stSOL supply
 * @param snapshot Solido snapshot
 * @returns Exchange rate
 */
export const getExchangeRate = (snapshot: Snapshot): number => {
  const totalSolInLamports = snapshot.solido.exchange_rate.sol_balance;
  const stSolSupplyInStLamports = snapshot.solido.exchange_rate.st_sol_supply;

  return totalSolInLamports.toNumber() / stSolSupplyInStLamports.toNumber();
};

/**
 * Get the number of  token accounts that exist for the token specified by the mint address
 * @param connection Connection to the cluster
 *
 * **Note: RPC node needs to have account indexing enabled for the SPL token mint that we query, and this is not enabled by default**
 *
 * @param tokenMintAddress Address of the token mint account
 * @returns Number of token accounts
 */
export const getTotalNumberOfTokenAccounts = async (
  connection: Connection,
  tokenMintAddress: PublicKey
) => {
  const memcmpFilter = {
    memcmp: { bytes: tokenMintAddress.toString(), offset: 0 },
  };
  const config = {
    filters: [{ dataSize: 165 }, memcmpFilter],
    dataSlice: { offset: 0, length: 0 },
    encoding: 'base64',
  };

  const accounts = await connection.getParsedProgramAccounts(
    TOKEN_PROGRAM_ID,
    config
  );

  return accounts.length;
};

/**
 * Get all the token accounts (specified by the mint address) for the given owner account
 * @param connection Connection to the cluster
 * @param tokenMintAddress Address of the token mint account
 * @param ownerAccountAddress Address of the owner of the token
 * @returns List of token accounts
 */
export const getTokenAccountsByOwner = async (
  connection: Connection,
  tokenMintAddress: PublicKey,
  ownerAccountAddress: PublicKey
): Promise<{ address: PublicKey; balance: Lamports }[]> => {
  const tokenAccounts: { address: PublicKey; balance: Lamports }[] = [];

  const { value } = await connection.getParsedTokenAccountsByOwner(
    ownerAccountAddress,
    {
      mint: tokenMintAddress,
    }
  );

  value.forEach((v) => {
    const address = v.pubkey;
    const balance = new Lamports(v.account.data.parsed.info.tokenAmount.amount);

    tokenAccounts.push({ address, balance });
  });

  return tokenAccounts;
};
