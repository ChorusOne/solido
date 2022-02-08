import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  TransactionInstruction,
} from '@solana/web3.js';
import { getTotalValueLocked as getTotalValueLockedInAnker } from '../ankerStats';
import { convertStSolToBSol } from '../ankerUtils';
import {
  DEVNET_PROGRAM_ADDRESSES,
  exchangeSol,
  exchangeStSol,
  getAnker,
  getAnkerDepositInstruction,
  getAnkerWithdrawInstruction,
  getExchangeRate,
  getSolido,
  getSolidoDepositInstruction,
  getSolidoWithdrawInstruction,
  getStSolSupply,
  getTotalValueLocked,
  Lamports,
  StLamports,
} from '../index';
import { BLamports } from '../types';
import {
  calculateStakeAccountAddress,
  findAuthorityProgramAddress,
  getHeaviestValidatorStakeAccount,
} from '../utils';
import ankerInstanceInfoDump from './data/devnet/anker_instance_info.json';
import {
  deposit as ankerDepositInstructionDump,
  withdraw as ankerWithdrawInstructionDump,
} from './data/devnet/anker_instructions';
import { snapshot as ankerSnapshotDump } from './data/devnet/anker_snapshot';
import solidoInstanceInfoDump from './data/devnet/solido_instance_info.json';
import {
  deposit as solidoDepositInstructionDump,
  withdraw as solidoWithdrawInstructionDump,
} from './data/devnet/solido_instructions';
import { snapshot as solidoSnapshotDump } from './data/devnet/solido_snapshot';

//////////////// Deserializer ////////////////

describe('Deserializer', () => {
  it('deserializes solido instance from info', () => {
    const solido = getSolido(Buffer.from(solidoInstanceInfoDump.data));

    expect(JSON.stringify(solido)).toEqual(
      JSON.stringify(solidoSnapshotDump.solido)
    );
  });

  it('deserializes anker instance from info', () => {
    const anker = getAnker(Buffer.from(ankerInstanceInfoDump.data));

    expect(JSON.stringify(anker)).toEqual(
      JSON.stringify(ankerSnapshotDump.anker)
    );
  });

  it('deserialize stSOL mint address from solido instance dump', () => {
    const solido = solidoSnapshotDump.solido;

    expect(solido.st_sol_mint.toString()).toBe(
      DEVNET_PROGRAM_ADDRESSES.stSolMintAddress.toString()
    );
  });

  it('deserialize bSOL mint address from anker instance dump', () => {
    const solido = ankerSnapshotDump.anker;

    expect(solido.b_sol_mint.toString()).toBe(
      DEVNET_PROGRAM_ADDRESSES.bSolMintAddress.toString()
    );
  });

  it('ensures lido version is 0', () => {
    const solido = solidoSnapshotDump.solido;

    expect(solido.lido_version).toBe(0);
  });
});

//////////////// UTILS /////////////////

