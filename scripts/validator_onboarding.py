# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

import sys

from typing import Iterable, NamedTuple

Address = str

class ValidatorResponse(NamedTuple):
    """
    This struct mirrors the columns in the response sheet of the validator
    onboarding form.
    """
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
    Yield rows from stdin, excluding header, excluding blank lines.
    """
    for line in sys.stdin:
        if line.strip() == '':
            continue

        result = ValidatorResponse(*line.strip().split('\t'))

        if result.timestamp == 'Timestamp':
            # This is the header row
            continue

        yield result
