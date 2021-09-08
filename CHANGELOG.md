# Changelog

## v1.0.1

Released 2021-09-08.

Bugfixes:

 * Fix an outdated version number and filename in `buildimage.sh` that caused
   it to fail. No changes to the on-chain program or `solido`.

## v1.0.0

Released 2021-09-08.

**Compatibility**:

 * The `--validator-vote-account` option has been removed from `solido
   run-maintainer`. Previously this was used to specify the validator to claim
   validation fees for, but now the maintenance bot will claim fees on behalf of
   all validators whenever possible.

New features:

 * `solido` now supports `--keypair` (and `SOLIDO_KEYPAIR` when passed as
   environment variable) as an alternative to `--keypair-path`. This is useful
   for example to load the signer key from Hashicorp Vault with
   [Vaultenv](https://github.com/channable/vaultenv).
 * We added scripts to automate some of the checks for validator onboarding.

Bugfixes:

 * Issues identified by Neodyme have been fixed, including one critical issue
   that enabled an attacker to asymptotically own 100% of the stSOL supply.
 * Issues identified by Jon Cinque in a peer review have been fixed. Thanks Jon!

Other changes:

 * Solido now has a bug bounty and security policy, see SECURITY.md.

## v0.5.0

Released 2021-08-25.

**Compatibility**:

 * `solido run-maintainer` now accepts the listen address with the `--listen`
   option, instead of accepting it directly; this was an oversight in previous
   versions.
 * The `Validator` struct, part of the on-chain `Lido` struct, now stores two
   more stake account seeds, and the `weight: u32` has been replaced with
   `active: bool`. Both of these play a role in validator removal and stake
   redistribution. We intend for this to be the final on-chain format that will
   be used in v1.
 * The serialization format of some instructions changed, due to the
   introduction of new instructions. From v1 onwards we will make sure to keep
   these stable.

New features:

 * Withdrawals are now possible through the new `Withdraw` instruction that
   splits off a stake account. For testing purposes and for advanced users,
   `solido` gained a new subcommand, `withdraw`, to submit a withdraw
   transaction.
 * `solido create-solido` now accepts a mint address. This can be used to create
   the stSOL mint in advance at a known address, which is what we did on
   mainnet-beta.
 * `solido` now includes more detail when printing some errors.
 * `solido multisig` now outputs some details about the transaction for
   `approve` and `execute-transaction`.
 * Validators now have an `active` status (that supersedes `weight` — the stake
   distribution will be uniform). Validators start out active, but can be
   deactivated to initiate their removal.
 * The new `solido deactivate-validator` subcommand can propose a multisig
   transaction to deactivate a validator.
 * There is a new `Unstake` instruction that will be used for active stake
   rebalancing in a future version.

Bugfixes:

 * `solido` now properly handles derivation paths when using `usb://ledger`
   keypair paths.
 * The stake authority account is no longer writable for the `StakeDeposit`
   instruction.

Other changes:

 * We updated to Solana from 1.7.3 to 1.7.8 and the Serum Multisig program from
   v0.4.0 to v0.6.0, and we updated dependencies with `cargo audit` issues.
 * A Grafana dashboard is available in `etc/grafana-dashboard.json`.
 * The audit report of the Bramah Systems audit is now available in `audit`.
 * A deposit transaction now logs what it did, to make it easier to understand
   transactions on block explorers.
 * The `StakeDeposit` instruction now requires staking with the validator that
   has the least stake, to reduce the need to trust maintainers.

## v0.4.0

Released 2021-07-30.

**Compatibility**:

 * The on-chain `Lido` struct gained a new field in its `metrics` field for
   tracking withdrawals.

New features:

 * A first version of withdrawals has been implemented.
 * `solido show-solido` now shows more information about validators.
 * The maintenance daemon can now be configured to claim fees for one validator.
   Claiming the fee transfers validation fee stSOL to the validator’s configured
   fee account.

Other changes:

 * We now have more accurate coverage measurements for the unit tests and CLI
   test scripts. The `solana_program_test` tests and on-chain program still do
   not have coverage reports.
 * Internal cleanup.

## v0.3.0

Released 2021-07-20.

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
   stake accounts, which frees up some of the rent. Withdrawing excess stake is
   done by the `WithdrawInactiveStake` instruction, which was previously called
   `UpdateValidatorBalance`. The maintenance daemon calls this instruction when
   needed.
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
