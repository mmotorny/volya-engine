// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Configuration files of the Russian edition (BYOA) loaded from the combined
//! archive tree, checked against docs/formats/ltx-config.md (sections 6, 8–10
//! and 12). Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use volya_engine_core::config::{self, Config, PathAliases, Section, value};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::cp1251;
use volya_engine_formats::ltx::{Line, lines};

/// The installer's `fsgame.ltx` (spec section 10), `{userdocs}` replaced.
const FSGAME: &str = r"$app_data_root$ = true | false| C:\Users\Player\Documents\Stalker-SHOC
$game_data$ =     false| true | $fs_root$      | gamedata\
$game_ai$ =       true | false| $game_data$    | ai\
$game_spawn$ =    true | false| $game_data$    | spawns\
$game_anims$ =    true | true | $game_data$    | anims\
$game_levels$ =   true | false| $game_data$    | levels\
$game_meshes$ =   true | true | $game_data$    | meshes\
$game_dm$ =       true | true | $game_data$    | meshes\
$game_shaders$ =  true | true | $game_data$    | shaders\
$game_sounds$ =   true | true | $game_data$    | sounds\
$game_textures$ = true | true | $game_data$    | textures\
$game_scripts$ =  true | false| $game_data$    | scripts\
$game_config$ =   true | false| $game_data$    | config\
$level$ =         false| false| $game_levels$
$game_saves$ =    true | false| $app_data_root$| savedgames\
$logs$ =          true | false| $app_data_root$| logs\
$screenshots$ =   true | false| $app_data_root$| screenshots\
";

/// Paths of the `.ltx` and `.ini` files under `dir`.
fn config_files(vfs: &dyn Vfs, dir: &str, out: &mut Vec<String>) {
    for e in vfs.read_dir(dir).unwrap() {
        let path = format!("{dir}/{}", e.name);
        match e.kind {
            EntryKind::Dir => config_files(vfs, &path, out),
            EntryKind::File if path.ends_with(".ltx") || path.ends_with(".ini") => out.push(path),
            EntryKind::File => {}
        }
    }
}

fn entries(config: &Config) -> usize {
    config.sections().map(Section::len).sum()
}

