// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-loading.md on every target: the lexer's C-style
//! comments (section 1) and the script loader (section 2) on files of our
//! own.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Cursor};
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::vfs::{self, DirEntry, Vfs, VfsFile};

use super::ensure;
use crate::loader::{self, Common, FailureKind, Prefix, Scripts};
use crate::{ErrorKind, Lua, Value};

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let mut failures = Vec::new();
    for (what, result) in [
        ("comments", comment_checks()),
        ("prefix", prefix_checks()),
        ("loader", loader_checks()),
        ("loader -_g", g_prefix_checks()),
    ] {
        if let Err(e) = result {
            failures.push(format!("{what}: {e}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// What a chunk returns (its values joined by spaces), or its error's start.
type Outcome = Result<&'static [u8], &'static str>;

/// Chunks and their outcomes.
const COMMENTS: &[(&[u8], Outcome)] = &[
    // Division is unchanged (1.2).
    (b"return 6/3, 6 / 3, 6/ 3", Ok(b"2 2 2")),
    (b"local a = 4 return 8/a", Ok(b"2")),
    // `//` up to the next LF (1.3).
    (b"return 1 // a comment", Ok(b"1")),
    (b"return 1 //", Ok(b"1")),
    (b"return 6 //\n/ 3", Ok(b"2")),
    (b"// return 1\nreturn 2", Ok(b"2")),
    (b"//\r\nreturn 3", Ok(b"3")),
    // A CR doesn't end it.
    (b"// a\r return 1\nreturn 2", Ok(b"2")),
    // Quotes, brackets and `--` inside it mean nothing.
    (b"// \"unfinished\nreturn 3", Ok(b"3")),
    (b"// [[\nreturn 4", Ok(b"4")),
    (b"// --[[\nreturn 5", Ok(b"5")),
    (b"// /* \nreturn 6", Ok(b"6")),
    // Not inside strings or `--` comments (1.2).
    (
        b"return \"a//b\", '/*', [[//]], [=[/* */]=]",
        Ok(b"a//b /* // /* */"),
    ),
    (b"return \"a\" // \"b\"", Ok(b"a")),
    (b"-- /*\nreturn 1", Ok(b"1")),
    (b"--[[ /* ]] return 2", Ok(b"2")),
    (b"--[[ // ]] return 3", Ok(b"3")),
    (b"return '\\\n// not a comment'", Ok(b"\n// not a comment")),
    // `/* */` (1.4).
    (b"return /* x */ 5", Ok(b"5")),
    (b"return 1 /**/ + 1", Ok(b"2")),
    (b"return /*\nreturn 1\n*/ 7", Ok(b"7")),
    (b"return /* \"unfinished [[ -- */ 8", Ok(b"8")),
    // `**` is a pair: `/***/` and `**/` don't end it.
    (b"return 1 /***/ + 1 */ + 2", Ok(b"3")),
    (b"return 1 /* a **/ + 5 */ + 1", Ok(b"2")),
    (b"return 1 /* a ***/ + 1", Ok(b"2")),
    // No nesting: the first `*/` ends it.
    (b"return 1 /* /* */ + 1", Ok(b"2")),
    (
        b"return 1 /* /* */ + 1 */",
        Err("t:1: unexpected symbol near"),
    ),
    // No end is no error.
    (b"return 9 /* never ends", Ok(b"9")),
    (b"return 9 /* ends in a star *", Ok(b"9")),
    (b"return 9 /*", Ok(b"9")),
    // It separates tokens.
    (b"local a/**/=/**/3 return/**/a", Ok(b"3")),
    (b"return 1/**/2", Err("t:1: ")),
    // Line breaks inside aren't counted (1.4); `//` counts its LF (1.3).
    (
        b"x = 1\n/*\r\n\n\r*/\n+",
        Err("t:3: unexpected symbol near"),
    ),
    (b"/*\n\n*/ error('here')", Err("t:1: here")),
    (b"/*\n*/\n/*\n\n*/ error('here')", Err("t:2: here")),
    (b"// a\r\n// b\n+", Err("t:3: unexpected symbol near")),
    (b"// a\rb\n+", Err("t:2: unexpected symbol near")),
    // Errors are the release's.
    (b"x = /* */ +", Err("t:1: unexpected symbol near '+'")),
    (b"return 'a // b", Err("t:1: unfinished string")),
];

fn text(values: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        match v {
            Value::String(s) => out.extend_from_slice(s),
            Value::Number(n) => out.extend_from_slice(crate::number_text(*n).as_bytes()),
            v => out.extend_from_slice(format!("{v:?}").as_bytes()),
        }
    }
    out
}

fn comment_checks() -> Result<(), String> {
    let mut lua = Lua::new().map_err(|e| e.message)?;
    let mut failures = Vec::new();
    for (code, want) in COMMENTS {
        let got = lua.exec("=t", code);
        let ok = match (&got, want) {
            (Ok(values), Ok(want)) => text(values) == *want,
            (Err(e), Err(want)) => e.message.starts_with(want),
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{:?}: got {got:?}, want {want:?}",
                String::from_utf8_lossy(code)
            ));
        }
    }
    // Every chunk the runtime compiles (1.1): `loadstring` and `load` too.
    let compiled = lua.exec(
        "=t",
        b"local parts = { 'return 1 // c\\n', '+ /* c */ 1' }
          local i = 0
          local f = load(function() i = i + 1 return parts[i] end)
          return loadstring('return 2 /* c */ // c')(), f(), (loadstring('/*\\n*/ +', '=s'))",
    );
    let want = [2.0.into(), 2.0.into(), Value::Nil];
    if compiled.as_deref() != Ok(&want[..]) {
        failures.push(format!("loadstring and load: {compiled:?}"));
    }
    let line = lua.exec("=t", b"return select(2, loadstring('/*\\n*/ +', '=s'))");
    if !matches!(&line, Ok(v) if v.first().and_then(Value::as_str).is_some_and(|s| s.starts_with("s:1:")))
    {
        failures.push(format!("loadstring's line numbers: {line:?}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn prefix_checks() -> Result<(), String> {
    let chunk = |name: &[u8], prefix| loader::chunk(name, b"<file>", prefix);
    let xr_logic = b"local function script_name() return \"xr_logic\" end local this = {} \
        xr_logic= this  setmetatable(this, {__index = _G}) setfenv(1, this) \t\t<file>";
    ensure(
        chunk(b"xr_logic", Prefix::Default).as_deref() == Some(&xr_logic[..]),
        || format!("xr_logic: {:?}", chunk(b"xr_logic", Prefix::Default)),
    )?;
    // The templates' lengths, `%s` included (2.3): the empty name's chunk
    // would be the template less its 3 `%s` and plus `=`.
    for (prefix, len) in [(Prefix::Default, 126), (Prefix::G, 104)] {
        let a = chunk(b"a", prefix).unwrap();
        ensure(a.len() - b"<file>".len() == len - 6 + 1 + 2, || {
            format!("{prefix:?} length: {}", String::from_utf8_lossy(&a))
        })?;
    }
    let g = chunk(b"a.b.c", Prefix::G).unwrap();
    let want = b"local function script_name() return \"a.b.c\" end local this = {} this._G = _G \
        a={b={c= this }} setfenv(1, this) \t\t<file>";
    ensure(g == want, || {
        format!("a.b.c: {}", String::from_utf8_lossy(&g))
    })?;
    ensure(
        chunk(b"_G", Prefix::Default).as_deref() == Some(&b"<file>"[..]),
        || "_G has a prefix".into(),
    )?;
    for bad in [&b""[..], b"a.", b".a", b"a..b"] {
        ensure(chunk(bad, Prefix::Default).is_none(), || {
            format!("{bad:?} has a chunk")
        })?;
    }
    ensure(
        Prefix::from_command_line("-NoSound -_G") == Prefix::G,
        || "-_G".into(),
    )?;
    ensure(
        Prefix::from_command_line("-nojit") == Prefix::Default,
        || "-nojit".into(),
    )?;
    ensure(
        loader::function_namespace("a.b.f") == Some("a.b")
            && loader::function_namespace("f").is_none(),
        || "function_namespace".into(),
    )?;
    Ok(())
}

/// Files in memory, recording every path opened.
struct Files {
    files: HashMap<String, Vec<u8>>,
    opened: RefCell<Vec<String>>,
}

impl Vfs for Files {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let key = vfs::normalize(path)?;
        self.opened.borrow_mut().push(key.clone());
        match self.files.get(&key) {
            Some(data) => Ok(Box::new(File(Cursor::new(data.clone())))),
            None => Err(io::Error::new(io::ErrorKind::NotFound, key)),
        }
    }

    fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
        Err(io::Error::other("not listed"))
    }
}

struct File(Cursor<Vec<u8>>);

impl io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Seek for File {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

impl VfsFile for File {
    fn size(&self) -> u64 {
        self.0.get_ref().len() as u64
    }
}

const FSGAME: &[u8] = b"$game_data$ = false| true | $fs_root$ | gamedata\\\r\n\
    $game_scripts$ = true | false| $game_data$ | scripts\\\r\n\
    $game_config$ = true | false| $game_data$ | config\\\r\n";

const SCRIPTS: &[(&str, &str)] = &[
    (
        "_g.script",
        "function global_f() return 'g' end g_loads = (g_loads or 0) + 1 // C-style\r\n",
    ),
    (
        "xr_a.script",
        "value = 1\r\nfunction get() return value, script_name(), this, print end",
    ),
    ("xr_b.script", "b_value = xr_a.value + 1"),
    ("xr_bad.script", "x = "),
    ("xr_run.script", "partial = 1 error('boom') after = 1"),
    ("a.b.script", "v = 1"),
    ("xr_case.script", "v = 'case'"),
    ("5.script", "v = 5"),
    ("copy of x.script", "v = 1"),
    (
        "xr_lines.script",
        "/*\r\n\r\n*/ function f() error('here') end\r\n",
    ),
    ("bad_ns.script", "v = 1"),
];

fn scripts(prefix: Prefix) -> Result<(Scripts, Rc<Files>), String> {
    let files = Rc::new(Files {
        files: SCRIPTS
            .iter()
            .map(|(name, text)| (format!("gamedata/scripts/{name}"), text.as_bytes().to_vec()))
            .chain([(
                "gamedata/config/script.ltx".to_owned(),
                b"[common]\r\nclass_registrators = class_registrator.register, \
                  game_registrator.register\r\nscript = _G\r\n"
                    .to_vec(),
            )])
            .collect(),
        opened: RefCell::new(Vec::new()),
    });
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let common = Common::read(&*files, &aliases).map_err(|e| e.to_string())?;
    ensure(
        common.class_registrators == ["class_registrator.register", "game_registrator.register"]
            && common.scripts == ["_G"],
        || format!("[common]: {common:?}"),
    )?;
    files.opened.borrow_mut().clear();
    let scripts = Scripts::new(files.clone(), &aliases, prefix).map_err(|e| e.message)?;
    Ok((scripts, files))
}

fn loader_checks() -> Result<(), String> {
    let (mut s, files) = scripts(Prefix::Default)?;
    let opened = |files: &Files, name: &str| {
        let path = format!("gamedata/scripts/{name}.script");
        files.opened.borrow().iter().filter(|p| **p == path).count()
    };
    let exec = |s: &mut Scripts, code: &str| s.lua().exec("=t", code.as_bytes());
    let run = |s: &mut Scripts, code: &str| exec(s, code).map_err(|e| format!("{code}: {e}"));

    // Nothing of the loader is reachable but `prefetch` and the handler.
    let hidden = run(
        &mut s,
        "return type(prefetch), rawget(_G, '__volya_script_chunk'), \
         rawget(_G, '__volya_script_report'), type(getmetatable(_G).__index)",
    )?;
    ensure(
        hidden == ["function".into(), Value::Nil, Value::Nil, "function".into()],
        || format!("globals: {hidden:?}"),
    )?;

    // Startup loads `_G` into the globals, without the test (2.2.1).
    s.load_globals().map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    s.load_namespace("_G").map_err(|e| e.message)?;
    let g = run(&mut s, "return global_f(), g_loads")?;
    ensure(g == ["g".into(), 2.0.into()], || format!("_G: {g:?}"))?;

    // An undefined global loads its script into a namespace (2.2.3, 2.3).
    let a = run(
        &mut s,
        "local t = xr_a prefetch('xr_a') local v, n, this, p = xr_a.get() \
         return xr_a.value, v, n, this == xr_a, type(p), t == xr_a",
    )?;
    let want: Vec<Value> = vec![
        1.0.into(),
        1.0.into(),
        "xr_a".into(),
        true.into(),
        "function".into(),
        true.into(),
    ];
    ensure(a == want, || format!("xr_a: {a:?}"))?;
    ensure(opened(&files, "xr_a") == 1, || "xr_a read twice".into())?;
    // A namespace reads missing names from the globals, which load too.
    let b = run(&mut s, "return xr_b.b_value, rawget(xr_b, 'xr_a')")?;
    ensure(b == [2.0.into(), Value::Nil], || format!("xr_b: {b:?}"))?;
    // Case-insensitive files; the namespace keeps the name's case.
    let case = run(&mut s, "return XR_Case.v, rawget(_G, 'xr_case')")?;
    ensure(case == ["case".into(), Value::Nil], || {
        format!("case: {case:?}")
    })?;
    // Dotted names nest (2.2, 2.3).
    let dotted = run(&mut s, "prefetch('a.b') return a.b.v, type(a)")?;
    ensure(dotted == [1.0.into(), "table".into()], || {
        format!("a.b: {dotted:?}")
    })?;

    // A missing file loads nothing; the last missing name isn't looked for
    // again while it is the one remembered.
    let missing = run(
        &mut s,
        "local a, b = no_such, no_such local c = other_missing return a, b, c, no_such",
    )?;
    ensure(missing == [const { Value::Nil }; 4], || {
        format!("missing: {missing:?}")
    })?;
    ensure(
        opened(&files, "no_such") == 2 && opened(&files, "other_missing") == 1,
        || format!("missing reads: {:?}", files.opened.borrow()),
    )?;
    // `prefetch` and the engine look for it whether or not it was missing.
    run(&mut s, "prefetch('other_missing')")?;
    s.load_namespace("other_missing").map_err(|e| e.message)?;
    ensure(opened(&files, "other_missing") == 3, || {
        "prefetch skipped a missing file".into()
    })?;
    // The empty string and empty components load nothing.
    run(&mut s, "local _ = _G[''] prefetch('a..b') prefetch('.x')")?;
    ensure(
        !files
            .opened
            .borrow()
            .iter()
            .any(|p| p.ends_with("/.script") || p.contains("..") || p.contains("/.x")),
        || format!("empty names read: {:?}", files.opened.borrow()),
    )?;
    ensure(s.take_failures().is_empty(), || {
        "unexpected failures".into()
    })?;

    // A compile failure leaves no namespace, so the next access tries again
    // (2.4); nothing reaches the script.
    let bad = run(&mut s, "local a = xr_bad return a, xr_bad")?;
    ensure(bad == [Value::Nil, Value::Nil], || {
        format!("xr_bad: {bad:?}")
    })?;
    ensure(opened(&files, "xr_bad") == 2, || {
        "xr_bad not retried".into()
    })?;
    let failures = s.take_failures();
    ensure(
        failures.len() == 2
            && failures.iter().all(|f| {
                f.kind == FailureKind::Compile
                    && f.message.starts_with("gamedata\\scripts\\xr_bad.script:1:")
            }),
        || format!("xr_bad failures: {failures:?}"),
    )?;
    // A run failure leaves the namespace with what it defined (2.4).
    let partial = run(
        &mut s,
        "return xr_run.partial, xr_run.after, xr_run.partial",
    )?;
    ensure(partial == [1.0.into(), Value::Nil, 1.0.into()], || {
        format!("xr_run: {partial:?}")
    })?;
    ensure(opened(&files, "xr_run") == 1, || "xr_run reloaded".into())?;
    let failures = s.take_failures();
    ensure(
        matches!(&failures[..], [f] if f.kind == FailureKind::Run && f.message.contains("boom")),
        || format!("xr_run failures: {failures:?}"),
    )?;
    // Names that aren't Lua names never compile (2.3), numbers convert.
    run(&mut s, "prefetch('copy of x') local _ = _G[5]")?;
    let failures = s.take_failures();
    ensure(
        failures.len() == 2
            && failures.iter().all(|f| f.kind == FailureKind::Compile)
            && opened(&files, "5") == 1,
        || format!("names: {failures:?}"),
    )?;
    // Line numbers skip block comments' line breaks; the chunk is named
    // after the file's path (2.3).
    let lines = run(&mut s, "return pcall(xr_lines.f)")?;
    ensure(
        lines
            == [
                false.into(),
                "gamedata\\scripts\\xr_lines.script:1: here".into(),
            ],
        || format!("xr_lines: {lines:?}"),
    )?;

    // A name in use by a non-table is the fatal error (2.2).
    let fatal = run(&mut s, "bad_ns = 5 return pcall(prefetch, 'bad_ns')")?;
    ensure(
        matches!(&fatal[..], [Value::Boolean(false), Value::String(m)]
            if String::from_utf8_lossy(m).contains("the namespace name is already being used by the non-table object")),
        || format!("fatal: {fatal:?}"),
    )?;
    let fatal = s.load_namespace("bad_ns");
    ensure(
        matches!(&fatal, Err(e) if e.kind == ErrorKind::Runtime),
        || format!("fatal from Rust: {fatal:?}"),
    )?;
    let failures = s.take_failures();
    ensure(
        failures.len() == 2 && failures.iter().all(|f| f.kind == FailureKind::Fatal),
        || format!("fatal failures: {failures:?}"),
    )?;
    ensure(opened(&files, "bad_ns") == 0, || "bad_ns read".into())?;
    let wrong = exec(&mut s, "prefetch({})");
    ensure(
        matches!(&wrong, Err(e) if e.message.contains("bad argument #1 to 'prefetch'")),
        || format!("prefetch({{}}): {wrong:?}"),
    )?;
    Ok(())
}

/// With `-_g`, namespaces don't fall back to the globals and have `_G` (2.3).
fn g_prefix_checks() -> Result<(), String> {
    let (mut s, _) = scripts(Prefix::G)?;
    s.load_namespace("xr_a").map_err(|e| e.message)?;
    let a = s
        .lua()
        .exec(
            "=t",
            b"local v, n, this, p = xr_a.get() return xr_a._G == _G, n, this == xr_a, p",
        )
        .map_err(|e| e.message)?;
    ensure(
        a == [true.into(), "xr_a".into(), true.into(), Value::Nil],
        || format!("xr_a: {a:?}"),
    )?;
    ensure(s.take_failures().is_empty(), || "failures".into())?;
    Ok(())
}
