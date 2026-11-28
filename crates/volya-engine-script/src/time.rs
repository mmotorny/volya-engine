// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Time as scripts reach it (script API H, docs/behavior/script-api-time.md;
//! section numbers are its): the class `CTime` (as `game.CTime`), the
//! functions `game.get_game_time` and `game.time`, the global
//! `time_global`, the `level` functions `get_time_days`, `get_time_hours`,
//! `get_time_minutes`, `get_time_factor` and `set_time_factor`, and the
//! class `profile_timer`.
//!
//! What scripts read is [`GameTimes`]: the clocks' readings, which the
//! engine feeds each level frame, and the factors `level.set_time_factor`
//! asks for, which the engine takes. [`Scripts::register_time`] registers
//! the names on a game script state. See docs/architecture/scripts.md,
//! Time.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use volya_engine_core::perf_counter;

use crate::classes::{EngineClass, EnginePart, Operand};
use crate::loader::Scripts;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::values::{to_bits32, to_f32};
use crate::{Error, Value};

/// Milliseconds in a day.
pub const DAY_MS: u64 = 86_400_000;

/// `CTime`'s precision constants (1), the arguments of `timeToString` and
/// `dateToString` (5.5).
pub const PRECISIONS: [(&str, i32); 7] = [
    ("TimeToHours", 0),
    ("TimeToMinutes", 1),
    ("TimeToSeconds", 2),
    ("TimeToMilisecs", 3),
    ("DateToDay", 0),
    ("DateToMonth", 1),
    ("DateToYear", 2),
];

/// The lengths of the months, February outside a leap year (2.1).
const MONTH_DAYS: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn leap(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn month_days(year: u32, month: usize) -> u64 {
    MONTH_DAYS[month - 1] + u64::from(month == 2 && leap(year))
}

/// Compose (2.1): a date and time of day as a time value, in unsigned
/// 64-bit arithmetic from the 32-bit numbers.
pub fn compose(
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    ms: u32,
) -> u64 {
    let y = u64::from(year);
    let mut days = y
        .wrapping_sub(1)
        .wrapping_mul(365)
        .wrapping_sub(y / 100)
        .wrapping_add(y / 4)
        .wrapping_add(y / 400);
    // A month number above 12 adds all eleven.
    for m in 1..month.min(12) as usize {
        days = days.wrapping_add(month_days(year, m));
    }
    days = days.wrapping_add(u64::from(day.wrapping_sub(1)));
    days.wrapping_mul(24)
        .wrapping_add(u64::from(hour))
        .wrapping_mul(60)
        .wrapping_add(u64::from(minute))
        .wrapping_mul(60 * 1000)
        .wrapping_add(u64::from(second).wrapping_mul(1000))
        .wrapping_add(u64::from(ms))
}

/// A time value's date and time of day, as split gives them (2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub ms: u32,
}

/// Split (2.2). Step 3's q·365 is the full 64-bit product (disputed in
/// review against the mod-2³² reading; the two differ only above year
/// 11.7 million; our choice, the reviewer's value).
pub fn split(t: u64) -> Split {
    let ms = (t % 1000) as u32;
    let t1 = t / 1000;
    let days = t1 / 86_400;
    let q = (days.wrapping_mul(400) / 146_097) as u32;
    let year = q.wrapping_add(1);
    let y = u64::from(year);
    let mut d = days
        .wrapping_sub(u64::from(q).wrapping_mul(365))
        .wrapping_sub(y / 4)
        .wrapping_add(y / 100)
        .wrapping_sub(y / 400)
        .wrapping_add(1);
    let mut month = 1;
    while month < 12 && d > month_days(year, month) {
        d -= month_days(year, month);
        month += 1;
    }
    Split {
        year,
        month: month as u32,
        day: d as u32,
        hour: (t1 / 3600 % 24) as u32,
        minute: (t1 / 60 % 60) as u32,
        second: (t1 % 60) as u32,
        ms,
    }
}

/// A number argument of `CTime`'s methods as a 32-bit unsigned number
/// (5.1): truncated toward zero, a negative one from 2³²; NaN and numbers
/// outside (−2³¹, 2³¹) give 2³¹ (believed, with SSE2).
pub fn to_unsigned32(n: f64) -> u32 {
    to_bits32(n) as u32
}

