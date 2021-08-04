#!/usr/bin/env bash
for i in `seq 1 6`
do
    solana airdrop 500.0
    if [ $? -eq 0 ]
    then
        break
    else
        if [ $i -eq 5 ]
        then
            echo "Airdrop failed after trying 5 times, giving up."
            exit 1
        else
            echo "Airdrop failed, awaiting 2s before asking again..."
            sleep 2
        fi
    fi
done
