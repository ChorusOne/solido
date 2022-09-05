#!/usr/bin/env python3

"""
This script has multiple options to update Solido state version

Usage:
    $ls
    solido/
    solido_old/
    solido_test.json

    $cd solido_old

    ../solido/scripts/update_solido_version.py --config ../solido_test.json deactivate-validators --keypair-path ./tests/.keys/maintainer.json > output

    ./target/debug/solido --config ../solido_test.json --keypair-path ./tests/.keys/maintainer.json multisig approve-batch --transaction-addresses-path output

    # Perfom maintainance till validator list is empty, wait for epoch boundary if on mainnet
    ./target/debug/solido --config ../solido_test.json --keypair-path tests/.keys/maintainer.json perform-maintenance

    ../solido/scripts/update_solido_version.py --config ../solido_test.json propose-upgrade --keypair-path ./tests/.keys/maintainer.json --program-filepath ../solido/target/deploy/lido.so > output

    ./target/debug/solido --config ../solido_test.json --keypair-path ./tests/.keys/maintainer.json multisig approve-batch --transaction-addresses-path output

    # cretae developer account owner Fp572FrBjhWprtT7JF4CHgeLzPD9g8s2Ht7k5bdaWjwF
    # solana-keygen new --no-bip39-passphrase --silent --outfile ~/developer_fee_key.json
    solana --url localhost transfer --allow-unfunded-recipient ./tests/.keys/maintainer.json 32.0

    $cd ../solido
    scripts/update_solido_version.py --config ../solido_test.json propose-migrate --keypair-path ../solido_old/tests/.keys/maintainer.json > output

    ./target/debug/solido --config ../solido_test.json --keypair-path ../solido_old/tests/.keys/maintainer.json multisig approve-batch --transaction-addresses-path output
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
        "--keypair-path", type=str, help='Signer keypair path', required=True
    )

    current_parser = subparsers.add_parser(
        'execute-transactions', help='Execute multisig transactions from stdin'
    )

    current_parser = subparsers.add_parser(
        'propose-upgrade',
        help='Write program from `program-filepath` to a random buffer address. Create multisig transaction to upgrade Solido state',
    )
    current_parser.add_argument(
        "--keypair-path", type=str, help='Signer keypair path', required=True
    )
    current_parser.add_argument(
        "--program-filepath", help='/path/to/program.so', required=True
    )

    current_parser = subparsers.add_parser(
        'propose-migrate', help='Update solido state to a version 2'
    )
    current_parser.add_argument(
        "--keypair-path", type=str, help='Signer keypair path', required=True
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

    elif args.command == "execute-transactions":
        for line in sys.stdin:
            solido(
                '--config',
                args.config,
                'multisig',
                'execute-transaction',
                '--transaction-address',
                line.strip(),
            )

    elif args.command == "propose-upgrade":
        lido_state = solido('--config', args.config, 'show-solido')
        program_result = solana(
            '--output', 'json', 'program', 'show', config['solido_program_id']
        )
        program_result = json.loads(program_result)
        if program_result['authority'] != lido_state['solido']['manager']:
            solana(
                'program',
                'set-upgrade-authority',
                '--new-upgrade-authority',
                lido_state['solido']['manager'],
                config['solido_program_id'],
            )

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
        # print("Buffer address %s" % write_result['buffer'])

        solana(
            'program',
            'set-buffer-authority',
            '--new-buffer-authority',
            lido_state['solido']['manager'],
            write_result['buffer'],
        )

        propose_result = solido(
            '--config',
            args.config,
            'multisig',
            'propose-upgrade',
            '--spill-address',
            get_signer(),
            '--buffer-address',
            write_result['buffer'],
            '--program-address',
            config['solido_program_id'],
            keypair_path=args.keypair_path,
        )
        print(propose_result['transaction_address'])

    elif args.command == "propose-migrate":
        update_result = solido(
            '--config',
            args.config,
            'migrate-state-to-v2',
            '--developer-account-owner',
            'Fp572FrBjhWprtT7JF4CHgeLzPD9g8s2Ht7k5bdaWjwF',
            '--st-sol-mint',
            config['st_sol_mint'],
            '--developer-fee-share',
            '2',
            '--treasury-fee-share',
            '4',
            '--st-sol-appreciation-share',
            '94',
            '--max-commission-percentage',
            '5',
            keypair_path=args.keypair_path,
        )
        print(update_result['transaction_address'])

    else:
        eprint("Unknown command %s" % args.command)
