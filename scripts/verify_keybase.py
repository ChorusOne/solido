#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script verifies details provided by validators that are onboarding.

Takes tab-separated values on stdin with the following columns:

 * Timestamp
 * Email address
 * Validator name
 * Keybase account name
 * Vote account
 * (Withdraw authority check, constant)
 * (Commission percentage check, constant)
 * (Vote promise check, constant)
 * stSOL account
 * (stSOL account mint check, constant)
 * (Added public key to keybase check)
 * Identity name as published on identity account
 * (unused column, optional)
 * Maintainer address (if applicable; optional)

This script then verifies:

 * The vote account withdraw authority.
 * The vote account commission.
 * That the identity account of the vote account has the Keybase username as
   provided here.
 * That the public key of the identity account of the vote account has been
   added to Keybase.
 * That the name of the identity account matches the name provided here.
 * That the stSOL account has the right mint.

The header row will be stripped.
"""

import json
import subprocess
import sys

from typing import Any, Dict, Iterable, NamedTuple


SOLIDO_AUTHORIZED_WITHDAWER = 'GgrQiJ8s2pfHsfMbEFtNcejnzLegzZ16c9XtJ2X2FpuF'
ST_SOL_MINT = '7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj'


def solana(*args: str) -> Dict[str, Any]:
    full_args = ['solana', '--url', 'https://api.mainnet-beta.solana.com', *args]
    result = subprocess.run(full_args, check=True, capture_output=True, encoding='utf-8')
    return json.loads(result.stdout)


class VoteAccount(NamedTuple):
    validator_identity_address: str
    authorized_withdrawer: str
    commission: int
    num_votes: int


class ValidatorResponse(NamedTuple):
    timestamp: str
    email: str
    validator_name: str
    keybase_username: str
    vote_account_address: str
    withdraw_authority_check: str
    commission_check: str
    will_vote_check: str
    st_sol_account_address: str
    st_sol_mint_check: str
    added_to_keybase_check: str
    identity_name: str
    unused: str = ''
    maintainer_address: str = ''

    def get_vote_account(self) -> VoteAccount:
        result = solana('vote-account', '--output', 'json', self.vote_account_address)
        return VoteAccount(
            validator_identity_address=result['validatorIdentity'],
            authorized_withdrawer=result['authorizedWithdrawer'],
            commission=result['commission'],
            num_votes=len(result['votes'])
        )

    def check(self) -> None:
        print(self.validator_name)
        vote_account = self.get_vote_account()

        if vote_account.authorized_withdrawer == SOLIDO_AUTHORIZED_WITHDAWER:
            print('  OK: Authorized withdrawer set to Solido.')
        else:
            print('  ERROR: Wrong authorized withdrawer.')

        if vote_account.num_votes > 0:
            print('  OK: Vote account has votes.')
        else:
            print('  WARN: Vote account has not voted yet.')


def iter_rows_from_stdin() -> Iterable[ValidatorResponse]:
    """
    Yield rows from stdin, including header, excluding blank lines.
    """
    for line in sys.stdin:
        if line.strip() == '':
            continue
        yield ValidatorResponse(*line.strip().split('\t'))


def main() -> None:
    for row in iter_rows_from_stdin():
        if row.timestamp == 'Timestamp':
            # This is the header row, skip over it.
            continue

        row.check()


if __name__ == '__main__':
    main()
