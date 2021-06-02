#!/usr/bin/env bash

COMD=$1
ARGERR="Please supply first argument to the script as either 'start' or 'stop'"


if [ -z $COMD ]; then
  echo $ARGERR
  exit 1
fi


build(){
cargo build --release
}


clean(){
 cargo clean

}


start(){

 # 1. Start minikube locally
 minikube start

 # 2. Clean previous build
 clean

 # 3. Build project
 build

 # 4. Use Tilt to build local container and spin up local cluster
 tilt up -f testnet/Tiltfile

}

stop(){
  # 1. Shutdown Tilt
  tilt down -f testnet/Tiltfile
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
