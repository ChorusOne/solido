#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script reads the validator onboarding form responses, and calls "solido add-validator"
to propose adding those validators. It expects you have a Solido config file ready
with the right signer configured.

Usage:

    scripts/propose_add_validators.py solido_config.json transactions.txt < validators.tsv
"""

import json
import subprocess
import sys

from typing import Iterable, NamedTuple


# A type alias to make some things more self-documenting.
Address = str


class ValidatorResponse(NamedTuple):
    timestamp: str
    email: str
    validator_name: str
    keybase_username: str
    vote_account_address: Address
    withdraw_authority_check: Address
    commission_check: str
    will_vote_check: str
    st_sol_account_address: Address
    st_sol_mint_check: Address
    added_to_keybase_check: str
    identity_name: str
    unused: str = ''
    maintainer_address: Address = ''


def iter_rows_from_stdin() -> Iterable[ValidatorResponse]:
    """
    Yield rows from stdin, including header, excluding blank lines.
    """
    for line in sys.stdin:
        if line.strip() == '':
            continue
        yield ValidatorResponse(*line.strip().split('\t'))


def main() -> None:
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)

    with open(sys.argv[2], 'w', encoding='utf-8') as transaction_file:
        for row in iter_rows_from_stdin():
            if row.timestamp == 'Timestamp':
                # This is the header row, skip over it.
                continue

            print(f'Creating transaction to add {row.validator_name} ...')
            cmd = [
                'target/debug/solido',
                '--config',
                sys.argv[1],
                '--output',
                'json',
                'add-validator',
                '--validator-vote-account', row.vote_account_address,
                '--validator-fee-account', row.st_sol_account_address,
            ]
            try:
                result = subprocess.run(cmd, check=True, capture_output=True, encoding='utf-8')

            except subprocess.CalledProcessError as exc:
                print('Command failed:', ' '.join(cmd))
                print(exc.stdout)
                print(exc.stderr)
                sys.exit(1)

            transaction_address = json.loads(result.stdout)['transaction_address']
            transaction_file.write(f'{transaction_address}  # Add {row.validator_name}\n')
            transaction_file.flush()
            print(f'-> {transaction_address}')


if __name__ == '__main__':
    main()
