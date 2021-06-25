#![cfg(feature = "test-bpf")]

use solana_program_test::tokio;

use crate::assert_solido_error;
use crate::context::Context;

use lido::error::LidoError;
use lido::state::ExchangeRate;
use lido::token::{Lamports, StLamports};

#[tokio::test]
async fn test_update_exchange_rate() {
    let mut context = Context::new_with_maintainer().await;

    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;
    let start_epoch = epoch_schedule.first_normal_epoch;
    let slots_per_epoch = epoch_schedule.slots_per_epoch;

    // Move to the next epoch, then update the exchange rate.
    context.context.warp_to_slot(start_slot).unwrap();
    context.update_exchange_rate().await;

    // Initially the balance is zero, and we haven't minted any stSOL.
    let solido = context.get_solido().await;
    assert_eq!(
        solido.exchange_rate,
        ExchangeRate {
            computed_in_epoch: start_epoch,
            st_sol_supply: StLamports(0),
            sol_balance: Lamports(0),
        }
    );

    // If we try to update once more in this epoch, that should fail.
    let result = context.try_update_exchange_rate().await;
    assert_solido_error!(result, LidoError::ExchangeRateAlreadyUpToDate);

    // Make a deposit, so something should change next epoch.
    let recipient = context.deposit(Lamports(100_000_000)).await;

    // This is the first deposit, so the exchange rate is 1:1, we should have
    // gotten the same number of stSOL lamports, as we put in in SOL lamports.
    let received_st_sol = context.get_st_sol_balance(recipient).await;
    assert_eq!(received_st_sol, StLamports(100_000_000));

    context
        .context
        .warp_to_slot(start_slot + 1 * slots_per_epoch)
        .unwrap();
    context.update_exchange_rate().await;

    // There was one deposit, the exchange rate was 1:1, we should now have the
    // same amount of SOL and stSOL.
    let solido = context.get_solido().await;
    assert_eq!(
        solido.exchange_rate,
        ExchangeRate {
            computed_in_epoch: start_epoch + 1,
            st_sol_supply: StLamports(100_000_000),
            sol_balance: Lamports(100_000_000),
        }
    );
}
