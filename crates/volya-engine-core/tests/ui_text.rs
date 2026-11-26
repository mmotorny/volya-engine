// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! UI layout data, texture descriptions, sequences, string tables and fonts
//! of the Russian edition (BYOA), checked against docs/formats/ui-layout.md,
//! docs/formats/text-and-fonts.md and docs/behavior/title-menu.md. Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_2;
use std::time::Instant;

use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::text::{self, Fonts, Localization, StringTable};
use volya_engine_core::ui::{self, TextureSource};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::cp1251;
use volya_engine_formats::text::font::{GlyphMetrics, UiFont};
use volya_engine_formats::text::layout::{Surface, TextBox, layout_text};
use volya_engine_formats::ui::layout::{HAlign, Rect, VAlign};
use volya_engine_formats::ui::picture::{Picture, PicturePlacement};
use volya_engine_formats::ui::seq::Sequence;
use volya_engine_formats::ui::texture_desc::{self, TextureDescriptions};
use volya_engine_formats::ui::title_menu::TitleMenuLayout;
use volya_engine_formats::ui::{Color, Resolution};
use volya_engine_formats::xml;

/// The aliases of the installer's `fsgame.ltx` that these files use.
const FSGAME: &[u8] = b"$game_data$ = false| true | $fs_root$ | gamedata\\\r\n\
    $game_config$ = true | false| $game_data$ | config\\\r\n\
    $game_textures$ = true | true | $game_data$ | textures\\\r\n";

const XGA: Resolution = Resolution::new(1024, 768);
const SVGA: Resolution = Resolution::new(800, 600);
const HD: Resolution = Resolution::new(1920, 1080);

struct Game {
    vfs: DbVfs<Box<dyn volya_engine_core::vfs::VfsFile + 'static>>,
    aliases: PathAliases,
}

fn game() -> Option<Game> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let start = Instant::now();
    let vfs = DbVfs::open_game(root, Some("mods")).unwrap();
    eprintln!("mounted archives in {:.1?}", start.elapsed());
    Some(Game {
        vfs,
        aliases: PathAliases::parse(FSGAME, "").unwrap(),
    })
}

/// Files under `dir` (recursively) whose names end in `suffix`.
fn files(vfs: &dyn Vfs, dir: &str, suffix: &str, out: &mut Vec<String>) {
    for e in vfs.read_dir(dir).unwrap() {
        let path = format!("{dir}/{}", e.name);
        match e.kind {
            EntryKind::Dir => files(vfs, &path, suffix, out),
            EntryKind::File if e.name.ends_with(suffix) => out.push(path),
            EntryKind::File => {}
        }
    }
}

/// Width and height of a DDS image from its header (public DDS layout:
/// magic, header size, flags, then height and width).
fn dds_size(vfs: &dyn Vfs, path: &str) -> (u32, u32) {
    let data = read(vfs, path).unwrap();
    assert_eq!(&data[..4], b"DDS ", "{path}");
    let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    (u32_at(16), u32_at(12))
}

