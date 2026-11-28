# Script API H: time

What the time names of the game scripts' interface do in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the class `CTime` (reached as `game.CTime`), the functions `game.get_game_time` and `game.time`, the
global `time_global`, the `level` functions `get_time_days`, `get_time_hours`, `get_time_minutes`,
`get_time_factor` and `set_time_factor`, and the class `profile_timer`; and, as far as these names need it, the
game's clocks they read and set. This is spec H of the approved scoping specification
`docs/behavior/script-api-scope.md` (section 6, order 7), which assigns these names to it; its scope rule
applies: this spec owns every name of these classes and functions as the binary registers them, listed or
not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md` (the runtime, LuaJIT 1.0.3; one number type, the 64-bit double, its
  section 2.5), `docs/behavior/script-loading.md` (the 250 loadable scripts, its section 3).
- `docs/formats/sav.md`: game time as a `u64` count of milliseconds and its calendar formula (section 12), the
  game-time chunk with its time factor and normal time factor (section 5).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are
  matched, what a mismatched call does; its operator list, section 7.2). Where a statement needs A's ground it
  says **depends on A**.
- Spec E, `docs/behavior/script-api-value-types-and-math.md`: its **f32**, its rule for **X[…]** (x87
  computations, section 2.5) and its argument conversion "To f32" (section 2.2).
- Spec B, `docs/behavior/script-api-engine-services.md`: the render device's global timer and
  `device():time_global()` (its section 3.3), the game pause (3.5), the game type (4.5).
- `docs/behavior/console-log.md` section 3.8 (the fatal-error report).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the
  runtime library" `bin\xrLUA.dll` (290,816 bytes, MD5 `a65ba53867962afc92e0f17409e9cd1f`) [HT-1].
- **The listing** is `scripts\lua_help.script` (script-api-scope 2.1), a guide only; every member below is
  established from the binary's registration code.
- A **time value** is an unsigned 64-bit count of milliseconds (sav.md 12). Arithmetic "mod 2⁶⁴" wraps as
  unsigned 64-bit integers do. A **number** is the runtime's double; a **text** a Lua string.
- **Compose**(Y, M, D, h, m, s, ms) is sav.md section 12's formula, as the game computes it (section 2.1);
  **split** is its inverse as the game computes it (section 2.2).
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (read
  from game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary
  was run.
- "Fatal error" means the program ends through the fatal-error report of console-log.md section 3.8.

**Checks.** Every claim carries a tag [HT-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-time/run.py` (`python3 -I run.py`): it reads the binaries (bytes, import tables, data
values), the evidence of read-only headless static analysis of them (produced by `ghidra.py` beside it into
`bin/`), the listing and the shipped scripts. All checks pass (`ALL PASS 14`, output in `run.out`).

## 1. What is registered

The listing gives this spec 30 items: the classes `CTime` (7 constants, 9 functions; it also lists 7
operators, which the scoping spec does not count) and `profile_timer` (4 functions, 2 operators), the
functions `time` and `get_game_time` of the namespace `game`, the 5 `level` functions, and the global
`time_global` [HT-2]. The binary registers exactly these, with no property for either class [HT-3]:

| Name | Where | Arguments → result | Section |
|---|---|---|---|
| `CTime` | class in the namespace `game` | constructors `game.CTime()`, `game.CTime(t)`; constants `TimeToHours` 0, `TimeToMinutes` 1, `TimeToSeconds` 2, `TimeToMilisecs` 3, `DateToDay` 0, `DateToMonth` 1, `DateToYear` 2 | 5 |
| | methods | `add(t)`, `sub(t)`, `diffSec(t)` → number, `setHMS(h, m, s)`, `setHMSms(h, m, s, ms)`, `set(Y, M, D, h, m, s, ms)`, `get(…)` → 7 numbers, `timeToString(p)` → text, `dateToString(p)` → text | 5 |
| | operator methods | `__lt`, `__le`, `__gt`, `__ge`, `__eq` → boolean; `__add`, `__sub` → a new `CTime` | 5.4 |
| `game.get_game_time()` | namespace `game` | → a new `CTime` | 3.1 |
| `game.time()` | namespace `game` | → number | 3.3 |
| `level.get_time_days()`, `level.get_time_hours()`, `level.get_time_minutes()` | namespace `level` | → number | 4.1 |
| `level.get_time_factor()` | namespace `level` | → number | 4.2 |
| `level.set_time_factor(f)` | namespace `level` | → nothing | 4.2 |
| `time_global()` | global | → number | 6 |
| `profile_timer` | global class | constructors `profile_timer()`, `profile_timer(t)`; methods `start()`, `stop()`, `time()` → number; operator methods `__add` → a new timer, `__lt` → boolean, `__tostring` | 7 |

