// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The engine services of the game scripts' interface (script API B,
//! docs/behavior/script-api-engine-services.md; section numbers are its):
//! the console object, the render device object, the global functions
//! `app_ready`, `editor`, `log`, `error_log`, `flush`,
//! `verify_if_thread_is_running`, `xrRender_test_r2_hw`,
//! `script_server_object_version` and `IsGameTypeSingle` (`prefetch` is
//! the loader's, [`crate::loader`]), and the classes `DLL_Pure`,
//! `ISheduled`, `IRenderable`, `ICollidable`, `IRender_Visual`,
//! `CKinematicsAnimated` and `CBlend`.
//!
//! What scripts read and change is [`EngineServices`]: the engine's console
//! and pause, and the device, loading phases and game type as the spec
//! states them, which the engine feeds ([`Device::frame`] and the rest;
//! TODO(#941): the app doesn't yet). [`Scripts::register_engine_services`]
//! registers the names on a game script state. See
//! docs/architecture/scripts.md, Engine services.

use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;

use volya_engine_core::console::{Console, FatalReport, Kind, Value as CommandValue};
use volya_engine_core::pause::Pause;
use volya_engine_formats::cp1251;
use volya_engine_formats::ltx::{Config, value};
use volya_engine_formats::skeletal_motions::MotionSet;

use crate::classes::{EngineClass, EnginePart};
use crate::loader::Scripts;
use crate::{Error, Value};

/// The longest line `execute` takes, in bytes (2.2).
pub const LINE_LENGTH: usize = 1023;

/// The longest name `execute_script` takes, in bytes (2.3).
pub const SCRIPT_NAME_LENGTH: usize = 109;

/// The frames of the precache phase a video-device reset asks for (3.4).
pub const RESET_PRECACHE_FRAMES: u32 = 20;

/// The constant-step mode's step, milliseconds (3.3).
pub const CONSTANT_STEP_MS: u32 = 20;

/// The most `f_time_delta` becomes, seconds (3.3).
pub const MAX_FRAME_TIME: f32 = 0.1;

/// The section and key of `script_server_object_version` (4.7).
pub const SERVER_OBJECT_VERSION: (&str, &str) = ("script", "current_server_entity_version");

/// A camera's values as the device keeps them (3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Camera {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    pub top: [f32; 3],
    pub right: [f32; 3],
    /// The vertical field of view, degrees.
    pub fov: f32,
    /// Height over width (believed, 3.2).
    pub aspect_ratio: f32,
}

/// The render device as scripts see it (3): the frame size, the frame
/// counter and times, the precache phase and the camera's values. Its
/// caller moves it on: [`Device::frame`] at the start of every frame,
/// [`Device::end_frame`] at its end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    /// The back buffer's size at the last video-device reset (3.2).
    pub width: u32,
    pub height: u32,
    /// The device flag of 3.3's constant-step mode (what sets it is
    /// unestablished; nothing does here).
    pub constant_step: bool,
    /// 0 before the first frame (believed, 3.2).
    frame: u32,
    precache_frame: u32,
    /// The current precache phase's length *n* (3.4).
    precache_frames: u32,
    /// 0 when the main loop starts (3.3).
    time_global: u32,
    time_delta: u32,
    f_time_delta: f32,
    camera: Camera,
}

impl Device {
    /// The start of a frame the main loop moves the device on (3.3): the
    /// global timer reads `global_ms` (scaled by the time factor, stopped
    /// while the game is paused: the engine's global clock,
    /// [`volya_engine_core::pause::GlobalClock`]), the frame timer
    /// `elapsed_s` seconds since the previous frame, and the game pause is
    /// `paused`.
    // TODO(#934): gap 2 leaves which iterations don't move the device on;
    // the simplest reading: the caller calls this for every frame.
    pub fn frame(&mut self, global_ms: u64, elapsed_s: f32, paused: bool) {
        self.frame = self.frame.wrapping_add(1);
        if self.constant_step {
            self.time_global = self.time_global.wrapping_add(CONSTANT_STEP_MS);
            self.time_delta = CONSTANT_STEP_MS;
            self.f_time_delta = CONSTANT_STEP_MS as f32 / 1000.0;
        } else {
            self.f_time_delta = if paused {
                0.0
            } else {
                (0.9 * elapsed_s + 0.1 * self.f_time_delta).min(MAX_FRAME_TIME)
            };
            // The device's time is unsigned 32-bit.
            let now = global_ms as u32;
            self.time_delta = now.wrapping_sub(self.time_global);
            self.time_global = now;
        }
        if self.precache_frame != 0 {
            // Our choice: the spec leaves which of sine and cosine goes to
            // which component unestablished (3.4).
            let angle = 2.0 * PI * self.precache_frame as f32 / self.precache_frames as f32;
            let dir = [angle.sin(), 0.0, angle.cos()];
            let top = [0.0, 1.0, 0.0];
            self.camera.dir = dir;
            self.camera.top = top;
            self.camera.right = cross(top, dir);
        }
    }

