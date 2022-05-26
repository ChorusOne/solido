import { PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import { Snapshot } from '../../types';

export const snapshot: Snapshot = {
  solido: {
    lido_version: 0,
    manager: new PublicKey('GQ3QPrB1RHPRr4Reen772WrMZkHcFM4DL5q44x1BBTFm'),
    st_sol_mint: new PublicKey('7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj'),
    exchange_rate: {
      computed_in_epoch: new BN('275'),
      st_sol_supply: new BN('1892971837707973'),
      sol_balance: new BN('1936245653069130'),
    },
    sol_reserve_authority_bump_seed: 255,
    stake_authority_bump_seed: 251,
    mint_authority_bump_seed: 255,
    reward_distribution: {
      treasury_fee: 4,
      validation_fee: 5,
      developer_fee: 1,
      st_sol_appreciation: 90,
    },
    fee_recipients: {
      treasury_account: new PublicKey(
        'CYpYPtwY9QVmZsjCmguAud1ctQjXWKpWD7xeL5mnpcXk'
      ),
      developer_account: new PublicKey(
        '9JRpM85Z9Ufrr8itUuHeFTyz9Az6bTeoGwEhyGeQ2CRZ'
      ),
    },
    metrics: {
      fee_treasury_sol_total: new BN('1132253663863'),
      fee_validation_sol_total: new BN('1415317075136'),
      fee_developer_sol_total: new BN('283063415694'),
      st_sol_appreciation_sol_total: new BN('25475707450866'),
      fee_treasury_st_sol_total: new BN('1117556685051'),
      fee_validation_st_sol_total: new BN('1396945847140'),
      fee_developer_st_sol_total: new BN('279389170710'),
      deposit_amount: {
        counts1: new BN('10'),
        counts2: new BN('59'),
        counts3: new BN('259'),
        counts4: new BN('982'),
        counts5: new BN('3180'),
        counts6: new BN('7419'),
        counts7: new BN('11005'),
        counts8: new BN('12223'),
        counts9: new BN('12448'),
        counts10: new BN('12489'),
        counts11: new BN('12492'),
        counts12: new BN('12492'),
        total: new BN('3065076294603798'),
      },
      withdraw_amount: {
        total_st_sol_amount: new BN('1080465374517203'),
        total_sol_amount: new BN('1093549024078369'),
        count: new BN('2407'),
      },
    },
    validators: {
      entries: [
        {
          pubkey: new PublicKey('2zXALjRWvoyzNdXk456VGNjC9BQa5kLm64tSuiRo9XPH'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              '8N3uRYdmGc5GMXXohaqZvH8hD4DNwTXJiouS5NLzryYm'
            ),
            stake_seeds: { begin: new BN('44'), end: new BN('45') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('138028985326924'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('FERNT7FPaRgz18Zc5buW3vKg4YKwvadKVvUDfy7xYNi'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              'Duq65YqiXS2aaJnm6Qc5AcS83uQsecj1hpkHnK93ctNe'
            ),
            stake_seeds: { begin: new BN('44'), end: new BN('45') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('137005868602482'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('BrcacASBAeVymsRy9iCWeGrpqawksYBh5B38BAqtFCFQ'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              'FLcndw8dpa9kaZWuCxmCQA1kx2Q8w8v7UvrZffeQvhQU'
            ),
            stake_seeds: { begin: new BN('43'), end: new BN('44') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('133456098979568'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('SFundJhhsF9JF3zpbXBVG9ox3bEzKyiVd6n7rXLiW6a'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              '9L4re8Rze1ogWJGrz9RhW24Hzb42yqR7zpqgvidfcgBt'
            ),
            stake_seeds: { begin: new BN('44'), end: new BN('45') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('137404963354336'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('LidoSPDw5hiraRkqh2uWTxsvao9AGKHJMthB6YFgqVj'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              'FkuatPmLRXEmtBNrMtjfMqvmReLyUpXZKCBawPBRmJAf'
            ),
            stake_seeds: { begin: new BN('49'), end: new BN('50') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('138336519623873'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('3rV2tk7RANbNU88yNUHKtVpKnniUu6V2XVP8w2AH3mdq'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              '57TF6KpdbDauqEx7CxMTFyvuosgnfzoxJzeE98zApmWF'
            ),
            stake_seeds: { begin: new BN('45'), end: new BN('46') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('137026973333387'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('BWjYF7ZX8b7Hbj1VHKJVPvdGmxdh59psWotJb7pLBAUZ'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              'dWKiREu7XUPLCj1KJU9pAScDvoY14zCxc7y3eYYv5Po'
            ),
            stake_seeds: { begin: new BN('41'), end: new BN('42') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('138030710908795'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('p2p6sw1z5bCjoEtTAVrccc2gCjZm24q7F3SD62TLekk'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              '6KpBpCJE75nmbTKvbQL8D4JvUx74GUJYg9Z4Gd2Ur8Yp'
            ),
            stake_seeds: { begin: new BN('41'), end: new BN('42') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('137727128497871'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('6sRLk4b6gLvVNbBPH98SgY8QARUyvPK5gSzrmbAbu1mU'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              'GhBmF8SftNBWN3UsRkk4U5CSAoZkMMUeKUPDjG7dmMmn'
            ),
            stake_seeds: { begin: new BN('43'), end: new BN('44') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('138008062931851'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('HwTne32jzXHBqxP6FnNBJpRSRuNPzYDA1KEBMJTtKqCS'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              '9UxjKXBW2ufDEmU4KDyj791udaJwkJCxNKkYdAQu9ZYQ'
            ),
            stake_seeds: { begin: new BN('45'), end: new BN('46') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('138129839548385'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('7KxXp5THC8ZhmuPVh7WjMiVAd8yRekHaz52niJGYzLfy'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              'FMGiSpY2q8pmECZs32HCuWKiAnpWFZ5tyUr4opbHNFRQ'
            ),
            stake_seeds: { begin: new BN('45'), end: new BN('46') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('129270417601653'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('3JwUXwdCHLazV1oaUY8Yg87eWS8sE7qTpbJ2BZRQBjru'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              '8Gg3sE7pvEvdf3PfJAfFm3PBxnWPMSnCHdAWYMBcTjNo'
            ),
            stake_seeds: { begin: new BN('39'), end: new BN('40') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('138076047839105'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('D8tYjtVVe9kkB7MxmDfntqQU4ZCM32kW22bf1mpPKcDY'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              '2W6MUCkihfUosuyurg9A9qGEXiw7i4mijLLgSCxnuLti'
            ),
            stake_seeds: { begin: new BN('39'), end: new BN('40') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('137129138539784'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey('BqoNCkYacAqKtKpZswHbDQtSK8eHGq15NBd9nYq28TJH'),
          entry: {
            fee_credit: new BN('0'),
            fee_address: new PublicKey(
              'GeHYNPBartPRbCvH6km7eN3vNDoPZ4ytrfmEs8zPUZgc'
            ),
            stake_seeds: { begin: new BN('38'), end: new BN('39') },
            unstake_seeds: { begin: new BN('0'), end: new BN('0') },
            stake_accounts_balance: new BN('138335242672963'),
            unstake_accounts_balance: new BN('0'),
            active: 1,
          },
        },
      ],
      maximum_entries: 60,
    },
    maintainers: {
      entries: [
        {
          pubkey: new PublicKey('AR7FaVeVvUQwnLtojZNUc42H987KiHqfc4AN1qEwPUJw'),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey('2rqLzNZCBWykEs8bFMbmgqCz4eosaEfU3aRL4RJWdZgQ'),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey('DqCZaFR6cTMvFMuz43HS77Zcz1quR93n11kT1yY6aVf4'),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey('p2pokvNcNc1SFCMoUrp1UBQ6SBET7H5EdLqahz4g55k'),
          entry: Uint8Array.from([]),
        },
      ],
      maximum_entries: 10,
    },
  },
  programAddresses: {
    solidoProgramId: new PublicKey(
      'CrX7kMhLC3cSsXJdT7JDgqrRVWGnUpX3gfEfxxU2NVLi'
    ),
    solidoInstanceId: new PublicKey(
      '49Yi1TKkNyYjPAFdR9LBvoHcUjuPX4Df5T5yv39w2XTn'
    ),
    stSolMintAddress: new PublicKey(
      '7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj'
    ),
  },
  reserveAccountBalance: { lamports: new BN('83867614370011') },
  stSolSupply: { stLamports: new BN('1954270411879168') },
  stakeAccountRentExemptionBalance: { lamports: new BN('2282880') },
  validatorsStakeAccounts: [
    {
      validatorVoteAddress: new PublicKey(
        '2zXALjRWvoyzNdXk456VGNjC9BQa5kLm64tSuiRo9XPH'
      ),
      address: new PublicKey('2virNsGL9jhynjcF1QA9k19G82iGDZ9jM9wnEyftFi1h'),
      balance: { lamports: new BN('138028985326924') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'FERNT7FPaRgz18Zc5buW3vKg4YKwvadKVvUDfy7xYNi'
      ),
      address: new PublicKey('J5NtT2wS2HRtrsF34K3MXb4cCXicVd1m3BiS2cnx7UH'),
      balance: { lamports: new BN('137005868602482') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'BrcacASBAeVymsRy9iCWeGrpqawksYBh5B38BAqtFCFQ'
      ),
      address: new PublicKey('FxitajEyp7pXKmkHXp3uHGSCiQr6QYrhnVZnSosbscL5'),
      balance: { lamports: new BN('133456098979568') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'SFundJhhsF9JF3zpbXBVG9ox3bEzKyiVd6n7rXLiW6a'
      ),
      address: new PublicKey('3ANzWS2VVhyQPWj1o75m3nUbiv7mxResf6SWshKBfXKN'),
      balance: { lamports: new BN('137404963354336') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'LidoSPDw5hiraRkqh2uWTxsvao9AGKHJMthB6YFgqVj'
      ),
      address: new PublicKey('BWTQVoMZHdDesXdKCJjyDkqMZ51i7Q3CC2foRRB24utf'),
      balance: { lamports: new BN('138336519623873') },
    },
    {
      validatorVoteAddress: new PublicKey(
        '3rV2tk7RANbNU88yNUHKtVpKnniUu6V2XVP8w2AH3mdq'
      ),
      address: new PublicKey('GD3T57xNAXcAbWg16EdEWSdsyG6ban2W4v6SgDAfmPKp'),
      balance: { lamports: new BN('137026973333387') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'BWjYF7ZX8b7Hbj1VHKJVPvdGmxdh59psWotJb7pLBAUZ'
      ),
      address: new PublicKey('AgnQJ6BXSYQfhTndYT65jGX3AkTruj3pqkY2RNa6D9ad'),
      balance: { lamports: new BN('138030710908795') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'p2p6sw1z5bCjoEtTAVrccc2gCjZm24q7F3SD62TLekk'
      ),
      address: new PublicKey('8mwYWbG2tjSMkAeyHDeNvfknkJqXCYTSLfttLcAmzPGT'),
      balance: { lamports: new BN('137727128497871') },
    },
    {
      validatorVoteAddress: new PublicKey(
        '6sRLk4b6gLvVNbBPH98SgY8QARUyvPK5gSzrmbAbu1mU'
      ),
      address: new PublicKey('6gs3ghqfLSxwCd7cCPDKDUZgSuMjATudEujpNwizzdmL'),
      balance: { lamports: new BN('138008062931851') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'HwTne32jzXHBqxP6FnNBJpRSRuNPzYDA1KEBMJTtKqCS'
      ),
      address: new PublicKey('6oGqf6vdPW3rRF8EbvfWdM2ZhygH67xSVkuMJbGCZgj5'),
      balance: { lamports: new BN('138129839548385') },
    },
    {
      validatorVoteAddress: new PublicKey(
        '7KxXp5THC8ZhmuPVh7WjMiVAd8yRekHaz52niJGYzLfy'
      ),
      address: new PublicKey('F5Y9LRBnNxKMAfN99xhUp3ciz91bTvxdaPkgZNPPjV7Z'),
      balance: { lamports: new BN('129270417601653') },
    },
    {
      validatorVoteAddress: new PublicKey(
        '3JwUXwdCHLazV1oaUY8Yg87eWS8sE7qTpbJ2BZRQBjru'
      ),
      address: new PublicKey('DzR5v9ouv41vSFiuq7vME33uvtVd5uXsUPB2gTrVUsVZ'),
      balance: { lamports: new BN('138076047839105') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'D8tYjtVVe9kkB7MxmDfntqQU4ZCM32kW22bf1mpPKcDY'
      ),
      address: new PublicKey('Eo23YKoMLraYtAD8AohcbynFdSBBtep3eDqFRbeeRDde'),
      balance: { lamports: new BN('137129138539784') },
    },
    {
      validatorVoteAddress: new PublicKey(
        'BqoNCkYacAqKtKpZswHbDQtSK8eHGq15NBd9nYq28TJH'
      ),
      address: new PublicKey('D19EcRNJ6yHpocCPiR34UXUnigVFW4s8PiKAht5sthLX'),
      balance: { lamports: new BN('138335242672963') },
    },
  ],
};
