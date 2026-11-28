// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The runtime's behaviour, the script loader, script classes, the
//! object factory, the engine services, the value types, configuration
//! and files, game objects, object binders, object event callbacks, the
//! story state, the engine's entry points, the dialog files' functions, the
//! dialogs and trade, time, the sound, the simulation registry, the
//! scripted actions, the window description files and movement, sight
//! and memory,
//! checked on every target: natively by `cargo test`, in the browser by
//! the `script_selftest` op
//! (docs/testing.md, Shared conformance checks).

use crate::{ErrorKind, Lua, Value};

pub mod character_talk_state;
pub mod classes;
pub mod dialogs;
pub mod dialogs_and_trade;
pub mod entry_points;
pub mod files;
pub mod game_object;
mod loading;
pub mod movement_sight_and_memory;
pub mod object_binder;
pub mod object_callbacks;
pub mod object_factory;
pub mod physics;
pub mod scripted_actions;
pub mod services;
pub mod simulation;
pub mod simulation_objects;
pub mod sound;
pub mod story;
pub mod time;
pub mod values;
pub mod window_files;

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let mut failures = Vec::new();
    let mut fail = |what: String| failures.push(what);
    if let Err(e) = lua_checks() {
        fail(e);
    }
    if let Err(e) = api_checks() {
        fail(e);
    }
    if let Err(e) = loading::check() {
        fail(e);
    }
    if let Err(e) = classes::check() {
        fail(e);
    }
    if let Err(e) = object_factory::check() {
        fail(e);
    }
    if let Err(e) = services::check() {
        fail(e);
    }
    if let Err(e) = values::check() {
        fail(e);
    }
    if let Err(e) = files::check() {
        fail(e);
    }
    if let Err(e) = game_object::check() {
        fail(e);
    }
    if let Err(e) = object_binder::check() {
        fail(e);
    }
    if let Err(e) = story::check() {
        fail(e);
    }
    if let Err(e) = entry_points::check() {
        fail(e);
    }
    if let Err(e) = object_callbacks::check() {
        fail(e);
    }
    if let Err(e) = physics::check() {
        fail(e);
    }
    if let Err(e) = dialogs::check() {
        fail(e);
    }
    if let Err(e) = sound::check() {
        fail(e);
    }
    if let Err(e) = dialogs_and_trade::check() {
        fail(e);
    }
    if let Err(e) = time::check() {
        fail(e);
    }
    if let Err(e) = character_talk_state::check() {
        fail(e);
    }
    if let Err(e) = movement_sight_and_memory::check() {
        fail(e);
    }
    if let Err(e) = simulation::check() {
        fail(e);
    }
    if let Err(e) = simulation_objects::check() {
        fail(e);
    }
    if let Err(e) = scripted_actions::check() {
        fail(e);
    }
    if let Err(e) = window_files::check() {
        fail(e);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// selftest.lua: the libraries, numbers, strings, tables, errors,
/// coroutines and `jit`.
fn lua_checks() -> Result<(), String> {
    let mut lua = Lua::new().map_err(|e| e.message)?;
    let results = lua
        .exec("=selftest", include_bytes!("selftest.lua"))
        .map_err(|e| format!("selftest.lua: {e}"))?;
    let Some(Value::Table(failures)) = results.first() else {
        return Err(format!("selftest.lua returned {results:?}"));
    };
    let mut failures: Vec<(f64, String)> = failures
        .iter()
        .map(|(k, v)| {
            let text = match v {
                Value::String(s) => String::from_utf8_lossy(s).into_owned(),
                v => format!("{v:?}"),
            };
            (k.as_number().unwrap_or(0.0), text)
        })
        .collect();
    failures.sort_by(|a, b| a.0.total_cmp(&b.0));
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures
            .into_iter()
            .map(|(_, t)| t)
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

fn ensure(ok: bool, what: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what()) }
}

