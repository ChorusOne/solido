#!/usr/bin/env python3

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
    create_test_account,
    solana_program_deploy,
    create_spl_token,
    create_vote_account,
    solido,
    multisig,
    solana,
    approve_and_execute,
    TestAccount,
    get_solido_program_path,
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
    '--owner',
    ','.join(t.pubkey for t in test_addrs),
)
multisig_instance = multisig_data['multisig_address']
multisig_pda = multisig_data['multisig_program_derived_address']
print(f'> Created instance at {multisig_instance}.')

print('\nCreating Solido instance ...')
result = solido(
    'create-solido',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--fee-numerator',
    '4',
    '--fee-denominator',
    '31',
    '--max-validators',
    '9',
    '--max-maintainers',
    '1',
    '--treasury-fee',
    '5',
    '--validation-fee',
    '3',
    '--developer-fee',
    '2',
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
assert solido_instance['solido']['st_sol_total_shares'] == 0
assert solido_instance['solido']['fee_distribution'] == {
    'treasury_fee': 5,
    'validation_fee': 3,
    'developer_fee': 2,
}

print('\nAdding a validator ...')
validator_fee_account_owner = create_test_account(
    'tests/.keys/validator-token-account-key.json'
)
print(f'> Validator token account owner: {validator_fee_account_owner}')

validator = create_test_account('tests/.keys/validator-account-key.json')

validator_vote_account = create_vote_account(
    'tests/.keys/validator-vote-account-key.json', validator.keypair_path
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
approve_and_execute(
    multisig,
    multisig_program_id,
    multisig_instance,
    transaction_address,
    test_addrs[0].keypair_path,
)
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
approve_and_execute(
    multisig,
    multisig_program_id,
    multisig_instance,
    transaction_address,
    test_addrs[1].keypair_path,
)

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
approve_and_execute(
    multisig,
    multisig_program_id,
    multisig_instance,
    transaction_address,
    test_addrs[0].keypair_path,
)
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
approve_and_execute(
    multisig,
    multisig_program_id,
    multisig_instance,
    transaction_address,
    test_addrs[0].keypair_path,
)


print('\nRunning maintenance (should be no-op) ...')
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

print('\nSimulating 10 SOL deposit, then running maintenance ...')
# TODO(#154): Perform an actual deposit here.
reserve_authority: str = solido_instance['reserve_authority']
solana('transfer', '--allow-unfunded-recipient', reserve_authority, '10.0')
print(f'> Funded reserve {reserve_authority} with 10.0 SOL')

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
        'amount_lamports': int(10.0e9),
    }
}
assert result == expected_result, f'\nExpected: {expected_result}\nActual:   {result}'
print(f'> Staked deposit with {validator_vote_account}.')

print(
    '\nSimulating 0.0005 SOL deposit (too little to stake), then running maintenance ...'
)
# TODO(#154): Perform an actual deposit here.
solana('transfer', '--allow-unfunded-recipient', reserve_authority, '0.0005')
print(f'> Funded reserve {reserve_authority} with 0.0005 SOL')

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