The scoping spec's two candidate strings for this spec, `__add` and `__lt`, are registrations: the operator
methods above [HT-3]. The namespace `game` also holds names owned by other specs (`translate_string`,
`has_active_tutorial`, `start_tutorial`, and further ones) [HT-3]. How a call with arguments of the wrong
number or type is rejected **depends on A**.

## 2. Game time values

### 2.1 Compose

A date and time of day become a time value by sav.md section 12's formula, computed in unsigned 64-bit
arithmetic with Y, ⌊Y/4⌋, ⌊Y/100⌋, ⌊Y/400⌋ taken from the 32-bit year [HT-4]: the day count is
(Y − 1)·365 − ⌊Y/100⌋ + ⌊Y/4⌋ + ⌊Y/400⌋, plus the lengths of the months before M (31, 28 or 29 for February in
a leap year, 31, 30, 31, 30, 31, 31, 30, 31, 30; a month number above 12 adds all eleven), plus D − 1 (as a
32-bit unsigned number, so D = 0 adds 2³² − 1); then time = ((days·24 + h)·60 + m)·60·1000 + s·1000 + ms
[HT-4]. Consequences (from this formula, [HT-4]): the New Game start 01.05.2012 05:30:00 is
63,471,533,400,000; 31.12.2012 and 01.01.2013 give the same value; 01.01.2012 is two days after 31.12.2011;
in other years 1 January follows 31 December by one day.

### 2.2 Split

A time value t becomes (Y, M, D, h, m, s, ms) thus (static) [HT-4]:

1. ms = t mod 1000; then with t′ = ⌊t/1000⌋: s = t′ mod 60, m = ⌊t′/60⌋ mod 60, h = ⌊t′/3600⌋ mod 24, and
   **days** = ⌊t′/86400⌋.
2. q = ⌊(days·400 mod 2⁶⁴) / 146097⌋, kept as a 32-bit unsigned number; Y = q + 1.
3. d = days − (q·365 mod 2³²) − ⌊Y/4⌋ + ⌊Y/100⌋ − ⌊Y/400⌋ + 1, a signed 64-bit day of the year (Y's leap rule:
   divisible by 4 and not by 100, or by 400). **Disputed in review:** q·365 is not taken mod 2³²: its full
   64-bit product is subtracted (the high part with the borrows), so d = days − q·365 − ⌊Y/4⌋ + ⌊Y/100⌋ −
   ⌊Y/400⌋ + 1 in 64-bit arithmetic; the two differ only when q·365 ≥ 2³² (years above about 11.7 million).
4. M = 1; while M < 12 and d (compared as unsigned 64-bit) exceeds the length of month M (31, 28 + 1 if Y is
   a leap year, 31, 30, 31, 30, 31, 31, 30, 31, 30), d is reduced by that length and M increases by 1.
5. D = d's low 32 bits.

Splitting a composed date does not always give the date back: for each year 1990–2040 except 13 (1992, 1996,
2001, 2005, 2009, 2013, 2017, 2021, 2025, 2029, 2033, 2037, 2038) exactly one date differs: in a leap year
31 December splits to 1 January of the next year; on 1 January after a leap year to 31 December of that leap
year; on 1 January of other years to "32 December" of the year before (D = 32) [HT-4]. Examples: split of
63,471,533,400,000 is 2012, 5, 1, 5, 30, 0, 0; split of 43,200,000 is 1, 1, 1, 12, 0, 0, 0; the day after
31.12.2011 00:00 splits to 2011, 12, 32 [HT-4].

