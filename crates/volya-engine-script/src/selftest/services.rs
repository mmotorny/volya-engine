// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-engine-services.md on every target, one check
//! per section, over [`EngineServices`] with the engine's console.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::console::{ActionLine, COMMANDS, Console, ConsoleFiles, Kind};
use volya_engine_formats::ltx;
use volya_engine_formats::skeletal_motions::{MotionDefinition, MotionParams, MotionSet};

use super::classes::PRELUDE;
use super::ensure;
use super::loading::scripts_of;
use crate::loader::{Prefix, Scripts};
use crate::services::{Camera, EngineServices, GameType, KinematicsAnimated, RenderVisual};
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 console", console),
    ("3 render device", device),
    ("4 global functions", functions),
    ("5 classes", classes),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("services {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

type Shared = Rc<RefCell<EngineServices>>;

const SYSTEM: &str = "[script]\r\ncurrent_server_entity_version = 7\r\n";

/// A game script state with the services registered and `_G` loaded.
fn state(system: &str) -> Result<(Scripts, Shared), String> {
    let (mut s, _) = scripts_of(&[("_g.script", "")], Prefix::Default)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(system.as_bytes()).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system)));
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    Ok((s, services))
}

/// Runs `body` after the classes' prelude; its failures.
pub(super) fn run(s: &mut Scripts, name: &str, body: &str) -> Result<(), String> {
    let code = format!("{PRELUDE}{body}\nreturn failures");
    let results = s
        .exec(&format!("={name}"), code.as_bytes())
        .map_err(|e| format!("{name}: {e}"))?;
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

/// `code`'s results.
fn exec(s: &mut Scripts, code: &str) -> Result<Vec<Value>, String> {
    s.exec("=check", code.as_bytes()).map_err(|e| e.message)
}

/// `code` must stop the state with a fatal error whose report's rows
/// include `rows`, in the log.
fn fatal(code: &str, rows: &[&str]) -> Result<(), String> {
    let (mut s, services) = state(SYSTEM)?;
    let result = s.exec("=fatal", format!("pcall(function() {code} end)").as_bytes());
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: {result:?}, not fatal"),
    )?;
    let log = services.borrow().console.log().join("\n");
    for row in rows {
        ensure(log.contains(row), || {
            format!("{code}: no {row:?} in the log:\n{log}")
        })?;
    }
    let later = s.exec("=later", b"return 1");
    ensure(
        matches!(&later, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: the state goes on: {later:?}"),
    )
}

/// A command of `kind`'s first (registered).
fn command_of(kind: fn(&Kind) -> bool) -> &'static str {
    COMMANDS
        .iter()
        .find(|d| kind(&d.kind) && Console::def(d.name).is_some())
        .map(|d| d.name)
        .expect("a command of the kind")
}

/// Section 1: what is registered, and the single objects.
pub fn registered() -> Result<(), String> {
    let (mut s, _) = state(SYSTEM)?;
    run(
        &mut s,
        "1",
        r##"
for _, f in ipairs({"get_console", "device", "app_ready", "editor", "log", "error_log", "flush",
    "prefetch", "verify_if_thread_is_running", "xrRender_test_r2_hw",
    "script_server_object_version", "IsGameTypeSingle"}) do
  eq(type(_G[f]), "function", f)
end
local class_mt = getmetatable(DLL_Pure)
for _, c in ipairs({"CConsole", "render_device", "DLL_Pure", "ISheduled", "IRenderable",
    "ICollidable", "IRender_Visual", "CKinematicsAnimated", "CBlend"}) do
  eq(getmetatable(_G[c]) == class_mt, true, c .. " is a class")
end
eq(rawequal(get_console(), get_console()), true, "the same console object")
eq(rawequal(device(), device()), true, "the same device object")
eq(rawequal(get_console(), device()), false, "two objects")
eq(CBlend.anything, nil, "CBlend has no members")
"##,
    )
}

