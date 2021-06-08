#!/usr/bin/env python3

"""
This script calls 'solana' and 'solido' to confirm that functionality works.

It exits with exit code 0 if everything works as expected, or with a nonzero
exit code if anything fails. It expects a test validator to be running at at the
default localhost port, and it expects a keypair at ~/.config/solana/id.json
that corresponds to a sufficiently funded account.
"""

import sys
import json

from util import create_test_account, create_test_accounts, solana_program_deploy, create_stake_account, create_spl_token, create_vote_account, solido, get_multisig, solana, approve_and_execute


# We start by generating three accounts that we will need later.
print('Creating test accounts ...')
addrs = create_test_accounts(num_accounts=2)
print(f'> {addrs}')

treasury_account_owner = create_test_account('treasury-key.json')
print(f'> Treasury account owner:    {treasury_account_owner}')

insurance_account_owner = create_test_account('insurance-key.json')
print(f'> Insurance account owner:   {insurance_account_owner}')

manager_fee_account_owner = create_test_account('manager-fee-key.json')
print(f'> Manager fee account owner: {manager_fee_account_owner}')


print('\nUploading stake pool program ...')
stake_pool_program_id = solana_program_deploy(
    'target/deploy/spl_stake_pool.so')
print(f'> Stake pool program id is {stake_pool_program_id}.')


print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy('target/deploy/lido.so')
print(f'> Solido program id is {solido_program_id}.')

print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy('target/deploy/multisig.so')
print(f'> Multisig program id is {multisig_program_id}.')
multisig = get_multisig(multisig_program_id)

print('\nCreating new multisig ...')
multisig_data = multisig(
    'create-multisig',
    '--threshold', '2',
    '--owner', addrs[0].pubkey,
    '--owner', addrs[1].pubkey,
)
multisig_instance = multisig_data['multisig_address']
multisig_pda = multisig_data['multisig_program_derived_address']
print(f'> Created instance at {multisig_instance}.')

print('\nCreating Solido instance ...')
result = solido(
    'create-solido',
    '--stake-pool-program-id', stake_pool_program_id,
    '--solido-program-id', solido_program_id,
    '--fee-numerator', '4',
    '--fee-denominator', '31',
    '--max-validators', '9',
    '--max-maintainers', '1',
    '--insurance-fee', '7',
    '--treasury-fee', '5',
    '--validation-fee', '3',
    '--manager-fee', '2',
    '--treasury-account-owner', treasury_account_owner,
    '--insurance-account-owner', insurance_account_owner,
    '--manager-fee-account-owner', manager_fee_account_owner,
    '--manager', multisig_pda,
    keypair_path='test-key-1.json'
)
solido_address = result['solido_address']
treasury_account = result['treasury_account']
insurance_account = result['insurance_account']
manager_fee_account = result['manager_fee_account']
st_sol_mint_account = result['st_sol_mint_address']

print(f'> Created instance at {solido_address}.')

solido_instance = solido('show-solido',
                         '--solido-program-id', solido_program_id,
                         '--solido-address', solido_address,
                         )
assert solido_instance['solido']['manager'] == multisig_pda
assert solido_instance['solido']['st_sol_total_shares'] == 0
assert solido_instance['solido']['fee_distribution'] == {
    'insurance_fee': 7,
    'treasury_fee': 5,
    'validation_fee': 3,
    'manager_fee': 2
}

print('\nAdding a validator ...')
validator_token_account_owner = create_test_account(
    'validator-token-account-key.json')
print(f'> Validator token account owner: {validator_token_account_owner}')

validator_stake_account = create_stake_account(
    'validator-stake-account-key.json')
print(f'> Validator stake account: {validator_stake_account}')

validator = create_test_account(
    'validator-account-key.json')

validator_vote_account = create_vote_account(
    'validator-vote-account-key.json', 'validator-account-key.json')
print(
    f'> Creating validator vote account {validator_vote_account}')

print(
    f'> Creating validator token account with owner {validator_token_account_owner}')

# Create SPL token
validator_token_account = create_spl_token(
    'validator-token-account-key.json', st_sol_mint_account)
print(f'> Validator stSol token account: {validator_token_account}')
print('Creating validator stake account')
transaction_result = solido('create-validator-stake-account',
                            '--solido-program-id', solido_program_id,
                            '--solido-address', solido_address,
                            '--stake-pool-program-id', stake_pool_program_id,
                            '--validator-vote', validator_vote_account,
                            '--multisig-program-id', multisig_program_id,
                            '--multisig-address', multisig_instance,
                            keypair_path='test-key-1.json')
