#!/bin/bash

###############################################################################
#                                   EPOCH 0                                   #
###############################################################################

cd home

# start local validator
rm -rf tests/.keys/ test-ledger/ tests/__pycache__/ && \
    solana-test-validator --slots-per-epoch 150

# withdraw SOLs from local validator vote account to start fresh
solana withdraw-from-vote-account test-ledger/vote-account-keypair.json \
       $(solana-keygen pubkey) \
       999999.9 --authorized-withdrawer test-ledger/vote-account-keypair.json

# create instance
./tests/deploy_test_solido.py --verbose

# optional for test
cp ./solido_test.json ./solido_config.json

# start maintainer
./solido_v1/target/release/solido --config ./solido_config.json \
                      --keypair-path ./solido_v1/tests/.keys/maintainer.json \
                      run-maintainer --max-poll-interval-seconds 1

# deposit some SOL
./solido_v1/target/release/solido --config ./solido_config.json deposit --amount-sol 100

###############################################################################
#                                   EPOCH 1                                   #
###############################################################################

# receive some rewards

###############################################################################
#                                   EPOCH 2                                   #
###############################################################################

# create new v2 accounts
./solido_v2/target/release/solido \
    --output json \
    --config ./solido_test.json create-v2-accounts \
    --developer-account-owner 2d7gxHrVHw2grzWBdRQcWS7T1r9KnaaGXZBtzPBbzHEF \
    > v2_new_accounts.json
jq -s '.[0] * .[1]' v2_new_accounts.json ./solido_test.json > ./temp.json
mv ./temp.json ./solido_test.json

# load program to a buffer account
./solido_v2/scripts/operation.py \
    load-program --program-filepath ./solido_v2/target/deploy/lido.so --outfile buffer

# deactivate validators
./solido_v2/scripts/operation.py \
    deactivate-validators --keypair-path ./solido_v1/tests/.keys/maintainer.json --outfile deactivation_trx.txt

# verify transaction
./solido_v2/scripts/operation.py \
    check-transactions --phase deactivation --transactions-path deactivation_trx.txt


# batch sign transactions
./solido_v2/target/release/solido --config solido_config.json \
                      --keypair-path ./solido_v1/tests/.keys/maintainer.json \
                      multisig approve-batch --silent --transaction-addresses-path deactivation_trx.txt
# execute transactions one by one
./solido_v2/scripts/operation.py \
    execute-transactions --transactions deactivation_trx.txt \
    --keypair-path ./solido_v1/tests/.keys/maintainer.json \
    --phase deactivation

# create a new validator keys with a 5% commission
solana-keygen new --no-bip39-passphrase --force --silent \
              --outfile ./solido_v1/tests/.keys/vote-account-key.json
solana-keygen new --no-bip39-passphrase --force --silent \
              --outfile ./solido_v1/tests/.keys/vote-account-withdrawer-key.json
solana create-vote-account \
       ./solido_v1/tests/.keys/vote-account-key.json \
       ./solido_v1/test-ledger/validator-keypair.json \
       ./solido_v1/tests/.keys/vote-account-withdrawer-key.json --commission 5

###############################################################################
#                                   EPOCH 3                                   #
###############################################################################


# propose program upgrade
./solido_v2/target/release/solido --output json --config ./solido_config.json \
                      --keypair-path ./solido_v1/tests/.keys/maintainer.json \
                      multisig propose-upgrade \
                      --spill-address $(solana-keygen pubkey) \
                      --buffer-address "$(< ./buffer)" \
                      --program-address $(jq -r .solido_program_id ./solido_config.json) > tempfile
awk '/{/,/}/' tempfile | jq -r .transaction_address >> upgrade_trx.txt

# propose migration
./solido_v2/target/release/solido --output json --config ./solido_config.json \
                      --keypair-path ./solido_v1/tests/.keys/maintainer.json\
                      migrate-state-to-v2 --developer-fee-share 1 \
                      --treasury-fee-share 4 \
                      --st-sol-appreciation-share 95 \
                      --max-commission-percentage 5 > tempfile
awk '/{/,/}/' tempfile | jq -r .transaction_address >> upgrade_trx.txt

# verify transaction
./solido_v2/scripts/operation.py \
    check-transactions --phase upgrade --transactions-path upgrade_trx.txt

# wait for maintainers to remove validators, approve program update and migration
./solido_v2/target/release/solido --config ./solido_config.json \
                      --keypair-path ./solido_v1/tests/.keys/maintainer.json \
                      multisig approve-batch --transaction-addresses-path upgrade_trx.txt

# start a new maintainer
./target/debug/solido --config ../solido_test.json \
                      --keypair-path ../solido_old/tests/.keys/maintainer.json \
                      run-maintainer --max-poll-interval-seconds 1 \
                      --end-of-epoch-threshold 75

# add validators
solana-keygen pubkey ./solido_v1/tests/.keys/vote-account-key.json > validators.txt
./solido_v2/scripts/operation.py \
    add-validators --outfile adding_trx.txt \
    --vote-accounts validators.txt \
    --keypair-path ./solido_v1/tests/.keys/maintainer.json

# verify transaction
./solido_v2/scripts/operation.py \
    check-transactions --phase adding --transactions-path adding_trx.txt

# batch sign transactions
./solido_v2/target/release/solido --config ./solido_config.json \
                      --keypair-path ./solido_v1/tests/.keys/maintainer.json \
                      multisig approve-batch --silent --transaction-addresses-path adding_trx.txt
# execute transactions one by one
./solido_v2/scripts/operation.py \
    execute-transactions --transactions adding_trx.txt \
    --keypair-path ./solido_v1/tests/.keys/maintainer.json \
    --phase adding

###############################################################################
#                                   EPOCH 4                                   #
###############################################################################


# try to withdraw
./solido_v2/target/release/solido --config ./solido_config.json withdraw --amount-st-sol 1.1

# withdraw developer some fee to self
spl-token transfer --from DEVELOPER_FEE_ADDRESS STSOL_MINT_ADDRESS \
          0.0001 $(solana-keygen pubkey) --owner ~/developer_fee_key.json
# spl-token account-info --address DEVELOPER_FEE_ADDRESS
