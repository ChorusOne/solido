# Changelog

## v0.3.0

Released 2021-07-19.

**Compatibility**:

 * Solido now requires validators to use a vote account that has its *withdraw
   authority* set to a Solido-controlled address, and that has the commission
   set to 100%. Solido then ensures that enrolled validators all get the same
   commission percentage, paid in stSOL. The manager can configure the fee
   percentage. This approach discourages direct delegations (users delegating
   directly to the vote account, instead of depositing with Solido).
 * The on-chain `Lido` struct gained two fields:
   `rewards_withdraw_authority_bump_seed: u8`, and `metrics: Metrics`. See below
   for more information.

New features:

 * All options for the `solido` program can now be configured through a config
   file or environment variable. Previously this was not possible for the
   `--keypair-path`, `--cluster`, and `--output-mode` options.
 * The `solido` program gained a `deposit` subcommand that enables deposits from
   the command line. This command is intended for testing purposes, not for end-
   users.
 * Solido now records metrics about deposits, rewards, and fees, in the on-chain
   data structure. It can be read by any client (for example, by our deposit
   widget, to show how much SOL users deposited so far). The maintenance daemon
   also exposes these metrics in its Prometheus `/metrics` endpoint.

Changes:

 * Solido now expects all rewards to appear in vote accounts. A new instruction,
   `CollectValidatorFee`, withdraws rewards as much SOL as possible from a vote
   account, and distributes fees. The maintenance daemon calls this instruction
   when needed.
 * Solido can now withdraw excess balance in stake accounts back to the reserve,
   from where it can be staked. The main cause of excess balance is merging
   stake accounts, which frees up some of the rent.
 * Dependencies on Solana Program Library programs are now through crates.io.
   Previously we embedded the Solana Program Library as a Git submodule.

## v0.2.0

Released 2021-07-07.

**Compatibility**:

 * The reserve no longer doubles as mint authority. There is now a separate
   program-derived address that acts as the mint authority.
 * The on-chain `Lido` struct gained a field: `mint_authority_bump_seed: u8`.
 * The `Deposit` instruction takes one additional account input: the mint
   authority.

New features:

 * When adding a validator, we can now specify its weight, which determines the
   weighted stake distribution.
 * The `solido` program now accepts options from environment variables, in
   addition to the command-line and a config file.

Bugfixes and other changes:

 * CI now automatically builds a maintainer container image for every release.
 * We now avoid creating new stake accounts during `StakeDeposit` if possible,
   and the logic for `MergeStake` was simplified.
 * The maintainer bot uses fewer RPC calls and tries harder to get a consistent
   view of the on-chain state.
 * Fix bug where no fees would be minted if there are donations but no deposits.

## v0.1.0

Released 2021-07-01.

This is an internal milestone, intended as a cut-off point for the initial audit.
