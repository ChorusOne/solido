#!/usr/bin/env python3

"""
Deployment emulation - starts solana-test-validator, creates an instance and starts a bot.
"""

import unittest
import subprocess
import time
import os
import signal
from typing import Any, Tuple
import json

from util import run, get_solido_path
from start_test_validator import get_rpc_block_height
from deploy_test_solido import Instance


class Emulator(unittest.TestCase):
    """Launches a validator, Solido instance and a maintainer for test purposes.

    You can inherit from it to write your tests and interact with Solido instance.
    """

    def setUp(self) -> None:
        "launches local solana-test-validator"
        run('rm', '-rf', 'tests/.keys/', 'test-ledger/', 'tests/__pycache__/')
        subprocess.run(['killall', '-qw', 'solana-test-validator'])
        self.solana_test_validator = subprocess.Popen(
            ["solana-test-validator --slots-per-epoch 150"],
            stdout=subprocess.DEVNULL,
            shell=True,
            preexec_fn=os.setsid,
        )

        # wait for solana-test-validator to be up and ready
        self.assertNotEqual(get_rpc_block_height(), None)

        # wait for instance to be deployed locally
        self.instance = Instance()

        # start a maintainer and redirect logs to a file
        self.logs_file = open("tests/.logs", "w")
        self.maintainer_process = subprocess.Popen(
            [
                get_solido_path(),
                '--keypair-path',
                'tests/.keys/maintainer.json',
                '--config',
                '../solido_test.json',
                'run-maintainer',
                '--max-poll-interval-seconds',
                '1',
            ],
            stdout=self.logs_file,
            universal_newlines=True,
            preexec_fn=os.setsid,
        )

    @property
    def epoch(self) -> Tuple[int, float]:
        info = run('solana', 'epoch-info', '--output', 'json')
        parsed = json.loads(info)
        return int(parsed['epoch']), float(parsed['epochCompletedPercent'])

    def tearDown(self) -> None:
        os.killpg(os.getpgid(self.solana_test_validator.pid), signal.SIGTERM)
        os.killpg(os.getpgid(self.maintainer_process.pid), signal.SIGTERM)
        self.logs_file.close()


class MyTest(Emulator):
    def test_some(self) -> None:
        print(self.epoch)


if __name__ == "__main__":
    mt = MyTest()
    unittest.main(verbosity=2, warnings='ignore')
