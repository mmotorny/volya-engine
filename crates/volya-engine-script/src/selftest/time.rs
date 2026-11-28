// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-time.md on every target, one check per
//! section, over [`GameTimes`] fed as a level frame feeds them.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;

use super::ensure;
use super::loading::scripts_of;
use super::services::run;
use crate::ErrorKind;
use crate::loader::{Prefix, Scripts};
use crate::services::EngineServices;
use crate::time::{
    GameTimes, Split, Timer, compose, date_to_string, diff_sec, split, time_to_string,
    to_unsigned32,
};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 game time values", game_time_values),
    ("3 clocks", clocks),
    ("4 level time functions", level_functions),
    ("5.1 objects and constructors", constructors),
    ("5.2 setting and reading", setting_and_reading),
    ("5.3 arithmetic and difference", arithmetic),
    ("5.4 operators", operators),
    ("5.5 texts", texts),
    ("6 time_global", time_global),
    ("7 profile_timer", profile_timer),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("time {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// The New Game start, 01.05.2012 05:30:00 (2.1).
pub const NEW_GAME: u64 = 63_471_533_400_000;

struct State {
    s: Scripts,
    services: Rc<RefCell<EngineServices>>,
    times: Rc<RefCell<GameTimes>>,
}

/// A game script state with the services and the time names registered
/// and `_G` loaded; the clocks at the New Game start at factor 10 unless
/// `level` is false.
fn state(level: bool) -> Result<State, String> {
    let (mut s, _) = scripts_of(&[("_g.script", "")], Prefix::Default)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(b"").map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system)));
    let times = Rc::new(RefCell::new(GameTimes::new()));
    if level {
        times.borrow_mut().level_frame(NEW_GAME, 10.0);
    }
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    s.register_time(Rc::clone(&services), Rc::clone(&times))
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    Ok(State { s, services, times })
}

/// `code` must stop the state with a fatal error whose report's rows
/// include `rows`, in the log.
fn fatal(level: bool, code: &str, rows: &[&str]) -> Result<(), String> {
    let mut st = state(level)?;
    let result =
        st.s.exec("=fatal", format!("pcall(function() {code} end)").as_bytes());
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: {result:?}, not fatal"),
    )?;
    let log = st.services.borrow().console.log().join("\n");
    for row in rows {
        ensure(log.contains(row), || {
            format!("{code}: no {row:?} in the log:\n{log}")
        })?;
    }
    Ok(())
}

/// Section 1: what is registered.
pub fn registered() -> Result<(), String> {
    let mut st = state(true)?;
    run(
        &mut st.s,
        "1",
        r##"
local class_mt = getmetatable(profile_timer)
eq(getmetatable(game.CTime) == class_mt, true, "game.CTime is a class")
eq(rawget(_G, "CTime"), nil, "no global CTime")
eq(getmetatable(profile_timer) ~= nil, true, "profile_timer is a class")
for _, f in ipairs({"get_game_time", "time"}) do eq(type(game[f]), "function", "game." .. f) end
for _, f in ipairs({"get_time_days", "get_time_hours", "get_time_minutes", "get_time_factor",
    "set_time_factor"}) do
  eq(type(level[f]), "function", "level." .. f)
end
eq(type(time_global), "function", "time_global")
local constants = { TimeToHours = 0, TimeToMinutes = 1, TimeToSeconds = 2, TimeToMilisecs = 3,
  DateToDay = 0, DateToMonth = 1, DateToYear = 2 }
for k, v in pairs(constants) do eq(game.CTime[k], v, k) end
"##,
    )
}

/// The dates of `year` as (month, day).
fn dates(year: u32) -> impl Iterator<Item = (u32, u32)> {
    let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    (1..=12u32).flat_map(move |m| (1..=lengths[m as usize - 1]).map(move |d| (m, d)))
}