/// `n` × `f` in the x87 unit at 24-bit precision (E 2.5): `n` loaded
/// exactly, the product rounded once to a 24-bit significand, ties to
/// even, then stored as an f32 (exact). `f` is a positive normal f32.
fn x_product(n: u64, f: f32) -> f32 {
    let bits = f.to_bits();
    let significand = u128::from(bits & 0x7F_FFFF | 0x80_0000);
    let exponent = ((bits >> 23) & 0xFF) as i32 - 150;
    let p = u128::from(n) * significand;
    if p == 0 {
        return 0.0;
    }
    let width = 128 - p.leading_zeros() as i32;
    let (mut q, mut shift) = (p, 0);
    if width > 24 {
        shift = width - 24;
        q = p >> shift;
        let rest = p & ((1 << shift) - 1);
        let half = 1 << (shift - 1);
        if rest > half || (rest == half && q & 1 == 1) {
            q += 1;
        }
    }
    (q as f64 * 2f64.powi(exponent + shift)) as f32
}

/// `diffSec` (5.3): (a − b) / 1000 seconds, negative (−0 for equal
/// values) when b is the later or the same.
pub fn diff_sec(a: u64, b: u64) -> f32 {
    let seconds = x_product(a.abs_diff(b), 0.001);
    if b >= a { -seconds } else { seconds }
}

/// `timeToString(precision)` (5.5); `None` for an unknown precision.
pub fn time_to_string(t: u64, precision: u32) -> Option<String> {
    let s = split(t);
    let (h, m, sec, ms) = (s.hour as i32, s.minute as i32, s.second as i32, s.ms as i32);
    Some(match precision {
        0 => format!("{h:02}"),
        1 => format!("{h:02}:{m:02}"),
        2 => format!("{h:02}:{m:02}:{sec:02}"),
        3 => format!("{h:02}:{m:02}:{sec:02}:{ms:02}"),
        4 => format!("{}d {h:02}:{m:02}:{sec:02}", (t / DAY_MS) as u32 as i32),
        _ => return None,
    })
}

/// `dateToString(precision)` (5.5); `None` for an unknown precision.
pub fn date_to_string(t: u64, precision: u32) -> Option<String> {
    let s = split(t);
    let (y, m, d) = (s.year as i32, s.month as i32, s.day as i32);
    Some(match precision {
        0 => format!("{d:02}/{m:02}/{y:04}"),
        1 => format!("{m:02}/{y:04}"),
        2 => format!("{y:04}"),
        _ => return None,
    })
}

/// The game's clocks as scripts read them (3), and what they ask of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameTimes {
    /// The simulation clock's reading (3.1), while the simulation exists.
    pub simulation_clock: Option<u64>,
    /// The client game clock's reading and factor (3.2), while a level
    /// exists.
    pub client_game_clock: Option<(u64, f32)>,
    /// Whether the level runs the server (4.2): believed always so in a
    /// single-player game.
    pub runs_server: bool,
    time_factors: Vec<f32>,
}

impl GameTimes {
    pub fn new() -> Self {
        Self::default()
    }

    /// A level frame's clocks (our reading of 3.2: Volya Engine has one
    /// game clock, so the simulation clock and the client game clock read
    /// the same, `reading` at `factor`, the client's following every
    /// factor change at the next frame): a level running the server.
    /// TODO(#1113): the clock read as 2.3 states.
    pub fn level_frame(&mut self, reading: u64, factor: f32) {
        self.simulation_clock = Some(reading);
        self.client_game_clock = Some((reading, factor));
        self.runs_server = true;
    }

    /// The level and the simulation are gone.
    pub fn level_ended(&mut self) {
        *self = Self::default();
    }

    /// The factors `level.set_time_factor` set the server game's clock to
    /// since the last call, oldest first (4.2): the engine sets its clock's
    /// factor to each (sav.md's normal time factor unchanged).
    pub fn take_time_factors(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.time_factors)
    }
}

/// A `CTime`'s time value (5.1). Shared, so a copy constructor can find it
/// by its object.
#[derive(Clone, Debug)]
pub struct CTime(Rc<Cell<u64>>);

impl CTime {
    pub fn value(&self) -> u64 {
        self.0.get()
    }
}

