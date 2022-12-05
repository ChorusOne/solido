#!/usr/bin/env python3

import argparse
import json
import sys
import os.path
from typing import Any

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.append(os.path.dirname(SCRIPT_DIR))

from tests.util import solido, solana, run  # type: ignore


def check_env(param):
    buf = param + " = " + os.getenv(param)
    if os.getenv(param) != None:
        buf += " [OK]"
    else:
        buf += " [BAD]"
    print(buf)


def verify_installation():
    check_env("PWD")
    check_env("SOLIDO_V1")
    check_env("SOLIDO_V2")
    check_env("SOLIDO_CONFIG")


def install_solido():
    pathStr = os.getenv("PWD")

    # install solido v1
    if not os.path.isdir(pathStr + "/solido_v1/"):
        outout = os.system(
            "git clone --recurse-submodules -b v1.3.6 https://github.com/lidofinance/solido solido_v1"
        )
    output = os.chdir(pathStr + "/solido_v1/")
    outout = os.system("cargo build --release")
    if os.path.isfile(pathStr + "/solido_v1/target/release/solido"):
        os.environ["SOLIDO_V1"] = pathStr + "/solido_v1/target/release/solido"
    else:
        print("Program not exist: " + pathStr + "/solido_v1/target/release/solido")
    output = os.chdir(pathStr)

    # install solido v2
    if not os.path.isdir(pathStr + "/solido_v2/"):
        outout = os.system(
            "git clone --recurse-submodules -b v2.0.0 https://github.com/lidofinance/solido solido_v2"
        )
    output = os.chdir(pathStr + "/solido_v2/")
    outout = os.system("cargo build --release")
    if os.path.isfile(pathStr + "/solido_v2/target/release/solido"):
        os.environ["SOLIDO_V2"] = pathStr + "/solido_v2/target/release/solido"
    else:
        print("Program not exist: " + pathStr + "/solido_v2/target/release/solido")
    output = os.chdir(pathStr)

    # install config
    if not os.path.isfile(pathStr + "/solido_config.json"):
        outout = os.system("cp ./solido_v2/solido_config.json solido_config.json")
    os.environ["SOLIDO_CONFIG"] = pathStr + "/solido_config.json"

    # verify installation
    verify_installation()
