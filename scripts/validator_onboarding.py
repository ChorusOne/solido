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
    commission_check: str
    will_vote_check: str
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


def print_color(message: str, *, ansi_color_code: str, end: str = '\n') -> None:
    # Switch to the given color, print the message, then reset formatting.
    print(f'\x1b[{ansi_color_code}m{message}\x1b[0m', end=end)


def print_error(message: str) -> None:
    # 31 is red.
    print_color(f'ERROR {message}', ansi_color_code='31')


def print_ok(message: str) -> None:
    # 32 is green. For OK, we only make the prefix green, not the entire message,
    # to make validation failures stand out more.
    print_color(f'OK   ', ansi_color_code='32', end=' ')
    print(message)


def print_warn(message: str) -> None:
    # 33 is yellow.
    print_color(f'WARN  {message}', ansi_color_code='33')