/// A profile timer's counts (7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timer {
    pub start: u64,
    pub accumulated: u64,
    pub intervals: u32,
    pub depth: i32,
}

/// The time-stamp counter's frequency, counts per second: unestablished
/// (7); ours, the performance counter in microseconds.
pub const COUNTER_FREQUENCY: f64 = 1_000_000.0;

/// The counter's current value.
fn counter() -> u64 {
    (perf_counter::now_ms() * 1000.0) as u64
}

impl Timer {
    /// `start()` (7).
    pub fn start(&mut self, now: u64) {
        if self.depth == 0 {
            self.depth = 1;
            self.intervals = self.intervals.wrapping_add(1);
            self.start = now;
        } else {
            self.depth += 1;
        }
    }

    /// `stop()` (7).
    pub fn stop(&mut self, now: u64) {
        self.depth -= 1;
        if self.depth == 0 && now > self.start {
            self.accumulated = self.accumulated.wrapping_add(now - self.start);
        }
    }

    /// `time()` (7), microseconds: X64[accumulated / frequency ×
    /// 1,000,000] rounded to f32 (in f64 here, our choice).
    pub fn time(&self) -> f32 {
        (self.accumulated as f64 / COUNTER_FREQUENCY * 1_000_000.0) as f32
    }
}

/// A `profile_timer`'s part.
#[derive(Clone, Debug)]
pub struct ProfileTimer(Rc<Cell<Timer>>);

/// The parts of a class's objects by object, so a copy constructor finds
/// its argument's.
type Known<T> = Rc<RefCell<HashMap<EnginePart, Weak<Cell<T>>>>>;

fn remember<T>(known: &Known<T>, part: EnginePart, cell: &Rc<Cell<T>>) {
    let mut k = known.borrow_mut();
    if k.len() >= 64 && k.len().is_power_of_two() {
        k.retain(|_, w| w.strong_count() > 0);
    }
    k.insert(part, Rc::downgrade(cell));
}

fn known_value<T: Copy>(known: &Known<T>, v: &Value) -> Option<T> {
    let Value::Object(part) = v else {
        return None;
    };
    known.borrow().get(part)?.upgrade().map(|c| c.get())
}

/// What the members share.
#[derive(Clone)]
struct Context {
    services: Rc<RefCell<EngineServices>>,
    times: Rc<RefCell<GameTimes>>,
    fatal: Rc<RefCell<Option<String>>>,
    known_times: Known<u64>,
    known_timers: Known<Timer>,
}

type Compare = fn(u64, u64) -> bool;
type Combine = fn(u64, u64) -> u64;
type Format = fn(u64, u32) -> Option<String>;

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    fn report(
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

    /// A fatal error standing for a crash the spec believes (our wording).
    fn crash(&self, function: &str, line: u32, text: &str) -> String {
        self.report(function, line, "fatal error", "<no expression>", &[text])
    }

    /// A new `CTime` object holding `value`, made by `make`.
    fn new_time(&self, value: u64, make: impl FnOnce(CTime) -> Value) -> Value {
        let cell = Rc::new(Cell::new(value));
        let object = make(CTime(Rc::clone(&cell)));
        if let Value::Object(part) = object {
            remember(&self.known_times, part, &cell);
        }
        object
    }

    fn new_timer(&self, timer: Timer, make: impl FnOnce(ProfileTimer) -> Value) -> Value {
        let cell = Rc::new(Cell::new(timer));
        let object = make(ProfileTimer(Rc::clone(&cell)));
        if let Value::Object(part) = object {
            remember(&self.known_timers, part, &cell);
        }
        object
    }

    /// The clock `game.get_game_time()` reads (3.1): the simulation's,
    /// else the client game's; with neither, believed a crash (ours, a
    /// fatal error).
    fn game_time(&self, function: &str) -> Result<u64, String> {
        let times = self.times.borrow();
        let reading = (times.simulation_clock).or(times.client_game_clock.map(|(t, _)| t));
        drop(times);
        reading.ok_or_else(|| self.crash(function, line!(), "there is no game clock"))
    }

    /// The client game clock (3.2); without a level, believed a crash
    /// (ours, a fatal error).
    fn client_game_clock(&self, function: &str) -> Result<(u64, f32), String> {
        let clock = self.times.borrow().client_game_clock;
        clock.ok_or_else(|| self.crash(function, line!(), "there is no level"))
    }
}

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// Number argument `i` converted as 5.1 (our wording for a mismatch;
/// depends on A).
fn number(args: &[Value], i: usize, what: &str) -> Result<u32, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(to_unsigned32(*n)),
        _ => Err(format!("{what}: number expected")),
    }
}

