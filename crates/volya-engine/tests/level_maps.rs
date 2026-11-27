// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level-map data of the Russian edition against
//! docs/behavior/pda-level-maps.md: the list in its entry order (2), the
//! global map (4), every level's section and picture (5.2), the mappings'
//! worked examples (6.2), the levels without a map (8, 9) and the
//! minimap's sections (7). Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]
// The spec's values as written.
#![allow(clippy::excessive_precision)]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::config;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{self, StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level_maps::{LevelMaps, MapPicture, NO_MAP, NO_MAP_RECT};
use volya_engine_formats::dds::{Dds, Format};
use volya_engine_formats::ui::Resolution;

/// Section 5.2: name, texture, picture size, `bound_rect`, `global_rect`,
/// in the list's entry order (2).
#[allow(clippy::type_complexity)]
const LEVELS: [(&str, Option<(&str, (u32, u32), [f32; 4])>, [f32; 4]); 17] = [
    (
        "l01_escape",
        Some((
            "map\\map_escape",
            (512, 1024),
            [-317.164, -633.718, 404.880, 811.627],
        )),
        [355.0, 2011.0, 576.0, 2453.0],
    ),
    (
        "l02_garbage",
        Some((
            "map\\map_garbage",
            (1024, 1024),
            [-322.334, -347.225, 321.685, 318.000],
        )),
        [371.0, 1767.0, 561.0, 1963.256226],
    ),
    (
        "l03_agroprom",
        Some((
            "map\\map_agroprom",
            (1024, 1024),
            [-266.505, -355.611, 331.388, 213.838],
        )),
        [161.0, 1834.0, 344.0, 2008.293945],
    ),
    (
        "l03u_agr_underground",
        Some((
            "map\\map_agr_underground",
            (1024, 1024),
            [-137.000, -147.000, -35.302, 50.254],
        )),
        [255.0, 1900.0, 257.0, 1903.879150],
    ),
    (
        "l05_bar",
        Some((
            "map\\map_bar",
            (512, 1024),
            [0.000, -512.030, 512.000, 512.001],
        )),
        [407.0, 1414.0, 556.0, 1712.0],
    ),
    (
        "l06_rostok",
        Some((
            "map\\map_rostok",
            (512, 1024),
            [-512.000, -512.030, 0.000, 512.001],
        )),
        [258.0, 1414.0, 407.0, 1712.0],
    ),
    (
        "l04_darkvalley",
        Some((
            "map\\map_darkvalley",
            (1024, 1024),
            [-445.320, -663.867, 449.864, 231.317],
        )),
        [567.0, 1608.0, 1003.0, 2044.338],
    ),
    (
        "l04u_labx18",
        Some((
            "map\\map_labx18_map",
            (1024, 1024),
            [-51.236, -37.911, 47.999, 81.159],
        )),
        [788.0, 1144.0, 790.0, 1146.399780],
    ),
    (
        "l07_military",
        Some((
            "map\\map_military",
            (1024, 1024),
            [-443.567, -130.466, 196.434, 509.534],
        )),
        [414.0, 1217.0, 604.0, 1406.999756],
    ),
    (
        "l08_yantar",
        Some((
            "map\\map_yantar",
            (1024, 1024),
            [-282.869, -432.963, 287.342, 138.271],
        )),
        [93.0, 1566.0, 256.0, 1729.292480],
    ),
    (
        "l08u_brainlab",
        Some((
            "map\\map_mind",
            (512, 256),
            [-146.670, -41.626, 153.227, 21.611],
        )),
        [172.0, 1612.0, 174.0, 1614.0],
    ),
    (
        "l10_radar",
        Some((
            "map\\map_radar",
            (1024, 1024),
            [-136.256, -503.108, 714.384, 353.524],
        )),
        [527.0, 975.0, 769.0, 1218.704712],
    ),
    (
        "l11_pripyat",
        Some((
            "map\\map_pripyat",
            (1024, 1024),
            [-529.399, -457.516, 574.854, 769.050],
        )),
        [457.0, 586.0, 798.0, 964.770996],
    ),
    (
        "l12_stancia",
        Some((
            "map\\map_aes_1",
            (1024, 1024),
            [-256.776, -747.782, 1398.440, 850.523],
        )),
        [253.0, 74.0, 737.0, 541.358704],
    ),
    (
        "l12_stancia_2",
        Some((
            "map\\map_aes_1",
            (1024, 1024),
            [-256.776, -747.782, 1398.440, 850.523],
        )),
        [253.0, 74.0, 737.0, 541.358704],
    ),
    ("l12u_control_monolith", None, [440.0, 333.0, 442.0, 335.0]),
    ("l12u_sarcofag", None, [440.0, 333.0, 442.0, 335.0]),
];