/// Section 2: compose and split, the worked examples.
pub fn game_time_values() -> Result<(), String> {
    ensure(compose(2012, 5, 1, 5, 30, 0, 0) == NEW_GAME, || {
        "the New Game start".into()
    })?;
    ensure(
        compose(2012, 12, 31, 0, 0, 0, 0) == compose(2013, 1, 1, 0, 0, 0, 0),
        || "31.12.2012 is 01.01.2013".into(),
    )?;
    ensure(
        compose(2012, 1, 1, 0, 0, 0, 0) - compose(2011, 12, 31, 0, 0, 0, 0) == 2 * 86_400_000,
        || "01.01.2012 is two days after 31.12.2011".into(),
    )?;
    ensure(
        compose(2011, 1, 1, 0, 0, 0, 0) - compose(2010, 12, 31, 0, 0, 0, 0) == 86_400_000,
        || "01.01.2011 is a day after 31.12.2010".into(),
    )?;
    // A month above 12 adds all eleven; day 0 adds 2³² − 1.
    ensure(
        compose(2010, 13, 1, 0, 0, 0, 0) == compose(2010, 12, 1, 0, 0, 0, 0),
        || "month 13".into(),
    )?;
    ensure(
        compose(2010, 1, 0, 0, 0, 0, 0)
            == compose(2010, 1, 1, 0, 0, 0, 0).wrapping_add(u64::from(u32::MAX) * 86_400_000),
        || "day 0".into(),
    )?;
    let at = |year, month, day, hour, minute| Split {
        year,
        month,
        day,
        hour,
        minute,
        second: 0,
        ms: 0,
    };
    ensure(split(NEW_GAME) == at(2012, 5, 1, 5, 30), || {
        format!("split of the New Game start: {:?}", split(NEW_GAME))
    })?;
    ensure(split(43_200_000) == at(1, 1, 1, 12, 0), || {
        format!("split of 43,200,000: {:?}", split(43_200_000))
    })?;
    let after = compose(2011, 12, 31, 0, 0, 0, 0) + 86_400_000;
    ensure(split(after) == at(2011, 12, 32, 0, 0), || {
        format!("the day after 31.12.2011: {:?}", split(after))
    })?;
    // 1990–2040: one date each splits to another, except 13 years.
    const EXACT: [u32; 13] = [
        1992, 1996, 2001, 2005, 2009, 2013, 2017, 2021, 2025, 2029, 2033, 2037, 2038,
    ];
    for year in 1990..=2040 {
        let differing: Vec<(u32, u32)> = dates(year)
            .filter(|&(m, d)| {
                let s = split(compose(year, m, d, 0, 0, 0, 0));
                (s.year, s.month, s.day) != (year, m, d)
            })
            .collect();
        let want = usize::from(!EXACT.contains(&year));
        ensure(differing.len() == want, || {
            format!("{year}: dates that split to others {differing:?}")
        })?;
    }
    Ok(())
}

/// Section 3: `game.get_game_time()` and `game.time()` on the clocks, and
/// without them.
pub fn clocks() -> Result<(), String> {
    let mut st = state(true)?;
    run(
        &mut st.s,
        "3",
        r##"
local t = game.get_game_time()
eq(t:timeToString(game.CTime.TimeToSeconds), "05:30:00", "the New Game start")
eq(rawequal(t, game.get_game_time()), false, "a new CTime each call")
eq(game.time(), 506699712, "the low 32 bits")
"##,
    )?;
    // The simulation clock first, then the client game's.
    st.times.borrow_mut().simulation_clock = None;
    st.times.borrow_mut().client_game_clock = Some((NEW_GAME + 1000, 10.0));
    run(
        &mut st.s,
        "3 client",
        r##"eq(game.get_game_time():timeToString(2), "05:30:01", "the client game clock")"##,
    )?;
    st.times.borrow_mut().simulation_clock = Some(4_294_967_296 + 7);
    run(
        &mut st.s,
        "3 wrap",
        r##"eq(game.time(), 7, "wraps every 2^32 ms")"##,
    )?;
    fatal(false, "game.get_game_time()", &["there is no game clock"])?;
    fatal(false, "game.time()", &["there is no game clock"])
}

/// Section 4: the `level` time functions.
pub fn level_functions() -> Result<(), String> {
    let mut st = state(true)?;
    run(
        &mut st.s,
        "4",
        r##"
eq(level.get_time_days(), 1, "days")
eq(level.get_time_hours(), 5, "hours")
eq(level.get_time_minutes(), 30, "minutes")
eq(level.get_time_factor(), 10, "factor")
level.set_time_factor(10000)
level.set_time_factor(0.1)
eq(level.get_time_factor(), 10, "the client game clock changes later")
"##,
    )?;
    let factors = st.times.borrow_mut().take_time_factors();
    ensure(factors == [10000.0, 0.1f32], || {
        format!("factors set: {factors:?}")
    })?;
    st.times.borrow_mut().runs_server = false;
    run(&mut st.s, "4 client", "level.set_time_factor(3)")?;
    let factors = st.times.borrow_mut().take_time_factors();
    ensure(factors.is_empty(), || {
        format!("without the server: {factors:?}")
    })?;
    fatal(false, "level.get_time_hours()", &["there is no level"])
}

