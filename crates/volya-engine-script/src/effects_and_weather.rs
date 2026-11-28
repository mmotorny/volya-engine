// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Effects and weather as scripts reach them (script API U,
//! docs/behavior/script-api-effects-and-weather.md; section numbers are
//! its): the engine class `effector` and the player's manager's screen
//! effects (2), the record `effector_params` (3), the `level` functions of
//! animated screen effects, complex effects and camera effects (4, 5, 6),
//! the weather's `level` functions and the two environment classes
//! ([`weather`], 7), and the classes `color_animator`, `particles_object`
//! and `particle_params` ([`objects`], 8, 9, 10).
//!
//! What the engine gives is [`EffectsAndWeather`]: the player's manager's
//! list of screen effects, which [`Scripts::process_screen_effects`]
//! processes each camera update into the set of frame-presentation.md 2.2
//! steps 1 – 4 ([`EffectsAndWeather::screen_effect_set`]), and the
//! environment as the level blends it ([`Environment`]). Where the engine
//! lacks the effect itself, the function checks what the spec states
//! before it and then raises a script error naming the issue that adds it:
//! animated screen effects (#1193), camera effects (#1194), weather
//! switching (#1195) and particle objects (#1196). The functions are set
//! on `level` by the Lua side (`effects_and_weather.lua`) over one hidden
//! Rust function. See docs/architecture/scripts.md, Effects and weather.

pub mod objects;
pub mod weather;

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::LEVEL;
use volya_engine_formats::cp1251;
use volya_engine_formats::lanims::ColourAnimations;

use crate::classes::{Conversion, EngineClass, EnginePart, ObjectMaker, OverrideFailure};
use crate::files::FileSystem;
use crate::game_object::{GameObjects, Kinds};
use crate::loader::Scripts;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::values::{Color, Duality, Noise, lua_number, to_f32, to_flags32};
use crate::{Error, ErrorKind, Value};

/// The alias of the game's animation folder (4.1, 6.1).
pub const GAME_ANIMS: &str = "$game_anims$";

/// The error of a function whose effect the engine lacks: what, and the
/// issue that adds it.
fn not_implemented(function: &str, what: &str, issue: u32) -> String {
    format!("{function}: {what} are not implemented (#{issue})")
}

/// The issue adding animated screen effects (4).
pub const ANIMATED_EFFECTS_ISSUE: u32 = 1193;
/// The issue adding camera effects (6).
pub const CAMERA_EFFECTS_ISSUE: u32 = 1194;
/// The issue adding weather switching (7.1 – 7.3).
pub const WEATHER_SWITCHING_ISSUE: u32 = 1195;
/// The issue adding particle objects (9).
pub const PARTICLE_OBJECTS_ISSUE: u32 = 1196;

/// The ten screen-effect values (3; frame-presentation.md 2.1), as
/// `effector_params` holds them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectorParams {
    pub blur: f32,
    pub gray: f32,
    pub dual: Duality,
    pub noise: Noise,
    pub color_base: Color,
    pub color_gray: Color,
    pub color_add: Color,
}

const fn color(v: f32) -> Color {
    Color { r: v, g: v, b: v }
}

impl EffectorParams {
    /// All ten values 0: an effect's values before its processing (2.3,
    /// 2.4 step 1).
    pub const ZERO: Self = Self {
        blur: 0.0,
        gray: 0.0,
        dual: Duality { h: 0.0, v: 0.0 },
        noise: Noise {
            intensity: 0.0,
            grain: 0.0,
            fps: 0.0,
        },
        color_base: color(0.0),
        color_gray: color(0.0),
        color_add: color(0.0),
    };

    /// The engine's default record, what `effector_params()` holds (3).
    pub const DEFAULT: Self = Self {
        noise: Noise {
            intensity: 0.0,
            grain: 1.0,
            fps: 10.0,
        },
        color_base: color(0.5),
        color_gray: color(0.333),
        ..Self::ZERO
    };

    /// The identity (frame-presentation.md 2.1): the default record with
    /// noise.fps 30.
    pub const IDENTITY: Self = Self {
        noise: Noise {
            intensity: 0.0,
            grain: 1.0,
            fps: 30.0,
        },
        ..Self::DEFAULT
    };

