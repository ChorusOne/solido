#!/bin/bash

scp -r solido_old/target/deploy/serum_multisig.so  build:/home/guyos/test_setup/solido_old/target/deploy/
scp -r solido_old/target/deploy/lido.so  build:/home/guyos/test_setup/solido_old/target/deploy/
scp -r solido_old/target/release/solido  build:/home/guyos/test_setup/solido_old/target/debug/

scp -r solido/target/deploy/serum_multisig.so  build:/home/guyos/test_setup/solido/target/deploy/
scp -r solido/target/deploy/lido.so  build:/home/guyos/test_setup/solido/target/deploy/
scp -r solido/target/release/solido  build:/home/guyos/test_setup/solido/target/debug/
