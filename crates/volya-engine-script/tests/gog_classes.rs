// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's class declarations (BYOA) under the class system,
//! checked against
//! docs/behavior/script-api-script-classes-deriving-from-engine-classes.md
//! (1.2, 2.2, 3.2, 6.3): the scripts are loaded through the loader with
//! test engine classes named after the 33 engine bases they derive from
//! (none is the game's), every class they declare is constructed, and the
//! base-class calls they make find defaults. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, VfsFile, read};
use volya_engine_script::Value;
use volya_engine_script::classes::EngineClass;
use volya_engine_script::loader::{Prefix, Scripts, Trees};

/// The aliases scripts use (as in tests/gog.rs).
const FSGAME: &[u8] = b"$game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n";

/// The 33 engine bases of 1.3 and their constructors' parameter counts
/// (3.2; one count for a base the spec gives one for).
const ENGINE_BASES: &[(&str, &[usize])] = &[
    ("property_evaluator", &[2, 0]),
    ("action_base", &[2, 0]),
    ("CUIScriptWnd", &[0]),
    ("object_binder", &[1]),
    ("effector", &[2]),
    ("CUIWindow", &[0]),
    ("CUIListItemEx", &[0]),
    ("cse_alife_item", &[1]),
    ("cse_alife_smart_zone", &[1]),
    ("game_sv_mp_script", &[0]),
    ("CGameObject", &[0]),
    ("CSE_AbstractVisual", &[1]),
    ("UIGame_custom_script", &[0]),
    ("cse_alife_car", &[1]),
    ("cse_alife_helicopter", &[1]),
    ("cse_alife_human_stalker", &[1]),
    ("cse_alife_item_artefact", &[1]),
    ("cse_alife_item_custom_outfit", &[1]),
    ("cse_alife_item_torch", &[1]),
    ("cse_alife_item_weapon", &[1]),
    ("cse_alife_item_weapon_magazined", &[1]),
    ("cse_alife_item_weapon_magazined_w_gl", &[1]),
    ("cse_alife_item_weapon_shotgun", &[1]),
    ("cse_alife_monster_base", &[1]),
    ("cse_alife_object_physic", &[1]),
    ("cse_alife_space_restrictor", &[1]),
    ("cse_alife_trader", &[1]),
    ("cse_anomalous_zone", &[1]),
    ("cse_zone_visual", &[1]),
    ("game_PlayerState", &[0]),
    ("game_cl_mp_script", &[0]),
    ("action_planner", &[0]),
    ("game_sv_Deathmatch", &[0]),
];

/// The game's trees, kept for the test's lifetime.
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

#[derive(Debug, PartialEq)]
enum Token {
    Name(String),
    String(Vec<u8>),
    Punct(u8),
    Other,
}

/// The end of a long bracket `[==[` ... `]==]` whose opening is at `i`, or
/// `None` if there is none at `i`.
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

/// Lua's tokens of a script, after its `--` and C-style comments
/// (docs/behavior/script-loading.md 1); numbers and other symbols count
/// only as tokens.
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

/// What the scan of a script's tokens finds (1.2, 6.3).
#[derive(Default)]
struct Scan {
    /// Declared class names, with whether a base follows.
    declarations: Vec<(String, bool)>,
    /// `EngineBase.m(self` calls: (base, method).
    engine_calls: Vec<(String, String)>,
    /// `namespace.Class.m(self` calls: (namespace, class, method).
    script_calls: Vec<(String, String, String)>,
}

fn scan(tokens: &[Token]) -> Scan {
    let name = |i: usize| match tokens.get(i) {
        Some(Token::Name(n)) => Some(n.as_str()),
        _ => None,
    };
    let punct = |i: usize, c: u8| tokens.get(i) == Some(&Token::Punct(c));
    let mut scan = Scan::default();
    for i in 0..tokens.len() {
        if name(i) == Some("class")
            && let Some(Token::String(s)) = tokens.get(i + 1)
        {
            let declared = String::from_utf8_lossy(s).into_owned();
            scan.declarations.push((declared, punct(i + 2, b'(')));
        }
        let after_dot = i > 0 && (punct(i - 1, b'.') || punct(i - 1, b':'));
        if after_dot {
            continue;
        }
        if let (Some(a), Some(m)) = (name(i), name(i + 2))
            && punct(i + 1, b'.')
            && punct(i + 3, b'(')
            && name(i + 4) == Some("self")
            && ENGINE_BASES.iter().any(|(base, _)| *base == a)
        {
            scan.engine_calls.push((a.to_owned(), m.to_owned()));
        }
        if let (Some(ns), Some(c), Some(m)) = (name(i), name(i + 2), name(i + 4))
            && punct(i + 1, b'.')
            && punct(i + 3, b'.')
            && punct(i + 5, b'(')
            && name(i + 6) == Some("self")
        {
            scan.script_calls
                .push((ns.to_owned(), c.to_owned(), m.to_owned()));
        }
    }
    scan
}

