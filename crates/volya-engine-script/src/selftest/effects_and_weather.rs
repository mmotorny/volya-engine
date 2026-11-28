// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-effects-and-weather.md on every target, one
//! check per section, over a state with the game object basics, the player
//! and files of our own: animation files, weather sections and a colour
//! animation library.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;

use super::ensure;
use super::files::Tree;
use super::game_object::{PLAYER, data};
use super::services::run;
use crate::effects_and_weather::{EffectorParams, EffectsAndWeather};
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass};
use crate::loader::{Prefix, Scripts};
use crate::services::EngineServices;
use crate::values::Color;
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 effector", effector),
    ("2.4 the player's manager", manager),
    ("3 effector_params", effector_params),
    (
        "4 – 6 animated, complex and camera effects",
        effect_functions,
    ),
    ("7 weather", weather),
    ("8 color_animator", color_animator),
    ("9, 10 particles", particles),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| {
            f().err()
                .map(|e| format!("effects and weather {what}: {e}"))
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const FSGAME: &[u8] = b"$game_data$ = false| true | $fs_root$ | gamedata\\\r\n\
    $game_scripts$ = true | false| $game_data$ | scripts\\\r\n\
    $game_config$ = true | false| $game_data$ | config\\\r\n\
    $game_anims$ = true | false| $game_data$ | anims\\\r\n";

const SYSTEM: &[u8] = b"[weathers]\r\ndefault = default_cycle\r\n\
    [weather_effects]\r\nsurge_day = surge_cycle\r\n\
    [complex_pp]\r\npp_eff_name = blink.ppe\r\npp_eff_cyclic = true\r\n\
    [complex_cam]\r\ncam_eff_name = camera_effects\\earthquake.anm\r\ncam_eff_cyclic = false\r\n\
    [complex_none]\r\nx = 1\r\n\
    [complex_no_flag]\r\npp_eff_name = blink.ppe\r\n";

/// A chunk of `chunks` (docs/formats/chunks.md): id, size, payload.
fn chunk(id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = id.to_le_bytes().to_vec();
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// A colour animation library (CA 2) of one entry, `ui_slow_blinking_alpha`
/// of CA 3.4 (30 fps, 30 frames) and `levels\psy_antenna\starting` (5 fps,
/// 100 frames, one key).
fn library() -> Vec<u8> {
    let entry = |name: &str, fps: f32, frames: u32, keys: &[(i32, u32)]| {
        let mut common = name.as_bytes().to_vec();
        common.push(0);
        common.extend_from_slice(&fps.to_le_bytes());
        common.extend_from_slice(&frames.to_le_bytes());
        let mut k = (keys.len() as i32).to_le_bytes().to_vec();
        for (f, v) in keys {
            k.extend_from_slice(&f.to_le_bytes());
            k.extend_from_slice(&v.to_le_bytes());
        }
        [chunk(1, &common), chunk(2, &k)].concat()
    };
    let list = [
        chunk(
            0,
            &entry(
                "ui_slow_blinking_alpha",
                30.0,
                30,
                &[(0, 0xFFFF_FFFF), (15, 0x48FF_FFFF), (28, 0xFFFF_FFFF)],
            ),
        ),
        chunk(
            1,
            &entry(
                "levels\\psy_antenna\\starting",
                5.0,
                100,
                &[(0, 0x8040_20FF)],
            ),
        ),
    ]
    .concat();
    [chunk(0, &1u16.to_le_bytes()), chunk(1, &list)].concat()
}

struct State {
    s: Scripts,
    services: Rc<RefCell<EngineServices>>,
    state: Rc<RefCell<EffectsAndWeather>>,
}

/// The state: the game object basics and script API U registered, `_G`
/// loaded, the player spawned when `player`.
fn state(player: bool) -> Result<State, String> {
    let files: [(&str, &[u8]); 5] = [
        ("gamedata/scripts/_g.script", b""),
        ("gamedata/anims/blink.ppe", b""),
        ("gamedata/anims/blink.anm", b""),
        ("gamedata/anims/camera_effects/earthquake.anm", b""),
        ("gamedata/lanims.xr", &library()),
    ];
    let tree = Tree(
        (files.iter())
            .map(|(p, t)| ((*p).to_owned(), t.to_vec()))
            .collect(),
    );
    let tree = Rc::new(tree);
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(|e| e.message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(SYSTEM).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(|e| e.message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(objects, Rc::clone(&services), files)
        .map_err(|e| e.message)?;
    let state = Rc::new(RefCell::new(EffectsAndWeather::new()));
    s.register_effects_and_weather(Rc::clone(&state))
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    if player {
        s.spawn_engine_object(
            ObjectClass::Player,
            data(PLAYER, "actor", "actor", "O_ACTOR"),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(State { s, services, state })
}

/// `code` must stop the state with a fatal error whose report includes
/// `row`.
fn fatal(player: bool, code: &str, row: &str) -> Result<(), String> {
    let mut t = state(player)?;
    let result =
        t.s.exec("=fatal", format!("pcall(function() {code} end)").as_bytes());
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: {result:?}, not fatal"),
    )?;
    let log = t.services.borrow().console.log().join("\n");
    ensure(log.contains(row), || {
        format!("{code}: no {row:?} in the log:\n{log}")
    })
}

/// 1.1, 1.2: the classes and functions registered.
pub fn registered() -> Result<(), String> {
    let mut t = state(false)?;
    run(
        &mut t.s,
        "1",
        r#"
for _, c in ipairs({"effector", "effector_params", "color_animator", "particles_object",
    "particle_params", "environment", "environment_descriptor"}) do
  eq(type(_G[c]), "userdata", c .. " is a class")
end
for _, f in ipairs({"add_pp_effector", "remove_pp_effector", "set_pp_effector_factor",
    "add_complex_effector", "remove_complex_effector", "add_cam_effector",
    "add_cam_effector2", "remove_cam_effector", "set_weather", "get_weather",
    "set_weather_fx", "is_wfx_playing", "rain_factor", "environment"}) do
  eq(type(level[f]), "function", "level." .. f)
end
eq(__volya_effects_and_weather, nil, "the Rust side is hidden")
eq(__volya_environment, nil, "hidden")
raises(function() return environment() end, "", "no constructor")
"#,
    )
}

/// 2.1 – 2.3: a script class deriving from `effector`, started, processed
/// and finished.
pub fn effector() -> Result<(), String> {
    let mut t = state(true)?;
    run(
        &mut t.s,
        "2",
        r#"
class "pp" (effector)
function pp:__init(n, life, keep) super(n, life) self.keep = keep self.calls = 0 end
function pp:process(p)
  effector.process(self, p)
  self.calls = self.calls + 1
  p.gray = 0.25
  local c = color() c.r = 1 c.g = 0.5 c.b = 0.5
  p.color_base = c
  return self.keep
end
e = pp(1001, 10, true)
e:start()
gone = pp(1002, 10, false)
gone:start()
dead = pp(1003, 0, true)
dead:start()
"#,
    )?;
    ensure(t.state.borrow().screen_effects().len() == 3, || {
        "three effects started".into()
    })?;
    t.s.process_screen_effects().map_err(|e| e.to_string())?;
    let set = t.state.borrow().screen_effect_set();
    // e's values minus the identity's (2.4 step 3): the zeros it leaves
    // subtract the identity's (noise grain 1, fps 30, colour_gray 0.333).
    let mut want = EffectorParams::ZERO;
    want.gray = 0.25;
    want.color_base = Color {
        r: 1.0,
        g: 0.5,
        b: 0.5,
    };
    ensure(set == want, || format!("the set: {set:?}"))?;
    let numbers: Vec<i32> = t
        .state
        .borrow()
        .screen_effects()
        .iter()
        .map(|e| e.number)
        .collect();
    ensure(numbers == [1001], || {
        format!("false and lifeless removed: {numbers:?}")
    })?;
    run(
        &mut t.s,
        "2.2",
        r#"
eq(e.calls, 1, "processed once")
eq(dead.calls, 0, "not processed")
again = pp(1001, 10, true)
again:start()
"#,
    )?;
    ensure(t.state.borrow().screen_effects().len() == 1, || {
        "start replaces its number".into()
    })?;
    run(&mut t.s, "2.2 finish", "again:finish()")?;
    ensure(t.state.borrow().screen_effects().is_empty(), || {
        "finish removes".into()
    })?;
    t.s.process_screen_effects().map_err(|e| e.to_string())?;
    let set = t.state.borrow().screen_effect_set();
    ensure(set == EffectorParams::IDENTITY, || {
        format!("no effect: {set:?}")
    })?;
    fatal(
        false,
        "effector(1, 2):start()",
        "the player's object doesn't exist",
    )
}

/// 2.3, 2.4: the engine's own `process` and a failing override.
pub fn manager() -> Result<(), String> {
    let mut t = state(true)?;
    t.services.borrow_mut().device.frame(0, 0.5, false);
    let dt = t.services.borrow().device.f_time_delta();
    run(
        &mut t.s,
        "2.3",
        r#"
plain = effector(7, 1)
plain:start()
"#,
    )?;
    t.s.process_screen_effects().map_err(|e| e.to_string())?;
    let set = t.state.borrow().screen_effect_set();
    // All ten 0: the identity subtracted.
    ensure(set == EffectorParams::ZERO, || {
        format!("the engine's own: {set:?}")
    })?;
    let part = t.state.borrow().screen_effects()[0].part;
    let life =
        t.s.with_part(part, |e: &mut crate::effects_and_weather::Effector| e.life);
    ensure(life == Some(1.0 - dt), || format!("life {life:?}, dt {dt}"))?;
    run(
        &mut t.s,
        "2.3 failing",
        r#"
class "bad" (effector)
function bad:__init(n, life) super(n, life) end
function bad:process(p) error("broken", 0) end
bad(8, 1):start()
"#,
    )?;
    let failed = t.s.process_screen_effects();
    ensure(
        matches!(&failed, Err(f) if f.error.kind == ErrorKind::Fatal
            && f.error.message == "LUA error: broken"),
        || format!("a failing override: {failed:?}"),
    )
}

/// 3: the record's defaults, conversions and copies.
pub fn effector_params() -> Result<(), String> {
    let mut t = state(false)?;
    run(
        &mut t.s,
        "3",
        r#"
local p = effector_params()
eq(p.blur, 0, "blur")
eq(p.noise.grain, 1, "grain")
eq(p.noise.fps, 10, "fps")
eq(p.color_base.r, 0.5, "color_base")
eq(string.format("%.3f", p.color_gray.g), "0.333", "color_gray")
p.blur = "2"
eq(p.blur, 2, "a numeric string")
p.gray = {}
eq(p.gray, 0, "a non-number gives 0")
p.gray = 0.1
eq(p.gray, 0.10000000149011612, "a 32-bit float")
p.dual.h = 1
eq(p.dual.h, 0, "a copy")
local d = duality() d.h = 0.5 d.v = 0.25
p.dual = d
eq(p.dual.v, 0.25, "assigned")
local q = effector_params()
q:assign(p)
eq(q.dual.h, 0.5, "assign")
eq(q.blur, 2, "assign blur")
"#,
    )
}

/// 4 – 6: the files found and checked, then the issues' errors; the
/// functions acting on effects the manager does not have.
pub fn effect_functions() -> Result<(), String> {
    let mut t = state(true)?;
    run(
        &mut t.s,
        "4 – 6",
        r#"
raises(function() level.add_pp_effector("blink.ppe", 1, false) end, "(#1193)", "animated")
raises(function() level.add_cam_effector("camera_effects\\earthquake.anm", 1, false, "") end,
  "(#1194)", "camera")
raises(function() level.add_cam_effector2("blink.anm", 1, true, "f") end, "(#1194)", "camera 2")
raises(function() level.add_complex_effector("complex_pp", 1) end, "(#1193)", "complex pp")
raises(function() level.add_complex_effector("complex_cam", 1) end, "(#1194)", "complex cam")
eq(level.add_complex_effector("complex_none", 1), nil, "neither key")
eq(level.set_pp_effector_factor(1, 0.5), nil, "no animated effect")
eq(level.set_pp_effector_factor(1, 0.5, 0.3), nil, "with a rate")
eq(level.remove_pp_effector(1), nil, "none to remove")
eq(level.remove_cam_effector(1), nil, "no camera effect")
e = effector(5, 10)
e:start()
level.remove_pp_effector(5)
"#,
    )?;
    ensure(t.state.borrow().screen_effects().len() == 1, || {
        "remove_pp_effector leaves an effector".into()
    })?;
    run(&mut t.s, "5", "level.remove_complex_effector(5)")?;
    ensure(t.state.borrow().screen_effects().is_empty(), || {
        "remove_complex_effector removes the screen effect".into()
    })?;
    fatal(
        true,
        "level.add_pp_effector('missing.ppe', 1, false)",
        "Can't find motion file 'missing.ppe'.",
    )?;
    fatal(
        true,
        "level.add_pp_effector('blink.anm', 1, false)",
        "Can't support files with many animations set",
    )?;
    fatal(
        true,
        "level.add_cam_effector('none.anm', 1, false, '')",
        "Can't find motion file 'none.anm'.",
    )?;
    fatal(
        true,
        "level.add_complex_effector('complex_no_flag', 1)",
        "pp_eff_cyclic",
    )?;
    fatal(
        false,
        "level.remove_pp_effector(1)",
        "the player's object doesn't exist",
    )
}

/// 7: the weather's names checked, the environment's values read.
pub fn weather() -> Result<(), String> {
    let mut t = state(false)?;
    run(
        &mut t.s,
        "7 before",
        r#"
eq(level.get_weather(), nil, "no weather")
eq(level.is_wfx_playing(), false, "no effect")
eq(level.rain_factor(), 0, "no rain")
eq(level.set_weather("rainy", true), nil, "not a cycle")
raises(function() level.set_weather("default", false) end, "(#1195)", "a cycle")
raises(function() level.set_weather_fx("surge_day") end, "(#1195)", "an effect")
"#,
    )?;
    let log = t.services.borrow().console.log().join("\n");
    ensure(log.contains("! Invalid weather name: rainy"), || {
        format!("logged: {log}")
    })?;
    {
        let mut s = t.state.borrow_mut();
        s.environment.name = Some(b"default".to_vec());
        s.environment.rain_density = 0.25;
        s.environment.far_plane = 300.0;
        s.environment.fog_density = 0.5;
    }
    run(
        &mut t.s,
        "7",
        r#"
eq(level.get_weather(), "default", "the current name")
eq(level.rain_factor(), 0.25, "rain")
local d = level.environment():current()
eq(d.far_plane, 300, "far plane")
eq(d.fog_density, 0.5, "fog density")
eq(rawequal(level.environment(), level.environment()), true, "the one environment")
"#,
    )?;
    t.state.borrow_mut().environment.effect_playing = true;
    let r =
        t.s.exec("=t", b"return level.set_weather_fx('x')")
            .map_err(|e| e.message)?;
    ensure(r == [Value::Boolean(false)], || {
        format!("an effect plays: {r:?}")
    })?;
    fatal(false, "level.set_weather('', true)", "! Empty weather name")?;
    fatal(
        false,
        "level.set_weather_fx('')",
        "! Empty weather effect name",
    )?;
    fatal(
        false,
        "level.set_weather_fx('x')",
        "Invalid weather effect name.",
    )
}

/// 8: an entry evaluated as CA 3, its length.
pub fn color_animator() -> Result<(), String> {
    let mut t = state(false)?;
    run(
        &mut t.s,
        "8",
        r#"
local a = color_animator("ui_slow_blinking_alpha")
eq(a:length(), 1000, "30 frames at 30 fps")
local c = a:calculate(0.5)
eq(string.format("%.4f %.4f", c.a, c.r), "0.2824 1.0000", "frame 15")
c = a:calculate(1.1)
eq(string.format("%.4f", c.a), "0.8549", "wrapped: frame 3")
a:load("levels\\psy_antenna\\starting")
eq(a:length(), 20000, "100 frames at 5 fps")
c = a:calculate(3)
eq(string.format("%.4f %.4f %.4f %.4f", c.r, c.g, c.b, c.a), "0.2510 0.1255 1.0000 0.5020",
  "the channels")
"#,
    )?;
    fatal(false, "color_animator('nothing')", "Can't find color anim:")
}

/// 9, 10: `particles_object` raises #1196's error; `particle_params`
/// takes 0 to 3 vectors.
pub fn particles() -> Result<(), String> {
    let mut t = state(false)?;
    run(
        &mut t.s,
        "9, 10",
        r#"
raises(function() return particles_object("x") end, "(#1196)", "particles_object")
eq(type(particle_params()), "userdata", "none")
eq(type(particle_params(vector())), "userdata", "one")
eq(type(particle_params(vector(), vector(), vector())), "userdata", "three")
raises(function() return particle_params(1) end, "", "not a vector")
"#,
    )
}
