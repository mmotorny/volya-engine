// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's window description files through script API G3's
//! names, checked against
//! docs/behavior/script-api-window-description-files-fonts-and-textures.md
//! (cited by section), on a game state started with the shipped scripts:
//! 5.2's texture example on the main menu's registry, the title menu's item
//! list read from `ui_mm_main.xml` (3.2), `ui_game_over.script`'s loop
//! stopping at `item_3` (3.4) and the string table built at the first
//! lookup (6). Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::rc::Rc;

use shipped_scripts::{FSGAME, GameTrees};
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::ui;
use volya_engine_core::vfs::StdVfs;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_formats::ui::Resolution;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::loader::Prefix;
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::Story;
use volya_engine_script::window_files::{WindowFiles, WindowKind};
use volya_engine_script::{ErrorKind, Value};

const XGA: Resolution = Resolution::new(1024, 768);

/// A game state started with the shipped scripts, the registry and colours
/// loaded as the main menu loads them, the fonts created, no string table.
fn started() -> Option<(GameScripts, Rc<RefCell<EngineServices>>)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let tree = &trees.tree;
    let system = config::load_aliased(tree, &aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(tree, &aliases, "$game_config$", "game.ltx").unwrap();
    let window_files = WindowFiles {
        resolution: XGA,
        colors: ui::load_color_defs(tree, &aliases, XGA).unwrap(),
        textures: ui::load_texture_descriptions(tree, &aliases, &system, XGA).unwrap(),
        strings: None,
        fonts_created: true,
        windows: Vec::new(),
    };
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let files = Rc::new(RefCell::new(FileSystem::new(
        trees.clone(),
        aliases.clone(),
        Rc::new(system),
        Rc::new(game),
    )));
    let scripts = GameScripts::start(Startup {
        trees: trees.clone(),
        aliases,
        prefix: Prefix::Default,
        services: Rc::clone(&services),
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::default(),
        window_files: Rc::new(RefCell::new(window_files)),
        level: Rc::default(),
        effects_and_weather: Rc::default(),
    })
    .unwrap();
    Some((scripts, services))
}

fn number(v: &Value) -> f64 {
    v.as_number().unwrap_or(f64::NAN)
}

/// 5.2's worked example, 3.2's title menu item list, 6's first lookup.
#[test]
fn the_names_on_the_shipped_files() {
    let Some((mut scripts, _)) = started() else {
        return;
    };
    let results = scripts
        .scripts()
        .exec(
            "=check",
            br#"
local info = GetTextureInfo("ui_iconsTotal_found_thing", "ui_iconsTotal_found_thing")
local r = info:get_rect()
local xml = CScriptXmlInit()
xml:ParseFile("ui_mm_main.xml")
local menu = xml:InitMMShniaga("shniaga_wnd", nil)
return info:get_file_name(), r.x1, r.y1, r.x2, r.y2, menu ~= nil,
  type(game.translate_string("st_tip")), game.translate_string("")
"#,
        )
        .unwrap();
    assert_eq!(results[0], Value::from("ui\\ui_iconsTotal"));
    let rect: Vec<f64> = results[1..5].iter().map(number).collect();
    assert_eq!(rect, [0.0, 94.0, 83.0, 141.0]);
    assert_eq!(results[5], Value::Boolean(true));
    assert_eq!(results[6], Value::from("string"));
    assert_eq!(results[7], Value::from(""));
    let files = scripts.window_files().borrow();
    let menu = files.windows.last().unwrap();
    assert_eq!(menu.kind, WindowKind::TitleMenuItemList);
    assert!(
        files.strings.is_some(),
        "the table is built at the first lookup"
    );
}

/// 3.4: `ui_game_over.script`'s loop stops the game at `item_3`, the three
/// statics made before attached to nothing.
#[test]
fn the_game_over_loop() {
    let Some((mut scripts, services)) = started() else {
        return;
    };
    let result = scripts.scripts().exec(
        "=check",
        br#"
local xml = CScriptXmlInit()
xml:ParseFile("ui_game_over.xml")
for n = 0, 30 do
  if xml:InitStatic("item_" .. n, nil) == nil then break end
end
"#,
    );
    assert!(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        "{result:?}"
    );
    let log = services.borrow().console.log().join("\n");
    assert!(
        log.contains("XML node not found") && log.contains("item_3"),
        "{log}"
    );
    let files = scripts.window_files().borrow();
    let statics: Vec<_> = (files.windows.iter())
        .filter(|w| w.parent.is_none())
        .collect();
    assert_eq!(statics.len(), 3);
    assert!(statics.iter().all(|w| w.kind == WindowKind::Static));
}