/// What the test engine classes' constructors saw: (engine base, number
/// of arguments), in order.
type Constructed = Rc<RefCell<Vec<(&'static str, usize)>>>;

/// The defaults run: (engine base, method).
type DefaultsRun = Rc<RefCell<Vec<(&'static str, String)>>>;

/// A test engine class named after engine base `index`, with a constructor per count, and an
/// overridable method for each method the scripts' base calls name.
fn engine_class(
    index: usize,
    methods: &BTreeSet<String>,
    constructed: &Constructed,
    defaults_run: &DefaultsRun,
) -> EngineClass {
    let (base, counts) = ENGINE_BASES[index];
    // Which test base a class reaches: `Class.test_engine_base` (4.4).
    let mut class = EngineClass::new(base).constant("test_engine_base", index as i32 + 1);
    for &count in counts {
        let constructed = Rc::clone(constructed);
        class = class.constructor(
            count,
            |_| Some(0),
            move |_, args| {
                constructed.borrow_mut().push((base, args.len()));
                Ok(())
            },
        );
    }
    for method in methods {
        let defaults_run = Rc::clone(defaults_run);
        let m = method.clone();
        class = class.overridable(method, move |_: &mut (), _| {
            defaults_run.borrow_mut().push((base, m.clone()));
            Ok(Vec::new())
        });
    }
    class
}

/// Whether the game can load a script of this name: a Lua name
/// (docs/behavior/script-loading.md 2.3).
fn loadable_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}

#[test]
fn shipped_class_declarations() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();

    // The 439 scanned scripts: `scripts\*.script` but `lua_help`.
    let dir = "gamedata/scripts";
    let mut names = Vec::new();
    let mut scans = BTreeMap::new();
    for entry in tree.read_dir(dir).unwrap() {
        let Some(name) = entry.name.strip_suffix(".script") else {
            continue;
        };
        if name == "lua_help" {
            continue;
        }
        let text = read(&tree, &format!("{dir}/{}", entry.name)).unwrap();
        let name = if name == "_g" { "_G" } else { name };
        scans.insert(name.to_owned(), scan(&tokens(&text)));
        names.push(name.to_owned());
    }
    assert_eq!(names.len(), 439, "scanned scripts");

    // 1.2: the declarations.
    let declarations: Vec<_> = scans.values().flat_map(|s| s.declarations.iter()).collect();
    let distinct: BTreeSet<_> = declarations.iter().map(|(n, _)| n).collect();
    let declaring = scans
        .values()
        .filter(|s| !s.declarations.is_empty())
        .count();
    let with_base = declarations.iter().filter(|(_, b)| *b).count();
    assert_eq!(
        (declarations.len(), distinct.len(), declaring),
        (380, 355, 190),
        "declarations, names, scripts"
    );
    assert_eq!((with_base, declarations.len() - with_base), (286, 94));

    // 6.3: the base-class calls.
    let engine_calls: Vec<_> = scans.values().flat_map(|s| s.engine_calls.iter()).collect();
    let pairs: BTreeSet<_> = engine_calls.iter().collect();
    // On a script base: a class its namespace's script declares.
    let script_calls: Vec<_> = scans
        .values()
        .flat_map(|s| s.script_calls.iter())
        .filter(|(ns, c, _)| {
            scans
                .get(ns)
                .is_some_and(|s| s.declarations.iter().any(|(d, _)| d == c))
        })
        .collect();
    assert_eq!(
        (engine_calls.len(), pairs.len(), script_calls.len()),
        (380, 86, 22),
        "engine base calls, their (base, method) pairs, script base calls"
    );

    // The test engine classes, then every script with a Lua name.
    let constructed = Constructed::default();
    let defaults_run = DefaultsRun::default();
    let mut s = Scripts::new(Rc::new(GameTrees { tree, root }), &aliases, Prefix::Default).unwrap();
    for (index, &(base, _)) in ENGINE_BASES.iter().enumerate() {
        let methods = pairs
            .iter()
            .filter(|(b, _)| b == base)
            .map(|(_, m)| m.clone())
            .collect();
        s.register_class(engine_class(index, &methods, &constructed, &defaults_run))
            .unwrap();
    }
    s.load_globals().unwrap();
    for name in names.iter().filter(|n| loadable_name(n)) {
        s.load_namespace(name).unwrap();
    }
    // Scripts whose chunks failed: they stop before what follows the
    // failure (docs/behavior/script-loading.md 2.4).
    let failed: BTreeSet<String> = s.take_failures().into_iter().map(|f| f.name).collect();

    // 1.2: every declaration of a script that ran to its end is a class in
    // its namespace (in the globals for `_G`).
    let mut declared = BTreeSet::new();
    let mut missing = Vec::new();
    for (script, scan) in scans.iter().filter(|(n, _)| loadable_name(n)) {
        for (class, _) in &scan.declarations {
            let lookup = if script == "_G" {
                format!("rawget(_G, '{class}')")
            } else {
                format!("rawget(rawget(_G, '{script}') or {{}}, '{class}')")
            };
            let is_class = s
                .exec(
                    "=is_class",
                    format!("return getmetatable({lookup}) == getmetatable(property_evaluator)")
                        .as_bytes(),
                )
                .unwrap();
            if is_class == [true.into()] {
                declared.insert((script.clone(), class.clone(), lookup));
            } else {
                missing.push(format!("{script}.{class}"));
                assert!(
                    failed.contains(script),
                    "{script}.{class}: missing, and its script didn't fail"
                );
            }
        }
    }
    eprintln!(
        "{} classes declared, {} declarations in failed scripts",
        declared.len(),
        missing.len()
    );
    assert_eq!(declared.len(), 337, "classes the loaded scripts declare");

    // 2.2 and 3.2, for the classes of scripts that ran to their end: each
    // has its own `__init`; constructing one reaching an engine base calls
    // `super` first, with as many arguments as a constructor of the base
    // takes.
    let complete: Vec<_> = declared
        .iter()
        .filter(|(script, _, _)| !failed.contains(script))
        .collect();
    assert_eq!(
        complete.len(),
        327,
        "classes of scripts that ran to their end"
    );
    let mut engine_based = 0;
    for (script, class, lookup) in complete {
        let what = format!("{script}.{class}");
        let info = s
            .exec(
                "=info",
                format!("local c = {lookup} return type(c.__init), c.test_engine_base").as_bytes(),
            )
            .unwrap();
        assert_eq!(info[0], "function".into(), "{what}: own __init");
        let before = constructed.borrow().len();
        s.exec("=construct", format!("pcall({lookup})").as_bytes())
            .unwrap();
        let made: Vec<_> = constructed.borrow()[before..].to_vec();
        match info[1] {
            Value::Number(base) => {
                engine_based += 1;
                let (name, counts) = ENGINE_BASES[base as usize - 1];
                assert!(
                    matches!(&made[..], [(b, n)] if *b == name && counts.contains(n)),
                    "{what}: constructed {made:?} on {name}"
                );
            }
            _ => assert!(made.is_empty(), "{what}: constructed {made:?}"),
        }
    }
    eprintln!("{engine_based} of them reach an engine base");
    assert_eq!(engine_based, 251, "classes reaching an engine base");

    // 6.3: every engine base call names a default, run on an object of a
    // class deriving from the base; every script base call finds the
    // method among the script base's members.
    for (base, method) in &pairs {
        let (_, counts) = ENGINE_BASES.iter().find(|(b, _)| b == base).unwrap();
        let args = vec!["nil"; counts[0]].join(", ");
        let ran = s
            .exec(
                "=base_call",
                format!(
                    "class \"__base_call\" ({base}) \
                     function __base_call:__init() super({args}) end \
                     local o = __base_call() __base_call = nil \
                     return type({base}.{method}), pcall({base}.{method}, o)"
                )
                .as_bytes(),
            )
            .unwrap();
        assert_eq!(
            ran[..2],
            ["function".into(), true.into()],
            "{base}.{method}"
        );
    }
    assert_eq!(defaults_run.borrow().len(), pairs.len());
    for (ns, class, method) in &script_calls {
        let found = s
            .exec(
                "=script_call",
                format!("return type(rawget(rawget(_G, '{ns}'), '{class}').{method})").as_bytes(),
            )
            .unwrap();
        assert_eq!(found, ["function".into()], "{ns}.{class}.{method}");
    }
}
