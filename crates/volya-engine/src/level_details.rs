// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A level's detail objects (docs/behavior/detail-objects.md): read in
//! `st_loading_details` with the sway values, their shader resolved with
//! the level's in `st_loading_textures`, and run each drawn frame of the
//! level (`crate::core::details`).

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use volya_engine_formats::level_details::LevelDetails;
use volya_engine_formats::shaders_xr::{Definition, Value};

use crate::core::config::Config;
use crate::core::details::{self, DetailFrame, Details, Sway};
use crate::core::file_cache;
use crate::core::vfs::Vfs;
use crate::render::details::DetailGeometry;
use crate::shading;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The class tag of the detail-object shader definitions (6.2).
const DETAIL_TAG: &str = "D_STILL ";

/// The level's detail objects as read (2.1): the cache and the two sway
/// sets.
pub struct LevelDetailObjects {
    pub cache: Details,
    pub normal: Sway,
    pub fast: Sway,
}

/// `level.details` of the level in `dir` with the sway values of
/// `[details]` (2.1); `None` when the level has no such file. The file is
/// streamed; its compact slot grid, which stands for the mapped file (a
/// leaner structure, docs/architecture/memory.md rule 5), is held in the
/// file cache under its path.
pub fn read(tree: &dyn Vfs, dir: &str, system: &Config) -> io::Result<Option<LevelDetailObjects>> {
    let path = format!("{dir}\\level.details");
    if !tree.exists(&path) {
        return Ok(None);
    }
    let grid = LevelDetails::read_in(&mut tree.open(&path)?, |len| file_cache::hold(&path, len))
        .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))?;
    let (normal, fast) = Sway::read(system)?;
    Ok(Some(LevelDetailObjects {
        cache: Details::new(grid),
        normal,
        fast,
    }))
}

/// What the presenter uploads with the world.
#[derive(Clone, Debug)]
pub struct DetailsWorld {
    pub models: Vec<DetailGeometry>,
    /// The texture's file path.
    pub texture: String,
    /// The alpha test of `Alpha-blend` (6.2).
    pub alpha_test: bool,
    /// The most instances a frame can draw: every entry of the cache's
    /// groups at the smallest density's (n + 1)² positions.
    pub capacity: usize,
}

/// The models' shader and texture (LD 3.1, DO 6.2) resolved with the
/// level's: the definition must be of the detail kind, its texture located
/// as the level's are.
pub fn resolve(
    grid: &LevelDetails,
    definitions: &HashMap<String, Definition>,
    mut locate: impl FnMut(&str) -> io::Result<String>,
) -> io::Result<DetailsWorld> {
    let Some(first) = grid.models.first() else {
        return Err(invalid("level.details: no model"));
    };
    // TODO(#407): every shipped model has `details\blend` with
    // `build_details`; models with different shaders are not supported.
    if grid
        .models
        .iter()
        .any(|m| m.shader != first.shader || m.texture != first.texture)
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "level.details: models with different shaders",
        ));
    }
    let definition = definitions
        .get(&first.shader)
        .ok_or_else(|| invalid(format!("shader {:?} not found", first.shader)))?;
    if definition.tag != DETAIL_TAG {
        // TODO(#407): a model naming a shader of another kind.
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("shader {:?} is not a detail shader", first.shader),
        ));
    }
    let alpha_test = match definition.get("Alpha-blend") {
        Some(Value::Boolean(b)) => *b,
        Some(Value::Integer { value, .. }) => *value != 0,
        _ => false,
    };
    let name = shading::texture_list(&first.texture)
        .into_iter()
        .next()
        .unwrap_or_default();
    let texture = locate(&name)?;
    let models = grid
        .models
        .iter()
        .map(|m| {
            let vertices: Vec<_> = m.vertices.iter().map(|v| (v.position, v.uv)).collect();
            DetailGeometry::new(&vertices, &m.indices)
        })
        .collect();
    let positions = (details::count(DENSITY_MIN) + 1).pow(2) as usize;
    Ok(DetailsWorld {
        models,
        texture,
        alpha_test,
        capacity: 48 * 48 * positions,
    })
}

/// `r__detail_density`'s range (7) and built-in value.
pub const DENSITY_MIN: f32 = 0.2;
pub const DENSITY_MAX: f32 = 0.6;
pub const DENSITY_DEFAULT: f32 = 0.3;

/// The detail objects of one frame, as the scene carries them.
#[derive(Clone, Debug, PartialEq)]
pub struct DetailsScene {
    pub frame: Arc<DetailFrame>,
    /// The sway values in use (WA 5.2).
    pub sway: Sway,
    /// The global time, seconds (6.5).
    pub time: f64,
}
