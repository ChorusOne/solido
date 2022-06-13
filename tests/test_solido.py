#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script calls 'solana' and 'solido' to confirm that functionality works.

It exits with exit code 0 if everything works as expected, or with a nonzero
exit code if anything fails. It expects a test validator to be running at at the
default localhost port, and it expects a keypair at ~/.config/solana/id.json
that corresponds to a sufficiently funded account.

If TEST_LEDGER environment variable is set, it will use the ledger as a signing
key-pair, as in `TEST_LEDGER=true ./tests/test_solido.py`
"""
import os
import json

from util import (
    TestAccount,
    create_spl_token_account,
    create_test_account,
    create_vote_account,
    get_solido_program_path,
    multisig,
    solana,
    solana_program_deploy,
    solido,
    spl_token,
    MAX_VALIDATION_FEE,
)

from typing import Any, Dict, NamedTuple, Tuple

# We start by generating an account that we will need later. We put the tests
# keys in a directory where we can .gitignore them, so they don't litter the
# working directory so much.
print('Creating test accounts ...')
os.makedirs('tests/.keys', exist_ok=True)
test_addrs = [create_test_account('tests/.keys/test-key-1.json')]

# If testing with ledger, add the ledger account.
if os.getenv('TEST_LEDGER') != None:
    test_ledger = True
    ledger_address = solana('--keypair', 'usb://ledger', 'address').split()[0]
    solana('transfer', '--allow-unfunded-recipient', ledger_address, '100.0')
    test_addrs.append(TestAccount(ledger_address, 'usb://ledger'))
# Otherwise, generate another one from key-pair file.
else:
    test_addrs.append(create_test_account('tests/.keys/test-key-2.json'))
print(f'> {test_addrs}')

treasury_account_owner = create_test_account('tests/.keys/treasury-key.json')
print(f'> Treasury account owner:      {treasury_account_owner}')

developer_account_owner = create_test_account('tests/.keys/developer-fee-key.json')
print(f'> Developer fee account owner: {developer_account_owner}')

print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy(get_solido_program_path() + '/lido.so')
print(f'> Solido program id is {solido_program_id}.')

print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy(
    get_solido_program_path() + '/serum_multisig.so'
)
print(f'> Multisig program id is {multisig_program_id}.')

print('\nCreating new multisig ...')
multisig_data = multisig(
    'create-multisig',
    '--multisig-program-id',
    multisig_program_id,
    '--threshold',
    '2',
    '--owners',
    ','.join(t.pubkey for t in test_addrs),
)
multisig_instance = multisig_data['multisig_address']
multisig_pda = multisig_data['multisig_program_derived_address']
print(f'> Created instance at {multisig_instance}.')


def approve_and_execute(
    transaction_to_approve: str,
    signer: TestAccount,
) -> None:
    """
    Helper to approve and execute a transaction with a single key.
    """
    multisig(
        'approve',
        '--multisig-program-id',
        multisig_program_id,
        '--multisig-address',
        multisig_instance,
        '--transaction-address',
        transaction_to_approve,
        keypair_path=signer.keypair_path,
    )
    multisig(
        'execute-transaction',
        '--multisig-program-id',
        multisig_program_id,
        '--multisig-address',
        multisig_instance,
        '--transaction-address',
        transaction_to_approve,
        keypair_path=signer.keypair_path,
    )


# Test creating a solido instance with a known minter.
solido_test_account = create_test_account('tests/.keys/solido_address.json', fund=False)
authorities = solido(
    'show-authorities',
    '--solido-address',
    solido_test_account.pubkey,
    '--solido-program-id',
    solido_program_id,
)

mint_address = create_test_account('tests/.keys/mint_address.json', fund=False)
spl_token('create-token', 'tests/.keys/mint_address.json')
# Test changing the mint authority.
spl_token('authorize', mint_address.pubkey, 'mint', authorities['mint_authority'])
print('\nCreating Solido instance with a known solido and minter address...')
result = solido(
    'create-solido',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--max-validators',
    '9',
    '--max-maintainers',
    '1',
    '--max-validation-fee',
    str(MAX_VALIDATION_FEE),
    '--treasury-fee-share',
    '5',
    '--developer-fee-share',
    '2',
    '--st-sol-appreciation-share',
    '93',
    '--treasury-account-owner',
    treasury_account_owner.pubkey,
    '--developer-account-owner',
    developer_account_owner.pubkey,
    '--multisig-address',
    multisig_instance,
    '--solido-key-path',
    solido_test_account.keypair_path,
    '--mint-address',
    mint_address.pubkey,
    keypair_path=test_addrs[0].keypair_path,
)
# The previously created instance is not used throughout the test, and it's
# done to test creating an instance with a separate mint.

print('\nCreating Solido instance ...')
result = solido(
    'create-solido',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--max-validators',
    '9',
    '--max-maintainers',
    '1',
    '--max-validation-fee',
    str(MAX_VALIDATION_FEE),
    '--treasury-fee-share',
    '5',
    '--developer-fee-share',
    '2',
    '--st-sol-appreciation-share',
    '93',
    '--treasury-account-owner',
    treasury_account_owner.pubkey,
    '--developer-account-owner',
    developer_account_owner.pubkey,
    '--multisig-address',
    multisig_instance,
    keypair_path=test_addrs[0].keypair_path,
)

solido_address = result['solido_address']
treasury_account = result['treasury_account']
developer_account = result['developer_account']
st_sol_mint_account = result['st_sol_mint_address']

print(f'> Created instance at {solido_address}.')

solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)
assert solido_instance['solido']['manager'] == multisig_pda
assert solido_instance['solido']['exchange_rate'] == {
    'computed_in_epoch': 0,
    'st_sol_supply': 0,
    'sol_balance': 0,
}
assert solido_instance['solido']['reward_distribution'] == {
    'treasury_fee': 5,
    'developer_fee': 2,
    'st_sol_appreciation': 93,
}


class Validator(NamedTuple):
    account: TestAccount
    vote_account: TestAccount
    withdrawer_account: TestAccount


def add_validator(
    keypath_account: str, keypath_vote: str
) -> Tuple[Validator, Dict[str, Any]]:
    print('\nAdding a validator ...')
    account = create_test_account(f'tests/.keys/{keypath_account}.json')
    vote_account, withdrawer_account = create_vote_account(
        f'tests/.keys/{keypath_vote}.json',
        account.keypair_path,
        f'tests/.keys/{keypath_vote}_withdrawer.json',
        MAX_VALIDATION_FEE,
    )
    print(f'> Creating validator vote account {vote_account}')

    validator = Validator(
        account=account,
        vote_account=vote_account,
        withdrawer_account=withdrawer_account,
    )

    transaction_result = solido(
        'add-validator',
        '--multisig-program-id',
        multisig_program_id,
        '--solido-program-id',
        solido_program_id,
        '--solido-address',
        solido_address,
        '--validator-vote-account',
        vote_account.pubkey,
        '--multisig-address',
        multisig_instance,
        keypair_path=test_addrs[1].keypair_path,
    )
    return (validator, transaction_result)


print('> Call function to add validator')
(validator, transaction_result) = add_validator(
    'validator-account-key', 'validator-vote-account-key'
)

transaction_address = transaction_result['transaction_address']
transaction_status = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--transaction-address',
    transaction_address,
)
assert transaction_status['did_execute'] == False
assert (
    transaction_status['signers']['Current']['signers'].count(
        {'owner': test_addrs[1].pubkey, 'did_sign': True}
    )
    == 1
)


approve_and_execute(transaction_address, test_addrs[0])
transaction_status = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--transaction-address',
    transaction_address,
)
assert transaction_status['did_execute'] == True
assert (
    transaction_status['signers']['Current']['signers'].count(
        {'owner': test_addrs[0].pubkey, 'did_sign': True}
    )
    == 1
)


solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)

assert solido_instance['solido']['validators']['entries'][0] == {
    'pubkey': validator.vote_account.pubkey,
    'entry': {
        'stake_seeds': {
            'begin': 0,
            'end': 0,
        },
        'unstake_seeds': {
            'begin': 0,
            'end': 0,
        },
        'stake_accounts_balance': 0,
        'unstake_accounts_balance': 0,
        'active': True,
    },
}, f'Unexpected validator entry, in {json.dumps(solido_instance, indent=True)}'

maintainer = create_test_account('tests/.keys/maintainer-account-key.json')

print(f'\nAdd and remove maintainer ...')
print(f'> Adding maintainer {maintainer}')

transaction_result = solido(
    'add-maintainer',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--maintainer-address',
    maintainer.pubkey,
    '--multisig-address',
    multisig_instance,
    keypair_path=test_addrs[0].keypair_path,
)
transaction_address = transaction_result['transaction_address']
approve_and_execute(transaction_address, test_addrs[1])

solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)
assert solido_instance['solido']['maintainers']['entries'][0] == {
    'pubkey': maintainer.pubkey,
    'entry': None,
}

print(f'> Removing maintainer {maintainer}')
transaction_result = solido(
    'remove-maintainer',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--maintainer-address',
    maintainer.pubkey,
    '--multisig-address',
    multisig_instance,
    keypair_path=test_addrs[1].keypair_path,
)
transaction_address = transaction_result['transaction_address']
approve_and_execute(transaction_address, test_addrs[0])
solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)

assert len(solido_instance['solido']['maintainers']['entries']) == 0

print(f'> Adding maintainer {maintainer} again')
transaction_result = solido(
    'add-maintainer',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--maintainer-address',
    maintainer.pubkey,
    '--multisig-address',
    multisig_instance,
    keypair_path=test_addrs[1].keypair_path,
)
transaction_address = transaction_result['transaction_address']
approve_and_execute(transaction_address, test_addrs[0])

current_epoch = int(solana('epoch'))


def perform_maintenance() -> Any:
    return solido(
        'perform-maintenance',
        '--solido-address',
        solido_address,
        '--solido-program-id',
        solido_program_id,
        '--stake-time',
        'anytime',
        keypair_path=maintainer.keypair_path,
    )


print('\nRunning maintenance (should be no-op if epoch is unchanged) ...')
result = perform_maintenance()
if solido_instance['solido']['exchange_rate']['computed_in_epoch'] == current_epoch:
    assert result is None, f'Huh, perform-maintenance performed {result}'
    print('> There was nothing to do, as expected.')
else:
    update_exchange_rate_result = 'UpdateExchangeRate'
    # Epoch is likely to be > 0 for the test-net runs
    assert (
        result == update_exchange_rate_result
    ), f'\nExpected: {update_exchange_rate_result}\nActual:   {result}'
    print('> Updated the exchange rate, as expected in a change of Epoch.')


def deposit(lamports: int, expect_created_token_account: bool = False) -> None:
    print(f'\nDepositing {lamports/1_000_000_000} SOL ...')
    deposit_result = solido(
        'deposit',
        '--solido-address',
        solido_address,
        '--solido-program-id',
        solido_program_id,
        '--amount-sol',
        f'{lamports / 1_000_000_000}',
    )
    # The recipient address depends on the signer, it does not have a fixed expectation.
    del deposit_result['recipient']
    expected = {
        'expected_st_lamports': lamports,
        'st_lamports_balance_increase': lamports,
        'created_associated_st_sol_account': expect_created_token_account,
    }
    assert deposit_result == expected, f'{deposit_result} == {expected}'
    print(
        f'> Got {deposit_result["st_lamports_balance_increase"]/1_000_000_000} stSOL.'
    )


deposit(lamports=3_000_000_000, expect_created_token_account=True)

print('\nRunning maintenance ...')
result = perform_maintenance()
expected_result = {
    'StakeDeposit': {
        'validator_vote_account': validator.vote_account.pubkey,
        'amount_lamports': int(3.0e9),
    }
}
stake_account_address = result['StakeDeposit']['stake_account']
del result['StakeDeposit'][
    'stake_account'
]  # This one we can't easily predict, don't compare it.
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'
print(f'> Staked deposit with {validator.vote_account}.')

print(
    '\nSimulating 0.0005 SOL deposit (too little to stake), then running maintenance ...'
)
deposit(lamports=500_000)

# 0.0005 SOL is not enough to make a stake account, so even though the reserve
# is not empty, we can't stake what's in the reserve.
result = perform_maintenance()
assert result is None, f'Huh, perform-maintenance performed {result}'
print('> There was nothing to do, as expected.')

# Adding another validator
print('\nAdd another validator')
(validator_1, transaction_result) = add_validator(
    'validator-account-key-1',
    'validator-vote-account-key-1',
)

transaction_address = transaction_result['transaction_address']
transaction_status = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--transaction-address',
    transaction_address,
)
approve_and_execute(transaction_address, test_addrs[0])

# Should unstake 1/2 (1.5 - 0.0005/2 Sol) of the validator's balance.
result = perform_maintenance()

# V0:       3
# V1:       0
# Reserve:  0.0005

del result['UnstakeFromActiveValidator']['from_stake_account']
del result['UnstakeFromActiveValidator']['to_unstake_account']
expected_result = {
    'UnstakeFromActiveValidator': {
        'validator_vote_account': validator.vote_account.pubkey,
        'from_stake_seed': 0,
        'to_unstake_seed': 0,
        'amount': 1_499_750_000,  # 1.5 Sol - 0.0005 / 2
    }
}
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'

# By donating to the stake account, we trigger maintenance to run WithdrawInactiveStake.
print(
    f'\nDonating to stake account {stake_account_address}, then running maintenance ...'
)
solana('transfer', stake_account_address, '0.1')

result = perform_maintenance()
assert 'WithdrawInactiveStake' in result
expected_result = {
    'WithdrawInactiveStake': {
        'validator_vote_account': validator.vote_account.pubkey,
        'expected_difference_stake_lamports': 100_000_000,  # We donated 0.1 SOL.
        'unstake_withdrawn_to_reserve_lamports': 1_499_750_000,  # Amount that was unstaked for the newcomming validator.
    }
}

# V0:       1.500250000
# V1:       0 Sol
# Reserve:  1.600250000 (1.500250000 + 0.1)

assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'

print('> Performed WithdrawInactiveStake as expected.')


print('\nDonating 1.0 SOL to reserve, then running maintenance ...')
reserve_account: str = solido_instance['reserve_account']
solana('transfer', '--allow-unfunded-recipient', reserve_account, '1.0')
print(f'> Funded reserve {reserve_account} with 1.0 SOL')

# V0:       1.500250000
# V1:       0 Sol
# Reserve:  2.600250000 (1.500250000 + 0.1 + 1)

print('\nRunning maintenance ...')

result = perform_maintenance()

del result['StakeDeposit']['stake_account']
expected_result = {
    'StakeDeposit': {
        'validator_vote_account': validator_1.vote_account.pubkey,
        'amount_lamports': 2_050_250_000,  # (1.500250000 + 2.600250000) / 2
    }
}
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'
print('> Deposited to the second validator, as expected.')

print('\nRunning maintenance (should be no-op) ...')
result = perform_maintenance()

assert result is None, f'Huh, perform-maintenance performed {result}'
print('> There was nothing to do, as expected.')

print(f'\nDeactivating validator {validator.vote_account.pubkey} ...')
transaction_result = solido(
    'deactivate-validator',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_instance,
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--validator-vote-account',
    validator.vote_account.pubkey,
    keypair_path=test_addrs[0].keypair_path,
)
transaction_address = transaction_result['transaction_address']
print(f'> Deactivation multisig transaction address is {transaction_address}.')
transaction_status = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--transaction-address',
    transaction_address,
)
assert (
    'DeactivateValidator'
    in transaction_status['parsed_instruction']['SolidoInstruction']
)
approve_and_execute(transaction_address, test_addrs[1])

solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)
assert not solido_instance['solido']['validators']['entries'][0]['entry'][
    'active'
], 'Validator should be inactive after deactivation.'
print('> Validator is inactive as expected.')

print('\nRunning maintenance (should unstake from inactive validator) ...')
result = perform_maintenance()

del result['UnstakeFromInactiveValidator']['from_stake_account']
del result['UnstakeFromInactiveValidator']['to_unstake_account']
expected_result = {
    'UnstakeFromInactiveValidator': {
        'validator_vote_account': validator.vote_account.pubkey,
        'from_stake_seed': 0,
        'to_unstake_seed': 1,
        'amount': 1_500_250_000,
    }
}
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'

solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)
# Should have bumped the validator's `stake_seeds` and `unstake_seeds`.
val = solido_instance['solido']['validators']['entries'][0]['entry']
assert val['stake_seeds'] == {'begin': 1, 'end': 1}
assert val['unstake_seeds'] == {'begin': 1, 'end': 2}


print('\nRunning maintenance (should withdraw from validator\'s unstake account) ...')
result = perform_maintenance()
expected_result = {
    'WithdrawInactiveStake': {
        'validator_vote_account': validator.vote_account.pubkey,
        'expected_difference_stake_lamports': 0,
        'unstake_withdrawn_to_reserve_lamports': 1_500_250_000,
    }
}
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'

print('\nRunning maintenance (should stake deposit to the second validator) ...')
result = perform_maintenance()
del result['StakeDeposit']['stake_account']
expected_result = {
    'StakeDeposit': {
        'validator_vote_account': validator_1.vote_account.pubkey,
        'amount_lamports': 4100500000,
    }
}

print('\nRunning maintenance (should remove the validator) ...')
result = perform_maintenance()
expected_result = {
    'RemoveValidator': {
        'validator_vote_account': validator.vote_account.pubkey,
    }
}
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'

solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)
number_validators = len(solido_instance['solido']['validators']['entries'])
assert (
    number_validators == 1
), f'\nExpected no validators\nGot: {number_validators} validators'

# change validator commission above limit
solana(
    "vote-update-commission",
    validator_1.vote_account.pubkey,
    str(MAX_VALIDATION_FEE + 1),  # exceed maximum allowed limit
    validator_1.withdrawer_account.keypair_path,
)

print(
    '\nRunning maintenance (should deactivate a validator that exceed max validation fee) ...'
)
result = perform_maintenance()
# check validator_1 is deactivated
expected_result = {
    'DeactivateValidatorIfCommissionExceedsMax': {
        'validator_vote_account': validator_1.vote_account.pubkey
    }
}
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'
