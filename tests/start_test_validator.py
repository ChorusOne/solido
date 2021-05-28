#!/usr/bin/env python3

"""
Start a test validator and wait for it to be available, then print its PID.

This script is used to start the test validator on CI.
"""

import subprocess
import sys
import time

# Start the validator, pipe its stdout to /dev/null.
test_validator = subprocess.Popen(
    [
        'solana-test-validator',
    ],
    stdout=subprocess.DEVNULL,
)

# Wait up to 5 seconds for the validator to be running. We check this by running
# "solana cluster-version". If it does not fail, the RPC is responding.
is_rpc_online = False

for _ in range(50):
    result = subprocess.run(
        ['solana', 'cluster-version'],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode == 0:
        is_rpc_online = True
        break

    sleep_seconds = 0.1
    time.sleep(sleep_seconds)

if is_rpc_online and test_validator.poll() is None:
    # The RPC is online, and the process is still running.
    print(test_validator.pid)

elif is_rpc_online:
    print('RPC is online, but the process is gone ... was a validator already running?')
    sys.exit(1)

else:
    print('Test validator is still not responding, something is wrong.')
    sys.exit(1)