    /// The values in a fixed order: blur, gray, dual, noise, the colours.
    fn values(&self) -> [f32; 16] {
        let c = |c: &Color| [c.r, c.g, c.b];
        let [r0, g0, b0] = c(&self.color_base);
        let [r1, g1, b1] = c(&self.color_gray);
        let [r2, g2, b2] = c(&self.color_add);
        [
            self.blur,
            self.gray,
            self.dual.h,
            self.dual.v,
            self.noise.intensity,
            self.noise.grain,
            self.noise.fps,
            r0,
            g0,
            b0,
            r1,
            g1,
            b1,
            r2,
            g2,
            b2,
        ]
    }

    fn from_values(v: [f32; 16]) -> Self {
        Self {
            blur: v[0],
            gray: v[1],
            dual: Duality { h: v[2], v: v[3] },
            noise: Noise {
                intensity: v[4],
                grain: v[5],
                fps: v[6],
            },
            color_base: Color {
                r: v[7],
                g: v[8],
                b: v[9],
            },
            color_gray: Color {
                r: v[10],
                g: v[11],
                b: v[12],
            },
            color_add: Color {
                r: v[13],
                g: v[14],
                b: v[15],
            },
        }
    }

    /// `self` + (`effect` − identity), value by value (2.4 step 3).
    fn add_difference(&self, effect: &Self) -> Self {
        let (s, e, i) = (self.values(), effect.values(), Self::IDENTITY.values());
        Self::from_values(std::array::from_fn(|k| s[k] + (e[k] - i[k])))
    }
}

/// An `effector`'s engine part (2.1): its number and remaining life.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effector {
    pub number: i32,
    /// Seconds.
    pub life: f32,
}

/// One of the player's manager's screen effects (2.4): an `effector` (2),
/// by its engine part. Animated effects (4) are #1193's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenEffect {
    pub number: i32,
    pub part: EnginePart,
}

/// The environment's values scripts read (7.4 – 7.6), as the level blends
/// them each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Environment {
    /// The current name (7.4): the cycle in force; `None` before any.
    pub name: Option<Vec<u8>>,
    /// Whether a weather effect plays (7.4). Nothing starts one (#1195).
    pub effect_playing: bool,
    /// The blended rain density ρ of the last frame (7.5).
    pub rain_density: f32,
    /// The blended far plane and fog density of the last frame (7.6).
    pub far_plane: f32,
    pub fog_density: f32,
}

/// What script API U keeps: the player's manager's screen effects and
/// their set, the environment, and what the functions read.
pub struct EffectsAndWeather {
    /// The player's manager's screen effects, in order (2.4).
    screen_effects: Vec<ScreenEffect>,
    /// The set of the last processing (frame-presentation.md 2.2 steps
    /// 1 – 4).
    set: EffectorParams,
    /// The environment as the engine last set it.
    pub environment: Environment,
    /// The running level's folder, `$level$` (4.1, 6.1); the engine sets
    /// it.
    pub level_dir: Option<String>,
    /// The colour animation library (8), read at first use.
    animations: Option<Rc<ColourAnimations>>,
}

impl Default for EffectsAndWeather {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectsAndWeather {
    pub fn new() -> Self {
        EffectsAndWeather {
            screen_effects: Vec::new(),
            set: EffectorParams::IDENTITY,
            environment: Environment::default(),
            level_dir: None,
            animations: None,
        }
    }

    /// The player's manager's screen effects, in order.
    pub fn screen_effects(&self) -> &[ScreenEffect] {
        &self.screen_effects
    }

    /// The set the last processing combined (the identity before any): for
    /// the renderer (frame-presentation.md 2.2 step 5, #1197).
    pub fn screen_effect_set(&self) -> EffectorParams {
        self.set
    }

    /// The level ended: the player's manager goes with the player, its
    /// effects with it (the effectors are never deleted, 2.1).
    pub fn level_ended(&mut self) {
        self.screen_effects.clear();
        self.set = EffectorParams::IDENTITY;
        self.level_dir = None;
    }

    /// Removes the first screen effect of number `n` (2.4).
    fn remove_first(&mut self, n: i32) {
        if let Some(i) = self.screen_effects.iter().position(|e| e.number == n) {
            self.screen_effects.remove(i);
        }
    }