fn numbers<const N: usize>(args: &[Value], what: &str) -> Result<[u32; N], String> {
    let mut out = [0; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = number(args, i, what)?;
    }
    Ok(out)
}

/// `CTime` argument `i`'s value: nil is believed a crash (5.3; ours, a
/// fatal error), another value a mismatch (our wording; depends on A).
fn time_arg(
    c: &Context,
    o: &crate::classes::Objects,
    this: &CTime,
    args: &[Value],
    i: usize,
    what: &str,
) -> Result<u64, String> {
    match arg(args, i) {
        Value::Nil => Err(c.crash(what, line!(), "a nil CTime")),
        v => match o.get::<CTime>(v) {
            Some(Operand::This) => Ok(this.value()),
            Some(Operand::Other(t)) => Ok(t.value()),
            None => Err(format!("{what}: CTime expected")),
        },
    }
}

fn timer_arg(
    o: &crate::classes::Objects,
    this: &ProfileTimer,
    args: &[Value],
    what: &str,
) -> Result<Timer, String> {
    match o.get::<ProfileTimer>(arg(args, 0)) {
        Some(Operand::This) => Ok(this.0.get()),
        Some(Operand::Other(t)) => Ok(t.0.get()),
        None => Err(format!("{what}: profile_timer expected")),
    }
}

