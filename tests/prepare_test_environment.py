#!/usr/bin/env python3

"""
Set up a Solido instance on a local testnet, and print its details. This is
useful when testing the maintenance daemon locally.
"""

import os
from util import (
    TestAccount,
    create_test_account,
    solana_program_deploy,
    create_spl_token,
    create_vote_account,
    get_network,
    solido,
    multisig,
)

print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy('target/deploy/lido.so')
print(f'> Solido program id is {solido_program_id}')

print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy('target/deploy/multisig.so')
print(f'> Multisig program id is {multisig_program_id}')

os.makedirs('tests/.keys', exist_ok=True)
maintainer = create_test_account('tests/.keys/maintainer.json')
st_sol_accounts_owner = create_test_account('tests/.keys/st-sol-accounts-owner.json')

print('\nCreating new multisig ...')
multisig_data = multisig(
    'create-multisig',
    '--multisig-program-id',
    multisig_program_id,
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
treasury_account = result['treasury_account']
developer_account = result['developer_account']
st_sol_mint_account = result['st_sol_mint_address']

print(f'> Created instance at {solido_address}')


def approve_and_execute(transaction_address: str) -> None:
    multisig(
        'approve',
        '--multisig-program-id',
        multisig_program_id,
        '--multisig-address',
        multisig_instance,
        '--transaction-address',
        transaction_address,
        keypair_path=maintainer.keypair_path,
    )
    multisig(
        'execute-transaction',
        '--multisig-program-id',
        multisig_program_id,
        '--multisig-address',
        multisig_instance,
        '--transaction-address',
        transaction_address,
        keypair_path=maintainer.keypair_path,
    )


def add_validator(index: int) -> TestAccount:
    print(f'\nCreating validator {index} ...')
    validator_fee_st_sol_account_owner = create_test_account(
        f'tests/.keys/validator-{index}-fee-st-sol-account.json'
    )
    validator_fee_st_sol_account = create_spl_token(
        validator_fee_st_sol_account_owner.keypair_path,
        st_sol_mint_account,
    )
    print(f'> Validator token account owner: {validator_fee_st_sol_account_owner}')
    print(f'> Validator stSOL token account: {validator_fee_st_sol_account}')

    validator = create_test_account(f'tests/.keys/validator-{index}-account.json')
    validator_vote_account = create_vote_account(
        f'tests/.keys/validator-{index}-vote-account.json', validator.keypair_path
    )
    print(f'> Validator vote account:        {validator_vote_account}')

    print('Adding validator ...')
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
        validator_fee_st_sol_account,
        '--multisig-address',
        multisig_instance,
        keypair_path=maintainer.keypair_path,
    )
    approve_and_execute(transaction_result['transaction_address'])
    return validator_vote_account


validators = [add_validator(i) for i in range(3)]

print('Adding maintainer ...')
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


solido_instance = solido(
    'show-solido',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
)
print('\nDetails:')
print(f'  Multisig program id:      {multisig_program_id}')
print(f'  Multisig address:         {multisig_instance}')
print(f'  Solido program id:        {solido_program_id}')
print(f'  Solido address:           {solido_address}')
print(f'  Reserve address:          {solido_instance["reserve_authority"]}')
print(f'  Maintainer address:       {maintainer.pubkey}')

for i, vote_account in enumerate(validators):
    print(f'  Validator {i} vote account: {vote_account.pubkey}')


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
