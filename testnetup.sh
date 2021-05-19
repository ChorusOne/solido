#!/usr/bin/env bash

COMD=$1
ARGERR="Please supply first argument to the script as either 'start' or 'stop'"


if [ -z $COMD ]; then
  echo $ARGERR
  exit 1
fi

remove(){

 rm -rf ./testnet/harness/images/solido/build
 rm -rf ./result

}


start(){

 # 1. Start minikube locally
 minikube start

 # 2. Clean previous build
 remove

 # 3. Use Nix to build project
 nix-build

 # 4. Copy build files
 mkdir -p ./testnet/harness/images/solido/build
 cp -rf ./result/target.tar.zst ./testnet/harness/images/solido/build/

 # 5. Use Tilt to spin up local cluster
 tilt up -f testnet/harness/Tiltfile

}

stop(){
  # 1. Shutdown Tilt
  tilt down -f testnet/harness/Tiltfile
  # 2. Shutdown minikube
  minikube stop
  # 3. Clean any builds
  remove
}


case $COMD in
"start")
  start
  ;;
"stop")
  stop
  ;;
*)
  echo $ARGERR
  ;;
esac
