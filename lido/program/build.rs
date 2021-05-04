use std::process::{exit, Command};
use std::path::Path;

fn main() {
    if std::env::var("XARGO").is_err()
        && std::env::var("RUSTC_WRAPPER").is_err()
        && std::env::var("RUSTC_WORKSPACE_WRAPPER").is_err()
    {
        println!(
            "cargo:warning=(not a warning) Building BPF {} program",
            std::env::var("CARGO_PKG_NAME").unwrap()
        );

        if Path::new("../../target/deploy/spl_stake_pool.so").exists() {
            println!("cargo:warning=(not a warning) stake-pool program found, building lido only");
            run_build_lido();
        } else {
            println!("cargo:warning=(not a warning) stake-pool program not found, building stake-pool then lido");
            run_build_lido_with_stakepool();
        }        
    }
}

fn run_build_lido() {
    if !Command::new("cargo")
            .args(&[
                "build-bpf",
                "-v",
                "--manifest-path",
                "./Cargo.toml",
            ])
            .status()
            .expect("Failed to build BPF lido program")
            .success()
        {
            exit(1);
        }
}

fn run_build_lido_with_stakepool() {
    let stakepool_success = Command::new("cargo")
            .args(&[
                "build-bpf",
                "-v",
                "--manifest-path",
                "../../stake-pool/program/Cargo.toml",
            ])
            .status()
            .expect("Failed to build BPF stake-pool program")
            .success();

    if !(stakepool_success && Command::new("cargo")
            .args(&[
                "build-bpf",
                "--manifest-path",
                "./Cargo.toml",
            ])
            .status()
            .expect("Failed to build BPF lido program")
            .success())
        {
            exit(1);
        }
}