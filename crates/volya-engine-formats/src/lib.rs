// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Game file format parsers over `Read + Seek`, with no I/O of their own; see
//! docs/architecture.md (Crates). Each format follows its clean-room spec in
//! docs/formats/.

pub mod audio;
pub mod cp1251;
pub mod db;
pub mod dds;
pub mod ltx;
pub mod pe;
pub mod text;
pub mod ui;
pub mod video;
pub mod xml;

#[cfg(test)]
fn md5_hex(data: &[u8]) -> String {
    use md5::{Digest, Md5};
    Md5::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