/// Section 2: `execute`, `execute_script`, `show`, `hide`, the getters.
pub fn console() -> Result<(), String> {
    let (mut s, services) = state(SYSTEM)?;
    let on_off = command_of(|k| matches!(k, Kind::OnOff));
    let int = command_of(|k| matches!(k, Kind::Int { min, max } if *min <= 1 && *max >= 3));
    let real = command_of(|k| matches!(k, Kind::Real { .. }));

    // 2.2: one line, no echo, the collapse of spaces, unknown names.
    exec(
        &mut s,
        &format!(
            "get_console():execute('\t{on_off}   on') get_console():execute('no_such_command x')"
        ),
    )?;
    {
        let services = services.borrow();
        let console = &services.console;
        ensure(console.on(on_off), || format!("{on_off} wasn't set"))?;
        let log = console.log().join("\n");
        ensure(!log.contains("~ "), || format!("an echo: {log}"))?;
        ensure(log.contains("! Unknown command:  no_such_command"), || {
            format!("no unknown command: {log}")
        })?;
    }
    // Cut to 1023 bytes: the argument's last byte is past them.
    let pad = 1023 - format!("{int} ").len() - 1;
    exec(
        &mut s,
        &format!("get_console():execute('{int} ' .. string.rep(' ', {pad}) .. '1' .. '2')"),
    )?;
    let value = services.borrow().console.value(int);
    ensure(
        value == Some(volya_engine_core::console::Value::Int(1)),
        || format!("{int} after a cut line: {value:?}"),
    )?;

    // 2.3: `cfg_load <name>`.
    services.borrow_mut().console.take_actions();
    exec(&mut s, "get_console():execute_script('user.ltx')")?;
    let actions = services.borrow_mut().console.take_actions();
    ensure(
        actions
            == [ActionLine {
                name: "cfg_load",
                argument: "user.ltx".into(),
            }],
        || format!("execute_script: {actions:?}"),
    )?;
    let longest = "a".repeat(109);
    exec(
        &mut s,
        &format!("get_console():execute_script('{longest}')"),
    )?;
    fatal(
        "get_console():execute_script(string.rep('a', 110))",
        &["error handler is invoked!"],
    )?;

    // 2.4.
    exec(
        &mut s,
        "get_console():show() get_console():hide() get_console():show()",
    )?;
    let window = services.borrow_mut().take_console_window();
    ensure(window == [true, false, true], || {
        format!("show and hide: {window:?}")
    })?;

    // 2.5.
    exec(&mut s, &format!("get_console():execute('{int} 3')"))?;
    let status = services.borrow().console.status(on_off).unwrap_or_default();
    let real_value = services.borrow().console.number(real);
    run(
        &mut s,
        "2.5",
        &format!(
            r##"
local c = get_console()
eq(c:get_string("no_such_command"), nil, "get_string, not found")
eq(c:get_token("no_such_command"), nil, "get_token, not found")
eq(c:get_string("{on_off}"), "{status}", "get_string")
eq(c:get_token("{on_off}"), "{status}", "get_token")
eq(c:get_string("cfg_load"), "", "get_string of an action")
eq(c:get_integer("no_such_command"), 0, "get_integer, not found")
eq(c:get_integer("{int}"), 3, "get_integer of an integer")
eq(c:get_integer("{on_off}"), 1, "get_integer of an on/off")
eq(c:get_integer(string.upper("{int}")), 0, "case-sensitive")
eq(c:get_integer(" {int}"), 0, "not trimmed")
eq(c:get_float("no_such_command"), 0, "get_float, not found")
eq(c:get_float("{real}"), {real_value:?}, "get_float")
eq(c:get_bool("{on_off}"), true, "get_bool of an on/off")
eq(c:get_bool("{int}"), true, "get_bool of an integer")
-- Indeterminate in the original; our choice.
eq(c:get_bool("no_such_command"), false, "get_bool, not found")
c:execute("{on_off} off")
eq(c:get_bool("{on_off}"), false, "get_bool, off")
eq(c:get_integer("{on_off}"), 0, "get_integer, off")
"##,
            real_value = f64::from(real_value),
        ),
    )?;
    fatal(
        &format!("get_console():get_integer('{real}')"),
        &["assertion failed", "cm"],
    )?;
    fatal(
        &format!("get_console():get_float('{on_off}')"),
        &["fatal error"],
    )?;
    fatal(
        &format!("get_console():get_bool('{real}')"),
        &["fatal error"],
    )
}