#[test]
fn loads_every_ui_layout_file() {
    let Some(g) = game() else { return };
    let mut names = Vec::new();
    files(&g.vfs, "gamedata/config/ui", ".xml", &mut names);
    let names: Vec<&str> = names
        .iter()
        .map(|p| p.rsplit('/').next().unwrap())
        .collect();
    assert_eq!(names.len(), 154);
    let variants: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| n.ends_with("_16.xml"))
        .collect();
    assert_eq!(variants.len(), 24, "{variants:?}");
    for v in &variants {
        let normal = v.replace("_16.xml", ".xml");
        assert!(names.contains(&normal.as_str()), "{v} without {normal}");
    }

    // Section 7.2: the eight never-loaded files with `header`, `normal` or
    // `graffiti18` fail with the fatal unknown-font error; all others load.
    const UNKNOWN_FONT: [&str; 8] = [
        "carbody.xml",
        "inventory.xml",
        "inventorymp_new.xml",
        "inventorymp_new2.xml",
        "jobs_new.xml",
        "maingame_new.xml",
        "map.xml",
        "trade_new.xml",
    ];
    let start = Instant::now();
    for resolution in [XGA, HD] {
        for name in &names {
            let loaded = ui::load_ui_file(&g.vfs, &g.aliases, name, resolution);
            if UNKNOWN_FONT.contains(name) {
                let err = loaded.expect_err(name).to_string();
                assert!(
                    ["header", "normal", "graffiti18"]
                        .iter()
                        .any(|f| err.contains(&format!("unknown font \"{f}\""))),
                    "{name}: {err}"
                );
            } else {
                loaded.unwrap_or_else(|e| panic!("{name} at {resolution:?}: {e}"));
            }
        }
    }
    eprintln!(
        "loaded {} UI files twice in {:.1?}",
        names.len(),
        start.elapsed()
    );
    // Section 1.4 and Evidence summary: the first element after include
    // expansion of each file loaded on its own.
    let mut roots = BTreeMap::<String, usize>::new();
    let mut without = Vec::new();
    for name in &names {
        let (_, text) = ui::read_ui_file(&g.vfs, &g.aliases, name, XGA).unwrap();
        match xml::parse(&text).unwrap().root() {
            Some(root) => *roots.entry(root.name.clone()).or_default() += 1,
            None => without.push(*name),
        }
    }
    without.sort();
    assert_eq!(
        without,
        [
            "follower_character.xml",
            "inventory_character.xml",
            "map_spots_darkvalley.xml",
            "map_spots_military.xml",
            "map_spots_rostok.xml",
            "pda_dialog.xml",
        ]
    );
    assert_eq!(roots.remove("window"), Some(70));
    assert_eq!(roots.remove("ui_texture"), Some(23));
    assert_eq!(roots.remove("w"), Some(22));
    assert_eq!(roots.len(), 29, "other names: {roots:?}");
    assert_eq!(roots.values().sum::<usize>(), 33);
    assert!(roots.contains_key("colors") && roots.contains_key("map_spots"));

    // Section 1.1: which file each resolution loads.
    let found = |name, r| {
        ui::find_ui_file(&g.vfs, &g.aliases, name, r)
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        found("maingame.xml", HD),
        "gamedata\\config\\ui\\maingame_16.xml"
    );
    assert_eq!(
        found("maingame.xml", XGA),
        "gamedata\\config\\ui\\maingame.xml"
    );
    assert_eq!(
        found("ui_mm_main.xml", HD),
        "gamedata\\config\\ui\\ui_mm_main.xml"
    );
    // Section 1.2: `carbody_item_16.xml` includes the `_16` parameters.
    let (_, wide) = ui::read_ui_file(&g.vfs, &g.aliases, "carbody_item.xml", HD).unwrap();
    let (_, normal) = ui::read_ui_file(&g.vfs, &g.aliases, "carbody_item.xml", XGA).unwrap();
    let contains = |text: &[u8], file: &str| {
        let part = xml::preprocess(&read(&g.vfs, file).unwrap(), &mut |_| unreachable!()).unwrap();
        text.windows(part.len()).any(|w| w == part)
    };
    assert!(contains(&wide, "gamedata/config/ui/af_params_16.xml"));
    assert!(!contains(&wide, "gamedata/config/ui/af_params.xml"));
    assert!(contains(&normal, "gamedata/config/ui/af_params.xml"));
    // Section 1.1 example: motion_icon_16 narrows by about 0.75.
    let colors = ui::load_color_defs(&g.vfs, &g.aliases, XGA).unwrap();
    let motion = ui::load_ui_file(&g.vfs, &g.aliases, "motion_icon.xml", HD).unwrap();
    let background = volya_engine_formats::ui::layout::StaticDesc::read(
        motion.find("background", 0).unwrap(),
        &colors,
    )
    .unwrap();
    assert_eq!(background.window.rect(), Rect::new(0.0, 640.0, 94.0, 125.0));
    assert!(background.stretch);

    // Text-and-fonts section 4.1 / ui-layout section 2.
    for (name, rgb) in [
        ("UI_orange", (238, 153, 26)),
        ("ui_clr_1", (240, 217, 182)),
        ("ui_clr_2", (216, 186, 140)),
        ("ui_clr_3", (135, 123, 116)),
        ("white", (255, 255, 255)),
    ] {
        assert_eq!(
            colors.get(name),
            Some(Color::rgba(rgb.0, rgb.1, rgb.2, 255)),
            "{name}"
        );
    }
    for name in ["red", "green", "blue", "gray"] {
        assert!(colors.get(name).is_some(), "{name}");
    }
}

