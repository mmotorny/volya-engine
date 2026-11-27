// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level-map data (docs/behavior/pda-level-maps.md, cited by section):
//! the list of level maps of `game.ltx` (2), the global map (4) and each
//! level map's section (3, 5), and how a level position maps onto the
//! global map (6). The PDA map draws them (docs/behavior/pda-map.md).

use std::io;

use crate::core::config::{self, Config};
use crate::core::vfs::Vfs;
use crate::data::GameData;
use volya_engine_formats::ltx::value::{float, floats};

fn invalid(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

/// The single-player list of level maps (2).
pub const LIST: &str = "level_maps_single";
/// The global map's section (4).
pub const GLOBAL: &str = "global_map";
/// The picture of a map section without `texture` (3).
pub const NO_MAP: &str = "ui\\ui_nomap2";
/// Its rectangle (3).
pub const NO_MAP_RECT: [f32; 4] = [-10000.0, -10000.0, 10000.0, 10000.0];

/// What every map section reads first (3): its picture and the rectangle
/// the picture covers (x1, y1, x2, y2).
#[derive(Clone, Debug, PartialEq)]
pub struct MapPicture {
    /// A texture path (ui-layout.md 6.3).
    pub texture: String,
    pub bound: [f32; 4],
}

impl MapPicture {
    /// Section `name` (lowercase) of `game` (3): `texture` and, with it,
    /// the required `bound_rect`; without `texture`, [`NO_MAP`] over
    /// [`NO_MAP_RECT`]. The test for `texture` is the plain existence test,
    /// byte for byte, so `name` must be lowercase (3).
    pub fn read(game: &Config, name: &str) -> io::Result<Self> {
        let has_texture = game.contains(name)
            && game
                .section(name)
                .is_some_and(|s| s.get("texture").is_some());
        if !has_texture {
            return Ok(Self {
                texture: NO_MAP.to_owned(),
                bound: NO_MAP_RECT,
            });
        }
        Ok(Self {
            texture: game.read(name, "texture")?.to_owned(),
            bound: floats(game.read(name, "bound_rect")?),
        })
    }

    pub fn width(&self) -> f32 {
        self.bound[2] - self.bound[0]
    }

    pub fn height(&self) -> f32 {
        self.bound[3] - self.bound[1]
    }
}

/// `[global_map]` (4).
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalMap {
    /// Its rectangle is in global-map units, y downward.
    pub picture: MapPicture,
    /// The largest zoom, UI units per global-map unit.
    pub max_zoom: f32,
}

/// One level map (5).
#[derive(Clone, Debug, PartialEq)]
pub struct LevelMap {
    /// The list entry lowercased: the level's name (2).
    pub name: String,
    /// Its `bound_rect` is in level metres: x1, x2 on x, y1, y2 on z (5.1).
    pub picture: MapPicture,
    /// `global_rect`: where the level lies on the global map (5.1).
    pub global: [f32; 4],
}

impl LevelMap {
    /// Global-map units per metre, q = (gx2 − gx1) / (x2 − x1): the width
    /// ratio, used for both axes (6.2).
    pub fn ratio(&self) -> f32 {
        (self.global[2] - self.global[0]) / self.picture.width()
    }

    /// The global-map point of level point (`x`, `z`) (6.2): gx = gx1 +
    /// (x − x1) · q, gy = gy1 + (y2 − z) · q.
    pub fn to_global(&self, x: f32, z: f32) -> [f32; 2] {
        let q = self.ratio();
        let b = self.picture.bound;
        [
            self.global[0] + (x - b[0]) * q,
            self.global[1] + (b[3] - z) * q,
        ]
    }
}

/// The global map and the level maps, in the list's entry order (2).
#[derive(Clone, Debug, PartialEq)]
pub struct LevelMaps {
    pub global: GlobalMap,
    pub levels: Vec<LevelMap>,
}

impl LevelMaps {
    /// Reads `config\game.ltx` (1).
    pub fn load(tree: &dyn Vfs, data: &GameData) -> io::Result<Self> {
        let path = data.aliases.path("$game_config$", "game.ltx")?;
        Self::read(&config::load(tree, &path)?)
    }

    /// The configuration `game` (1): the global map (4, `max_zoom`
    /// required), then one level map per entry of [`LIST`] in entry order,
    /// its name lowercased, a name twice fatal; no list, no level maps (2).
    pub fn read(game: &Config) -> io::Result<Self> {
        let global = GlobalMap {
            picture: MapPicture::read(game, GLOBAL)?,
            max_zoom: float(game.read(GLOBAL, "max_zoom")?),
        };
        let mut levels: Vec<LevelMap> = Vec::new();
        for entry in game.section(LIST).iter().flat_map(|s| s.entries()) {
            let name = entry.key().to_ascii_lowercase();
            if levels.iter().any(|l| l.name == name) {
                return Err(invalid(format!("Duplicate level name not allowed: {name}")));
            }
            let picture = MapPicture::read(game, &name)?;
            let global = floats(game.read(&name, "global_rect")?);
            levels.push(LevelMap {
                name,
                picture,
                global,
            });
        }
        Ok(Self { global, levels })
    }

    /// The level map of level `name` (9: by its registered name).
    pub fn level(&self, name: &str) -> Option<usize> {
        self.levels.iter().position(|l| l.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use volya_engine_formats::ltx::parse;

    /// Excerpts of the shipped values (pda-level-maps.md 2, 4, 5.2).
    const GAME: &str = "[level_maps_single]\nL01_escape\nl12u_sarcofag\n\
        [global_map]\ntexture = ui\\ui_global_map\nbound_rect = 0.0,0.0, 1024, 2634.0\nmax_zoom = 6.0\n\
        [L01_escape]\ntexture = map\\map_escape\nbound_rect = -317.164, -633.718, 404.880, 811.627\n\
        global_rect = 355, 2011, 576, 2453\n\
        [l12u_sarcofag]\nglobal_rect = 440, 333, 442, 335\nweathers = default\n";

    #[test]
    fn reads_the_list_lowercased_and_the_sections() {
        let maps = LevelMaps::read(&parse(GAME.as_bytes()).unwrap()).unwrap();
        assert_eq!(maps.global.max_zoom, 6.0);
        assert_eq!(maps.global.picture.texture, "ui\\ui_global_map");
        assert_eq!(maps.global.picture.bound, [0.0, 0.0, 1024.0, 2634.0]);
        let names: Vec<&str> = maps.levels.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["l01_escape", "l12u_sarcofag"]);
        assert_eq!(maps.levels[0].picture.texture, "map\\map_escape");
        // No `texture`: the no-map picture and rectangle (3).
        assert_eq!(maps.levels[1].picture.texture, NO_MAP);
        assert_eq!(maps.levels[1].picture.bound, NO_MAP_RECT);
        assert_eq!(maps.levels[1].global, [440.0, 333.0, 442.0, 335.0]);
        assert_eq!(maps.level("l01_escape"), Some(0));
        assert_eq!(maps.level("l10u_bunker"), None);
    }

    #[test]
    fn maps_the_start_position_as_the_worked_example() {
        let maps = LevelMaps::read(&parse(GAME.as_bytes()).unwrap()).unwrap();
        // 6.2: x −246.7263, z −134.4339 lies at (376.56, 2300.57).
        let [gx, gy] = maps.levels[0].to_global(-246.7263, -134.43387);
        assert!((gx - 376.56).abs() < 0.01, "{gx}");
        assert!((gy - 2300.57).abs() < 0.01, "{gy}");
    }

    #[test]
    fn a_name_twice_is_fatal_and_no_list_is_no_maps() {
        let twice = "[level_maps_single]\nL01_escape\nl01_escape\n\
            [global_map]\nmax_zoom = 6\n[l01_escape]\nglobal_rect = 0,0,1,1\n";
        let e = LevelMaps::read(&parse(twice.as_bytes()).unwrap()).unwrap_err();
        assert!(e.to_string().contains("Duplicate level name"), "{e}");
        let none = "[global_map]\nmax_zoom = 6\n";
        let maps = LevelMaps::read(&parse(none.as_bytes()).unwrap()).unwrap();
        assert!(maps.levels.is_empty());
        // `max_zoom` is required (4).
        assert!(LevelMaps::read(&parse(b"[global_map]\n").unwrap()).is_err());
    }
}
