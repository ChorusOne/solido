import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { Snapshot, Lamports, StLamports } from "../../../types";

export const snapshot: Snapshot = {
  solido: {
    lido_version: 0,
    manager: new PublicKey("HUGAB3ufHTi1CJnUaRUADxwYzsDCJzT9zMMTEABZuBL7"),
    st_sol_mint: new PublicKey("H6L2MwgQPVCoyETqFyqiuJgW3reCxFdesnAb579qzX88"),
    exchange_rate: {
      computed_in_epoch: new BN("255"),
      st_sol_supply: new BN("1600000000"),
      sol_balance: new BN("1800000000"),
    },
    sol_reserve_authority_bump_seed: 255,
    stake_authority_bump_seed: 254,
    mint_authority_bump_seed: 252,
    rewards_withdraw_authority_bump_seed: 253,
    reward_distribution: {
      treasury_fee: 5,
      validation_fee: 3,
      developer_fee: 2,
      st_sol_appreciation: 90,
    },
    fee_recipients: {
      treasury_account: new PublicKey(
        "5FWjZ5JUiwzs7wd2vxj4Y7WcdMpQQi2KnvH4iU7LMqRu"
      ),
      developer_account: new PublicKey(
        "H9oWuNrdAdxJT8BiBMrJPGnqEU8xRTk6XcWokPNx7xhH"
      ),
    },
    metrics: {
      fee_treasury_sol_total: new BN("0"),
      fee_validation_sol_total: new BN("0"),
      fee_developer_sol_total: new BN("0"),
      st_sol_appreciation_sol_total: new BN("0"),
      fee_treasury_st_sol_total: new BN("0"),
      fee_validation_st_sol_total: new BN("0"),
      fee_developer_st_sol_total: new BN("0"),
      deposit_amount: {
        counts1: new BN("7"),
        counts2: new BN("7"),
        counts3: new BN("8"),
        counts4: new BN("18"),
        counts5: new BN("25"),
        counts6: new BN("25"),
        counts7: new BN("25"),
        counts8: new BN("25"),
        counts9: new BN("25"),
        counts10: new BN("25"),
        counts11: new BN("25"),
        counts12: new BN("25"),
        total: new BN("6824430000"),
      },
      withdraw_amount: {
        total_st_sol_amount: new BN("0"),
        total_sol_amount: new BN("0"),
        count: new BN("0"),
      },
    },
    validators: { entries: [], maximum_entries: 9 },
    maintainers: {
      entries: [
        {
          pubkey: new PublicKey("7TuGTkphS8xyYJToFEaz4sHowsqh4em8owrGkQXLrcqp"),
          entry: Uint8Array.from([]),
        },
      ],
      maximum_entries: 3,
    },
  },
  programAddresses: {
    solidoProgramId: new PublicKey(
      "874qdedig9MnSiinBkErWvafQacAfwzkHjHyE6XTa8kg"
    ),
    solidoInstanceId: new PublicKey(
      "EMtjYGwPnXdtqK5SGL8CWGv4wgdBQN79UPoy53x9bBTJ"
    ),
    stSolMintAddress: new PublicKey(
      "H6L2MwgQPVCoyETqFyqiuJgW3reCxFdesnAb579qzX88"
    ),
    ankerProgramId: new PublicKey(
      "8MT6MtwbSdNyYH655cDxf2MypYSVfmAdx8jXrBWPREzf"
    ),
    ankerInstanceId: new PublicKey(
      "BovX97d8MnVTbpwbBdyjSrEr7RvxN8AHEk3dYwTEx7RD"
    ),
    bSolMintAddress: new PublicKey(
      "3FMBoeddUhtqxepzkrxPrMUV3CL4bZM5QmMoLJfEpirz"
    ),
  },
  reserveAccountBalance: new Lamports("7024430000"),
  stSolSupply: new StLamports("6243937759"),
  stakeAccountRentExemptionBalance: new Lamports("2282880"),
  validatorsStakeAccounts: [],
};
