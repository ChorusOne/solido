#!/usr/bin/env python3

"""
This script has multiple options to to interact with Solido
"""


import argparse
import json
import sys
import os.path
from typing import Any, Optional
import verify_transaction
from install_solido import install_solido


SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.append(os.path.dirname(SCRIPT_DIR))

from tests.util import solido, solana, run  # type: ignore


def set_solido_cli_path(strData):
    if os.path.isfile(strData):
        os.environ["SOLPATH"] = strData
    else:
        print("Program does not exist: " + strData)


def eprint(*args: Any, **kwargs: Any) -> None:
    print(*args, file=sys.stderr, **kwargs)


def get_signer() -> Any:
    return run('solana-keygen', 'pubkey').strip()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()

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
        "--outfile", type=str, help='Output file path', required=True
    )

    current_parser = subparsers.add_parser(
        'load-program',
        help='Write program from `program-filepath` to a random buffer address.',
    )
    current_parser.add_argument(
        "--program-filepath", help='/path/to/program.so', required=True
    )
    current_parser.add_argument(
        "--outfile", type=str, help='Output file path', required=True
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
        "--outfile", type=str, help='Output file path', required=True
    )

    current_parser = subparsers.add_parser(
        'execute-transactions', help='Execute transactions from file one by one'
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
    current_parser.add_argument(
        "--phase",
        type=str,
        help='Phase of deploy: preparation, deactivation, upgrade, adding',
        required=True,
    )

    current_parser = subparsers.add_parser(
        'check-transactions', help='Check transactions from a file'
    )
    current_parser.add_argument(
        "--phase",
        type=str,
        help='Phase of deploy: preparation, deactivation, upgrade, adding',
        required=True,
    )
    current_parser.add_argument(
        "--transactions-path", type=str, help='Path to transactions file', required=True
    )

    current_parser = subparsers.add_parser(
        'install-solido',
        help='Install solido_v1 and solido_v2 for deploy actions',
    )

    current_parser = subparsers.add_parser('test', help='`Command for tests`')

    args = parser.parse_args()

    sys.argv.append('--verbose')

    install_solido()
    with open(str(os.getenv("SOLIDO_CONFIG"))) as f:
        config = json.load(f)
        cluster = config.get("cluster")
        if cluster:
            os.environ['NETWORK'] = cluster

    if args.command == "deactivate-validators":
        set_solido_cli_path(os.getenv("SOLIDO_V1"))
        lido_state = solido('--config', os.getenv("SOLIDO_CONFIG"), 'show-solido')
        validators = lido_state['solido']['validators']['entries']
        print("vote accounts:")
        with open(args.outfile, 'w') as ofile:
            for validator in validators:
                print(validator['pubkey'])
                result = solido(
                    '--config',
                    os.getenv("SOLIDO_CONFIG"),
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
        set_solido_cli_path(os.getenv("SOLIDO_V2"))
        print("vote accounts:")
        with open(args.vote_accounts) as infile, open(args.outfile, 'w') as ofile:
            for pubkey in infile:
                print(pubkey)
                result = solido(
                    '--config',
                    os.getenv("SOLIDO_CONFIG"),
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
            if args.phase == "deactivation":
                set_solido_cli_path(os.getenv("SOLIDO_V1"))
            elif args.phase == "adding":
                print(args.phase)
                set_solido_cli_path(os.getenv("SOLIDO_V2"))
            else:
                print("Unknown phase")

            for transaction in infile:
                transaction = transaction.strip()
                transaction_info = solido(
                    '--config',
                    os.getenv("SOLIDO_CONFIG"),
                    'multisig',
                    'show-transaction',
                    '--transaction-address',
                    transaction,
                )
                if not transaction_info['did_execute']:
                    print(f"Executing transaction {transaction}")
                    result = solido(
                        '--config',
                        os.getenv("SOLIDO_CONFIG"),
                        'multisig',
                        'execute-transaction',
                        '--transaction-address',
                        transaction,
                        keypair_path=args.keypair_path,
                    )
                    print(f"Transaction {transaction} executed")

    elif args.command == "load-program":
        set_solido_cli_path(os.getenv("SOLIDO_V1"))
        lido_state = solido('--config', os.getenv("SOLIDO_CONFIG"), 'show-solido')
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
            if args.phase == "deactivation":
                print(args.phase)
                set_solido_cli_path(os.getenv("SOLIDO_V1"))
                verify_transaction.verify_solido_state()
                verify_transaction.verify_transactions(ifile)
            elif args.phase == "preparation":
                print(args.phase)
            elif args.phase == "upgrade":
                print(args.phase)
                set_solido_cli_path(os.getenv("SOLIDO_V1"))
                verify_transaction.verify_solido_state()
                set_solido_cli_path(os.getenv("SOLIDO_V2"))
                verify_transaction.verify_transactions(ifile)
            elif args.phase == "adding":
                print(args.phase)
                set_solido_cli_path(os.getenv("SOLIDO_V2"))
                verify_transaction.verify_solido_state()
                verify_transaction.verify_transactions(ifile)
            else:
                print("Unknown phase")
    elif args.command == "install-solido":
        print("Install solido...")
    else:
        eprint("Unknown command %s" % args.command)
