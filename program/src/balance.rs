// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Logic for keeping the stake pool balanced.

use std::ops::Mul;

use crate::account_map::PubkeyAndEntry;
use crate::state::{Validator, Validators};
use crate::{
    error::LidoError,
    token,
    token::{Lamports, Rational},
};

/// Compute the ideal stake balance for each validator.
///
/// The validator order in the result is the same as in `current_balance`.
///
/// This function targets a uniform distribution over all active validators.
pub fn get_target_balance(
    undelegated_lamports: Lamports,
    validators: &Validators,
) -> Result<Vec<Lamports>, LidoError> {
    let total_delegated_lamports: token::Result<Lamports> = validators
        .iter_entries()
        .map(|v| v.stake_accounts_balance)
        .sum();

    let total_lamports = total_delegated_lamports.and_then(|t| t + undelegated_lamports)?;

    // We only want to target validators that are not in the process of being
    // removed.
    let num_active_validators = validators.iter_active().count() as u64;

    // No active validators.
    if num_active_validators == 0 {
        return Err(LidoError::NoActiveValidators);
    }

    let lamports_per_validator = total_lamports
        .mul(Rational {
            numerator: 1,
            denominator: num_active_validators,
        })
        .expect("Does not divide by zero because `num_active_validators != 0`");

    // Target an uniform distribution.
    let mut target_balance: Vec<Lamports> = validators
        .iter_entries()
        .map(|validator| {
            if validator.active {
                lamports_per_validator
            } else {
                Lamports(0)
            }
        })
        .collect();

    // The total lamports to distribute may be slightly larger than the total
    // lamports we distributed so far, because we round down.
    let total_lamports_distributed = target_balance
        .iter()
        .cloned()
        .sum::<token::Result<Lamports>>()
        .expect("Does not overflow, is at most total_lamports.");

    let mut remainder = (total_lamports - total_lamports_distributed)
        .expect("Does not underflow because we distribute at most total_lamports.");

    assert!(remainder.0 < num_active_validators);

    // Distribute the remainder among the first few active validators, give them
    // one Lamport each. This does mean that the validators early in the list
    // are in a more beneficial position because their stake target is one
    // Lamport higher, but to put that number into perspective, the transaction
    // fee per signature is 10k Lamports at the time of writing. Also, there is
    // a minimum amount we can stake, so in practice, validators will never be
    // as close to their target that the one Lamport matters anyway.
    for (target, validator) in target_balance.iter_mut().zip(validators.iter_entries()) {
        if remainder == Lamports(0) {
            break;
        }
        if validator.active {
            *target = (*target + Lamports(1)).expect(
                "Does not overflow because per-validator balance is at most total_lamports.",
            );
            remainder =
                (remainder - Lamports(1)).expect("Does not underflow due to loop condition.");
        }
    }

    // Sanity check: now we should have distributed all inputs.
    let total_lamports_distributed = target_balance
        .iter()
        .cloned()
        .sum::<token::Result<Lamports>>()
        .expect("Does not overflow, is at most total_lamports.");

    assert_eq!(total_lamports_distributed, total_lamports);

    Ok(target_balance)
}

/// Get the index of the validator to unstake from, if we need to unstake at all.

/// If any validator is more than threshold away from its target, this function
/// will try to unstake, and return the index of the validator where unstaking
/// will have the largest impact.
pub fn get_unstake_validator_index(
    validators: &Validators,
    target_balance: &[Lamports],
    threshold: Rational,
) -> Option<(usize, Lamports)> {
    // Check if we need to rebalance because a validator is too far away from
    // its target.
    let needs_unstake =
        validators
            .entries
            .iter()
            .zip(target_balance.iter())
            .any(|(validator, target)| {
                // should't we take a difference by modulus?
                let target_difference = target
                    .0
                    .saturating_sub(validator.entry.effective_stake_balance().0);
                if target == &Lamports(0) {
                    return false;
                }
                Rational {
                    numerator: target_difference,
                    denominator: target.0,
                } >= threshold
            });

    // second iteration on validators, could use just one
    let ((idx, validator), target) = validators
        .entries
        .iter()
        .enumerate()
        .zip(target_balance)
        .max_by_key(|((_idx, validator), target)| {
            validator
                .entry
                .effective_stake_balance()
                .0
                .saturating_sub(target.0)
        })?;

    let amount = validator
        .entry
        .effective_stake_balance()
        .0
        .saturating_sub(target.0);
    let ratio = Rational {
        numerator: amount,
        denominator: target.0,
    };
    if ratio >= threshold || needs_unstake {
        Some((idx, Lamports(amount)))
    } else {
        None
    }
}

