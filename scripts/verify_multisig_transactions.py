#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script verifies transactions created by `propose_add_validators.py` against
validator form submissions from on stdin.

Usage:

    scripts/verify_multisig_transactions.py solido_config.json transactions.txt < validators.tsv
"""

import sys
import subprocess
import json

from validator_onboarding import Address, ValidatorResponse, iter_rows_from_stdin
from validator_onboarding import print_ok, print_error
from typing import Iterable


SOLIDO_ADDRESS = '49Yi1TKkNyYjPAFdR9LBvoHcUjuPX4Df5T5yv39w2XTn'


def iter_transaction_addresses() -> Iterable[Address]:
    with open(sys.argv[2], 'r', encoding='utf-8') as f:
        for line in f:
            # The first word on the line is the transaction address,
            # we ignore any content after that.
            yield line.split()[0]


def main() -> None:
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)

    for form_response, transaction_address in zip(
        iter_rows_from_stdin(), iter_transaction_addresses()
    ):
        print(f'\n{form_response.validator_name}:')
        cmd = [
            'target/debug/solido',
            '--config',
            sys.argv[1],
            '--output',
            'json',
            'multisig',
            'show-transaction',
            '--transaction-address',
            transaction_address,
        ]
        result = subprocess.run(cmd, check=True, capture_output=True, encoding='utf-8')
        transaction = json.loads(result.stdout)
        instruction = (
            transaction.get('parsed_instruction', {})
            .get('SolidoInstruction', {})
            .get('AddValidator')
        )

        if instruction is None:
            print_error('Instruction is not an AddValidator instruction.')
        else:
            print_ok('Instruction is an AddValidator instruction.')

        if form_response.vote_account_address == instruction['validator_vote_account']:
            print_ok('Validator vote account in form response matches instruction.')
        else:
            print_error(
                'Validator vote account in form response does not match instruction.'
            )

        if instruction['solido_instance'] == SOLIDO_ADDRESS:
            print_ok('Solido instance is the mainnet instance.')
        else:
            print_error('Solido instance is not the mainnet instance.')


if __name__ == '__main__':
    main()
