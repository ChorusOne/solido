// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

// The actual tests all live as modules in the `tests` directory.
// Without this, `cargo test-bpf` tries to build every top-level
// file as a separate binary, which then causes
//
// * Every build error in a shared file to be reported once per file that uses it.
// * Unused function warnings for the helpers that do not get used in *every* module.
// * Rather verbose test output, with one section per binary.
//
// By putting everything in a single module, we sidestep this problem.
pub mod tests;