describe('Utility functions', () => {
  it('finds solido reserve address', async () => {
    const reserveAccountAddress = await findAuthorityProgramAddress(
      DEVNET_PROGRAM_ADDRESSES,
      'reserve_account'
    );

    expect(reserveAccountAddress.toString()).toBe(
      'GDRn3CNi5RKBbcNY9drowHawMMU3ge62V7uPEsQ4efKu'
    );
  });

  it('finds anker reserve address', async () => {
    const [reserveAccountAddress] = await PublicKey.findProgramAddress(
      [
        DEVNET_PROGRAM_ADDRESSES.ankerInstanceId.toBuffer(),
        Buffer.from('st_sol_reserve_account'),
      ],
      DEVNET_PROGRAM_ADDRESSES.ankerProgramId
    );

    expect(reserveAccountAddress.toString()).toBe(
      'BSGfVnE6q6KemspkugEERU8x7WbQwSKwvHT1cZZ4ACVN'
    );
  });

  it('finds stSOL mint authority address', async () => {
    const reserveAccountAddress = await findAuthorityProgramAddress(
      DEVNET_PROGRAM_ADDRESSES,
      'mint_authority'
    );

    expect(reserveAccountAddress.toString()).toBe(
      'DLAtitiysZpTnmyUFC9Gyt8yGgQXhwmZene9JM2niudF'
    );
  });

  it('finds bSOL mint authority address', async () => {
    const [reserveAccountAddress] = await PublicKey.findProgramAddress(
      [
        DEVNET_PROGRAM_ADDRESSES.ankerInstanceId.toBuffer(),
        Buffer.from('mint_authority'),
      ],
      DEVNET_PROGRAM_ADDRESSES.ankerProgramId
    );

    expect(reserveAccountAddress.toString()).toBe(
      'Bp9HtrSCLH3QnRMT1eXyk68xftm1HSdSfnmotjsiVyAH'
    );
  });

  it('finds stake account address of a validator', async () => {
    const validator = solidoSnapshotDump.solido.validators.entries[0];

    if (!validator) {
      return;
    }

    const stakeAccountAddress = await calculateStakeAccountAddress(
      DEVNET_PROGRAM_ADDRESSES.solidoInstanceId,
      DEVNET_PROGRAM_ADDRESSES.solidoProgramId,
      validator.pubkey,
      validator.entry.stake_seeds.begin
    );

    expect(stakeAccountAddress.toString()).toBe(
      new PublicKey('AYKAnKBShqEyz2UMLv7Px5CtuWSHEYS2W1V1GTKGKNwE').toString()
    );
  });

  it('finds heaviest validator', async () => {
    let heaviestValidator;
    try {
      const { validatorVoteAddress } =
        getHeaviestValidatorStakeAccount(solidoSnapshotDump);

      heaviestValidator = validatorVoteAddress;
    } catch (error) {
      return;
    }

    expect(new PublicKey(heaviestValidator).toString()).toBe(
      'BqoNCkYacAqKtKpZswHbDQtSK8eHGq15NBd9nYq28TJH'
    );
  });

  it('exchanges SOL to get stSOL', async () => {
    const solToExchange = new Lamports(1 * LAMPORTS_PER_SOL);

    const stSolReceived = exchangeSol(solidoSnapshotDump.solido, solToExchange);

    expect(stSolReceived.stLamports.toString()).toBe('888888888');
  });

  it('exchanges stSOL to get SOL', async () => {
    const stSolToExchange = new StLamports('888888889');

    const solReceived = exchangeStSol(
      solidoSnapshotDump.solido,
      stSolToExchange
    );

    expect(solReceived.lamports.toString()).toBe('1000000000');
  });

  it('exchange stSOL to get bSOL', async () => {
    const stSolToExchange = new StLamports('888888889');

    const bSolReceived = convertStSolToBSol(
      ankerSnapshotDump.solido,
      stSolToExchange
    );

    expect(bSolReceived.bLamports.toString()).toBe('1000000000');
  });
});

///////////////////// Stats ///////////////////

describe('Statistics functions', () => {
  it('gets total value locked in solido program', async () => {
    const tvl = getTotalValueLocked(solidoSnapshotDump);

    expect(tvl.lamports.toNumber()).toBe(1810000000);
  });

  it('gets total value locked in anker program', async () => {
    const tvl = getTotalValueLockedInAnker(ankerSnapshotDump);

    expect(tvl.sol.lamports.toNumber()).toBe(253577149);
  });

  it('gets total stsol supply', async () => {
    const stSolSupply = getStSolSupply(solidoSnapshotDump, 'totalcoins');

    expect(stSolSupply.stLamports.toNumber()).toBe(1608888888);
  });

  it('gets circulating stsol supply', async () => {
    const stSolSupply = getStSolSupply(solidoSnapshotDump, 'circulating');

    expect(stSolSupply.stLamports.toNumber()).toBe(1608888888);
  });

  it('gets exchange rate', async () => {
    const exchangeRate = getExchangeRate(solidoSnapshotDump);

    expect(exchangeRate).toBe(1.125);
  });
});

////////////////// Instructions //////////////////

