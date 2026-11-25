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
/// files that may not be Lua at all. Each with the start of its error
/// after the path (cut before any quote of the script's own text).
const KNOWN_FAILURES: &[(&str, &str)] = &[
    // C-style comments.
    (
        "gamedata/scripts/.script",
        "270: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/_g.script",
        "99: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/arbug.script",
        "4: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/bind_stalker.script",
        "431: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/ce_switcher.script",
        "101: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/game_last_dialog.script",
        "14: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/game_last_sv.script",
        "375: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/h.script",
        "20: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/level_psy_antenna.script",
        "19: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/monster_test.script",
        "57: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/ph_test.script",
        "39: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/postprocess.script",
        "241: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/se_new_attachable_item.script",
        "9: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/se_switcher.script",
        "54: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/sr_psy_antenna.script",
        "8: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/state_mgr_animation.script",
        "63: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/test_invisible.script",
        "17: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/test_zombie.script",
        "16: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/ui_main_base_tab.script",
        "25: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/ui_mp_main.script",
        "411: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/ui_save_dialog.script",
        "107: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/x3.script",
        "7: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/xr_actions_id.script",
        "105: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/xr_evaluators_id.script",
        "94: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/xr_rest.script",
        "280: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/xr_sounds.script",
        "201: unexpected symbol near '/'",
    ),
    (
        "gamedata/scripts/zone_no_weapon.script",
        "8: unexpected symbol near '/'",
    ),
    // Not Lua, or a broken string.
    (
        "gamedata/config/misc/gulag_radar_u.script",
        "1: unexpected symbol near ';'",
    ),
    (
        "gamedata/scripts/copy of _test.script",
        "1: unexpected symbol",
    ),
    (
        "gamedata/scripts/gulag_selo.script",
        "13: unfinished string",
    ),
    (
        "gamedata/scripts/lua_help.script",
        "1: '=' expected near 'of'",
    ),
    (
        "gamedata/scripts/mob_alife_control.script",
        "44: '=' expected near 'self'",
    ),
    (
        "gamedata/scripts/test_ini.script",
        "13: '<name>' expected near '.'",
    ),
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
        let known = KNOWN_FAILURES.iter().find(|(p, _)| p == path);
        match (&result, known) {
            (Ok(()), None) => {}
            (Err(e), Some((_, error))) if e.message.starts_with(&format!("{path}:{error}")) => {}
            (Err(e), _) => unexpected.push(format!("{path}: {e}")),
            (Ok(()), Some(_)) => {
                unexpected.push(format!("{path}: compiles (remove it from KNOWN_FAILURES)"))
            }
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
        .filter(|(p, _)| !paths.iter().any(|q| q == p))
        .collect();
    assert!(missing.is_empty(), "known failures not found: {missing:?}");
}
