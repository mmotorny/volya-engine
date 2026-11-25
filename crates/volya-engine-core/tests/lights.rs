// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Game-data tests of the colour animations (docs/formats/lanims-xr.md
//! sections 2, 3, 6.1, 7, 8) and the levels' lights and glows
//! (docs/behavior/dynamic-lights.md 10, glows.md 7). Skipped without
//! `VOLYA_GOG_INSTALLER`.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet};

use volya_engine_core::lights::{LevelLight, LightKind, Lights};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::lanims::{ColourAnimations, Entry};
use volya_engine_formats::level::{LIGHT_DIRECTIONAL, LIGHT_POINT, Level, split_shader_entry};

/// Section 7: name, rate, frames, keys, last key frame, length in ms,
/// alpha values, CRC-32 of the colours at frames 0 … frame count.
#[allow(clippy::type_complexity)]
const ENTRIES: [(&str, u32, i32, usize, i32, i32, &[u32], u32); 65] = [
    ("spot_blim", 200, 50, 10, 45, 250, &[0], 0x3A14F57B),
    ("torch_blink", 100, 30, 7, 8, 300, &[0], 0x23722D5B),
    ("warning_01", 15, 10, 3, 9, 666, &[0], 0xF04AEA5F),
    ("svet_00", 30, 30, 3, 29, 1000, &[0], 0xB2899AEA),
    ("light 40 hz", 10, 100, 12, 58, 10000, &[0], 0xE3DDAAAB),
    ("light alarm 1", 30, 25, 4, 24, 833, &[0], 0x5CABFE1F),
    ("light alarm 2", 15, 15, 3, 5, 1000, &[0], 0x99C0C4C1),
    ("tv light 1", 15, 60, 9, 59, 4000, &[0], 0xEB736E77),
    ("explode_light", 20, 30, 7, 29, 1500, &[0], 0x14610579),
    ("svet_01", 30, 30, 4, 29, 1000, &[0], 0xD111A0B8),
    ("svet_02", 30, 30, 3, 29, 1000, &[0], 0xB517F2AE),
    ("alarm_r", 15, 15, 3, 14, 1000, &[0], 0x0A57792B),
    ("galogen_br", 15, 60, 10, 54, 4000, &[0], 0xDE95B578),
    ("light_green", 10, 40, 6, 39, 4000, &[0], 0x4DE1FF2D),
    ("koster", 30, 60, 14, 59, 2000, &[0], 0x7425B6D0),
    ("koster_00", 15, 60, 16, 59, 4000, &[0], 0x41F70164),
    ("koster_01", 20, 60, 18, 57, 3000, &[0], 0x65AFB891),
    (
        "ui_pda_contacts",
        5,
        11,
        11,
        10,
        2200,
        &[0, 255],
        0x13862BB6,
    ),
    ("ui_new_jobs", 15, 1, 1, 0, 66, &[255], 0x2144DF1C),
    (
        "ui_minimap_enemy",
        20,
        500,
        7,
        499,
        25000,
        &[0, 255],
        0xD61B8B40,
    ),
    ("ui_minimap_neutral", 15, 1, 1, 0, 66, &[255], 0x66AC3B0D),
    ("ui_minimap_friend", 15, 1, 1, 0, 66, &[255], 0xAEEA89B5),
    ("ui_claws_animation", 15, 1, 1, 0, 66, &[255], 0x2144DF1C),
    (
        "ui_main_msgs",
        15,
        200,
        3,
        199,
        13333,
        &[0, 254, 255],
        0xB171C0BC,
    ),
    ("ui_map_area_anim", 15, 10, 2, 9, 666, &[0, 255], 0x5B5AFF5E),
    ("ui_mp_chat", 30, 240, 3, 235, 8000, &[0, 255], 0xEB48E4F0),
    (
        "light\\light alarm_idle",
        10,
        60,
        12,
        59,
        6000,
        &[0],
        0x7B497D31,
    ),
    (
        "light\\light_alarm_blim",
        15,
        60,
        17,
        59,
        4000,
        &[0],
        0xA08DC7E3,
    ),
    ("light_green_00", 15, 30, 6, 25, 2000, &[0], 0x0D436A99),
    (
        "light\\light_omni_white_idle",
        15,
        60,
        5,
        59,
        4000,
        &[0],
        0x70FEFD7B,
    ),
    (
        "light\\light_omni_blue_idle",
        15,
        60,
        8,
        59,
        4000,
        &[0],
        0xD7447C68,
    ),
    ("light\\omni_01", 15, 1, 1, 0, 66, &[0], 0x57F579B6),
    (
        "light\\omni_01_br",
        15,
        120,
        17,
        119,
        8000,
        &[0],
        0xCD1C0E8E,
    ),
    ("light\\omni_02", 15, 1, 1, 0, 66, &[0], 0x546958E0),
    (
        "levels\\psy_antenna\\starting",
        5,
        100,
        21,
        99,
        20000,
        &[0, 10, 20, 30, 40, 50, 90, 120],
        0x0F8A0B22,
    ),
    (
        "levels\\psy_antenna\\working",
        5,
        100,
        27,
        99,
        20000,
        &[0, 20, 50, 70, 80, 90, 100, 120, 150],
        0x91057D3C,
    ),
    (
        "levels\\psy_antenna\\stopping",
        10,
        100,
        8,
        99,
        10000,
        &[0, 10, 40, 255],
        0x9EFC8B2C,
    ),
    (
        "weathers\\thunderbolt_00",
        50,
        50,
        6,
        49,
        1000,
        &[0],
        0xA0E2F34C,
    ),
    (
        "ui_mp_minimap_af_neutral",
        20,
        31,
        4,
        30,
        1550,
        &[0, 255],
        0x785CECBC,
    ),
    (
        "ui_mp_minimap_af_friend",
        20,
        31,
        4,
        30,
        1550,
        &[0, 255],
        0x2AC116DE,
    ),
    (
        "ui_mp_minimap_af_enemy",
        20,
        31,
        4,
        30,
        1550,
        &[0, 255],
        0x38817DB5,
    ),
    (
        "ui_mm_mp_srvinfo",
        30,
        30,
        2,
        29,
        1000,
        &[0, 255],
        0x576AC204,
    ),
    ("ui_task_selected", 15, 30, 3, 29, 2000, &[255], 0xD210DF1E),
    (
        "ui_slow_blinking",
        30,
        30,
        3,
        29,
        1000,
        &[100, 255],
        0x5845510B,
    ),
    (
        "ui_slow_blinking_alpha",
        30,
        30,
        3,
        28,
        1000,
        &[72, 255],
        0x4B09B447,
    ),
    (
        "ui_too_slow_blinking",
        17,
        30,
        3,
        29,
        1764,
        &[100, 255],
        0x5845510B,
    ),
    (
        "hud_hit_mark",
        15,
        30,
        3,
        29,
        2000,
        &[0, 200, 230],
        0x282891B7,
    ),
    (
        "monster_claws",
        60,
        30,
        3,
        29,
        500,
        &[0, 200, 230],
        0x282891B7,
    ),
    ("ui_btn_hint", 30, 30, 2, 29, 1000, &[0, 255], 0x576AC204),
    (
        "ui_main_msgs_short",
        15,
        36,
        2,
        35,
        2400,
        &[0, 255],
        0x9A8BF5F3,
    ),
    ("light_green_01", 15, 30, 6, 28, 2000, &[0], 0xC385B1CF),
    ("light_green_02", 10, 40, 6, 39, 4000, &[0], 0x4DE1FF2D),
    ("light\\omni_03", 15, 1, 1, 0, 66, &[0], 0xB64B8211),
    (
        "light\\light_galogen_br_01",
        30,
        60,
        18,
        58,
        2000,
        &[0],
        0xDF55ACBD,
    ),
    ("credits_vis", 15, 75, 4, 74, 5000, &[0, 255], 0x2551D553),
    (
        "credits_vis_sub",
        15,
        60,
        4,
        59,
        4000,
        &[0, 255],
        0x0A87507F,
    ),
    ("credits_head", 15, 15, 2, 14, 1000, &[0, 255], 0x6E1A6BE7),
    ("buy_menu_info", 15, 75, 5, 74, 5000, &[0, 255], 0x401CF456),
    (
        "tutor_rad_sign_xform",
        15,
        41,
        5,
        40,
        2733,
        &[0, 64, 128, 192, 255],
        0xD1864C9A,
    ),
    (
        "tutor_rad_sign_clr",
        15,
        15,
        3,
        14,
        1000,
        &[64, 128],
        0x41CCB7F6,
    ),
    ("map_spot_rel", 30, 15, 3, 14, 500, &[0, 255], 0x4BFC4B0D),
    ("map_spot_rel_xform", 6, 15, 5, 14, 2500, &[0], 0xC2F3D3A4),
    ("map_spot_new_xform", 6, 15, 5, 14, 2500, &[0], 0x1CE9A8AE),
    (
        "teleport_stancia_in_01",
        30,
        30,
        7,
        24,
        1000,
        &[0],
        0xE6F5F27C,
    ),
    ("asus_logo_01", 30, 120, 4, 119, 4000, &[0, 255], 0x922F3B21),
];

