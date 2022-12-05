#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script calls 'solana' and 'solido multisig' to go through two flows:

 * Upgrade a program managed by a multisig.
 * Change the owners of a multisig.

It exits with exit code 0 if everything works as expected, or with a nonzero
exit code if anything fails. It expects a test validator to be running at at the
default localhost port, and it expects a keypair at ~/.config/solana/id.json
that corresponds to a sufficiently funded account.
"""

import json
import os.path
import shutil
import subprocess
import sys
import tempfile

from typing import Any, Dict, Optional, NamedTuple

from util import (
    solana,
    create_test_account,
    solana_program_deploy,
    solana_program_show,
    multisig,
    get_solido_program_path,
    spl_token,
)


# We start by generating accounts that we will need later. We put the tests
# keys in a directory where we can .gitignore them, so they don't litter the
# working directory so much.
print('Creating test accounts ...')
os.makedirs('tests/.keys', exist_ok=True)
addr1 = create_test_account('tests/.keys/test-key-1.json')
addr2 = create_test_account('tests/.keys/test-key-2.json')
addr3 = create_test_account('tests/.keys/test-key-3.json')
print(f'> {addr1}')
print(f'> {addr2}')
print(f'> {addr3}')


print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy(
    get_solido_program_path() + '/serum_multisig.so'
)
print(f'> Multisig program id is {multisig_program_id}.')

print('\nCreating new multisig ...')
result = multisig(
    'create-multisig',
    '--multisig-program-id',
    multisig_program_id,
    '--threshold',
    '2',
    '--owners',
    ','.join([addr1.pubkey, addr2.pubkey, addr3.pubkey]),
)
multisig_address = result['multisig_address']
multisig_program_derived_address = result['multisig_program_derived_address']
print(f'> Multisig address is {multisig_address}.')


print('\nUploading v1 of program to upgrade ...')
with tempfile.TemporaryDirectory() as scratch_dir:
    # We reuse the multisig binary for this purpose, but copy it to a different
    # location so 'solana program deploy' doesn't reuse the program id.
    program_fname = os.path.join(scratch_dir, 'program_v1.so')
    shutil.copyfile(get_solido_program_path() + '/serum_multisig.so', program_fname)
    program_id = solana_program_deploy(program_fname)
    print(f'> Program id is {program_id}.')

    # Change the owner of the program to the multisig derived address. Although
    # 'solana program deploy' sports an '--upgrade-authority' option, using that
    # does not actually set the upgrade authority on deploy, so we do it in a
    # separate step.
    solana(
        'program',
        'set-upgrade-authority',
        '--new-upgrade-authority',
        multisig_program_derived_address,
        program_id,
    )

    upload_info = solana_program_show(program_id)
    print(f'> Program was uploaded in slot {upload_info.last_deploy_slot}.')
    assert upload_info.upgrade_authority == multisig_program_derived_address

    print('\nUploading v2 of program to buffer ...')
    program_fname = os.path.join(scratch_dir, 'program_v2.so')
    shutil.copyfile(get_solido_program_path() + '/serum_multisig.so', program_fname)
    result = solana(
        'program',
        'write-buffer',
        '--output',
        'json',
        '--buffer-authority',
        multisig_program_derived_address,
        program_fname,
    )
    buffer_address = json.loads(result)['buffer']

    # Same for the buffer authority, it must be equal to the upgrade authority
    # of the program to upgrade, but the '--buffer-authority' argument of
    # 'solana write-buffer' does not work for some reason, so we set it after
    # upload instead.
    solana(
        'program',
        'set-buffer-authority',
        '--new-buffer-authority',
        multisig_program_derived_address,
        buffer_address,
    )
    print(f'> Program was uploaded to buffer {buffer_address}.')
    # Exit the scope, clean up the temporary directory.


# Confirm that we are unable to upgrade the program directly, only the multisig
# derived address should be able to.
print('\nAttempting direct upgrade, which should fail ...')
try:
    solana('program', 'deploy', '--program-id', program_id, '--buffer', buffer_address)
except subprocess.CalledProcessError as err:
    assert err.returncode == 1
    new_info = solana_program_show(program_id)
    assert new_info == upload_info, 'Program should not have changed.'
    print('> Deploy failed as expected.')
else:
    print('> Deploy succeeded even though it should not have.')
    sys.exit(1)


print('\nProposing program upgrade ...')
result = multisig(
    'propose-upgrade',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--program-address',
    program_id,
    '--buffer-address',
    buffer_address,
    '--spill-address',
    addr1.pubkey,
    keypair_path=addr1.keypair_path,
)
upgrade_transaction_address = result['transaction_address']
print(f'> Transaction address is {upgrade_transaction_address}.')


# Confirm that only the proposer signed the transaction at this point, and that
# it is the upgrade transaction that we intended.
result = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    program_id,
    '--transaction-address',
    upgrade_transaction_address,
)
assert result['did_execute'] == False

assert 'BpfLoaderUpgrade' in result['parsed_instruction']
assert result['parsed_instruction']['BpfLoaderUpgrade'] == {
    'program_to_upgrade': program_id,
    'program_data_address': upload_info.program_data_address,
    'buffer_address': buffer_address,
    'spill_address': addr1.pubkey,
}
assert result['signers']['Current']['signers'] == [
    {'owner': addr1.pubkey, 'did_sign': True},
    {'owner': addr2.pubkey, 'did_sign': False},
    {'owner': addr3.pubkey, 'did_sign': False},
]


print('\nTrying to execute with 1 of 2 signatures, which should fail ...')
try:
    multisig(
        'execute-transaction',
        '--multisig-program-id',
        multisig_program_id,
        '--multisig-address',
        multisig_address,
        '--transaction-address',
        upgrade_transaction_address,
    )
except subprocess.CalledProcessError as err:
    assert err.returncode != 0
    assert 'Not enough owners signed this transaction' in err.stdout
    new_info = solana_program_show(program_id)
    assert new_info == upload_info, 'Program should not have changed.'
    print('> Execution failed as expected.')
else:
    print('> Execution succeeded even though it should not have.')
    sys.exit(1)


print('\nApproving transaction from a second account ...')
result = multisig(
    'approve',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--transaction-address',
    upgrade_transaction_address,
    keypair_path=addr2.keypair_path,
)
assert result['num_approvals'][0] == 2
assert result['threshold'] == 2

result = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    program_id,
    '--transaction-address',
    upgrade_transaction_address,
)
assert result['signers']['Current']['signers'] == [
    {'owner': addr1.pubkey, 'did_sign': True},
    {'owner': addr2.pubkey, 'did_sign': True},
    {'owner': addr3.pubkey, 'did_sign': False},
]
print(f'> Transaction is now signed by {addr2} as well.')


print('\nTrying to execute with 2 of 2 signatures, which should succeed ...')
result = multisig(
    'execute-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--transaction-address',
    upgrade_transaction_address,
)
assert 'transaction_id' in result
result = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    program_id,
    '--transaction-address',
    upgrade_transaction_address,
)
assert result['did_execute'] == True
print('> Transaction is marked as executed.')

upgrade_info = solana_program_show(program_id)
assert upgrade_info.last_deploy_slot > upload_info.last_deploy_slot
print(f'> Program was upgraded in slot {upgrade_info.last_deploy_slot}.')


print('\nTrying to execute a second time, which should fail ...')
try:
    multisig(
        'execute-transaction',
        '--multisig-program-id',
        multisig_program_id,
        '--multisig-address',
        multisig_address,
        '--transaction-address',
        upgrade_transaction_address,
    )
except subprocess.CalledProcessError as err:
    assert err.returncode != 0
    assert 'The given transaction has already been executed.' in err.stdout
    new_info = solana_program_show(program_id)
    assert new_info == upgrade_info, 'Program should not have changed.'
    print('> Execution failed as expected.')
else:
    print('> Execution succeeded even though it should not have.')
    sys.exit(1)


# Next we are going to test changing the multisig. Before we go and do that,
# confirm that it currently looks like we expect it to look.
multisig_before = multisig(
    'show-multisig',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
)
assert multisig_before == {
    'multisig_program_derived_address': multisig_program_derived_address,
    'threshold': 2,
    'owners': [addr1.pubkey, addr2.pubkey, addr3.pubkey],
}


print('\nProposing to remove the third owner from the multisig ...')
# This time we omit the third owner. The threshold remains 2.
result = multisig(
    'propose-change-multisig',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--threshold',
    '2',
    '--owners',
    ','.join([addr1.pubkey, addr2.pubkey]),
    keypair_path=addr1.keypair_path,
)
change_multisig_transaction_address = result['transaction_address']
print(f'> Transaction address is {change_multisig_transaction_address}.')


print('\nApproving transaction from a second account ...')
result = multisig(
    'approve',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--transaction-address',
    change_multisig_transaction_address,
    keypair_path=addr3.keypair_path,
)
assert result['num_approvals'][0] == 2
assert result['threshold'] == 2

result = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    program_id,
    '--transaction-address',
    change_multisig_transaction_address,
)
assert result['signers']['Current']['signers'] == [
    {'owner': addr1.pubkey, 'did_sign': True},
    {'owner': addr2.pubkey, 'did_sign': False},
    {'owner': addr3.pubkey, 'did_sign': True},
]
assert result['parsed_instruction'] == {
    'MultisigChange': {
        'old_threshold': 2,
        'new_threshold': 2,
        'old_owners': [addr1.pubkey, addr2.pubkey, addr3.pubkey],
        'new_owners': [addr1.pubkey, addr2.pubkey],
    }
}
print('> Transaction has the required number of signatures.')


print('\nExecuting multisig change transaction ...')
result = multisig(
    'execute-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--transaction-address',
    change_multisig_transaction_address,
)
assert 'transaction_id' in result
result = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    program_id,
    '--transaction-address',
    change_multisig_transaction_address,
)
assert result['did_execute'] == True
print('> Transaction is marked as executed.')

multisig_after = multisig(
    'show-multisig',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
)
assert multisig_after == {
    'multisig_program_derived_address': multisig_program_derived_address,
    'threshold': 2,
    'owners': [addr1.pubkey, addr2.pubkey],
}
print(f'> The third owner was removed.')


print('\nChecking that the old transaction does not show outdated owner info ...')
result = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    program_id,
    '--transaction-address',
    upgrade_transaction_address,
)
assert 'Outdated' in result['signers']
assert result['signers']['Outdated'] == {'num_signed': 2, 'num_owners': 3}
print('> Owners ids are gone, but approval count is preserved as expected.')


# Next we will propose a final program upgrade, to confirm that the third owner
# is no longer allowed to approve.
print('\nProposing new program upgrade ...')
result = multisig(
    'propose-upgrade',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--program-address',
    program_id,
    '--buffer-address',
    buffer_address,
    '--spill-address',
    addr1.pubkey,
    keypair_path=addr1.keypair_path,
)
upgrade_transaction_address = result['transaction_address']
print(f'> Transaction address is {upgrade_transaction_address}.')


print('\nApproving this transaction from owner 3, which should fail ...')
try:
    multisig(
        'approve',
        '--multisig-program-id',
        multisig_program_id,
        '--multisig-address',
        multisig_address,
        '--transaction-address',
        upgrade_transaction_address,
        keypair_path=addr3.keypair_path,
    )
except subprocess.CalledProcessError as err:
    assert err.returncode != 0
    assert 'The given owner is not part of this multisig.' in err.stdout
    result = multisig(
        'show-transaction',
        '--multisig-program-id',
        multisig_program_id,
        '--solido-program-id',
        program_id,
        '--transaction-address',
        upgrade_transaction_address,
    )
    assert result['signers']['Current']['signers'] == [
        {'owner': addr1.pubkey, 'did_sign': True},
        {'owner': addr2.pubkey, 'did_sign': False},
    ]
    print('> Approve failed as expected.')
else:
    print('> Approve succeeded even though it should not have.')
    sys.exit(1)


test_token = create_test_account('tests/.keys/test-token.json', fund=False)
test_token_account_1 = create_test_account(
    'tests/.keys/test-token-account-1.json', fund=False
)
test_token_account_2 = create_test_account(
    'tests/.keys/test-token-account-2.json', fund=False
)

spl_token('create-token', test_token.keypair_path)
spl_token(
    'create-account',
    test_token.pubkey,
    test_token_account_1.keypair_path,
    '--owner',
    multisig_program_derived_address,
)
print(f'\nTesting transferring token from mint {test_token} ...')

spl_token('create-account', test_token.pubkey, test_token_account_2.keypair_path)
spl_token('mint', test_token.pubkey, '100', test_token_account_1.pubkey)
print(
    f'> Testing transfering 10 tokens from {test_token_account_1} to {test_token_account_2}.'
)
result = multisig(
    'token-transfer',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--from-address',
    test_token_account_1.pubkey,
    '--to-address',
    test_token_account_2.pubkey,
    '--amount',
    '10',
    keypair_path=addr1.keypair_path,
)

token_transfer_transaction_address = result['transaction_address']
multisig(
    'approve',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--transaction-address',
    token_transfer_transaction_address,
    keypair_path=addr2.keypair_path,
)

multisig(
    'execute-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--multisig-address',
    multisig_address,
    '--transaction-address',
    token_transfer_transaction_address,
)

result = multisig(
    'show-transaction',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    program_id,
    '--transaction-address',
    token_transfer_transaction_address,
)
assert result['parsed_instruction']['TokenInstruction']['Transfer'] == {
    'from_address': test_token_account_1.pubkey,
    'to_address': test_token_account_2.pubkey,
    'token_address': test_token.pubkey,
    'amount': 10,
}

token_balance = spl_token('balance', '--address', test_token_account_2.pubkey)
assert float(token_balance) * 1e9 == 10.0
print(f'> Successfully transferred tokens.')
