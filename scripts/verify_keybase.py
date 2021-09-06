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

from typing import Any, Dict, Iterable, NamedTuple, Optional


SOLIDO_AUTHORIZED_WITHDAWER = 'GgrQiJ8s2pfHsfMbEFtNcejnzLegzZ16c9XtJ2X2FpuF'
ST_SOL_MINT = '7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj'


def solana(*args: str) -> Any:
    full_args = ['solana', '--url', 'https://api.mainnet-beta.solana.com', *args]
    result = subprocess.run(full_args, check=True, capture_output=True, encoding='utf-8')
    return json.loads(result.stdout)


class ValidatorInfo(NamedTuple):
    identity_address: str
    info_address: str
    keybase_username: Optional[str]
    name: Optional[str]


def iter_validator_infos() -> Iterable[ValidatorInfo]:
    """
    Return the validator info for all validators on mainnet.
    """
    for info in solana('validator-info', 'get', '--output', 'json'):
        yield ValidatorInfo(
            identity_address=info['identityPubkey'],
            info_address=info['infoPubkey'],
            keybase_username=info['info'].get('keybaseUsername'),
            name=info['info'].get('name'),
        )


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

    def check(self, validators_by_identity: Dict[str, ValidatorInfo]) -> None:
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

        if vote_account.commission == 100:
            print('  OK: Vote account commission is 100%.')
        else:
            print('  ERROR: Vote account commission is not 100%.')

        validator_info = validators_by_identity.get(vote_account.validator_identity_address)
        if validator_info is None:
            print('  ERROR: Validator identity does not exist.')
            return
        else:
            print('  OK: Validator identity exists.')

        if validator_info.keybase_username == self.keybase_username:
            print('  OK: Keybase username in form matches username in identity account.')
        else:
            print('  ERROR: Keybase username in identity account does not match the form.')

        if validator_info.name.startswith('Lido / '):
            print('  OK: Validator identity name starts with "Lido / ".')
        else:
            print('  ERROR: Validator identity name does not start with "Lido / ".')

        if validator_info.name == self.identity_name:
            print('  OK: Name in identity account matches name in form.')
        else:
            print('  ERROR: Name in identity account does not mach name in form.')

        # TODO: Check mint of stSOL account.
        # TODO: Check Keybase for public key.


def iter_rows_from_stdin() -> Iterable[ValidatorResponse]:
    """
    Yield rows from stdin, including header, excluding blank lines.
    """
    for line in sys.stdin:
        if line.strip() == '':
            continue
        yield ValidatorResponse(*line.strip().split('\t'))


def main() -> None:
    # Build a map of validators by identity address.
    validators_by_identity: Dict[str, ValidatorInfo] = {
        info.identity_address: info
        for info in iter_validator_infos()
    }

    for row in iter_rows_from_stdin():
        if row.timestamp == 'Timestamp':
            # This is the header row, skip over it.
            continue

        row.check(validators_by_identity)


if __name__ == '__main__':
    main()
