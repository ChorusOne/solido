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

from validator_onboarding import iter_rows_from_stdin


def main() -> None:
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)

    with open(sys.argv[2], 'w', encoding='utf-8') as transaction_file:
        for row in iter_rows_from_stdin():
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