    /// The end of a frame: during a precache phase `precache_frame`
    /// decreases by 1 and the loading image is drawn (`true`; 3.4).
    // TODO(#934): gap 1 leaves what else a phase's end does; the simplest
    // reading: the sound muted during it plays again ([`Self::precaching`]
    // turns false), nothing more.
    pub fn end_frame(&mut self) -> bool {
        if self.precache_frame == 0 {
            return false;
        }
        self.precache_frame -= 1;
        true
    }

    /// A video-device reset to a back buffer of `width` × `height`: the
    /// frame size, and a precache phase of 20 frames (3.2, 3.4).
    pub fn reset(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.request_precache(RESET_PRECACHE_FRAMES);
    }

    /// The engine asks for a precache phase of `frames` frames (3.4; the
    /// level start's is 30, loading-screen.md).
    pub fn request_precache(&mut self, frames: u32) {
        self.precache_frame = frames;
        self.precache_frames = frames;
    }

    /// Whether a precache phase runs: sound is muted (3.4).
    pub fn precaching(&self) -> bool {
        self.precache_frame != 0
    }

    /// The camera applies its values to the device (3.2); while precaching,
    /// the directions stay the precache phase's (3.4).
    pub fn apply_camera(&mut self, camera: Camera) {
        if self.precaching() {
            self.camera = Camera {
                dir: self.camera.dir,
                top: self.camera.top,
                right: self.camera.right,
                ..camera
            };
        } else {
            self.camera = camera;
        }
    }

    pub fn frame_count(&self) -> u32 {
        self.frame
    }

    pub fn precache_frame(&self) -> u32 {
        self.precache_frame
    }

    /// `time_global()`, milliseconds (3.3).
    pub fn time_global(&self) -> u32 {
        self.time_global
    }

    pub fn time_delta(&self) -> u32 {
        self.time_delta
    }

    pub fn f_time_delta(&self) -> f32 {
        self.f_time_delta
    }