/// Section 3: the fields, the times, precaching and the pause.
pub fn device() -> Result<(), String> {
    let (mut s, services) = state(SYSTEM)?;
    let read = |s: &mut Scripts, field: &str| -> Result<Value, String> {
        Ok(exec(s, &format!("return device().{field}"))?.remove(0))
    };
    let number = |v: f64| Value::Number(v);

    ensure(read(&mut s, "frame")? == number(0.0), || {
        "frame before the first".into()
    })?;
    ensure(
        exec(&mut s, "return device():time_global()")? == [number(0.0)],
        || "time_global at the start".into(),
    )?;

    // 3.2: the frame size and the precache phase of a reset.
    services.borrow_mut().device.reset(1024, 768);
    for (field, want) in [
        ("width", 1024.0),
        ("height", 768.0),
        ("precache_frame", 20.0),
    ] {
        let got = read(&mut s, field)?;
        ensure(got == number(want), || format!("{field}: {got:?}"))?;
    }

    // 3.3: frames and times.
    services.borrow_mut().device.frame(16, 0.016, false);
    let f1 = 0.9f32 * 0.016 + 0.1 * 0.0;
    for (field, want) in [
        ("frame", 1.0),
        ("time_delta", 16.0),
        ("f_time_delta", f64::from(f1)),
    ] {
        let got = read(&mut s, field)?;
        ensure(got == number(want), || {
            format!("{field}: {got:?}, want {want}")
        })?;
    }
    services.borrow_mut().device.frame(1016, 1.0, false);
    let got = exec(
        &mut s,
        "return device():time_global(), device().time_delta, device().f_time_delta",
    )?;
    ensure(
        got == [number(1016.0), number(1000.0), number(f64::from(0.1f32))],
        || format!("a long frame: {got:?}"),
    )?;
    services.borrow_mut().device.frame(1016, 0.5, true);
    ensure(read(&mut s, "f_time_delta")? == number(0.0), || {
        "paused: f_time_delta".into()
    })?;
    services.borrow_mut().device.constant_step = true;
    services.borrow_mut().device.frame(5000, 3.0, false);
    let got = exec(
        &mut s,
        "return device():time_global(), device().time_delta, device().f_time_delta",
    )?;
    ensure(
        got == [number(1036.0), number(20.0), number(f64::from(0.02f32))],
        || format!("constant step: {got:?}"),
    )?;

    // 3.4: three frames done of 20; the directions while precaching.
    for _ in 0..3 {
        services.borrow_mut().device.end_frame();
    }
    ensure(read(&mut s, "precache_frame")? == number(17.0), || {
        "precache_frame".into()
    })?;
    services.borrow_mut().device.apply_camera(Camera {
        pos: [1.0, 2.0, 3.0],
        dir: [0.0, 0.0, 9.0],
        top: [0.0, 9.0, 0.0],
        right: [9.0, 0.0, 0.0],
        fov: 67.5,
        aspect_ratio: 0.75,
    });
    run(
        &mut s,
        "3.4",
        r##"
local d = device()
eq(d.cam_pos.x + d.cam_pos.y + d.cam_pos.z, 6, "cam_pos is applied")
eq(d.cam_top.x, 0, "cam_top.x") eq(d.cam_top.y, 1, "cam_top.y") eq(d.cam_top.z, 0, "cam_top.z")
local dir, right = d.cam_dir, d.cam_right
eq(dir.y, 0, "cam_dir is horizontal")
eq(math.abs(dir.x * dir.x + dir.z * dir.z - 1) < 1e-6, true, "cam_dir is a unit vector")
eq(right.x, dir.z, "cam_right = cam_top x cam_dir, x") eq(right.z, -dir.x, "z")
eq(d.fov, 67.5, "fov") eq(d.aspect_ratio, 0.75, "aspect_ratio")
local p = d.cam_pos
p.x = 100
eq(d.cam_pos.x, 1, "a vector read is a copy")
"##,
    )?;
    while services.borrow_mut().device.end_frame() {}
    ensure(!services.borrow().device.precaching(), || {
        "the phase ends".into()
    })?;
    services.borrow_mut().device.frame(5000, 0.0, false);
    services.borrow_mut().device.apply_camera(Camera::default());
    ensure(
        services.borrow().device.camera() == Camera::default(),
        || "after the phase, the camera's directions apply".into(),
    )?;

    // 3.5: the pause.
    run(
        &mut s,
        "3.5",
        r##"
local d = device()
eq(d:is_paused(), false, "not paused at first")
d:pause(true)
eq(d:is_paused(), true, "pause(true)")
d:pause(true)
d:pause(false)
eq(d:is_paused(), false, "pause(false), not nested")
"##,
    )?;
    ensure(services.borrow().pause_caption, || {
        "the caption is enabled".into()
    })?;
    {
        let mut services = services.borrow_mut();
        services.pause_caption = false;
        services.game_started("all/deathmatch/alife/new");
    }
    exec(&mut s, "device():pause(true)")?;
    ensure(
        services.borrow().pause_caption && !services.borrow().pause.is_on(),
        || "multiplayer: only the caption".into(),
    )?;
    {
        let mut services = services.borrow_mut();
        services.game_started("all/single/alife/new");
        services.pause_caption = false;
        services.benchmark = true;
    }
    exec(&mut s, "device():pause(true)")?;
    ensure(
        !services.borrow().pause_caption && !services.borrow().pause.is_on(),
        || "benchmark: nothing".into(),
    )
}

