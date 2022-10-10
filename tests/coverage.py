#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2021 Chorus One AG
# SPDX-License-Identifier: GPL-3.0

"""
Run the tests with coverage instrumentation, and collect the results.
Most of this is based on [1].

Requires the following programs:

 * rustfilt (available on crates.io)
 * cargo-cov, cargo-profdata (available on crates.io in cargo-binutils)
 * llvm-cov, llvm-profdata (available as rustup component "llvm-tools-preview")
 * A nightly rustc, see `NIGHTLY` below for the exact version.

[1]: https://doc.rust-lang.org/beta/unstable-book/compiler-flags/instrument-coverage.html.
"""

import subprocess
import os
import json

from typing import List, Iterable


# Using -Z instrument-coverage requires a nightly rustc. Passing
# except-unused-generics to it requires a fairly recent nightly
# (2021-05 was too old).
NIGHTLY = '+nightly-2021-06-25'


def build_binaries(command: List[str]) -> Iterable[str]:
    print(f'Building: {" ".join(command)} ...')
    new_env = dict(os.environ)
    new_env['RUSTFLAGS'] = '-Z instrument-coverage=except-unused-generics'
    result = subprocess.run(
        ['cargo', NIGHTLY, *command, '--message-format=json'],
        encoding='utf-8',
        capture_output=True,
        check=True,
        env=new_env,
    )
    for line in result.stdout.splitlines():
        message = json.loads(line)
        executable = message.get('executable')
        if executable is not None:
            yield executable


def run_test_binary(executable_path: str) -> None:
    print(f'Running {executable_path}')
    new_env = dict(os.environ)
    new_env['LLVM_PROFILE_FILE'] = 'coverage/test-%m.profraw'
    # Note, we don't require the program to exit successfully here (check=False),
    # because "solido" in particular, without arguments, exits with a nonzero
    # exit code. It's no big deal, we run the tests elsewhere already, this is
    # just for gathering coverage assuming that the tests pass.
    subprocess.run([executable_path], check=False, env=new_env)


def merge_profdata() -> None:
    print('Merging coverage data ...')
    cmd = [
        # "cargo profdata" looks up a compatible version of llvm-profdata.
        'cargo',
        'profdata',
        '--',
        'merge',
        '-output',
        'coverage/tests.profdata',
    ]
    for fname in os.listdir('coverage'):
        if fname.endswith('.profraw'):
            cmd.append(os.path.join('coverage', fname))

    subprocess.run(cmd, check=True)


def clean_old_profdata() -> None:
    print('Deleting old coverage data ...')
    # But create the directory if it did not yest exist, before we clean it.
    os.makedirs('coverage', exist_ok=True)
    for fname in os.listdir('coverage'):
        if fname.endswith('.profraw'):
            os.remove(os.path.join('coverage', fname))


def generate_report(executables: List[str]) -> None:
    print('Generating report ...')

    # llvm-cov needs to know the executables that we instrumented.
    object_args = []
    for executable_path in executables:
        object_args.extend(['-object', executable_path])

    # Exclude coverage for dependencies, we are only interested in our own
    # code. According to the docs, `llvm-cov show` also accepts file names
    # of all source files at the end of the command, but I haven't been able
    # to get that to work, it interprets them as object files and says:
    # "Failed to load coverage: The file was not recognized as a valid
    # object file".
    ignore_regex = (
        '-ignore-filename-regex=\\.cargo/registry|solana-program-library|rustc/'
    )

    # Export in "lcov" format for codecov.io to parse.
    cmd_lcov = [
        # "cargo cov" looks up a compatible version of llvm-cov.
        'cargo',
        'cov',
        '--',
        'export',
        '-format=lcov',
        ignore_regex,
        '-instr-profile=coverage/tests.profdata',
        *object_args,
    ]
    result_mangled = subprocess.run(cmd_lcov, check=True, capture_output=True)

    # The resulting file contains mangled symbols, and unlike "llvm-cov show",
    # "llvm-cov export" does not support passing a demangler, so we need to pull
    # it through rustfilt manually. You can install "rustfilt" with
    # "cargo install rustfilt".
    result = subprocess.run(
        ['rustfilt'], check=True, capture_output=True, input=result_mangled.stdout
    )

    # Then write it with a magic name that codecov.io recognizes.
    with open('coverage/lcov.info', 'wb') as f:
        f.write(result.stdout)

    # Also generate an html report for local use.
    cmd_html = [
        'cargo',
        'cov',
        '--',
        'show',
        ignore_regex,
        '-Xdemangler=rustfilt',
        '-instr-profile=coverage/tests.profdata',
        '-format=html',
        '-output-dir=coverage/report',
        *object_args,
    ]
    subprocess.run(cmd_html, check=True)
    print(f'Check report at file://{os.getcwd()}/coverage/report/index.html.')


if __name__ == '__main__':
    clean_old_profdata()

    binaries = [
        *build_binaries(
            ['test', '--no-run', '--manifest-path', 'cli/maintainer/Cargo.toml']
        ),
        *build_binaries(['test', '--no-run', '--manifest-path', 'program/Cargo.toml']),
        *build_binaries(['build']),
    ]

    # Run all binaries. The most interesting ones are the test binaries that
    # execute the unit tests. It also happens to run "solido" without arguments
    # because it is a build artifact too.
    for binary in binaries:
        run_test_binary(binary)

    # Also run our test script that relies on the CLI, so we can collect coverage
    # for that.
    run_test_binary('tests/test_solido.py')
    run_test_binary('tests/test_multisig.py')

    merge_profdata()
    generate_report(binaries)