    /// Adds `effect`, first removing the first of its number (2.4).
    fn add(&mut self, effect: ScreenEffect) {
        self.remove_first(effect.number);
        self.screen_effects.push(effect);
    }
}

/// What the functions share.
#[derive(Clone)]
pub(crate) struct Context {
    state: Rc<RefCell<EffectsAndWeather>>,
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
    files: Rc<RefCell<FileSystem>>,
    fatal: Rc<RefCell<Option<String>>>,
}

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error with `description` and `arguments`; the error to
    /// raise.
    fn fatal(
        &self,
        function: &str,
        line: u32,
        expression: &str,
        description: &str,
        arguments: &[&str],
    ) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression,
                description,
                arguments,
            },
        )
    }

    /// The player's manager, which every effect function reaches through
    /// the player's object without testing it (conventions): without it,
    /// a fatal error standing for the believed crash (our wording).
    fn player(&self, function: &str) -> Result<(), String> {
        let exists = (self.objects.borrow().iter()).any(|o| o.kinds.has(Kinds::PLAYER));
        if exists {
            return Ok(());
        }
        Err(self.fatal(
            function,
            line!(),
            "fatal error",
            "<no expression>",
            &[&format!("{function}: the player's object doesn't exist")],
        ))
    }

    /// The device's frame time *dt*, seconds (conventions; B 3.3).
    fn dt(&self) -> f32 {
        self.services.borrow().device.f_time_delta()
    }

    /// Animation file `file` found in `$level$`, then `$game_anims$` (4.1
    /// step 1, 6.1 step 1); fatal `Can't find motion file '%s'.` when
    /// neither has it.
    fn motion_file(&self, function: &str, file: &[u8]) -> Result<String, String> {
        let name = cp1251::decode(file);
        let level_dir = self.state.borrow().level_dir.clone();
        let files = self.files.borrow();
        let mut aliases = files.aliases.clone();
        let mut folders = Vec::new();
        if let Some(dir) = &level_dir {
            aliases.set_dir(LEVEL, dir);
            folders.push(LEVEL);
        }
        folders.push(GAME_ANIMS);
        let found = folders.into_iter().find_map(|alias| {
            let path = aliases.path(alias, &name).ok()?;
            files.index.entry(&path).map(|_| path)
        });
        drop(files);
        found.ok_or_else(|| {
            self.fatal(
                function,
                line!(),
                "fatal error",
                "<no expression>",
                &[&format!("Can't find motion file '{name}'.")],
            )
        })
    }

    /// An animated effect from `file` (4.1): the file found, its name
    /// checked (step 2), then #1193's error: its curves are `ppe-format`'s.
    fn animated_effect(&self, function: &str, file: &[u8]) -> String {
        if let Err(e) = self.motion_file(function, file) {
            return e;
        }
        if let Some(dot) = file.iter().rposition(|&b| b == b'.')
            && &file[dot..] != b".ppe"
        {
            return self.fatal(
                function,
                line!(),
                "fatal error",
                "<no expression>",
                &["ERROR: Can't support files with many animations set. Incorrect file."],
            );
        }
        not_implemented(function, "animated screen effects", ANIMATED_EFFECTS_ISSUE)
    }

    /// A camera effect from `file` (6.1): the file found, then #1194's
    /// error: the motion format is unestablished.
    fn camera_effect(&self, function: &str, file: &[u8]) -> String {
        match self.motion_file(function, file) {
            Err(e) => e,
            Ok(_) => not_implemented(function, "camera effects", CAMERA_EFFECTS_ISSUE),
        }
    }
}

type Results = Result<Vec<Value>, String>;

pub(crate) fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// A number argument (our wording for a mismatch; depends on A).
pub(crate) fn number(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

/// A number argument made an integer (conventions; T's).
pub(crate) fn integer(args: &[Value], i: usize, what: &str) -> Result<i32, String> {
    number(args, i, what).map(|n| to_flags32(n) as i32)
}

pub(crate) fn text(args: &[Value], i: usize, what: &str) -> Result<Vec<u8>, String> {
    match arg(args, i) {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("{what}: text expected")),
    }
}

pub(crate) fn boolean(args: &[Value], i: usize, what: &str) -> Result<bool, String> {
    match arg(args, i) {
        Value::Boolean(b) => Ok(*b),
        _ => Err(format!("{what}: boolean expected")),
    }
}

/// `args` must be `count` long (our wording for a call no form matches;
/// A 7.3 gives it no message).
fn forms(args: &[Value], counts: &[usize], what: &str) -> Result<(), String> {
    if counts.contains(&args.len()) {
        Ok(())
    } else {
        Err(format!("{what}: no form of it takes these arguments"))
    }
}

