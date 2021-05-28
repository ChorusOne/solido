#!/usr/bin/env python3

"""
This script calls 'solana' and 'solido' to confirm that functionality works.

It exits with exit code 0 if everything works as expected, or with a nonzero
exit code if anything fails. It expects a test validator to be running at at the
default localhost port, and it expects a keypair at ~/.config/solana/id.json
that corresponds to a sufficiently funded account.
"""

import json

from typing import Any, Optional

from util import run, solana, create_test_account, solana_program_deploy, solana_program_show


# We start by generating three accounts that we will need later.
print('Creating test accounts ...')
addr1 = create_test_account('test-key-1.json')
print(f'> {addr1}')

treasury_account_owner = create_test_account('treasury-key.json')
print(f'> Treasury account owner:    {treasury_account_owner}')

insurance_account_owner = create_test_account('insurance-key.json')
print(f'> Insurance account owner:   {insurance_account_owner}')

manager_fee_account_owner = create_test_account('manager-fee-key.json')
print(f'> Manager fee account owner: {manager_fee_account_owner}')


print('\nUploading stake pool program ...')
stake_pool_program_id = solana_program_deploy('target/deploy/spl_stake_pool.so')
print(f'> Stake pool program id is {stake_pool_program_id}.')


print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy('target/deploy/lido.so')
print(f'> Solido program id is {solido_program_id}.')


def solido(*args: str, keypair_path: Optional[str] = None) -> Any:
    """
    Run 'solido' against localhost, return its parsed json output.
    """
    output = run(
        'target/debug/solido',
        '--cluster', 'localnet',
        '--output', 'json',
        *([] if keypair_path is None else ['--keypair-path', keypair_path]),
        *args,
    )
    if output == '':
        return {}
    else:
        try:
            return json.loads(output)
        except json.JSONDecodeError:
            print('Failed to decode output as json, output was:')
            print(output)
            raise


print('\nCreating Solido instance')
result = solido(
    'create-solido',
    '--stake-pool-program-id', stake_pool_program_id,
    '--solido-program-id', solido_program_id,
    '--fee-numerator', '4',
    '--fee-denominator', '31',
    '--max-validators', '9',
    '--insurance-fee', '7',
    '--treasury-fee', '5',
    '--validation-fee', '3',
    '--manager-fee', '2',
    '--treasury-account-owner', treasury_account_owner,
    '--insurance-account-owner', insurance_account_owner,
    '--manager-fee-account-owner', manager_fee_account_owner,
)
solido_address = result['solido_address']
treasury_account = result['treasury_account']
insurance_account = result['insurance_account']
manager_fee_account = result['manager_fee_account']
print(f'> Created instance at {solido_address}.')