/// `CTime` (5).
fn time_class(ctx: &Context) -> EngineClass {
    let made = ctx.clone();
    let copied = ctx.clone();
    let scored = ctx.clone();
    let mut class = EngineClass::new("CTime")
        .constructor(
            0,
            |_| Some(0),
            move |part, _| {
                let cell = Rc::new(Cell::new(0));
                remember(&made.known_times, part, &cell);
                Ok(CTime(cell))
            },
        )
        .constructor(
            1,
            move |args| known_value(&scored.known_times, arg(args, 0)).map(|_| 0),
            move |part, args| {
                let value = known_value(&copied.known_times, arg(args, 0))
                    .ok_or_else(|| "CTime: CTime expected".to_owned())?;
                let cell = Rc::new(Cell::new(value));
                remember(&copied.known_times, part, &cell);
                Ok(CTime(cell))
            },
        )
        .constructible();
    for (name, value) in PRECISIONS {
        class = class.constant(name, value);
    }
    class = class
        .method("setHMS", |t: &mut CTime, args| {
            let [h, m, s] = numbers(args, "CTime.setHMS")?;
            t.0.set(compose_hms(h, m, s, 0));
            Ok(Vec::new())
        })
        .method("setHMSms", |t: &mut CTime, args| {
            let [h, m, s, ms] = numbers(args, "CTime.setHMSms")?;
            t.0.set(compose_hms(h, m, s, ms));
            Ok(Vec::new())
        })
        .method("set", |t: &mut CTime, args| {
            let [y, mo, d, h, mi, s, ms] = numbers(args, "CTime.set")?;
            t.0.set(compose(y, mo, d, h, mi, s, ms));
            Ok(Vec::new())
        })
        .method("get", |t: &mut CTime, args| {
            // Seven numbers whose values aren't used (5.2).
            numbers::<7>(args, "CTime.get")?;
            let s = split(t.value());
            Ok([s.year, s.month, s.day, s.hour, s.minute, s.second, s.ms]
                .into_iter()
                .map(|n| f64::from(n).into())
                .collect())
        });
    let c = ctx.clone();
    class = class.method_with_objects("add", move |t: &mut CTime, o, args| {
        c.stopped()?;
        let b = time_arg(&c, o, t, args, 0, "CTime.add")?;
        t.0.set(t.value().wrapping_add(b));
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method_with_objects("sub", move |t: &mut CTime, o, args| {
        c.stopped()?;
        let b = time_arg(&c, o, t, args, 0, "CTime.sub")?;
        let a = t.value();
        t.0.set(a.saturating_sub(b));
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method_with_objects("diffSec", move |t: &mut CTime, o, args| {
        c.stopped()?;
        let b = time_arg(&c, o, t, args, 0, "CTime.diffSec")?;
        Ok(vec![f64::from(diff_sec(t.value(), b)).into()])
    });
    let comparisons: [(&str, Compare); 5] = [
        ("__lt", |a, b| a < b),
        ("__le", |a, b| a <= b),
        ("__eq", |a, b| a == b),
        ("__gt", |a, b| a > b),
        ("__ge", |a, b| a >= b),
    ];
    for (name, compare) in comparisons {
        let c = ctx.clone();
        let what = format!("CTime.{name}");
        class = class.method_with_objects(name, move |t: &mut CTime, o, args| {
            c.stopped()?;
            let b = time_arg(&c, o, t, args, 0, &what)?;
            Ok(vec![compare(t.value(), b).into()])
        });
    }
    let arithmetic: [(&str, Combine); 2] =
        [("__add", u64::wrapping_add), ("__sub", u64::wrapping_sub)];
    for (name, op) in arithmetic {
        let c = ctx.clone();
        let what = format!("CTime.{name}");
        class = class.method_with_objects(name, move |t: &mut CTime, o, args| {
            c.stopped()?;
            let b = time_arg(&c, o, t, args, 0, &what)?;
            let value = op(t.value(), b);
            Ok(vec![c.new_time(value, |part| o.new_object("CTime", part))])
        });
    }
    let strings: [(&str, Format); 2] = [
        ("timeToString", time_to_string),
        ("dateToString", date_to_string),
    ];
    for (name, format) in strings {
        let c = ctx.clone();
        let what = format!("CTime.{name}");
        class = class.method(name, move |t: &mut CTime, args| {
            c.stopped()?;
            let precision = number(args, 0, &what)?;
            match format(t.value(), precision) {
                Some(text) => Ok(vec![text.as_str().into()]),
                // A failed check without a message (5.5).
                None => Err(c.report(
                    &what,
                    line!(),
                    "assertion failed",
                    "!\"Unknown type of date precision\"",
                    &[],
                )),
            }
        });
    }
    class
}

/// `setHMS` and `setHMSms` (5.2): a duration of day 0.
fn compose_hms(h: u32, m: u32, s: u32, ms: u32) -> u64 {
    ((u64::from(h) * 60 + u64::from(m)) * 60 + u64::from(s)) * 1000 + u64::from(ms)
}

/// `profile_timer` (7).
fn timer_class(ctx: &Context) -> EngineClass {
    let made = ctx.clone();
    let copied = ctx.clone();
    let scored = ctx.clone();
    let mut class = EngineClass::new("profile_timer")
        .constructor(
            0,
            |_| Some(0),
            move |part, _| {
                let cell = Rc::new(Cell::new(Timer::default()));
                remember(&made.known_timers, part, &cell);
                Ok(ProfileTimer(cell))
            },
        )
        .constructor(
            1,
            move |args| known_value(&scored.known_timers, arg(args, 0)).map(|_| 0),
            move |part, args| {
                // A copy, including its running state.
                let timer = known_value(&copied.known_timers, arg(args, 0))
                    .ok_or_else(|| "profile_timer: profile_timer expected".to_owned())?;
                let cell = Rc::new(Cell::new(timer));
                remember(&copied.known_timers, part, &cell);
                Ok(ProfileTimer(cell))
            },
        )
        .constructible()
        .method("start", |t: &mut ProfileTimer, _| {
            let mut timer = t.0.get();
            timer.start(counter());
            t.0.set(timer);
            Ok(Vec::new())
        })
        .method("stop", |t: &mut ProfileTimer, _| {
            let mut timer = t.0.get();
            timer.stop(counter());
            t.0.set(timer);
            Ok(Vec::new())
        })
        .method("time", |t: &mut ProfileTimer, _| {
            Ok(vec![f64::from(t.0.get().time()).into()])
        });
    let c = ctx.clone();
    class = class.method_with_objects("__add", move |t: &mut ProfileTimer, o, args| {
        let (a, b) = (t.0.get(), timer_arg(o, t, args, "profile_timer.__add")?);
        let sum = Timer {
            accumulated: a.accumulated.wrapping_add(b.accumulated),
            intervals: a.intervals.wrapping_add(b.intervals),
            ..Timer::default()
        };
        Ok(vec![c.new_timer(sum, |part| {
            o.new_object("profile_timer", part)
        })])
    });
    class = class.method_with_objects("__lt", move |t: &mut ProfileTimer, o, args| {
        let b = timer_arg(o, t, args, "profile_timer.__lt")?;
        Ok(vec![(t.0.get().accumulated < b.accumulated).into()])
    });
    let c = ctx.clone();
    // A new timer, a copy, not a text (7).
    class.method_with_objects("__tostring", move |t: &mut ProfileTimer, o, _| {
        let copy = t.0.get();
        Ok(vec![c.new_timer(copy, |part| {
            o.new_object("profile_timer", part)
        })])
    })
}

/// Moves the hidden registrations into their namespaces, made unless
/// another spec made them.
const NAMESPACES: &[u8] = br#"
local function namespace(name)
  local t = rawget(_G, name)
  if t == nil then
    t = {}
    rawset(_G, name, t)
  end
  return t
end
local game, level = namespace("game"), namespace("level")
local function move(target, name)
  local hidden = "__volya_time_" .. name
  target[name] = rawget(_G, hidden)
  rawset(_G, hidden, nil)
end
game.CTime = rawget(_G, "CTime")
rawset(_G, "CTime", nil)
move(game, "get_game_time")
move(game, "time")
for _, name in ipairs({ "get_time_days", "get_time_hours", "get_time_minutes",
    "get_time_factor", "set_time_factor" }) do
  move(level, name)
end
"#;

fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    scripts.register_class(time_class(ctx))?;
    scripts.register_class(timer_class(ctx))?;
    let lua = scripts.lua_mut();
    let c = ctx.clone();
    lua.register_with_objects("__volya_time_get_game_time", move |maker, _| {
        c.stopped()?;
        let t = c.game_time("game.get_game_time")?;
        Ok(vec![c.new_time(t, |part| maker.new_object("CTime", part))])
    })?;
    let c = ctx.clone();
    lua.register("__volya_time_time", move |_| {
        c.stopped()?;
        // The low 32 bits (3.3).
        let t = c.game_time("game.time")?;
        Ok(vec![f64::from(t as u32).into()])
    })?;
    type Part = fn(&Split) -> u32;
    let parts: [(&str, Part); 3] = [
        ("get_time_days", |s| s.day),
        ("get_time_hours", |s| s.hour),
        ("get_time_minutes", |s| s.minute),
    ];
    for (name, part) in parts {
        let c = ctx.clone();
        let what = format!("level.{name}");
        lua.register(&format!("__volya_time_{name}"), move |_| {
            c.stopped()?;
            let (t, _) = c.client_game_clock(&what)?;
            Ok(vec![f64::from(part(&split(t))).into()])
        })?;
    }
    let c = ctx.clone();
    lua.register("__volya_time_get_time_factor", move |_| {
        c.stopped()?;
        let (_, factor) = c.client_game_clock("level.get_time_factor")?;
        Ok(vec![f64::from(factor).into()])
    })?;
    let c = ctx.clone();
    lua.register("__volya_time_set_time_factor", move |args| {
        c.stopped()?;
        let Value::Number(f) = arg(args, 0) else {
            return Err("level.set_time_factor: number expected".into());
        };
        let mut times = c.times.borrow_mut();
        if times.runs_server {
            times.time_factors.push(to_f32(*f));
        }
        Ok(Vec::new())
    })?;
    let c = ctx.clone();
    lua.register("time_global", move |_| {
        c.stopped()?;
        // B 3.3's global timer, as `device():time_global()`.
        Ok(vec![
            f64::from(c.services.borrow().device.time_global()).into(),
        ])
    })?;
    lua.exec("=time", NAMESPACES).map(drop)
}

impl Scripts {
    /// Registers the time names (1) over `times`, with the device of
    /// `services` (B 3.3): before [`Scripts::load_globals`].
    pub fn register_time(
        &mut self,
        services: Rc<RefCell<EngineServices>>,
        times: Rc<RefCell<GameTimes>>,
    ) -> Result<(), Error> {
        let ctx = Context {
            services,
            times,
            fatal: self.fatal_flag(),
            known_times: Rc::default(),
            known_timers: Rc::default(),
        };
        register(self, &ctx)
    }
}