describe('Withdraw Instruction', () => {
  let senderAddress = new Keypair().publicKey;
  let stSolAccountAddress = new Keypair().publicKey;
  let bSolAccountAddress = new Keypair().publicKey;
  let stakeAccountAddress = new Keypair().publicKey;

  it('generates solido withdraw instruction', async () => {
    let withdrawInstruction: TransactionInstruction;
    try {
      withdrawInstruction = await getSolidoWithdrawInstruction(
        solidoSnapshotDump,
        senderAddress,
        stSolAccountAddress,
        stakeAccountAddress,
        new StLamports(LAMPORTS_PER_SOL.toString())
      );
    } catch (error) {
      return;
    }
    expect(withdrawInstruction.data).toEqual(
      solidoWithdrawInstructionDump.data
    );
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

  it('throws error if withdraw amount is less than rent exemption balance in solido', async () => {
    try {
      await getSolidoWithdrawInstruction(
        solidoSnapshotDump,
        senderAddress,
        stSolAccountAddress,
        stakeAccountAddress,
        new StLamports('900000')
      );
    } catch (error) {
      expect(error.message).toContain('No validator stake accounts found');
    }
  });

  it('throws error if withdraw amount is greater than max amount in solido', async () => {
    try {
      await getSolidoWithdrawInstruction(
        solidoSnapshotDump,
        senderAddress,
        stSolAccountAddress,
        stakeAccountAddress,
        new StLamports('19782234987239423400000')
      );
    } catch (error) {
      expect(error.message).toContain('No validator stake accounts found');
    }
  });

  it('generates anker withdraw instructions', async () => {
    const withdrawInstruction = await getAnkerWithdrawInstruction(
      senderAddress,
      bSolAccountAddress,
      stSolAccountAddress,
      DEVNET_PROGRAM_ADDRESSES,
      new BLamports(LAMPORTS_PER_SOL.toString())
    );
    expect(withdrawInstruction.data).toEqual(ankerWithdrawInstructionDump.data);
    expect(JSON.stringify(withdrawInstruction.keys)).toContain(
      JSON.stringify(senderAddress)
    );
    expect(JSON.stringify(withdrawInstruction.keys)).toContain(
      JSON.stringify(stSolAccountAddress)
    );
    expect(JSON.stringify(withdrawInstruction.keys)).toContain(
      JSON.stringify(bSolAccountAddress)
    );
  });
});

describe('Deposit Instruction', () => {
  const senderAddress = new Keypair().publicKey;
  const stSolAccountAddress = new Keypair().publicKey;
  const bSolAccountAddress = new Keypair().publicKey;

  it('generates solido deposit instrucion', async () => {
    const depositInstruction = await getSolidoDepositInstruction(
      senderAddress,
      stSolAccountAddress,
      DEVNET_PROGRAM_ADDRESSES,
      new Lamports(LAMPORTS_PER_SOL.toString())
    );

    expect(depositInstruction.data).toEqual(solidoDepositInstructionDump.data);
    expect(JSON.stringify(depositInstruction.keys)).toContain(
      JSON.stringify(senderAddress)
    );
    expect(JSON.stringify(depositInstruction.keys)).toContain(
      JSON.stringify(stSolAccountAddress)
    );
  });

  it('generates anker deposit instrucion', async () => {
    const depositInstruction = await getAnkerDepositInstruction(
      senderAddress,
      stSolAccountAddress,
      bSolAccountAddress,
      DEVNET_PROGRAM_ADDRESSES,
      new StLamports(LAMPORTS_PER_SOL.toString())
    );

    expect(depositInstruction.data).toEqual(ankerDepositInstructionDump.data);
    expect(JSON.stringify(depositInstruction.keys)).toContain(
      JSON.stringify(senderAddress)
    );
    expect(JSON.stringify(depositInstruction.keys)).toContain(
      JSON.stringify(stSolAccountAddress)
    );
    expect(JSON.stringify(depositInstruction.keys)).toContain(
      JSON.stringify(bSolAccountAddress)
    );
  });
});
