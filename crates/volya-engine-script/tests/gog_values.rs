// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's use of the value types and bit functions (BYOA),
//! checked against docs/behavior/script-api-value-types-and-math.md 10: the
//! 250 loadable scripts (docs/behavior/script-loading.md 3, recomputed
//! here), their calls of this spec's names, and the classes under the
//! loader's state. Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, VfsFile, read};
use volya_engine_script::Value;
use volya_engine_script::loader::{Prefix, Scripts, Trees};

/// The aliases scripts use (as in tests/gog.rs).
const FSGAME: &[u8] = b"$game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n";

struct GameTrees {
    tree: DbVfs<Box<dyn VfsFile + 'static>>,
    root: &'static StdVfs,
}

impl Trees for GameTrees {
    fn with_trees(&self, f: &mut dyn FnMut(&dyn Vfs, &dyn Vfs)) -> io::Result<()> {
        f(&self.tree, self.root);
        Ok(())
    }
}

/// Every file under `dir`, `/`-separated.
fn walk(vfs: &dyn Vfs, dir: &str, out: &mut Vec<String>) {
    for e in vfs.read_dir(dir).unwrap() {
        let path = if dir.is_empty() {
            e.name.clone()
        } else {
            format!("{dir}/{}", e.name)
        };
        match e.kind {
            EntryKind::Dir => walk(vfs, &path, out),
            EntryKind::File => out.push(path),
        }
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Name(String),
    String(Vec<u8>),
    Punct(u8),
    Other,
}

/// The end of a long bracket whose opening is at `i`, and its body's start.
fn long_bracket(text: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i + 1;
    while text.get(j) == Some(&b'=') {
        j += 1;
    }
    if text.get(i) != Some(&b'[') || text.get(j) != Some(&b'[') {
        return None;
    }
    let close = [b"]".as_slice(), &vec![b'='; j - i - 1], b"]"].concat();
    let body = j + 1;
    let end = text[body..]
        .windows(close.len())
        .position(|w| w == close)
        .map_or(text.len(), |p| body + p + close.len());
    Some((body, end))
}

/// Lua's tokens of a script after its `--` and C-style comments
/// (docs/behavior/script-loading.md 1), as tests/gog_classes.rs reads them.
fn tokens(text: &[u8]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let b = text[i];
        let next = text.get(i + 1).copied();
        if b.is_ascii_whitespace() {
            i += 1;
        } else if b == b'-' && next == Some(b'-') {
            match long_bracket(text, i + 2) {
                Some((_, end)) => i = end,
                None => {
                    while i < text.len() && text[i] != b'\n' && text[i] != b'\r' {
                        i += 1;
                    }
                }
            }
        } else if b == b'/' && next == Some(b'/') {
            while i < text.len() && text[i] != b'\n' {
                i += 1;
            }
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
        } else if b == b'"' || b == b'\'' {
            let mut s = Vec::new();
            i += 1;
            while i < text.len() && text[i] != b && text[i] != b'\n' {
                if text[i] == b'\\' && i + 1 < text.len() {
                    i += 1;
                }
                s.push(text[i]);
                i += 1;
            }
            i += 1;
            out.push(Token::String(s));
        } else if let Some((body, end)) = long_bracket(text, i) {
            out.push(Token::String(text[body..end.min(text.len())].to_vec()));
            i = end;
        } else if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < text.len() && (text[i].is_ascii_alphanumeric() || text[i] == b'_') {
                i += 1;
            }
            out.push(Token::Name(
                String::from_utf8_lossy(&text[start..i]).into_owned(),
            ));
        } else if b.is_ascii_digit() {
            while i < text.len() && (text[i].is_ascii_alphanumeric() || text[i] == b'.') {
                i += 1;
            }
            out.push(Token::Other);
        } else {
            out.push(Token::Punct(b));
            i += 1;
        }
    }
    out
}

/// The words of `bytes`: runs of letters, digits and `_` not starting with
/// a digit, lowercase (script-loading.md 3).
fn words(bytes: &[u8], out: &mut BTreeSet<String>) {
    for run in bytes.split(|b| !(b.is_ascii_alphanumeric() || *b == b'_')) {
        if run.first().is_some_and(|b| !b.is_ascii_digit()) {
            out.insert(String::from_utf8_lossy(run).to_ascii_lowercase());
        }
    }
}

/// A script's words after comment removal, string contents kept.
fn script_words(tokens: &[Token]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in tokens {
        match t {
            Token::Name(n) => {
                out.insert(n.to_ascii_lowercase());
            }
            Token::String(s) => words(s, &mut out),
            _ => {}
        }
    }
    out
}

/// What a script does with this spec's names (10): the names it calls (a
/// name followed by arguments, whatever the receiver), constructs (called
/// without a receiver), and the fields it reads after `.` or `:`.
#[derive(Default)]
struct Uses {
    called: BTreeSet<String>,
    constructed: BTreeSet<String>,
    read: BTreeSet<String>,
}

fn uses(tokens: &[Token]) -> Uses {
    let mut uses = Uses::default();
    for (i, t) in tokens.iter().enumerate() {
        let Token::Name(name) = t else {
            continue;
        };
        let after = |c: u8| i > 0 && tokens[i - 1] == Token::Punct(c);
        let receiver = after(b'.') || after(b':');
        let call = matches!(
            tokens.get(i + 1),
            Some(Token::Punct(b'(' | b'{') | Token::String(_))
        );
        if call {
            uses.called.insert(name.clone());
            if !receiver {
                uses.constructed.insert(name.clone());
            }
        } else if receiver {
            uses.read.insert(name.clone());
        }
    }
    uses
}