/// The class `effector` (2): one constructor of two numbers, `process`
/// overridable; `start` and `finish` are the Lua side's, which keeps the
/// object (2.2).
fn effector_class(ctx: &Context) -> EngineClass {
    let c = ctx.clone();
    EngineClass::new("effector")
        .constructor(
            2,
            |args: &[Value]| matches!(args, [Value::Number(_), Value::Number(_)]).then_some(0),
            |_, args: &[Value]| -> Result<Effector, String> {
                Ok(Effector {
                    number: integer(args, 0, "effector")?,
                    life: to_f32(number(args, 1, "effector")?),
                })
            },
        )
        .constructible()
        // 2.3: the engine's own lowers the remaining life by dt and keeps
        // the effect, `pp` unchanged.
        .overridable("process", move |e: &mut Effector, _| {
            e.life -= c.dt();
            Ok(vec![Value::Boolean(true)])
        })
}

/// A number property of `effector_params`: read exactly; assigned by Lua's
/// number conversion without a type test, as a 32-bit float (3; E 2.4).
fn float_property(
    class: EngineClass,
    name: &str,
    get: fn(&EffectorParams) -> f32,
    set: fn(&mut EffectorParams, f32),
) -> EngineClass {
    class.writable_property(
        name,
        move |p: &EffectorParams| Value::Number(f64::from(get(p))),
        move |p, v| {
            set(p, to_f32(lua_number(v)));
            Ok(())
        },
    )
}

/// A record property of `effector_params` of value type `T` (engine class
/// `class`): read as a new object holding a copy, assigned by copying the
/// values in (3). Another type is our error (unestablished).
fn record_property<T: Clone + 'static>(
    class: EngineClass,
    name: &'static str,
    type_name: &'static str,
    get: fn(&EffectorParams) -> T,
    set: fn(&mut EffectorParams, T),
) -> EngineClass {
    class.property_with_objects(
        name,
        move |p: &EffectorParams, o| Ok(o.new_object(type_name, get(p))),
        Some(Box::new(move |p: &mut EffectorParams, o, v| {
            match o.get::<T>(v) {
                Some(crate::classes::Operand::Other(value)) => {
                    set(p, value);
                    Ok(())
                }
                _ => Err(format!("effector_params.{name}: {type_name} expected")),
            }
        })),
    )
}

/// The class `effector_params` (3).
fn effector_params_class() -> EngineClass {
    let class = EngineClass::new("effector_params")
        .constructor(
            0,
            |_| Some(0),
            |_, _| -> Result<EffectorParams, String> { Ok(EffectorParams::DEFAULT) },
        )
        .constructible();
    let class = float_property(class, "blur", |p| p.blur, |p, v| p.blur = v);
    let class = float_property(class, "gray", |p| p.gray, |p, v| p.gray = v);
    let class = record_property(class, "dual", "duality", |p| p.dual, |p, v| p.dual = v);
    let class = record_property(class, "noise", "noise", |p| p.noise, |p, v| p.noise = v);
    let class = record_property(
        class,
        "color_base",
        "color",
        |p| p.color_base,
        |p, v| p.color_base = v,
    );
    let class = record_property(
        class,
        "color_gray",
        "color",
        |p| p.color_gray,
        |p, v| p.color_gray = v,
    );
    let class = record_property(
        class,
        "color_add",
        "color",
        |p| p.color_add,
        |p, v| p.color_add = v,
    );
    // `p:assign(q)`: all ten values of `q` (q being `p` copies nothing).
    class.method_with_objects("assign", |p: &mut EffectorParams, o, args| {
        match o.get::<EffectorParams>(arg(args, 0)) {
            Some(crate::classes::Operand::Other(q)) => *p = q,
            Some(crate::classes::Operand::This) => {}
            None => return Err("effector_params:assign: effector_params expected".into()),
        }
        Ok(Vec::new())
    })
}

