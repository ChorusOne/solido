#!/usr/bin/env bash

set -ex

# Check for required arguments
req_args=("$MULTISIG_PROGRAM_ID")
for i in "${req_args[@]}"
do
   if [ -z "$i" ]
   then
     echo "USAGE:\n"
     echo "solido --multisig_program_id $MULTISIG_PROGRAM_ID --cluster $CLUSTER  run-maintainer --solido-program-id $SOLIDO_PROGRAM_ID --solido-address $SOLIDO_ADDRESS\n"
     echo "WHERE:\n"
     echo "MULTISIG_PROGRAM_ID: Is the public key address of the multisig governance program for Solido"
     echo "CLUSTER: Is the url to connect to the Solana environment, it can be one of: https://api.devnet.solana.com, https://api.testnet.solana.com or https://api.mainnet-beta.solana.com"
     echo "SOLIDO_PROGRAM_ID: Is the public key address of the Solido program"
     echo "SOLIDO_ADDRESS: Is the public key address for the solido program serialized data"
   exit 1
   fi
done


echo "Running Solido maintenance."

exec /solido --multisig_program_id $MULTISIG_PROGRAM_ID --cluster $CLUSTER  run-maintainer --solido-program-id $SOLIDO_PROGRAM_ID --solido-address $SOLIDO_ADDRESS
