#!/usr/bin/env python3

"""
This script has multiple options to to interact with Solido
"""


import argparse
import json
import sys
import os.path
from typing import Any
import verify_transaction

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
    current_parser.add_argument(
        "--outfile",
        type=str,
        help='Output file path',
        required=True,
    )

    current_parser = subparsers.add_parser(
        'load-program',
        help='Write program from `program-filepath` to a random buffer address.',
    )
    current_parser.add_argument(
        "--program-filepath", help='/path/to/program.so', required=True
    )
    current_parser.add_argument(
        "--outfile",
        type=str,
        help='Output file path',
        required=True,
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
    current_parser.add_argument(
        "--outfile",
        type=str,
        help='Output file path',
        required=True,
    )

    current_parser = subparsers.add_parser(
        'execute-transactions',
        help='Execute transactions from file one by one',
    )
    current_parser.add_argument(
        "--keypair-path",
        type=str,
        help='Signer keypair or a ledger path',
        required=True,
    )
    current_parser.add_argument(
        "--transactions",
        type=str,
        help='Transactions file path. Each transaction per line',
        required=True,
    )

    current_parser = subparsers.add_parser(
        'check-transactions',
        help='Check transactions from a file',
    )
    current_parser.add_argument(
        "--transactions-path",
        type=str,
        help='Path to transactions file',
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
        print("vote accounts:")
        with open(args.outfile, 'w') as ofile:
            for validator in validators:
                print(validator['pubkey'])
                result = solido(
                    '--config',
                    args.config,
                    'deactivate-validator',
                    '--validator-vote-account',
                    validator['pubkey'],
                    keypair_path=args.keypair_path,
                )
                address = result.get('transaction_address')
                if address is None:
                    eprint(result)
                else:
                    ofile.write(address + '\n')

    elif args.command == "add-validators":
        print("vote accounts:")
        with open(args.vote_accounts) as infile, open(args.outfile, 'w') as ofile:
            for pubkey in infile:
                print(pubkey)
                result = solido(
                    '--config',
                    args.config,
                    'add-validator',
                    '--validator-vote-account',
                    pubkey.strip(),
                    keypair_path=args.keypair_path,
                )
                address = result.get('transaction_address')
                if address is None:
                    eprint(result)
                else:
                    ofile.write(address + '\n')

    elif args.command == "execute-transactions":
        with open(args.transactions) as infile:
            for transaction in infile:
                transaction = transaction.strip()
                transaction_info = solido(
                    '--config',
                    args.config,
                    'multisig',
                    'show-transaction',
                    '--transaction-address',
                    transaction,
                )
                if not transaction_info['did_execute']:
                    print(f"Executing transaction {transaction}")
                    result = solido(
                        '--config',
                        args.config,
                        'multisig',
                        'execute-transaction',
                        '--transaction-address',
                        transaction,
                        keypair_path=args.keypair_path,
                    )
                    print(f"Transaction {transaction} executed")

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
        with open(args.outfile, 'w') as ofile:
            ofile.write(write_result['buffer'])

    elif args.command == "check-transactions":
        with open(args.transactions_path, 'r') as ifile:
            Counter = 0
            Success = 0
            for transaction in ifile:
                result = solido(
                    '--config',
                    args.config,
                    'multisig',
                    'show-transaction',
                    '--transaction-address',
                    transaction.strip(),
                )
                Counter += 1
                print("Transaction #" + str(Counter) + ": " + transaction.strip())
                if verify_transaction.verify_transaction_data(result):
                    Success += 1

                # print(result['signers'])
                # result['']
                # config.get('program-id')
            print(
                "Summary: successfully verified "
                + str(Success)
                + " from "
                + str(Counter)
                + " transactions"
            )
    else:
        eprint("Unknown command %s" % args.command)
