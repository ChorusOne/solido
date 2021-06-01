//! Logic for keeping the stake pool balanced.

use spl_stake_pool::state::{StakeStatus, ValidatorStakeInfo};
use crate::token::Lamports;
use crate::error::LidoError;

/// Compute the ideal stake balance for each validator.
///
/// The validator order in `target_balance` is the same as in `current_balance`.
///
/// At the moment, this function targets a uniform distribution over all
/// validators. In the future we could do something more sophisticated (such as
/// allocating more stake to faster validators, or ones with a proven track
/// record).
pub fn get_target_balance(
    undelegated_lamports: Lamports,
    current_balance: &[ValidatorStakeInfo],
    target_balance: &mut [Lamports],
) -> Result<(), LidoError> {
    assert_eq!(
        current_balance.len(),
        target_balance.len(),
        "Must have as many target balance outputs as current balance inputs."
    );

    // If there are no validators, there is nothing to divide. Bail out to avoid
    // division by zero below.
    if current_balance.len() == 0 {
        return Ok(())
    }

    let total_delegated_lamports: Option<Lamports> = current_balance.iter().map(
        |stake_info| Lamports(stake_info.stake_lamports)
    ).sum();

    let total_lamports = total_delegated_lamports
        .and_then(|t| t + undelegated_lamports)
        .ok_or(LidoError::CalculationFailure.into())?;

    // We only want to target validators that are not in the process of being
    // removed. Count how many there are.
    let num_active_validators = current_balance.iter().map(
        |stake_info| match stake_info.status {
            StakeStatus::Active => 1,
            StakeStatus::DeactivatingTransient => 0,
            StakeStatus::ReadyForRemoval => 0,
        }).sum();

    // We simply target a uniform distribution.
    let target_balance_per_active_validator = (
        total_lamports / num_active_validators
    ).expect("Does not divide by 0 because we checked previously.");

    for (stake_info, target) in current_balance.iter().zip(target_balance.iter_mut()) {
        match stake_info.status {
            StakeStatus::Active => *target = target_balance_per_active_validator,
            _ => *target = Lamports(0),
        }
    }

    // The total lamports to distribute may be slightly larger than the total
    // lamports we distributed so far, because we round down.
    let total_lamports_distributed = target_balance
        .iter()
        .cloned()
        .sum::<Option<Lamports>>()
        .expect("Does not overflow, is at most total_lamports.");

    let mut remainder = (total_lamports - total_lamports_distributed)
        .expect("Does not underflow because we distribute at most total_lamports.");

    assert!(remainder.0 < current_balance.len() as u64);

    // Distribute the remainder among the first few validators, give them one
    // Lamport each.
    let mut i = 0;
    while remainder > Lamports(0) {
        match current_balance[i].status {
            StakeStatus::Active => {
                target_balance[i] = (target_balance[i] + Lamports(1))
                    .expect("Does not overflow because per-validator balance is at most total_lamports.");
                remainder = (remainder - Lamports(1))
                    .expect("Does not underflow due to loop condition.");
            }
            _ => {}
        }
        i += 1;
    }

    // Sanity check: now we should have distributed all inputs.
    let total_lamports_distributed = target_balance
        .iter()
        .cloned()
        .sum::<Option<Lamports>>()
        .expect("Does not overflow, is at most total_lamports.");
    assert_eq!(total_lamports_distributed, total_lamports);

    Ok(())
}

/// Given a list of validators and their target balance, return the index of the
/// one furthest below its target, and the amount by which it is below.
pub fn get_least_balanced_validator(
    current_balance: &[ValidatorStakeInfo],
    target_balance: &mut [Lamports],
) -> (usize, Lamports) {
    assert_eq!(
        current_balance.len(),
        target_balance.len(),
        "Must have as many target balance outputs as current balance inputs."
    );
    let mut index = 0;
    let mut amount = Lamports(0);

    for (i, (stake_info, target)) in current_balance.iter().zip(target_balance).enumerate() {
        let amount_below = Lamports(target.0.saturating_sub(stake_info.stake_lamports));
        if amount_below > amount {
            amount = amount_below;
            index = i;
        }
    }

    (index, amount)
}