fn with_tree(f: impl FnOnce(&dyn Vfs)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, None).unwrap();
    f(&tree);
}

/// The colours at frames 0 … frame count, little-endian.
fn frame_bytes(e: &Entry) -> Vec<u8> {
    (0..=e.frame_count)
        .flat_map(|f| e.colour_at_frame(f).unwrap().to_le_bytes())
        .collect()
}

#[test]
fn colour_animations_from_game_data() {
    with_tree(|tree| {
        let file = read(tree, "gamedata/lanims.xr").unwrap();
        // Section 2: the dba copy (identical to db0's).
        assert_eq!((file.len(), crc32fast::hash(&file)), (6_665, 0xBFA9_BD87));
        let lib = ColourAnimations::parse(&file).unwrap();
        assert_eq!(lib.version, 1);
        assert_eq!(lib.entries.len(), 65);
        let mut all = Vec::new();
        let mut keys = 0;
        for (i, (e, &(name, rate, frames, count, last, ms, alpha, crc))) in
            lib.entries.iter().zip(&ENTRIES).enumerate()
        {
            assert_eq!(e.name, name.as_bytes());
            assert_eq!(lib.find(name.as_bytes()), Some(i), "{name}");
            assert_eq!(
                (e.frame_rate, e.frame_count),
                (rate as f32, frames),
                "{name}"
            );
            assert_eq!(e.keys.len(), count, "{name}");
            assert_eq!(e.keys.last().unwrap().0, last, "{name}");
            assert_eq!(e.length_ms(), ms, "{name}");
            let alphas: BTreeSet<u32> = e.keys.iter().map(|k| k.1 >> 24).collect();
            assert_eq!(alphas.into_iter().collect::<Vec<_>>(), alpha, "{name}");
            let bytes = frame_bytes(e);
            assert_eq!(crc32fast::hash(&bytes), crc, "{name}");
            all.extend(bytes);
            keys += e.keys.len();
            // Every entry has a key at frame 0; only svet_01 one before.
            assert!(e.keys.iter().any(|k| k.0 == 0), "{name}");
            assert_eq!(e.keys[0].0 < 0, name == "svet_01", "{name}");
        }
        assert_eq!(keys, 405);
        assert_eq!(all.len() / 4, 3_453);
        assert_eq!(crc32fast::hash(&all), 0x6B7D_7C28);

        // Section 6.1: the f32 rounding makes 127 channel values in 75
        // frames one less than the exact value, none otherwise.
        let (mut channels, mut frames) = (0, 0);
        for e in &lib.entries {
            for f in 0..=e.frame_count {
                let at = e.keys.partition_point(|k| k.0 < f);
                let Some(&(f1, c1)) = e.keys.get(at).filter(|k| k.0 != f && at > 0) else {
                    continue;
                };
                let (f0, c0) = e.keys[at - 1];
                let got = e.colour_at_frame(f).unwrap();
                let mut low = 0;
                for s in [0, 8, 16, 24] {
                    let (a, b) = (i64::from(c0 >> s & 0xFF), i64::from(c1 >> s & 0xFF));
                    let span = i64::from(f1 - f0);
                    let exact = (a * span + i64::from(f - f0) * (b - a)).div_euclid(span);
                    let g = i64::from(got >> s & 0xFF);
                    assert!(g == exact || g == exact - 1, "{:?} frame {f}", e.name);
                    low += i32::from(g != exact);
                }
                channels += low;
                frames += i32::from(low > 0);
            }
        }
        assert_eq!((channels, frames), (127, 75));

        // Section 8.
        let at = |name: &str, t: f32| lib.entries[lib.find(name.as_bytes()).unwrap()].colour_at(t);
        for (name, t, frame, colour) in [
            ("light 40 hz", 0.0, 0, 0x00FF_D5A5),
            ("light 40 hz", 0.35, 3, 0x00FF_DEAB),
            ("light 40 hz", 1.15, 11, 0x00FF_EBB3),
            ("light 40 hz", 10.35, 3, 0x00FF_DEAB),
            ("tv light 1", 0.5, 7, 0x0042_7DAF),
            ("tv light 1", 2.0, 30, 0x0050_AFFD),
            ("koster_00", 1.3, 19, 0x00C5_8F34),
            ("koster", 1.27, 38, 0x00FF_A553),
            ("weathers\\thunderbolt_00", 0.6, 30, 0x003F_3F3F),
            ("asus_logo_01", 0.5, 15, 0x7700_0000),
            ("asus_logo_01", 3.0, 90, 0xD900_0000),
            ("spot_blim", 0.24, 48, 0x0000_0000),
            ("svet_01", 0.0, 0, 0x00FF_C066),
            ("ui_new_jobs", 123.4, 0, 0xFFFF_FFFF),
            ("ui_minimap_enemy", 10.05, 201, 0xB9B9_2300),
            ("warning_01", f32::from_bits(0x3F2A_AAAA), 9, 0x00AB_1111),
        ] {
            assert_eq!(at(name, t), (colour, frame), "{name} at {t}");
        }
        // The keys of startup.md 6.4.
        let asus = &lib.entries[lib.find(b"asus_logo_01").unwrap()];
        assert_eq!(
            asus.keys,
            vec![(0, 0), (32, 0xFF00_0000), (85, 0xFF00_0000), (119, 0)]
        );
    });
}

