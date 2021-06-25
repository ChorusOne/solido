#!/usr/bin/env python3

"""
Run the tests with coverage instrumentation, and collect the results.
Most of this is based on [1].

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
        [
            'cargo',
            NIGHTLY,
            *command,
            '--message-format=json'
        ],
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
    for fname in os.listdir('coverage'):
        if fname.endswith('.profraw'):
            os.remove(os.path.join('coverage', fname))


def generate_report(executables: List[str]) -> None:
    print('Generating report ...')
    cmd = [
        # "cargo cov" looks up a compatible version of llvm-cov.
        'cargo',
        'cov',
        '--',
        'show',
        # Exclude coverage for dependencies, we are only interested in our own
        # code. According to the docs, `llvm-cov show` also accepts file names
        # of all source files at the end of the command, but I haven't been able
        # to get that to work, it interprets them as object files and says:
        # "Failed to load coverage: The file was not recognized as a valid
        # object file".
        '-ignore-filename-regex=\\.cargo/registry|solana-program-library|rustc/',
        # Demangle symbols. This requires "rustfilt", which you can install with
        # "cargo install rustfilt".
        '-Xdemangler=rustfilt',
        '-instr-profile=coverage/tests.profdata',
    ]

    for executable_path in executables:
        cmd.extend(['-object', executable_path])

    cmd_html = [*cmd, '-format=html', '-output-dir=coverage/report']
    cmd_txt = [*cmd, '-format=text', '-output-dir=coverage/txt']

    subprocess.run(cmd_txt, check=True)
    subprocess.run(cmd_html, check=True)
    print(f'Check report at file://{os.getcwd()}/coverage/report/index.html.')


if __name__ == '__main__':
    clean_old_profdata()

    binaries = [
        *build_binaries(['test', '--no-run', '--manifest-path', 'cli/Cargo.toml']),
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

    merge_profdata()
    generate_report(binaries)
