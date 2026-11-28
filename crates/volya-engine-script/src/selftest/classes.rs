// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-script-classes-deriving-from-engine-classes.md
//! on every target, one check per section, with test engine classes (no
//! engine class of the game exists here).

use std::cell::RefCell;
use std::rc::Rc;

use super::ensure;
use super::loading::scripts_of;
use crate::classes::{Conversion, EngineClass, EnginePart};
use crate::loader::{FailureKind, Prefix};
use crate::{ErrorKind, Lua, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 declaring", declaring),
    ("2 constructing", constructing),
    ("3 super", super_calls),
    ("4 members", members),
    ("5 engine calls", engine_calls),
    ("6 base-class calls", base_calls),
    ("7 __finalize and operators", finalize_and_operators),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("classes {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// A test engine part: which constructor made it, from how many
/// arguments, and what its methods did.
struct Probe {
    constructor: &'static str,
    args: usize,
    x: f64,
    log: Vec<String>,
}

/// The parts made, in order.
type Made = Rc<RefCell<Vec<EnginePart>>>;

fn make(
    constructor: &'static str,
    made: &Made,
) -> impl Fn(EnginePart, &[Value]) -> Result<Probe, String> + 'static {
    let made = Rc::clone(made);
    move |part, args| {
        made.borrow_mut().push(part);
        Ok(Probe {
            constructor,
            args: args.len(),
            x: 0.0,
            log: Vec::new(),
        })
    }
}

fn numbers(args: &[Value]) -> Option<u32> {
    args.iter()
        .all(|a| matches!(a, Value::Number(_)))
        .then_some(0)
}

/// `test_base`; `test_engine`, deriving from it, with four constructors
/// (none, one, two numbers, any two) and every kind of member;
/// `test_single`, with one constructor of one parameter.
fn test_classes(made: &Made) -> Vec<EngineClass> {
    vec![
        EngineClass::new("test_base")
            .method("base_name", |_: &mut Probe, _| Ok(vec!["test_base".into()]))
            .constant("base_constant", 7),
        EngineClass::new("test_engine")
            .base("test_base")
            .constructor(0, |_| Some(0), make("none", made))
            .constructor(1, |_| Some(0), make("one", made))
            .constructor(2, numbers, make("numbers", made))
            .constructor(2, |_| Some(1), make("any2", made))
            .method("constructor", |p: &mut Probe, _| {
                Ok(vec![p.constructor.into()])
            })
            .method("args", |p: &mut Probe, _| Ok(vec![(p.args as f64).into()]))
            .method("id", |_: &mut Probe, _| Ok(vec!["id".into()]))
            .overridable("update", |p: &mut Probe, args| {
                p.log.push(format!("update default {}", args.len()));
                Ok(vec!["default".into()])
            })
            .overridable("evaluate", |_: &mut Probe, _| Ok(vec![false.into()]))
            .method_with_default(
                "describe",
                |_: &mut Probe, _| Ok(vec!["member".into()]),
                |_: &mut Probe, _| Ok(vec!["default".into()]),
            )
            .method("get_x", |p: &mut Probe, _| Ok(vec![p.x.into()]))
            .writable_property(
                "x",
                |p: &Probe| p.x.into(),
                |p: &mut Probe, v| {
                    p.x = v.as_number().ok_or("number expected")?;
                    Ok(())
                },
            )
            .property("kind", |_: &Probe| "probe".into())
            .constant("answer", 42),
        EngineClass::new("test_single")
            .constructor(1, |_| None, make("single", made))
            .method("args", |p: &mut Probe, _| Ok(vec![(p.args as f64).into()])),
    ]
}

/// A state with the class system and the test engine classes.
fn state() -> Result<(Lua, Made), String> {
    let mut lua = Lua::new().map_err(|e| e.message)?;
    lua.open_classes().map_err(|e| e.message)?;
    let made = Made::default();
    for class in test_classes(&made) {
        lua.register_class(class).map_err(|e| e.message)?;
    }
    Ok((lua, made))
}

/// Helpers for the Lua checks: `eq`, `raises`, and `failures`, which
/// [`run`] reads. Objects are compared with `rawequal`: their `==` is an
/// operator (7.2).
pub(super) const PRELUDE: &str = r#"
local failures = {}
local function show(v)
  local t = type(v)
  if t == "string" or t == "number" or t == "boolean" or t == "nil" then return tostring(v) end
  return t
end
local function eq(got, want, what)
  if got ~= want then
    table.insert(failures, what .. ": got " .. show(got) .. ", want " .. show(want))
  end
end
local function raises(f, text, what)
  local ok, e = pcall(f)
  if ok then
    table.insert(failures, what .. ": no error")
  elseif type(e) ~= "string" or not string.find(e, text, 1, true) then
    table.insert(failures, what .. ": error " .. show(e))
  end
end
"#;

/// Runs `body` after [`PRELUDE`]; its failures.
fn run(lua: &mut Lua, name: &str, body: &str) -> Result<(), String> {
    let code = format!("{PRELUDE}{body}\nreturn failures");
    let results = lua
        .exec(&format!("={name}"), code.as_bytes())
        .map_err(|e| format!("{name}: {e}"))?;
    failures_of(name, &results)
}

/// The failures a check after [`PRELUDE`] returned.
pub(super) fn failures_of(name: &str, results: &[Value]) -> Result<(), String> {
    let Some(Value::Table(failures)) = results.first() else {
        return Err(format!("{name} returned {results:?}"));
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
            .join("; "))
    }
}

