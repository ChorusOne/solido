import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { AnkerSnapshot } from "../../../types";

export const snapshot: AnkerSnapshot = {
  anker: {
    solido_program_id: new PublicKey(
      "874qdedig9MnSiinBkErWvafQacAfwzkHjHyE6XTa8kg"
    ),
    solido: new PublicKey("EMtjYGwPnXdtqK5SGL8CWGv4wgdBQN79UPoy53x9bBTJ"),
    b_sol_mint: new PublicKey("3FMBoeddUhtqxepzkrxPrMUV3CL4bZM5QmMoLJfEpirz"),
    token_swap_pool: new PublicKey(
      "4q1F4NMNjYmp9kGBSnmGSMPs5BqaPt9j6b3pP6GFTes5"
    ),
    terra_rewards_destination: [
      227, 190, 108, 118, 30, 173, 54, 233, 47, 39, 115, 184, 175, 189, 56, 133,
      79, 21, 187, 146,
    ],
    wormhole_parameters: {
      core_bridge_program_id: new PublicKey(
        "3u8hJUVTA4jH1wYAyUur7FFZVQ8H635K3tSHHF4ssjQ5"
      ),
      token_bridge_program_id: new PublicKey(
        "DZnkkTmCiFWfYTfT41X3Rd1kDgozqzxWaHqsw6W4x2oe"
      ),
    },
    metrics: {
      swapped_rewards_st_sol_total: new BN("475155555"),
      swapped_rewards_ust_total: new BN("27611"),
    },
    self_bump_seed: 254,
    mint_authority_bump_seed: 250,
    reserve_authority_bump_seed: 255,
    st_sol_reserve_account_bump_seed: 255,
    ust_reserve_account_bump_seed: 255,
  },
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
        counts1: new BN("0"),
        counts2: new BN("0"),
        counts3: new BN("1"),
        counts4: new BN("2"),
        counts5: new BN("4"),
        counts6: new BN("4"),
        counts7: new BN("4"),
        counts8: new BN("4"),
        counts9: new BN("4"),
        counts10: new BN("4"),
        counts11: new BN("4"),
        counts12: new BN("4"),
        total: new BN("1610000000"),
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
  stSolReserveAccountBalance: { stLamports: new BN("225401911") },
};