### 2.3 Readings of a clock

Every clock below holds a **base** (a time value), a **start** (a reading of a real-time source in
milliseconds) and a **factor** (an f32), and is read as

    base + F(X[(now − start) × factor])

where now is the source's current reading, the elapsed milliseconds (now − start as an unsigned 32-bit
difference for the simulation clock, 3.1, and as an unsigned 64-bit one for the game objects' clocks, 3.2)
enter the x87 unit exactly (a 32-bit difference of 2³¹ or more is first rounded to 24 bits), the product is rounded to
f32 (E 2.5: X[…], then stored as f32), and F is a conversion of that f32 to a signed 32-bit integer, sign
extended and added mod 2⁶⁴ [HT-5, HT-6]. F is the floor for values in (−2³¹, 2³¹); for a value in [2³¹, 2³²)
it is the floor minus 2³²; for larger values it is a small number made of the value's top bits (2³² gives 1,
3·2³² gives 3, 2³³ gives 2) [HT-5]. So a clock reads without jumping back while (now − start) × factor stays below 2³¹
game milliseconds (24 days 20:31:23.648 of game time since its start was last set; at factor 10, about
2.5 real days of play without one of the events of 3.1); beyond that its reading jumps back (believed
reachable: nothing else resets the start).

"Setting a clock's factor to f" means: base = the clock's current reading, start = the source's current
reading, factor = f; so the reading continues without a jump and runs at the new factor from then on [HT-5,
HT-6].

## 3. Clocks

### 3.1 The simulation clock and `game.get_game_time()`

The simulation (offline-simulation.md) keeps the **simulation clock**, whose source is the render device's
global timer (B 3.3: it stops while the game is paused), with the time factor and the normal time factor of
sav.md section 5 [HT-5]:

- **New Game**: base = compose(`start_date`, `start_time`, ms 0) of `config\alife.ltx` `[alife]` (read with
  the formats `%d.%d.%d` as day, month, year and `%d:%d:%d` as hours, minutes, seconds), factor =
  `time_factor`, normal factor = `normal_time_factor`, start = the global timer's current value [HT-5].
- **Loading** a save: base, factor and normal factor from the game-time chunk (sav.md 5); start = the global
  timer's current value. A save without the chunk is a fatal error (a failed check with a message) [HT-5].
- **Saving**: the clock is first rebased (base = its current reading, start = now, factor unchanged), then the
  chunk is written with base, factor and normal factor [HT-5].
- **Changing the factor** (4.2): as "setting a clock's factor" of 2.3 [HT-5].

`game.get_game_time()` returns a new `CTime` (section 5) holding the simulation clock's reading when the
simulation exists, and otherwise the client game clock's reading (3.2) [HT-7]; with no level either, the
call reads through no object (**believed** a crash).

### 3.2 The game objects' clocks

A level has two game objects, the server side's and the client side's; each has a clock of the form of 2.3
whose source is a millisecond clock of the level's network client (**believed** its estimate of the server's
time) [HT-6]. How that source relates to the device's global timer, and whether it stops while the
game is paused, is **unestablished**.

- A game object starts with base 43,200,000 (01.01.0001 12:00 by split), factor = `[alife] time_factor` of
  the settings (10 in the shipped data), start = its source's reading [HT-6].
- The **server game's reading** is the simulation clock's when the simulation exists and a flag of it is set
  (which state the flag marks is **unestablished**), and otherwise its own clock's; the same choice gives its
  factor [HT-5, HT-6].
- The **client game clock** is set from the server game: whenever the server sends the client the state of
  the game, the state ends with the server game's reading and factor (and an environment time and factor);
  on receipt the client game's clock gets base = that reading, start = the source's current reading, factor =
  that factor [HT-6]. When the server sends the state is **unestablished** here. So the client game clock
  follows the server game's from one state message to the next (**believed**: both run at the same factor
  between messages).