/// Section 4: the global functions.
pub fn functions() -> Result<(), String> {
    let (mut s, services) = state(SYSTEM)?;
    let ready = |s: &mut Scripts| exec(s, "return app_ready()");
    // 4.1.
    ensure(ready(&mut s)? == [false.into()], || {
        "app_ready before the first level".into()
    })?;
    let runs = services.borrow().console.executions("stat_memory");
    services.borrow_mut().open_loading_phase();
    services.borrow_mut().open_loading_phase();
    services.borrow_mut().close_loading_phase();
    ensure(ready(&mut s)? == [false.into()], || {
        "a phase still open".into()
    })?;
    services.borrow_mut().close_loading_phase();
    ensure(ready(&mut s)? == [true.into()], || {
        "the last phase closed".into()
    })?;
    let ran = services.borrow().console.executions("stat_memory") - runs;
    ensure(ran == 1, || format!("stat_memory ran {ran} times"))?;
    services.borrow_mut().open_loading_phase();
    ensure(ready(&mut s)? == [false.into()], || "a phase opened".into())?;
    services.borrow_mut().close_loading_phase();

    // 4.5.
    for (option, single, kind) in [
        ("all/single/alife/new", true, GameType::Single),
        ("all/DeathMatch", false, GameType::Deathmatch),
        ("all/teamdeathmatch", false, GameType::TeamDeathmatch),
        ("all/artefacthunt", false, GameType::ArtefactHunt),
        ("all/singles", true, GameType::None),
        ("single", true, GameType::None),
    ] {
        services.borrow_mut().game_started(option);
        let got = exec(&mut s, "return IsGameTypeSingle()")?;
        ensure(
            got == [single.into()] && services.borrow().game_type == kind,
            || format!("{option}: {got:?}"),
        )?;
    }

    run(
        &mut s,
        "4",
        r##"
eq(editor(), false, "editor")
eq(select("#", log("x")), 0, "log returns nothing")
eq(select("#", error_log("x")), 0, "error_log returns nothing")
eq(select("#", flush()), 0, "flush returns nothing")
eq(select("#", verify_if_thread_is_running()), 0, "verify_if_thread_is_running")
eq(select("#", prefetch("_G")), 0, "prefetch returns nothing")
eq(xrRender_test_r2_hw(), false, "no R2 renderer")
eq(script_server_object_version(), 7, "script_server_object_version")
eq(script_server_object_version(), 7, "read once")
"##,
    )?;
    let flushes = services.borrow().console.executions("flush");
    ensure(flushes == 0, || "flush() isn't the console's".into())?;
    // 4.7: a missing key is fatal.
    let (mut s, _) = state("[script]\r\n")?;
    let result = s.exec("=missing", b"return script_server_object_version()");
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("a missing key: {result:?}"),
    )
}

