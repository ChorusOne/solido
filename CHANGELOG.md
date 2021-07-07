# Changelog

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