/// The hidden Rust function the Lua side calls: `op`, then the
/// arguments.
fn raw(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    ctx.stopped()?;
    let op = match args.first() {
        Some(Value::String(s)) => String::from_utf8_lossy(s).into_owned(),
        _ => return Err("an operation expected".into()),
    };
    let args = &args[1..];
    let what = format!("level.{op}");
    let what = what.as_str();
    match op.as_str() {
        // 2.2: the effect added to (or removed from) the player's manager.
        "start" | "finish" => {
            let what = if op == "start" {
                "effector:start"
            } else {
                "effector:finish"
            };
            let Value::Object(part) = arg(args, 0) else {
                return Err(format!("{what}: effector expected"));
            };
            let effector = (maker.get::<Effector>(arg(args, 0)))
                .ok_or_else(|| format!("{what}: effector expected"))?;
            ctx.player(what)?;
            let mut state = ctx.state.borrow_mut();
            if op == "start" {
                state.add(ScreenEffect {
                    number: effector.number,
                    part: *part,
                });
            } else {
                state.remove_first(effector.number);
            }
            Ok(Vec::new())
        }
        // 4.1.
        "add_pp_effector" => {
            forms(args, &[3], what)?;
            let file = text(args, 0, what)?;
            integer(args, 1, what)?;
            boolean(args, 2, what)?;
            ctx.player(what)?;
            Err(ctx.animated_effect(what, &file))
        }
        // 4.3, 4.4: only animated effects are changed, and the manager has
        // none (#1193); an `effector` is not affected.
        "set_pp_effector_factor" => {
            forms(args, &[2, 3], what)?;
            for i in 0..args.len() {
                number(args, i, what)?;
            }
            ctx.player(what)?;
            Ok(Vec::new())
        }
        "remove_pp_effector" => {
            forms(args, &[1], what)?;
            integer(args, 0, what)?;
            ctx.player(what)?;
            Ok(Vec::new())
        }
        // 5.
        "add_complex_effector" => {
            forms(args, &[2], what)?;
            let section = cp1251::decode(&text(args, 0, what)?);
            integer(args, 1, what)?;
            ctx.player(what)?;
            complex_effector(ctx, what, &section)
        }
        "remove_complex_effector" => {
            forms(args, &[1], what)?;
            let n = integer(args, 0, what)?;
            ctx.player(what)?;
            // The camera effect of number n: none exists (#1194).
            ctx.state.borrow_mut().remove_first(n);
            Ok(Vec::new())
        }
        // 6.1, 6.3.
        "add_cam_effector" | "add_cam_effector2" => {
            forms(args, &[4], what)?;
            let file = text(args, 0, what)?;
            integer(args, 1, what)?;
            boolean(args, 2, what)?;
            text(args, 3, what)?;
            ctx.player(what)?;
            Err(ctx.camera_effect(what, &file))
        }
        "remove_cam_effector" => {
            forms(args, &[1], what)?;
            integer(args, 0, what)?;
            ctx.player(what)?;
            // The manager's camera effects: none exists (#1194).
            Ok(Vec::new())
        }
        _ => weather::raw(ctx, &op, args),
    }
}

/// `add_complex_effector`'s section `section` (5): its animated effect,
/// then its camera effect; a section without either key adds nothing. A
/// missing section, or a missing cyclic key, is fatal as reading one is
/// (ltx-config.md; believed for the key).
fn complex_effector(ctx: &Context, what: &str, section: &str) -> Results {
    let system = Rc::clone(&ctx.files.borrow().system);
    let missing = |key: &str| {
        ctx.fatal(
            what,
            line!(),
            "fatal error",
            "<no expression>",
            &[&format!("Can't find variable {key} in [{section}]")],
        )
    };
    let Some(s) = system.section(section) else {
        return Err(ctx.fatal(
            what,
            line!(),
            "fatal error",
            "<no expression>",
            &[&format!("Can't open section '{section}'")],
        ));
    };
    let value = |key: &str| s.get(key).map(|v| v.unwrap_or("").to_owned());
    let flag = |key: &str| -> Result<bool, String> {
        let v = value(key).ok_or_else(|| missing(key))?;
        Ok(crate::files::read_bool(&v))
    };
    if let Some(file) = value("pp_eff_name") {
        flag("pp_eff_cyclic")?;
        let file = cp1251::encode(&file).unwrap_or_else(|| file.into_bytes());
        return Err(ctx.animated_effect(what, &file));
    }
    if let Some(file) = value("cam_eff_name") {
        flag("cam_eff_cyclic")?;
        let file = cp1251::encode(&file).unwrap_or_else(|| file.into_bytes());
        return Err(ctx.camera_effect(what, &file));
    }
    Ok(Vec::new())
}

/// Whether a result is true as Lua reads it (2.3).
fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Boolean(false))
}

/// What a state with script API U registered keeps.
pub(crate) struct Registered {
    ctx: Context,
}

fn api(message: &str) -> Error {
    Error {
        kind: ErrorKind::Api,
        message: message.into(),
    }
}

