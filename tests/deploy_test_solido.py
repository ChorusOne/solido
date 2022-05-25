#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
Set up a Solido instance on a local testnet, and print its details. This is
useful when testing the maintenance daemon locally.
"""

import json
import os
from typing import Optional, Dict, Any

from util import (
    create_test_account,
    solana_program_deploy,
    create_spl_token_account,
    create_vote_account,
    get_network,
    solana,
    solido,
    multisig,
    get_approve_and_execute,
    get_solido_program_path,
)

print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy(get_solido_program_path() + '/lido.so')
print(f'> Solido program id is {solido_program_id}')

print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy(
    get_solido_program_path() + '/serum_multisig.so'
)
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
treasury_account = result['treasury_account']
developer_account = result['developer_account']
st_sol_mint_account = result['st_sol_mint_address']

print(f'> Created instance at {solido_address}')

approve_and_execute = get_approve_and_execute(
    multisig_program_id=multisig_program_id,
    multisig_instance=multisig_instance,
    signer_keypair_paths=[maintainer.keypair_path],
)


def add_validator(index: int, vote_account: Optional[str]) -> str:
    """
    Add a validator to the instance, create the right accounts for it. The vote
    account can be a pre-existing one, but if it is not provided, we will create
    one. Returns the vote account address.
    """
    print(f'\nCreating validator {index} ...')
    validator_fee_st_sol_account_owner = create_test_account(
        f'tests/.keys/validator-{index}-fee-st-sol-account.json'
    )
    validator_fee_st_sol_account = create_spl_token_account(
        validator_fee_st_sol_account_owner.keypair_path,
        st_sol_mint_account,
    )
    print(f'> Validator token account owner: {validator_fee_st_sol_account_owner}')
    print(f'> Validator stSOL token account: {validator_fee_st_sol_account}')

    if vote_account is None:
        solido_instance = solido(
            'show-solido',
            '--solido-program-id',
            solido_program_id,
            '--solido-address',
            solido_address,
        )
        validator = create_test_account(f'tests/.keys/validator-{index}-account.json')
        validator_vote_account = create_vote_account(
            f'tests/.keys/validator-{index}-vote-account.json',
            validator.keypair_path,
            solido_instance['rewards_withdraw_authority'],
        )
        vote_account = validator_vote_account.pubkey

    print(f'> Validator vote account:        {vote_account}')

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
        vote_account,
        '--validator-fee-account',
        validator_fee_st_sol_account,
        '--multisig-address',
        multisig_instance,
        keypair_path=maintainer.keypair_path,
    )
    approve_and_execute(transaction_result['transaction_address'])
    return vote_account


# Compares a validator structure
def get_vote_account_withdrawer(vote_account: str) -> str:
    result = solana('vote-account', vote_account, '--output', 'json')
    authorized_withdrawer: str = json.loads(result)['authorizedWithdrawer']
    return authorized_withdrawer


# For the first validator, add the test validator itself, so we include a
# validator that is actually voting, and earning rewards.
current_validators = json.loads(solana('validators', '--output', 'json'))

# If we're running on localhost, change the comission to 100% and withdrawer
# address to the Solido's rewards withdraw authority.
if get_network() == 'http://127.0.0.1:8899':
    solido_instance = solido(
        'show-solido',
        '--solido-program-id',
        solido_program_id,
        '--solido-address',
        solido_address,
    )
    print(f'> Changing validator\'s comission to 100% ...')
    validator = current_validators['validators'][0]
    validator['commission'] = '100'
    solana(
        'vote-update-commission',
        validator['voteAccountPubkey'],
        '100',
        './test-ledger/vote-account-keypair.json',
    )
    print(f'> Changing validator\'s withdrawer to Solido\'s ...')
    solana(
        'vote-authorize-withdrawer',
        validator['voteAccountPubkey'],
        './test-ledger/vote-account-keypair.json',
        solido_instance['rewards_withdraw_authority'],
    )


# Allow only validators that are voting, have 100% commission, and have their
# withdrawer set to Solido's rewards withdraw authority. On a local testnet,
# this will only contain the test validator, but on devnet or testnet, there can
# be more validators.
active_validators = [
    v
    for v in current_validators['validators']
    if (not v['delinquent'])
    and v['commission'] == '100'
    and get_vote_account_withdrawer(v['voteAccountPubkey'])
    == solido_instance['rewards_withdraw_authority']
]

# Add up to 5 of the active validators. Locally there will only be one, but on
# the devnet or testnet there can be more, and we don't want to add *all* of them.
validators = [
    add_validator(i, vote_account=v['voteAccountPubkey'])
    for (i, v) in enumerate(active_validators[:5])
]

# Create two validators of our own, so we have a more interesting stake
# distribution. These validators are not running, so they will not earn
# rewards.
validators.extend(
    add_validator(i, vote_account=None)
    for i in range(len(validators), len(validators) + 2)
)


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
print(f'  Reserve address:          {solido_instance["reserve_account"]}')
print(f'  Maintainer address:       {maintainer.pubkey}')

for i, vote_account in enumerate(validators):
    print(f'  Validator {i} vote account: {vote_account}')


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