    pub fn camera(&self) -> Camera {
        self.camera
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The game type (4.5), by its number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameType {
    /// At startup, and for an unknown option.
    #[default]
    None = 0,
    Single = 1,
    Deathmatch = 2,
    TeamDeathmatch = 6,
    ArtefactHunt = 7,
}

impl GameType {
    /// The game type of the option the engine starts a game with: its
    /// second `/`-separated item, lower-cased, compared as text (4.5).
    pub fn from_option(option: &str) -> Self {
        let item = option.split('/').nth(1).unwrap_or("").to_ascii_lowercase();
        match item.as_str() {
            "single" => GameType::Single,
            "deathmatch" => GameType::Deathmatch,
            "teamdeathmatch" => GameType::TeamDeathmatch,
            "artefacthunt" => GameType::ArtefactHunt,
            _ => GameType::None,
        }
    }

    /// `IsGameTypeSingle()`: single-player or none (4.5).
    pub fn is_single(self) -> bool {
        matches!(self, GameType::None | GameType::Single)
    }
}

/// The engine state the services read and change.
pub struct EngineServices {
    /// The engine's single console (2).
    pub console: Console,
    pub device: Device,
    /// The game pause (3.5).
    pub pause: Pause,
    pub game_type: GameType,
    /// The benchmark mode, in which `pause` does nothing (3.5; how it is
    /// entered is unestablished: nothing sets it here).
    pub benchmark: bool,
    /// The pause caption's flag (3.5, title-menu.md 2): while the game
    /// pause is on, the HUD draws it (TODO(#941)).
    pub pause_caption: bool,
    /// `xrRender_test_r2_hw()` (4.6): whether the hardware runs the R2
    /// renderer. Volya Engine draws only the static renderer, so false, as
    /// the options' renderer list has its dynamic entries disabled
    /// (TODO(#385)).
    pub r2_hardware: bool,
    /// The settings, `config\system.ltx` (4.7).
    system: Config,
    /// `script_server_object_version()` once read (4.7).
    server_object_version: Option<u16>,
    /// Open loading phases (4.1).
    loading_phases: u32,
    /// The flag `app_ready()` reads: 0 at first, set when the last open
    /// loading phase closes (4.1).
    ready: bool,
    /// `show()` (true) and `hide()` calls, in order, for the console
    /// window to apply (2.4).
    console_window: Vec<bool>,
}

impl EngineServices {
    /// The services over the engine's `console`, with the settings
    /// `system` (`config\system.ltx`).
    pub fn new(console: Console, system: Config) -> Self {
        EngineServices {
            console,
            device: Device::default(),
            pause: Pause::default(),
            game_type: GameType::None,
            benchmark: false,
            pause_caption: false,
            r2_hardware: false,
            system,
            server_object_version: None,
            loading_phases: 0,
            ready: false,
            console_window: Vec::new(),
        }
    }

    /// `app_ready()` (4.1): true exactly while no loading phase is open,
    /// after the first one closed.
    pub fn app_ready(&self) -> bool {
        self.ready && self.loading_phases == 0
    }

    /// A loading phase opens (4.1; they nest).
    pub fn open_loading_phase(&mut self) {
        self.loading_phases += 1;
        self.ready = false;
    }

    /// A loading phase closes: when it was the last open one, the console
    /// runs `stat_memory` without echo, then `app_ready()` is true (4.1).
    pub fn close_loading_phase(&mut self) {
        let Some(open) = self.loading_phases.checked_sub(1) else {
            return;
        };
        self.loading_phases = open;
        if open == 0 {
            self.console.exec("stat_memory");
            self.ready = true;
        }
    }

    /// A game starts with option `option` (4.5).
    pub fn game_started(&mut self, option: &str) {
        self.game_type = GameType::from_option(option);
    }

    /// The scripts' `show()` (true) and `hide()` calls since the last call,
    /// in order: the console window opens or closes as
    /// docs/behavior/console-window.md 1 and 2 describe (nothing when it
    /// already is open or closed).
    pub fn take_console_window(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.console_window)
    }

    /// `pause(on)` (3.5).
    fn device_pause(&mut self, on: bool) {
        if self.benchmark {
            return;
        }
        if on {
            if !self.pause.is_on() {
                self.pause_caption = true;
            }
            if self.game_type.is_single() {
                self.pause.device(true);
            }
        } else {
            self.pause.device(false);
        }
    }
}

/// A playback `PlayCycle` started (5.2): its cycle and the definition's
/// playback parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Blend {
    pub cycle: String,
    /// The motion set and the definition in it.
    pub set: usize,
    pub definition: usize,
    /// The partition it plays on (0 … 3).
    pub partition: u16,
    pub speed: f32,
    pub accrue: f32,
    pub falloff: f32,
    /// Flags bit 1: the cycle stops at the end.
    pub stops_at_end: bool,
    /// Fading out at this falloff, since a later cycle started on its
    /// partition (5.2).
    pub fading: Option<f32>,
}

/// A skeletal visual with motions (`CKinematicsAnimated`, 5.2): its motion
/// sets and the playbacks started on it.
// TODO(#943): nothing plays them; how playback proceeds is unestablished.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KinematicsAnimated {
    pub sets: Vec<MotionSet>,
    pub blends: Vec<Blend>,
}

/// The cycle partitions every partition (0xFFFF) means (5.2).
const ALL_PARTITIONS: u16 = 0xFFFF;

impl KinematicsAnimated {
    pub fn new(sets: Vec<MotionSet>) -> Self {
        KinematicsAnimated {
            sets,
            blends: Vec::new(),
        }
    }