transaction_address = transaction_result['transaction_address']
# Fund the PDA so we transfer from it in the create-validator-stake-account instruction
solana('transfer', '--allow-unfunded-recipient', multisig_pda, '10.0')
print(f'> Approving transaction: {transaction_address}')
multisig('approve',
         '--multisig-address', multisig_instance,
         '--transaction-address', transaction_address,
         keypair_path='test-key-2.json'
         )
print(f'> Executing transaction: {transaction_address}')
multisig('execute-transaction',
         '--multisig-address', multisig_instance,
         '--transaction-address', transaction_address,
         keypair_path='test-key-1.json'
         )
stake_account_pda = multisig('show-transaction',
                             '--solido-program-id', solido_program_id,
                             '--transaction-address', transaction_address)

print('> Call function to add validator')
transaction_result = solido('add-validator',
                            '--solido-program-id', solido_program_id,
                            '--solido-address', solido_address,
                            '--stake-pool-program-id', stake_pool_program_id,
                            '--validator-vote', validator_vote_account,
                            '--validator-rewards-address', validator_token_account,
                            '--multisig-program-id', multisig_program_id,
                            '--multisig-address', multisig_instance,
                            keypair_path='test-key-1.json'
                            )
transaction_address = transaction_result['transaction_address']
transaction_status = multisig(
    'show-transaction',
    '--transaction-address', transaction_address,
)
assert transaction_status['did_execute'] == False
assert transaction_status['signers']['Current']['signers'].count(
    {'owner': addrs[0].pubkey, 'did_sign': True}) == 1
approve_and_execute(multisig,
                    multisig_instance, transaction_address, 'test-key-2.json')
transaction_status = multisig(
    'show-transaction',
    '--transaction-address', transaction_address,
)
assert transaction_status['did_execute'] == True
assert transaction_status['signers']['Current']['signers'].count(
    {'owner': addrs[1].pubkey, 'did_sign': True}) == 1


solido_instance = solido('show-solido',
                         '--solido-program-id', solido_program_id,
                         '--solido-address', solido_address)

assert solido_instance['solido']['validators']['entries'][0] == {
    'pubkey': stake_account_pda['parsed_instruction']['SolidoInstruction']['CreateValidatorStakeAccount']['stake_account'],
    'entry': {
        'fee_credit': 0,
        'fee_address': validator_token_account
    }
}

maintainer = create_test_account('maintainer-account-key.json')

print(f'\nAdd and remove maintainer ...')
print(f'> Adding maintainer {maintainer}')

transaction_result = solido('add-maintainer',
                            '--solido-program-id', solido_program_id,
                            '--solido-address', solido_address,
                            '--maintainer-address', maintainer,
                            '--multisig-program-id', multisig_program_id,
                            '--multisig-address', multisig_instance,
                            keypair_path='test-key-1.json'
                            )
transaction_address = transaction_result['transaction_address']
approve_and_execute(multisig,
                    multisig_instance, transaction_address, 'test-key-2.json')

solido_instance = solido('show-solido',
                         '--solido-program-id', solido_program_id,
                         '--solido-address', solido_address)
assert solido_instance['solido']['maintainers']['entries'][0] == {
    'pubkey': maintainer,
    'entry': None
}

print(f'> Removing maintainer {maintainer}')
transaction_result = solido('remove-maintainer',
                            '--solido-program-id', solido_program_id,
                            '--solido-address', solido_address,
                            '--maintainer-address', maintainer,
                            '--multisig-program-id', multisig_program_id,
                            '--multisig-address', multisig_instance,
                            keypair_path='test-key-1.json'
                            )
transaction_address = transaction_result['transaction_address']
approve_and_execute(multisig,
                    multisig_instance, transaction_address, 'test-key-2.json')
solido_instance = solido('show-solido',
                         '--solido-program-id', solido_program_id,
                         '--solido-address', solido_address)

assert len(solido_instance['solido']['maintainers']['entries']) == 0

print(f'> Adding maintainer {maintainer} again')
transaction_result = solido('add-maintainer',
                            '--solido-program-id', solido_program_id,
                            '--solido-address', solido_address,
                            '--maintainer-address', maintainer,
                            '--multisig-program-id', multisig_program_id,
                            '--multisig-address', multisig_instance,
                            keypair_path='test-key-1.json'
                            )
transaction_address = transaction_result['transaction_address']
approve_and_execute(multisig,
                    multisig_instance, transaction_address, 'test-key-2.json')