#[test]
fn loads_russian_configuration() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let vfs = DbVfs::open_game(&root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME.as_bytes(), "").unwrap();

    // Section 10: resolution with the installer's file.
    assert_eq!(aliases.dir("$game_data$").unwrap(), "gamedata\\");
    assert_eq!(aliases.dir("$game_config$").unwrap(), "gamedata\\config\\");
    assert_eq!(aliases.dir("$level$").unwrap(), "gamedata\\levels\\");
    assert_eq!(
        aliases.dir("$game_saves$").unwrap(),
        "c:\\users\\player\\documents\\stalker-shoc\\savedgames\\"
    );

    let start = Instant::now();
    let system = config::load_aliased(&vfs, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(&vfs, &aliases, "$game_config$", "game.ltx").unwrap();
    let textures = config::load_aliased(&vfs, &aliases, "$game_textures$", "textures.ltx").unwrap();
    eprintln!(
        "loaded system, game and textures in {:.1?}",
        start.elapsed()
    );

    // Section 12: sizes.
    assert_eq!(
        (system.len(), entries(&system)),
        (1_245, 57_094),
        "system.ltx"
    );
    assert_eq!((game.len(), entries(&game)), (152, 1_288), "game.ltx");
    assert_eq!(
        (textures.len(), entries(&textures)),
        (3, 10_877),
        "textures.ltx"
    );
    let names: Vec<&str> = textures.sections().map(Section::name).collect();
    assert_eq!(names, ["association", "specification", "types"]);

    // Section 6: ordering.
    assert_eq!(system.sections().next().unwrap().name(), "??tg_event");
    let keys: Vec<&str> = system
        .section("mp_ammo_9x18_fmj")
        .unwrap()
        .entries()
        .take(5)
        .map(|e| e.key())
        .collect();
    assert_eq!(
        keys,
        [
            "$prefetch",
            "$spawn",
            "GroupControlSection",
            "box_size",
            "buck_shot"
        ]
    );

    // Section 12: test vectors.
    let some = |v: &str| Some(Some(v.to_owned()));
    let get = |c: &Config, s: &str, k: &str| c.get(s, k).map(|v| v.map(str::to_owned));
    for (section, key, expected) in [
        ("script", "current_server_entity_version", some("7")),
        ("inventory", "max_belt", some("5")),
        ("inventory", "slot_persistent_1", some("true")),
        ("artefacts_tasks", "script_func", Some(None)),
        ("sound", "snd_collide_max_volume", some("200.")),
        ("helicopter", "light_color", some("0.9,0.8,0.4")),
        ("helicopter", "light_range", some("5")),
        (
            "ammo_9x18_fmj",
            "$spawn",
            some("\"weapons\\ammo\\ammo_9x18_fmj\""),
        ),
        ("mp_ammo_9x18_fmj", "$spawn", some("\"\"")),
        ("mp_ammo_9x18_fmj", "cost", some("70")),
        ("mp_ammo_9x18_fmj", "use_aim_bullet", some("false")),
        ("mp_ammo_9x18_fmj", "discovery_dependency", Some(None)),
        ("MP_AMMO_9x18_FMJ", "k_hit", some("1")),
        ("kurtka_mask_damage", "jaw_1", some("1.0,0.05")),
        (
            "m_dog_step_params",
            "stand_walk_fwd_0",
            some("1,0.2,0.75,0.4,0.7,0.6,0.75,0.8,0.75"),
        ),
    ] {
        assert_eq!(get(&system, section, key), expected, "[{section}] {key}");
    }
    let count = |c: &Config, s: &str| c.section(s).unwrap().len();
    assert_eq!(count(&system, "mp_ammo_9x18_fmj"), 35);
    assert_eq!(count(&system, "ammo_9x18_fmj"), 32);
    assert_eq!(count(&system, "kurtka_mask_damage"), 25);
    assert_eq!(count(&system, "body_kurtka_damage"), 18);
    assert_eq!(count(&system, "head_mask_damage"), 6);
    let files = system.get("texture_desc", "files").unwrap().unwrap();
    assert!(
        files.starts_with("ui_mp_icon_rank,ui_icons_map,ui_common,")
            && files.ends_with(",ui_asus_intro")
            && !files.contains(' '),
        "{files}"
    );
    let levels = config::load_aliased(&vfs, &aliases, "$game_config$", "game_levels.ltx").unwrap();
    assert_eq!(
        get(&levels, "level143", "caption"),
        some("\"l04u_labx18_test")
    );
    let agroprom =
        config::load_aliased(&vfs, &aliases, "$game_config$", "misc\\gulag_agroprom.ltx").unwrap();
    assert_eq!(
        get(&agroprom, "on_hit@agr_factory_hold_fake", "on_info"),
        some("{=hit_by_actor}%+agr_krot_skirmish_start%")
    );
    let font = config::load_aliased(
        &vfs,
        &aliases,
        "$game_textures$",
        "ui\\ui_font_letter_18.ini",
    )
    .unwrap();
    let coords = get(&font, "symbol_coords", "065").unwrap().unwrap();
    assert_eq!(coords, "13,18,22,36");
    assert_eq!(value::ints::<4>(&coords), [13, 18, 22, 36]);

    // Section 7: a few interpretations on real values.
    let v = |s: &str, k: &str| system.get(s, k).unwrap().unwrap();
    assert_eq!(value::int(v("mp_ammo_9x18_fmj", "cost")), 70);
    assert_eq!(value::float(v("sound", "snd_collide_max_volume")), 200.0);
    assert_eq!(
        value::floats::<3>(v("helicopter", "light_color")),
        [0.9, 0.8, 0.4]
    );
    assert!(value::bool(v("inventory", "slot_persistent_1")));
    assert!(!value::bool(v("mp_ammo_9x18_fmj", "use_aim_bullet")));
    assert_eq!(
        value::unquote(v("ammo_9x18_fmj", "$spawn")),
        "weapons\\ammo\\ammo_9x18_fmj"
    );
    assert_eq!(
        value::class_id(v("ammo_9x18_fmj", "class")),
        u64::from_be_bytes(*b"AMMO    ")
    );

    // Section 9: console scripts read as configurations have no sections.
    for name in [
        "default_controls.ltx",
        "rspec_minimum.ltx",
        "rspec_low.ltx",
        "rspec_default.ltx",
        "rspec_high.ltx",
        "rspec_extreme.ltx",
        "rspec_maximum.ltx",
    ] {
        let config = config::load_aliased(&vfs, &aliases, "$game_config$", name).unwrap();
        assert!(config.is_empty(), "{name}");
    }
    // Section 8: a missing root yields an empty configuration.
    assert!(
        config::load_aliased(&vfs, &aliases, "$game_config$", "no_such.ltx")
            .unwrap()
            .is_empty()
    );
    // Section 8: values naming configuration files relative to `$game_config$`.
    let trade = config::load_aliased(
        &vfs,
        &aliases,
        "$game_config$",
        "misc\\gulag_mil_freedom1.ltx",
    )
    .unwrap();
    assert!(
        trade
            .sections()
            .any(|s| s.get("trade") == Some(Some("misc\\trade_freedom.ltx")))
    );
    assert!(
        system
            .sections()
            .any(|s| s.get("custom_data") == Some(Some("stalker_custom_data.ltx")))
    );
}

