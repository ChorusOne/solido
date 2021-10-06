// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;
mod instruction;
mod processor;
