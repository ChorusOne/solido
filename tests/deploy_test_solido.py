#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
Set up a Solido instance on a local testnet, and print its details. This is
useful when testing the maintenance daemon locally.
"""

import json
import os
from typing import Optional, Dict, Any

from util import (
    create_test_account,
    solana_program_deploy,
    create_spl_token_account,
    create_vote_account,
    get_network,
    solana,
    solido,
    multisig,
    get_approve_and_execute,
    get_solido_program_path,
    MAX_VALIDATION_COMMISSION_PERCENTAGE,
)


class Instance:
    def __init__(self) -> None:
        print('\nUploading Solido program ...')
        self.solido_program_id = solana_program_deploy(
            get_solido_program_path() + '/lido.so'
        )
        print(f'> Solido program id is {self.solido_program_id}')

        print('\nUploading Multisig program ...')
        self.multisig_program_id = solana_program_deploy(
            get_solido_program_path() + '/serum_multisig.so'
        )
        print(f'> Multisig program id is {self.multisig_program_id}')

        os.makedirs('tests/.keys', exist_ok=True)
        self.maintainer = create_test_account('tests/.keys/maintainer.json')
        st_sol_accounts_owner = create_test_account(
            'tests/.keys/st-sol-accounts-owner.json'
        )

        print('\nCreating new multisig ...')
        multisig_data = multisig(
            'create-multisig',
            '--multisig-program-id',
            self.multisig_program_id,
            '--threshold',
            '1',
            '--owners',
            self.maintainer.pubkey,
        )
        self.multisig_instance = multisig_data['multisig_address']
        multisig_pda = multisig_data['multisig_program_derived_address']
        print(f'> Created instance at {self.multisig_instance}')

        print('\nCreating Solido instance ...')
        result = solido(
            'create-solido',
            '--multisig-program-id',
            self.multisig_program_id,
            '--solido-program-id',
            self.solido_program_id,
            '--max-validators',
            '9',
            '--max-maintainers',
            '3',
            '--max-commission-percentage',
            str(MAX_VALIDATION_COMMISSION_PERCENTAGE),
            '--treasury-fee-share',
            '5',
            '--developer-fee-share',
            '2',
            '--st-sol-appreciation-share',
            '93',
            '--treasury-account-owner',
            st_sol_accounts_owner.pubkey,
            '--developer-account-owner',
            st_sol_accounts_owner.pubkey,
            '--multisig-address',
            self.multisig_instance,
            keypair_path=self.maintainer.keypair_path,
        )

        self.solido_address = result['solido_address']
        self.treasury_account = result['treasury_account']
        self.developer_account = result['developer_account']
        self.st_sol_mint_account = result['st_sol_mint_address']
        self.validator_list_address = result['validator_list_address']
        self.maintainer_list_address = result['maintainer_list_address']

        print(f'> Created instance at {self.solido_address}')

        solido_instance = self.pull_solido()
        solana(
            'program',
            'set-upgrade-authority',
            '--new-upgrade-authority',
            solido_instance['solido']['manager'],
            self.solido_program_id,
        )

        self.approve_and_execute = get_approve_and_execute(
            multisig_program_id=self.multisig_program_id,
            multisig_instance=self.multisig_instance,
            signer_keypair_paths=[self.maintainer.keypair_path],
        )

        # For the first validator, add the test validator itself, so we include a
        # validator that is actually voting, and earning rewards.
        current_validators = json.loads(solana('validators', '--output', 'json'))

        # If we're running on localhost, change the comission
        if get_network() == 'http://127.0.0.1:8899':
            solido_instance = self.pull_solido()
            print(
                '> Changing validator\'s comission to {}% ...'.format(
                    MAX_VALIDATION_COMMISSION_PERCENTAGE
                )
            )
            validator = current_validators['validators'][0]
            validator['commission'] = str(MAX_VALIDATION_COMMISSION_PERCENTAGE)
            solana(
                'vote-update-commission',
                validator['voteAccountPubkey'],
                str(MAX_VALIDATION_COMMISSION_PERCENTAGE),
                './test-ledger/vote-account-keypair.json',
            )
            solana(
                'validator-info',
                'publish',
                '--keypair',
                './test-ledger/validator-keypair.json',
                "solana-test-validator",
            )

        # Allow only validators that are voting, have 100% commission, and have their
        # withdrawer set to Solido's rewards withdraw authority. On a local testnet,
        # this will only contain the test validator, but on devnet or testnet, there can
        # be more validators.
        active_validators = [
            v
            for v in current_validators['validators']
            if (not v['delinquent'])
            and v['commission'] == str(MAX_VALIDATION_COMMISSION_PERCENTAGE)
        ]

        # Add up to 5 of the active validators. Locally there will only be one, but on
        # the devnet or testnet there can be more, and we don't want to add *all* of them.
        validators = [
            self.add_validator(i, vote_account=v['voteAccountPubkey'])
            for (i, v) in enumerate(active_validators[:5])
        ]

        # Create two validators of our own, so we have a more interesting stake
        # distribution. These validators are not running, so they will not earn
        # rewards.
        # validators.extend(
        #     self.add_validator(i, vote_account=None)
        #     for i in range(len(validators), len(validators) + 1)
        # )

        print('Adding maintainer ...')
        transaction_result = solido(
            'add-maintainer',
            '--multisig-program-id',
            self.multisig_program_id,
            '--solido-program-id',
            self.solido_program_id,
            '--solido-address',
            self.solido_address,
            '--maintainer-address',
            self.maintainer.pubkey,
            '--multisig-address',
            self.multisig_instance,
            keypair_path=self.maintainer.keypair_path,
        )
        self.approve_and_execute(transaction_result['transaction_address'])

        output = {
            "cluster": get_network(),
            "multisig_program_id": self.multisig_program_id,
            "multisig_address": self.multisig_instance,
            "solido_program_id": self.solido_program_id,
            "solido_address": self.solido_address,
            "st_sol_mint": self.st_sol_mint_account,
        }
        print("Config file is ../solido_test.json")
        with open('../solido_test.json', 'w') as outfile:
            json.dump(output, outfile, indent=4)

        for i, vote_account in enumerate(validators):
            print(f'  Validator {i} vote account: {vote_account}')

        print('\nMaintenance command line:')
        print(
            ' ',
            ' '.join(
                [
                    'solido',
                    '--keypair-path',
                    self.maintainer.keypair_path,
                    '--config',
                    '../solido_test.json',
                    'run-maintainer',
                    '--max-poll-interval-seconds',
                    '10',
                ]
            ),
        )

    def pull_solido(self) -> Any:
        return solido(
            'show-solido',
            '--solido-program-id',
            self.solido_program_id,
            '--solido-address',
            self.solido_address,
        )

    def add_validator(self, index: int, vote_account: Optional[str]) -> str:
        """
        Add a validator to the instance, create the right accounts for it. The vote
        account can be a pre-existing one, but if it is not provided, we will create
        one. Returns the vote account address.
        """
        print(f'\nCreating validator {index} ...')

        if vote_account is None:
            solido_instance = self.pull_solido()
            validator = create_test_account(
                f'tests/.keys/validator-{index}-account.json'
            )
            validator_vote_account, _ = create_vote_account(
                f'tests/.keys/validator-{index}-vote-account.json',
                validator.keypair_path,
                f'tests/.keys/validator-{index}-withdraw-account.json',
                MAX_VALIDATION_COMMISSION_PERCENTAGE,
            )
            vote_account = validator_vote_account.pubkey

        print(f'> Validator vote account:        {vote_account}')

        print('Adding validator ...')
        transaction_result = solido(
            'add-validator',
            '--multisig-program-id',
            self.multisig_program_id,
            '--solido-program-id',
            self.solido_program_id,
            '--solido-address',
            self.solido_address,
            '--validator-vote-account',
            vote_account,
            '--multisig-address',
            self.multisig_instance,
            keypair_path=self.maintainer.keypair_path,
        )
        self.approve_and_execute(transaction_result['transaction_address'])
        return vote_account


if __name__ == "__main__":
    Instance()