fn close(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

/// A texture's size and format, `path` without its extension.
fn texture(tree: &dyn Vfs, data: &GameData, path: &str) -> ((u32, u32), Format) {
    let file = data
        .aliases
        .path("$game_textures$", &format!("{path}.dds"))
        .unwrap();
    let bytes = vfs::read(tree, &file).unwrap_or_else(|e| panic!("{file}: {e}"));
    let dds = Dds::parse(&bytes).unwrap();
    ((dds.width, dds.height), dds.format)
}

#[test]
fn level_maps_of_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    let maps = LevelMaps::load(&tree, &data).unwrap();

    // 4: the global map.
    let g = &maps.global;
    assert_eq!(g.picture.texture, "ui\\ui_global_map");
    assert_eq!(g.picture.bound, [0.0, 0.0, 1024.0, 2634.0]);
    assert_eq!(g.max_zoom, 6.0);
    let (size, format) = texture(&tree, &data, "ui\\ui_global_map");
    assert_eq!((size, format), ((1024, 2048), Format::Bc1));
    // 3: the no-map picture.
    let (size, format) = texture(&tree, &data, NO_MAP);
    assert_eq!((size, format), ((64, 64), Format::Bc2));

    // 2, 5.2: the 17 level maps in entry order, their sections and
    // pictures.
    let names: Vec<&str> = maps.levels.iter().map(|l| l.name.as_str()).collect();
    let wanted: Vec<&str> = LEVELS.iter().map(|l| l.0).collect();
    assert_eq!(names, wanted);
    for (level, (name, picture, global)) in maps.levels.iter().zip(LEVELS) {
        for (a, b) in level.global.iter().zip(global) {
            assert!(close(*a, b, 1e-3), "{name} global_rect {:?}", level.global);
        }
        match picture {
            Some((path, size, bound)) => {
                assert_eq!(level.picture.texture, path, "{name}");
                for (a, b) in level.picture.bound.iter().zip(bound) {
                    assert!(
                        close(*a, b, 1e-3),
                        "{name} bound_rect {:?}",
                        level.picture.bound
                    );
                }
                assert_eq!(texture(&tree, &data, path), (size, Format::Bc3), "{name}");
            }
            None => {
                assert_eq!(level.picture.texture, NO_MAP, "{name}");
                assert_eq!(level.picture.bound, NO_MAP_RECT, "{name}");
            }
        }
    }

    // 6.2: the New Game's start on Escape: global (376.56, 2300.57),
    // picture pixel (49.9, 670.8) of 512 × 1024.
    let escape = &maps.levels[0];
    let [gx, gy] = escape.to_global(-246.7263, -134.4339);
    assert!(
        close(gx, 376.56, 0.01) && close(gy, 2300.57, 0.01),
        "{gx} {gy}"
    );
    let [u, v] = escape.picture.point(-246.7263, -134.4339, 512.0);
    assert!(close(u, 49.9, 0.05) && close(v, 670.8, 0.05), "{u} {v}");
    // 6.2: Brainlab's whole z range spans 0.4217 global units.
    let brainlab = &maps.levels[maps.level("l08u_brainlab").unwrap()];
    let b = brainlab.picture.bound;
    let span = brainlab.to_global(b[0], b[1])[1] - brainlab.to_global(b[0], b[3])[1];
    assert!(close(span, 0.4217, 1e-4), "{span}");

    // 8, 9: the bunker has no level map.
    assert_eq!(maps.level("l10u_bunker"), None);

    // 7: every single-player level's minimap section is in game.ltx; the
    // bunker's has no picture.
    let game = config::load(
        &tree,
        &data.aliases.path("$game_config$", "game.ltx").unwrap(),
    )
    .unwrap();
    for name in wanted.iter().chain(&["l10u_bunker"]) {
        assert!(game.contains(name), "{name}");
    }
    let (p, size) = MapPicture::minimap(&game, None, "l01_escape", 100.0).unwrap();
    assert_eq!(p.texture, "map\\map_escape");
    assert!(
        close(size[0], 722.044, 1e-2) && close(size[1], 1445.345, 1e-2),
        "{size:?}"
    );
    let (p, _) = MapPicture::minimap(&game, None, "l10u_bunker", 100.0).unwrap();
    assert_eq!(p.texture, NO_MAP);
}
