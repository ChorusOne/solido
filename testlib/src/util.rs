// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::env;
use std::process::Command;

/// Download the Orca Token Swap program from the chain and put it in `target/deploy`.
///
/// If the file already exists, this will not download it again.
pub fn ensure_orca_program_exists() {
    let mut path = env::current_exe().expect("Failed to get executable path.");

    // The executable path of the test driver is something like
    // /repo/target/debug/deps/mod-8d7ddfb574f4dee2
    // So to get to "target/deploy", we drop 3 components and then add "deploy".
    path.pop();
    path.pop();
    path.pop();
    path.push("deploy");
    path.push("orca_token_swap_v2.so");

    if path.exists() {
        // Program already there, we are not going to download it again.
        return;
    }

    println!("Orca program not found at {:?}, downloading ...", path);
    let result = Command::new("solana")
        .args(&["--url", "https://api.mainnet-beta.solana.com"])
        .args(&["program", "dump"])
        .arg(anker::orca_token_swap_v2::id().to_string())
        .arg(&path)
        .status();

    match result {
        Ok(status) if status.success() => { /* Ok */ }
        _ => {
            panic!(
                "Failed to obtain Orca program from chain. \
                 Please run 'solana program dump {} target/deploy/orca_token_swap_v2.so'.",
                anker::orca_token_swap_v2::id(),
            );
        }
    }

    assert!(path.exists(), "{:?} should exist by now.", path);
}
