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
    '--solido-program-id', solido_program_id,
    '--fee-numerator', '4',
    '--fee-denominator', '31',
    '--max-validators', '251',
)
print(result)