fn motion_set(definitions: &[(&str, u16, u16, f32)]) -> MotionSet {
    MotionSet {
        params: MotionParams {
            version: 3,
            partitions: Vec::new(),
            definitions: definitions
                .iter()
                .enumerate()
                .map(|(k, &(name, flags, partition, falloff))| MotionDefinition {
                    name: name.into(),
                    flags,
                    bone_or_partition: partition,
                    motion_index: k as u16,
                    speed: 1.0,
                    power: 1.0,
                    accrue: 2.0,
                    falloff,
                })
                .collect(),
        },
        key_counts: vec![10; definitions.len()],
    }
}

/// Section 5: the classes.
pub fn classes() -> Result<(), String> {
    let (mut s, _) = state(SYSTEM)?;
    let sets = vec![
        motion_set(&[("idle", 0, 0xFFFF, 3.0), ("walk", 2, 0, 4.0)]),
        motion_set(&[("fx_hit", 1, 0, 1.0), ("walk", 0, 1, 5.0)]),
    ];
    let animated = s.new_visual(Some(sets)).map_err(|e| e.message)?;
    let plain = s.new_visual(None).map_err(|e| e.message)?;
    let lua = s.lua_mut();
    lua.register_object_function("test_visual", move |_| Ok(Some(animated)))
        .map_err(|e| e.message)?;
    lua.register_object_function("test_plain_visual", move |_| Ok(Some(plain)))
        .map_err(|e| e.message)?;
    run(
        &mut s,
        "5",
        r##"
class "pure" (DLL_Pure)
function pure:__init() super() end
local o = pure()
eq(rawequal(o:_construct(), o), true, "the engine's _construct returns the object")
class "collidable" (ICollidable)
function collidable:__init() super() end
eq(collidable().__ok, true, "ICollidable has a constructor")
eq(ISheduled.anything, nil, "ISheduled has no members")
class "visual" (IRender_Visual)
function visual:__init() super() end
eq(visual():dcast_PKinematicsAnimated(), nil, "a visual of its own: no motions")
eq(test_plain_visual():dcast_PKinematicsAnimated(), nil, "another visual")
local k = test_visual():dcast_PKinematicsAnimated()
eq(getmetatable(k) ~= nil, true, "a skeletal visual with motions")
eq(rawequal(k, test_visual():dcast_PKinematicsAnimated()), true, "the same object")
eq(select("#", k:PlayCycle("idle")), 0, "PlayCycle returns nothing")
k:PlayCycle("walk")
"##,
    )?;
    let kinematics = s
        .with_part(animated, |v: &mut RenderVisual| v.animated)
        .flatten()
        .ok_or("the visual has no CKinematicsAnimated")?;
    let blends = s
        .with_part(kinematics, |k: &mut KinematicsAnimated| k.blends.clone())
        .unwrap_or_default();
    let summary: Vec<_> = blends
        .iter()
        .map(|b| (b.cycle.as_str(), b.set, b.partition, b.fading))
        .collect();
    let want = [
        ("idle", 0, 0, None),
        ("idle", 0, 1, Some(5.0)),
        ("idle", 0, 2, None),
        ("idle", 0, 3, None),
        ("walk", 1, 1, None),
    ];
    ensure(summary == want, || format!("playbacks: {summary:?}"))?;
    ensure(!blends[4].stops_at_end && blends[0].falloff == 3.0, || {
        format!("parameters: {blends:?}")
    })?;
    // Case-sensitive, fx motions aren't cycles.
    for name in ["WALK", "fx_hit"] {
        let code = format!("test_visual():dcast_PKinematicsAnimated():PlayCycle('{name}')");
        let (mut s, services) = state(SYSTEM)?;
        let part = s
            .new_visual(Some(vec![motion_set(&[
                ("walk", 0, 0, 1.0),
                ("fx_hit", 1, 0, 1.0),
            ])]))
            .map_err(|e| e.message)?;
        s.lua_mut()
            .register_object_function("test_visual", move |_| Ok(Some(part)))
            .map_err(|e| e.message)?;
        let result = s.exec("=play", format!("pcall(function() {code} end)").as_bytes());
        ensure(
            matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
            || format!("{name}: {result:?}"),
        )?;
        let log = services.borrow().console.log().join("\n");
        for row in [
            "motion_ID.valid()",
            "! MODEL: can't find cycle: ",
            &format!("Arguments     : {name}"),
        ] {
            ensure(log.contains(row), || {
                format!("{name}: no {row:?} in\n{log}")
            })?;
        }
    }
    Ok(())
}
