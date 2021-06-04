#!/usr/bin/env bash

# 1. Get last commit hash
VERSION=$(git rev-parse --short HEAD)

# 2. Build container image
docker build -t chorusone/solido:$VERSION .


