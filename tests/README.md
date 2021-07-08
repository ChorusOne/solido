# Tests

This directory contains tests that test the actual on-chain programs and CLI
client, by uploading the programs to a local testnet, and using the CLI client
to interact with them. This requires a local validator to be running.

The tests are scripts that exit with status code 0 when the test passes, or with
a nonzero status code when the test fails.

In addition to the “black-box” tests here, there are also tests in the
`program/tests` directory of the repository. Those tests do not require a
local validator.

## Running a local validator

The following programs must be present in the local network:

 * The SPL token program, which `solana-test-validator` includes by default.

## Keys

The tests generate various key pairs to test with multiple accounts. These are
stored in `tests/.keys`. They are not valuable or security-sensitive whatsoever.

## Debugging

It is possible to run all the scripts with `--verbose` to make them print
more details when a called program exits with a nonzero exit code. This is
disabled by default because some calls fail intentionally in the tests.

## Running against testnet or devnet

Set the `NETWORK` environment variable to `https://api.testnet.solana.com` or
devnet respectively, to run the tests against testnet or devnet. Beware of
rate limits and airdrop limitations on these networks.

## Coverage

`coverage.py` will compile the tests and the CLI with coverage instrumentation,
and then run the unit tests, and the test scripts in this directory. It outputs
lcov files that are used for codecov.io, and an html report that you can browse
locally. Both are written to the `coverage` directory in the repository root.

Collecting coverage requires a nightly Rust compiler and a few other tools, see
`coverage.py` itself for details.

Note that this script does not measure coverage of the `solana_program_test`
tests in `program/tests`, as those tests run the BPF programs, and we don’t have
a way to instrument those.