    /// The cycle `name` (exact, case-sensitive) of the last motion set
    /// holding one: (set, definition). Within a set, its first definition
    /// of that name (our choice; fx motions aren't cycles).
    pub fn find_cycle(&self, name: &[u8]) -> Option<(usize, usize)> {
        self.sets.iter().enumerate().rev().find_map(|(s, set)| {
            set.params
                .definitions
                .iter()
                .position(|d| !d.is_fx() && d.name.as_bytes() == name)
                .map(|d| (s, d))
        })
    }

    /// `PlayCycle` of a found cycle (5.2): played on its definition's
    /// partition (0 … 3 for 0xFFFF), the cycles playing there fading out at
    /// its falloff.
    // TODO(#934): gap 3 leaves what is skipped on a partition the visual
    // doesn't define; the simplest reading: nothing, 0xFFFF plays on 0 … 3.
    pub fn play_cycle(&mut self, set: usize, definition: usize) {
        let d = &self.sets[set].params.definitions[definition];
        let partitions = if d.bone_or_partition == ALL_PARTITIONS {
            0..4
        } else {
            d.bone_or_partition..d.bone_or_partition + 1
        };
        for partition in partitions {
            for b in self.blends.iter_mut().filter(|b| b.partition == partition) {
                b.fading.get_or_insert(d.falloff);
            }
            self.blends.push(Blend {
                cycle: d.name.clone(),
                set,
                definition,
                partition,
                speed: d.speed,
                accrue: d.accrue,
                falloff: d.falloff,
                stops_at_end: d.flags & volya_engine_core::cycles::STOPS_AT_END != 0,
                fading: None,
            });
        }
    }
}

/// An `IRender_Visual` (5.2): the same visual as a `CKinematicsAnimated`
/// when it is a skeletal visual with motions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderVisual {
    pub animated: Option<EnginePart>,
}

/// The console object's part: the console is [`EngineServices::console`].
struct ConsoleObject;

/// The render device object's part: the device is
/// [`EngineServices::device`].
struct DeviceObject;

/// A `DLL_Pure`'s part: the object `_construct` returns (5.1).
struct Pure(EnginePart);

/// What the services' functions share: the engine state and the state's
/// fatal error.
#[derive(Clone)]
struct Context {
    services: Rc<RefCell<EngineServices>>,
    fatal: Rc<RefCell<Option<String>>>,
}

impl Context {
    /// The fatal error if the state is stopped.
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error (CL 3.8): its report goes to the log, and the state
    /// stops, as the original program ends. The error to raise.
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

    /// `get_console():execute(line)` (2.2).
    fn execute(&self, line: &[u8]) {
        let line = cp1251::decode(&line[..line.len().min(LINE_LENGTH)]);
        self.services.borrow_mut().console.exec(&line);
    }
}

/// A fatal error's report (CL 3.8): where it was raised and its rows.
pub(crate) struct Fatal<'a> {
    pub function: &'a str,
    pub file: &'a str,
    pub line: u32,
    pub expression: &'a str,
    pub description: &'a str,
    pub arguments: &'a [&'a str],
}

/// A fatal error (CL 3.8): its report goes to the console's log, and the
/// state stops (`fatal` holds the message), as the original program ends.
/// The error to raise.
pub(crate) fn report_fatal(
    services: &RefCell<EngineServices>,
    fatal: &RefCell<Option<String>>,
    report: Fatal,
) -> String {
    let Fatal {
        function,
        file,
        line,
        expression,
        description,
        arguments,
    } = report;
    services.borrow_mut().console.log_mut().fatal(&FatalReport {
        expression,
        function,
        file,
        line: line as i32,
        description,
        arguments,
    });
    let message = [expression, description]
        .into_iter()
        .chain(arguments.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");
    *fatal.borrow_mut() = Some(message.clone());
    message
}

/// The text argument `args[i]` of `what` (our wording for a mismatch; the
/// binding library's rule, A).
fn text<'a>(args: &'a [Value], i: usize, what: &str) -> Result<&'a [u8], String> {
    match args.get(i) {
        Some(Value::String(s)) => Ok(s),
        _ => Err(format!("{what}: text expected")),
    }
}

