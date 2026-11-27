# Wind sway of trees and its clock

Behaviour specification of the wind sway of the static world's trees and bushes (tree instances, visual
types 7 and 11 of `docs/formats/ogf.md`) with the first renderer: which clock drives it, how that clock
advances, its value at the start of a New Game and after loading a saved game, and every constant of the
sway that depends on it. It settles the clock left **unestablished** in `docs/formats/ogf.md` section 5
("*t* = the engine's global time in seconds") and states the sway's per-vertex rule, which the game's own
vertex programs hold. What could not be established is marked **unestablished**; what is inferred rather
than read from the program or the data is marked **believed**.

Builds on (not repeated here):

- `docs/formats/ogf.md` section 5: the tree instance's transform *M* (the world height of the tree's base
  is M[13]), the SHORT4 texture coordinate whose third value times 1/2048 is the vertex's **rigidity**, and
  the constants `wave`, `wind` and `consts` the first renderer passes to the tree vertex programs.
- `docs/behavior/sound-weather-ambient.md` section 3.1: the engine's **global clock** in milliseconds
  ("now"), the **pause** (one on/off state) and every event that turns it on or off, the game type, and
  what follows for the title screen, the main menu, loading, the New Game intro and the movies.
- `docs/behavior/startup.md` (launch to title menu) and `docs/behavior/loading-screen.md` section 2 (the
  frames drawn while a level loads).

Conventions: a frame is one frame of the engine; *t* is in seconds; "single precision" is IEEE 754
binary32; positions are world coordinates of the level files (metres, +y up). frac(*x*) = *x* − ⌊*x*⌋.

**Checks.** Every claim carries a tag W1 – W9. The checks are in the dirty workspace's `windclk/`:
`extract.py` extracts the tree vertex programs from the archives into `x/`, `export.py` regenerates the
evidence in `ev/` (headless Ghidra exports of the shipped program files, read-only), and `check.py` runs
every tag against `ev/`, `x/` and the program files themselves (constants are read from the files).
Statements taken from `docs/behavior/sound-weather-ambient.md` keep that specification's own tags and
are marked [SWA 3.1].

## 1. Summary

- The sway is driven by **the engine's global time in seconds**, *t*: the same timer as the global clock
  "now" of `docs/behavior/sound-weather-ambient.md` section 3.1, read as seconds in single precision once
  per frame, at the start of the frame [W3].
- *t* is real time elapsed since the engine was initialized at program start, minus the time the pause was
  on. Its rate is exactly real time (rate factor 1, never changed); the game time and its time factor do not
  reach it; it is not a sum of frame times [W3, W4, W5, W6].
- It is never reset or stored: not by a New Game, a level change, quitting to the menu or loading a saved
  game. Its value at any moment is a property of the program run, not of the game state, so there is no
  fixed value at a New Game's start (section 4) [W3, W4].
- With it, per frame, `wave` = (0.1, 0.01, 0.11, *t*) / (2π) and `wind` = 0.005 · (sin 0.2π*t*, 0,
  cos 0.2π*t*, 0), in single precision with the constants of section 2 [W1, W2]. The crown's bend has a
  period of 2π s ≈ 6.283 s in *t*; the bend's direction turns once per 10 s (section 5) [W8, W9].

## 2. The sway's inputs per frame

The first renderer keeps one shared copy of `wave` and `wind` (with `consts`) and recomputes it when it
sets a tree's constants in a frame whose frame number differs from the number stored with the copy; the
stored number starts at 0, and the engine increments its frame number at the start of every frame before it
reads *t*, so the first tree drawn in each frame recomputes the copy from that frame's *t*, and every tree
of the frame uses the same values [W1]. Nothing else is kept from frame to frame: the sway is a function of
the current frame's *t* alone [W1].

With *t* the frame's value (single precision):

- **Wind angle** *a* = *t* × *k*, *k* = 0.62831855 (single precision `0x3F20D97C`, the nearest single to
  0.2π), the product rounded to single precision [W1, W2].
- **`wind`** = (sin *a*, 0, cos *a*, 0) with the sine and the cosine each rounded to single precision;
  then divided by its length (the square root of the sum of the four squares; the square root and its
  reciprocal computed in extended precision and rounded to single), then each component multiplied by
  0.005 (`0x3BA3D70A`) [W1, W2]. The fourth component is 0. **Disputed in review:** "computed in
  extended precision" is not established: the x87 registers are used, but the precision they compute in
  depends on the x87 precision mode, which is unestablished (section 7). The sum of the four squares is
  computed in single precision.