// Looks like this function is similar to get_unstake_validator_index() and
// they could be merged in one
/// Given a list of validators and their target balance, return the index of the
/// validator that has less stake, and the amount by which it is below its target.
///
/// This assumes that there is at least one active validator. Panics otherwise.
pub fn get_minimum_stake_validator_index_amount(
    validators: &Validators,
    target_balance: &[Lamports],
) -> (usize, Lamports) {
    assert_eq!(
        validators.len(),
        target_balance.len(),
        "Must have as many target balances as current balances."
    );

    // Our initial index, that will be returned when no validator is below its target,
    // is the first active validator.
    let mut index = validators
        .iter_entries()
        .position(|v| v.active)
        .expect("get_minimum_stake_validator_index_amount requires at least one active validator.");
    let mut lowest_balance = validators.entries[index].entry.effective_stake_balance();
    let mut amount = Lamports(
        target_balance[index]
            .0
            .saturating_sub(validators.entries[index].entry.effective_stake_balance().0),
    );

    for (i, (validator, target)) in validators.iter_entries().zip(target_balance).enumerate() {
        if validator.active && validator.effective_stake_balance() < lowest_balance {
            index = i;
            amount = Lamports(
                target
                    .0
                    .saturating_sub(validator.effective_stake_balance().0),
            );
            lowest_balance = validator.effective_stake_balance();
        }
    }

    (index, amount)
}

