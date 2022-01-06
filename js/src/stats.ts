import { Connection, PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import { ExchangeRate, Lamports, Snapshot, StLamports } from './types';

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
export const getExchangeRate = (snapshot: Snapshot): ExchangeRate => {
  const totalSolInLamports = snapshot.solido.exchange_rate.sol_balance;
  const stSolSupplyInLamports = snapshot.solido.exchange_rate.st_sol_supply;

  return {
    solBalance: { lamports: totalSolInLamports },
    stSolSupply: { stLamports: stSolSupplyInLamports },
  };
};

/**
 * Get the number of all the token accounts exist for the token specified by the mint address
 * @param connection Connection to the cluster
 * @param tokenMintAddress Address of the token mint account
 * @param tokenProgramId Address of the Token program
 * @returns Number of token accounts
 */
export const getTotalNumberOfTokenAccounts = async (
  connection: Connection,
  tokenMintAddress: PublicKey,
  tokenProgramId: PublicKey
) => {
  const memcmpFilter = {
    memcmp: { bytes: tokenMintAddress.toString(), offset: 0 },
  };
  const config = {
    filters: [{ dataSize: 165 }, memcmpFilter],
    encoding: 'jsonParsed',
  };

  const accounts = await connection.getParsedProgramAccounts(
    tokenProgramId,
    config
  );

  return accounts.length;
};

/**
 * Get all the token accounts(specified by the mint address) for the given owner account
 * @param connection Connection to the cluster
 * @param tokenMintAddress Address of the token mint account
 * @param ownerAccountAddress Address of the owner of the token
 * @returns List of token accounts
 */
export const getOwnerTokenAccounts = async (
  connection: Connection,
  tokenMintAddress: PublicKey,
  ownerAccountAddress: PublicKey
) => {
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