/// The Rust API: values both ways, globals, Rust functions, errors.
fn api_checks() -> Result<(), String> {
    let mut lua = Lua::new().map_err(|e| e.message)?;

    let values = lua
        .exec("=values", b"return nil, true, 1.5, 'text', {1, 2, k = 'v'}")
        .map_err(|e| e.message)?;
    ensure(values.len() == 5, || format!("results: {values:?}"))?;
    ensure(
        values[..4] == [Value::Nil, true.into(), 1.5.into(), "text".into()],
        || format!("scalars: {values:?}"),
    )?;
    let table = &values[4];
    ensure(
        table.get(&1.0.into()) == Some(&1.0.into())
            && table.get(&2.0.into()) == Some(&2.0.into())
            && table.get(&"k".into()) == Some(&"v".into()),
        || format!("table: {table:?}"),
    )?;

    lua.exec(
        "=define",
        b"function describe(...) local t = {} for i = 1, select('#', ...) do t[i] = type((select(i, ...))) end return table.concat(t, ',') end
          function sum(t) local s = 0 for _, v in ipairs(t) do s = s + v end return s end",
    )
    .map_err(|e| e.message)?;
    let args = [
        Value::Nil,
        false.into(),
        2.0.into(),
        Value::string(vec![0xC0, 0]),
        Value::Table(vec![]),
    ];
    let described = lua.call("describe", &args).map_err(|e| e.message)?;
    ensure(
        described == ["nil,boolean,number,string,table".into()],
        || format!("describe: {described:?}"),
    )?;
    let list = Value::Table(
        (1..=4)
            .map(|i| (f64::from(i).into(), f64::from(i).into()))
            .collect(),
    );
    let sum = lua.call("sum", &[list]).map_err(|e| e.message)?;
    ensure(sum == [10.0.into()], || format!("sum: {sum:?}"))?;

    let missing = lua.call("missing", &[]);
    ensure(
        matches!(&missing, Err(e) if e.kind == ErrorKind::Runtime && e.message.contains("attempt to call")),
        || format!("missing function: {missing:?}"),
    )?;
    let syntax = lua.exec("=bad", b"return +");
    ensure(
        matches!(&syntax, Err(e) if e.kind == ErrorKind::Syntax && e.message.starts_with("bad:1:")),
        || format!("syntax error: {syntax:?}"),
    )?;
    let raised = lua.exec("=raise", b"error('boom', 0)");
    ensure(
        matches!(&raised, Err(e) if e.kind == ErrorKind::Runtime && e.message == "boom"),
        || format!("raised: {raised:?}"),
    )?;
    let table_error = lua.exec("=raise", b"error({})");
    ensure(
        matches!(&table_error, Err(e) if e.message == "(error object is a table value)"),
        || format!("table error: {table_error:?}"),
    )?;
    ensure(lua.compile("=compile", b"error('not run')").is_ok(), || {
        "compile ran the chunk".into()
    })?;
    let function = lua.exec("=function", b"return print");
    ensure(function == Ok(vec![Value::Other("function")]), || {
        format!("function: {function:?}")
    })?;
    let cyclic = lua.exec("=cyclic", b"local t = {} t.t = t return t");
    ensure(
        matches!(&cyclic, Err(e) if e.kind == ErrorKind::Api),
        || format!("cyclic: {cyclic:?}"),
    )?;
    let other = lua.call("describe", &[Value::Other("function")]);
    ensure(matches!(&other, Err(e) if e.kind == ErrorKind::Api), || {
        format!("other: {other:?}")
    })?;
    for key in [Value::Nil, f64::NAN.into()] {
        let bad_key = lua.call("describe", &[Value::Table(vec![(key, 1.0.into())])]);
        ensure(
            matches!(&bad_key, Err(e) if e.kind == ErrorKind::Api),
            || format!("nil or NaN key: {bad_key:?}"),
        )?;
    }
    let number_error = lua.exec("=raise", b"error(-1/3, 0)");
    ensure(
        matches!(&number_error, Err(e) if e.message == "-0.33333333333333"),
        || format!("number error: {number_error:?}"),
    )?;

    // Bytes, NUL included, cross both ways unchanged.
    lua.exec("=identity", b"function identity(...) return ... end")
        .map_err(|e| e.message)?;
    let bytes = Value::String(vec![b'a', 0, 0xC0, 0, 0xFF, b'z']);
    let nested = Value::Table(vec![(bytes.clone(), bytes.clone())]);
    let back = lua
        .call("identity", &[bytes.clone(), nested.clone()])
        .map_err(|e| e.message)?;
    ensure(back == [bytes, nested], || format!("bytes: {back:?}"))?;
    let len = lua
        .exec("=len", b"return #'a\0b', string.byte('a\0b', 2)")
        .map_err(|e| e.message)?;
    ensure(len == [3.0.into(), 0.0.into()], || {
        format!("NUL in Lua: {len:?}")
    })?;

    // Nesting: 64 levels cross, deeper ones are refused.
    let deep = |n| format!("local t = {{}} for i = 1, {n} do t = {{t}} end return t");
    let ok = lua.exec("=deep", deep(63).as_bytes());
    ensure(ok.is_ok(), || format!("64 levels: {ok:?}"))?;
    let too_deep = lua.exec("=deep", deep(64).as_bytes());
    ensure(
        matches!(&too_deep, Err(e) if e.kind == ErrorKind::Api),
        || format!("65 levels: {too_deep:?}"),
    )?;

    let mut calls = 0;
    lua.register("rust_add", move |args| {
        calls += 1;
        match args {
            [Value::Number(a), Value::Number(b)] => {
                Ok(vec![(a + b).into(), f64::from(calls).into()])
            }
            _ => Err(format!("rust_add: two numbers expected, got {args:?}")),
        }
    })
    .map_err(|e| e.message)?;
    lua.register("rust_table", |_| {
        Ok(vec![Value::Table(vec![
            ("name".into(), "volya".into()),
            ("list".into(), Value::Table(vec![(1.0.into(), true.into())])),
        ])])
    })
    .map_err(|e| e.message)?;
    let added = lua
        .exec(
            "=rust",
            b"local s, n = rust_add(2, 3)
              local s2, n2 = rust_add(s, 1)
              local t = rust_table()
              local ok, err = pcall(rust_add, 'x')
              local co = coroutine.wrap(function() return rust_add(1, 1) end)
              return s2, n2, t.name, t.list[1], ok, err, co()",
        )
        .map_err(|e| e.message)?;
    let want: Vec<Value> = vec![
        6.0.into(),
        2.0.into(),
        "volya".into(),
        true.into(),
        false.into(),
        "rust_add: two numbers expected, got [String([120])]".into(),
        2.0.into(),
        4.0.into(),
    ];
    ensure(added == want, || format!("Rust functions: {added:?}"))?;
    Ok(())
}
