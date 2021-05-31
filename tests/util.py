"""
Utilities that help writing tests, mainly for invoking programs.
"""

import json
import os.path
import subprocess
import sys

from typing import List, NamedTuple, Any, Optional


def run(*args: str) -> str:
    """
    Run a program, ensure it exits with code 0, return its stdout.
    """
    try:
        result = subprocess.run(
            args, check=True, capture_output=True, encoding='utf-8')

    except subprocess.CalledProcessError as err:
        # If a test fails, it is helpful to print stdout and stderr here, but
        # we don't print them by default because some calls are expected to
        # fail, and we don't want to pollute the output in that case, because
        # a log full of errors makes it difficult to locate the actual error in
        # the noise.
        if '--verbose' in sys.argv:
            print('Command failed:', ' '.join(args))
            print('Stdout:', err.stdout)
            print('Stderr:', err.stderr)
        raise

    return result.stdout


def solido(*args: str, keypair_path: Optional[str] = None) -> Any:
    """
    Run 'solido' against localhost, return its parsed json output.
    """

    output = run(
        'target/debug/solido',
        '--cluster', 'localnet',
        '--output', 'json',
        *([] if keypair_path is None else ['--keypair-path', keypair_path]),
        *args,
    )

    if output == '':
        return {}
    else:
        try:
            return json.loads(output)
        except json.JSONDecodeError:
            print('Failed to decode output as json, output was:')
            print(output)
            raise


def solana(*args: str) -> str:
    """
    Run 'solana' against localhost.
    """
    return run('solana', '--url', 'localhost', *args)


def spl_token(*args: str) -> str:
    """
    Run 'spl_token' against localhost.
    """
    return run('spl-token', '--url', 'localhost', *args)


def solana_program_deploy(fname: str) -> str:
    """
    Deploy a .so file, return its program id.
    """
    assert os.path.isfile(fname)
    result = solana('program', 'deploy', '--output', 'json', fname)
    program_id: str = json.loads(result)['programId']
    return program_id


class SolanaProgramInfo(NamedTuple):
    program_id: str
    owner: str
    program_data_address: str
    upgrade_authority: str
    last_deploy_slot: int
    data_len: int


def solana_program_show(program_id: str) -> SolanaProgramInfo:
    """
    Return information about a program.,
    """
    result = solana('program', 'show', '--output', 'json', program_id)
    data: Dict[str, Any] = json.loads(result)
    return SolanaProgramInfo(
        program_id=data['programId'],
        owner=data['owner'],
        program_data_address=data['programdataAddress'],
        upgrade_authority=data['authority'],
        last_deploy_slot=data['lastDeploySlot'],
        data_len=data['dataLen'],
    )


def create_test_account(keypair_fname: str, *, fund: bool = True) -> str:
    """
    Generate a key pair, fund the account with 1 SOL, and return its public key.
    """
    run(
        'solana-keygen',
        'new',
        '--no-bip39-passphrase',
        '--force',
        '--silent',
        '--outfile',
        keypair_fname,
    )
    pubkey = run('solana-keygen', 'pubkey', keypair_fname).strip()
    if fund:
        solana('transfer', '--allow-unfunded-recipient', pubkey, '10.0')
    return pubkey


def create_stake_account(keypair_fname: str) -> str:
    """
    Generate a stake account funded with 2 Sol, returns its public key.
    """
    pubkey = create_test_account(keypair_fname, fund=False)
    solana(
        'create-stake-account',
        keypair_fname,
        '2',
    )
    return pubkey


def create_vote_account(vote_key_fname: str, validator_key_fname: str):
    """
    Generate a vote account for the validator
    """
    pubkey = create_test_account(vote_key_fname, fund=False)
    solana(
        'create-vote-account',
        vote_key_fname,
        validator_key_fname,
    )
    return pubkey


def create_spl_token(owner_keypair_fname: str, minter: str) -> str:
    """
    Creates an spl token for the given minter
    Returns 'Creating account <address>
             Signature: <tx-signature>'
    """
    return spl_token('create-account', minter, '--owner', owner_keypair_fname).split('\n')[0].split(' ')[2]


class TestAccount(NamedTuple):
    pubkey: str
    keypair_path: str


def create_test_accounts(*, num_accounts: int) -> List[TestAccount]:
    result = []

    for i in range(num_accounts):
        fname = f'test-key-{i + 1}.json'
        pubkey = addr1 = create_test_account(fname)
        result.append(TestAccount(pubkey, fname))

    return result