/// Section 5.1: the constructors and the number conversion.
pub fn constructors() -> Result<(), String> {
    for (n, want) in [
        (1.9, 1),
        (-1.5, u32::MAX),
        (-0.5, 0),
        (f64::NAN, 1 << 31),
        (3e9, 1 << 31),
        (-3e9, 1 << 31),
        (2_147_483_647.5, i32::MAX as u32),
    ] {
        ensure(to_unsigned32(n) == want, || {
            format!("{n} converts to {}", to_unsigned32(n))
        })?;
    }
    let mut st = state(true)?;
    run(
        &mut st.s,
        "5.1",
        r##"
local zero = game.CTime()
eq(zero:timeToString(4), "0d 00:00:00", "game.CTime() holds 0")
local t = game.get_game_time()
local copy = game.CTime(t)
t:setHMS(1, 2, 3)
eq(copy:timeToString(2), "05:30:00", "a copy")
eq(t:timeToString(2), "01:02:03", "the original changed")
local d = game.CTime()
d:setHMSms(0, 0, 0, -1.5)
eq(d:diffSec(game.CTime()), 4294967.5, "-1.5 converts to 2^32 - 1")
raises(function() game.CTime(1) end, "no constructor", "a number is no CTime")
"##,
    )
}

/// Section 5.2: `setHMS`, `setHMSms`, `set` and `get`.
pub fn setting_and_reading() -> Result<(), String> {
    let mut st = state(true)?;
    run(
        &mut st.s,
        "5.2",
        r##"
local t = game.CTime()
t:setHMS(25, 0, 0)
eq(t:timeToString(4), "1d 01:00:00", "a duration of day 0")
t:setHMSms(1, 2, 3, 7)
eq(t:timeToString(3), "01:02:03:07", "with milliseconds")
t:set(2012, 5, 1, 5, 30, 0, 0)
eq(t:timeToString(4), "734624d 05:30:00", "set composes")
local y, m, d, h, mi, s, ms = t:get(0, 0, 0, 0, 0, 0, 0)
eq(y, 2012, "Y") eq(m, 5, "M") eq(d, 1, "D") eq(h, 5, "h") eq(mi, 30, "m") eq(s, 0, "s")
eq(ms, 0, "ms")
raises(function() t:get() end, "number expected", "get takes 7 numbers")
"##,
    )
}

/// Section 5.3: `add`, `sub`, `diffSec`, a nil argument.
pub fn arithmetic() -> Result<(), String> {
    ensure(
        diff_sec(1500, 0) == (1500.0 * f64::from(0.001f32)) as f32,
        || format!("diffSec of 1500 ms: {}", diff_sec(1500, 0)),
    )?;
    ensure(diff_sec(0, 1500) == -diff_sec(1500, 0), || {
        "negative when later".into()
    })?;
    let same = diff_sec(5, 5);
    ensure(same == 0.0 && same.is_sign_negative(), || {
        format!("equal values give -0: {same}")
    })?;
    let mut st = state(true)?;
    run(
        &mut st.s,
        "5.3",
        r##"
local a, b = game.CTime(), game.CTime()
a:setHMS(1, 0, 0)
b:setHMS(0, 30, 0)
a:add(b)
eq(a:timeToString(1), "01:30", "add")
a:add(a)
eq(a:timeToString(1), "03:00", "add itself")
b:sub(a)
eq(b:timeToString(4), "0d 00:00:00", "sub clamps at 0")
a:sub(game.CTime())
eq(a:timeToString(1), "03:00", "sub")
local c = game.CTime()
c:setHMS(0, 0, 1)
eq(c:diffSec(game.CTime()), 1, "diffSec")
eq(game.CTime():diffSec(c), -1, "diffSec negative")
raises(function() a:add(1) end, "CTime expected", "a number is no CTime")
"##,
    )?;
    fatal(true, "game.CTime():add(nil)", &["a nil CTime"])
}

/// Section 5.4: the operators.
pub fn operators() -> Result<(), String> {
    let mut st = state(true)?;
    run(
        &mut st.s,
        "5.4",
        r##"
local a, b = game.CTime(), game.CTime()
a:setHMS(2, 0, 0)
b:setHMS(1, 0, 0)
eq(b < a, true, "<") eq(a < b, false, "not <")
eq(b <= b, true, "<=")
eq(a > b, true, ">") eq(a >= a, true, ">=")
eq(a == game.CTime(a), true, "==") eq(a == b, false, "~=")
eq((a + b):timeToString(1), "03:00", "+")
eq((a - b):timeToString(1), "01:00", "-")
local wrapped = b - a
eq(wrapped > a, true, "- wraps without clamping")
eq(a:__gt(b), true, "__gt by name") eq(a:__ge(b), true, "__ge by name")
eq(rawequal(a + b, a), false, "+ makes a new CTime")
eq(game.CTime(a + b):timeToString(1), "03:00", "a result copied")
"##,
    )
}

