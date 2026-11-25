// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's scripts (BYOA) under the runtime, checked against
//! docs/behavior/script-loading.md: every script the game can load compiles
//! as the game builds its chunk, and the runtime's C-style comments give the
//! tokens and line numbers of the spec's equivalent text transformation
//! (1.5). Nothing is run. Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::Instant;

use md5::{Digest, Md5};
use volya_engine_core::config::PathAliases;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_script::loader::{self, Common, Prefix};
use volya_engine_script::{Lua, Value};

/// The aliases scripts use, copied from the `fsgame.ltx` this edition's
/// installer generates at install time (`volya_engine::data::INSTALLER_FSGAME`;
/// not among the installer's files, so it can't be read from them).
const FSGAME: &[u8] = b"$game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n";

/// `*.script` files outside `$game_scripts$`: never loaded (2.1).
const OUTSIDE: &[&str] = &[
    "gamedata/config/misc/gulag_radar_u.script",
    "gamedata/config/scripts/qa.script",
];

/// Files in `$game_scripts$` whose names the game can't load (2.3, 4.1).
const UNLOADABLE: &[(&str, &str)] = &[
    // The empty name: no trigger reaches the loader with it.
    ("", "never loaded"),
    // Not Lua names: the prefix doesn't compile.
    ("copy of _test", "1: '=' expected near 'of'"),
    ("copy of a1", "1: '=' expected near 'of'"),
];

/// Loadable names whose chunks don't compile: none is reachable (4.1). Each
/// with the start of its error after the path (cut before any quote of the
/// script's own text).
const NOT_LUA: &[(&str, &str)] = &[
    // A string closed by a Windows-1251 right double quotation mark.
    ("gulag_selo", "13: unfinished string"),
    // A listing of exported classes in C++-like notation.
    ("lua_help", "1: '=' expected near 'of'"),
    // A statement that is an expression.
    ("mob_alife_control", "44: '=' expected near 'self'"),
    // C++ text pasted in.
    ("test_ini", "13: '<name>' expected near '.'"),
];

/// Files of 4.3: name, then MD5s of the transformed bytes (1.5) and of the
/// chunk (default prefix, then 1.5); line feeds before and after.
const HASHES: &[(&str, &str, &str, usize, usize)] = &[
    (
        "_G",
        "221c84fe1cb40e731d670ef7a667d65b",
        "221c84fe1cb40e731d670ef7a667d65b",
        1282,
        1282,
    ),
    (
        "bind_stalker",
        "97e424a7c82bf7bcc5dc2e5906b8f036",
        "335f362cd0ffd97eacb442851b29ed42",
        451,
        431,
    ),
    (
        "game_last_dialog",
        "e50c332f8074f3d05c8e2f82ddccbfd9",
        "f99bb3df2fab50b027342f418d95d693",
        228,
        197,
    ),
    (
        "h",
        "ca1d11225dfc73b2f474884e02739349",
        "3bfc1e226db2378d4cd8a0009d747f59",
        39,
        26,
    ),
    (
        "xr_rest",
        "688ead187275cf845762fd3547dd9b9b",
        "f74ae5557038b3527eb81f23168906dc",
        0,
        0,
    ),
    (
        "a1",
        "d3d9442930bdd0c4ee07ec6e0f547e47",
        "702bcf7a65bb75dde2913e5332a078ef",
        0,
        0,
    ),
];

fn md5_hex(data: &[u8]) -> String {
    Md5::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn is_newline(b: u8) -> bool {
    b == b'\n' || b == b'\r'
}

/// The length of a long bracket's opening `[==[` at `i`, and its level.
fn long_bracket(text: &[u8], i: usize, open: u8) -> Option<(usize, usize)> {
    let mut j = i + 1;
    while text.get(j) == Some(&b'=') {
        j += 1;
    }
    (text.get(i) == Some(&open) && text.get(j) == Some(&open)).then_some((j + 1 - i, j - i - 1))
}

/// The end of a long string or comment of `level` whose body starts at `i`.
fn long_end(text: &[u8], mut i: usize, level: usize) -> usize {
    while i < text.len() {
        if let Some((len, l)) = long_bracket(text, i, b']')
            && l == level
        {
            return i + len;
        }
        i += 1;
    }
    text.len()
}

/// The spec's equivalent text transformation (1.5), written here from its
/// prose to check the runtime's lexer against.
fn transform(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let start = i;
        let b = text[i];
        let next = text.get(i + 1).copied();
        if b == b'"' || b == b'\'' {
            i += 1;
            while i < text.len() && text[i] != b && !is_newline(text[i]) {
                if text[i] == b'\\' && i + 1 < text.len() {
                    i += 1;
                    if is_newline(text[i])
                        && text
                            .get(i + 1)
                            .is_some_and(|&c| is_newline(c) && c != text[i])
                    {
                        i += 1;
                    }
                }
                i += 1;
            }
            i = (i + 1).min(text.len());
        } else if let Some((len, level)) = long_bracket(text, i, b'[') {
            i = long_end(text, i + len, level);
        } else if b == b'-' && next == Some(b'-') {
            i += 2;
            match long_bracket(text, i, b'[') {
                Some((len, level)) => i = long_end(text, i + len, level),
                None => {
                    while i < text.len() && !is_newline(text[i]) {
                        i += 1;
                    }
                }
            }
        } else if b == b'/' && next == Some(b'/') {
            while i < text.len() && text[i] != b'\n' {
                i += 1;
            }
            continue;
        } else if b == b'/' && next == Some(b'*') {
            i += 2;
            while i < text.len() {
                let star = text[i] == b'*';
                i += 1;
                if star && i < text.len() {
                    i += 1;
                    if text[i - 1] == b'/' {
                        break;
                    }
                }
            }
            out.push(b' ');
            continue;
        } else {
            i += 1;
        }
        out.extend_from_slice(&text[start..i]);
    }
    out
}

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

