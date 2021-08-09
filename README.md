# Lido for Solana

[![Coverage][cov-img]][cov]

[cov-img]: https://codecov.io/gh/ChorusOne/solido/branch/main/graph/badge.svg?token=USB921ZL6B
[cov]:     https://codecov.io/gh/ChorusOne/solido/branch/main/graph/badge.svg?token=USB921ZL6B

*Lido for Solana* (“Solido” for short) is a [Lido DAO][lido]-governed liquid
staking protocol for the Solana blockchain. Anyone who stakes their SOL tokens
with Lido will be issued an on-chain representation of the SOL staking position
with Lido validators, called *stSOL*.

Lido for Solana gives you:

 * **Liquidity** — No delegation/activation delays and the ability to sell your
   staked tokens
 * **One-click staking** — No complicated steps
 * **Decentralized security** — Assets spread across the industry’s leading
   validators chosen by the Lido DAO

Further resources:

   <!-- TODO: Update link to staking page once we are live on mainnet. -->
 * [Staking page for end users (devnet)][stake]
 * [Documentation][documentation]
 * [Blog][blog]

[lido]:          https://lido.fi
[stake]:         https://solana-dev.testnet.lido.fi/
[documentation]: https://chorusone.github.io/solido/
[blog]:          https://medium.com/chorus-one

## Repository layout

This repository contains the source code for the on-chain program, and for the
`solido` utility to interact with it. The source code for the staking widget,
and documentation, are in a different repository, which is not yet public.

 * `program` — The on-chain Solana BPF program.
 * `multisig` — A pinned version of the on-chain [Serum multisig
   program][multisig], used as the upgrade authority of the Solido program, and
   as the manager of the Solido instance.
 * `cli` — The command-line `solido` utility for interacting with the on-chain
   programs.
 * `docker` — Dockerfiles for reproducible builds, and for the maintainer image.
 * `tests` — Scripts that test the actual `solido` binary and on-chain program.
 * `program/tests` — Tests using the `solana-program-test` test framework.

[multisig]: https://github.com/project-serum/multisig

## License

Lido for Solana is licensed under the GNU General Public License version 3.