#[test]
fn every_russian_config_file_parses() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let vfs = DbVfs::open_game(&root, Some("mods")).unwrap();
    let start = Instant::now();
    let mut paths = Vec::new();
    config_files(&vfs, "gamedata", &mut paths);
    let ini = paths.iter().filter(|p| p.ends_with(".ini")).count();
    assert_eq!((paths.len(), paths.len() - ini, ini), (625, 523, 102));

    let mut bytes = 0;
    let mut outside = 0;
    let mut value_paths = BTreeSet::new();
    let (mut headers, mut with_parents, mut entry_lines, mut include_lines) = (0, 0, 0, 0);
    // Included path -> number of include lines naming it.
    let mut included: BTreeMap<String, usize> = BTreeMap::new();
    for path in &paths {
        let text = read(&vfs, path).unwrap();
        bytes += text.len();
        let dir = &path[..=path.rfind('/').unwrap()];
        let mut in_section = false;
        for line in lines(&text) {
            match line.unwrap_or_else(|e| panic!("{path}: {e}")).line {
                Line::Include(include) => {
                    include_lines += 1;
                    let target = format!("{dir}{}", cp1251::decode(&include)).to_ascii_lowercase();
                    let target = volya_engine_core::vfs::normalize(&target).unwrap();
                    assert!(vfs.open(&target).is_ok(), "{path}: include {target}");
                    *included.entry(target).or_default() += 1;
                }
                Line::Header { parents, .. } => {
                    in_section = true;
                    headers += 1;
                    with_parents += usize::from(!parents.is_empty());
                }
                Line::Entry { value, .. } if in_section => {
                    entry_lines += 1;
                    let value = cp1251::decode(&value.unwrap_or_default());
                    if value.to_ascii_lowercase().ends_with(".ltx") {
                        value_paths.insert(value);
                    }
                }
                Line::Entry { .. } => outside += 1,
            }
        }
    }
    eprintln!(
        "{} files, {bytes} bytes, {headers} headers ({with_parents} with parents), \
         {entry_lines} entry lines ({outside} outside sections), {include_lines} includes",
        paths.len()
    );
    assert_eq!(bytes, 4_025_893);
    assert_eq!((headers, with_parents), (6_895, 637));
    // The spec's count matches entry lines inside a section (190 more come
    // before a file's first header and are ignored).
    assert_eq!((entry_lines, outside), (90_182, 190));
    assert_eq!((include_lines, included.len()), (177, 177));
    // Section 8: every `.ltx` path in a value exists under `gamedata\config\`.
    assert_eq!(value_paths.len(), 12, "{value_paths:?}");
    for path in &value_paths {
        let full = format!("gamedata\\config\\{path}");
        assert!(vfs.open(&full).is_ok(), "{full}");
    }

    // Every file nothing includes loads as a root, except three fragments
    // whose parents are defined elsewhere.
    let mut failed = Vec::new();
    let mut roots = 0;
    for path in paths.iter().filter(|p| !included.contains_key(*p)) {
        roots += 1;
        if let Err(e) = config::load(&vfs, path) {
            assert!(e.to_string().contains("missing parent"), "{e}");
            failed.push(path.as_str());
        }
    }
    assert_eq!(roots, 448);
    assert_eq!(
        failed,
        [
            "gamedata/config/creatures/m_fraction.ltx",
            "gamedata/config/prefetch/old_variant_prefetch_single.ltx",
            "gamedata/config/weapons/w_mac10.ltx",
        ]
    );
    assert!(
        config::load(&vfs, "gamedata/config/misc/gulag_general.ltx")
            .unwrap()
            .is_empty()
    );
    eprintln!("parsed and loaded every file in {:.1?}", start.elapsed());
}