- In a single-player game the client game's environment time and factor are its game clock's reading and
  factor; the level's per-frame update hands the environment (the weather, `sky.md`) the time of day as an f32
  number of seconds, X[(reading mod 86,400,000) × f32(0.001)], and the factor [HT-6] (**believed** once per
  frame: the same routine runs the engine's per-frame level update).

### 3.3 `game.time()`

`game.time()` reads the same clock as `game.get_game_time()` (3.1) and returns only the **low 32 bits** of the
time value, as a number in [0, 2³²) [HT-7]. At the New Game start that is 506,699,712 (63,471,533,400,000
mod 2³²), and the value wraps to 0 every 2³² game milliseconds (49 days 17:02:47.296 of game time) [HT-7].
`_g.script` uses it as a game-millisecond clock (a time of day as `math.mod(game.time(), ms_per_day)`, and the
wait of 4.2) (measured) [HT-14].

## 4. `level` time functions

### 4.1 Time of day

`level.get_time_days()`, `level.get_time_hours()` and `level.get_time_minutes()` split (2.2) the client game
clock's reading (3.2) and return its D (the day of the month, not a count of days), h and m respectively
[HT-8]. Example: at 01.05.2012 05:30 they give 1, 5 and 30 [HT-4, HT-8]. Before the client game clock has
received a state from the server they read its starting clock (3.2).

### 4.2 Time factor

- `level.get_time_factor()` returns the client game clock's factor (3.2), an f32 [HT-8].
- `level.set_time_factor(f)` (f converted as E 2.2 "To f32", **believed**: the parameter is an f32) does
  nothing unless the level runs the server: the level's server exists and has at least one client [HT-8]
  (**believed** always so in a single-player game). In a further mode of the level, selected by two of its
  states, a third state decides instead; which modes these are is **unestablished** (**believed** the
  playback of a recorded demo). When it acts, it sets the server game's factor: when the simulation exists and
  its flag (3.2) is set, the simulation clock's factor is set to f (2.3), otherwise the server game's own
  clock's [HT-5, HT-8]. The normal time factor is not changed. The client game clock (and with it
  `get_time_factor()`, the `level` times and the weather) changes only when the next state message reaches
  the client (3.2).

`_g.script` reads `level.get_time_factor()`, sets the factor to 10000, waits until `game.time()` reaches a
target, and sets the factor back to the value it read (measured) [HT-14].

## 5. `CTime`

### 5.1 Objects and constructors

A `CTime` object holds one time value [HT-3, HT-9]. `game.CTime()` makes one holding 0; `game.CTime(t)` makes
a copy of t [HT-9]. The class constants (section 1) are the precision arguments of `timeToString` and
`dateToString` (5.5) [HT-3]. Number arguments of the methods below are converted to 32-bit unsigned numbers
by truncation toward zero; a number in (−2³¹, 0) gives 2³² plus its truncation (−1.5 gives 2³² − 1); NaN and
numbers outside (−2³¹, 2³¹) give 2³¹ (**believed**: on a processor with SSE2, which takes that path of the
conversion; without SSE2 the number is truncated to a 64-bit integer whose low 32 bits are used) [HT-9].

### 5.2 Setting and reading

- `setHMS(h, m, s)` replaces the value by ((h·60 + m)·60 + s)·1000; `setHMSms(h, m, s, ms)` by
  ((h·60 + m)·60 + s)·1000 + ms (unsigned 64-bit arithmetic from the 32-bit arguments) [HT-9]. The date part
  becomes day 0, so the result is a duration (the scripts add it to a game time).
- `set(Y, M, D, h, m, s, ms)` replaces the value by compose(Y, M, D, h, m, s, ms) (2.1) [HT-9].
- `get(a1, …, a7)` returns the 7 numbers of split (2.2) of the value: Y, M, D, h, m, s, ms, in that order. It
  takes 7 number arguments whose values are not used (the call matches only with 7 numbers) [HT-9].
  `utils.script` writes a time as `get`'s Y − 2000, M, D, h, m, s (8 bits each) and ms (16 bits), and reads it
  back with `set(Y + 2000, …)`, an absent time, or one equal to the script's own zero time `CTime_0`, as a single 0 (measured) [HT-14].

