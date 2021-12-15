#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script prints past multisig transactions for LDO incentive distribution as a Markdown table.

Usage:

    scripts/print_incentive_history.py
"""

from typing import NamedTuple


class Entry(NamedTuple):
    date: str
    name: str
    pool: str
    amount_ldo: int
    multisig_tx: str


# fmt: off
entries = [
    Entry('2021-10-06', 'Mercurial', 'stSOL-SOL',      75_000, '7ymsoHodgC9ES6NJsWmt6aA7csJmkFvhsC4nBsTY67gF'),
    Entry('2021-10-06', 'Orca',      'stSOL-wstETH',  125_000, '5V1gUNKBgFPpVmH2qoNfyhB3PjpmyZbJ6VHfeVxWKkfY'),
    Entry('2021-10-06', 'Raydium',   'stSOL-USDC',    125_000, '6q6QgB2eAdhg9KLZCRTty8MDwFL9xqUa7v1FRDusTfyk'),
    Entry('2021-11-07', 'Mercurial', 'stSOL-SOL',      75_000, '6a1K1eF6k6oXp5PYKnUqGm2Y3uJxfBkGn1JDdiXgsud7'),
    Entry('2021-11-07', 'Orca',      'stSOL-wstETH',  125_000, 'Dmfp4UuFRqBJ5TU2U21JhPaTjv4HcLzZQgWBvj6DadZS'),
    Entry('2021-11-07', 'Raydium',   'stSOL-USDC',    125_000, 'ByJAsTdHzrabU8aihvZCtmLRorhtgVsXLBCF31P2PgUz'),
]
# fmt: on
