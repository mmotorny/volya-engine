// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Game file format parsers over `Read + Seek`, with no I/O of their own; see
//! docs/architecture.md (Crates). Each format follows its clean-room spec in
//! docs/formats/.

pub mod audio;
pub mod chunks;
pub mod cp1251;
pub mod cross_table;
pub mod db;
pub mod dds;
pub mod env_mod;
pub mod etc2;
pub mod game_graph;
pub mod gamemtl;
pub mod lanims;
pub mod level;
pub mod level_ai;
pub mod level_cform;
pub mod level_geom;
pub mod ltx;
pub mod lzo1x;
pub mod model;
pub mod object;
pub mod ogf;
pub mod packet;
pub mod particles;
pub mod patrol_path;
pub mod pe;
pub mod sav;
pub mod shaders_xr;
pub mod skeletal_motions;
pub mod sound_params;
pub mod spawn;
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