fn boolean(args: &[Value], i: usize, what: &str) -> Result<bool, String> {
    match args.get(i) {
        Some(Value::Boolean(b)) => Ok(*b),
        _ => Err(format!("{what}: boolean expected")),
    }
}

/// A text for Lua: Windows-1251 bytes.
fn lua_text(text: &str) -> Value {
    Value::String(cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec()))
}

fn vector(v: [f32; 3]) -> Value {
    // TODO(#942): spec E's vector object; a new table holding a copy.
    Value::Table(vec![
        ("x".into(), f64::from(v[0]).into()),
        ("y".into(), f64::from(v[1]).into()),
        ("z".into(), f64::from(v[2]).into()),
    ])
}

/// A command's kind and value by the exact, case-sensitive lookup of line
/// processing (2.5).
fn command(console: &Console, name: &[u8]) -> Option<(Kind, CommandValue)> {
    let name = cp1251::decode(name);
    Some((Console::def(&name)?.kind, console.value(&name)?))
}

/// `CConsole` and `get_console()` (2).
fn console_class(ctx: &Context) -> EngineClass {
    let c = ctx.clone();
    let mut class =
        EngineClass::new("CConsole").method("execute", move |_: &mut ConsoleObject, args| {
            c.stopped()?;
            c.execute(text(args, 0, "CConsole.execute")?);
            Ok(Vec::new())
        });
    let c = ctx.clone();
    class = class.method("execute_script", move |_: &mut ConsoleObject, args| {
        c.stopped()?;
        let name = text(args, 0, "CConsole.execute_script")?;
        if name.len() > SCRIPT_NAME_LENGTH {
            // Which rows hold the text is unestablished (2.3): ours, the
            // fatal message's argument.
            return Err(c.fatal(
                "CConsole::ExecuteScript",
                line!(),
                "fatal error",
                "<no expression>",
                &["error handler is invoked!"],
            ));
        }
        c.execute(&[b"cfg_load ".as_slice(), name].concat());
        Ok(Vec::new())
    });
    for (name, show) in [("show", true), ("hide", false)] {
        let c = ctx.clone();
        class = class.method(name, move |_: &mut ConsoleObject, _| {
            c.stopped()?;
            c.services.borrow_mut().console_window.push(show);
            Ok(Vec::new())
        });
    }
    for name in ["get_string", "get_token"] {
        let c = ctx.clone();
        let what = format!("CConsole.{name}");
        class = class.method(name, move |_: &mut ConsoleObject, args| {
            c.stopped()?;
            let name = cp1251::decode(text(args, 0, &what)?);
            let status = c.services.borrow().console.status(&name);
            Ok(vec![status.map_or(Value::Nil, |s| lua_text(&s))])
        });
    }
    let c = ctx.clone();
    class = class.method("get_integer", move |_: &mut ConsoleObject, args| {
        c.stopped()?;
        let found = command(
            &c.services.borrow().console,
            text(args, 0, "CConsole.get_integer")?,
        );
        let n = match found {
            None => 0.0,
            Some((_, CommandValue::Int(v))) => v as f64,
            Some((_, CommandValue::OnOff(on))) => f64::from(u8::from(on)),
            Some(_) => {
                return Err(c.fatal(
                    "CConsole::GetInteger",
                    line!(),
                    "assertion failed",
                    "cm",
                    &[],
                ));
            }
        };
        Ok(vec![n.into()])
    });
    let c = ctx.clone();
    class = class.method("get_float", move |_: &mut ConsoleObject, args| {
        c.stopped()?;
        let found = command(
            &c.services.borrow().console,
            text(args, 0, "CConsole.get_float")?,
        );
        let n = match found {
            None => 0.0,
            Some((Kind::Real { .. }, CommandValue::Real(v))) => f64::from(v),
            // Believed a crash (2.5); ours, a fatal error.
            Some(_) => return Err(not_of_kind(&c, "CConsole::GetFloat", line!(), "real")),
        };
        Ok(vec![n.into()])
    });
    let c = ctx.clone();
    class = class.method("get_bool", move |_: &mut ConsoleObject, args| {
        c.stopped()?;
        let found = command(
            &c.services.borrow().console,
            text(args, 0, "CConsole.get_bool")?,
        );
        let b = match found {
            // Indeterminate in the original (2.5); our choice.
            None => false,
            Some((_, CommandValue::OnOff(on))) => on,
            Some((Kind::Int { .. }, CommandValue::Int(v))) => v != 0,
            // Believed a crash (2.5); ours, a fatal error.
            Some(_) => {
                return Err(not_of_kind(
                    &c,
                    "CConsole::GetBool",
                    line!(),
                    "on/off or integer",
                ));
            }
        };
        Ok(vec![b.into()])
    });
    class
}