#[cfg(test)]
mod test {
    use spl_stake_pool::state::{StakeStatus, ValidatorStakeInfo};
    use crate::token::Lamports;
    use solana_program::pubkey::Pubkey;
    use super::{get_least_balanced_validator, get_target_balance};

    #[test]
    fn get_target_balance_works_for_single_validator() {
        // 100 Lamports delegated + 50 undelegated => 150 per validator target.
        let validators = [
            ValidatorStakeInfo {
                status: StakeStatus::Active,
                vote_account_address: Pubkey::new_unique(),
                stake_lamports: 100,
                last_update_epoch: 0,
            }
        ];
        let mut targets = [Lamports(0); 1];
        let undelegated_stake = Lamports(50);
        let result = get_target_balance(
            undelegated_stake,
            &validators[..],
            &mut targets[..],
        );
        assert!(result.is_ok());
        assert_eq!(targets[0], Lamports(150));
    }

    #[test]
    fn get_target_balance_works_for_integer_multiple() {
        // 200 Lamports delegated + 50 undelegated => 125 per validator target.
        let validators = [
            ValidatorStakeInfo {
                status: StakeStatus::Active,
                vote_account_address: Pubkey::new_unique(),
                stake_lamports: 101,
                last_update_epoch: 0,
            },
            ValidatorStakeInfo {
                status: StakeStatus::Active,
                vote_account_address: Pubkey::new_unique(),
                stake_lamports: 99,
                last_update_epoch: 0,
            }
        ];
        let mut targets = [Lamports(0); 2];
        let undelegated_stake = Lamports(50);
        let result = get_target_balance(
            undelegated_stake,
            &validators[..],
            &mut targets[..],
        );
        assert!(result.is_ok());
        assert_eq!(targets, [Lamports(125), Lamports(125)]);
    }

    #[test]
    fn get_target_balance_works_for_non_integer_multiple() {
        // 200 Lamports delegated + 51 undelegated => 125 per validator target,
        // and one validator gets 1 more.
        let validators = [
            ValidatorStakeInfo {
                status: StakeStatus::Active,
                vote_account_address: Pubkey::new_unique(),
                stake_lamports: 101,
                last_update_epoch: 0,
            },
            ValidatorStakeInfo {
                status: StakeStatus::Active,
                vote_account_address: Pubkey::new_unique(),
                stake_lamports: 99,
                last_update_epoch: 0,
            }
        ];
        let mut targets = [Lamports(0); 2];
        let undelegated_stake = Lamports(51);
        let result = get_target_balance(
            undelegated_stake,
            &validators[..],
            &mut targets[..],
        );
        assert!(result.is_ok());
        assert_eq!(targets, [Lamports(126), Lamports(125)]);
    }

    #[test]
    fn get_target_balance_avoids_deactivating_validators() {
        // 200 Lamports delegated, but only one active validator,
        // so all of the target should be with that one validator.
        let validators = [
            ValidatorStakeInfo {
                status: StakeStatus::Active,
                vote_account_address: Pubkey::new_unique(),
                stake_lamports: 17,
                last_update_epoch: 0,
            },
            ValidatorStakeInfo {
                status: StakeStatus::DeactivatingTransient,
                vote_account_address: Pubkey::new_unique(),
                stake_lamports: 183,
                last_update_epoch: 0,
            }
        ];
        let mut targets = [Lamports(0); 2];
        let undelegated_stake = Lamports(0);
        let result = get_target_balance(
            undelegated_stake,
            &validators[..],
            &mut targets[..],
        );
        assert!(result.is_ok());
        assert_eq!(targets, [Lamports(200), Lamports(0)]);
    }
}