### 5.3 Arithmetic and difference

- `add(t)`: value = value + t's value, mod 2⁶⁴ [HT-9].
- `sub(t)`: value = value − t's value when the value is greater than t's, and 0 otherwise [HT-9].
- `diffSec(t)`: (value − t's value) / 1000 as a number of seconds, negative when t is later: the absolute
  difference, converted exactly to the x87 unit's extended format, times f32(0.001), negated when t's value is
  the greater or equal, returned as an f32 (E 2.5) [HT-9].
- What a nil argument does is **unestablished** (**believed** a crash: the value is read through no object).

### 5.4 Operators

Each `CTime` object carries the metatable the runtime library gives objects of engine classes, which has the
12 operator entries of A 7.2's names [HT-11]: applying an operator looks the operator's name up as a member of the first
operand, then of the second, and calls the first found with the operands in their original order, keeping
one result; with neither, the error is `No such operator defined` [HT-11]. `CTime`'s members:

| Member | Result for (a, b) |
|---|---|
| `__lt` | `true` when a's value < b's value |
| `__le` | `true` when a's value ≤ b's value |
| `__eq` | `true` when the values are equal |
| `__add` | a new `CTime` holding a's value + b's value, mod 2⁶⁴ |
| `__sub` | a new `CTime` holding a's value − b's value, mod 2⁶⁴ (no clamping, unlike `sub`) |
| `__gt`, `__ge` | `true` when a's value > b's value, ≥ b's value |

[HT-9]. Comparisons are of the unsigned 64-bit values. `__gt` and `__ge` are not among the 12 operator
entries, so they are reached only when called by name; `a > b` and `a >= b` reach `__lt` and `__le` with the
operands exchanged (**believed**: Lua's documented evaluation of `>` and `>=`). Comparing or adding a
`CTime` and a value of another type is decided by the runtime's rules for metamethods (**unestablished**).

### 5.5 `timeToString(p)` and `dateToString(p)`

Both split (2.2) the value and return a new text; numbers are written as decimal integers, `%02i` meaning at
least two digits with leading zeros [HT-10]:

| Call | Text |
|---|---|
| `timeToString(0)` (`TimeToHours`) | h as `%02i` |
| `timeToString(1)` (`TimeToMinutes`) | h `:` m |
| `timeToString(2)` (`TimeToSeconds`) | h `:` m `:` s |
| `timeToString(3)` (`TimeToMilisecs`) | h `:` m `:` s `:` ms, ms also as `%02i` (so 7 ms is `07`, not `007`) |
| `timeToString(4)` (no constant) | ⌊value / 86,400,000⌋ as a signed 32-bit integer, `d`, a space, then h `:` m `:` s |
| `dateToString(0)` (`DateToDay`) | D `/` M `/` Y, Y as at least four digits (`%04i`) |
| `dateToString(1)` (`DateToMonth`) | M `/` Y |
| `dateToString(2)` (`DateToYear`) | Y as `%04i` |

h, m, s, ms, D and M are `%02i` [HT-10]. The precision is a number converted as in 5.1 [HT-10]; any other
precision is a fatal error,
a failed check without a message whose checked condition is `!"Unknown type of date precision"` [HT-10]. At
the New Game start the texts are `05`, `05:30`, `05:30:00`, `05:30:00:00`, `734624d 05:30:00`, `01/05/2012`,
`05/2012` and `2012` [HT-10]. `game_stats.script` uses `game.get_game_time():timeToString(game.CTime.TimeToSeconds)`;
only `test_script.script` (not loadable) calls `dateToString` (measured) [HT-14].

## 6. `time_global()`

Returns the render device's `time_global()` value (B 3.3), the global timer in milliseconds as a number in
[0, 2³²): the same value as `device():time_global()` [HT-12]. It stops while the game is paused (B 3.5) and
runs at the device's own time factor (B 3.3), which `level.set_time_factor` does not set (4.2) [HT-8,
HT-12]. Because it is registered, `_g.script`'s replacement for an absent `time_global` (script-api-scope
section 5) is not defined (measured) [HT-14].