/// The fatal error standing for the believed crash of `get_float` and
/// `get_bool` on a command of another kind (2.5); our wording.
fn not_of_kind(c: &Context, function: &str, line: u32, kind: &str) -> String {
    let description = format!("not a command of the {kind} kind");
    c.fatal(
        function,
        line,
        "fatal error",
        "<no expression>",
        &[&description],
    )
}

/// `render_device` and `device()` (3).
fn device_class(ctx: &Context) -> EngineClass {
    type Field = fn(&Device) -> Value;
    let fields: [(&str, Field); 12] = [
        ("width", |d| f64::from(d.width).into()),
        ("height", |d| f64::from(d.height).into()),
        ("frame", |d| f64::from(d.frame).into()),
        ("precache_frame", |d| f64::from(d.precache_frame).into()),
        ("time_delta", |d| f64::from(d.time_delta).into()),
        ("f_time_delta", |d| f64::from(d.f_time_delta).into()),
        ("cam_pos", |d| vector(d.camera.pos)),
        ("cam_dir", |d| vector(d.camera.dir)),
        ("cam_top", |d| vector(d.camera.top)),
        ("cam_right", |d| vector(d.camera.right)),
        ("fov", |d| f64::from(d.camera.fov).into()),
        ("aspect_ratio", |d| f64::from(d.camera.aspect_ratio).into()),
    ];
    let mut class = EngineClass::new("render_device");
    for (name, read) in fields {
        let c = ctx.clone();
        class = class.property(name, move |_: &DeviceObject| {
            read(&c.services.borrow().device)
        });
    }
    let c = ctx.clone();
    class = class.method("time_global", move |_: &mut DeviceObject, _| {
        c.stopped()?;
        Ok(vec![
            f64::from(c.services.borrow().device.time_global).into(),
        ])
    });
    let c = ctx.clone();
    class = class.method("is_paused", move |_: &mut DeviceObject, _| {
        c.stopped()?;
        Ok(vec![c.services.borrow().pause.is_on().into()])
    });
    let c = ctx.clone();
    class.method("pause", move |_: &mut DeviceObject, args| {
        c.stopped()?;
        let on = boolean(args, 0, "render_device.pause")?;
        c.services.borrow_mut().device_pause(on);
        Ok(Vec::new())
    })
}

/// `CKinematicsAnimated` (5.2).
fn kinematics_class(ctx: &Context) -> EngineClass {
    let c = ctx.clone();
    EngineClass::new("CKinematicsAnimated").method(
        "PlayCycle",
        move |k: &mut KinematicsAnimated, args| {
            c.stopped()?;
            let name = text(args, 0, "CKinematicsAnimated.PlayCycle")?;
            match k.find_cycle(name) {
                Some((set, definition)) => k.play_cycle(set, definition),
                None => {
                    let name = cp1251::decode(name);
                    return Err(c.fatal(
                        "CKinematicsAnimated::ID_Cycle",
                        line!(),
                        "motion_ID.valid()",
                        "! MODEL: can't find cycle: ",
                        &[&name],
                    ));
                }
            }
            Ok(Vec::new())
        },
    )
}

