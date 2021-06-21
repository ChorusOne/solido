#!/usr/bin/env bash


# Check for required arguments
if [ -z "$MULTISIG_PROGRAM_ID" ]
then
  echo "Please supply MULTISIG_PROGRAM_ID as environment variable"
  echo "MULTISIG_PROGRAM_ID: Is the public key address of the multisig governance program for Solido"
  exit 1
fi

if [ -z "$CLUSTER" ]
then
  echo "Please supply CLUSTER as environment variable"
  echo "CLUSTER: Is the url to connect to the Solana environment, it can be one of: https://api.devnet.solana.com, https://api.testnet.solana.com or https://api.mainnet-beta.solana.com"
  exit 1
fi

if [ -z "$SOLIDO_PROGRAM_ID" ]
then
  echo "Please supply SOLIDO_PROGRAM_ID as environment variable"
  echo "SOLIDO_PROGRAM_ID: Is the public key address of the Solido program"
  exit 1
fi

if [ -z "$SOLIDO_ADDRESS" ]
then
  echo "Please supply MULTISIG_PROGRAM_ID as environment variable"
  echo "SOLIDO_ADDRESS: Is the public key address for the solido program serialized data"
  exit 1
fi


echo "Running Solido maintenance."

set -ex

exec /solido --multisig_program_id $MULTISIG_PROGRAM_ID --cluster $CLUSTER  run-maintainer --solido-program-id $SOLIDO_PROGRAM_ID --solido-address $SOLIDO_ADDRESS
