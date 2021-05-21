# Tests

This directory contains tests that test the actual on-chain programs and CLI
client, by uploading the programs to a local testnet, and using the CLI cient
to interact with them. This requires a local validator to be running.

The tests are scripts that exit with status code 0 when the test passes, or with
a nonzero status code when the test fails.
