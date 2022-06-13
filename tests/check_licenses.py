#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
Check license compatibility of Solido dependencies. Requires "cargo-license" to
be installed.
"""

from typing import Any, Dict, List
from subprocess import run

import json
import sys

# Dependencies that use these licenses, are OK to include in the on-chain program,
# or in the CLI binary.
ALLOWED_LICENSES = [
    'Apache-2.0',
    'BSD-2-Clause',
    'BSD-3-Clause',
    'CC0-1.0',
    'GPL-3.0',
    'ISC',
    'MIT',
    'MPL-2.0',
    'LGPL-2.1-or-later',
]

# These dependencies do not satisfy the above condition, but are allowed anyway
# for reasons listed below.
ALLOWED_DEPENDENCIES = [
    # Actually Apache 2.0, but it's not part of the crate metadata.
    'serum-multisig',
    # Has a complex licensing situation, that has been verified by Wenger & Vieli
    # to be compatible with Solido.
    'ring',
    # All runtime code that we depend on is covered by an ISC-style license.
    'webpki',
    # https://github.com/hsivonen/encoding_rs#licensing
    # Some parts of it are (Apache2 OR MIT) and some parts of it are 3-clause BSD
    # but all of these are whitelisted separately
    'encoding_rs',
]


def get_deps(manifest_path: str) -> List[Dict[str, Any]]:
    """
    Return all runtime dependencies and their licenses. Example element:
    {
      "name": "solana-vote-program",
      "version": "1.9.28",
      "authors": "Solana Maintainers <maintainers@solana.foundation>",
      "repository": "https://github.com/solana-labs/solana",
      "license": "Apache-2.0",
      "license_file": null,
      "description": "Solana Vote program"
    }
    """
    result = run(
        [
            'cargo',
            'license',
            '--avoid-dev-deps',
            '--avoid-build-deps',
            '--json',
            '--manifest-path',
            manifest_path,
        ],
        check=True,
        capture_output=True,
        encoding='utf-8',
    )
    result_parsed: List[Dict[str, Any]] = json.loads(result.stdout)
    return result_parsed


def main() -> None:
    all_ok = True

    # Get the dependencies of the on-chain program, and of the CLI binary.
    deps_on_chain = get_deps('program/Cargo.toml')
    deps_cli = get_deps('cli/maintainer/Cargo.toml')
    deps = deps_on_chain + deps_cli

    for dep in deps:
        dep_name = dep['name']
        if dep_name in ALLOWED_DEPENDENCIES:
            continue

        license = dep.get('license')

        if license is None:
            print(f'{dep_name} does not have a machine-readable license specified')
            all_ok = False
            continue

        options = license.split(' OR ')
        is_ok = any(option in ALLOWED_LICENSES for option in options)

        if not is_ok:
            print(f'{dep_name} has an unknown license: {license}')
            all_ok = False

    if all_ok:
        print('All dependency licenses are ok.')
    else:
        sys.exit(1)


if __name__ == '__main__':
    main()
