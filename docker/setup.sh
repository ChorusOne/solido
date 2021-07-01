#!/usr/bin/env bash

KEYPAIR_PATH="~/.config/solana"

# Check for Solana keypair and inject into id.json file if available.
if [ -z "$SOLANA_KEYPAIR" ]
then
  echo "Please supply SOLANA_KEYPAIR as environment variable"
  echo "SOLANA_KEYPAIR: The solana file system wallet contents in the form [22,76,...,5]"
  exit 1
else
  echo "Injecting SOLANA_KEYPAIR to $KEYPAIR_PATH"
  mkdir -p $KEYPAIR_PATH
  echo $SOLANA_KEYPAIR >> $KEYPAIR_PATH/id.json
fi

