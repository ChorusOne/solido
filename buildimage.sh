#!/usr/bin/env bash

# 1. Clean build dir
cargo clean

# 2. Build for CLI
cargo build --release

# 3. Get last commit hash
VERSION=$(git rev-parse --short HEAD)

# 4. Build container image
docker build -t chorusone/solido:$VERSION .


