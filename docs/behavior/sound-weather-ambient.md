# Ambient sounds of the weather

Specification S4 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`): which
sounds of the weather's **ambient sets** and of their **ambient effects** play, when and where. It
covers the data (`env_ambient` key of the weather sections, the sets and effect sections of
`config\weathers\env_ambient.ltx`), the per-frame rule that chooses the set, starts random sounds around
the listener and starts and ends effects, and the values of every set and effect of this edition, with
the first level's default weather in detail. The effects' particles and wind (`particles`,
`wind_gust_factor`, the particle placement) are specification P11 of
`docs/behavior/particles-and-lights-scope.md`; this specification states the timing they and the
effects' sounds follow. What could not be established is marked **unestablished**; what is inferred
rather than read from the program or the data is marked **believed**.

Builds on (not repeated here):

- S1, `docs/formats/sound-ogg-params.md`: a sound name *n* names `sounds\<n>.ogg`; each file's min
  distance *m*, max distance *M*, base volume *b* and AI sound type; the reports to the AI.
- S2, positional sound playback (`docs/behavior/sound-positional.md`): creating a sound with a volume
  category; **play at position** with the flags looped and 2D and a delay; restart of a sound that is
  still playing; the effects category; the listener (the camera's eye); pausing; stop all.
- `docs/behavior/sky.md` section 1 and 3.3 and `docs/behavior/static-shading.md` section 9.2: the
  weather in effect, its two **current sections** A and B (the hourly sections bracketing the game time)
  and their blend weight *t*; `docs/behavior/level-loading.md` section 2.6 (the first level's weather,
  `default`, the start time 05:30).
- `docs/behavior/level-objects.md` section 5.4: an object's **sky visibility** *h*.
- `docs/behavior/start-position.md` section 6: the camera's eye position *E*.
- `docs/formats/ltx-config.md`: reading the files (whitespace removed from values; an empty value is "no
  value"; floats; vectors of floats; lists).

Conventions: "now" is the global clock in milliseconds (section 3.1); a frame is one frame of the
engine. *r* denotes the next number drawn from the shared generator (section 3.2), each occurrence a
new draw. x mod y is the remainder with the sign of x (C semantics). Positions are in the world
coordinates of the level files (metres, +y up).

**Checks.** Every factual claim carries a tag. The checks are scripts in the dirty workspace,
`checks/sound-weather-ambient/`: `extract.py` extracts the configuration, script and `sounds\ambient\`
files from the archives; `export.py` regenerates the evidence in `ev/` (headless Ghidra exports of the
shipped program files); `run_all.py` runs `check_data.py` (tags D1 – D9: the game data, decoded by a
reader written from section 2), `check_bin.py` (tags B1 – B17: the program files) and `model.py`
(tags M1 – M6: an implementation of sections 3 – 4 written from this text, reproducing the worked
examples of section 7).

## 1. Overview

- A weather section may name an **ambient set** by its key `env_ambient`. A set holds a list of sounds
  with a period and a distance range, and a list of **ambient effects** with a period. An effect has a
  particle effect, an optional sound, an offset, a life time and a wind gust factor (section 2).
- Every frame, while a level runs and the game is not paused, the program takes the set of one of the
  two current weather sections, chosen at random with the blend weight (section 4.4). From it:
  - a **random sound**: whenever the sound period has run out, one of the set's sounds is played at a
    random point 30 – 60 m (as the set says) around the listener and 10 m above it (section 4.5);
  - an **ambient effect**: whenever the effect period has run out, no effect is active and the view is
    **outdoors** (section 4.3), one of the set's effects starts at the listener plus its offset; its
    sound plays there; it ends after its life time, or as soon as the view is no longer outdoors
    (section 4.6).
- Random sounds play outdoors and indoors alike; only effects need the view to be outdoors (section 4.3).
- Every sound is played through S2 as a 3D sound of the effects category, not looped, without delay
  (section 6).
- No script reads these keys, sets or effects [D9]: the rule runs in the program without scripts.

## 2. Data

### 2.1 Lists

The keys `sounds` and `effects` hold comma-separated lists. The number of items of a list value is
found as follows [B3]:

- no value (or an empty value): 0 items;
- otherwise, scanning from the start: each `,` found counts one item and scanning continues after it;
  the scan stops at the first `,` that is directly followed by another `,`, or when no further `,` is
  found; then one more item is counted if any text follows the last `,` counted (or if no `,` was
  counted).

Item *i* (from 0) is the text between the *i*-th and the (*i*+1)-th `,` (from the start for item 0, to
the end for the last), with bytes 0x00 – 0x20 removed from both ends [B3]. So a trailing comma adds no
item (`a,b,` has 2 items), and `a,,b` has 2 items, `a` and an empty item. Because the LTX reader
removes whitespace from values, items never have surrounding spaces in practice
(`docs/formats/ltx-config.md`). In this edition four sets end their `sounds` list with a comma, and no
list has an empty item [D4].

Duplicates are kept: every item is its own entry, so a name listed *k* times is *k* separate sounds of
the same file, *k* times as likely to be chosen (sections 4.5, 5.2) [B1, B6].

### 2.2 Ambient sets

A set is a section named by `env_ambient` (section 2.4). It is read once, when the weather sections are
loaded (the sections of `docs/behavior/sky.md` section 3.2), the first time a weather section names it;
every weather section naming the same section shares the one set [B4]. Keys [B1, B2]:

| Key | Format | Read when | Meaning |
|---|---|---|---|
| `sounds` | list of sound names | always if present | the random sounds; each item is created as a sound of the effects category with the file's AI sound type (S1, S2) |
| `sound_period` | 2 floats *p*₀, *p*₁, seconds | the set has `sounds` | time between random sounds |
| `sound_dist` | 2 floats *d*₀, *d*₁, metres | the set has `sounds` | horizontal distance of a random sound from the listener |
| `effects` | list of effect section names | always if present | the ambient effects (section 2.3) |
| `effect_period` | 2 floats *q*₀, *q*₁, seconds | the set has `effects` | time between effect starts |

- `sound_period` and `effect_period` are each multiplied by 1000 and rounded down to whole
  milliseconds (towards minus infinity), giving *P*₀, *P*₁ and *Q*₀, *Q*₁; they are kept in that order
  [B1, B2, B15].
- `sound_dist` is kept as two floats in increasing order: if *d*₀ > *d*₁ they are swapped [B1].
- A set without `sounds` has no sounds, and its sound period and distance are not read; their values
  are then **unestablished** (section 4.7). A set without `effects`, or with `effects` without a value,
  has no effects [B1, B2]. Missing vector components are 0 (`docs/formats/ltx-config.md`).

Every set of this edition has all five keys [D2].

### 2.3 Ambient effects

An item of a set's `effects` names an effect section. Keys [B2]:

| Key | Format | Meaning |
|---|---|---|
| `life_time` | float, seconds | the effect's life *T*_life: × 1000, rounded down to milliseconds |
| `particles` | particle effect name | the effect's particles (P11) |
| `offset` | 3 floats | position of the effect relative to the listener, in world axes (section 4.6) |
| `wind_gust_factor` | float | the wind gust while the effect is active (P11) |
| `sound` | sound name; optional | the effect's sound, created as a sound of the effects category with the file's AI sound type |

Each item is read into the set separately, so an effect section listed by several sets (or twice in one
list) is read once per item [B2]. What a missing `life_time`, `particles`, `offset` or
`wind_gust_factor` key does is **unestablished**; every effect section of this edition has all five
keys [D2].

### 2.4 Weather sections

A weather section (an hourly section of a weather cycle, or of a weather effect: `docs/behavior/sky.md`
section 3.2) may have the key `env_ambient`, naming a set. Without the key the section has no set [B4].
A copy of a weather section keeps its set [B4].

## 3. Clock and random numbers

### 3.1 The global clock

"Now" is the engine's global clock: milliseconds of real time since the engine started, as an unsigned
32-bit number [B14]:

- it stops while the game is paused (the pause state in which this rule does not run, section 4.1) and
  continues from the same value when the pause ends [B14];
- its rate factor is 1 initially and no shipped program file changes it [B14];
- with the device flag 0x100 set it instead advances by 20 ms per frame; the flag is clear initially
  [B14] (what sets it is **unestablished**).

The times kept by this specification (section 4.2) are on this clock. It is not S2's sound clock.

### 3.2 The shared generator

Every random choice of this specification draws from one pseudo-random generator shared with other parts
of the program [B5]. Its state *s* is a 32-bit number; a draw sets *s* ← (*s* · 214,013 + 2,531,011)
mod 2³² and returns *r* = (*s* ≫ 16) AND 32,767, an integer 0 … 32,767 [B5]. The state starts at 1 and
is set, at start-up of the core library, to the low 32 bits of the processor's time-stamp counter [B5],
so the sequence differs from run to run; and other parts of the program draw from it in between. An
implementation cannot reproduce the original's sequence and may use any generator of uniform integers
0 … 32,767 (or this one, with any seed).

Derived values [B5, B6]:

- **set choice**: *r* · (1/32,767), compared in 32-bit float (section 4.4);
- **item** of a list of *n* > 0 items: *r* mod *n*; a list with no items gives none and draws nothing;
- **period** from bounds *a*, *b* (ms): (*r* mod (*b* − *a*)) + *a*; with *a* < *b*, an integer
  *a* … min(*b* − 1, *a* + 32,767); the bounds are not reordered (with *a* > *b* the result lies between
  *a* and *a* + (*a* − *b*) − 1);
- **distance** from *d*₀ ≤ *d*₁: *d*₀ + *r* · (*d*₁ − *d*₀) / 32,767, so *d*₀ … *d*₁;
- **angle**: *r* · (2π / 32,767) radians, the factor stored as the 32-bit float 0.00019175345.

## 4. The per-frame rule

### 4.1 When it runs

Once per frame, while a level is running, after the engine's frame (which updates the environment: A, B,
*t* of `docs/behavior/sky.md` section 1 for this frame) and only while the game is not paused [B7]. It
does nothing without a level [B7].

### 4.2 State

The program keeps, for the whole run [B13, B12]:

| State | Initial |
|---|---|
| next sound time *N*_s (ms) | 0 |
| next effect time *N*_e (ms) | 0 |
| effect end time *N*_end (ms) | 0 |
| the active effect's particles | none |
| the wind gust factor in force (P11) | 0 (set when the environment is created) |

The times start at 0 when the program starts and are not reset when a level is loaded or left [B13].
When a level is left, the active effect's particles (if any) are removed at once and every sound is
stopped at once (S2 "stop all") [B13]. Because the times start at 0, the first frame of play starts a
random sound at once and, if the view is outdoors, an effect.

### 4.3 Outdoors

The view is **outdoors** in a frame when the level has a view entity (the object whose camera is shown;
in normal play the actor) and that object's sky visibility *h* (`docs/behavior/level-objects.md`
section 5.4: the fraction of free sky rays, followed with the frame-time smoothing given there) is at
least 0.05 [B9]. Without a view entity the view is not outdoors [B9].

- The value read is *h* as last smoothed; reading it applies that section's smoothing step if it has not
  run yet in the frame, without casting new rays [B9].
- Whether the actor's *h* is updated (its rays cast) while the actor's own model is not drawn
  (first-person view) is **unestablished**. If it never is, *h* keeps its initial value 0.5
  (`level-objects.md` section 5.4) and the view always counts as outdoors.

### 4.4 The set of the frame

At the start of every frame's rule one number *r* is drawn, and the frame's set is B's if
1 − *t* ≤ *r* · (1/32,767) (both sides in 32-bit float), otherwise A's [B8]. So over many frames B's set
is taken in a fraction of about *t* of the frames: at *t* = 0 only when *r* = 32,767, at *t* = 1 always
(M1). The set may be none (the chosen section has no `env_ambient`).

Everything else in the frame uses this one set.

### 4.5 Random sounds

If the frame has a set and *N*_s < now [B10]:

1. A sound is chosen: item *r* mod *n* of the set's `sounds` (none if the set has no sounds; then no
   number is drawn).
2. A period is drawn from *P*₀, *P*₁ (section 3.2) and *N*_s ← now + period. This happens also when no
   sound was chosen.
3. If a sound was chosen: an angle *a* and a distance *d* (from *d*₀, *d*₁) are drawn, in that order, and
   the sound is played at the position

       E + (d · cos a, 10, d · sin a)

   where *E* is the camera's eye position of the frame (the listener of S2; `start-position.md`
   section 6) [B10]. (The program forms the horizontal unit vector (cos *a*, 0, sin *a*) and normalises
   it; the result is the same within float precision [B10].)

So random sounds follow each other at intervals of *P*₀ … *P*₁ − 1 ms of the global clock, measured from
the frame in which the previous one was started; a sound is started at the first frame after the
interval has run out, so the actual interval is that value rounded up to the frame times. The distance
from the eye is √(*d*² + 100) m: 31.62 – 60.83 m for *d* 30 – 60 (M2).

The draws of one random sound, in order: item, period, angle, distance (after the set-choice draw of the
frame) [B10].

### 4.6 Ambient effects

**Start.** If the frame has a set, the view is outdoors, no effect particles exist (section 4.2),
*N*_e < now, and the set has at least one effect, then [B11]:

1. An effect is chosen: item *r* mod *n* of the set's effects. (The draw happens only when the other
   conditions hold.)
2. The wind gust factor in force becomes the effect's `wind_gust_factor` (P11).
3. A period is drawn from *Q*₀, *Q*₁ and *N*_e ← now + period.
4. *N*_end ← now + *T*_life.
5. The effect's particles are created and started at the position *E* + `offset` (the offset added in
   world axes, not turned with the view; identity orientation) (P11).
6. If the effect has a sound (its `sound` key exists and the sound was created), the sound is played at
   the same position *E* + `offset` [B11].

The draws of one effect start, in order: item, period [B11].

**End.** In every frame, after the start step, whether or not the frame has a set [B12]:

- if the view is outdoors and now < *N*_end, nothing ends;
- otherwise (the view is not outdoors, or now ≥ *N*_end): the active effect's particles, if any, are
  told to stop, letting their running particles finish (a deferred stop; P11), and the wind gust factor
  in force becomes 0. This repeats in every such frame.

**Removal.** At the end of every frame, effect particles that have been told to stop and are no longer
playing are removed, and only then may a new effect start (section 4.6, "no effect particles exist")
[B12]. When stopped particles count as no longer playing is P11's.

Consequences:

- An effect lasts *T*_life from its start while the view stays outdoors; it ends early in the first
  frame in which the view is not outdoors [B12].
- A new effect can start only in a frame after the previous effect's particles are gone, and not before
  its own period (from the previous effect's start) has run out [B11, B12]. With a period shorter than
  the life time (set `ambient_env_tuman`: 5 – 10 s, effect `ae0_effect_4`: 15 s) the next effect waits
  for the previous one's particles to finish.
- The effect's sound is not stopped when the effect ends: it plays to its own end (S2) [B12].
- While the view is not outdoors no effect starts, but random sounds continue (section 4.5) (M4).

### 4.7 Faults

A period is drawn with the divisor *P*₁ − *P*₀ (or *Q*₁ − *Q*₀). A set with *P*₀ = *P*₁ makes the program
fault (an integer division by zero) at its first random sound, and one with *Q*₀ = *Q*₁ at its first
effect (believed from static analysis, not executed) [B6, B10, B11]. A set without `sounds` still draws a
sound period (section 4.5, step 2) from bounds that were never read (section 2.2); what happens then is
**unestablished**. No set of this edition has equal bounds or lacks `sounds` [D2, D4].

## 5. Data of this edition

### 5.1 The first level's weather

`config\game_maps_single.ltx` `[l01_escape]` `weathers = default`; `[weathers]` `default =
sect_default_weather` in `config\weathers\environment.ltx`; that cycle (in
`config\weathers\weather_default.ltx`) has 24 hourly sections, one per whole hour [D1]. Their sets
(`env_ambient`, the prefix `ambient_env_` omitted) [D1]:

| Hours | Set | Hours | Set |
|---|---|---|---|
| 00, 02, 03, 23 | `night` | 10 | `day` |
| 01, 08, 12, 13, 16, 17, 18 | `rain` | 11, 14, 15, 19 | `0` |
| 04, 05 | `tuman` | 20, 21, 22 | `evening` |
| 06, 07, 09 | `morning` | | |

The default weather uses 7 sets [D1]. At the start (05:30, A = `default_weather_05`, B =
`default_weather_06`, *t* = 0.5) the frame's set is `ambient_env_tuman` or `ambient_env_morning` with equal
chance [D8, M1]. At other times the sets of the two sections bracketing the game time are mixed by *t*
(section 4.4): between two whole hours the set of the earlier hour gives way to that of the later one,
e.g. between 03:00 and 04:00 `night` to `tuman`.

### 5.2 Sets

The 11 sets of `config\weathers\env_ambient.ltx` (which has 21 sections: the 11 sets and 10 effect
sections) and the weathers whose sections name them (of the 13 weathers of `[weathers]` and the 2 of
`[weather_effects]`; 123 hourly entries in all, of which only the 2 entries of weather `map`, both naming section `map_01`,
have no `env_ambient`) [D2, D3, D4]:

| Set | Weathers | `sound_period` (s) | `sound_dist` (m) | Items | Distinct | `effect_period` (s) | Effects (`ae0_effect_`…) |
|---|---|---|---|---|---|---|---|
| `ambient_env_0` | default | 5, 10 | 30, 60 | 38 | 36 | 30, 60 | 0, 1, 2, 3, 8 |
| `ambient_env_day` | default | 5, 10 | 30, 60 | 43 | 41 | 40, 90 | 1, 2, 3, 5, 6, 7, 8 |
| `ambient_env_evening` | default | 5, 10 | 30, 60 | 35 | 24 | 90, 120 | none (`effects` without a value) |
| `ambient_env_morning` | default | 5, 10 | 30, 60 | 30 | 27 | 30, 60 | 4 |
| `ambient_env_night` | default | 5, 10 | 30, 60 | 35 | 24 | 130, 160 | 1, 2, 3, 8 |
| `ambient_env_rain` | default, pripyat, radar, rain, test, yantar | 5, 10 | 30, 60 | 38 | 29 | 20, 40 | 1 – 8 |
| `ambient_env_tuman` | default | 5, 10 | 30, 60 | 33 | 33 | 5, 10 | 4, 6 |
| `ambient_env_indoor` | indoor, sarkofag, yantar_indoor | 10, 30 | 10, 30 | 15 | 15 | 130, 160 | none |
| `ambient_env_x18` | indoor_x18 | 30, 60 | 10, 30 | 8 | 8 | 130, 160 | none |
| `ambient_env_stancia1` | stancia, surge_day, p_surge_day | 5, 10 | 30, 60 | 9 | 6 | 10, 20 | 1 – 9 |
| `ambient_env_stancia2` | stancia2 | 5, 10 | 30, 60 | 18 | 17 | 10, 20 | 1 – 9 |

("Items" counts list items by section 2.1, duplicates included; "Distinct" counts distinct names.)

- Duplicates in the default weather's sets: none in `tuman`; in `rain` six names repeat (`rnd_drone1` and
  `rnd_drone2` three times each) [D4]. **Disputed in review:** the list is incomplete: in `rain`,
  `rnd_drone1`, `rnd_drone2` and `rnd_wind_tree` occur three times each and `rnd_dark`, `rnd_dark3` and
  `rnd_dark5` twice each (38 items, 29 distinct). `evening` and `night` have identical lists, in which `crickets_1`
  occurs 3 times and `crickets_2` and `crickets_3` 4 times each of 35 [D4].
- The sets `ambient_env_indoor`, `ambient_env_morning`, `ambient_env_stancia1` and `ambient_env_tuman`
  end their `sounds` list with a comma (no extra item, section 2.1) [D4].
- The default weather's 7 sets name 80 distinct sounds; all 11 sets and the 10 effects name 106 [D6].
- The complete lists are in the data; a digest for checking a decoder is in section 7.

### 5.3 Effect sections

| Section | `life_time` (s) | `particles` | `sound` | `offset` | `wind_gust_factor` |
|---|---|---|---|---|---|
| `ae0_effect_0` | 10 | `nature\fog_stormy` | `ambient\rnd_outdoor\rnd_wind_3` | 0, 0, 0 | 0.015 |
| `ae0_effect_1` | 7 | `nature\fog_stormy_02` | `ambient\rnd_outdoor\rnd_wind_2` | 0, 0, 0 | 0.0155 |
| `ae0_effect_2` | 10 | `nature\fog_tornado_01` | `ambient\rnd_outdoor\rnd_wind_3` | 0, 0, 10 | 0.0155 |
| `ae0_effect_3` | 10 | `nature\fog_tornado_00` | `ambient\rnd_outdoor\rnd_wind_3` | 0, 0, 0 | 0.015 |
| `ae0_effect_4` | 15 | `nature\fog_foggy_00` | `ambient\rnd_outdoor\rnd_wind_2` | 0, 0, 0 | 0.015 |
| `ae0_effect_5` | 7 | `nature\vortex_01` | `ambient\rnd_outdoor\rnd_wind_2` | 0, 0, 25 | 0.03 |
| `ae0_effect_6` | 7 | `nature\fog_stormy_01` | `ambient\rnd_outdoor\rnd_wind_1` | 0, 0, 0 | 0.02 |
| `ae0_effect_7` | 8 | `nature\fog_stormy_02` | `ambient\rnd_outdoor\rnd_wind_1` | 0, 0, 15 | 0.025 |
| `ae0_effect_8` | 7 | `nature\fog_stormy_00` | `ambient\rnd_outdoor\rnd_wind_2` | 0, 0, 0 | 0.01 |
| `ae0_effect_9` | 10 | `nature\fog_stormy_01` | `ambient\rnd_outdoor\rnd_wind_3` | 0, 0, 0 | 0.03 |

[D5] Every effect section is used by some set; the default weather uses nine of them, all but
`ae0_effect_9` [D5]. Their sounds are `ambient\rnd_outdoor\rnd_wind_1`, `_2`, `_3`, none of which is in a
`sounds` list [D6]. An offset (0, 0, 10) places the effect and its sound 10 m from the eye along world +z,
whatever the view direction.

### 5.4 Sound files

All 106 named files exist, mono 44,100 Hz, each with a version 3 parameter record (S1) [D6]:

- the 83 names under `ambient\rnd_outdoor\` (all random sounds of the outdoor sets and the three wind
  sounds) have max distance 75 m and base volume 1; their min distances are 1 (9 files), 1.5 (3), 2 (8),
  3 (9), 4 (6), 5 (40), 6 (7) and 7 (1) [D6];
- the 23 names of `ambient_env_indoor` and `ambient_env_x18` (under `ambient\underground\` and
  `ambient\x18\`) have max distances 50 … 1000 m, the smallest 50 m (`rnd_rat_panic_1` … `_3`) [D6];
- every listed sound's max distance exceeds its set's farthest placement √(*d*₁² + 100) (60.83 m for the
  outdoor sets, 31.62 m for the two indoor sets), so no random sound is placed beyond its max distance
  [D6];
- the AI sound type is 0 except for `crickets_1` … `_3` and `owl_1` … `_3` (0x08000080, `world_ambient`)
  [D6];
- durations (S2 section 2) of the default weather's 80 sounds: 1,120 ms (`crow2`) … 11,654 ms
  (`rnd_dark6`); the wind sounds `rnd_wind_1`, `_2`, `_3`: 6,961, 6,961 and 10,432 ms, min distances 5,
  7 and 5 m [D6].

## 6. Playback

Every random sound and effect sound is a sound created when its set is loaded (section 2.2), of the
**effects** volume category, with the AI sound type taken from its file [B1, B2, B10]. It is played
through S2 "play at position" with no object, looped off, 2D off, delay 0, at the position of section 4.5
or 4.6 [B10, B11]. Hence (S2):

- it is a 3D emitter at that fixed position, heard with distance attenuation from its file's *m* and *M*
  relative to the listener, who keeps moving; the sound does not follow the listener;
- if the same list item (the same created sound) is chosen again while its previous play is still
  running, that play is restarted at the new position (S2 section 3.2) rather than a second emitter
  being created; a duplicate item is a separate sound and plays alongside;
- the play volume is 1; the effects volume setting applies;
- whether the AI receives reports for these sounds follows S1 section 7.2: the files with a non-zero AI
  sound type are played with no object, so no report is made (believed, as S1 states for sounds not
  associated with an object).

Occlusion and reverberation are those of S6 and S7.

## 7. Verification facts

Data (section 5) [D1 – D8]: the tables of section 5; and a digest of the decoded sets: the SHA-256 of the
UTF-8 text made of, for each of the 11 sets in order of the set name, the line
`<set>|<sound_period>|<sound_dist>|<effect_period>|<effect items joined by ,>` (the values as stored after
whitespace removal; no value as empty) followed by one line `<set>|<i>|<item i lowercased>` per sound item
*i* from 0, each line ending in `\n`: 313 lines,
`6e444a3ceb57e2ae0acc31578eefb05ad21c3cd7362247fc90209216edc18846` [D7].

Worked examples, computed by `model.py` from sections 3 – 4 [M1 – M6]:

| Case | Result |
|---|---|
| `ambient_env_tuman` bounds | *P* = 5,000 … 10,000 ms, *d* 30 … 60 m, *Q* = 5,000 … 10,000 ms [M1] |
| sound period from 5,000 / 10,000 ms, *r* = 12,345 / 32,767 / 0 | 7,345 / 7,767 / 5,000 ms [M1] |
| `ambient_env_indoor` period, *r* = 12,345 | 22,345 ms [M1] |
| set choice at *t* = 0.5 | *r* = 16,383 → A, *r* = 16,384 → B; at *t* = 0 only *r* = 32,767 gives B; at *t* = 1 always B [M1] |
| *r*_angle = 8,192, *r*_dist = 16,384, eye at the start, *E* = (−246.72630, −23.263261, −134.43387) | *a* = 1.570844, *d* = 45.00046, position (−246.728, −13.263, −89.433) [M2] |
| *r*_angle = 0, *r*_dist = 0 | position *E* + (30, 10, 0) [M2] |

Run with the generator of section 3.2 seeded with 1 (first draws 41, 18467, 6334, 26500, 19169, 15724,
11478), sets `tuman` (A) and `morning` (B) at a constant *t* = 0.5, the start eye, the view outdoors,
frames at now = 20, 40, … 60,000 ms, and effect particles taken as gone in the frame they are told to
stop [M3]:

| now (ms) | Event |
|---|---|
| 20 | set `tuman` (*r* = 41); sound item 20 `rnd_shooting_6` (*r* = 18,467 mod 33), period 6,334 ms, angle *r* 26,500, distance *r* 19,169: position (−229.572, −13.263, −178.782); effect `ae0_effect_4` (*r* = 15,724 mod 2 = 0), period 6,478 ms, end 15,020 |
| 6,360 | sound `rnd_dog1` (tuman), period 8,195 ms |
| 14,560 | sound `rnd_boar2` (tuman), period 7,864 ms |
| 15,020 | effect ended (end time reached) |
| 15,040 | effect `ae0_effect_4` from `morning`, period 47,103 ms, end 30,040 |
| 22,440, 31,420 | sounds `rnd_swamp`, `rnd_darkwind3` (tuman) |
| 30,040 | effect ended |
| 38,940, 47,760, 56,840 | sounds `rnd_fly` (morning), `rnd_krik4` (tuman), `rnd_darkwind5` (morning) |

The same run with the view not outdoors starts the same kind of random sounds and no effect [M4]; with
sections without a set nothing plays, though one number is still drawn every frame [M5]; an effect
started in a frame with a set ends at its end time even if the following frames have no set, as long as
the view stays outdoors [M6].

## 8. Unestablished

- Whether the actor's sky visibility is updated in first-person view (section 4.3).
- What a missing `life_time`, `particles`, `offset` or `wind_gust_factor` key of an effect section does
  (section 2.3); what sets device flag 0x100 (section 3.1).
- A set without `sounds` (section 4.7); that equal period bounds fault is believed (section 4.7).
- When stopped effect particles count as no longer playing (P11).
- That no AI report is made for these sounds (believed, section 6).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives
  `gamedata.db0` … `gamedata.dbd`, and the shipped program files of `bin\`.
- Data examined: `config\weathers\*.ltx` and the `config\system.ltx` and `config\game.ltx` include trees,
  the `sounds\ambient\` files (Ogg pages, Vorbis headers, parameter records), and the shipped
  `scripts\*.script` files (searched for the keys).
- Analysis: static analysis (decompilation and disassembly with Ghidra, headless) of the shipped engine
  executable, game library, sound library, first renderer and core library; reading constants and
  initial data from them. No program was executed. Public references: the Ogg and Vorbis I
  specifications.

## Gaps found in review

- Which object's sky visibility is read for the outdoors test when the view entity is not the actor
  (section 4.3).
