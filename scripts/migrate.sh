#!/bin/bash

# EPOCH 0

cd solido_old

# start local validator
rm -rf tests/.keys/ test-ledger/ tests/__pycache__/ && solana-test-validator --slots-per-epoch 150

# withdraw SOLs from local validator vote account to start fresh
solana withdraw-from-vote-account test-ledger/vote-account-keypair.json v9zvcQbyuCAuFw6rt7VLedE2qV4NAY8WLaLg37muBM2 999999.9 --authorized-withdrawer test-ledger/vote-account-keypair.json

# create instance
./tests/deploy_test_solido.py --verbose

# start maintainer
./target/debug/solido --config ~/Documents/solido_test.json --keypair-path ../solido_old/tests/.keys/maintainer.json run-maintainer --max-poll-interval-seconds 1

# deposit some SOL
./target/debug/solido --config ../solido_test.json deposit --amount-sol 100

# EPOCH 1

# receive some rewards

# EPOCH 2

# deactivate validators
../solido/scripts/update_solido_version.py --config ../solido_test.json deactivate-validators --keypair-path ./tests/.keys/maintainer.json > output
./target/debug/solido --config ../solido_test.json --keypair-path ./tests/.keys/maintainer.json multisig approve-batch --transaction-addresses-path output

# propose program upgrade
../solido/scripts/update_solido_version.py --config ../solido_test.json load-program --program-filepath ../solido/target/deploy/lido.so |xargs -I {}  ./target/debug/solido --config ~/Documents/solido_test.json --keypair-path ../solido_old/tests/.keys/maintainer.json multisig propose-upgrade --spill-address $(solana-keygen pubkey) --buffer-address {} --program-address $(cat ../solido_test.json | jq -r .solido_program_id) > ../solido/output

# create a new validator with a 5% commission and propose to add it
solana-keygen new --no-bip39-passphrase --force --silent --outfile ../solido_old/tests/.keys/vote-account-key.json
solana-keygen new --no-bip39-passphrase --force --silent --outfile ../solido_old/tests/.keys/vote-account-withdrawer-key.json
solana create-vote-account ../solido_old/tests/.keys/vote-account-key.json ../solido_old/test-ledger/validator-keypair.json ../solido_old/tests/.keys/vote-account-withdrawer-key.json --commission 5

cd ../solido

# transfer SOLs for allocating space for account lists
solana --url localhost transfer --allow-unfunded-recipient ../solido_old/tests/.keys/maintainer.json 32.0

# propose migration
scripts/update_solido_version.py --config ../solido_test.json propose-migrate --keypair-path ../solido_old/tests/.keys/maintainer.json >> output

# EPOCH 3

# wait for maintainers to remove validators, approve program update and migration
./target/debug/solido --config ../solido_test.json --keypair-path ../solido_old/tests/.keys/maintainer.json multisig approve-batch --transaction-addresses-path output

# add validator
./target/debug/solido --config ~/Documents/solido_test.json --keypair-path ../solido_old/tests/.keys/maintainer.json add-validator --validator-vote-account $(solana-keygen pubkey ../solido_old/tests/.keys/vote-account-key.json)
echo ADD_VALIDATOR_TRANSACTION > ../solido/output
./target/debug/solido --config ../solido_test.json --keypair-path ../solido_old/tests/.keys/maintainer.json multisig approve-batch --transaction-addresses-path output

# EPOCH 4

# try to withdraw
./target/debug/solido --config ~/Documents/solido_test.json withdraw --amount-st-sol 1.1

# withdraw developer some fee to self
spl-token transfer --from DEVELOPER_FEE_ADDRESS STSOL_MINT_ADDRESS 0.0001 $(solana-keygen pubkey) --owner ~/developer_fee_key.json
# spl-token account-info --address DEVELOPER_FEE_ADDRESS