## 7. `profile_timer`

A **profile timer** measures real processor time with the processor's time-stamp counter [HT-13]. It holds a
start count, an accumulated count, a number of measured intervals and a nesting depth; `profile_timer()`
makes one with all four 0, `profile_timer(t)` a copy of t, including its running state [HT-13].

- `start()`: if the depth is 0, the depth becomes 1, the number of intervals increases by 1 and the start
  count becomes the counter's current value; otherwise the depth increases by 1 [HT-13].
- `stop()`: the depth decreases by 1; if it is then 0, the counter's current value minus the start count is
  added to the accumulated count, but only when the counter has moved forward [HT-13]. A `stop()` without a
  `start()` leaves the depth at −1, so the next `start()` only brings it back to 0 and starts nothing [HT-13].
- `time()`: the accumulated count (not including a running interval) in microseconds: X64[accumulated /
  counter frequency × 1,000,000], computed by the x87 unit at 64-bit precision and rounded to f32 [HT-13].
  The counter frequency (counts per second) is the core library's; how it is obtained is **unestablished**
  here.
- `__add(a, b)`: a new timer, not running (depth 0, start 0), whose accumulated count and number of intervals
  are the sums of a's and b's [HT-13].
- `__lt(a, b)`: `true` when a's accumulated count is less than b's [HT-13].
- `__tostring(t)`: returns a new timer, a copy of t, not a text [HT-13]; what Lua's `tostring` then gives is
  **unestablished**. No shipped script applies `tostring` to a timer (measured) [HT-14].

The only user is `profiler.script` (loadable), which sums timers with `+`, sorts them with `<` and prints
`time()/1000` as milliseconds (measured) [HT-14].

## 8. Script uses

Loadable scripts using each name (and all scanned scripts), comments removed (measured) [HT-14]:
`time_global()` 40 (57), `game.get_game_time` 15 (18), `:diffSec` 10 (12), `level.get_time_hours` 6 (7),
`game.time` 4 (8), `game.CTime` 4 (6), `:setHMSms` 2 (2), `level.get_time_minutes`, `level.get_time_days`,
`level.get_time_factor`, `level.set_time_factor`, `:timeToString`, `CTime:add`, `CTime:get`/`set`,
`profile_timer` 1 each; `:setHMS` and `:dateToString` 0 (2 and 1).

## 9. Unestablished

- How arguments are matched and what a mismatched call does (**depends on A**); the runtime's rules for
  operators between a `CTime` and another type (5.4); a nil `CTime` argument (5.3).
- The source of the game objects' clocks (the network client's server clock) and whether it stops while
  the game is paused (3.2).
- When the server sends the game state to the client, so how long the client game clock (and the `level`
  times, `get_time_factor` and the weather) lags a factor change (3.2, 4.2).
- Which simulation state the flag of 3.2 marks; the level modes of 4.2 other than "runs the server".
- The counter frequency of `profile_timer` (7); what `tostring` gives for a timer.
- Whether the clocks' 2³¹ limit (2.3) is reached in play.

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the binaries identified above;
  the archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier approved specs
  (`scripts\*.script`, `scripts\lua_help.script`).
- Analysis: static analysis with Ghidra (headless, read-only) of the game library's registration of these
  names and of the routines they reach (the calendar conversions, the clocks and their setting, saving and
  loading, the game-state message's time fields, the level's per-frame hand-over to the environment, the
  formatting of times and dates, the number conversions), and of the runtime library's metatable of engine
  objects and its operator dispatch; import tables and data values read by a throwaway PE reader; the
  calendar conversions and the float-to-integer conversion re-implemented from that analysis to compute the
  examples; the listing and the scripts read as text. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- How the render device's time factor (B 3.3, which assigns it to this spec), which scales the global timer
  and so `time_global()` and the simulation clock's source, is set and what its value is in a single-player
  game.
