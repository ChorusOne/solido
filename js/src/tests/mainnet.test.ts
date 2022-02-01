import { Keypair, PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import {
  getDepositInstruction,
  getExchangeRate,
  getSolido,
  getStSolSupply,
  getTotalValueLocked,
  getWithdrawInstruction,
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
} from './data/instructions';
import { snapshot as snapshotDump } from './data/snapshot';
import solidoInstanceInfoDump from './data/solido_instance_info.json';

//////////////// Deserializer ////////////////

describe('Deserializer', () => {
  it('deserializes solido instance from info', () => {
    const solido = getSolido(Buffer.from(solidoInstanceInfoDump.data.data));

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
      new PublicKey('F7GF79UqF5gRJBTPGdzzF2Bne7sMGdDJGxqEBip1aPqC').toString()
    );
  });

  it('finds heaviest validator', async () => {
    const { validatorVoteAddress: heaviestValidator } =
      await getHeaviestValidatorStakeAccount(snapshotDump);

    expect(new PublicKey(heaviestValidator).toString()).toBe(
      '3rV2tk7RANbNU88yNUHKtVpKnniUu6V2XVP8w2AH3mdq'
    );
  });
});

///////////////////// Stats ///////////////////

describe('Statistics functions', () => {
  it('gets total value locked', async () => {
    const tvl = await getTotalValueLocked(snapshotDump);

    expect(tvl.lamports.toNumber()).toBe(958588849714483);
  });

  it('gets total stsol supply', async () => {
    const stSolSupply = await getStSolSupply(snapshotDump, 'totalcoins');

    expect(stSolSupply.stLamports.toNumber()).toBe(944449110579506);
  });

  it('gets circulating stsol supply', async () => {
    const stSolSupply = await getStSolSupply(snapshotDump, 'circulating');

    expect(stSolSupply.stLamports.toNumber()).toBe(944449110579506);
  });

  it('gets exchange rate', async () => {
    const exchangeRate = await getExchangeRate(snapshotDump);

    expect(exchangeRate.solBalance.lamports.toNumber()).toBe(961542925010009);
    expect(exchangeRate.stSolSupply.stLamports.toNumber()).toBe(
      947808007179733
    );
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