- **`wave`** = (0.1, 0.01, 0.11, *t*) each multiplied by 0.15915494 (`0x3E22F983`, the nearest single to
  1/(2π)) in single precision; the three spatial values are `0x3DCCCCCD`, `0x3C23D70A`, `0x3DE147AE` before
  the multiplication [W1, W2].
- **`consts`** = (1/2048, 1/2048, 0, 0), 1/2048 = `0x3A000000` (unrelated to *t*) [W1, W2].

These constants are part of the renderer file and are only read; no setting or console command changes
them [W2]. The sine, cosine and square root are the processor's x87 instructions on single-precision
arguments [W1]; the x87 precision mode in effect when they run is **unestablished**; since every result is
rounded to single precision, the difference it can make is **believed** to be within a few units in the
last place of single precision.

## 3. The clock *t*

### 3.1 Definition

*t* is the elapsed time, in seconds, of the engine's **global timer**, converted to single precision [W3].
The global timer counts ticks of the Windows performance counter (`QueryPerformanceCounter`, frequency
from `QueryPerformanceFrequency`) [W5]:

*t* = (*C* − *C*₀ − *P* − *O*) / *F*, rounded to single precision,

- *C*: the counter read in this frame's update (while the pause is on: the value frozen when it was turned
  on, see 3.2) [W5, W6];
