import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { Snapshot } from "../../../types";

export const snapshot: Snapshot = {
  solido: {
    lido_version: 0,
    manager: new PublicKey("GQ3QPrB1RHPRr4Reen772WrMZkHcFM4DL5q44x1BBTFm"),
    st_sol_mint: new PublicKey("7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj"),
    exchange_rate: {
      computed_in_epoch: new BN("277"),
      st_sol_supply: new BN("1963604090792835"),
      sol_balance: new BN("2010312053965162"),
    },
    sol_reserve_authority_bump_seed: 255,
    stake_authority_bump_seed: 251,
    mint_authority_bump_seed: 255,
    rewards_withdraw_authority_bump_seed: 254,
    reward_distribution: {
      treasury_fee: 4,
      validation_fee: 5,
      developer_fee: 1,
      st_sol_appreciation: 90,
    },
    fee_recipients: {
      treasury_account: new PublicKey(
        "CYpYPtwY9QVmZsjCmguAud1ctQjXWKpWD7xeL5mnpcXk"
      ),
      developer_account: new PublicKey(
        "9JRpM85Z9Ufrr8itUuHeFTyz9Az6bTeoGwEhyGeQ2CRZ"
      ),
    },
    metrics: {
      fee_treasury_sol_total: new BN("1214162996942"),
      fee_validation_sol_total: new BN("1517703741308"),
      fee_developer_sol_total: new BN("303540748954"),
      st_sol_appreciation_sol_total: new BN("27318667445634"),
      fee_treasury_st_sol_total: new BN("1197580919166"),
      fee_validation_st_sol_total: new BN("1496976139460"),
      fee_developer_st_sol_total: new BN("299395229220"),
      deposit_amount: {
        counts1: new BN("97"),
        counts2: new BN("189"),
        counts3: new BN("407"),
        counts4: new BN("1164"),
        counts5: new BN("3447"),
        counts6: new BN("7860"),
        counts7: new BN("11561"),
        counts8: new BN("12828"),
        counts9: new BN("13061"),
        counts10: new BN("13103"),
        counts11: new BN("13106"),
        counts12: new BN("13106"),
        total: new BN("3132858973579107"),
      },
      withdraw_amount: {
        total_st_sol_amount: new BN("1133700287569540"),
        total_sol_amount: new BN("1148010757157563"),
        count: new BN("2492"),
      },
    },
    validators: {
      entries: [
        {
          pubkey: new PublicKey("2zXALjRWvoyzNdXk456VGNjC9BQa5kLm64tSuiRo9XPH"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "8N3uRYdmGc5GMXXohaqZvH8hD4DNwTXJiouS5NLzryYm"
            ),
            stake_seeds: { begin: new BN("46"), end: new BN("47") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("144416765591178"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("FERNT7FPaRgz18Zc5buW3vKg4YKwvadKVvUDfy7xYNi"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "Duq65YqiXS2aaJnm6Qc5AcS83uQsecj1hpkHnK93ctNe"
            ),
            stake_seeds: { begin: new BN("46"), end: new BN("47") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("143190015405669"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("BrcacASBAeVymsRy9iCWeGrpqawksYBh5B38BAqtFCFQ"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "FLcndw8dpa9kaZWuCxmCQA1kx2Q8w8v7UvrZffeQvhQU"
            ),
            stake_seeds: { begin: new BN("45"), end: new BN("46") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("141458197427196"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("SFundJhhsF9JF3zpbXBVG9ox3bEzKyiVd6n7rXLiW6a"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "9L4re8Rze1ogWJGrz9RhW24Hzb42yqR7zpqgvidfcgBt"
            ),
            stake_seeds: { begin: new BN("46"), end: new BN("47") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("141420554305585"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("LidoSPDw5hiraRkqh2uWTxsvao9AGKHJMthB6YFgqVj"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "FkuatPmLRXEmtBNrMtjfMqvmReLyUpXZKCBawPBRmJAf"
            ),
            stake_seeds: { begin: new BN("51"), end: new BN("52") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("143464153458490"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("3rV2tk7RANbNU88yNUHKtVpKnniUu6V2XVP8w2AH3mdq"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "57TF6KpdbDauqEx7CxMTFyvuosgnfzoxJzeE98zApmWF"
            ),
            stake_seeds: { begin: new BN("47"), end: new BN("48") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("143037369459146"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("BWjYF7ZX8b7Hbj1VHKJVPvdGmxdh59psWotJb7pLBAUZ"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "dWKiREu7XUPLCj1KJU9pAScDvoY14zCxc7y3eYYv5Po"
            ),
            stake_seeds: { begin: new BN("43"), end: new BN("44") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("142034245345210"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("p2p6sw1z5bCjoEtTAVrccc2gCjZm24q7F3SD62TLekk"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "6KpBpCJE75nmbTKvbQL8D4JvUx74GUJYg9Z4Gd2Ur8Yp"
            ),
            stake_seeds: { begin: new BN("43"), end: new BN("44") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("144162884354284"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("6sRLk4b6gLvVNbBPH98SgY8QARUyvPK5gSzrmbAbu1mU"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "GhBmF8SftNBWN3UsRkk4U5CSAoZkMMUeKUPDjG7dmMmn"
            ),
            stake_seeds: { begin: new BN("45"), end: new BN("46") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("144243763515724"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("HwTne32jzXHBqxP6FnNBJpRSRuNPzYDA1KEBMJTtKqCS"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "9UxjKXBW2ufDEmU4KDyj791udaJwkJCxNKkYdAQu9ZYQ"
            ),
            stake_seeds: { begin: new BN("47"), end: new BN("48") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("144444030659696"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("7KxXp5THC8ZhmuPVh7WjMiVAd8yRekHaz52niJGYzLfy"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "FMGiSpY2q8pmECZs32HCuWKiAnpWFZ5tyUr4opbHNFRQ"
            ),
            stake_seeds: { begin: new BN("47"), end: new BN("48") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("144367641725018"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("3JwUXwdCHLazV1oaUY8Yg87eWS8sE7qTpbJ2BZRQBjru"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "8Gg3sE7pvEvdf3PfJAfFm3PBxnWPMSnCHdAWYMBcTjNo"
            ),
            stake_seeds: { begin: new BN("41"), end: new BN("42") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("144443026836686"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("D8tYjtVVe9kkB7MxmDfntqQU4ZCM32kW22bf1mpPKcDY"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "2W6MUCkihfUosuyurg9A9qGEXiw7i4mijLLgSCxnuLti"
            ),
            stake_seeds: { begin: new BN("41"), end: new BN("42") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("144466017710923"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey("BqoNCkYacAqKtKpZswHbDQtSK8eHGq15NBd9nYq28TJH"),
          entry: {
            fee_credit: new BN("0"),
            fee_address: new PublicKey(
              "GeHYNPBartPRbCvH6km7eN3vNDoPZ4ytrfmEs8zPUZgc"
            ),
            stake_seeds: { begin: new BN("40"), end: new BN("41") },
            unstake_seeds: { begin: new BN("0"), end: new BN("0") },
            stake_accounts_balance: new BN("144470020410384"),
            unstake_accounts_balance: new BN("0"),
            active: 1,
          },
        },
      ],
      maximum_entries: 60,
    },
    maintainers: {
      entries: [
        {
          pubkey: new PublicKey("AR7FaVeVvUQwnLtojZNUc42H987KiHqfc4AN1qEwPUJw"),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey("2rqLzNZCBWykEs8bFMbmgqCz4eosaEfU3aRL4RJWdZgQ"),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey("DqCZaFR6cTMvFMuz43HS77Zcz1quR93n11kT1yY6aVf4"),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey("p2pokvNcNc1SFCMoUrp1UBQ6SBET7H5EdLqahz4g55k"),
          entry: Uint8Array.from([]),
        },
      ],
      maximum_entries: 10,
    },
  },
  programAddresses: {
    solidoProgramId: new PublicKey(
      "CrX7kMhLC3cSsXJdT7JDgqrRVWGnUpX3gfEfxxU2NVLi"
    ),
    solidoInstanceId: new PublicKey(
      "49Yi1TKkNyYjPAFdR9LBvoHcUjuPX4Df5T5yv39w2XTn"
    ),
    stSolMintAddress: new PublicKey(
      "7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj"
    ),
    ankerProgramId: new PublicKey("11111111111111111111111111111111"),
    ankerInstanceId: new PublicKey("11111111111111111111111111111111"),
    bSolMintAddress: new PublicKey("11111111111111111111111111111111"),
  },
  reserveAccountBalance: { lamports: new BN("5583605149193") },
  stSolSupply: { stLamports: new BN("1967472080845457") },
  stakeAccountRentExemptionBalance: { lamports: new BN("2282880") },
  validatorsStakeAccounts: [
    {
      validatorVoteAddress: new PublicKey(
        "2zXALjRWvoyzNdXk456VGNjC9BQa5kLm64tSuiRo9XPH"
      ),
      address: new PublicKey("AYKAnKBShqEyz2UMLv7Px5CtuWSHEYS2W1V1GTKGKNwE"),
      balance: { lamports: new BN("144416765591178") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "FERNT7FPaRgz18Zc5buW3vKg4YKwvadKVvUDfy7xYNi"
      ),
      address: new PublicKey("5xurLo7d6c8gL7r6AeAFaDsJ8SSca4WTkTpU2n2xkPwD"),
      balance: { lamports: new BN("143190015405669") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "BrcacASBAeVymsRy9iCWeGrpqawksYBh5B38BAqtFCFQ"
      ),
      address: new PublicKey("EUHvsw8oCzPsQFvL1kbqgSNdnSNedgBp7AzwaeckcagD"),
      balance: { lamports: new BN("141458197427196") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "SFundJhhsF9JF3zpbXBVG9ox3bEzKyiVd6n7rXLiW6a"
      ),
      address: new PublicKey("7X42vDCPh8WDHNwmGPKkocKuiJb2GruL5vZV76paxroU"),
      balance: { lamports: new BN("141420554305585") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "LidoSPDw5hiraRkqh2uWTxsvao9AGKHJMthB6YFgqVj"
      ),
      address: new PublicKey("FYHoErc4rthHUc851meoLyrExUS6hbw3KZtppeRtRDxu"),
      balance: { lamports: new BN("143464153458490") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "3rV2tk7RANbNU88yNUHKtVpKnniUu6V2XVP8w2AH3mdq"
      ),
      address: new PublicKey("2GaDEEts5ShjY36T8YRKkip4YaXgBbVRm9bX986qdE3v"),
      balance: { lamports: new BN("143037369459146") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "BWjYF7ZX8b7Hbj1VHKJVPvdGmxdh59psWotJb7pLBAUZ"
      ),
      address: new PublicKey("HFbC7EjKAWg3gsyM8Aur2dKPCLJ8dgSEkgosAHmyG7VB"),
      balance: { lamports: new BN("142034245345210") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "p2p6sw1z5bCjoEtTAVrccc2gCjZm24q7F3SD62TLekk"
      ),
      address: new PublicKey("EdLQcLo7F7kCTN1TiBgbE9uYtsWPMC7U45s615Pm1meC"),
      balance: { lamports: new BN("144162884354284") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "6sRLk4b6gLvVNbBPH98SgY8QARUyvPK5gSzrmbAbu1mU"
      ),
      address: new PublicKey("d8qojqZk9dnJCkBpSP3vheWhfsgtorbepuda6DvtA6N"),
      balance: { lamports: new BN("144243763515724") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "HwTne32jzXHBqxP6FnNBJpRSRuNPzYDA1KEBMJTtKqCS"
      ),
      address: new PublicKey("5XceDtfYWYFSdzqSX5sRBNPFGpheMhXnT2Toijz46AoP"),
      balance: { lamports: new BN("144444030659696") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "7KxXp5THC8ZhmuPVh7WjMiVAd8yRekHaz52niJGYzLfy"
      ),
      address: new PublicKey("EkfnXxpy8tX1mvejgsGfZoRkNMM6e7LqRrEU9dA57bMp"),
      balance: { lamports: new BN("144367641725018") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "3JwUXwdCHLazV1oaUY8Yg87eWS8sE7qTpbJ2BZRQBjru"
      ),
      address: new PublicKey("DjaoNJUch1qsgnSxZMd7gtVCjYcGBAXj4yno7frz2LYs"),
      balance: { lamports: new BN("144443026836686") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "D8tYjtVVe9kkB7MxmDfntqQU4ZCM32kW22bf1mpPKcDY"
      ),
      address: new PublicKey("CHL5YZA2bBBUEmxsfWD1vtKC7aNd5GsrEQTE3oR4WZWV"),
      balance: { lamports: new BN("144466017710923") },
    },
    {
      validatorVoteAddress: new PublicKey(
        "BqoNCkYacAqKtKpZswHbDQtSK8eHGq15NBd9nYq28TJH"
      ),
      address: new PublicKey("YTQhE2P2NSBUkJtsTMoEonTonYxXLquQ4k6Z1AC1LPQ"),
      balance: { lamports: new BN("144470020410384") },
    },
  ],
};
