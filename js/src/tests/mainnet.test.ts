import { Keypair, LAMPORTS_PER_SOL, PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import {
  exchangeSol,
  exchangeStSol,
  getDepositInstruction,
  getExchangeRate,
  getSolido,
  getStSolSupply,
  getTotalValueLocked,
  getWithdrawInstruction,
  Lamports,
  MAINNET_PROGRAM_ADDRESSES,
  StLamports,
} from '..';
import {
  calculateStakeAccountAddress,
  findAuthorityProgramAddress,
  getHeaviestValidatorStakeAccount,
} from '../utils';
import {
  deposit as depositInstructionDump,
  withdraw as withdrawInstructionDump,
} from './data/mainnet/instructions';
import solidoInstanceInfoDump from './data/mainnet/solido_instance_info.json';
import { snapshot as snapshotDump } from './data/mainnet/solido_snapshot';

//////////////// Deserializer ////////////////

describe('Deserializer', () => {
  it('deserializes solido instance from info', () => {
    const solido = getSolido(Buffer.from(solidoInstanceInfoDump.data));

    expect(JSON.stringify(solido)).toEqual(JSON.stringify(snapshotDump.solido));
  });

  it('deserialize stSOL mint address from solido instance dump', () => {
    const solido = snapshotDump.solido;

    expect(solido.st_sol_mint.toString()).toBe(
      MAINNET_PROGRAM_ADDRESSES.stSolMintAddress.toString()
    );
  });

  it('ensures lido version is 0', () => {
    const solido = snapshotDump.solido;

    expect(solido.lido_version).toBe(0);
  });
});

//////////////// UTILS /////////////////

describe('Utility functions', () => {
  it('finds reserve address', async () => {
    const reserveAccountAddress = await findAuthorityProgramAddress(
      MAINNET_PROGRAM_ADDRESSES,
      'reserve_account'
    );

    expect(reserveAccountAddress.toString()).toBe(
      '3Kwv3pEAuoe4WevPB4rgMBTZndGDb53XT7qwQKnvHPfX'
    );
  });

  it('finds mint authority address', async () => {
    const reserveAccountAddress = await findAuthorityProgramAddress(
      MAINNET_PROGRAM_ADDRESSES,
      'mint_authority'
    );

    expect(reserveAccountAddress.toString()).toBe(
      '8kRRsKezwXS21beVDcAoTmih1XbyFnEAMXXiGXz6J3Jz'
    );
  });

  it('finds stake account address of a validator', async () => {
    const validator = snapshotDump.solido.validators.entries[0];

    const stakeAccountAddress = await calculateStakeAccountAddress(
      MAINNET_PROGRAM_ADDRESSES.solidoInstanceId,
      MAINNET_PROGRAM_ADDRESSES.solidoProgramId,
      validator.pubkey,
      validator.entry.stake_seeds.begin
    );

    expect(stakeAccountAddress.toString()).toBe(
      new PublicKey('AYKAnKBShqEyz2UMLv7Px5CtuWSHEYS2W1V1GTKGKNwE').toString()
    );
  });

  it('finds heaviest validator', async () => {
    const { validatorVoteAddress: heaviestValidator } =
      getHeaviestValidatorStakeAccount(snapshotDump);

    expect(new PublicKey(heaviestValidator).toString()).toBe(
      'BqoNCkYacAqKtKpZswHbDQtSK8eHGq15NBd9nYq28TJH'
    );
  });

  it('exchanges SOL to get stSOL', async () => {
    const solToExchange = new Lamports(1 * LAMPORTS_PER_SOL);

    const stSolReceived = exchangeSol(snapshotDump.solido, solToExchange);

    expect(stSolReceived.stLamports.toString()).toBe('976765814');
  });

  it('exchanges stSOL to get SOL', async () => {
    const stSolToExchange = new StLamports('976765815');

    const solReceived = exchangeStSol(snapshotDump.solido, stSolToExchange);

    expect(solReceived.lamports.toString()).toBe('1000000000');
  });
});

///////////////////// Stats ///////////////////

describe('Statistics functions', () => {
  it('gets total value locked', async () => {
    const tvl = await getTotalValueLocked(snapshotDump);

    expect(tvl.lamports.toNumber()).toBe(2015202291354382);
  });

  it('gets total stsol supply', async () => {
    const stSolSupply = getStSolSupply(snapshotDump, 'totalcoins');

    expect(stSolSupply.stLamports.toNumber()).toBe(1967472080845457);
  });

  it('gets circulating stsol supply', async () => {
    const stSolSupply = getStSolSupply(snapshotDump, 'circulating');

    expect(stSolSupply.stLamports.toNumber()).toBe(1967472080845457);
  });

  it('gets exchange rate', async () => {
    const exchangeRate = getExchangeRate(snapshotDump);

    expect(exchangeRate).toBe(1.0237868536694013);
  });
});

////////////////// Instructions //////////////////

describe('Withdraw Instruction', () => {
  let senderAddress = new Keypair().publicKey;
  let stSolAccountAddress = new Keypair().publicKey;
  let stakeAccountAddress = new Keypair().publicKey;

  it('generates withdraw instruction', async () => {
    const withdrawInstruction = await getWithdrawInstruction(
      snapshotDump,
      senderAddress,
      stSolAccountAddress,
      stakeAccountAddress,
      new StLamports('1978200000')
    );
    expect(withdrawInstruction.data).toEqual(withdrawInstructionDump.data);
    expect(JSON.stringify(withdrawInstruction.keys)).toContain(
      JSON.stringify(senderAddress)
    );
    expect(JSON.stringify(withdrawInstruction.keys)).toContain(
      JSON.stringify(stSolAccountAddress)
    );
    expect(JSON.stringify(withdrawInstruction.keys)).toContain(
      JSON.stringify(stakeAccountAddress)
    );
  });

  it('throws error if withdraw amount is less than rent exemption balance', async () => {
    try {
      await getWithdrawInstruction(
        snapshotDump,
        senderAddress,
        stSolAccountAddress,
        stakeAccountAddress,
        new StLamports('900000')
      );
    } catch (error) {
      expect(error.message).toContain('Amount must be greater');
    }
  });

  it('throws error if withdraw amount is greater than max amount', async () => {
    try {
      await getWithdrawInstruction(
        snapshotDump,
        senderAddress,
        stSolAccountAddress,
        stakeAccountAddress,
        new StLamports('19782234987239423400000')
      );
    } catch (error) {
      expect(error.message).toContain('Amount must be less');
    }
  });
});

describe('Deposit Instruction', () => {
  it('generates deposit instrucion', async () => {
    const depositInstruction = await getDepositInstruction(
      new Keypair().publicKey,
      new Keypair().publicKey,
      MAINNET_PROGRAM_ADDRESSES,
      { lamports: new BN('3988300000') }
    );

    expect(depositInstruction.data).toEqual(depositInstructionDump.data);
  });
});
