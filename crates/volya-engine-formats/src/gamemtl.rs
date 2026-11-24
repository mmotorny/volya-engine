// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game material library `gamemtl.xr`, as far as collision needs it
//! (docs/formats/level-cform.md section 3): the materials in file order with
//! their ids, names, flags and physical values. Material pairs (chunk
//! 0x1003) are skipped.

use std::io;

use crate::chunks::Chunks;
use crate::cp1251;

/// Top-level chunks (section 3.1).
pub const VERSION_CHUNK: u32 = 0x1000;
pub const MATERIALS_CHUNK: u32 = 0x1002;
pub const PAIRS_CHUNK: u32 = 0x1003;
/// The only library version the game loads.
pub const VERSION: u16 = 1;

/// Chunks of a material record (section 3.1).
const ID_NAME_CHUNK: u32 = 0x1000;
const FLAGS_CHUNK: u32 = 0x1001;
const PHYSICS_CHUNK: u32 = 0x1002;
const FACTORS_CHUNK: u32 = 0x1003;
const LIQUID_CHUNK: u32 = 0x1004;
const DESCRIPTION_CHUNK: u32 = 0x1005;
const DAMAGE_CHUNK: u32 = 0x1006;

/// Material flags tested by collision (section 3.2).
pub const BOUNCY: u32 = 0x4;
pub const PASS_THROUGH: u32 = 0x80;
pub const DYNAMIC: u32 = 0x100;
/// Copied into triangle bit 14 at load (section 3.3).
pub const TRIANGLE_BIT_14: u32 = 0x400;
/// Copied into triangle bit 15 at load (section 3.3).
pub const TRIANGLE_BIT_15: u32 = 0x800;
pub const ACTOR_OBSTACLE: u32 = 0x1000;
pub const DEADLY: u32 = 0x1000_0000;
pub const LIQUID: u32 = 0x8000_0000;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One game material (section 3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct GameMaterial {
    pub id: u32,
    /// E.g. `materials\concrete`.
    pub name: String,
    pub description: String,
    pub flags: u32,
    pub friction: f32,
    pub damping: f32,
    pub spring: f32,
    pub bounce_start_speed: f32,
    pub bounce: f32,
    /// Factor 1 is the impact factor (section 4.4); 0, 2 and 3 are
    /// unestablished.
    pub factors: [f32; 4],
    pub liquid_factor: f32,
    pub damage_rate: f32,
}

impl Default for GameMaterial {
    /// The values a record starts from (section 3.1).
    fn default() -> Self {
        Self {
            id: 0xFFFF_FFFF,
            name: "unknown".into(),
            description: String::new(),
            flags: 0,
            friction: 1.0,
            damping: 1.0,
            spring: 1.0,
            bounce_start_speed: 0.0,
            bounce: 0.1,
            factors: [0.0, 1.0, 0.0, 0.0],
            liquid_factor: 1.0,
            damage_rate: 0.0,
        }
    }
}

impl GameMaterial {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    /// Factor 1 (section 4.4).
    pub fn impact_factor(&self) -> f32 {
        self.factors[1]
    }
}

fn floats<const N: usize>(data: &[u8], what: &str) -> io::Result<[f32; N]> {
    if data.len() < 4 * N {
        return Err(invalid(format!(
            "game material {what}: {} bytes, needs {}",
            data.len(),
            4 * N
        )));
    }
    Ok(std::array::from_fn(|i| {
        f32::from_le_bytes(data[4 * i..4 * i + 4].try_into().unwrap())
    }))
}

fn string_z(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    cp1251::decode(&data[..end])
}

fn material(record: &[u8], index: usize) -> io::Result<GameMaterial> {
    let chunks = Chunks::parse(record)?;
    let what = format!("game material {index}");
    let require = |id| -> io::Result<_> { chunks.require(id, &what)?.content() };
    let mut m = GameMaterial::default();
    let id_name = require(ID_NAME_CHUNK)?;
    if id_name.len() < 4 {
        return Err(invalid(format!(
            "{what}: id chunk of {} bytes",
            id_name.len()
        )));
    }
    m.id = u32::from_le_bytes(id_name[..4].try_into().unwrap());
    m.name = string_z(&id_name[4..]);
    if let Some(c) = chunks.find(DESCRIPTION_CHUNK) {
        m.description = string_z(&c.content()?);
    }
    let flags = require(FLAGS_CHUNK)?;
    m.flags = flags
        .get(..4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| invalid(format!("{what}: flags chunk of {} bytes", flags.len())))?;
    [
        m.friction,
        m.damping,
        m.spring,
        m.bounce_start_speed,
        m.bounce,
    ] = floats(&require(PHYSICS_CHUNK)?, "physics")?;
    m.factors = floats(&require(FACTORS_CHUNK)?, "factors")?;
    if let Some(c) = chunks.find(LIQUID_CHUNK) {
        [m.liquid_factor] = floats(&c.content()?, "liquid factor")?;
    }
    if let Some(c) = chunks.find(DAMAGE_CHUNK) {
        [m.damage_rate] = floats(&c.content()?, "damage rate")?;
    }
    Ok(m)
}

/// The materials of a `gamemtl.xr` file in file order: a material's index
/// is its position in the returned list (section 3.1). A version other than
/// 1 is an error (the game loads no materials then, so every level load
/// fails, section 3.3); so is a record missing a required chunk.
pub fn parse(file: &[u8]) -> io::Result<Vec<GameMaterial>> {
    let chunks = Chunks::parse(file)?;
    let version = chunks.require(VERSION_CHUNK, "gamemtl.xr")?.content()?;
    let version = version
        .get(..2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap()));
    if version != Some(VERSION) {
        return Err(invalid(format!(
            "gamemtl.xr: invalid library version {version:?}"
        )));
    }
    let list = chunks.require(MATERIALS_CHUNK, "gamemtl.xr")?.content()?;
    Chunks::parse(&list)?
        .iter()
        .enumerate()
        .map(|(i, c)| material(&c.content()?, i))
        .collect()
}