#[test]
fn loads_texture_descriptions_and_sequences() {
    let Some(g) = game() else { return };
    let system = config::load_aliased(&g.vfs, &g.aliases, "$game_config$", "system.ltx").unwrap();
    let names = texture_desc::file_names(&system);
    assert_eq!(names.len(), 23);
    assert_eq!(names[0], "ui_mp_icon_rank");
    assert_eq!(names[22], "ui_asus_intro");
    for name in &names {
        let file = format!("{name}.xml");
        assert_eq!(
            ui::find_ui_file(&g.vfs, &g.aliases, &file, HD).unwrap(),
            ui::find_ui_file(&g.vfs, &g.aliases, &file, XGA).unwrap(),
            "{name} has no _16 variant"
        );
    }
    let start = Instant::now();
    let all = ui::load_texture_descriptions(&g.vfs, &g.aliases, &system, XGA).unwrap();
    eprintln!("loaded texture descriptions in {:.1?}", start.elapsed());
    assert_eq!(all.len(), 1_131);
    let per_file = |name: &str| {
        let mut d = TextureDescriptions::default();
        d.add_file(&ui::load_ui_file(&g.vfs, &g.aliases, &format!("{name}.xml"), XGA).unwrap())
            .unwrap();
        d.len()
    };
    let total: usize = names.iter().map(|n| per_file(n)).sum();
    assert_eq!(total, 1_131, "no duplicate ids");
    for (name, count) in [
        ("ui_common", 478),
        ("ui_iconstotal", 121),
        ("ui_npc_unique", 109),
        ("ui_old_textures", 95),
        ("ui_hud", 90),
        ("ui_mainmenu", 48),
        ("ui_buy_menu", 29),
        ("ui_numpad", 22),
        ("ui_inventory2", 21),
        ("ui_npc_monster", 19),
        ("ui_models_multiplayer", 18),
        ("ui_map_description", 16),
        ("ui_inventory", 16),
        ("ui_icons_npc", 16),
        ("ui_statistics", 13),
        ("ui_mp_icon_rank", 5),
        ("ui_mp_main", 5),
    ] {
        assert_eq!(per_file(name), count, "{name}");
    }
    // Title-menu ids (title-menu.md section 3.1).
    for (id, rect) in [
        ("ui_menu_backgraund", (0.0, 0.0, 1024.0, 768.0)),
        ("ui_menu_move", (7.0, 951.0, 392.0, 73.0)),
        ("ui_menu_bigroll_l", (723.0, 878.0, 86.0, 86.0)),
        ("ui_menu_bigroll_r", (635.0, 878.0, 86.0, 86.0)),
        ("ui_menu_grating_l", (865.0, 878.0, 46.0, 70.0)),
        ("ui_menu_grating_r", (810.0, 878.0, 55.0, 67.0)),
        ("ui_menu_roll_L_01", (0.0, 768.0, 55.0, 55.0)),
    ] {
        let d = all.get(id).unwrap();
        assert_eq!(d.file, "ui\\ui_mainMenu", "{id}");
        assert_eq!(d.rect, Rect::new(rect.0, rect.1, rect.2, rect.3), "{id}");
    }
    for i in 1..=20 {
        assert!(all.get(&format!("ui_menu_roll_L_{i:02}")).is_some());
        assert!(all.get(&format!("ui_menu_roll_R_{i:02}")).is_some());
    }
    assert!(all.get("UI_MENU_MOVE").is_none(), "ids are case-sensitive");
    // Every description's texture is a texture.
    let mut textures: Vec<&str> = all.iter().map(|(_, d)| d.file.as_str()).collect();
    textures.sort();
    textures.dedup();
    let missing: Vec<&str> = textures
        .iter()
        .copied()
        .filter(|t| ui::texture_source(&g.vfs, &g.aliases, t).is_err())
        .collect();
    // A missing texture is fatal only when a screen uses its ids.
    assert_eq!(textures.len(), 23);
    assert_eq!(missing, ["ui\\ui_mp_icon_rank"]);
    assert_eq!(
        ui::texture_source(&g.vfs, &g.aliases, "ui\\ui_mainMenu").unwrap(),
        TextureSource::Image("gamedata\\textures\\ui\\ui_mainmenu.dds".into())
    );
    assert_eq!(
        dds_size(&g.vfs, "gamedata/textures/ui/ui_mainmenu.dds"),
        (1024, 1024)
    );

    // Section 6.4: every sequence.
    let mut seqs = Vec::new();
    files(&g.vfs, "gamedata/textures", ".seq", &mut seqs);
    assert_eq!(seqs.len(), 11, "{seqs:?}");
    let mut parsed = BTreeMap::new();
    for path in &seqs {
        let s = Sequence::parse(&read(&g.vfs, path).unwrap()).unwrap();
        assert!(!s.cycled, "{path}");
        for frame in &s.frames {
            ui::still_image(&g.vfs, &g.aliases, frame).unwrap_or_else(|e| panic!("{path}: {e}"));
        }
        let name = path.trim_start_matches("gamedata/textures/").to_owned();
        parsed.insert(name, s);
    }
    for (name, fps, period, frames) in [
        ("ui/ui_ani_cursor.seq", 19, 52, 8),
        ("ui/ui_noise.seq", 25, 40, 11),
        ("fx/fx_stalker.seq", 5, 200, 30),
        ("fx/fx_wood_fire.seq", 15, 66, 20),
        ("water/water_normal.seq", 27, 37, 29),
        ("artifact/artifact_hell1.seq", 35, 28, 10),
    ] {
        let s = &parsed[name];
        assert_eq!(
            (s.fps, s.period_ms(), s.frames.len()),
            (fps, period, frames),
            "{name}"
        );
    }
    let noise: Vec<&str> = parsed["ui/ui_noise.seq"]
        .frames
        .iter()
        .map(|f| f.trim_start_matches("ui\\ui_noise_"))
        .collect();
    assert_eq!(
        noise,
        [
            "00", "01", "02", "03", "04", "03", "01", "02", "00", "03", "04"
        ]
    );
    assert!(parsed["fx/fx_stalker.seq"].frames[0].ends_with(".tga"));
    assert_eq!(
        ui::still_image(&g.vfs, &g.aliases, "fx\\fx_wood_fire_1.tga").unwrap(),
        "gamedata\\textures\\fx\\fx_wood_fire_1.dds"
    );
    // Title-menu cursor (title-menu.md section 7).
    let TextureSource::Sequence(_, cursor) =
        ui::texture_source(&g.vfs, &g.aliases, "ui\\ui_ani_cursor").unwrap()
    else {
        panic!("cursor is not a sequence");
    };
    for (t, frame) in [(0, 1), (51, 1), (52, 2), (415, 8), (416, 1), (1000, 4)] {
        assert_eq!(
            cursor.frame(t),
            format!("ui\\ui_ani_cursor_0{frame}"),
            "t = {t}"
        );
    }
    let frame = ui::still_image(&g.vfs, &g.aliases, cursor.frame(0)).unwrap();
    assert_eq!(dds_size(&g.vfs, &frame), (64, 64));
}

/// Loads the string table, timing it.
fn strings(g: &Game) -> (Localization, StringTable) {
    let localization = text::load_localization(&g.vfs, &g.aliases).unwrap();
    let start = Instant::now();
    let table = text::load_string_table(&g.vfs, &g.aliases, &localization).unwrap();
    eprintln!("loaded the string table in {:.1?}", start.elapsed());
    (localization, table)
}

