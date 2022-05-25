#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
Set up a Solido and Anker instance on Solana devnet or a local testnet, and print
its details. Anker requires a Wormhole deployment for full functionality, which
does exist on devnet, but which is difficult to recreate locally. Everything
aside from sending rewards can be tested locally though.

"""

import json
import os
import subprocess
import sys

from uuid import uuid4

from util import (
    create_test_account,
    get_approve_and_execute,
    get_network,
    get_solido_program_path,
    multisig,
    rpc_get_account_info,
    solana,
    solana_program_deploy,
    solido,
    spl_token,
)

DEVNET_ORCA_PROGRAM_ID = '3xQ8SWv2GaFXXpHZNqkXsdxq5DZciHBz6ZFoPPfbFd7U'
DEVNET_WORMHOLE_UST_MINT = '5Dmmc5CC6ZpKif8iN5DSY9qNYrWJvEKcX2JrxGESqRMu'
DEVNET_TERRA_REWARDS_ADDRESS = 'terra1uwlxcas745mwjte8wwu2l0fcs483twujnt8j5l'
DEVNET_WORMHOLE_CORE_BRIDGE_PROGRAM_ID = '3u8hJUVTA4jH1wYAyUur7FFZVQ8H635K3tSHHF4ssjQ5'
DEVNET_WORMHOLE_TOKEN_BRIDGE_PROGRAM_ID = 'DZnkkTmCiFWfYTfT41X3Rd1kDgozqzxWaHqsw6W4x2oe'

# Create a fresh directory where we store all the keys and configuration for this
# deployment.
run_id = uuid4().hex[:10]
test_dir = f'tests/.keys/{run_id}'
os.makedirs(test_dir, exist_ok=True)
print(f'Keys directory: {test_dir}')

# Before we start, check our current balance. We also do this at the end,
# and then we know how much the deployment cost.
sol_balance_pre = float(solana('balance').split(' ')[0])

# Start with the UST accounts, because on devnet we can't create UST, we need to
# receive it externally, and if we don't have the UST yet, then don't waste time
# by uploading programs (which is slow) and only later failing, we can fail fast.
# If we are on devnet, then Wormhole UST exists already, and we use that address.
# Otherwise, we create a new SPL token and pretend that that's UST.
ust_mint_keypair = None
if rpc_get_account_info(DEVNET_WORMHOLE_UST_MINT) is None:
    print('\nSetting up UST mint ...')
    ust_mint_keypair = create_test_account(f'{test_dir}/ust-mint.json', fund=False)
    spl_token('create-token', f'{test_dir}/ust-mint.json', '--decimals', '6')
    ust_mint_address = ust_mint_keypair.pubkey
else:
    print('\nFound existing devnet Wormhole UST mint.')
    ust_mint_address = DEVNET_WORMHOLE_UST_MINT
print(f'> UST mint is {ust_mint_address}.')

try:
    ust_account_info_json = spl_token(
        'create-account', ust_mint_address, '--output', 'json'
    )
except subprocess.CalledProcessError:
    # "spl-token create-account" fails if the associated token account exists
    # already. It would be nice to check whether it exists before we try to
    # create it, but unfortunately there appears to be no way to get the address
    # of the associated token account, either through the Solana RPC, or through
    # one of the command-line tools. The associated token account address remains
    # implicit everywhere :/
    pass

# If we control the UST mint (on a local test validator), we can mint ourselves
# 0.1 UST. But if we don't control the mint, then we need to be sure that we have
# some to start with.
if ust_mint_keypair is not None:
    spl_token('mint', ust_mint_keypair.pubkey, '0.1')
    print('> Minted ourselves 0.1 UST.')
else:
    ust_balance_json = spl_token('balance', ust_mint_address, '--output', 'json')
    ust_balance_dict = json.loads(ust_balance_json)
    ust_balance_micro_ust = int(ust_balance_dict['amount'])
    if ust_balance_micro_ust < 100_000:
        print('Please ensure that your associated token account has at least 0.1 UST.')
        owner_addr = solana('address').strip()
        print(f'It should go into the associated token account of {owner_addr}.')
        sys.exit(1)
    else:
        print(
            '> We have sufficient UST to proceed: '
            f'{ust_balance_micro_ust / 1e6:.6f} >= 0.1.'
        )

print('\nUploading Multisig program ...')
multisig_program_id = solana_program_deploy(
    get_solido_program_path() + '/serum_multisig.so'
)
print(f'> Multisig program id is {multisig_program_id}')

print('\nUploading Solido program ...')
solido_program_id = solana_program_deploy(get_solido_program_path() + '/lido.so')
print(f'> Solido program id is {solido_program_id}')

print('\nUploading Anker program ...')
anker_program_id = solana_program_deploy(get_solido_program_path() + '/anker.so')
print(f'> Anker program id is {anker_program_id}')

# If the Orca program exists, use that, otherwise upload it at a new address.
orca_info = rpc_get_account_info(DEVNET_ORCA_PROGRAM_ID)
if orca_info is not None:
    print('\nFound existing instance of Orca Token Swap program.')
    token_swap_program_id = DEVNET_ORCA_PROGRAM_ID
else:
    print('\nUploading Orca Token Swap program ...')
    token_swap_program_id = solana_program_deploy(
        get_solido_program_path() + '/orca_token_swap_v2.so'
    )
print(f'> Token swap program id is {token_swap_program_id}')

maintainer = create_test_account(test_dir + '/maintainer.json')
st_sol_accounts_owner = create_test_account(test_dir + '/st-sol-accounts-owner.json')

print('\nCreating new multisig ...')
multisig_data = multisig(
    'create-multisig',
    '--multisig-program-id',
    multisig_program_id,
    '--threshold',
    '1',
    '--owners',
    maintainer.pubkey,
)
multisig_instance = multisig_data['multisig_address']
multisig_pda = multisig_data['multisig_program_derived_address']
print(f'> Created instance at {multisig_instance}')

print('\nCreating Solido instance ...')
result = solido(
    'create-solido',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--max-validators',
    '9',
    '--max-maintainers',
    '3',
    '--max-validation-fee',
    '5',
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
    multisig_instance,
    keypair_path=maintainer.keypair_path,
)

solido_address = result['solido_address']
st_sol_mint_address = result['st_sol_mint_address']
print(f'> Created instance at {solido_address}')

approve_and_execute = get_approve_and_execute(
    multisig_program_id=multisig_program_id,
    multisig_instance=multisig_instance,
    signer_keypair_paths=[maintainer.keypair_path],
)

print('\nAdding maintainer ...')
transaction_result = solido(
    'add-maintainer',
    '--multisig-program-id',
    multisig_program_id,
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--maintainer-address',
    maintainer.pubkey,
    '--multisig-address',
    multisig_instance,
    keypair_path=maintainer.keypair_path,
)
approve_and_execute(transaction_result['transaction_address'])
print(f'> Maintainer {maintainer.pubkey} added.')

# Next up is the token pool, but to be able to set that up,
# we need some stSOL (and some UST, which we have already),
# and then we need to put that in some new accounts that the
# pool will take ownership of.
print('\nSetting up stSOL-UST pool ...')
solido(
    'deposit',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--amount-sol',
    '0.1',
)
pool_ust_keypair = create_test_account(f'{test_dir}/pool-ust.json', fund=False)
pool_st_sol_keypair = create_test_account(f'{test_dir}/pool-st-sol.json', fund=False)
spl_token('create-account', ust_mint_address, pool_ust_keypair.keypair_path)
spl_token('create-account', st_sol_mint_address, pool_st_sol_keypair.keypair_path)
spl_token('transfer', ust_mint_address, '0.1', pool_ust_keypair.pubkey)
spl_token('transfer', st_sol_mint_address, '0.1', pool_st_sol_keypair.pubkey)
result = solido(
    'anker',
    'create-token-pool',
    '--token-swap-program-id',
    token_swap_program_id,
    '--ust-mint-address',
    ust_mint_address,
    '--ust-account',
    pool_ust_keypair.pubkey,
    '--st-sol-account',
    pool_st_sol_keypair.pubkey,
)
token_pool_address = result['pool_address']
print(f'Pool address is {token_pool_address}.')

print('\nCreating Anker instance ...')
result = solido(
    'anker',
    'create',
    '--solido-program-id',
    solido_program_id,
    '--solido-address',
    solido_address,
    '--anker-program-id',
    anker_program_id,
    '--ust-mint-address',
    ust_mint_address,
    '--token-swap-pool',
    token_pool_address,
    '--wormhole-core-bridge-program-id',
    DEVNET_WORMHOLE_CORE_BRIDGE_PROGRAM_ID,
    '--wormhole-token-bridge-program-id',
    DEVNET_WORMHOLE_TOKEN_BRIDGE_PROGRAM_ID,
    '--terra-rewards-address',
    DEVNET_TERRA_REWARDS_ADDRESS,
)
anker_address = result['anker_address']
print(f'> Created instance at {anker_address}.')

sol_balance_post = float(solana('balance').split(' ')[0])
total_cost_sol = sol_balance_pre - sol_balance_post
print(f'\nDeployment cost {total_cost_sol:.3f} SOL.')

# Save the configuration to a file, to make it easier to run the maintainer
# and other commands later.
config = {
    'keypair_path': test_dir + '/maintainer.json',
    'cluster': get_network(),
    'multisig_program_id': multisig_program_id,
    'multisig_address': multisig_instance,
    'solido_program_id': solido_program_id,
    'solido_address': solido_address,
    'anker_program_id': anker_program_id,
    'anker_address': anker_address,
    'max_poll_interval_seconds': 10,
}
with open(test_dir + '/config.json', 'w', encoding='utf-8') as config_file:
    json.dump(config, config_file, indent=2)


print('\nMaintenance command line:')
print(f'solido --config {test_dir}/config.json run-maintainer')