/// Section 1: `class`, where it stores, deriving.
pub fn declaring() -> Result<(), String> {
    let (mut lua, _) = state()?;
    run(
        &mut lua,
        "1.1",
        r#"
class "plain"
-- TODO(#897): gap 3 leaves `type` of a class unstated; ours is a userdata.
eq(type(plain), "userdata", "a class's type")
local class_mt = getmetatable(plain)
local function is_class(v) return v ~= nil and getmetatable(v) == class_mt end
eq(type(class "returns"), "function", "class returns a function")
do
  local this = {}
  class "in_this"
  eq(rawget(_G, "in_this"), nil, "a local `this`: not global")
  eq(is_class(rawget(this, "in_this")), true, "a local `this`: stored there")
  local function body() class "in_body" end
  body()
  eq(is_class(rawget(_G, "in_body")), true, "in a function body: global")
end
do
  local this = 5
  class "this_number"
  eq(is_class(rawget(_G, "this_number")), true, "`this` not a table: global")
end
do
  local this = 1
  local this = {}
  class "first_this"
  eq(rawget(this, "first_this"), nil, "the first `this` counts")
  eq(is_class(rawget(_G, "first_this")), true, "the first `this` not a table: global")
end
class(5)
eq(is_class(rawget(_G, "5")), true, "a number name becomes its text")
raises(function() class({}) end, "index is nil", "a name that isn't a string")
local old = plain
class "plain"
eq(rawequal(old, plain), false, "a declaration replaces the earlier value")
"#,
    )?;
    run(
        &mut lua,
        "1.3",
        r#"
class "base_a"
function base_a:__init() end
function base_a:__finalize() end
function base_a:method() return "base_a.method" end
base_a.value = 5
class "derived_a" (base_a)
function base_a:later() end
eq(derived_a.method, base_a.method, "methods are copied")
eq(derived_a.value, 5, "members are copied")
eq(derived_a.later, nil, "members added to the base later aren't")
eq(derived_a.__init, nil, "__init isn't copied")
eq(derived_a.__finalize, nil, "__finalize isn't copied")
-- Our choice where the outcome is unestablished.
raises(function() class "bad" (nil) end, "the base of class 'bad' is not a class", "a base that isn't a class")
class "on_engine" (test_engine)
function on_engine:__init() super(1, 2) end
eq(on_engine.answer, 42, "an engine constant is copied")
eq(on_engine.base_constant, 7, "an engine base's constant is copied")
local o = on_engine()
eq(o.kind, "probe", "an engine property is copied")
eq(o:base_name(), "test_base", "an engine base's method is copied")
eq(o:constructor(), "numbers", "the engine part is the base's")
"#,
    )?;
    // Namespaces (1.1, 1.2): a top-level class is the script's, reached by
    // its bare name there; `namespace.Class` loads the base's script.
    let (mut s, _) = scripts_of(
        &[
            (
                "_g.script",
                "class \"global_class\" function global_class:__init() end",
            ),
            (
                "ui_base.script",
                "class \"base_tab\" (test_engine) function base_tab:__init() super() end \
                 function base_tab:InitControls() return 'base' end",
            ),
            (
                "ui_tab.script",
                "class \"tab\" (ui_base.base_tab) function tab:__init() super() end \
                 function tab:InitControls() return 'tab+' .. ui_base.base_tab.InitControls(self) end \
                 function make() return tab() end",
            ),
        ],
        Prefix::Default,
    )?;
    let made = Made::default();
    for class in test_classes(&made) {
        s.register_class(class).map_err(|e| e.message)?;
    }
    s.load_globals().map_err(|e| e.message)?;
    let result = s
        .exec(
            "=ns",
            b"local t = ui_tab.make() \
              return t:InitControls(), rawget(_G, 'tab'), rawget(_G, 'base_tab'), \
              type(rawget(_G, 'global_class')), t:constructor()",
        )
        .map_err(|e| e.message)?;
    ensure(
        result
            == [
                "tab+base".into(),
                Value::Nil,
                Value::Nil,
                "userdata".into(),
                "none".into(),
            ],
        || format!("namespaces: {result:?}"),
    )?;
    ensure(s.take_failures().is_empty(), || "namespace failures".into())?;
    // TODO(#897): gap 4 leaves `class` under `-_g` unstated; read through
    // the namespace without a fallback it is nil there.
    let (mut g, _) = scripts_of(&[("xr_g.script", "class \"x\"")], Prefix::G)?;
    g.load_namespace("xr_g").map_err(|e| e.message)?;
    let failures = g.take_failures();
    ensure(
        matches!(&failures[..], [f] if f.kind == FailureKind::Run
            && f.message.contains("attempt to call global 'class'")),
        || format!("-_g: {failures:?}"),
    )
}

/// Section 2: calling a class, `__init`.
pub fn constructing() -> Result<(), String> {
    let (mut lua, _) = state()?;
    run(
        &mut lua,
        "2",
        r#"
class "counted"
local seen
function counted:__init(a, b) seen = {self = self, a = a, b = b} return "ignored" end
local c = counted(1, 2)
eq(rawequal(seen.self, c), true, "__init gets the new object")
eq(seen.a, 1, "__init's first argument")
eq(seen.b, 2, "__init's second argument")
-- TODO(#897): gap 3 leaves `type` of a script object unstated; ours is a userdata.
eq(type(c), "userdata", "a script object's type")
eq(rawequal(counted(), c), false, "each call makes a new object")
class "failing"
function failing:__init() error("in init", 0) end
raises(function() failing() end, "in init", "__init's errors propagate")
class "base_i"
function base_i:__init() end
class "der_i" (base_i)
raises(function() der_i() end, "attempt to call a nil value", "no own __init")
class "dot_init"
function dot_init.__init(self, v) self.v = v end
eq(dot_init(3).v, 3, "Class.__init(self, ...) is the same")
raises(function() test_engine() end, "attempt to call a nil value", "an engine class has no __init")
"#,
    )
}

/// Section 3: `super`.
pub fn super_calls() -> Result<(), String> {
    let (mut lua, made) = state()?;
    run(
        &mut lua,
        "3",
        r#"
class "s_base"
function s_base:__init(v) self.base_v = v s_super = super end
class "s_mid" (s_base)
function s_mid:__init(v) super(v + 1) self.after = super end
class "s_top" (s_mid)
function s_top:__init(v) super(v * 10) end
local t = s_top(1)
eq(t.base_v, 11, "super runs the script base's __init")
eq(s_super, nil, "super in the __init of a base without a base")
eq(t.after, nil, "super after a script base's __init")
eq(super, nil, "super after construction")

class "e0" (test_engine)
function e0:__init() super() end
class "e1" (test_engine)
function e1:__init(a) super(a) end
class "e2" (test_engine)
function e2:__init(a, b) super(a, b) end
class "e3" (test_engine)
function e3:__init() super(1, 2, 3) end
class "single" (test_single)
function single:__init(...) super(...) end
eq(e0():constructor(), "none", "no arguments")
eq(e1(nil):constructor(), "one", "one argument (nil counts)")
eq(e2(1, 2):constructor(), "numbers", "the best score")
eq(e2("a", 2):constructor(), "any2", "the only match")
local none = e3()
eq(none.__ok, false, "no constructor matches: no engine part")
eq(single():args(), 0, "the only constructor, unchecked, no arguments")
eq(single(1, 2, 3):args(), 3, "the only constructor, unchecked, three arguments")

class "keeps" (test_engine)
function keeps:__init() super() kept = super end
keeps()
eq(type(kept), "function", "super isn't reset after an engine base")
class "lazy" (test_engine)
function lazy:__init() end
eq(lazy().__ok, false, "no super: no engine part")
eq(e0().__ok, true, "an engine part")
class "no_base"
function no_base:__init() end
-- Our reading: only a class with a base awaits an engine part.
eq(no_base().__ok, true, "a class without a base")
raises(function() lazy():constructor() end, "has no engine part", "an engine method without the part (our choice)")
class "twice" (test_engine)
function twice:__init() super() super(1) end
eq(twice():constructor(), "one", "super again constructs again")
-- super still holds the last object constructed on an engine base.
kept, super = nil, nil
"#,
    )?;
    // Parts: e0, e1, e2 ×2, single ×2, keeps, e0, twice ×2 (one replaced);
    // the constructors had each part's id.
    ensure(made.borrow().len() == 10, || {
        format!("parts made: {}", made.borrow().len())
    })?;
    lua.exec("=gc", b"collectgarbage() collectgarbage()")
        .map_err(|e| e.message)?;
    ensure(lua.engine_parts() == 0, || {
        format!("parts kept after collection: {}", lua.engine_parts())
    })
}

/// Section 4: members of script objects and of classes.
pub fn members() -> Result<(), String> {
    let (mut lua, _) = state()?;
    run(
        &mut lua,
        "4",
        r#"
class "m" (test_engine)
function m:__init() super(1, 2) end
function m:update() return "override" end
local o = m()
eq(o.__ok, true, "__ok")
o.__ok = "field"
eq(o.__ok, true, "__ok before a field")
o.field = 1
eq(o.field, 1, "a field")
eq(o:update(), "override", "an override before the engine method")
eq(o:describe(), "member", "an engine method among the copied members")
o.update = "own"
eq(o.update, "own", "a field before a class member")
eq(o.kind, "probe", "an engine property")
o.x = 5
eq(o.x, 5, "a written engine property")
eq(o:get_x(), 5, "a written engine property is the engine part's")
o.kind = "mine"
eq(o.kind, "mine", "a read-only property's assignment is a field")
eq(o.nothing, nil, "nothing else")
eq(o[1], nil, "a number key")
o[1] = "one"
eq(o[1], "one", "a number key's field")
class "props" (test_engine)
function props:__init() super() end
function props:kind() return "member" end
eq(type(props().kind), "function", "a class member before an engine property")
raises(function() o.constructor(5) end, "not called on an object of its engine class", "an engine method on a non-object (our message)")
class "plain_m"
function plain_m:__init() end
raises(function() o.constructor(plain_m()) end, "not called on an object of its engine class", "an engine method on another class's object")

-- 4.4: class objects.
eq(m.answer, 42, "Class.constant")
eq(test_engine.answer, 42, "EngineClass.constant")
eq(m.nothing, nil, "Class.nothing")
m.added = "later"
eq(o.added, "later", "assigned to a class: its objects see it")
eq(m.added, "later", "assigned to a class: a default")
eq(m.update ~= nil, true, "Class.override")
eq(type(test_engine.update), "function", "EngineClass.method is a default")
"#,
    )
}

/// Section 5: the engine's calls into overrides.
pub fn engine_calls() -> Result<(), String> {
    let (mut lua, made) = state()?;
    run(
        &mut lua,
        "5",
        r#"
class "binder" (test_engine)
function binder:__init() super(1, 2) end
function binder:update(a) self.got = a return a * 2 end
function binder:evaluate() return eval_result end
function binder:fail() error("boom") end
function binder:dead() error("cannot resume dead coroutine", 0) end
function binder:table() error({}) end
binder.weight = 5
b = binder()
class "plain_binder" (test_engine)
function plain_binder:__init() super(1, 2) end
p = plain_binder()
p.update = function() return "field" end
"#,
    )?;
    let [b, p] = made.borrow()[..] else {
        return Err(format!("parts: {:?}", made.borrow()));
    };
    let call = |lua: &mut Lua, part, method: &str, args: &[Value], conversion| {
        lua.dispatch(part, method, args, conversion)
    };
    let got = call(&mut lua, b, "update", &[3.0.into()], Conversion::Value);
    ensure(got == Ok(6.0.into()), || format!("override: {got:?}"))?;
    let field = lua.exec("=got", b"return b.got").map_err(|e| e.message)?;
    ensure(field == [3.0.into()], || {
        format!("override's self: {field:?}")
    })?;
    let got = call(&mut lua, b, "update", &[3.0.into()], Conversion::Nothing);
    ensure(got == Ok(Value::Nil), || format!("no result: {got:?}"))?;
    // Not overridden (an object field doesn't count): the default.
    let got = call(
        &mut lua,
        p,
        "update",
        &[1.0.into(), 2.0.into()],
        Conversion::Value,
    );
    ensure(got == Ok("default".into()), || format!("default: {got:?}"))?;
    let log = lua.with_part(p, |probe: &mut Probe| probe.log.clone());
    ensure(log == Some(vec!["update default 2".into()]), || {
        format!("default's log: {log:?}")
    })?;
    let got = call(&mut lua, b, "describe", &[], Conversion::Value);
    ensure(got == Ok("default".into()), || {
        format!("the default, not the member: {got:?}")
    })?;
    for (result, want) in [
        ("2.5", 2.0),
        ("3.5", 4.0),
        ("-2.5", -2.0),
        ("2.75", 3.0),
        ("'7'", 7.0),
        ("true", 0.0),
        ("nil", 0.0),
        ("2^31", -2_147_483_648.0),
    ] {
        lua.exec("=r", format!("eval_result = {result}").as_bytes())
            .map_err(|e| e.message)?;
        let got = call(&mut lua, b, "evaluate", &[], Conversion::Integer);
        ensure(got == Ok(want.into()), || {
            format!("integer of {result}: {got:?}")
        })?;
    }
    let failed = call(&mut lua, b, "fail", &[], Conversion::Nothing);
    // Lua's message, with its position and without a traceback.
    ensure(
        matches!(&failed, Err(f) if f.error.kind == ErrorKind::Fatal
            && f.error.message.starts_with("LUA error: 5:")
            && f.error.message.ends_with(": boom")
            && f.log.as_deref() == f.error.message.strip_prefix("LUA error: ")),
        || format!("an override's error: {failed:?}"),
    )?;
    let failed = call(&mut lua, b, "dead", &[], Conversion::Nothing);
    ensure(
        matches!(&failed, Err(f) if f.log.is_none()
            && f.error.message == "LUA error: cannot resume dead coroutine"),
        || format!("a dead coroutine: {failed:?}"),
    )?;
    for (method, text) in [
        ("weight", "attempt to call a number value"),
        ("nothing", "attempt to call a nil value"),
        ("table", "(error object is a table value)"),
    ] {
        let failed = call(&mut lua, b, method, &[], Conversion::Nothing);
        ensure(
            matches!(&failed, Err(f) if f.error.kind == ErrorKind::Fatal
                && f.error.message == format!("LUA error: {text}")),
            || format!("{method}: {failed:?}"),
        )?;
    }
    lua.exec("=gc", b"b = nil collectgarbage() collectgarbage()")
        .map_err(|e| e.message)?;
    let gone = call(&mut lua, b, "update", &[], Conversion::Nothing);
    ensure(
        matches!(&gone, Err(f) if f.error.kind == ErrorKind::Api),
        || format!("a released part: {gone:?}"),
    )?;

    // A failing override stops the game's state.
    let (mut s, _) = scripts_of(
        &[(
            "_g.script",
            "class \"g_binder\" (test_engine) function g_binder:__init() super() end \
             function g_binder:update() error('stop', 0) end g = g_binder()",
        )],
        Prefix::Default,
    )?;
    let made = Made::default();
    for class in test_classes(&made) {
        s.register_class(class).map_err(|e| e.message)?;
    }
    s.load_globals().map_err(|e| e.message)?;
    let part = made.borrow()[0];
    let failed = s.dispatch(part, "update", &[], Conversion::Nothing);
    ensure(
        matches!(&failed, Err(f) if f.error.message == "LUA error: stop"),
        || format!("scripts: {failed:?}"),
    )?;
    let after = s.exec("=after", b"return 1");
    ensure(
        matches!(&after, Err(e) if e.kind == ErrorKind::Fatal && e.message == "LUA error: stop"),
        || format!("after the fatal error: {after:?}"),
    )
}

/// Section 6: base-class calls.
pub fn base_calls() -> Result<(), String> {
    let (mut lua, made) = state()?;
    run(
        &mut lua,
        "6",
        r#"
class "bc" (test_engine)
function bc:__init() super(1, 2) end
function bc:describe() return "bc+" .. test_engine.describe(self) end
function bc:update() return "override" end
local o = bc()
eq(o:describe(), "bc+default", "EngineBase.m is the default implementation")
eq(test_engine.update(o), "default", "the default, not the override")
eq(o:id(), "id", "a member without a default")
raises(function() test_engine.id(o) end, "pure virtual function called", "a default without an implementation")
class "sb" (bc)
function sb:__init() super() end
function sb:describe() return "sb+" .. bc.describe(self) end
local s = sb()
eq(s:describe(), "sb+bc+default", "ScriptBase.m")
eq(rawequal(bc.evaluate, test_engine.evaluate), true, "a script base's copied engine member is the default")
eq(sb.evaluate(s), false, "calling it")
"#,
    )?;
    let logs: Vec<_> = made
        .borrow()
        .iter()
        .map(|&p| lua.with_part(p, |probe: &mut Probe| probe.log.clone()))
        .collect();
    ensure(
        logs == [Some(vec!["update default 0".into()]), Some(Vec::new())],
        || format!("defaults run: {logs:?}"),
    )
}

/// Section 7: `__finalize` and operators.
pub fn finalize_and_operators() -> Result<(), String> {
    let (mut lua, made) = state()?;
    run(
        &mut lua,
        "7.1",
        r#"
finalized = {}
class "f_base"
function f_base:__init() end
function f_base:__finalize() table.insert(finalized, "f_base") end
class "f_mid" (f_base)
function f_mid:__init() super() end
class "f_top" (f_mid)
function f_top:__init() super() end
function f_top:__finalize() table.insert(finalized, "f_top") error("discarded") end
class "f_engine" (test_engine)
function f_engine:__init() super() end
function f_engine:__finalize() table.insert(finalized, "f_engine " .. self:constructor()) end
f_top()
f_engine()
-- super still holds the object constructed on an engine base.
super = nil
collectgarbage()
collectgarbage()
table.sort(finalized)
eq(table.concat(finalized, ","), "f_base,f_engine none,f_top", "finalizers up the script bases")
"#,
    )?;
    ensure(made.borrow().len() == 1 && lua.engine_parts() == 0, || {
        "the engine part is released after __finalize".into()
    })?;
    run(
        &mut lua,
        "7.2",
        r#"
class "v"
function v:__init(n) self.n = n end
local function n(x) if type(x) == "number" then return x end return x.n end
function v.__add(a, b) return "add " .. n(a) .. " " .. n(b) end
function v.__sub(a, b) return "sub" end
function v.__mul(a, b) return "mul" end
function v.__div(a, b) return "div" end
function v.__pow(a, b) return "pow" end
function v.__lt(a, b) return n(a) < n(b) end
function v.__le(a, b) return n(a) <= n(b) end
function v.__eq(a, b) return n(a) == n(b) end
function v.__call(self, x) return "call " .. x end
function v.__unm(a) return "unm " .. a.n end
function v.__tostring(a) return "v" .. a.n end
function v.__concat(a, b) return "concat" end
local a, b = v(1), v(2)
eq(a + b, "add 1 2", "__add")
eq(a + 5, "add 1 5", "__add with a number second")
eq(5 + a, "add 5 1", "__add found on the second operand")
eq(a - b, "sub", "__sub")
eq(a * b, "mul", "__mul")
eq(a / b, "div", "__div")
eq(a ^ b, "pow", "__pow")
eq(a < b, true, "__lt")
eq(a <= b, true, "__le")
eq(a == v(1), true, "__eq")
eq(a == b, false, "__eq false")
eq(a(3), "call 3", "__call")
eq(-a, "unm 1", "__unm")
eq(tostring(a), "v1", "__tostring")
eq(a .. "x", "concat", "__concat")
eq("x" .. a, "concat", "__concat found on the second operand")
local f = v(4)
f.__add = function() return "own" end
eq(f + 1, "own", "an operator found as object.name is")

class "w"
function w:__init() end
local x, y = w(), w()
raises(function() return x + y end, "No such operator defined", "no operator")
raises(function() return x(1) end, "No such operator defined", "no __call")
raises(function() return x == y end, "No such operator defined", "no __eq")
-- TODO(#897): gap 3 leaves tostring of an object without __tostring
-- unstated; 7.2 read as written makes it the missing operator's error.
raises(function() return tostring(x) end, "No such operator defined", "no __tostring")
eq(rawequal(x, x) and x == x, true, "an object equals itself")
"#,
    )
}