/// The global functions of 4 that need nothing of the state but `ctx`.
fn register_functions(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    let lua = scripts.lua_mut();
    let c = ctx.clone();
    lua.register("app_ready", move |_| {
        c.stopped()?;
        Ok(vec![c.services.borrow().app_ready().into()])
    })?;
    lua.register("editor", |_| Ok(vec![false.into()]))?;
    // They end in a log routine that returns at once (4.3).
    for name in ["log", "error_log", "flush", "verify_if_thread_is_running"] {
        lua.register(name, |_| Ok(Vec::new()))?;
    }
    let c = ctx.clone();
    lua.register("IsGameTypeSingle", move |_| {
        c.stopped()?;
        Ok(vec![c.services.borrow().game_type.is_single().into()])
    })?;
    let c = ctx.clone();
    lua.register("xrRender_test_r2_hw", move |_| {
        c.stopped()?;
        Ok(vec![c.services.borrow().r2_hardware.into()])
    })?;
    let c = ctx.clone();
    lua.register("script_server_object_version", move |_| {
        c.stopped()?;
        let mut services = c.services.borrow_mut();
        if let Some(version) = services.server_object_version {
            return Ok(vec![f64::from(version).into()]);
        }
        let (section, key) = SERVER_OBJECT_VERSION;
        // Read once (4.7); 0 is stored before a failing read.
        services.server_object_version = Some(0);
        let read = services
            .system
            .read(section, key)
            .map(|v| value::int(v) as u16);
        drop(services);
        match read {
            Ok(version) => {
                c.services.borrow_mut().server_object_version = Some(version);
                Ok(vec![f64::from(version).into()])
            }
            // ltx-config.md 11: a missing section or key is fatal (our
            // wording).
            Err(e) => Err(c.fatal(
                "script_server_object_version",
                line!(),
                "fatal error",
                "<no expression>",
                &[&format!("system.ltx: {e}")],
            )),
        }
    })?;
    Ok(())
}

impl Scripts {
    /// Registers the engine services (1) over `services`: the engine
    /// registers them before [`Scripts::load_globals`]. The console and
    /// render device objects are made here.
    pub fn register_engine_services(
        &mut self,
        services: Rc<RefCell<EngineServices>>,
    ) -> Result<(), Error> {
        let ctx = Context {
            services,
            fatal: self.fatal_flag(),
        };
        // 5.1: bases of further classes' objects, for script classes to
        // derive from.
        self.register_class(
            EngineClass::new("DLL_Pure")
                .constructor(0, |_| Some(0), |part, _| Ok(Pure(part)))
                // The engine's own `_construct` returns the object itself.
                .object_method("_construct", |p: &mut Pure, _| Ok(Some(p.0))),
        )?;
        self.register_class(EngineClass::new("ISheduled"))?;
        self.register_class(EngineClass::new("IRenderable"))?;
        self.register_class(EngineClass::new("ICollidable").constructor(
            0,
            |_| Some(0),
            |_, _| Ok(()),
        ))?;
        // 5.2.
        self.register_class(kinematics_class(&ctx))?;
        self.register_class(
            EngineClass::new("IRender_Visual")
                .constructor(0, |_| Some(0), |_, _| Ok(RenderVisual::default()))
                .object_method("dcast_PKinematicsAnimated", |v: &mut RenderVisual, _| {
                    Ok(v.animated)
                }),
        )?;
        self.register_class(EngineClass::new("CBlend"))?;
        // 2, 3.
        self.register_class(console_class(&ctx))?;
        self.register_class(device_class(&ctx))?;
        let lua = self.lua_mut();
        let console = lua.new_engine_object("CConsole", |_| ConsoleObject)?;
        let device = lua.new_engine_object("render_device", |_| DeviceObject)?;
        lua.register_object_function("get_console", move |_| Ok(Some(console)))?;
        lua.register_object_function("device", move |_| Ok(Some(device)))?;
        register_functions(self, &ctx)
    }

    /// A visual for scripts (5.2): with `motions`, a skeletal visual with
    /// motions, whose `dcast_PKinematicsAnimated()` is its
    /// `CKinematicsAnimated` ([`KinematicsAnimated`]); without, any other
    /// visual. Returns the `IRender_Visual`'s part; both live as long as the
    /// state.
    pub fn new_visual(&mut self, motions: Option<Vec<MotionSet>>) -> Result<EnginePart, Error> {
        let lua = self.lua_mut();
        let animated = match motions {
            Some(sets) => Some(
                lua.new_engine_object("CKinematicsAnimated", |_| KinematicsAnimated::new(sets))?,
            ),
            None => None,
        };
        lua.new_engine_object("IRender_Visual", |_| RenderVisual { animated })
    }
}
