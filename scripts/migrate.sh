#!/bin/bash

###############################################################################
#                                   EPOCH 0                                   #
###############################################################################

cd solido_old

# start local validator
rm -rf tests/.keys/ test-ledger/ tests/__pycache__/ && \
    solana-test-validator --slots-per-epoch 150

# withdraw SOLs from local validator vote account to start fresh
solana withdraw-from-vote-account test-ledger/vote-account-keypair.json \
       $(solana-keygen pubkey) \
       999999.9 --authorized-withdrawer test-ledger/vote-account-keypair.json

# create instance
./tests/deploy_test_solido.py --verbose

# start maintainer
./target/debug/solido --config ../solido_test.json \
                      --keypair-path ../solido_old/tests/.keys/maintainer.json \
                      run-maintainer --max-poll-interval-seconds 1

# deposit some SOL
./target/debug/solido --config ../solido_test.json deposit --amount-sol 100

###############################################################################
#                                   EPOCH 1                                   #
###############################################################################

# receive some rewards

###############################################################################
#                                   EPOCH 2                                   #
###############################################################################

# create new v2 accounts
../solido/target/debug/solido \
    --output json \
    --config ../solido_test.json create-v2-accounts \
    --developer-account-owner 2d7gxHrVHw2grzWBdRQcWS7T1r9KnaaGXZBtzPBbzHEF \
    > v2_new_accounts.json
jq -s '.[0] * .[1]' v2_new_accounts.json ../solido_test.json > ../temp.json
mv ../temp.json ../solido_test.json

# load program to a buffer account
../solido/scripts/operation.py \
    --config ../solido_test.json \
    load-program --program-filepath ../solido/target/deploy/lido.so --outfile buffer

# deactivate validators
../solido/scripts/operation.py \
    --config ../solido_test.json \
    deactivate-validators --keypair-path ./tests/.keys/maintainer.json --outfile output
# batch sign transactions
./target/debug/solido --config ../solido_test.json \
                      --keypair-path ../solido_old/tests/.keys/maintainer.json \
                      multisig approve-batch --silent --transaction-addresses-path output
# execute transactions one by one
../solido/scripts/operation.py \
    --config ../solido_test.json \
    execute-transactions --transactions output --keypair-path ./tests/.keys/maintainer.json

# create a new validator keys with a 5% commission
solana-keygen new --no-bip39-passphrase --force --silent \
              --outfile ../solido_old/tests/.keys/vote-account-key.json
solana-keygen new --no-bip39-passphrase --force --silent \
              --outfile ../solido_old/tests/.keys/vote-account-withdrawer-key.json
solana create-vote-account \
       ../solido_old/tests/.keys/vote-account-key.json \
       ../solido_old/test-ledger/validator-keypair.json \
       ../solido_old/tests/.keys/vote-account-withdrawer-key.json --commission 5

cd ../solido

###############################################################################
#                                   EPOCH 3                                   #
###############################################################################


# propose program upgrade
./target/debug/solido --output json --config ../solido_test.json \
                      --keypair-path ../solido_old/tests/.keys/maintainer.json \
                      multisig propose-upgrade \
                      --spill-address $(solana-keygen pubkey) \
                      --buffer-address "$(< ../solido_old/buffer)" \
                      --program-address $(jq -r .solido_program_id ../solido_test.json) \
    | jq -r .transaction_address > output

# propose migration
./target/debug/solido --output json --config ../solido_test.json \
                      --keypair-path ../solido_old/tests/.keys/maintainer.json\
                      migrate-state-to-v2 --developer-fee-share 1 \
                      --treasury-fee-share 4 \
                      --st-sol-appreciation-share 95 \
                      --max-commission-percentage 5 \
    | jq -r .transaction_address >> output

# wait for maintainers to remove validators, approve program update and migration
./target/debug/solido --config ../solido_test.json \
                      --keypair-path ../solido_old/tests/.keys/maintainer.json \
                      multisig approve-batch --transaction-addresses-path output

# start a new maintainer
./target/debug/solido --config ../solido_test.json \
                      --keypair-path ../solido_old/tests/.keys/maintainer.json \
                      run-maintainer --max-poll-interval-seconds 1 \
                      --end-of-epoch-threshold 75

# add validators
solana-keygen pubkey ../solido_old/tests/.keys/vote-account-key.json > validators.txt
../solido/scripts/operation.py \
    --config ../solido_test.json \
    add-validators --outfile output \
    --vote-accounts validators.txt \
    --keypair-path ../solido_old/tests/.keys/maintainer.json
# batch sign transactions
./target/debug/solido --config ../solido_test.json \
                      --keypair-path ../solido_old/tests/.keys/maintainer.json \
                      multisig approve-batch --silent --transaction-addresses-path output
# execute transactions one by one
../solido/scripts/operation.py \
    --config ../solido_test.json \
    execute-transactions --transactions output --keypair-path ../solido_old/tests/.keys/maintainer.json

###############################################################################
#                                   EPOCH 4                                   #
###############################################################################


# try to withdraw
./target/debug/solido --config ../solido_test.json withdraw --amount-st-sol 1.1

# withdraw developer some fee to self
spl-token transfer --from DEVELOPER_FEE_ADDRESS STSOL_MINT_ADDRESS \
          0.0001 $(solana-keygen pubkey) --owner ~/developer_fee_key.json
# spl-token account-info --address DEVELOPER_FEE_ADDRESS