#[test]
fn loads_russian_string_tables() {
    let Some(g) = game() else { return };
    let (localization, table) = strings(&g);
    assert_eq!(localization.language, "rus");
    assert_eq!(localization.files.len(), 41);
    assert_eq!(localization.files[0], "ui_st_pda");
    assert_eq!(localization.files[40], "stable_game_credits");
    assert_eq!(localization.font_prefix, None);
    assert_eq!(table.len(), 7_639);

    // Section 1.7 and the title menu's captions (title-menu.md sections 4.1
    // and 4.5).
    for (key, text) in [
        ("ui_mm_newgame", "Новая игра"),
        ("ui_mm_load_game", "Загрузить игру"),
        ("ui_mm_last_save", "Последн. сохран."),
        ("ui_mm_multiplayer_n", "Сетевая игра"),
        ("ui_mm_options_n", "Опции"),
        ("ui_mm_credits", "Титры"),
        ("ui_mm_quit2windows", "Выйти в Windows"),
        ("ui_mm_novice", "Новичок"),
        ("ui_mm_stalker", "Сталкер"),
        ("ui_mm_veteran", "Ветеран"),
        ("ui_mm_master", "Мастер"),
        ("ui_mm_back", "Назад"),
        ("ui_mm_return_game", "Возврат к игре"),
        ("ui_mm_save_game", "Сохранить игру"),
        ("ui_mm_quit_game", "Завершить игру"),
        ("ui_mm_disconnect", "Отсоединиться"),
        (
            "ui_mm_quit_windows_message",
            "Действительно ли вы хотите выйти в Windows?",
        ),
        ("ui_st_btn_yes", "Да"),
        ("ui_st_btn_no", "Нет"),
        ("gd_stalker", "опытный"),
        ("st_zaitsev", "Евгений"),
    ] {
        assert_eq!(table.get(key), text, "{key}");
    }
    assert_eq!(table.get("no_such_key"), "no_such_key");
    assert_eq!(table.get(""), "");
    assert!(table.contains("ui_st_load_game_confirm"));
    assert!(table.contains("ui_mm_quit_game_message"));
    assert!(table.contains("ui_mm_disconnect_message"));

    // Section 1.6: sequences inside strings.
    assert_eq!(table.templated_keys().count(), 32);
    for key in table.templated_keys() {
        assert!(table.get(key).contains("$$ACTION_"), "{key}");
    }
    // Every entry of every table, in load order.
    let mut entries = Vec::new();
    for f in &localization.files {
        let path = g
            .aliases
            .path("$game_config$", &localization.table_path(f))
            .unwrap();
        let text = xml::preprocess(&read(&g.vfs, &path).unwrap(), &mut |inc| {
            read(
                &g.vfs,
                &g.aliases
                    .path("$game_config$", &cp1251::decode(inc))
                    .unwrap(),
            )
        })
        .unwrap();
        entries.extend(
            volya_engine_formats::text::string_table::entries(&xml::parse(&text).unwrap()).unwrap(),
        );
    }
    assert_eq!(entries.len(), 7_639, "no repeated keys");
    let includes = read(&g.vfs, "gamedata/config/text/rus/string_table_includes.xml").unwrap();
    let include_lines = includes
        .split(|&b| b == b'\r' || b == b'\n')
        .filter(|l| l.starts_with(b"#include"))
        .count();
    assert_eq!(include_lines, 47);
    let count = |pred: &dyn Fn(&str) -> bool| entries.iter().filter(|(_, t)| pred(t)).count();
    assert_eq!(count(&|t| t.contains("\\n")), 96);
    assert_eq!(count(&|t| t.contains("%c[")), 0);
    assert_eq!(count(&|t| t.is_empty()), 0);
    // Section 1.5: only the stated non-ASCII bytes besides Cyrillic.
    for (key, text) in &entries {
        for b in cp1251::encode(text).unwrap() {
            assert!(
                !(0x80..0xC0).contains(&b)
                    || [
                        0x85, 0x93, 0x94, 0x96, 0xA0, 0xA8, 0xAB, 0xAD, 0xAE, 0xB8, 0xBB
                    ]
                    .contains(&b),
                "{key}: unexpected byte {b:#04x}"
            );
        }
    }
}

/// The font definition sections of `fonts.ltx`.
const FONT_SECTIONS: [&str; 14] = [
    "stat_font",
    "hud_font_small",
    "hud_font_medium",
    "hud_font_di",
    "ui_font_arial_14",
    "ui_font_arial_21",
    "ui_font_graffiti19_russian",
    "ui_font_graffiti22_russian",
    "ui_font_graff_32",
    "ui_font_graff_40",
    "ui_font_graff_50",
    "ui_font_letterica16_russian",
    "ui_font_letterica18_russian",
    "ui_font_letter_25",
];