#[cfg(test)]
pub(crate) fn record(id: u32, name: &str, flags: u32, physics: [f32; 5]) -> Vec<u8> {
    use crate::chunks::chunk;
    let mut id_name = id.to_le_bytes().to_vec();
    id_name.extend(name.as_bytes());
    id_name.push(0);
    let f = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let mut out = chunk(ID_NAME_CHUNK, &id_name);
    out.extend(chunk(FLAGS_CHUNK, &flags.to_le_bytes()));
    out.extend(chunk(PHYSICS_CHUNK, &f(&physics)));
    out.extend(chunk(FACTORS_CHUNK, &f(&[0.0, 100.0, 0.0, 0.0])));
    out
}

#[cfg(test)]
pub(crate) fn library(records: &[Vec<u8>]) -> Vec<u8> {
    use crate::chunks::chunk;
    let mut out = chunk(VERSION_CHUNK, &VERSION.to_le_bytes());
    out.extend(chunk(0x1001, &[0; 8]));
    let mut list = Vec::new();
    for (i, r) in records.iter().enumerate() {
        list.extend(chunk(i as u32, r));
    }
    out.extend(chunk(MATERIALS_CHUNK, &list));
    out.extend(chunk(PAIRS_CHUNK, &[]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::chunk;

    #[test]
    fn parses_materials_in_file_order() {
        let mut water = record(
            11,
            "materials\\water",
            0x8000_0082,
            [1.0, 1.0, 1.0, 0.0, 0.1],
        );
        water.extend(chunk(LIQUID_CHUNK, &0.8f32.to_le_bytes()));
        water.extend(chunk(DESCRIPTION_CHUNK, &[0xC2, 0xEE, 0xE4, 0xE0, 0]));
        let mut death = record(92, "materials\\death", 0x1000_0080, [1.0; 5]);
        death.extend(chunk(DAMAGE_CHUNK, &30f32.to_le_bytes()));
        let file = library(&[
            record(0, "default", 0x6000_0010, [1.0, 1.0, 1.0, 1.5, 1.0]),
            water,
            death,
        ]);
        let m = parse(&file).unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!((m[0].id, m[0].name.as_str()), (0, "default"));
        assert_eq!((m[0].bounce_start_speed, m[0].bounce), (1.5, 1.0));
        assert_eq!(m[0].liquid_factor, 1.0);
        assert_eq!(m[1].liquid_factor, 0.8);
        assert_eq!(m[1].description, "Вода");
        assert!(m[1].has(LIQUID) && m[1].has(PASS_THROUGH) && !m[1].has(DYNAMIC));
        assert_eq!((m[2].damage_rate, m[2].impact_factor()), (30.0, 100.0));
        assert!(m[2].has(DEADLY));
    }

    #[test]
    fn rejects_bad_version_and_missing_chunks() {
        let mut file = chunk(VERSION_CHUNK, &2u16.to_le_bytes());
        file.extend(chunk(MATERIALS_CHUNK, &[]));
        assert!(parse(&file).is_err());
        let mut no_flags = Vec::new();
        no_flags.extend(chunk(ID_NAME_CHUNK, b"\0\0\0\0x\0"));
        assert!(parse(&library(&[no_flags])).is_err());
    }
}
