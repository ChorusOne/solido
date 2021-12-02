#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script calls 'solana' and 'solido' to confirm that functionality works.

It exits with exit code 0 if everything works as expected, or with a nonzero
exit code if anything fails. It expects a test validator to be running at at the
default localhost port, and it expects a keypair at ~/.config/solana/id.json
that corresponds to a sufficiently funded account.
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
)

print('Creating test accounts ...')
os.makedirs('tests/.keys', exist_ok=True)
test_addrs = [
    create_test_account('tests/.keys/test-key-1.json'),
    create_test_account('tests/.keys/test-key-2.json'),
]
print(f'> {test_addrs}')

treasury_account_owner = create_test_account('tests/.keys/treasury-key.json')
print(f'> Treasury account owner:      {treasury_account_owner}')

developer_account_owner = create_test_account('tests/.keys/developer-fee-key.json')
print(f'> Developer fee account owner: {developer_account_owner}')


print('\nSetting up UST mint ...')
ust_mint_address = create_test_account('tests/.keys/ust_mint_address.json', fund=False)
spl_token('create-token', 'tests/.keys/ust_mint_address.json')
print(f'> UST mint is {ust_mint_address.pubkey}.')

print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy(
    get_solido_program_path() + '/serum_multisig.so'
)
print(f'> Multisig program id is {multisig_program_id}.')

print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy(get_solido_program_path() + '/lido.so')
print(f'> Solido program id is {solido_program_id}.')

print('\nUploading Anker program ...')
anker_program_id = solana_program_deploy(get_solido_program_path() + '/anker.so')
print(f'> Anker program id is {anker_program_id}.')

print('\nCreating new multisig ...')
multisig_data = multisig(
    'create-multisig',
    '--multisig-program-id',
    multisig_program_id,
    '--threshold',
    '1',
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
    Helper to execute a multisig transaction. We set the threshold to 1 in this
    test (we assume the multisig works correctly, this is tested elsewhere), and
    the proposer already approves, so only executing is left to do.
    """
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
    '4',
    '--validation-fee-share',
    '5',
    '--developer-fee-share',
    '1',
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
st_sol_mint_address = result['st_sol_mint_address']

print(f'> Created instance at {solido_address}.')


print('\nCreating Anker instance ...')
result = solido(
    'create-anker',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--anker-program-id',
    anker_program_id,
    '--ust-mint-address',
    ust_mint_address.pubkey,
    '--token-swap-pool',
    # TODO: Deploy the Orca program on the test validator and set up a test pool.
    ust_mint_address.pubkey,
)
# TODO: Also provide --mint-address, we need to be sure that that one works.
anker_address = result['anker_address']
anker_st_sol_reserve_account = result['st_sol_reserve_account']
anker_ust_reserve_account = result['ust_reserve_account']
b_sol_mint_address = result['b_sol_mint_address']
print(f'> Created instance at {anker_address}.')