#[test]
fn loads_fonts_and_lays_out_title_menu_text() {
    let Some(g) = game() else { return };
    let localization = text::load_localization(&g.vfs, &g.aliases).unwrap();
    let fonts_ltx = config::load_aliased(&g.vfs, &g.aliases, "$game_config$", "fonts.ltx").unwrap();
    for s in FONT_SECTIONS {
        assert!(fonts_ltx.section(s).is_some(), "[{s}]");
    }

    // Section 2.6: every metric file of textures\ui.
    let mut inis = Vec::new();
    files(&g.vfs, "gamedata/textures/ui", ".ini", &mut inis);
    let inis: Vec<String> = inis
        .into_iter()
        .filter(|p| p.contains("/ui_font_"))
        .collect();
    assert_eq!(inis.len(), 101);
    let mut kinds = BTreeMap::<&str, Vec<String>>::new();
    for path in &inis {
        let c = config::load(&g.vfs, path).unwrap();
        let kind = [
            "symbol_coords",
            "mb_symbol_coords",
            "char widths",
            "font_size",
        ]
        .into_iter()
        .find(|s| c.section(s).is_some())
        .unwrap_or("none");
        let ok = GlyphMetrics::parse(&c).is_ok();
        kinds
            .entry(if ok { kind } else { "unparsed" })
            .or_default()
            .push(path.rsplit('/').next().unwrap().to_owned());
    }
    eprintln!(
        "metric files: {:?}",
        kinds.iter().map(|(k, v)| (k, v.len())).collect::<Vec<_>>()
    );
    eprintln!("unparsed metric files: {:?}", kinds["unparsed"]);
    assert_eq!(
        kinds["char widths"],
        ["ui_font_console_01.ini", "ui_font_hud_02.ini"]
    );
    assert_eq!(kinds["font_size"], ["ui_font_hud_01.ini"]);
    // 98 `[symbol_coords]` files, two of them without `height`.
    assert_eq!(kinds["symbol_coords"].len(), 96);
    assert_eq!(kinds["unparsed"].len(), 2);

    // Every UI font, at every variant, through the loader.
    let start = Instant::now();
    let by_height: Vec<(u32, Fonts)> = [600, 768, 1024]
        .into_iter()
        .map(|h| {
            (
                h,
                Fonts::load(&g.vfs, &g.aliases, &localization, h).unwrap(),
            )
        })
        .collect();
    eprintln!("loaded 3 × 11 fonts in {:.1?}", start.elapsed());
    let font = |h: u32, f: UiFont| by_height.iter().find(|(x, _)| *x == h).unwrap().1.get(f);
    for (h, _) in &by_height {
        for f in UiFont::ALL {
            let font = font(*h, f);
            let dds = format!("gamedata\\textures\\{}.dds", font.texture);
            dds_size(&g.vfs, &dds);
        }
    }
    for f in [UiFont::Medium, UiFont::Small, UiFont::Di] {
        let section = fonts_ltx.section(f.section()).unwrap();
        assert!(!section.contains("texture800") && !section.contains("texture1600"));
        assert_eq!(font(600, f).texture, font(1024, f).texture);
    }
    let small = font(768, UiFont::Small);
    assert_eq!(
        (small.texture.as_str(), small.interval),
        ("ui\\ui_font_hud_01", (0.75, 1.0))
    );
    assert_eq!(font(768, UiFont::Medium).texture, "ui\\ui_font_hud_02");
    assert!(font(768, UiFont::Di).gradient);

    // Section 2.3 examples and the variants table of section 2.7.
    for (h, suffix) in [
        (600, "800"),
        (768, "1024"),
        (720, "1024"),
        (1024, "1600"),
        (1080, "1600"),
    ] {
        let f = Fonts::load(&g.vfs, &g.aliases, &localization, h).unwrap();
        assert_eq!(
            f.get(UiFont::Graffiti22).texture,
            format!("ui\\ui_font_graff_22_{suffix}")
        );
        assert_eq!(
            f.get(UiFont::Graffiti19).texture,
            format!("ui\\ui_font_graff_19_{suffix}")
        );
    }
    for (f, name, sizes) in [
        (
            UiFont::Graffiti22,
            "graff_22",
            [(256, 256, 21.0), (256, 256, 22.0), (512, 256, 32.0)],
        ),
        (
            UiFont::Graffiti19,
            "graff_19",
            [(256, 128, 17.0), (256, 256, 19.0), (512, 256, 30.0)],
        ),
        (
            UiFont::Letterica16,
            "letter_16",
            [(256, 128, 14.0), (256, 128, 16.0), (256, 256, 21.0)],
        ),
        (
            UiFont::Arial14,
            "arial_14",
            [(256, 64, 14.0), (512, 32, 13.0), (256, 64, 16.0)],
        ),
    ] {
        for ((h, variant), (tw, th, height)) in [(600, "800"), (768, "1024"), (1024, "1600")]
            .into_iter()
            .zip(sizes)
        {
            let font = font(h, f);
            assert_eq!(font.texture, format!("ui\\ui_font_{name}_{variant}"));
            assert_eq!(font.height, height, "{name} {variant}");
            let dds = format!("gamedata\\textures\\{}.dds", font.texture);
            assert_eq!(dds_size(&g.vfs, &dds), (tw, th), "{name} {variant}");
        }
    }
    // Section 2.7 glyphs.
    let glyph = |h, c: char| {
        let g = font(h, UiFont::Graffiti22).glyph(cp1251::encode_char(c).unwrap());
        (g.x, g.y, g.width)
    };
    assert_eq!(glyph(768, ' '), (0.0, 0.0, 10.0));
    assert_eq!(glyph(768, '.'), (111.0, 0.0, 5.0));
    assert_eq!(glyph(768, '1'), (134.0, 0.0, 7.0));
    assert_eq!(glyph(768, 'Н'), (12.0, 110.0, 10.0));
    assert_eq!(glyph(768, 'а'), (217.0, 110.0, 8.0));
    assert_eq!(glyph(768, 'я'), (246.0, 132.0, 9.0));
    assert_eq!(glyph(600, ' ').2, 9.0);
    assert_eq!(glyph(600, 'Н'), (56.0, 105.0, 9.0));
    assert_eq!(glyph(600, 'а').2, 8.0);
    assert_eq!(glyph(1024, ' ').2, 14.0);
    assert_eq!(glyph(1024, 'Н'), (421.0, 96.0, 14.0));
    assert_eq!(glyph(1024, 'а').2, 12.0);
    for code in 0..32 {
        assert_eq!(font(768, UiFont::Graffiti22).glyph(code).width, 0.0);
    }

    // Section 5: the title menu's captions and version.
    let (_, table) = strings(&g);
    let colors = ui::load_color_defs(&g.vfs, &g.aliases, XGA).unwrap();
    let menu = ui::load_title_menu(&g.vfs, &g.aliases, &colors, XGA).unwrap();
    let main = menu.menu("menu_main").unwrap();
    assert_eq!(main.font, Some(UiFont::Graffiti22));
    assert_eq!(main.color, Color::rgba(0, 0, 0, 255));
    let version = &menu.static_version;
    let version_text = cp1251::encode("ver. 1.0006").unwrap();
    // (resolution, [item 0, item 5, version] as (left, right, top, bottom)).
    type Boxes = [(f32, f32, f32, f32); 3];
    let expected: [(Resolution, &str, Boxes); 4] = [
        (
            XGA,
            "1024",
            [
                (740.0, 825.0, 423.0, 445.0),
                (713.0, 851.0, 573.0, 595.0),
                (15.0, 94.0, 740.0, 759.0),
            ],
        ),
        (
            SVGA,
            "800",
            [
                (570.0, 651.0, 328.0, 349.0),
                (546.0, 675.0, 445.0, 466.0),
                (11.0, 77.0, 578.0, 595.0),
            ],
        ),
        (
            Resolution::new(1280, 720),
            "1024",
            [
                (935.0, 1020.0, 395.0, 417.0),
                (908.0, 1046.0, 536.0, 558.0),
                (18.0, 97.0, 693.0, 712.0),
            ],
        ),
        (
            Resolution::new(1280, 1024),
            "1600",
            [
                (916.0, 1038.0, 562.0, 594.0),
                (880.0, 1075.0, 762.0, 794.0),
                (18.0, 133.0, 986.0, 1016.0),
            ],
        ),
    ];
    for (resolution, variant, boxes) in expected {
        let fonts = Fonts::load(&g.vfs, &g.aliases, &localization, resolution.height).unwrap();
        let scale = Surface::screen(resolution);
        let extent = |l: &volya_engine_formats::text::layout::TextLayout| {
            let q = &l.quads;
            (
                q.iter().map(|q| q.min.0).fold(f32::MAX, f32::min),
                q.iter().map(|q| q.max.0).fold(f32::MIN, f32::max),
                q.iter().map(|q| q.min.1).fold(f32::MAX, f32::min),
                q.iter().map(|q| q.max.1).fold(f32::MIN, f32::max),
            )
        };
        let graffiti22 = fonts.get(main.font.unwrap());
        // The menu code centres captions both ways in 164 × 30 rows
        // (ui-layout.md section 7.3, text-and-fonts.md section 5).
        let boxes_of_menu = menu.caption_boxes(main);
        assert_eq!(boxes_of_menu.len(), 6);
        for (i, b) in boxes_of_menu.iter().enumerate() {
            assert_eq!(
                b.rect,
                Rect::new(700.0, 419.0 + 30.0 * i as f32, 164.0, 30.0)
            );
            assert_eq!(
                (b.align, b.vert_align, b.complex),
                (HAlign::Centre, VAlign::Centre, false)
            );
            assert_eq!(b.color, Color::rgba(0, 0, 0, 255));
        }
        for (item, want) in [(0, boxes[0]), (5, boxes[1])] {
            let caption = table.get(&main.buttons[item].caption);
            let l = layout_text(
                graffiti22,
                &cp1251::encode(caption).unwrap(),
                &boxes_of_menu[item],
                scale,
                &colors,
            )
            .unwrap();
            assert_eq!(l.texture, format!("ui\\ui_font_graff_22_{variant}"));
            assert_eq!(extent(&l), want, "item {item} at {resolution:?}");
        }
        let (b, f) = TextBox::of_static(version, version.window.absolute(0.0, 0.0));
        assert_eq!(f, UiFont::Graffiti19);
        assert_eq!(
            (b.align, b.vert_align, b.complex),
            (HAlign::Left, VAlign::Top, false)
        );
        assert_eq!(b.color, Color::rgba(135, 123, 116, 255));
        let l = layout_text(
            fonts.get(UiFont::Graffiti19),
            &version_text,
            &b,
            scale,
            &colors,
        )
        .unwrap();
        assert_eq!(extent(&l), boxes[2], "version at {resolution:?}");
        if resolution == XGA {
            // Section 3.3 example: `Н` of ui_font_graff_22_1024.
            let l = layout_text(
                graffiti22,
                &cp1251::encode("Н").unwrap(),
                &TextBox {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    offset: (0.0, 0.0),
                    align: HAlign::Left,
                    vert_align: VAlign::Top,
                    complex: false,
                    color: Color::WHITE,
                    line_gap: 0.0,
                },
                scale,
                &colors,
            )
            .unwrap();
            let (uv0, uv1) = l.quads[0].uv(256.0, 256.0);
            let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
            assert!(close(uv0.0, 0.048_828) && close(uv1.0, 0.087_891));
            assert!(close(uv0.1, 0.431_641) && close(uv1.1, 0.517_578));
        }
    }

    // Section 5 with r__supersample 2 and 4: the main menu laid out on the
    // off-screen target (section 4.4), boxes in screen pixels to three
    // decimals; (screen, surface, [item 0, item 5, version]).
    type Fractional = [[f32; 4]; 3];
    let on_target: [(Resolution, (u32, u32), Fractional); 8] = [
        (
            HD,
            (2048, 1526),
            [
                [1405.25, 1527.25, 598.742, 630.742],
                [1369.25, 1564.25, 809.646, 841.646],
                [28.125, 143.125, 1040.367, 1070.367],
            ],
        ),
        (
            HD,
            (2048, 2048),
            [
                [1405.25, 1527.25, 601.699, 633.699],
                [1369.25, 1564.25, 812.637, 844.637],
                [28.125, 143.125, 1040.449, 1070.449],
            ],
        ),
        (
            Resolution::new(1280, 1024),
            (1810, 1448),
            [
                [916.326, 1038.326, 567.16, 599.16],
                [880.326, 1075.326, 767.293, 799.293],
                [18.387, 133.387, 986.519, 1016.519],
            ],
        ),
        (
            Resolution::new(1280, 1024),
            (2048, 2048),
            [
                [916.5, 1038.5, 570.5, 602.5],
                [880.5, 1075.5, 770.5, 802.5],
                [18.75, 133.75, 986.5, 1016.5],
            ],
        ),
        (
            XGA,
            (1448, 1086),
            [
                [739.436, 824.436, 425.724, 447.724],
                [712.436, 850.436, 575.646, 597.646],
                [14.851, 93.851, 739.713, 758.713],
            ],
        ),
        (
            XGA,
            (2048, 1536),
            [
                [740.0, 825.0, 428.5, 450.5],
                [713.0, 851.0, 578.5, 600.5],
                [15.0, 94.0, 740.0, 759.0],
            ],
        ),
        (
            SVGA,
            (1130, 848),
            [
                [570.265, 651.265, 331.132, 352.132],
                [546.265, 675.265, 448.585, 469.585],
                [11.327, 77.327, 578.066, 595.066],
            ],
        ),
        (
            SVGA,
            (1600, 1200),
            [
                [570.5, 651.5, 333.5, 354.5],
                [546.5, 675.5, 451.0, 472.0],
                [11.5, 77.5, 578.0, 595.0],
            ],
        ),
    ];
    let boxes_of_menu = menu.caption_boxes(main);
    let (version_box, _) = TextBox::of_static(version, version.window.absolute(0.0, 0.0));
    for (screen, (w, h), want) in on_target {
        let fonts = Fonts::load(&g.vfs, &g.aliases, &localization, screen.height).unwrap();
        let surface = Surface {
            size: Resolution::new(w, h),
            screen,
        };
        let caption = |i: usize| cp1251::encode(table.get(&main.buttons[i].caption)).unwrap();
        let cases = [
            (UiFont::Graffiti22, caption(0), boxes_of_menu[0]),
            (UiFont::Graffiti22, caption(5), boxes_of_menu[5]),
            (UiFont::Graffiti19, version_text.clone(), version_box),
        ];
        for ((font, text, b), want) in cases.iter().zip(want) {
            let l = layout_text(fonts.get(*font), text, b, surface, &colors).unwrap();
            let q = &l.quads;
            let got = [
                q.iter().map(|q| q.min.0).fold(f32::MAX, f32::min),
                q.iter().map(|q| q.max.0).fold(f32::MIN, f32::max),
                q.iter().map(|q| q.min.1).fold(f32::MAX, f32::min),
                q.iter().map(|q| q.max.1).fold(f32::MIN, f32::max),
            ];
            assert!(
                got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1.5e-3),
                "{screen:?} on {w} × {h}: {got:?}, want {want:?}"
            );
        }
    }
}