impl Scripts {
    /// Registers script API U's names (1.1, 1.2) over `state`: after the
    /// game object basics (whose objects, services and files it shares, and
    /// whose object event callbacks made `level` a table) and the value
    /// types, before [`Scripts::load_globals`].
    pub fn register_effects_and_weather(
        &mut self,
        state: Rc<RefCell<EffectsAndWeather>>,
    ) -> Result<(), Error> {
        let basics = (self.game_object_state.as_ref())
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        let ctx = Context {
            state,
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            fatal: self.fatal_flag(),
        };
        self.register_class(effector_class(&ctx))?;
        self.register_class(effector_params_class())?;
        for class in objects::classes(&ctx) {
            self.register_class(class)?;
        }
        weather::register(self, &ctx)?;
        let c = ctx.clone();
        let lua = self.lua_mut();
        lua.register_with_objects("__volya_effects_and_weather", move |maker, args| {
            raw(&c, maker, args)
        })?;
        lua.exec(
            "=effects_and_weather",
            include_bytes!("effects_and_weather.lua"),
        )?;
        self.effects_and_weather_state = Some(Registered { ctx });
        Ok(())
    }

    /// Script API U's state, once registered.
    pub fn effects_and_weather(&self) -> Option<Rc<RefCell<EffectsAndWeather>>> {
        (self.effects_and_weather_state.as_ref()).map(|r| Rc::clone(&r.ctx.state))
    }

    /// The player's manager processes its screen effects in a camera update
    /// (2.4), from the last in the list to the first: an effect not valid
    /// (its remaining life not above 0) or whose `process(pp)` returns
    /// false is removed (the first of its number); the others' values,
    /// minus the identity's, are added to the set, which starts from the
    /// identity; a negative noise grain is then the identity's 1
    /// (frame-presentation.md 2.2 steps 1 – 4). An effect taken out by an
    /// earlier one's processing is not processed (our choice). A failing
    /// override is fatal (A 5.3).
    pub fn process_screen_effects(&mut self) -> Result<(), OverrideFailure> {
        let Some(state) = self.effects_and_weather() else {
            return Err(api("script API U isn't registered").into());
        };
        let list = state.borrow().screen_effects.clone();
        let mut set = EffectorParams::IDENTITY;
        for effect in list.iter().rev() {
            if !state.borrow().screen_effects.contains(effect) {
                continue;
            }
            let valid = self
                .with_part(effect.part, |e: &mut Effector| e.life > 0.0)
                .unwrap_or(false);
            let values = if valid {
                self.process_effect(effect.part)?
            } else {
                None
            };
            match values {
                Some(values) => set = set.add_difference(&values),
                None => state.borrow_mut().remove_first(effect.number),
            }
        }
        if set.noise.grain < 0.0 {
            set.noise.grain = EffectorParams::IDENTITY.noise.grain;
        }
        state.borrow_mut().set = set;
        Ok(())
    }

    /// The effect's processing (2.3): `process(pp)` with a record of all
    /// ten values 0, referring to the engine's (valid only during the call:
    /// a `pp` kept then fails when used, our choice). Its values, or `None`
    /// when it returned false.
    fn process_effect(
        &mut self,
        part: EnginePart,
    ) -> Result<Option<EffectorParams>, OverrideFailure> {
        let pp =
            self.guarded(|lua| lua.new_engine_object("effector_params", |_| EffectorParams::ZERO))?;
        let result = self.dispatch(part, "process", &[Value::Object(pp)], Conversion::Value);
        let values = self.with_part(pp, |p: &mut EffectorParams| *p);
        self.guarded(|lua| lua.release_engine_object(pp))?;
        let keep = truthy(&result?);
        Ok(values.filter(|_| keep))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_effect_contributes_its_difference_from_the_identity() {
        // 3: a default record contributes −20 to noise.fps and nothing else.
        let set = EffectorParams::IDENTITY.add_difference(&EffectorParams::DEFAULT);
        let mut want = EffectorParams::IDENTITY;
        want.noise.fps = 10.0;
        assert_eq!(set, want);
        // 2.4: a field left at 0 subtracts the identity's value.
        let set = EffectorParams::IDENTITY.add_difference(&EffectorParams::ZERO);
        assert_eq!(set, EffectorParams::ZERO);
    }

    #[test]
    fn adding_replaces_the_first_effect_of_its_number() {
        let mut s = EffectsAndWeather::new();
        let e = |number, part| ScreenEffect {
            number,
            part: EnginePart::from_value(part),
        };
        s.add(e(1, 1.0));
        s.add(e(2, 2.0));
        s.add(e(1, 3.0));
        assert_eq!(s.screen_effects(), [e(2, 2.0), e(1, 3.0)]);
        s.remove_first(2);
        assert_eq!(s.screen_effects(), [e(1, 3.0)]);
    }
}
