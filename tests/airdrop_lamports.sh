#!/usr/bin/env bash
for i in `seq 1 5`
do
    solana airdrop 500.0
    if [ $? -eq 0 ]
    then
        break
    else
        echo "Airdrop failed, awaiting 2s before asking again..."
        sleep 2
    fi
done
