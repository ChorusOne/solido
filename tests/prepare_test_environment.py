#!/usr/bin/env python3

"""
Set up a Solido instance on a local testnet, and print its details. This is
useful when testing the maintenance daemon locally.
"""

import os
from util import (
    create_test_account,
    solana_program_deploy,
    create_spl_token,
    create_vote_account,
    get_solido,
    get_multisig,
    get_network,
    solana,
)

print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy('target/deploy/lido.so')
print(f'> Solido program id is {solido_program_id}')

print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy('target/deploy/multisig.so')
print(f'> Multisig program id is {multisig_program_id}')

multisig = get_multisig(multisig_program_id)
solido = get_solido(multisig_program_id)

os.makedirs('tests/.keys', exist_ok=True)
maintainer = create_test_account('tests/.keys/maintainer.json')
owner = create_test_account('tests/.keys/owner.json')

print('\nCreating new multisig ...')
multisig_data = multisig(
    'create-multisig',
    '--threshold',
    '1',
    '--owner',
    maintainer.pubkey,
)
multisig_instance = multisig_data['multisig_address']
multisig_pda = multisig_data['multisig_program_derived_address']
print(f'> Created instance at {multisig_instance}')

print('\nCreating Solido instance ...')
result = solido(
    'create-solido',
    '--solido-program-id',
    solido_program_id,
    '--fee-numerator',
    '10',
    '--fee-denominator',
    '100',
    '--max-validators',
    '3',
    '--max-maintainers',
    '1',
    '--treasury-fee',
    '5',
    '--validation-fee',
    '3',
    '--developer-fee',
    '2',
    '--treasury-account-owner',
    owner.pubkey,
    '--developer-account-owner',
    owner.pubkey,
    '--multisig-address',
    multisig_instance,
    keypair_path=maintainer.keypair_path,
)

solido_address = result['solido_address']
treasury_account = result['treasury_account']
developer_account = result['developer_account']
st_sol_mint_account = result['st_sol_mint_address']

print(f'> Created instance at {solido_address}')


print('\nAdding a validator ...')
validator_fee_st_sol_account_owner = create_test_account(
    'tests/.keys/validator-token-account-key.json'
)
validator_fee_st_sol_account = create_spl_token(
    validator_fee_st_sol_account_owner.keypair_path,
    st_sol_mint_account,
)
print(f'> Validator token account owner: {validator_fee_st_sol_account_owner}')
print(f'> Validator stSOL token account: {validator_fee_st_sol_account}')

validator = create_test_account('tests/.keys/validator-account-key.json')
validator_vote_account = create_vote_account(
    'tests/.keys/validator-vote-account-key.json', validator.keypair_path
)
print(f'> Validator vote account: {validator_vote_account}')


def approve_and_execute(transaction_address: str) -> None:
    multisig(
        'approve',
        '--multisig-address',
        multisig_instance,
        '--transaction-address',
        transaction_address,
        keypair_path=maintainer.keypair_path,
    )
    multisig(
        'execute-transaction',
        '--multisig-address',
        multisig_instance,
        '--transaction-address',
        transaction_address,
        keypair_path=maintainer.keypair_path,
    )


print('Adding validator ...')
transaction_result = solido(
    'add-validator',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--validator-vote-account',
    validator_vote_account.pubkey,
    '--validator-fee-account',
    validator_fee_st_sol_account,
    '--multisig-address',
    multisig_instance,
    keypair_path=maintainer.keypair_path,
)
approve_and_execute(transaction_result['transaction_address'])


print('Adding maintainer ...')
transaction_result = solido(
    'add-maintainer',
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


solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)
print('\nDetails:')
print(f'  Solido address:          {solido_address}')
print(f'  Reserve address:         {solido_instance["reserve_authority"]}')
print(f'  Validator vote account:  {validator_vote_account.pubkey}')
print(f'  Maintainer address:      {maintainer.pubkey}')

print('\nMaintenance command line:')
print(
    ' ',
    ' '.join(
        [
            'solido',
            '--keypair-path',
            maintainer.keypair_path,
            '--cluster',
            get_network(),
            '--multisig-program-id',
            multisig_program_id,
            'run-maintainer',
            '--solido-program-id',
            solido_program_id,
            '--solido-address',
            solido_address,
            '--max-poll-interval-seconds',
            '10',
        ]
    ),
)
