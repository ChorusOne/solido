#!/usr/bin/env python3

"""
This script has multiple options to update Solido state version
"""


import pprint
import argparse
import json
import sys
import os.path
import fileinput
from typing import Any

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.append(os.path.dirname(SCRIPT_DIR))

from tests.util import solido, solana, run  # type: ignore


def eprint(*args: Any, **kwargs: Any) -> None:
    print(*args, file=sys.stderr, **kwargs)


def get_signer() -> Any:
    return run('solana-keygen', 'pubkey').strip()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--config", type=str, help='Path to json config file', required=True
    )

    subparsers = parser.add_subparsers(title='subcommands', dest="command")

    current_parser = subparsers.add_parser(
        'deactivate-validators',
        help='Create and output multisig transactions to deactivate all validators',
    )
    current_parser.add_argument(
        "--keypair-path",
        type=str,
        help='Signer keypair or a ledger path',
        required=True,
    )

    current_parser = subparsers.add_parser(
        'load-program',
        help='Write program from `program-filepath` to a random buffer address.',
    )
    current_parser.add_argument(
        "--program-filepath", help='/path/to/program.so', required=True
    )

    current_parser = subparsers.add_parser(
        'add-validators',
        help='Create add-validator transactions from file and print them to stdout',
    )
    current_parser.add_argument(
        "--vote-accounts",
        type=str,
        help='List of validator vote account file path',
        required=True,
    )
    current_parser.add_argument(
        "--keypair-path",
        type=str,
        help='Signer keypair or a ledger path',
        required=True,
    )

    args = parser.parse_args()

    sys.argv.append('--verbose')

    with open(args.config) as f:
        config = json.load(f)
        cluster = config.get("cluster")
        if cluster:
            os.environ['NETWORK'] = cluster

    if args.command == "deactivate-validators":
        lido_state = solido('--config', args.config, 'show-solido')
        validators = lido_state['solido']['validators']['entries']
        for validator in validators:
            result = solido(
                '--config',
                args.config,
                'deactivate-validator',
                '--validator-vote-account',
                validator['pubkey'],
                keypair_path=args.keypair_path,
            )
            print(result['transaction_address'])

    elif args.command == "add-validators":
        with open(args.vote_accounts) as infile:
            for pubkey in infile:
                result = solido(
                    '--config',
                    args.config,
                    'add-validator',
                    '--validator-vote-account',
                    pubkey.strip(),
                    keypair_path=args.keypair_path,
                )
                print(result['transaction_address'])

    elif args.command == "load-program":
        lido_state = solido('--config', args.config, 'show-solido')
        write_result = solana(
            '--output',
            'json',
            'program',
            'write-buffer',
            '--buffer-authority',
            lido_state['solido']['manager'],
            args.program_filepath,
        )
        write_result = json.loads(write_result)

        solana(
            'program',
            'set-buffer-authority',
            '--new-buffer-authority',
            lido_state['solido']['manager'],
            write_result['buffer'],
        )
        print(write_result['buffer'])

    else:
        eprint("Unknown command %s" % args.command)