#[test]
fn shipped_value_type_uses() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    // The archive tree of `gamedata.db*` alone (script-loading.md, provenance).
    let base = DbVfs::open_game(root, None).unwrap();
    let mut files = Vec::new();
    walk(&base, "", &mut files);

    // script-loading.md 3: the roots.
    let lower: Vec<(String, &String)> = files.iter().map(|f| (f.to_ascii_lowercase(), f)).collect();
    let ext = |f: &str, exts: &[&str]| exts.iter().any(|e| f.ends_with(e));
    let config: Vec<_> = lower
        .iter()
        .filter(|(l, _)| l.starts_with("gamedata/config/") && ext(l, &[".ltx", ".xml"]))
        .collect();
    let others: Vec<_> = lower
        .iter()
        .filter(|(l, _)| {
            !l.starts_with("gamedata/config/")
                && !l.starts_with("gamedata/scripts/")
                && ext(l, &[".ltx", ".ini", ".spawn", ".game"])
        })
        .collect();
    assert_eq!((config.len(), others.len()), (893, 195), "root files");
    let mut roots = BTreeSet::from(["_g".to_owned()]);
    for (_, path) in config.iter().chain(&others) {
        words(&read(&base, path).unwrap(), &mut roots);
    }
    let binaries = gog_archives::game_files("binaries", |p| {
        let p = p.to_ascii_lowercase();
        p == "bin/xrgame.dll" || p == "bin/xr_3da.exe"
    })
    .unwrap();
    assert_eq!(binaries.len(), 2, "xrGame.dll and XR_3DA.exe");
    for binary in &binaries {
        words(&fs::read(&binary.file).unwrap(), &mut roots);
    }

    // The scripts and the closure.
    let mut scripts = BTreeMap::new();
    for (l, path) in &lower {
        if let Some(name) = l
            .strip_prefix("gamedata/scripts/")
            .and_then(|n| n.strip_suffix(".script"))
        {
            scripts.insert(name.to_owned(), tokens(&read(&base, path).unwrap()));
        }
    }
    assert_eq!(scripts.len(), 440, "files in scripts\\");
    let mut reached = BTreeSet::new();
    let mut pending: Vec<String> = scripts
        .keys()
        .filter(|n| roots.contains(*n))
        .cloned()
        .collect();
    assert_eq!(pending.len(), 132, "scripts reached from the roots");
    while let Some(name) = pending.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let found = script_words(&scripts[&name]);
        pending.extend(
            scripts
                .keys()
                .filter(|n| found.contains(*n) && !reached.contains(*n))
                .cloned(),
        );
    }
    assert_eq!(reached.len(), 250, "loadable scripts");

    // 10: the uses.
    let all: BTreeMap<&String, Uses> = scripts
        .iter()
        .filter(|(n, _)| n.as_str() != "lua_help")
        .map(|(n, t)| (n, uses(t)))
        .collect();
    let loadable = || all.iter().filter(|(n, _)| reached.contains(n.as_str()));
    let calling = |name: &str| loadable().filter(|(_, u)| u.called.contains(name)).count();
    let constructing = |name: &str| {
        loadable()
            .filter(|(_, u)| u.constructed.contains(name))
            .count()
    };
    for name in [
        "similar",
        "getHPB",
        "mk_xform",
        "set_length",
        "align",
        "getP",
        "setHP",
        "bit_xor",
        "bit_not",
    ] {
        assert_eq!(calling(name), 0, "loadable scripts calling {name}");
    }
    for name in ["lt", "rb"] {
        let reading = loadable().filter(|(_, u)| u.read.contains(name)).count();
        assert_eq!(reading, 0, "loadable scripts reading {name}");
    }
    for name in ["matrix", "flags16"] {
        assert_eq!(
            constructing(name),
            0,
            "loadable scripts constructing {name}"
        );
    }
    for name in ["bit_xor", "bit_not"] {
        let any = all.values().filter(|u| u.called.contains(name)).count();
        assert_eq!(any, 0, "scripts calling {name}");
    }
    assert_eq!(
        (
            constructing("flags32"),
            calling("bit_and"),
            calling("bit_or"),
            calling("getH"),
            calling("crossproduct"),
        ),
        (1, 2, 1, 4, 1),
        "loadable scripts constructing flags32, calling bit_and, bit_or, getH, crossproduct"
    );

    // The classes and functions under the loader's state, before `_G`.
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let mut s = Scripts::new(Rc::new(GameTrees { tree, root }), &aliases, Prefix::Default).unwrap();
    s.load_globals().unwrap();
    let got = s
        .exec(
            "=values",
            b"local v = vector():set(1, 0, -1) local f = flags32() f:assign(6) \
              return v:getH(), f:is(2), bit_and(6, 3), bit_or(4, 1)",
        )
        .unwrap();
    assert_eq!(
        got,
        [
            Value::Number(3.9269909858703613),
            true.into(),
            2.0.into(),
            5.0.into()
        ]
    );
}
