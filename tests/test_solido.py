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
    create_spl_token,
    create_test_account,
    create_vote_account,
    get_solido_program_path,
    multisig,
    solana,
    solana_program_deploy,
    solido,
    spl_token,
)

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
multisig_program_id = solana_program_deploy(get_solido_program_path() + '/multisig.so')
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
    '--treasury-fee-share',
    '5',
    '--validation-fee-share',
    '3',
    '--developer-fee-share',
    '2',
    '--st-sol-appreciation-share',
    '90',
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
    '--treasury-fee-share',
    '5',
    '--validation-fee-share',
    '3',
    '--developer-fee-share',
    '2',
    '--st-sol-appreciation-share',
    '90',
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
    'validation_fee': 3,
    'developer_fee': 2,
    'st_sol_appreciation': 90,
}

print('\nAdding a validator ...')
validator_fee_account_owner = create_test_account(
    'tests/.keys/validator-token-account-key.json'
)
print(f'> Validator token account owner: {validator_fee_account_owner}')

validator = create_test_account('tests/.keys/validator-account-key.json')

validator_vote_account = create_vote_account(
    'tests/.keys/validator-vote-account-key.json',
    validator.keypair_path,
    solido_instance['rewards_withdraw_authority'],
)
print(f'> Creating validator vote account {validator_vote_account}')

print(f'> Creating validator token account with owner {validator_fee_account_owner}')

# Create SPL token
validator_fee_account = create_spl_token(
    'tests/.keys/validator-token-account-key.json', st_sol_mint_account
)
print(f'> Validator stSol token account: {validator_fee_account}')

print('> Call function to add validator')
transaction_result = solido(
    'add-validator',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--validator-vote-account',
    validator_vote_account.pubkey,
    '--validator-fee-account',
    validator_fee_account,
    '--multisig-address',
    multisig_instance,
    keypair_path=test_addrs[1].keypair_path,
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
    'pubkey': validator_vote_account.pubkey,
    'entry': {
        'fee_credit': 0,
        'fee_address': validator_fee_account,
        'stake_accounts_seed_begin': 0,
        'stake_accounts_seed_end': 0,
        'stake_accounts_balance': 0,
        'weight': 2000,
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

print('\nRunning maintenance (should be no-op if epoch is unchanged) ...')
result = solido(
    'perform-maintenance',
    '--solido-address',
    solido_address,
    '--solido-program-id',
    solido_program_id,
    keypair_path=maintainer.keypair_path,
)
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


deposit(lamports=1_000_000_000, expect_created_token_account=True)

print('\nRunning maintenance ...')
result = solido(
    'perform-maintenance',
    '--solido-address',
    solido_address,
    '--solido-program-id',
    solido_program_id,
    keypair_path=maintainer.keypair_path,
)
expected_result = {
    'StakeDeposit': {
        'validator_vote_account': validator_vote_account.pubkey,
        'amount_lamports': int(1.0e9),
    }
}

stake_account_address = result['StakeDeposit']['stake_account']
del result['StakeDeposit'][
    'stake_account'
]  # This one we can't easily predict, don't compare it.
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'
print(f'> Staked deposit with {validator_vote_account}.')

print(
    '\nSimulating 0.0005 SOL deposit (too little to stake), then running maintenance ...'
)
deposit(lamports=500_000)

# 0.0005 SOL is not enough to make a stake account, so even though the reserve
# is not empty, we can't stake what's in the reserve.
result = solido(
    'perform-maintenance',
    '--solido-address',
    solido_address,
    '--solido-program-id',
    solido_program_id,
    keypair_path=maintainer.keypair_path,
)
assert result is None, f'Huh, perform-maintenance performed {result}'
print('> There was nothing to do, as expected.')


# By donating to the stake account, we trigger maintenance to run WithdrawInactiveStake.
print(
    f'\nDonating to stake account {stake_account_address}, then running maintenance ...'
)
solana('transfer', stake_account_address, '0.1')

result = solido(
    'perform-maintenance',
    '--solido-address',
    solido_address,
    '--solido-program-id',
    solido_program_id,
    keypair_path=maintainer.keypair_path,
)
assert 'WithdrawInactiveStake' in result
assert result['WithdrawInactiveStake'] == {
    'validator_vote_account': validator_vote_account.pubkey,
    'expected_difference_lamports': 100_000_000,  # We donated 0.1 SOL.
}

print('> Performed WithdrawInactiveStake as expected.')


print('\nDonating 1.0 SOL to reserve, then running maintenance ...')
reserve_account: str = solido_instance['reserve_account']
solana('transfer', '--allow-unfunded-recipient', reserve_account, '1.0')
print(f'> Funded reserve {reserve_account} with 1.0 SOL')

print('\nRunning maintenance ...')
result = solido(
    'perform-maintenance',
    '--solido-address',
    solido_address,
    '--solido-program-id',
    solido_program_id,
    keypair_path=maintainer.keypair_path,
)
expected_result = {
    'StakeDeposit': {
        'validator_vote_account': validator_vote_account.pubkey,
        # We have 1.0 SOL from the true deposit, and 1.0 donated.
        'amount_lamports': int(2.0e9),
    }
}
print('> Staked as expected.')
