import {
  AccountInfo,
  Connection,
  PublicKey,
  StakeProgram,
} from '@solana/web3.js';
import BN from 'bn.js';
import { deserializeUnchecked } from 'borsh';
import { Lamports, ProgramAddresses, Snapshot, StLamports } from './types';
import {
  calculateStakeAccountAddress,
  findAuthorityProgramAddress,
} from './utils';

/**
 * Solido Program State
 *
 * Reference:
 * https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/state.rs#L187
 */
export class Solido {
  exchange_rate: ExchangeRate;
  fee_recipients: FeeRecipients;
  lido_version: number;
  maintainers: Maintainers;
  manager: PublicKey;
  metrics: Metrics;
  mint_authority_bump_seed: number;
  reward_distribution: RewardDistribution;
  rewards_withdraw_authority_bump_seed: number;
  sol_reserve_authority_bump_seed: number;
  st_sol_mint: PublicKey;
  stake_authority_bump_seed: number;
  validators: Validators;

  constructor(data: any) {
    const parsedData: Solido = {
      ...data,
      manager: new PublicKey(data.manager.toArray('le')),
      st_sol_mint: new PublicKey(data.st_sol_mint.toArray('le')),
    };

    Object.assign(this, parsedData);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/state.rs#L656
 */
class SeedRange {
  begin: BN;
  end: BN;

  constructor(data: SeedRange) {
    Object.assign(this, data);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/state.rs#L628
 */
class Validator {
  active: number;
  fee_address: PublicKey;
  fee_credit: BN;
  stake_seeds: SeedRange;
  unstake_seeds: SeedRange;
  stake_accounts_balance: BN;
  unstake_accounts_balance: BN;

  constructor(data: any) {
    const parsedData: Validator = {
      ...data,
      fee_address: new PublicKey(data.fee_address.toArray('le')),
    };
    Object.assign(this, parsedData);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/account_map.rs#L17
 */
class ValidatorPubKeyAndEntry {
  pubkey: PublicKey;
  entry: Validator;

  constructor(data: any) {
    const parsedData: ValidatorPubKeyAndEntry = {
      ...data,
      pubkey: new PublicKey(data.pubkey.toArray('le')),
    };
    Object.assign(this, parsedData);
  }
}

class MaintainerPubKeyAndEntry {
  pubkey: PublicKey;
  entry: Uint8Array;

  constructor(data: any) {
    const parsedData: MaintainerPubKeyAndEntry = {
      ...data,
      pubkey: new PublicKey(data.pubkey.toArray('le')),
    };
    Object.assign(this, parsedData);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/state.rs#L816
 */
class RewardDistribution {
  treasury_fee: number;
  validation_fee: number;
  developer_fee: number;
  st_sol_appreciation: number;

  constructor(data: RewardDistribution) {
    Object.assign(this, data);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/state.rs#L827
 */
class FeeRecipients {
  treasury_account: PublicKey;
  developer_account: PublicKey;

  constructor(data: any) {
    const parsedData: FeeRecipients = {
      ...data,
      treasury_account: new PublicKey(data.treasury_account.toArray('le')),
      developer_account: new PublicKey(data.developer_account.toArray('le')),
    };
    Object.assign(this, parsedData);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/state.rs#L39
 */
class Validators {
  entries: ValidatorPubKeyAndEntry[];
  maximum_entries: number;

  constructor(data: Validators) {
    Object.assign(this, data);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/state.rs#L50
 */
class Maintainers {
  entries: MaintainerPubKeyAndEntry[];
  maximum_entries: number;

  constructor(data: Maintainers) {
    Object.assign(this, data);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/state.rs#L131
 */
class ExchangeRate {
  computed_in_epoch: BN;
  st_sol_supply: BN;
  sol_balance: BN;

  constructor(data: ExchangeRate) {
    Object.assign(this, data);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/metrics.rs#L22
 */
class Metrics {
  fee_treasury_sol_total: BN;
  fee_validation_sol_total: BN;
  fee_developer_sol_total: BN;
  st_sol_appreciation_sol_total: BN;
  fee_treasury_st_sol_total: BN;
  fee_validation_st_sol_total: BN;
  fee_developer_st_sol_total: BN;
  deposit_amount: LamportsHistogram;
  withdraw_amount: WithdrawMetric;

  constructor(data: Metrics) {
    Object.assign(this, data);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/metrics.rs#L157
 */
class LamportsHistogram {
  counts1: BN;
  counts2: BN;
  counts3: BN;
  counts4: BN;
  counts5: BN;
  counts6: BN;
  counts7: BN;
  counts8: BN;
  counts9: BN;
  counts10: BN;
  counts11: BN;
  counts12: BN;
  total: BN;

  constructor(data: LamportsHistogram) {
    Object.assign(this, data);
  }
}

/**
 * Reference: https://github.com/ChorusOne/solido/blob/73040002ddbb62a3cee93107d03871f848ecd1e0/program/src/metrics.rs#L219
 */
class WithdrawMetric {
  count: BN;
  total_sol_amount: BN;
  total_st_sol_amount: BN;

  constructor(data: WithdrawMetric) {
    Object.assign(this, data);
  }
}

// @ts-ignore
export const schema = new Map([
  [
    ExchangeRate,
    {
      kind: 'struct',
      fields: [
        ['computed_in_epoch', 'u64'],
        ['st_sol_supply', 'u64'],
        ['sol_balance', 'u64'],
      ],
    },
  ],
  [
    LamportsHistogram,
    {
      kind: 'struct',
      fields: [
        ['counts1', 'u64'],
        ['counts2', 'u64'],
        ['counts3', 'u64'],
        ['counts4', 'u64'],
        ['counts5', 'u64'],
        ['counts6', 'u64'],
        ['counts7', 'u64'],
        ['counts8', 'u64'],
        ['counts9', 'u64'],
        ['counts10', 'u64'],
        ['counts11', 'u64'],
        ['counts12', 'u64'],
        ['total', 'u64'],
      ],
    },
  ],
  [
    WithdrawMetric,
    {
      kind: 'struct',
      fields: [
        ['total_st_sol_amount', 'u64'],
        ['total_sol_amount', 'u64'],
        ['count', 'u64'],
      ],
    },
  ],
  [
    Metrics,
    {
      kind: 'struct',
      fields: [
        ['fee_treasury_sol_total', 'u64'],
        ['fee_validation_sol_total', 'u64'],
        ['fee_developer_sol_total', 'u64'],
        ['st_sol_appreciation_sol_total', 'u64'],
        ['fee_treasury_st_sol_total', 'u64'],
        ['fee_validation_st_sol_total', 'u64'],
        ['fee_developer_st_sol_total', 'u64'],
        ['deposit_amount', LamportsHistogram],
        ['withdraw_amount', WithdrawMetric],
      ],
    },
  ],
  [
    SeedRange,
    {
      kind: 'struct',
      fields: [
        ['begin', 'u64'],
        ['end', 'u64'],
      ],
    },
  ],
  [
    Validator,
    {
      kind: 'struct',
      fields: [
        ['fee_credit', 'u64'],
        ['fee_address', 'u256'],
        ['stake_seeds', SeedRange],
        ['unstake_seeds', SeedRange],
        ['stake_accounts_balance', 'u64'],
        ['unstake_accounts_balance', 'u64'],
        ['active', 'u8'],
      ],
    },
  ],
  [
    ValidatorPubKeyAndEntry,
    {
      kind: 'struct',
      fields: [
        ['pubkey', 'u256'],
        ['entry', Validator],
      ],
    },
  ],
  [
    MaintainerPubKeyAndEntry,
    {
      kind: 'struct',
      fields: [
        ['pubkey', 'u256'],
        ['entry', [0]],
      ],
    },
  ],
  [
    RewardDistribution,
    {
      kind: 'struct',
      fields: [
        ['treasury_fee', 'u32'],
        ['validation_fee', 'u32'],
        ['developer_fee', 'u32'],
        ['st_sol_appreciation', 'u32'],
      ],
    },
  ],
  [
    FeeRecipients,
    {
      kind: 'struct',
      fields: [
        ['treasury_account', 'u256'],
        ['developer_account', 'u256'],
      ],
    },
  ],
  [
    Validators,
    {
      kind: 'struct',
      fields: [
        ['entries', [ValidatorPubKeyAndEntry]],
        ['maximum_entries', 'u32'],
      ],
    },
  ],
  [
    Maintainers,
    {
      kind: 'struct',
      fields: [
        ['entries', [MaintainerPubKeyAndEntry]],
        ['maximum_entries', 'u32'],
      ],
    },
  ],
  [
    Solido,
    {
      kind: 'struct',
      fields: [
        ['lido_version', 'u8'],

        ['manager', 'u256'],

        ['st_sol_mint', 'u256'],

        ['exchange_rate', ExchangeRate],

        ['sol_reserve_authority_bump_seed', 'u8'],
        ['stake_authority_bump_seed', 'u8'],
        ['mint_authority_bump_seed', 'u8'],
        ['rewards_withdraw_authority_bump_seed', 'u8'],

        ['reward_distribution', RewardDistribution],

        ['fee_recipients', FeeRecipients],

        ['metrics', Metrics],

        ['validators', Validators],

        ['maintainers', Maintainers],
      ],
    },
  ],
]);

export const getSolido = (solidoInstanceDataBuffer: Buffer) => {
  const deserialized = deserializeUnchecked(
    schema,
    Solido,
    solidoInstanceDataBuffer
  );

  return deserialized;
};

export const getSnapshot = async (
  connection: Connection,
  programAddresses: ProgramAddresses
): Promise<Snapshot> => {
  const accountsInfo: Record<string, AccountInfo<Buffer> | null> = {
    [programAddresses.stSolMintAddress.toString()]: null,
  };

  const reserveAccountAddress = await findAuthorityProgramAddress(
    programAddresses,
    'reserve_account'
  );

  while (true) {
    console.log('iteration');
    const addressesToGetAccountInfoFor = Object.keys(accountsInfo).map(
      (address) => new PublicKey(address)
    );

    const multipleAccountInfos = await connection.getMultipleAccountsInfo(
      addressesToGetAccountInfoFor,
      { encoding: 'jsonParsed' }
    );

    multipleAccountInfos.forEach((info, i) => {
      accountsInfo[addressesToGetAccountInfoFor[i].toString()] =
        info as AccountInfo<Buffer>;
    });

    const reserveAccountInfo = accountsInfo[reserveAccountAddress.toString()];
    const solidoInstanceAccountInfo =
      accountsInfo[programAddresses.solidoInstanceId.toString()];

    if (!solidoInstanceAccountInfo || !reserveAccountInfo) {
      accountsInfo[reserveAccountAddress.toString()] = null;
      accountsInfo[programAddresses.solidoInstanceId.toString()] = null;
      continue;
    }

    const solido = getSolido(solidoInstanceAccountInfo.data);

    const reserveAccountRentExemptionBalance =
      await connection.getMinimumBalanceForRentExemption(
        reserveAccountInfo.data.byteLength
      );

    const reserveAccountBalance = new Lamports(
      reserveAccountInfo.lamports - reserveAccountRentExemptionBalance
    );

    const stSolMintAccountInfo = accountsInfo[
      programAddresses.stSolMintAddress.toString()
    ] as any;

    if (!stSolMintAccountInfo) {
      accountsInfo[programAddresses.stSolMintAddress.toString()] = null;
      continue;
    }

    const amount = stSolMintAccountInfo.data.parsed.info.supply;
    const stSolSupply = new StLamports(amount);

    const stakeAccountRentExemptionBalance =
      await connection.getMinimumBalanceForRentExemption(StakeProgram.space);

    const validatorsStakeAccounts: {
      validatorVoteAddress: PublicKey;
      address: PublicKey;
      balance: Lamports;
    }[] = [];

    for (let i = 0; i < solido.validators.entries.length; i++) {
      const validator = solido.validators.entries[i];

      const validatorStakeAccountAddress = await calculateStakeAccountAddress(
        programAddresses.solidoInstanceId,
        programAddresses.solidoProgramId,
        validator.pubkey,
        validator.entry.stake_seeds.begin
      );

      validatorsStakeAccounts.push({
        validatorVoteAddress: validator.pubkey,
        address: validatorStakeAccountAddress,
        balance: new Lamports(0),
      });
    }

    const validatorStakeAccountsInfos = validatorsStakeAccounts.map(
      (account) => {
        return accountsInfo[account.address.toString()];
      }
    );

    const countOfInfoFetchedStakeAccounts = validatorStakeAccountsInfos.filter(
      (a) => !!a
    ).length;

    if (countOfInfoFetchedStakeAccounts < validatorsStakeAccounts.length) {
      validatorsStakeAccounts.forEach((a) => {
        accountsInfo[a.address.toString()] = null;
      });
      continue;
    }

    validatorsStakeAccounts.forEach((a, i) => {
      const validatorStakeAccountInfo = validatorStakeAccountsInfos[i];

      if (validatorStakeAccountInfo) {
        a.balance = new Lamports(validatorStakeAccountInfo.lamports);
      }
    });

    return {
      solido,
      programAddresses,
      reserveAccountBalance: reserveAccountBalance,
      stSolSupply: stSolSupply,
      stakeAccountRentExemptionBalance: new Lamports(
        stakeAccountRentExemptionBalance
      ),
      validatorsStakeAccounts,
    };
  }
};
