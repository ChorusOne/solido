#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
This script verifies details provided by validators that are onboarding.

Takes tab-separated form responses on stdin. This script then verifies:

 * The vote account withdraw authority.
 * The vote account commission.
 * That the identity account of the vote account has the Keybase username as
   provided here.
 * That the public key of the identity account of the vote account has been
   added to Keybase.
 * That the name of the identity account matches the name provided here.
 * That the stSOL account has the correct mint.

The header row will be stripped.

This script is meant to be used as a one-off in the onboarding process, it does
not do proper error handling etc. It is expected to run on trusted input; verify
the tsv file manually to confirm that no weird Keybase usernames etc. are in there.
"""

import json
import subprocess

from typing import Any, Dict, Iterable, NamedTuple, Optional
from validator_onboarding import Address, ValidatorResponse, iter_rows_from_stdin


SOLIDO_AUTHORIZED_WITHDAWER = 'GgrQiJ8s2pfHsfMbEFtNcejnzLegzZ16c9XtJ2X2FpuF'
ST_SOL_MINT = '7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj'
VOTE_PROGRAM = 'Vote111111111111111111111111111111111111111'
SPL_TOKEN_PROGRAM = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'


def solana(*args: str) -> Any:
    full_args = ['solana', '--url', 'https://api.mainnet-beta.solana.com', *args]
    result = subprocess.run(
        full_args, check=True, capture_output=True, encoding='utf-8'
    )
    return json.loads(result.stdout)


class ValidatorInfo(NamedTuple):
    identity_address: Address
    info_address: Address
    keybase_username: Optional[str]
    name: Optional[str]


def iter_validator_infos() -> Iterable[ValidatorInfo]:
    """
    Return the validator info for all validators on mainnet.
    """
    for info in solana('validator-info', 'get', '--output', 'json'):
        yield ValidatorInfo(
            identity_address=info['identityPubkey'],
            info_address=info['infoPubkey'],
            keybase_username=info['info'].get('keybaseUsername'),
            name=info['info'].get('name'),
        )


class TokenAccount(NamedTuple):
    mint_address: Address
    state: str


def get_token_account(address: Address) -> Optional[TokenAccount]:
    cmd = [
        'spl-token',
        '--url',
        'https://api.mainnet-beta.solana.com',
        'account-info',
        '--address',
        address,
        '--output',
        'json',
    ]
    try:
        process = subprocess.run(cmd, check=True, capture_output=True, encoding='utf-8')
        result = json.loads(process.stdout)
        return TokenAccount(
            mint_address=result['mint'],
            state=result['state'],
        )
    except subprocess.CalledProcessError:
        return None


def get_account_owner(address: Address) -> Address:
    result = solana('account', address, '--output', 'json')
    owner: Address = result['account']['owner']
    return owner


def check_keybase_has_identity_address(
    username: str, identity_account_address: Address
) -> bool:
    """
    Check whether the given Keybase user has published a file with the given identity address.
    """
    assert '/' not in username
    assert '?' not in username
    assert '%' not in username
    assert '&' not in username
    assert '.' not in username
    # This is the url from which keybase serves the raw file. It serves a web-
    # based file browser at keybase.pub/{username}, but that one does not serve
    # a 404 when the file is missing, and the raw url does.
    url = f'https://{username}.keybase.pub/solana/validator-{identity_account_address}'
    # Previously I tried with Python's urllib, but it complains:
    #
    #    Hostname mismatch, certificate is not valid for 'bd_validators.keybase.pub'
    #
    # Chromium and Curl do not have any problems validating the certificate,
    # so I am going to assume it's an urllib problem, and just call Curl instead.
    cmd = ['curl', '--head', url]
    result = subprocess.run(cmd, check=True, capture_output=True, encoding='utf-8')
    # We know that Keybase serves http/2, so we can just hard-code this. The
    # trailing space is intentional.
    return result.stdout.splitlines()[0] == 'HTTP/2 200 '


class VoteAccount(NamedTuple):
    validator_identity_address: Address
    authorized_withdrawer: Address
    commission: int
    num_votes: int


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


def get_vote_account(self: ValidatorResponse) -> Optional[VoteAccount]:
    try:
        result = solana('vote-account', '--output', 'json', self.vote_account_address)
        return VoteAccount(
            validator_identity_address=result['validatorIdentity'],
            authorized_withdrawer=result['authorizedWithdrawer'],
            commission=result['commission'],
            num_votes=len(result['votes']),
        )

    except subprocess.CalledProcessError:
        return None


def check_validator_response(
    self: ValidatorResponse,
    validators_by_identity: Dict[Address, ValidatorInfo],
    vote_accounts: Dict[Address, str],
    identity_accounts: Dict[Address, str],
    st_sol_accounts: Dict[Address, str],
) -> None:
    print('\n' + self.validator_name)
    vote_account = get_vote_account(self)

    if vote_account is not None:
        print_ok('Vote account address holds a vote account.')
    else:
        print_error('Vote account address does not hold a vote account.')
        return

    if vote_account.authorized_withdrawer == SOLIDO_AUTHORIZED_WITHDAWER:
        print_ok('Authorized withdrawer set to Solido.')
    else:
        print_error('Wrong authorized withdrawer.')

    if vote_account.num_votes > 0:
        print_ok('Vote account has votes.')
    else:
        print_warn('Vote account has not voted yet.')

    if vote_account.commission == 100:
        print_ok('Vote account commission is 100%.')
    else:
        print_error('Vote account commission is not 100%.')

    validator_info = validators_by_identity.get(vote_account.validator_identity_address)
    if validator_info is None:
        print_error('Validator identity does not exist.')
        return
    else:
        print_ok('Validator identity exists.')

    if validator_info.keybase_username == self.keybase_username:
        print_ok('Keybase username in form matches username in identity account.')
    else:
        print_error('Keybase username in identity account does not match the form.')

    if validator_info.name is not None and validator_info.name.startswith('Lido / '):
        print_ok('Validator identity name starts with "Lido / ".')
    else:
        print_error('Validator identity name does not start with "Lido / ".')

    if validator_info.name == self.identity_name:
        print_ok('Name in identity account matches name in form.')
    else:
        print_error('Name in identity account does not mach name in form.')

    if check_keybase_has_identity_address(
        self.keybase_username, vote_account.validator_identity_address
    ):
        print_ok(
            f'Validator identity public key is on Keybase under {self.keybase_username}.'
        )
    else:
        print_error('Could not verify validator identity through Keybase.')

    token_account = get_token_account(self.st_sol_account_address)
    if token_account is not None:
        print_ok('Fee account exists.')
    else:
        print_error(f'Fee account {self.st_sol_account_address} does not exist.')
        return

    if token_account.state == "initialized":
        print_ok('Token account is in an initialized state.')
    else:
        print_error('Token account is not initialized state.')

    if token_account.mint_address == ST_SOL_MINT:
        print_ok('Fee account is an stSOL account (it has the correct mint).')
    else:
        print_error('Fee account is not an stSOL account, the mint is wrong.')

    name = vote_accounts.setdefault(self.vote_account_address, self.validator_name)
    if name != self.validator_name:
        print_error(f'Vote account is already in use by {name}.')
    else:
        print_ok('Vote account address is unique among responses seen so far.')

    name = identity_accounts.setdefault(
        vote_account.validator_identity_address, self.validator_name
    )
    if name != self.validator_name:
        print_error(f'Identity account is already in use by {name}.')
    else:
        print_ok('Identity account is unique among responses seen so far.')

    name = st_sol_accounts.setdefault(self.st_sol_account_address, self.validator_name)
    if name != self.validator_name:
        print_error(f'Fee stSOL account is already in use by {name}.')
    else:
        print_ok('Fee stSOL account is unique among responses seen so far.')

    if get_account_owner(self.vote_account_address) == VOTE_PROGRAM:
        print_ok('Vote account is owned by the vote program.')
    else:
        print_error('Vote account is not owned by the vote program.')

    if get_account_owner(self.st_sol_account_address) == SPL_TOKEN_PROGRAM:
        print_ok('Fee stSOL account is owned by the SPL token program.')
    else:
        print_error('Fee stSOL account is not owned by the SPL token program.')


def main() -> None:
    # Build a map of validators by identity address.
    validators_by_identity: Dict[str, ValidatorInfo] = {
        info.identity_address: info for info in iter_validator_infos()
    }

    # We expect all validators to use different vote accounts, identity accounts,
    # and fee accounts. Track them in sets, so we can report an error if there is
    # a duplicate. This works for the initial onboarding; if we add more validators
    # later, we would also need to add the current validators here.
    vote_accounts: Dict[str, str] = {}
    identity_accounts: Dict[str, str] = {}
    st_sol_accounts: Dict[str, str] = {}

    for response in iter_rows_from_stdin():
        check_validator_response(
            response,
            validators_by_identity,
            vote_accounts,
            identity_accounts,
            st_sol_accounts,
        )


if __name__ == '__main__':
    main()