/// Compiles `code` as `name`: its `string.dump`, or the error.
fn dump(lua: &mut Lua, name: &str, code: &[u8]) -> Result<Vec<u8>, String> {
    let results = lua
        .call("dump", &[Value::String(code.to_vec()), name.into()])
        .map_err(|e| e.message)?;
    match &results[..] {
        [Value::String(s)] => Ok(s.clone()),
        [Value::Nil, Value::String(e)] => Err(String::from_utf8_lossy(e).into_owned()),
        r => panic!("dump returned {r:?}"),
    }
}

#[test]
fn every_loadable_script_compiles() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let vfs = DbVfs::open_game(&root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let mut paths = Vec::new();
    scripts(&vfs, "", &mut paths);
    assert_eq!(paths.len(), 442, "*.script files");

    // Startup's lists (2.2.2), and the files they name.
    let common = Common::read(&vfs, &aliases).unwrap();
    assert_eq!(
        common.class_registrators,
        ["class_registrator.register", "game_registrator.register"]
    );
    assert_eq!(common.scripts, ["_G"]);
    for function in &common.class_registrators {
        let namespace = loader::function_namespace(function).unwrap();
        assert!(vfs.exists(&loader::script_path(&aliases, namespace).unwrap()));
    }

    let mut lua = Lua::new().unwrap();
    lua.exec(
        "=dump",
        b"function dump(code, name)
            local f, e = loadstring(code, name)
            if not f then return nil, e end
            return string.dump(f)
          end",
    )
    .unwrap();
    // Dumps hold line numbers, so equal dumps mean equal lines too.
    assert_ne!(
        dump(&mut lua, "=t", b"a = 1\nb = 2"),
        dump(&mut lua, "=t", b"a = 1 b = 2")
    );
    let start = Instant::now();
    let dir = "gamedata/scripts/";
    let (mut loadable, mut compiled) = (0, 0);
    let mut unexpected = Vec::new();
    let mut seen = Vec::new();
    for path in &paths {
        let Some(file_name) = path.strip_prefix(dir).filter(|f| !f.contains('/')) else {
            assert!(OUTSIDE.contains(&path.as_str()), "{path} outside {dir}");
            continue;
        };
        let name = file_name.strip_suffix(".script").unwrap();
        // The script name whose file this is (lookups are case-insensitive).
        let name = if name == "_g" { "_G" } else { name };
        assert_eq!(
            loader::script_path(&aliases, name)
                .unwrap()
                .replace('\\', "/"),
            *path
        );
        let file = read(&vfs, path).unwrap();
        let chunk_name = format!("@{}", loader::script_path(&aliases, name).unwrap());
        let shown = &chunk_name[1..];
        let unloadable = UNLOADABLE.iter().find(|(n, _)| *n == name);
        let not_lua = NOT_LUA.iter().find(|(n, _)| *n == name);
        let Some(chunk) = loader::chunk(name.as_bytes(), &file, Prefix::Default) else {
            assert_eq!(unloadable.map(|u| u.1), Some("never loaded"), "{path}");
            seen.push(name);
            continue;
        };
        loadable += usize::from(unloadable.is_none());
        let result = dump(&mut lua, &chunk_name, &chunk);
        let expected = unloadable.or(not_lua);
        match (&result, expected) {
            (Ok(_), None) => compiled += 1,
            (Err(e), Some((_, error))) if e.starts_with(&format!("{shown}:{error}")) => {
                seen.push(name)
            }
            (Err(e), _) => unexpected.push(e.clone()),
            (Ok(_), Some(_)) => unexpected.push(format!("{path}: compiles (unpin it)")),
        }
        // The `-_g` prefix changes nothing at compile time.
        let g = loader::chunk(name.as_bytes(), &file, Prefix::G).unwrap();
        let g_result = dump(&mut lua, &chunk_name, &g);
        assert_eq!(g_result.is_ok(), result.is_ok(), "{path} with -_g");

        // The runtime's comments are the transformation of 1.5: the same
        // tokens, line numbers and errors. The prefix holds no comment.
        let transformed = transform(&file);
        let t_chunk = loader::chunk(name.as_bytes(), &transformed, Prefix::Default).unwrap();
        let t_result = dump(&mut lua, &chunk_name, &t_chunk);
        if result != t_result {
            unexpected.push(format!("{path}: differs from the transformation of 1.5"));
        }
        if let Some(&(_, t_md5, chunk_md5, lf, t_lf)) = HASHES.iter().find(|h| h.0 == name) {
            assert_eq!(md5_hex(&transformed), t_md5, "{path} transformed");
            assert_eq!(md5_hex(&t_chunk), chunk_md5, "{path} chunk");
            let lines = |t: &[u8]| t.iter().filter(|&&b| b == b'\n').count();
            if lf > 0 {
                assert_eq!((lines(&file), lines(&transformed)), (lf, t_lf), "{path}");
            }
        }
    }
    eprintln!(
        "compiled {compiled} of {loadable} scripts with loadable names as the game builds them \
         in {:.1?}",
        start.elapsed()
    );
    assert!(unexpected.is_empty(), "{}", unexpected.join("\n"));
    let pinned: Vec<&str> = UNLOADABLE.iter().chain(NOT_LUA).map(|p| p.0).collect();
    let missing: Vec<_> = pinned.iter().filter(|p| !seen.contains(p)).collect();
    assert!(missing.is_empty(), "pinned files not found: {missing:?}");
    assert_eq!((loadable, compiled), (437, 433));
}
