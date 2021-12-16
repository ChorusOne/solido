#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script prints past multisig transactions for LDO incentive distribution as a Markdown table.

Usage:

    scripts/print_incentive_history.py
"""

from typing import Any, Dict, List, NamedTuple, Tuple

import subprocess
import os
import json


class Entry(NamedTuple):
    date: str
    name: str
    pool: str
    amount_ldo: int
    multisig_tx: str


# fmt: off
entries = [
    # December 2021
    Entry('2021-12-15', 'Mercurial', 'stSOL-SOL',      40_000, 'FuoT4Yi2YMYwEyuFkBaQ36FARYDNVwjPp8dymB6mAizJ'),
    Entry('2021-12-15', 'Orca',      'stSOL-USDC',     60_000, 'JB92vLZuRj7t9cYRi2j4TnKoRPjdJNHJZepiFd7GQHD3'),
    Entry('2021-12-15', 'Orca',      'stSOL-wstETH',   10_000, 'FJTfrRt6xfYyR8mx4aQEQ3raBPi2vwcuyKtvSRLZBhxH'),
    Entry('2021-12-15', 'Raydium',   'stSOL-USDC',     60_000, '2UtYtZ4cydPJRv969ASqB3bqR9MDzDoWAs8gM42PkPtc'),

    # November 2021
    Entry('2021-11-07', 'Mercurial', 'stSOL-SOL',      75_000, '6a1K1eF6k6oXp5PYKnUqGm2Y3uJxfBkGn1JDdiXgsud7'),
    Entry('2021-11-07', 'Orca',      'stSOL-wstETH',  125_000, 'Dmfp4UuFRqBJ5TU2U21JhPaTjv4HcLzZQgWBvj6DadZS'),
    Entry('2021-11-07', 'Raydium',   'stSOL-USDC',    125_000, 'ByJAsTdHzrabU8aihvZCtmLRorhtgVsXLBCF31P2PgUz'),

    # October 20201
    Entry('2021-10-06', 'Mercurial', 'stSOL-SOL',     75_000, '7ymsoHodgC9ES6NJsWmt6aA7csJmkFvhsC4nBsTY67gF'),
    Entry('2021-10-06', 'Orca',      'stSOL-wstETH', 125_000, '5V1gUNKBgFPpVmH2qoNfyhB3PjpmyZbJ6VHfeVxWKkfY'),
    Entry('2021-10-06', 'Raydium',   'stSOL-USDC',   125_000, '6q6QgB2eAdhg9KLZCRTty8MDwFL9xqUa7v1FRDusTfyk'),
]
# fmt: on


def format_address_as_link(addr: str) -> str:
    """
    Format a Solana address as a Markdown hyperlink to Solscan.
    """
    href = f'https://solscan.io/account/{addr}'
    name = f'{addr[:6]}…{addr[-6:]}'
    return f"[{name}]({href} '{addr}')"


class Details(NamedTuple):
    # Multisig transaction details
    did_execute: bool

    # Token transfer details
    token_address: str
    from_address: str
    to_address: str
    amount: int

    def get_to_owner(self) -> str:
        """
        Return the address that owns the `to_address` SPL token account.
        """
        result = subprocess.run(
            [
                'spl-token',
                '--url',
                'https://lido.rpcpool.com',
                '--output',
                'json',
                'account-info',
                '--address',
                self.to_address,
            ],
            capture_output=True,
            check=True,
            encoding='utf-8',
        )
        result_object: Dict[str, Any] = json.loads(result.stdout)
        owner: str = result_object['owner']
        return owner

    @staticmethod
    def get_table_headers() -> List[Tuple[str, str]]:
        """
        Return table headers and alignment.
        """
        return [
            ('Name', ':--'),
            ('Amount (wLDO)', '--:'),
            ('Recipient stSOL account', ':--'),
            ('Recipient owner account', ':--'),
            ('Multisig transaction', ':--'),
        ]

    def to_table_row(self, entry: Entry) -> List[str]:
        return [
            f'{entry.name} {entry.pool}',
            # wLDO has 8 decimals, so divide by 1e8.
            f'{self.amount / 1e8:,.0f}',
            format_address_as_link(self.to_address),
            format_address_as_link(self.get_to_owner()),
            format_address_as_link(entry.multisig_tx),
        ]


def get_multisig_transaction_details(addr: str) -> Details:
    config = {
        **os.environ,
        'SOLIDO_CLUSTER': 'https://lido.rpcpool.com',
        'SOLIDO_MULTISIG_PROGRAM_ID': 'AAHT26ecV3FEeFmL2gDZW6FfEqjPkghHbAkNZGqwT8Ww',
        'SOLIDO_MULTISIG_ADDRESS': '3cXyJbjoAUNLpQsFrFJTTTp8GD3uPeabYbsCVobkQpD1',
        'SOLIDO_SOLIDO_PROGRAM_ID': 'CrX7kMhLC3cSsXJdT7JDgqrRVWGnUpX3gfEfxxU2NVLi',
        'SOLIDO_SOLIDO_ADDRESS': '49Yi1TKkNyYjPAFdR9LBvoHcUjuPX4Df5T5yv39w2XTn',
    }
    result = subprocess.run(
        [
            'target/debug/solido',
            '--output',
            'json',
            'multisig',
            'show-transaction',
            '--transaction-address',
            addr,
        ],
        env=config,
        check=True,
        capture_output=True,
        encoding='utf-8',
    )
    raw_details: Dict[str, Any] = json.loads(result.stdout)
    transfer_details: Dict[str, Any] = raw_details['parsed_instruction'][
        'TokenInstruction'
    ]['Transfer']
    signer_details: Dict[str, Any]

    return Details(
        did_execute=raw_details['did_execute'],
        from_address=transfer_details['from_address'],
        to_address=transfer_details['to_address'],
        token_address=transfer_details['token_address'],
        amount=transfer_details['amount'],
    )


def print_table_header() -> None:
    headers = Details.get_table_headers()
    row0 = [col[0] for col in headers]
    row1 = [col[1] for col in headers]
    print('| ' + ' | '.join(row0) + ' |')
    print('|' + '|'.join(row1) + '|')


def main() -> None:
    prev_date = ''

    for entry in entries:
        details = get_multisig_transaction_details(entry.multisig_tx)

        assert (
            details.token_address == 'HZRCwxP2Vq9PCpPXooayhJ2bxTpo5xfpQrwB1svh332p'
        ), f'Expected token to be wLDO for {entry}'
        assert (
            details.from_address == 'T7VpKriUL68aQAKXFyfG3jJjvPHnxaC95XsjaZKSZ7b'
        ), f'Expected from address to be the Multisig wLDO account for {entry}'
        assert (
            details.amount / 1e8 == entry.amount_ldo
        ), f'Expected amount in script to match transaction for {entry}'
        assert (
            details.did_execute
        ), f'Expected transaction to be executed for {entry}'

        if prev_date != entry.date:
            print(f'\n### {entry.date}')
            print_table_header()
            prev_date = entry.date

        print('| ' + ' | '.join(details.to_table_row(entry)) + ' |')


if __name__ == '__main__':
    main()
