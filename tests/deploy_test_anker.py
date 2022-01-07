#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
Set up a Solido and Anker instance on Solana devnet or a local testnet, and print
its details. Anker requires a Wormhole deployment for full functionality, which
does exist on devnet, but which is difficult to recreate locally. Everything
aside from sending rewards can be tested locally though.

"""

import json
import os

from typing import Optional, Dict, Any
from uuid import uuid4

from util import (
    create_test_account,
    get_approve_and_execute,
    get_network,
    get_solido_program_path,
    multisig,
    rpc_get_account_info,
    solana_program_deploy,
    solido,
)

DEVNET_ORCA_PROGAM_ID = '3xQ8SWv2GaFXXpHZNqkXsdxq5DZciHBz6ZFoPPfbFd7U'

# Create a fresh directory where we store all the keys and configuration for this
# deployment.
run_id = str(uuid4())[:10]
test_dir = f'tests/.keys/{run_id}'
os.makedirs(test_dir, exist_ok=True)
print(f'Keys directory: {test_dir}')

print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy(
    get_solido_program_path() + '/serum_multisig.so'
)
print(f'> Multisig program id is {multisig_program_id}')

print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy(get_solido_program_path() + '/lido.so')
print(f'> Solido program id is {solido_program_id}')

print('\nUploading Anker program ...')
anker_program_id = solana_program_deploy(get_solido_program_path() + '/anker.so')
print(f'> Anker program id is {anker_program_id}')

# If the Orca program exists, use that, otherwise upload it at a new address.
orca_info = rpc_get_account_info(DEVNET_ORCA_PROGAM_ID)
if orca_info is not None:
    print('\nFound existing instance of Orca Token Swap program.')
    token_swap_program_id = DEVNET_ORCA_PROGAM_ID
else:
    print('\nUploading Orca Token Swap program ...')
    token_swap_program_id = solana_program_deploy(get_solido_program_path() + '/orca_token_swap_v2.so')
print(f'> Token swap program id is {token_swap_program_id}')


maintainer = create_test_account(test_dir + '/maintainer.json')
st_sol_accounts_owner = create_test_account(test_dir + '/st-sol-accounts-owner.json')

print('\nCreating new multisig ...')
multisig_data = multisig(
    'create-multisig',
    '--multisig-program-id',
    multisig_program_id,
    '--threshold',
    '1',
    '--owners',
    maintainer.pubkey,
)
multisig_instance = multisig_data['multisig_address']
multisig_pda = multisig_data['multisig_program_derived_address']
print(f'> Created instance at {multisig_instance}')


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
    '3',
    '--treasury-fee-share',
    '5',
    '--validation-fee-share',
    '3',
    '--developer-fee-share',
    '2',
    '--st-sol-appreciation-share',
    '90',
    '--treasury-account-owner',
    st_sol_accounts_owner.pubkey,
    '--developer-account-owner',
    st_sol_accounts_owner.pubkey,
    '--multisig-address',
    multisig_instance,
    keypair_path=maintainer.keypair_path,
)

solido_address = result['solido_address']
print(f'> Created instance at {solido_address}')

approve_and_execute = get_approve_and_execute(
    multisig_program_id=multisig_program_id,
    multisig_instance=multisig_instance,
    signer_keypair_paths=[maintainer.keypair_path],
)

print('\nAdding maintainer ...')
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
    keypair_path=maintainer.keypair_path,
)
approve_and_execute(transaction_result['transaction_address'])

# Save the configuration to a file, to make it easier to run the maintainer
# and other commands later.
config = {
    'keypair_path': test_dir + '/maintainer.json',
    'cluster': get_network(),
    'multisig_program_id': multisig_program_id,
    'multisig_address': multisig_instance,
    'solido_program_id': solido_program_id,
    'solido_address': solido_address,
    'anker_program_id': anker_program_id,
    'max_poll_interval_seconds': 10,
}
with open(test_dir + '/config.json', 'w', encoding='utf-8') as config_file:
    json.dump(config, config_file, indent=2)


print('\nMaintenance command line:')
print(f'solido --config {test_dir}/config.json run-maintainer')