/// Directories under `gamedata/levels` holding a `level`.
fn all_levels(vfs: &dyn Vfs) -> Vec<String> {
    vfs.read_dir("gamedata/levels")
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EntryKind::Dir)
        .map(|e| e.name)
        .filter(|l| vfs.open(&format!("gamedata/levels/{l}/level")).is_ok())
        .collect()
}

fn level_lights(level: &Level) -> Vec<LevelLight> {
    level
        .lights
        .iter()
        .filter_map(|l| match l.kind {
            LIGHT_POINT => Some(LevelLight::Point {
                colour: l.diffuse,
                position: l.position,
                range: l.range,
            }),
            LIGHT_DIRECTIONAL => Some(LevelLight::Directional {
                direction: l.direction,
            }),
            _ => None,
        })
        .collect()
}

#[test]
fn level_lights_and_glows_from_game_data() {
    with_tree(|tree| {
        // dynamic-lights.md 10.1, glows.md 7 for l01_escape.
        let level = Level::parse(&read(tree, "gamedata/levels/l01_escape/level").unwrap()).unwrap();
        assert_eq!(level.lights.len(), 41);
        let sun = &level.lights[0];
        assert_eq!(sun.kind, LIGHT_DIRECTIONAL);
        for (a, b) in sun.direction.iter().zip([0.840, -0.423, 0.340]) {
            assert!((a - b).abs() < 5e-4, "{:?}", sun.direction);
        }
        let points = &level.lights[1..];
        assert!(points.iter().all(|l| l.kind == LIGHT_POINT));
        let ranges: Vec<f32> = points.iter().map(|l| l.range).collect();
        assert_eq!(ranges.iter().copied().fold(f32::MAX, f32::min), 7.0);
        assert_eq!(ranges.iter().copied().fold(f32::MIN, f32::max), 12.0);
        assert!((ranges.iter().sum::<f32>() - 366.0).abs() < 1e-3);
        // As records (section 3.1): none can be listed.
        let mut lights = Lights::default();
        lights.add_level_lights(&level_lights(&level)).unwrap();
        assert_eq!(lights.len(), 42);
        assert!(lights.iter().all(|(_, r)| r.excluded));
        assert_eq!(lights.iter().filter(|(_, r)| r.active).count(), 40);
        assert!(
            lights
                .iter()
                .filter(|(_, r)| r.kind == LightKind::Directional)
                .all(|(_, r)| !r.active)
        );

        assert_eq!(level.glows.len(), 22);
        let mut by: BTreeMap<(String, String), Vec<f32>> = BTreeMap::new();
        for g in &level.glows {
            let (shader, textures) = split_shader_entry(&level.shaders[g.shader as usize]).unwrap();
            by.entry((shader.to_owned(), textures.to_owned()))
                .or_default()
                .push(g.radius);
        }
        let counts: Vec<(&str, &str, usize)> = by
            .iter()
            .map(|((s, t), r)| (s.as_str(), t.as_str(), r.len()))
            .collect();
        assert_eq!(
            counts,
            [
                ("effects\\glow", "glow\\glow_04", 12),
                ("effects\\glow", "glow\\glow_red2", 6),
                ("effects\\glow", "glow\\glow_white", 4)
            ]
        );
        assert!(by.values().next().unwrap().iter().all(|&r| r == 1.0));
        let sum: f32 = level.glows.iter().map(|g| g.radius).sum();
        assert!((sum - 17.4).abs() < 1e-4);
        let g0 = &level.glows[0];
        assert_eq!(g0.radius, 1.0);
        for (a, b) in g0.position.iter().zip([30.484, 19.756, 679.066]) {
            assert!((a - b).abs() < 1e-3, "{:?}", g0.position);
        }

        // dynamic-lights.md 10.2, glows.md 7: every level.
        let names = all_levels(tree);
        assert_eq!(names.len(), 30);
        let (mut records, mut glows) = (0, 0);
        let mut shaders: BTreeMap<String, usize> = BTreeMap::new();
        for name in &names {
            let level =
                Level::parse(&read(tree, &format!("gamedata/levels/{name}/level")).unwrap())
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
            records += level.lights.len();
            assert_eq!(level.lights[0].kind, LIGHT_DIRECTIONAL, "{name}");
            let suns = level.lights.iter().filter(|l| l.kind == LIGHT_DIRECTIONAL);
            assert_eq!(suns.count(), 1, "{name}");
            for l in &level.lights {
                let leading = if l.kind == LIGHT_DIRECTIONAL { 1 } else { 2 };
                assert_eq!(l.leading, leading, "{name}");
            }
            glows += level.glows.len();
            for g in &level.glows {
                let (shader, _) = split_shader_entry(&level.shaders[g.shader as usize]).unwrap();
                *shaders.entry(shader.to_owned()).or_default() += 1;
            }
            let radii = level.glows.iter().map(|g| g.radius);
            assert!(radii.clone().all(|r| (0.1..=5.0).contains(&r)), "{name}");
        }
        assert_eq!(records, 1_682);
        assert_eq!(glows, 1_166);
        assert_eq!(
            shaders.into_iter().collect::<Vec<_>>(),
            [
                ("effects\\flare".to_owned(), 1),
                ("effects\\glow".to_owned(), 1_165)
            ]
        );
    });
}
