// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Every script of the Russian edition (BYOA) compiles under the runtime
//! (docs/behavior/script-environment.md). Nothing is run. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::Instant;

use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_script::Lua;

/// Scripts the public LuaJIT 1.0.3 rejects, until #104 establishes how the
/// original reads them: C-style comments (`//`, `/*`) where it stops, and
/// files that may not be Lua at all.
const KNOWN_FAILURES: &[&str] = &[
    // C-style comments.
    "gamedata/scripts/.script",
    "gamedata/scripts/_g.script",
    "gamedata/scripts/arbug.script",
    "gamedata/scripts/bind_stalker.script",
    "gamedata/scripts/ce_switcher.script",
    "gamedata/scripts/game_last_dialog.script",
    "gamedata/scripts/game_last_sv.script",
    "gamedata/scripts/h.script",
    "gamedata/scripts/level_psy_antenna.script",
    "gamedata/scripts/monster_test.script",
    "gamedata/scripts/ph_test.script",
    "gamedata/scripts/postprocess.script",
    "gamedata/scripts/se_new_attachable_item.script",
    "gamedata/scripts/se_switcher.script",
    "gamedata/scripts/sr_psy_antenna.script",
    "gamedata/scripts/state_mgr_animation.script",
    "gamedata/scripts/test_invisible.script",
    "gamedata/scripts/test_zombie.script",
    "gamedata/scripts/ui_main_base_tab.script",
    "gamedata/scripts/ui_mp_main.script",
    "gamedata/scripts/ui_save_dialog.script",
    "gamedata/scripts/x3.script",
    "gamedata/scripts/xr_actions_id.script",
    "gamedata/scripts/xr_evaluators_id.script",
    "gamedata/scripts/xr_rest.script",
    "gamedata/scripts/xr_sounds.script",
    "gamedata/scripts/zone_no_weapon.script",
    // Not Lua, or a broken string.
    "gamedata/config/misc/gulag_radar_u.script",
    "gamedata/scripts/copy of _test.script",
    "gamedata/scripts/gulag_selo.script",
    "gamedata/scripts/lua_help.script",
    "gamedata/scripts/mob_alife_control.script",
    "gamedata/scripts/test_ini.script",
];

/// Paths of the files under `dir` whose names end with `.script`.
fn scripts(vfs: &dyn Vfs, dir: &str, out: &mut Vec<String>) {
    for e in vfs.read_dir(dir).unwrap() {
        let path = if dir.is_empty() {
            e.name.clone()
        } else {
            format!("{dir}/{}", e.name)
        };
        match e.kind {
            EntryKind::Dir => scripts(vfs, &path, out),
            EntryKind::File if e.name.to_ascii_lowercase().ends_with(".script") => out.push(path),
            EntryKind::File => {}
        }
    }
}

#[test]
fn every_script_compiles() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let vfs = DbVfs::open_game(&root, Some("mods")).unwrap();
    let mut paths = Vec::new();
    scripts(&vfs, "", &mut paths);
    assert!(!paths.is_empty(), "no scripts");

    let mut lua = Lua::new().unwrap();
    let start = Instant::now();
    let mut bytes = 0;
    let mut failed = 0;
    let mut unexpected = Vec::new();
    for path in &paths {
        let code = read(&vfs, path).unwrap();
        bytes += code.len();
        let result = lua.compile(&format!("@{path}"), &code);
        failed += usize::from(result.is_err());
        if result.is_err() != KNOWN_FAILURES.contains(&path.as_str()) {
            unexpected.push(match result {
                Err(e) => format!("{path}: {e}"),
                Ok(()) => format!("{path}: compiles (remove it from KNOWN_FAILURES)"),
            });
        }
    }
    eprintln!(
        "compiled {} of {} scripts ({:.1} MB) in {:.1?}; {failed} known failures (#104)",
        paths.len() - failed,
        paths.len(),
        bytes as f64 / 1e6,
        start.elapsed(),
    );
    assert!(unexpected.is_empty(), "{}", unexpected.join("\n"));
    let missing: Vec<_> = KNOWN_FAILURES
        .iter()
        .filter(|p| !paths.iter().any(|q| q == *p))
        .collect();
    assert!(missing.is_empty(), "known failures not found: {missing:?}");
}