/// Section 5.5: `timeToString` and `dateToString`.
pub fn texts() -> Result<(), String> {
    let want = ["05", "05:30", "05:30:00", "05:30:00:00", "734624d 05:30:00"];
    for (p, w) in want.iter().enumerate() {
        let got = time_to_string(NEW_GAME, p as u32);
        ensure(got.as_deref() == Some(*w), || {
            format!("timeToString({p}): {got:?}")
        })?;
    }
    for (p, w) in ["01/05/2012", "05/2012", "2012"].iter().enumerate() {
        let got = date_to_string(NEW_GAME, p as u32);
        ensure(got.as_deref() == Some(*w), || {
            format!("dateToString({p}): {got:?}")
        })?;
    }
    ensure(
        time_to_string(7, 3).as_deref() == Some("00:00:00:07"),
        || "ms as %02i".into(),
    )?;
    ensure(
        date_to_string(43_200_000, 2).as_deref() == Some("0001"),
        || "the year as %04i".into(),
    )?;
    ensure(
        time_to_string(0, 5).is_none() && date_to_string(0, 3).is_none(),
        || "unknown precisions".into(),
    )?;
    let mut st = state(true)?;
    run(
        &mut st.s,
        "5.5",
        r##"
local t = game.get_game_time()
eq(t:dateToString(game.CTime.DateToDay), "01/05/2012", "DateToDay")
eq(t:timeToString(game.CTime.TimeToMilisecs), "05:30:00:00", "TimeToMilisecs")
eq(t:timeToString(2.9), "05:30:00", "the precision converted")
"##,
    )?;
    let rows = [
        "[error]Expression    : assertion failed",
        "[error]Description   : !\"Unknown type of date precision\"",
    ];
    fatal(true, "game.CTime():timeToString(5)", &rows)?;
    fatal(true, "game.CTime():dateToString(-1)", &rows)
}

/// Section 6: `time_global()` is the device's global timer.
pub fn time_global() -> Result<(), String> {
    let mut st = state(true)?;
    st.services.borrow_mut().device.frame(12_345, 0.016, false);
    run(
        &mut st.s,
        "6",
        r##"
eq(time_global(), 12345, "the global timer")
eq(time_global(), device():time_global(), "as device():time_global()")
"##,
    )?;
    st.services
        .borrow_mut()
        .device
        .frame(4_294_967_296 + 5, 0.016, false);
    run(
        &mut st.s,
        "6 wrap",
        r##"eq(time_global(), 5, "in [0, 2^32)")"##,
    )
}

/// Section 7: `profile_timer`.
pub fn profile_timer() -> Result<(), String> {
    let mut t = Timer::default();
    t.start(100);
    t.start(150);
    t.stop(200);
    ensure(t.depth == 1 && t.accumulated == 0, || {
        format!("nested: {t:?}")
    })?;
    t.stop(300);
    ensure(
        t.depth == 0 && t.accumulated == 200 && t.intervals == 1,
        || format!("an interval: {t:?}"),
    )?;
    t.start(400);
    t.stop(350);
    ensure(t.accumulated == 200 && t.intervals == 2, || {
        format!("the counter not moved forward: {t:?}")
    })?;
    ensure(t.time() == 200.0, || format!("time(): {}", t.time()))?;
    let mut lone = Timer::default();
    lone.stop(10);
    lone.start(20);
    ensure(lone.depth == 0 && lone.intervals == 0, || {
        format!("a stop without a start: {lone:?}")
    })?;
    let mut st = state(true)?;
    run(
        &mut st.s,
        "7",
        r##"
local a = profile_timer()
eq(a:time(), 0, "a new timer")
a:start()
local running = profile_timer(a)
a:stop()
local b = profile_timer()
local sum = a + b
eq(sum:time(), a:time(), "+ sums")
eq(rawequal(sum, a), false, "+ makes a new timer")
eq(b < a or b:time() == a:time(), true, "<")
eq(b < b, false, "not <")
local copy = a:__tostring()
eq(copy:time(), a:time(), "__tostring copies")
eq(type(copy), "userdata", "__tostring gives a timer")
running:stop()
eq(running:time() >= 0, true, "a copy keeps the running state")
"##,
    )
}
