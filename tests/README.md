# Tests

This directory contains tests that test the actual on-chain programs and CLI
client, by uploading the programs to a local testnet, and using the CLI client
to interact with them. This requires a local validator to be running, with some
programs deployed at expected addresses, see below.

The tests are scripts that exit with status code 0 when the test passes, or with
a nonzero status code when the test fails.

In addition to the “black-box” tests here, there are also tests in the
`program/tests` directory of the repository. Those tests do not require a
local validator.

## Running a local validator

The following programs must be present in the local network:

 * The SPL token program, which `solana-test-validator` includes by default.
 * The SPL stake pool program at `poo1B9L9nR3CrcaziKVYVpRX6A9Y1LAXYasjjfCbApj`.

To include them, after building with `cargo build-bpf`, start
`solana-test-validator` with these flags:

    --bpf-program poo1B9L9nR3CrcaziKVYVpRX6A9Y1LAXYasjjfCbApj target/deploy/spl_stake_pool.so
