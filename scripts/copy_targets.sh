#!/bin/bash

set -e

function copy_dir() {
    cargo build --release --manifest-path $1/Cargo.toml
    cargo build-bpf --manifest-path $1/Cargo.toml

    scp -r $1/target/deploy/serum_multisig.so $2/deploy
    scp -r $1/target/deploy/lido.so  $2/deploy
    scp -r $1/target/release/solido  $2/debug
}

copy_dir solido_old build:/home/guyos/test_setup/solido_old/target
copy_dir solido build:/home/guyos/test_setup/solido/target