#[test]
fn reads_title_menu_layout() {
    let Some(g) = game() else { return };
    let colors = ui::load_color_defs(&g.vfs, &g.aliases, XGA).unwrap();
    let system = config::load_aliased(&g.vfs, &g.aliases, "$game_config$", "system.ltx").unwrap();
    let descriptions = ui::load_texture_descriptions(&g.vfs, &g.aliases, &system, XGA).unwrap();
    for r in [XGA, HD] {
        assert_eq!(
            ui::find_ui_file(&g.vfs, &g.aliases, "ui_mm_main.xml", r)
                .unwrap()
                .unwrap(),
            "gamedata\\config\\ui\\ui_mm_main.xml"
        );
    }
    let menu: TitleMenuLayout = ui::load_title_menu(&g.vfs, &g.aliases, &colors, HD).unwrap();

    // Title-menu section 3: layers and their pictures.
    let picture = |s: &volya_engine_formats::ui::layout::StaticDesc| {
        Picture::resolve(s.texture.as_ref().unwrap(), &descriptions)
    };
    let rect = |x, y, w, h| Rect::new(x, y, w, h);
    assert_eq!(
        menu.back_movie.window.rect(),
        rect(148.0, 0.0, 876.0, 400.0)
    );
    assert_eq!(
        picture(&menu.back_movie),
        Picture {
            texture: "ui\\ui_mm_main_back_crop".into(),
            rect: Some(rect(0.0, 0.0, 876.0, 400.0))
        }
    );
    assert_eq!(menu.background.window.rect(), rect(0.0, 0.0, 1024.0, 768.0));
    assert_eq!(
        picture(&menu.background),
        Picture {
            texture: "ui\\ui_mainMenu".into(),
            rect: Some(rect(0.0, 0.0, 1024.0, 768.0))
        }
    );
    assert_eq!(
        menu.fire_movie.window.rect(),
        rect(525.0, 459.0, 128.0, 256.0)
    );
    assert_eq!(
        picture(&menu.fire_movie),
        Picture {
            texture: "ui\\ui_mm_fire".into(),
            rect: None
        }
    );
    for (s, path) in [
        (&menu.back_movie, "ui_mm_main_back_crop"),
        (&menu.fire_movie, "ui_mm_fire"),
    ] {
        assert_eq!(
            ui::texture_source(&g.vfs, &g.aliases, &picture(s).texture).unwrap(),
            TextureSource::Video(format!("gamedata\\textures\\ui\\{path}.ogm"))
        );
    }
    assert_eq!(menu.shniaga_wnd.rect(), rect(590.0, 419.0, 392.0, 280.0));
    assert_eq!(
        menu.static_version.window.rect(),
        rect(15.0, 740.0, 100.0, 30.0)
    );
    let version = menu.static_version.text.as_ref().unwrap();
    assert_eq!(version.font, Some(UiFont::Graffiti19));
    assert_eq!(version.align, Some(HAlign::Left));
    assert_eq!(version.vert_align, None);
    assert_eq!(version.complex_mode, Some(false));
    assert_eq!(version.key, None);

    // Section 4: the list region, the bar and its children.
    let region = &menu.buttons_region;
    assert_eq!(region.window.rect(), rect(110.0, 0.0, 164.0, 280.0));
    assert!(!region.always_show_scroll);
    assert_eq!(
        (
            region.right_ident,
            region.top_indent,
            region.bottom_indent,
            region.vert_interval
        ),
        (0.0, 0.0, 0.0, 0.0)
    );
    let (rows, content) = region.layout(&[30.0; 7]);
    assert_eq!(rows[6], (0.0, 180.0));
    assert!(!region.shows_scroll_bar(content));
    assert_eq!(menu.shniaga.window.rect(), rect(9.0, 0.0, 392.0, 73.0));
    assert_eq!(
        picture(&menu.shniaga).rect,
        Some(rect(7.0, 951.0, 392.0, 73.0))
    );
    assert_eq!(menu.magnifire.window.rect(), rect(76.0, 22.0, 232.0, 36.0));
    assert!(menu.magnifire.stretch);
    assert_eq!(picture(&menu.magnifire).texture, "ui\\ui_magnifier");
    assert_eq!(menu.y_offset, 0.0);
    for (s, id, centre) in [
        (&menu.left_anim, "ui_menu_bigroll_l", (43.0, 37.0)),
        (&menu.right_anim, "ui_menu_bigroll_r", (340.0, 37.0)),
    ] {
        assert!(s.stretch && s.heading && s.window.centred);
        assert_eq!((s.window.x, s.window.y), centre);
        assert_eq!(s.window.rect().width, 55.0);
        assert_eq!(s.texture.as_ref().unwrap().name, id);
    }
    for (s, id, x) in [
        (&menu.left_grating, "ui_menu_grating_l", 0.0),
        (&menu.right_grating, "ui_menu_grating_r", 337.0),
    ] {
        assert!(!s.stretch);
        assert_eq!(s.window.rect(), rect(x, 4.0, 55.0, 67.0));
        assert_eq!(s.texture.as_ref().unwrap().name, id);
    }
    // Absolute positions (ui-layout section 4.1) and the picture vectors of
    // sections 5.3 and 5.4 from the real data.
    let wnd = menu.shniaga_wnd.absolute(0.0, 0.0);
    let bar = menu.shniaga.window.absolute(wnd.x, wnd.y);
    assert_eq!((bar.x, bar.y), (599.0, 419.0));
    let grating = menu.left_grating.window.absolute(bar.x, bar.y);
    assert_eq!((grating.x, grating.y), (599.0, 423.0));
    let tex = (1024, 1024);
    let p = PicturePlacement::new(
        &menu.left_grating,
        grating,
        &picture(&menu.left_grating),
        tex,
    );
    assert_eq!(p.size, (46.0, 70.0));
    let anim = menu.left_anim.window.absolute(bar.x, bar.y);
    let p = PicturePlacement::new(&menu.left_anim, anim, &picture(&menu.left_anim), tex);
    let r = p.rotated(HD.scale(), FRAC_PI_2, None, HD.is_wide());
    assert!((r.corners[0].0 - 1160.783).abs() < 2e-3 && (r.corners[0].1 - 679.922).abs() < 2e-3);
    let p = PicturePlacement::new(
        &menu.background,
        menu.background.window.absolute(0.0, 0.0),
        &picture(&menu.background),
        tex,
    );
    let q = p.quad(XGA.scale());
    assert_eq!(
        (q.min, q.max, q.uv_max),
        ((-0.5, -0.5), (1023.5, 767.5), (1.0, 0.75))
    );

    // Section 4.1: the lists.
    let captions = |name: &str| -> Vec<(String, String)> {
        let m = menu.menu(name).unwrap();
        assert_eq!(m.btn_height, 30.0, "{name}");
        assert_eq!(m.font, Some(UiFont::Graffiti22), "{name}");
        assert_eq!(m.color, Color::rgba(0, 0, 0, 255), "{name}");
        m.buttons
            .iter()
            .map(|b| (b.name.clone(), b.caption.clone()))
            .collect()
    };
    let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
        p.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    let main = [
        ("btn_new_game", "ui_mm_newgame"),
        ("btn_load", "ui_mm_load_game"),
        ("btn_multiplayer", "ui_mm_multiplayer_n"),
        ("btn_options", "ui_mm_options_n"),
        ("btn_credits", "ui_mm_credits"),
        ("btn_quit", "ui_mm_quit2windows"),
    ];
    assert_eq!(captions("menu_main"), pairs(&main));
    let mut last_save = main.to_vec();
    last_save.insert(2, ("btn_lastsave", "ui_mm_last_save"));
    assert_eq!(captions("menu_main_last_save"), pairs(&last_save));
    assert_eq!(
        captions("menu_new_game"),
        pairs(&[
            ("btn_novice", "ui_mm_novice"),
            ("btn_stalker", "ui_mm_stalker"),
            ("btn_veteran", "ui_mm_veteran"),
            ("btn_master", "ui_mm_master"),
            ("btn_new_back", "ui_mm_back"),
        ])
    );
    assert_eq!(captions("menu_main_single").len(), 8);
    assert_eq!(captions("menu_main_single_dead").len(), 6);
    let mm = captions("menu_main_mm");
    assert_eq!(mm[4], ("btn_quit_to_mm".into(), "ui_mm_disconnect".into()));

    // Section 6: music only; wheel sounds are commented out.
    assert!(!menu.menu_sound.random);
    assert_eq!(menu.menu_sound.music, ["music\\wasteland2"]);
    assert_eq!(menu.menu_sound.whell_sound, None);
    assert_eq!(menu.menu_sound.whell_click, None);
}
