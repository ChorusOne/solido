#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

# 1. Get last commit hash
VERSION=$(git rev-parse --short HEAD)
TAG="chorusone/solido:$VERSION"
BASETAG="chorusone/solido-base"
SOLIPATH="/root/.local/share/solana/install/releases/1.7.15/solana-release/bin/solido"


# 2. Build container image
echo "Building container image $TAG"
docker build -t $BASETAG -f docker/Dockerfile.base .
docker build -t $TAG -f docker/Dockerfile.dev .

# 3. Clean directory for artefacts
echo "Cleaning artefact directories"
rm -rf build
mkdir -p build

#4. Run container
echo "Running build container $TAG"
$(docker run --rm -it $TAG sleep 15) &
sleep 2

#5. Find container id
CON_ID=$(docker ps | grep $TAG | awk '{print $1}')
echo "Running container id is=$CON_ID"


#6. Copy artefacts locally
## a. on-chain
programs=("lido" "serum_multisig")
for i in "${programs[@]}"
do
  echo -e $"\nCopying $i program and hash"
  docker cp $CON_ID:$SOLIPATH/deploy/$i.so ./build/$i.so
  docker cp $CON_ID:$SOLIPATH/deploy/$i.hash ./build/$i.hash
done

## b. cli
echo -e  "\nCopying cli and hash to build"
docker cp $CON_ID:$SOLIPATH/cli/maintainer/solido ./build/solido
docker cp $CON_ID:$SOLIPATH/cli/maintainer/solido.hash ./build/solido.hash

echo "All build artefacts copied to ./build. Associated container will exit shortly."
