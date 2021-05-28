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
