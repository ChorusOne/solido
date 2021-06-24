#![cfg(feature = "test-bpf")]

use solana_program_test::tokio;

use crate::assert_solido_error;
use crate::context::Context;

use lido::error::LidoError;
use lido::token::{StLamports, Lamports};
use lido::state::ExchangeRate;

#[tokio::test]
async fn test_update_exchange_rate() {
    let mut context = Context::new_with_maintainer().await;

    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let start_slot = epoch_schedule.first_normal_slot;
    let start_epoch = epoch_schedule.first_normal_epoch;
    let slots_per_epoch = epoch_schedule.slots_per_epoch;

    // Move to the next epoch, then update the exchange rate.
    println!("Before warp 1.");
    context.context.warp_to_slot(start_slot).unwrap();
    println!("After warp 1, before update 1.");
    context.update_exchange_rate().await;
    println!("After update 1.");

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
    println!("Before update 2.");
    let result = context.try_update_exchange_rate().await;
    println!("After update 2.");
    assert_solido_error!(result, LidoError::ExchangeRateAlreadyUpToDate);

    // Make a deposit, so something should change next epoch.
    println!("Before deposit.");
    context.deposit(Lamports(100)).await;
    println!("After deposit, before warp 2.");

    context.context.warp_to_slot(start_slot + 1 * slots_per_epoch).unwrap();
    println!("After warp 2, before update 3.");
    context.update_exchange_rate().await;
    println!("After update 3.");

    // There was one deposit, the exchange rate was 1:1, we should now have the
    // same amount of SOL and stSOL.
    let solido = context.get_solido().await;
    assert_eq!(
        solido.exchange_rate,
        ExchangeRate {
            computed_in_epoch: start_epoch + 1,
            st_sol_supply: StLamports(100),
            sol_balance: Lamports(100),
        }
    );
}
