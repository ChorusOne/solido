import { PublicKey } from '@solana/web3.js';
import BN from 'bn.js';
import { Snapshot } from '../../types';

export const snapshot: Snapshot = {
  solido: {
    lido_version: 0,
    manager: new PublicKey(
      new BN(
        '7003585ceaf54c89ac039fa0f2bb390729c289e416b346306bf277ed4561c6e4',
        'hex',
        'le'
      )
    ),
    st_sol_mint: new PublicKey(
      new BN(
        'fedef235d5df2a94343d6348287dbfc85f31ffc815d800ce9d6b471971cb7162',
        'hex',
        'le'
      )
    ),
    exchange_rate: {
      computed_in_epoch: new BN('0100', 'hex'),
      st_sol_supply: new BN('35e06bf954dd5', 'hex'),
      sol_balance: new BN('36a84a87b4459', 'hex'),
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
        new BN(
          'b072aee0d5953b6f5a66f1baa1aa0c9cb2022c21dc92f2149251023fe5097ab',
          'hex',
          'le'
        )
      ),
      developer_account: new PublicKey(
        new BN(
          '94305608df7d3d3020c27308db0d6f4388dc42a4ccbfa4b17a6efcd8d76d547b',
          'hex',
          'le'
        )
      ),
    },
    metrics: {
      fee_treasury_sol_total: new BN('8e09b8fe66', 'hex'),
      fee_validation_sol_total: new BN('b18c27327e', 'hex'),
      fee_developer_sol_total: new BN('23826e3ef1', 'hex'),
      st_sol_appreciation_sol_total: new BN('c7bdac27e46', 'hex'),
      fee_treasury_st_sol_total: new BN('8cf08ecc74', 'hex'),
      fee_validation_st_sol_total: new BN('b02cb268f8', 'hex'),
      fee_developer_st_sol_total: new BN('233c23b1c1', 'hex'),
      deposit_amount: {
        counts1: new BN('4', 'hex'),
        counts2: new BN('22', 'hex'),
        counts3: new BN('ba', 'hex'),
        counts4: new BN('2b6', 'hex'),
        counts5: new BN('705', 'hex'),
        counts6: new BN('efa', 'hex'),
        counts7: new BN('1603', 'hex'),
        counts8: new BN('1851', 'hex'),
        counts9: new BN('18ca', 'hex'),
        counts10: new BN('18da', 'hex'),
        counts11: new BN('18db', 'hex'),
        counts12: new BN('18db', 'hex'),
        total: new BN('57f2f83749a26', 'hex'),
      },
      withdraw_amount: {
        total_st_sol_amount: new BN('2217bd992b408', 'hex'),
        total_sol_amount: new BN('225399aa6130e', 'hex'),
        count: new BN('664', 'hex'),
      },
    },
    validators: {
      entries: [
        {
          pubkey: new PublicKey(
            new BN(
              'a45d0cbb9f9c090a33a663b3d8dc6280231833e93db6fc745dc98ad86fe5971d',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                '6aca2223203f6b2c265441b3b820e0489b744330a4adcf577fc8e32af349666d',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1b', 'hex'),
              end: new BN('1c', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3e4cd0147238', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              'bfec43604f265202427fa37caa2239731c014cf9af15e588e504e23f394ea503',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                'abf0811b21cdecd826f954b037988129a374de307590b6e11b5dc32066d1d4bf',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1a', 'hex'),
              end: new BN('1b', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3f0f34781db1', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              '6fae01035d995b2f8950f4dc5a37d0bb305893e3bc8968035c027ff3058f4aa1',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                '490cff067021b75bfc0c40148c2b37c9af64575a5e2885dc11df17e0dc1d0ad5',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1a', 'hex'),
              end: new BN('1b', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3f0ed4b6a470', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              'efaffe2906fb0c5fc6225a9b98c49288cce11794cf9c1ed19ea2f5cc0f617806',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                '0ba977e4013ff7c9f5d755438a0e2889b14ba615660996a5a9775ee0ffe3bf7b',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1b', 'hex'),
              end: new BN('1c', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3ef81b56ce6c', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              'da643b1ab31cd6b0e66a5c0f3e1d1b2cda75072d0b18b5f994e829c1941c0d05',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                'b4d30d419e497b14c76be2d21c2338650169553ef53ef415cf296d1d880c43db',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1f', 'hex'),
              end: new BN('20', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3c28ccef652c', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              'b8623e2fb0e569733308f0c3f956bc636582ee650bd24fcea5ac23a8b387642a',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                '18070a776df74203ba66ae0c626b3b054fa0604df809976983bd198cd3e7153d',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1c', 'hex'),
              end: new BN('1d', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3f0fc46b1ee4', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              '22fc161c6f0f27d0dfefb211253f9779e8f7fd348d905e609b276a82b5cf329c',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                '927bde1a6c9160c2fac1c94250dced79f98df9a4dbee10f7e5e6a2ef79105a09',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('19', 'hex'),
              end: new BN('1a', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3e8b2e25c5be', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              '95bcfe94705cdcacc869f5a9d6af962552c89b15d0a032cd76c275bed2580c0c',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                'a94476ffa6e0b658f8a138907dfa3d97f646b6f6910c00ca5d20ac423a3e1c4f',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('18', 'hex'),
              end: new BN('19', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3953afe18793', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              '73b2fc057a4e7bbe36c11c771fe3b2ce3f7527365b16bafc2df9307c31ff3457',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                '95321c022ead4871477ad90e518b978927dcec463e15c38ce428b181b0b42ae9',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1a', 'hex'),
              end: new BN('1b', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3d3759ff6a49', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              'd72cf1f6c8eb927a2ce93df5122327b3228e3336ceaa2eb0adc2686d7ca4aefb',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                'bda4fb6061f3f506f123241ec16d31e4fa3e0e77d533cd806c58f20ce330077e',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1d', 'hex'),
              end: new BN('1e', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3eb0deab6560', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              '78360931d8d9f54fbb9edc47185fea36efd27954706cbdd12b6571bd905d015e',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                'c7d4d12e1ef5c274242674a00a973988a8ac9e01d01523f5237c71ab6d0035d5',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('1b', 'hex'),
              end: new BN('1c', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3e911bb505fd', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              '3678026e23aafd4a4a5dc54e6b527afee87d5937b0b0c54bde7b0c27a5d74f22',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                'd81f52646c4e917e226c0c80e33a9df6acb258954d3ccfc3fa1316de86ac056c',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('17', 'hex'),
              end: new BN('18', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3de039250386', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              '5386ea6e2c80db96a40d53f0d88ff161ed04a70b445c7462132f0569ea9751b4',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                'f3b27c26e8deed6971c08ed5cf62c3bb0d3ff80bd6cc6a645086bd1f3c954f16',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('16', 'hex'),
              end: new BN('17', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3e27ad9f086a', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
        {
          pubkey: new PublicKey(
            new BN(
              'fa259a9f5fc30f04baf739836fd4df629fff47558c84922d64434fa27e2c15a1',
              'hex',
              'le'
            )
          ),
          entry: {
            fee_credit: new BN('00', 'hex'),
            fee_address: new PublicKey(
              new BN(
                '714d44fc64e9a705bfb1b738470c0607f13ca279d9a459523e6cce50227f6ce8',
                'hex',
                'le'
              )
            ),
            stake_seeds: {
              begin: new BN('15', 'hex'),
              end: new BN('16', 'hex'),
            },
            unstake_seeds: {
              begin: new BN('00', 'hex'),
              end: new BN('00', 'hex'),
            },
            stake_accounts_balance: new BN('3ea0ee0096f1', 'hex'),
            unstake_accounts_balance: new BN('00', 'hex'),
            active: 1,
          },
        },
      ],
      maximum_entries: 60,
    },
    maintainers: {
      entries: [
        {
          pubkey: new PublicKey(
            new BN(
              'cc0fec5fcfb189f40e0749efaff3345da6233fb789333a972df08685ae30e68b',
              'hex',
              'le'
            )
          ),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey(
            new BN(
              'fda15a510561d963b8e35c8ffb597a2caa7a349bfdbabf00c6b9242b5ecf9f1b',
              'hex',
              'le'
            )
          ),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey(
            new BN(
              '67ddbee54effb7905e8d31fa1e4c8e117c2d199f8e8bb0160baa3b15d831a5be',
              'hex',
              'le'
            )
          ),
          entry: Uint8Array.from([]),
        },
        {
          pubkey: new PublicKey(
            new BN(
              '0b5462427661aaf9c398f892b2da938c814da81ca3e55006a7b56e39575c0c0c',
              'hex',
              'le'
            )
          ),
          entry: Uint8Array.from([]),
        },
      ],
      maximum_entries: 10,
    },
  },
  reserveAccountBalance: { lamports: new BN('04384ebf2886', 'hex') },
  stSolSupply: { stLamports: new BN('035af8b1be9932', 'hex') },
  stakeAccountRentExemptionBalance: { lamports: new BN('22d580', 'hex') },
  validatorsStakeAccounts: [
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '1d97e56fd88ac95d74fcb63de93318238062dcd8b363a6330a099c9fbb0c5da4',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          'd19e565dfefc7f0f63a3a59e54a0953932b62e54ba2ac3689a09518b338bbdcb',
          'hex'
        )
      ),
      balance: { lamports: new BN('3e4cd0147238', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '03a54e393fe204e588e515aff94c011c733922aa7ca37f420252264f6043ecbf',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '02127f91d62c00b5bc4d385e71f2b4a909759ff487da7fbd5edeb63e22782036',
          'hex'
        )
      ),
      balance: { lamports: new BN('3f0f34781db1', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          'a14a8f05f37f025c036889bce3935830bbd0375adcf450892f5b995d0301ae6f',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '5b79c457183f75b4b3417dec7c2cef435156f5dbc6524b8aff1c44df13818188',
          'hex'
        )
      ),
      balance: { lamports: new BN('3f0ed4b6a470', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '0678610fccf5a29ed11e9ccf9417e1cc8892c4989b5a22c65f0cfb0629feafef',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          'b6dcd2523aed25b14584c2486aff90e35341819ddfdf6cebf7d15fb67d914bac',
          'hex'
        )
      ),
      balance: { lamports: new BN('3ef81b56ce6c', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '050d1c94c129e894f9b5180b2d0775da2c1b1d3e0f5c6ae6b0d61cb31a3b64da',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '1aa1080c6fe662125d89607c0975e08c11c6c7dbe1d00af8a0f1dc6e9c0fd4cc',
          'hex'
        )
      ),
      balance: { lamports: new BN('3c28ccef652c', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '2a6487b3a823aca5ce4fd20b65ee826563bc56f9c3f008337369e5b02f3e62b8',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '364a842cefee3562f03127f73b58b57c157b28fa26a9595659ab7f678fe56008',
          'hex'
        )
      ),
      balance: { lamports: new BN('3f0fc46b1ee4', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '9c32cfb5826a279b605e908d34fdf7e879973f2511b2efdfd0270f6f1c16fc22',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '0131918309c32d7014deee08d7546b1f5db88adbcd50d4573dbcf3c75997c9f7',
          'hex'
        )
      ),
      balance: { lamports: new BN('3e8b2e25c5be', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '0c0c58d2be75c276cd32a0d0159bc8522596afd6a9f569c8acdc5c7094febc95',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '8190402bc2a064de6236463c5a896dea648ff34db362b0ce634d2d847c03d542',
          'hex'
        )
      ),
      balance: { lamports: new BN('3953afe18793', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '5734ff317c30f92dfcba165b3627753fceb2e31f771cc136be7b4e7a05fcb273',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '8af8463fd8ae4f089dd9175a43a8abfffa7451d1a2a9474e32e59a063383811f',
          'hex'
        )
      ),
      balance: { lamports: new BN('3d3759ff6a49', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          'fbaea47c6d68c2adb02eaace36338e22b3272312f53de92c7a92ebc8f6f12cd7',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '6a11439cdea587aa12219b4b4738e65d7bf201ebabc38a5221e4f18d63420970',
          'hex'
        )
      ),
      balance: { lamports: new BN('3eb0deab6560', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '5e015d90bd71652bd1bd6c705479d2ef36ea5f1847dc9ebb4ff5d9d831093678',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          'f3d0613a79914199e1fc73ddfdabfb7bd3ff55097e14305e3e02647d1a834ae8',
          'hex'
        )
      ),
      balance: { lamports: new BN('3e911bb505fd', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          '224fd7a5270c7bde4bc5b0b037597de8fe7a526b4ec55d4a4afdaa236e027836',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '0886a5cfc47d07df718da74dd33ea0f5f2bcc38fb319e3e98a761410d7088c48',
          'hex'
        )
      ),
      balance: { lamports: new BN('3de039250386', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          'b45197ea69052f1362745c440ba704ed61f18fd8f0530da496db802c6eea8653',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          'f42a0c19561bea24a9a17811085daf34afb56be465c2953f117f46bf43c2d072',
          'hex'
        )
      ),
      balance: { lamports: new BN('3e27ad9f086a', 'hex') },
    },
    {
      validatorVoteAddress: new PublicKey(
        new BN(
          'a1152c7ea24f43642d92848c5547ff9f62dfd46f8339f7ba040fc35f9f9a25fa',
          'hex'
        )
      ),
      address: new PublicKey(
        new BN(
          '49c7675679f64eee558473fec8c1f573360bc2a42ffc52ecaaee3dbee786a504',
          'hex'
        )
      ),
      balance: { lamports: new BN('3ea0ee0096f1', 'hex') },
    },
  ],
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
};