pub fn get_validator_to_withdraw(
    validators: &Validators,
) -> Result<&PubkeyAndEntry<Validator>, crate::error::LidoError> {
    validators
        .entries
        .iter()
        .max_by_key(|v| v.entry.effective_stake_balance())
        .ok_or(LidoError::NoActiveValidators)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::state::Validators;
    use crate::token::Lamports;

    #[test]
    fn get_target_balance_works_for_single_validator() {
        // 100 Lamports delegated + 50 undelegated => 150 per validator target.
        let mut validators = Validators::new_fill_default(1);
        validators.entries[0].entry.stake_accounts_balance = Lamports(100);
        let undelegated_stake = Lamports(50);
        let targets = get_target_balance(undelegated_stake, &validators).unwrap();
        assert_eq!(targets[0], Lamports(150));

        // With only one validator, that one is the least balanced. It is
        // missing the 50 undelegated Lamports.
        assert_eq!(
            get_minimum_stake_validator_index_amount(&validators, &targets[..]),
            (0, Lamports(50))
        );
    }

    #[test]
    fn get_target_balance_works_for_integer_multiple() {
        // 200 Lamports delegated + 50 undelegated => 125 per validator target.
        let mut validators = Validators::new_fill_default(2);
        validators.entries[0].entry.stake_accounts_balance = Lamports(101);
        validators.entries[1].entry.stake_accounts_balance = Lamports(99);

        let undelegated_stake = Lamports(50);
        let targets = get_target_balance(undelegated_stake, &validators).unwrap();
        assert_eq!(targets, [Lamports(125), Lamports(125)]);

        // The second validator is further away from its target.
        assert_eq!(
            get_minimum_stake_validator_index_amount(&validators, &targets[..]),
            (1, Lamports(26))
        );
    }

    #[test]
    fn get_target_balance_works_for_non_integer_multiple() {
        // 200 Lamports delegated + 51 undelegated => 125 per validator target,
        // and one validator gets 1 more.
        let mut validators = Validators::new_fill_default(2);
        validators.entries[0].entry.stake_accounts_balance = Lamports(101);
        validators.entries[1].entry.stake_accounts_balance = Lamports(99);

        let undelegated_stake = Lamports(51);
        let targets = get_target_balance(undelegated_stake, &validators).unwrap();
        assert_eq!(targets, [Lamports(126), Lamports(125)]);

        // The second validator is further from its target, by one Lamport.
        assert_eq!(
            get_minimum_stake_validator_index_amount(&validators, &targets[..]),
            (1, Lamports(26))
        );
    }

    #[test]
    fn get_target_balance_already_balanced() {
        let mut validators = Validators::new_fill_default(2);
        validators.entries[0].entry.stake_accounts_balance = Lamports(50);
        validators.entries[1].entry.stake_accounts_balance = Lamports(50);

        let undelegated_stake = Lamports(0);
        let targets = get_target_balance(undelegated_stake, &validators).unwrap();
        assert_eq!(targets, [Lamports(50), Lamports(50)]);

        assert_eq!(
            get_minimum_stake_validator_index_amount(&validators, &targets[..]),
            (0, Lamports(0))
        );
    }
    #[test]
    fn get_target_balance_works_with_inactive_for_non_integer_multiple() {
        let mut validators = Validators::new_fill_default(3);
        validators.entries[0].entry.stake_accounts_balance = Lamports(101);
        validators.entries[1].entry.stake_accounts_balance = Lamports(0);
        validators.entries[1].entry.active = false;
        validators.entries[2].entry.stake_accounts_balance = Lamports(99);

        let undelegated_stake = Lamports(51);
        let targets = get_target_balance(undelegated_stake, &validators).unwrap();
        assert_eq!(targets, [Lamports(126), Lamports(0), Lamports(125)]);

        assert_eq!(
            get_minimum_stake_validator_index_amount(&validators, &targets[..]),
            (2, Lamports(26))
        );
    }

    #[test]
    fn get_target_balance_works_with_inactive_for_integer_multiple() {
        // 500 Lamports delegated, but only two active validators out of three.
        // All target should be divided equally within the active validators.
        let mut validators = Validators::new_fill_default(3);
        validators.entries[0].entry.stake_accounts_balance = Lamports(100);
        validators.entries[1].entry.stake_accounts_balance = Lamports(100);
        validators.entries[1].entry.active = false;
        validators.entries[2].entry.stake_accounts_balance = Lamports(300);

        let undelegated_stake = Lamports(0);
        let targets = get_target_balance(undelegated_stake, &validators).unwrap();
        assert_eq!(targets, [Lamports(250), Lamports(0), Lamports(250)]);

        assert_eq!(
            get_minimum_stake_validator_index_amount(&validators, &targets[..]),
            (0, Lamports(150))
        );
    }

    #[test]
    fn get_target_balance_all_inactive() {
        // No active validators exist.
        let mut validators = Validators::new_fill_default(3);
        validators.entries[0].entry.stake_accounts_balance = Lamports(1);
        validators.entries[1].entry.stake_accounts_balance = Lamports(2);
        validators.entries[2].entry.stake_accounts_balance = Lamports(3);
        validators.entries[0].entry.active = false;
        validators.entries[1].entry.active = false;
        validators.entries[2].entry.active = false;

        let undelegated_stake = Lamports(0);
        let result = get_target_balance(undelegated_stake, &validators);
        assert!(result.is_err());
    }

    #[test]
    fn get_target_balance_no_preference_but_some_inactive() {
        // Every validator is exactly at its target, no validator is below.
        // But the validator furthest below target should still be an active one,
        // not the inactive one.
        let mut validators = Validators::new_fill_default(2);
        validators.entries[0].entry.stake_accounts_balance = Lamports(0);
        validators.entries[1].entry.stake_accounts_balance = Lamports(10);
        validators.entries[0].entry.active = false;

        let undelegated_stake = Lamports(0);
        let targets = get_target_balance(undelegated_stake, &validators).unwrap();
        assert_eq!(
            get_minimum_stake_validator_index_amount(&validators, &targets[..]),
            (1, Lamports(0)),
        );
    }

    #[test]
    fn get_target_balance_works_for_minimum_staked_validator() {
        let mut validators = Validators::new_fill_default(3);
        validators.entries[0].entry.stake_accounts_balance = Lamports(101);
        validators.entries[1].entry.stake_accounts_balance = Lamports(101);
        validators.entries[2].entry.stake_accounts_balance = Lamports(100);

        let undelegated_stake = Lamports(200);
        let targets = get_target_balance(undelegated_stake, &validators).unwrap();
        assert_eq!(targets, [Lamports(168), Lamports(167), Lamports(167)]);

        assert_eq!(
            get_minimum_stake_validator_index_amount(&validators, &targets[..]),
            (2, Lamports(67))
        );
    }

    #[test]
    fn get_unstake_from_active_validator_above_or_equal_threshold() {
        let mut validators = Validators::new_fill_default(3);
        validators.entries[0].entry.stake_accounts_balance = Lamports(10);
        validators.entries[1].entry.stake_accounts_balance = Lamports(16);
        validators.entries[2].entry.stake_accounts_balance = Lamports(10);

        let targets = get_target_balance(Lamports(0), &validators).unwrap();

        let minimum_unstake = get_unstake_validator_index(
            &validators,
            &targets,
            Rational {
                numerator: 1,
                denominator: 4,
            },
        );
        assert_eq!(minimum_unstake, Some((1, Lamports(4))));
        let minimum_unstake = get_unstake_validator_index(
            &validators,
            &targets,
            Rational {
                numerator: 1,
                denominator: 5,
            },
        );
        assert_eq!(minimum_unstake, Some((1, Lamports(4))));
    }

    #[test]
    fn get_unstake_from_active_validator_below_threshold() {
        let mut validators = Validators::new_fill_default(3);
        validators.entries[0].entry.stake_accounts_balance = Lamports(10);
        validators.entries[1].entry.stake_accounts_balance = Lamports(16);
        validators.entries[2].entry.stake_accounts_balance = Lamports(10);

        let targets = get_target_balance(Lamports(0), &validators).unwrap();

        // Test below the threshold.
        let minimum_unstake = get_unstake_validator_index(
            &validators,
            &targets,
            Rational {
                numerator: 1,
                denominator: 2,
            },
        );
        assert_eq!(minimum_unstake, None);
    }

    #[test]
    fn get_unstake_from_active_validator_because_another_needs_stake() {
        let mut validators = Validators::new_fill_default(3);
        validators.entries[0].entry.stake_accounts_balance = Lamports(17);
        validators.entries[1].entry.stake_accounts_balance = Lamports(15);
        validators.entries[2].entry.stake_accounts_balance = Lamports(0);

        let targets = get_target_balance(Lamports(0), &validators).unwrap();

        // Test get the unstake index even if the validator is not below the threshold but some other is.
        let minimum_unstake = get_unstake_validator_index(
            &validators,
            &targets,
            Rational {
                numerator: 1,
                denominator: 1,
            },
        );
        assert_eq!(minimum_unstake, Some((0, Lamports(6))))
    }
}