- *C*₀: the counter read when the global timer was started, at **engine initialization**, early in program
  start before the main window is created (`docs/behavior/sound-weather-ambient.md` section 3.1: "a timer
  of real time that starts when the engine is initialized (before the first frame)") [W4]. It is started
  there once and never again by any shipped program file [W4];
- *P*: the counter ticks that passed while the pause was on (3.2) [W6];
- *O*: the **counter overhead**, measured once, by the core library's first initialization at program
  start, as the mean, rounded down to an integer, of the difference between two back-to-back counter reads
  over 256 pairs [W5];
- *F*: the counter frequency [W5].

The division is done in the processor's extended precision and the result rounded to single precision when
stored [W3, W5]. **Disputed in review:** the division is done on the x87 registers, but "extended
precision" holds only if the x87 precision mode is the 64-bit one, which is unestablished (section 7). *t* is therefore not "the time since the program started" counted from counter zero: it
starts at 0 at engine initialization (derived from W4, W5).

The global clock "now" of `docs/behavior/sound-weather-ambient.md` (milliseconds) is the same timer, read a
second time in the same frame update just after *t*, as the whole number of milliseconds [W3, SWA 3.1]. So
*t* and now / 1000 differ by less than 1 ms plus the time between the two reads (derived).

### 3.2 How it advances

- **Real time.** Each frame's *t* is the timer read at that frame's start, not a sum of frame steps: time in
  which no frame runs (for example during loading) is counted in full at the next frame [W3, SWA 3.1].
- **Rate.** The timer's rate factor starts at 1.0 and nothing in the shipped program files changes it: the
  engine's setter of its two timers' rate is only exported and is used by no shipped file, and the game
  library imports no rate setter and no timer start [W5, W4]. With rate 1 the elapsed ticks are the raw
  ticks of 3.1 exactly [W5]. The game time (the time of day of `docs/behavior/sky.md`, scaled by the game's
  time factor, the console command `time_factor` and the scripts' time-factor functions) is a different
  clock and does not reach *t* [W4, W5, SWA 3.1].
- **Pause.** The global timer is a pausable timer, registered with the engine's one pause state, which is
  off when the program starts [W6]. When the pause turns on, the timer's reading freezes; when it turns off,
  the paused interval is added to *P*, so *t* continues from the frozen value; paused time is never counted
  [W6, SWA 3.1]. In any frame drawn while the pause is on, *t* is the frozen value, so the trees hold still
  (derived). The events that turn the pause on and off, and what follows for
  the title screen, the main menu, loading, the intro and the movies, are those of
  `docs/behavior/sound-weather-ambient.md` section 3.1.
- **Fixed-step mode.** With device flag 0x100 set, the frame update instead adds a fixed 0.02 s
  (`0x3CA3D70A`) to *t* every frame, also while paused [W3]. The flag is clear initially (the device flags
  start as 0x1C641), no console command controls it (the engine's mask commands on these flags are
  `rs_v_sync` 0x4, `rs_fullscreen` 0x1, `rs_refresh_60hz` 0x80 and `rs_stats` 0x20), and no shipped
  program file sets it (the engine sets only masks 0x203 and 0x80000 directly, the game library only sets
  0x800 and clears 0x100, the first renderer only reads the flags) [W7]. So in the shipped game the
  fixed-step mode never occurs.
- **Other writers.** Besides the frame update (and the device object's own copy routines), the only code
  that writes *t* is the game library's playback of recorded multiplayer demos, which copies recorded frame
  times into it [W3]; it does not run in single-player play (**believed**: it is reached only from the
  demo playback, which single-player play does not use). The first renderer only reads *t* [W3].

### 3.3 Precision

*t* is single precision: its step is 2⁻²³·2^⌊log₂ *t*⌋ (for example 2⁻⁷ s from 2¹⁶ s ≈ 18.2 h of
unpaused run on) (derived from W3). `wave`'s fourth component *t*/(2π) and the wind angle are computed from
this rounded *t* (section 2).

## 4. Value at a New Game's start and after loading

*t* at a frame is the real time elapsed since engine initialization minus all time paused so far (3.1). It
is never reset and never stored in or restored from a saved game: in all shipped program files only the
frame update writes it, only engine initialization starts its timer, and neither the game library nor the
first renderer touches the timer (the game library's multiplayer demo playback writes *t*, section 3.2);
these and the second renderer are the only libraries that use the engine's device object, and the second
renderer is not examined here [W3, W4]. Hence:

- **At a New Game's start**, *t* is the sum of the unpaused intervals of the run so far. With the pause
  events of `docs/behavior/sound-weather-ambient.md` section 3.1 and the pause off at program start [W6]
  these are: the start-up from engine initialization to the first item of the logo sequence (the loading
  period of `docs/behavior/startup.md` section 4 and the steps of section 5), any frames between logo items
  and between the logos and the title menu, then, after New Game closes the main menu, the whole creation
  and loading of the level, and the frames before the New Game intro `intro_game` starts; the logo items,
  the title menu and the intro add nothing [W3, W4, W6, SWA 3.1]. The durations of the start-up and the
  loading depend on the machine and the run, so *t* has no fixed value at a New Game's start; it is at least
  the time the level took to load (derived). Any earlier play in the same run (an earlier game, quitting to
  the menu) is included, minus its paused time.
- **After loading a saved game**, *t* continues the run's value: from the title screen as for a New Game
  (the run's unpaused time, including this loading); during a game, loading turns the pause off before it
  starts [SWA 3.1], and *t* includes the loading time. The saved game has no influence on it [W3, W4].
- **Level changes** are loaded the same way and counted in real time [SWA 3.1].

Consequence for comparisons with the original (derived): the sway's phase in a frame of the original is
fixed only by that frame's *t*, which follows the counter readings the program receives. If the counter is
controlled from outside (for example by stepping the readings it returns), *t* is (*C* − *C*₀ − *P* −
*O*) / *F* of those readings; *C*₀ is the reading at engine initialization, and *O* is whatever the 256
calibration pairs at program start measured (with readings that advance by a fixed step per read, that
step).

## 5. The sway of a vertex

The tree vertex programs are game data, `shaders\r1\tree_*.vs` with `shaders\shared\common.h` (included
through `shaders\r1\common.h`) [W8]. For a vertex with model position transformed to world position
**p** = (*x*, *y*, *z*) by *M* (`docs/formats/ogf.md` section 5), rigidity *r* and base height *b* = M[13]:

1. Phase *φ* = `wave`.w + `wave`.x·*x* + `wave`.y·*y* + `wave`.z·*z* = (*t* + 0.1*x* + 0.01*y* + 0.11*z*) / (2π)
   (with the constants of section 2) [W8].
2. *f* = 2.8284271·frac(*φ*) − 1.4142136, and the bend factor *d* = *f*² − 1, which lies in [−1, 1]
   [W8].
3. Height above the base *h* = *y* − *b*; intensity *i* = *h*·*d* [W8].
4. Displacement (*Δx*, *Δz*) = *r*·*i*·(`wind`.x, `wind`.z), the straight interpolation from (0, 0) to
   *i*·(`wind`.x, `wind`.z) by *r* [W8].
5. The vertex is drawn at (*x* + *Δx*, *y*, *z* + *Δz*); its height is unchanged [W8].

The programs `tree_w.vs`, `tree_w_dt.vs`, `tree_w_point.vs`, `tree_w_spot.vs` and `tree_wave.vs`,
`tree_wave_dt.vs`, `tree_wave_point.vs`, `tree_wave_spot.vs` move the vertex so; `tree_s.vs`,
`tree_s_dt.vs`, `tree_s_point.vs` and `tree_s_spot.vs` compute the same values but draw the vertex at
**p**, unmoved [W8]. Which program a tree uses comes from its shader (as in `docs/formats/ogf.md`
section 5; not covered here).

So, in *t* (derived from W1, W2, W8, W9): *d* repeats every 2π ≈ 6.2832 s (the phase advances one cycle
per 2π s), offset per vertex by its world position; the displacement's direction is (sin 0.2π*t*,
cos 0.2π*t*) in (x, z), turning once per 10 s; its size is 0.005·*r*·|*h*·*d*| metres.

## 6. Verification facts

From the shipped files of this edition [W2, W3, W7, W8, W9]:

| Value | Single precision | Use |
|---|---|---|
| 0.62831855 | `0x3F20D97C` | wind angle per second of *t* (0.2π) |
| 0.15915494 | `0x3E22F983` | 1/(2π), the factor of `wave` |
| 0.005 | `0x3BA3D70A` | length of `wind` |
| 0.1, 0.01, 0.11 | `0x3DCCCCCD`, `0x3C23D70A`, `0x3DE147AE` | `wave` x, y, z before the factor |
| 1/2048 | `0x3A000000` | `consts` x and y |
| 0.02 | `0x3CA3D70A` | fixed step of *t* with device flag 0x100 (never set) |
| 2.8284271, 1.4142136 | (program constants) | the bend factor of section 5 |
| 0x1C641 | (initial device flags) | flag 0x100 clear |

Worked values (derived, double precision, before single-precision rounding): at *t* = 0, `wind` =
(0, 0, 0.005, 0) and `wave`.w = 0; at *t* = 2.5 s, *a* = π/2 and `wind` = (0.005, 0, 0, 0) (to rounding);
at *t* = 120 s, `wave`.w = 19.0986 and *a* = 75.398 rad (12 turns, `wind` ≈ (0, 0, 0.005, 0)).

## 7. Unestablished

- The x87 precision mode in effect when the wind's sine, cosine and length and the division of 3.1 are
  computed; its effect is believed to stay within a few units in the last place of single precision
  (sections 2, 3.1).
- The durations of the start-up and of a level's loading in any given run, and so a numeric value of *t* at
  a New Game's start (section 4); these are not properties of the data or the program.
- That the multiplayer demo playback cannot run in single-player play is believed, not shown (section 3.2).
- Whether the second renderer, which also uses the engine's device object, writes *t* or the global timer
  (not examined; this specification is for the first renderer) (section 4).

## Gaps found in review

- The x87 precision mode: the core library's start-up sets the x87 precision to the 64-bit mantissa, and
  the engine executable imports a core library function that sets it to the 24-bit mantissa; which mode
  holds during the frame update's reading of *t* and the renderer's computation of `wind` is unestablished.

## Provenance

- Date: 2026-11-27. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`, and the shipped program files of `bin\` (engine executable, core
  library, first renderer, game library; the other libraries searched for the device's rate-factor setter
  and global-timer accessor).
- Data examined: `shaders\r1\tree_*.vs`, `shaders\r1\common.h` and `shaders\shared\common.h` (winning
  copies, from `gamedata.dba`).
- Analysis: static analysis (headless decompilation and disassembly with Ghidra, read-only) of the engine
  executable, core library, first renderer and game library: the per-frame computation of the sway's
  constants, the frame update of the engine's times, the timers and the pause state, the counter
  calibration, every writer of the engine's time in seconds and of the global timer, every setter of the
  device flags; constants read directly from the files. No program was executed.
