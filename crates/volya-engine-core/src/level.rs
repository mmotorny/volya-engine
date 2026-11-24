// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's static world loaded from a level directory: the `level` file
//! (docs/formats/level.md), its `level.geom` (docs/formats/level-geom.md)
//! and the draw list of a static frame (docs/formats/ogf.md section 8).

use std::io;

pub use volya_engine_formats::level::{DrawItem, Level};
pub use volya_engine_formats::level_geom::LevelGeom;

use crate::vfs::{self, Vfs};

/// Everything a renderer needs for a static frame of one level.
#[derive(Clone, Debug)]
pub struct StaticWorld {
    pub level: Level,
    pub geom: LevelGeom,
    /// Every visual drawn, once, with its geometry range, shader entry and
    /// world transform.
    pub draw_items: Vec<DrawItem>,
}

impl StaticWorld {
    /// Loads the level in directory `dir` of `vfs` (e.g.
    /// `gamedata/levels/l01_escape`).
    pub fn load(vfs: &dyn Vfs, dir: &str) -> io::Result<Self> {
        let context =
            |file: &str, e: io::Error| io::Error::new(e.kind(), format!("{dir}/{file}: {e}"));
        let level = Level::parse(&vfs::read(vfs, &format!("{dir}/level"))?)
            .map_err(|e| context("level", e))?;
        let mut file = vfs.open(&format!("{dir}/level.geom"))?;
        let geom = LevelGeom::read(&mut file).map_err(|e| context("level.geom", e))?;
        let draw_items = level
            .static_draw_items(&geom)
            .map_err(|e| context("level", e))?;
        Ok(Self {
            level,
            geom,
            draw_items,
        })
    }
}
