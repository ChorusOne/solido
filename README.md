# LIDO for Solana



## Running the development testnet

The Solido code base includes a simple way to spin up a local Solana test validator with the Solido build packaged alongside to ease testing on local machine.  The local testnet uses [Nix](https://nixos.org/explore.html) to try to ensure consistent build and development environments.  To use the testnet, install Nix using the standard install [instructions](https://nixos.org/download.html) which, at time of writing, just entails running the install script:

```bash
curl -L https://nixos.org/nix/install | sh
```

Once Nix is installed you can then run 'nix-shell' in the root of the repository to get a shell with all components required to run the testnet:

- rust
- libs for building the rust project
  - libudev
  - hidapi
  - pkg-config
  - openssl
- [minikube](https://minikube.sigs.k8s.io/docs/)
- [k9s](https://k9scli.io/)
- [tilt](https://tilt.dev/)

### Running the local cluster

Once you have a Nix shell with all the required dependencies you will be able to spin up a Solido image that starts a test validator.  Simply run the testnetup shell script:

```bash
sh testnetup.sh start
```

This will start up minikube, run a nix build of the cargo project and then use Tilt to package Solido and Solana into a container and spin up the test validator locally. Tilt makes a dashboard available locally to monitor and/or inspect the Solido/Solana deployment.  Additionally, there is an option to stream any logs in the shell.

### Interacting with the Solido/Solana pod

To use any of the Solana SDK tools or deploy the Solido contract, you can execute into the pod using the minikube flavoured kubectl:

```bash
minikube kubectl exec ....
```

An more efficient way is to use k9s that comes with the nix-shell. Spin up an additional nix shell from the root of the project and run the k9s binary.  This will allow you to see the solido namespace and pod.  Run a shell into the solido pod with the 's' command and you will be able to access the Solana SDK tools and interact with the running test validator.

### Terminating the testnet

To exit the testnet, Ctrl+C to exit the terminal running tilt and then run the stop command in the testnet shell script:

```bash
sh testnetup.sh stop
```
