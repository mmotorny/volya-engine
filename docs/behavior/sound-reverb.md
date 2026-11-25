# Reverb

Specification S7 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`):
whether and how environmental reverb is applied to what the player hears. It covers when the game uses
the audio device's reverb at all (the setting `snd_efx` and the device), the library of reverb
**environments** (`senvironment.xr`), the per-level environment geometry (`level.snd_env`), how the
environment is chosen from the listener's position, how the listener's reverb changes over time, the
values sent to the device, and what is heard on `levels\l01_escape\`. What could not be established is
marked **unestablished**; what is inferred rather than read from the program or the data is marked
**believed**.

Builds on (not repeated here):

- S2, `docs/behavior/sound-positional.md`: sounds, emitters, the update and its order (section 5.1),
  the time step *dt*, the listener (section 7.2), OpenAL 1.1 as the audio device interface, the
  `-nosound` switch.
- S1, `docs/formats/sound-ogg-params.md` (sound files) and S3, `docs/behavior/sound-static-sources.md`
  (the level's static sources, which are emitters like any other).
- `docs/behavior/options-screens.md`: the check box "EAX" (`check_eax`) editing `snd_efx`, its default,
  and the sound restart that saving it marks.
- `docs/behavior/startup.md` (the user settings file `user.ltx`), `docs/behavior/level-loading.md` (the
  level directory `$level$`), `docs/formats/db-archive.md` (stored paths, the later archive wins) and
  `docs/formats/chunks.md` (chunked files: `u32` id, `u32` size, data).
- Public documents: the Creative Labs "Effects Extension Guide" for OpenAL (EFX, version 1.1, July
  2006, D. Peacock and P. Harrison), cited as "the EFX guide"; and the usual definitions of the
  millibel (1 mB = 1/100 dB) and of a gain in decibels (gain = 10^(dB/20)).

Conventions: positions are in the world coordinates of the level files (metres; *y* up, as in
`level-cform.md`); `f32` is an IEEE 754 single-precision number, all multi-byte values little-endian.
"The none environment" is defined in section 3.3.

**Checks.** Every factual claim carries a tag. The checks are scripts in the dirty workspace,
`checks/sound-reverb/`: `extract.py` extracts `senvironment.xr` and every `level.snd_env` from the
archives; `export.py` regenerates the evidence in `ev/` (headless Ghidra exports of the shipped sound
library, game library, engine executable and collision library); `run_all.py` runs `check_data.py`
(tags D1 – D9: the archives, decoded with `envfmt.py`, a parser written from sections 3 and 4),
`check_bin.py` (B1 – B14: the program files), `check_pub.py` (P0 – P3: the EFX guide) and `model.py`
(M1 – M5: an implementation of sections 5 – 7 written from this text, reproducing section 10).

## 1. Overview

- The program uses reverb only through the device's EAX 2.0 listener reverb (Creative's
  "Environmental Audio Extensions", reached through OpenAL), and only while `snd_efx` is on and the
  device qualifies (section 2). Otherwise it sends no reverb parameters at all.
  **Disputed in review:** not only through OpenAL: on the `-dsound` device path (S2 section 2) the
  program also tests the device for support of the same EAX 2.0 listener property set and can use it;
  that path is not covered here (see "Gaps found in review").
- An **environment** is a named set of 12 reverb values (section 3), from the library file
  `senvironment.xr`.
- A level may have a file `level.snd_env`: closed meshes (boxes) whose triangles name one environment
  on their front side and one on their back side (section 4). The environment at a position is found by
  casting a ray straight down from it (section 5); where there is no such geometry, or nothing is hit,
  the **none environment** applies, whose room level of −10000 mB silences the reverb.
- The listener's reverb moves gradually towards the environment at the listener's position (section 6)
  and is sent to the device once per update (section 7).
- On `l01_escape`, which has no `level.snd_env`, the none environment applies everywhere: no reverb is
  heard from the program's settings (section 9).

## 2. When reverb is used

### 2.1 The setting

- `snd_efx` is an on/off console variable, the bit with value 4 of the sound system's flags; the flags'
  built-in initial value is 6, so `snd_efx` starts **on** [B1]. The default `user.ltx` contains the line
  `snd_efx on` [D9]. (The options check box and its default: `options-screens.md`.)
- The setting is read at every update (section 6): switching it off stops the program from sending
  reverb values at the next update, switching it on resumes them [B1]. What the device then does with
  the values last sent is the device's (believed: they stay in effect) and **unestablished**.
- Saving `snd_efx` in the options marks a sound restart (`options-screens.md`) [B1]; the restart makes
  the program select the listener's environment again (section 6.1) [B10].

### 2.2 The device

When the sound system starts (S2 section 2), the program examines each OpenAL device and records
whether the device reports the OpenAL extension named `EAX2.0` and whether its name is exactly
`Generic Hardware` or `Generic Software` [B2]. For the device it opens, **EAX is usable** only if all of
these hold [B2]:

1. the device reports `EAX2.0`;
2. its name is neither `Generic Hardware` nor `Generic Software`;
3. the OpenAL functions `EAXSet` and `EAXGet` are available;
4. a probe succeeds: reading listener properties of the EAX 2.0 listener property set and writing the
   same values back is accepted by the device (the probe reads and writes property 2, then 3, then
   further properties) [B2].

The program also probes the same writes with the EAX **deferred** flag; if that succeeds, it sends all
values deferred and commits them together (section 7.1) [B2]. The log shows the outcome as
`* sound: EAX 2.0 extension: present` / `absent` and `* sound: EAX 2.0 deferred: present` / `absent`
[B2].

Rule 2 means that the reverb is never used on the two generic devices of the OpenAL implementation,
although the EFX guide states that the "Generic Software" device supports the EAX 2.0 reverb [P3]. Which
device the program opens (S2 does not specify device selection) is **unestablished** here; with
`-nosound` there is no sound and so no reverb (S2 section 2).

**Reverb is active** at an update when `snd_efx` is on and EAX is usable [B1].

## 3. The environment library: `senvironment.xr`

### 3.1 File

- Stored path `senvironment.xr` (in the game data root, `$game_data$`); the archives hold two identical
  copies (794 bytes, in `gamedata.db4` and `gamedata.dba`; the later one wins) [D1].
- It is read once when the sound system starts, only if it exists [B3]. Without it there is no library,
  and no level geometry can be built (section 4.3) [B3, B6].
- It is a chunked file. The chunks with ids 0, 1, 2, … are read in this order until an id is missing;
  each chunk holds one environment record [B3]. The shipped file has exactly the 11 chunks 0 … 10, in
  order, and nothing else [D2].

### 3.2 Record

| Offset | Type | Field |
|---|---|---|
| 0 | `u32` | version |
| 4 | zero-terminated string | name |
| then | 12 × `f32` | the values of the table below, in this order |
| then | `u32` | preset number (only if version > 3) |

[B3]

- A record whose version is below 3 is dropped (not added to the library) [B3]. Version 3 records have
  no preset number (it is 0). Every shipped record is version 4, and each chunk is exactly as long as its
  fields [D2].
- The library is the list of the records kept, in chunk order; an environment is identified by its
  **position** in that list. Looking a name up compares names ignoring case and takes the first match
  [B3]. The 11 shipped names are distinct ignoring case [D2].
- The preset number has no effect on what is sent (section 8) [B9].

The 12 values (their meanings and units: section 7.2):

| # | Value | Unit | Built-in default | Range (section 6.3) |
|---|---|---|---|---|
| 1 | room | mB | −1000 | −10000 … 0 |
| 2 | room HF | mB | −100 | −10000 … 0 |
| 3 | room rolloff | factor | 0 | 0 … 10 |
| 4 | decay time | s | 1.49 | 0.1 … 20 |
| 5 | decay HF ratio | ratio | 0.83 | 0.1 … 2.0 |
| 6 | reflections | mB | −2602 | −10000 … 1000 |
| 7 | reflections delay | s | 0.007 | 0 … 0.3 |
| 8 | reverb | mB | 200 | −10000 … 2000 |
| 9 | reverb delay | s | 0.011 | 0 … 0.1 |
| 10 | environment size | m | 7.5 | 1 … 100 |
| 11 | diffusion | 0 … 1 | 1.0 | 0 … 1 |
| 12 | air absorption HF | mB | −5 | −100 … 0 |

[B4, B5] The built-in defaults are what a record holds before it is read (so also the values of the none
environment, section 3.3) [B4].

### 3.3 The none environment

The **none environment** is the built-in defaults with room = −10000 and preset number 0 [B4]. It is
used where no environment is found (section 5) and is the listener's initial environment (section 6.2)
[B8, B14]. It is not taken from the library (the library's `identity` record happens to hold the same
values, section 3.4) [B4, D3].

### 3.4 Shipped environments

The 11 records, in library order (values rounded to 7 significant digits) [D2, D3]:

| Pos. | Name | Preset | Room | Room HF | Rolloff | Decay | HF ratio | Refl. | Refl. delay | Reverb | Rev. delay | Size | Diffusion | Air abs. HF |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | `identity` | 0 | −10000 | −100 | 0 | 1.49 | 0.83 | −2602 | 0.007 | 200 | 0.011 | 7.5 | 1 | −5 |
| 1 | `default` | 0 | −1000 | −100 | 0 | 1.49 | 0.83 | −2602 | 0.007 | 200 | 0.011 | 7.5 | 1 | −5 |
| 2 | `bathroom` | 3 | −1000 | −1200 | 0 | 1.49 | 0.54 | −370 | 0.007 | 1030 | 0.011 | 1.4 | 1 | −5 |
| 3 | `mountains` | 0 | −1000 | −2500 | 0 | 1.49 | 0.21 | −2780 | 0.3 | −1434 | 0.1 | 100 | 0.27 | −5 |
| 4 | `yard` | 15 | −1500 | −2500 | 0 | 2.5 | 0.3 | −2000 | 0.15 | −969 | 0.1 | 38 | 0.33 | −5 |
| 5 | `room` | 0 | 0 | 0 | 0 | 1 | 0.2 | −1376 | 0.006 | −1104 | 0.01 | 2.5 | 1 | −5 |
| 6 | `plant` | 10 | −1500 | −1500 | 0 | 4 | 0.7 | −700 | 0.04 | −214 | 0.03 | 50.3 | 1 | −5 |
| 7 | `long_corridor` | 13 | −1000 | −500 | 0 | 5.248 | 0.79 | −3262 | 0.015 | −1339 | 0.05 | 16.04 | 1 | −5 |
| 8 | `open_space` | 17 | −600 | −2500 | 0 | 2.5 | 0.15 | −2780 | 0.3 | −1434 | 0.1 | 100 | 0.27 | −5 |
| 9 | `house` | 4 | 0 | 0 | 0 | 0.5 | 0.1 | −1376 | 0.003 | −1104 | 0.008 | 2.5 | 1 | −5 |
| 10 | `forest` | 15 | −1000 | −3300 | 0 | 1.49 | 0.54 | −2560 | 0.162 | −229 | 0.088 | 38 | 0.3 | −5 |

- The air absorption HF is exactly −5.0 in `identity`, `default`, `bathroom`, `mountains` and `room`, and
  the `f32` with bits 0xC09FFFFF (−4.9999995) in the other six [D3].
- Room rolloff is 0 in every record; every stored value is already within its range [D3].
- The SHA-256 of the 132 values as `f32` (record order, value order, little-endian, concatenated) is
  `a07dbab03d310c68ce3fe0671ba64141dfff2eef1bd28840c8e956d18d4a073b` [D3].

## 4. Level environment geometry: `level.snd_env`

### 4.1 Files

- `level.snd_env` exists for exactly 5 of the 30 level directories: `l07_military` (8,747 bytes),
  `l08_yantar` (1,802), `l12u_sarcofag` (953), `testers_mp_factory` (1,810), `testers_mp_pool` (363);
  there is no `levels\l01_escape\level.snd_env`, and no other `.snd_env` file [D1].
- When a level is loaded (not on a dedicated server), if `$level$level.snd_env` exists it is read and
  becomes the sound system's environment geometry, replacing any previous one; this happens just before
  `level.som` is read (S6) [B6]. Nothing else in the game or the engine sets the geometry [B6].
- If the file does not exist, nothing is done: the geometry of the previously loaded level, if any,
  stays in effect; it is otherwise released only when the sound system is destroyed [B6]. So a level
  without the file has no geometry only if no level with one was loaded before in the same run of the
  program (believed from static analysis; not executed).

### 4.2 Format

A chunked file with chunk 0 and then chunk 1, nothing else (all shipped files) [D4]:

- **Chunk 0, names:** zero-terminated environment names, one after another until the chunk ends [B7];
  their order gives each a **local index** 0, 1, 2, … [B7]. Every shipped name is a library name [D4].
- **Chunk 1, mesh** [B7]:

  | Offset | Type | Content |
  |---|---|---|
  | 0 | `u32` | 4 in every shipped file; not used by the program |
  | 4 | `u32` | vertex count *V* |
  | 8 | `u32` | triangle count *T* |
  | 12 | 6 × `f32` | in every shipped file the minimum and maximum corner of the vertices (within 0.0001); not used by the program |
  | 36 | *V* × 3 `f32` | vertex positions *x*, *y*, *z* |
  | 36 + 12 *V* | *T* × 16 bytes | triangles: 3 × `u32` vertex indices *i*0, *i*1, *i*2, then `u16` **front** and `u16` **back** local indices |

  [B7, D5] The chunk is exactly 36 + 12 *V* + 16 *T* bytes in every shipped file [D5].

### 4.3 Building

- Without a library (section 3.1) the file is ignored [B6].
- Each name of chunk 0 is looked up in the library (section 3.2); a name that is not found is a fatal
  error [B7]. Each triangle's front and back local indices are replaced by the library positions of the
  names they index; a local index not below the number of names is a fatal error [B7].
- The mesh needs at least 4 vertices and 2 triangles (otherwise a fatal error) [B7]; every shipped file
  passes [D5].
- Fatal error means the program shows an error and stops (believed, as in S1 section 5; not executed).

### 4.4 Shipped geometry

In every shipped file [D6, D7]:

- the vertices come in groups of 8 consecutive vertices, each group used by exactly 12 triangles whose
  three indices lie in that group: each group is a closed six-sided box; all are axis-aligned except
  groups 2, 3, 21, 25 and 29 of `l07_military` (group 25 only by about 0.00001, one `f32` step);
- all triangles of a box have the same front and back names;
- every triangle's normal (*v*1 − *v*0) × (*v*2 − *v*1) points out of its box, so the **front** name
  applies outside a box and the **back** name inside it (section 5).

| Level | Boxes (*V*, *T*) | Inside (back) | Outside (front) |
|---|---|---|---|
| `l07_military` | 30 (240, 360) | `bathroom`, `house`, `long_corridor`, `open_space`, `room` | `house`, `long_corridor`, `mountains`, `open_space` |
| `l08_yantar` | 6 (48, 72) | `forest`, `plant` | `forest`, `identity` |
| `l12u_sarcofag` | 3 (24, 36) | `identity`, `long_corridor`, `plant` | `default`, `identity` |
| `testers_mp_factory` | 6 (48, 72) | `forest`, `identity`, `plant` | `default`, `forest`, `identity` |
| `testers_mp_pool` | 1 (8, 12) | `long_corridor` | `identity` |

[D7]

## 5. The environment at a position

The environment at a position *p* is found as follows [B8]:

1. If there is no environment geometry, it is the none environment.
2. Otherwise a ray is cast from *p* straight down, in direction *d* = (0, −1, 0), up to a distance of
   1000. It hits a triangle if the ray meets it at a distance *t* with 0 < *t* ≤ 1000; triangles are
   hit from either side, except that a triangle almost parallel to the ray is missed (the absolute value
   of the determinant of the standard Möller–Trumbore ray–triangle test, with edges *v*1 − *v*0 and
   *v*2 − *v*0, below 0.00001). Of all hits the one with the smallest *t* is taken; among equal distances the one found
   first is kept (which one is not specified) [B8].
3. No hit: the none environment.
4. Otherwise let *n* = (*v*1 − *v*0) × (*v*2 − *v*1) of the hit triangle (normalised; only its sign
   matters here) [B8]. If *n* · *d* < 0 (the triangle's normal points up, *n*.*y* > 0: the position is on
   the triangle's front side) the environment is the triangle's **front** environment, otherwise
   (*n*.*y* ≤ 0) its **back** environment [B8].

The result is the library record's values as stored (section 3.4) or the none environment's; the values
are not clamped until section 6.3 [B8, B5].

With the shipped boxes this means: a position inside a box whose floor is the first surface below it
gets the box's inside environment; a position above a box gets its outside environment; a position
with no box below it (beside or under all boxes) gets the none environment [M2, M5].

## 6. The listener's environment over time

The listener has a **current** environment *c* (12 values) and a **target** environment *g* [B9].

### 6.1 When the target is chosen

- The **moved mark** is set when the listener is sent to the device (S2 section 7.2) and any coordinate
  of the listener position differs by 0.001 or more from the stored listener position; the new position
  is then stored (a slower drift accumulates until it reaches 0.001) [B10]. A sound restart also sets the
  mark [B10]. It starts unset [B14].
- At an update with reverb active (section 2.2), if the mark is set it is cleared and *g* becomes the
  environment at the listener position of this update (section 5) [B9].

So the target follows the listener with a delay of one update, and only while reverb is active.

### 6.2 Initial values

Both *c* and *g* start as the none environment (after clamping) [B14]. Nothing else sets *c* [B9].

### 6.3 Each update

In S2's update order (section 5.1), after the emitters are updated and before the listener is sent, at
every update with reverb active [B9]:

1. if the moved mark is set: choose *g* (section 6.1);
2. each of the 12 values moves towards the target: *c* ← *g* · *dt* + *c* · (1 − *dt*), in `f32`, with *dt*
   the update's time step in seconds (S2) [B9];
3. each value of *c* is limited to its range of section 3.2 (values below the minimum become the
   minimum, above the maximum the maximum) [B5];
4. *c* is sent to the device (section 7) [B9].

The arithmetic is in `f32` (believed from the decompiled program, which computes in single precision);
so even a steady environment is reproduced only up to rounding (an example: section 9). At 60 updates
per second (*dt* = 1/60 s) a change of the target is 63.6 % complete after 60 updates (1 s) and 99 %
after 275 updates (4.58 s) [M3]; with *dt* > 1 s (a very long frame) step 2 overshoots and step 3 limits the
result (believed consequence of step 2; not executed). Worked example: section 10 [M3].

## 7. Output to the device

### 7.1 Sending

At each update with reverb active, the program sets 12 properties of the EAX 2.0 listener property set
(identifier {0306A6A8-B224-11D2-99E5-0000E8D8C722}) through `EAXSet`, in this order [B9, B12]:

| Property number | Value sent | Type |
|---|---|---|
| 2 | room | integer (section 7.3) |
| 3 | room HF | integer |
| 4 | room rolloff | `f32` |
| 5 | decay time | `f32` |
| 6 | decay HF ratio | `f32` |
| 7 | reflections | integer |
| 8 | reflections delay | `f32` |
| 9 | reverb | integer |
| 10 | reverb delay | `f32` |
| 13 | diffusion | `f32` |
| 14 | air absorption HF | `f32` |
| 15 | flags: the constant 0x3F | integer |

[B9] Properties 11 and 12 are never set, so neither the preset number nor the environment size is sent
[B9]. If deferred writes are supported (section 2.2), every property number carries the deferred flag
0x80000000 and, after the 12, the program sets property 0 of the same set without data to commit them;
otherwise each takes effect as it is set [B9]. The program sets no other EAX property, in particular none
per sound (section 8) [B12].

### 7.2 Meaning of the values

The EAX 2.0 reverb itself is the device's. That the 12 values are the standard EAX 2.0 listener reverb
parameters named in section 3.2 is **believed**: the program's built-in defaults and ranges are exactly
the EFX guide's defaults and ranges of its EAX reverb effect once each mB value is converted to a gain
10^(mB/2000) [P1, P2]:

| Value | EFX guide parameter (`AL_EAXREVERB_…`) | Program default → converted | Guide default |
|---|---|---|---|
| room | `GAIN` | −1000 mB → 0.316 | 0.32 |
| room HF | `GAINHF` | −100 mB → 0.891 | 0.89 |
| room rolloff | `ROOM_ROLLOFF_FACTOR` | 0 | 0.0 |
| decay time | `DECAY_TIME` | 1.49 s | 1.49 |
| decay HF ratio | `DECAY_HFRATIO` | 0.83 | 0.83 |
| reflections | `REFLECTIONS_GAIN` | −2602 mB → 0.0500 | 0.05 |
| reflections delay | `REFLECTIONS_DELAY` | 0.007 s | 0.007 |
| reverb | `LATE_REVERB_GAIN` | 200 mB → 1.259 | 1.26 |
| reverb delay | `LATE_REVERB_DELAY` | 0.011 s | 0.011 |
| diffusion | `DIFFUSION` | 1.0 | 1.0 |
| air absorption HF | `AIR_ABSORPTION_GAINHF` | −5 mB → 0.9943 | 0.994 |

[P1] The ranges agree in the same way (e.g. reflections −10000 … 1000 mB → 0 … 3.16, reverb −10000 …
2000 mB → 0 … 10, air absorption HF −100 … 0 mB → 0.891 … 1, where the guide gives 0.892 … 1) [P1]. In
these terms: room is the overall level of the reverberation (reflections and late reverb together), room
HF its level at high frequencies, reflections and reverb the levels of the early reflections and of the
late reverb, with their delays; decay time and its high-frequency ratio; diffusion; air absorption per
metre at high frequencies; room rolloff the distance attenuation of the reverberation (0: none) (the EFX
guide's descriptions). A room of −10000 mB (gain 0.00001, −100 dB) silences the reverberation (believed:
the guide's minimum gain, 0.0, is "no reverberation"; −100 dB is inaudible in practice).

The flags value 0x3F and the EFX parameters the program cannot set (density, the low-frequency and echo
and modulation controls, the pans, the HF and LF references, the decay HF limit) take the device's own
values; what they are is **unestablished** (believed: the device's defaults).

Which sounds the device's listener reverb applies to (every source, including 2D ones, or only some) is
the device's and **unestablished**; the program sets nothing per source [B12].

### 7.3 Integer values

Room, room HF, reflections and reverb are converted from `f32` to an integer by rounding **down**
(towards −∞): −2602.5 → −2603, −0.5 → −1, 0.5 → 0, 1999.9 → 1999; the one exception is −0.0, which
gives −1 [B11, M4]. (After the blending of section 6.3 most values are not whole numbers, e.g. the second value of the
room in section 10, −9714.3, is sent as −9715.)

## 8. Present but not heard

- **Per-sound environments.** Each emitter also selects an environment at its own position when it
  starts and, while playing, again after it is moved, and blends towards it like the listener's
  (section 6.3) [B12]. These are never sent to the device (the sound library contains no other EAX
  property set) [B12], so they have no audible effect.
- **Preset number and environment size:** read and blended but never sent (section 7.1) [B9].
- **An override environment:** the selection of section 5 is preceded by a test of an override mark
  which, when set, would return a fixed user environment instead; the mark starts unset and nothing in
  the sound library sets it [B13]. Whether any other program file sets it is **unestablished**
  (believed: no).

## 9. What is heard on `l01_escape`

`l01_escape` has no `level.snd_env` [D1]. After the program starts and New Game loads `l01_escape` as
the first level, there is no environment geometry (section 4.1), so [B6, B8, B14, M1]:

- **with reverb active** (section 2.2): the environment everywhere is the none environment; the
  current environment starts as it and stays at it up to `f32` rounding (section 6.3). Every update
  sends room −10000, room HF −100, room rolloff 0, decay time 1.49, decay HF ratio 0.83, reflections
  −2602, reflections delay 0.007, reverb 200, reverb delay 0.011, diffusion 1.0, air absorption HF −5,
  flags 0x3F; through rounding the room HF can be sent as −101 at some updates, depending on *dt* [M1].
  With room −10000 no reverberation is heard (believed, section 7.2).
- **with reverb not active** (`snd_efx` off, or a device on which EAX is not usable, including the two
  generic devices): the program sends nothing; whether the device applies a reverb of its own is the
  device's and **unestablished** (believed: none on a device without EAX).
- If a level with a `level.snd_env` was loaded before in the same run (e.g. a saved game on
  `l08_yantar`), its geometry stays in effect on `l01_escape`, at the same world coordinates (section
  4.1; believed, not executed).

So the faithful default for this level is: no reverb.

## 10. Verification facts

Data [D1 – D8]:

- `senvironment.xr`: 794 bytes, SHA-256
  `f8a857d6a6f17fae89ed5639c1995aaa6817acc54137993c263e889f1086dd4e`; 11 records, all version 4, in the
  order and with the values of section 3.4.
- SHA-256 of the `level.snd_env` files: `l07_military`
  `24451641ebd9ad2c92912113a8118600fbd3b0128f4001c08c14ff019386e38f`; `l08_yantar`
  `0c7ce22a8cbae565ed7cd4937f87f5ee38eb7c95667821c7865db3e3e272ac6e`; `l12u_sarcofag`
  `d327f1dbf345ca228fa4f9c7178e250934cc25a40f0cf3616267087054fb722b`; `testers_mp_factory`
  `d3473748e9c7e72d6edeaf97669800f74872786708da5903366e5a08d4bd49d9`; `testers_mp_pool`
  `af1cb2c3226cc6cfe0e53cef0f8630c1e44cdabb3a541781c2775602ac9c496e`.
- Names of chunk 0, in order: `l07_military` `open_space`, `mountains`, `house`, `long_corridor`,
  `room`, `bathroom`; `l08_yantar` `plant`, `forest`, `identity`; `l12u_sarcofag` `identity`, `default`,
  `plant`, `long_corridor`; `testers_mp_factory` `identity`, `default`, `plant`, `forest`;
  `testers_mp_pool` `long_corridor`, `identity` [D4].
- `testers_mp_pool`: one axis-aligned box from (−3.821, −6.962, −59.378) to (41.179, 18.038, −14.378)
  (rounded to 3 decimals); every triangle has front local index 1 (`identity`) and back 0
  (`long_corridor`) [D8, M2].

Worked examples, computed by `model.py` from sections 5 – 7 [M1 – M5]:

| Case | Result |
|---|---|
| no geometry (`l01_escape`), any position | none environment; integers sent: room −10000, room HF −100 [M1] |
| same, 20,000 updates with *dt* varying between 0.001 and 0.26 s | room always −10000; room HF −100 or −101 [M1] |
| `testers_mp_pool`, box centre | `long_corridor` (back side of the floor) [M2] |
| `testers_mp_pool`, 1 m above the box top, above its centre | `identity` (front side of the top) [M2] |
| `testers_mp_pool`, 1 m below the box, or 1 m beside it | no hit: none environment [M2] |
| current = none, target = `default`, *dt* = 0.016 | room sent after 1, 2, 3 updates: −9856, −9715, −9575; room ≥ −1100 first after 279 updates (4.46 s) [M3] |
| integer conversion of −2602.0, −2602.5, −0.5, 0.5, −0.0, 0.0, 1999.9 | −2602, −2603, −1, 0, −1, 0, 1999 [M4] |
| centre of each box, in group order: `l08_yantar` | `plant`, `plant`, `forest`, `plant`, `plant`, `plant` [M5] |
| same: `l12u_sarcofag` | `plant`, `plant`, `long_corridor` [M5] |
| same: `testers_mp_factory` | `forest`, `plant`, `plant`, `forest`, `plant`, `forest` [M5] |
| same: `l07_military` (30 boxes) | `open_space`, `house` × 12, `long_corridor`, `house` × 7, `long_corridor`, `open_space`, `room` × 4, `bathroom` × 2, `long_corridor` [M5] |

(The centre of a box is the midpoint of its vertices' minimum and maximum corner; its environment is
that of the first surface below the centre, which need not be the box's own floor.)

## 11. Unestablished

- Which OpenAL device the program opens (section 2.2), and so whether EAX is usable on a given system.
- What the device does with reverb values once the program stops sending them (section 2.1), and whether
  a device applies a reverb of its own when the program sends none (section 9).
- The device's values of the reverb controls the program does not set, and which sources the listener
  reverb applies to (section 7.2).
- That the 12 values are the EAX 2.0 parameters as named (believed from the agreement with the EFX
  guide, section 7.2), and that room −10000 is inaudible (believed).
- Whether a program file other than the sound library sets the override mark (section 8; believed: no).
- That a level without `level.snd_env` keeps the previous level's geometry (section 4.1; believed from
  static analysis, not executed), and the behaviour for *dt* > 1 s (section 6.3).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives
  `gamedata.db0` … `gamedata.dbd`, the default user settings file `__support\userdocs\user.ltx`, and the
  shipped program files of `bin\`.
- Data examined: `senvironment.xr` and every `levels\*\level.snd_env` file of the archives; the archive
  listings (every level directory).
- Analysis: static analysis (decompilation and disassembly with Ghidra, headless) of the shipped sound
  library, game library, engine executable and collision library; reading constants from them; the
  integer conversion was checked by emulating the instruction sequence on test values. No program was
  executed.
- Public documents: the Creative Labs "Effects Extension Guide" (OpenAL EFX, version 1.1, July 2006;
  file of 241 pages, SHA-256 `4b6a64abd083cf0dc278fe684377f45fee5e38e4f35965a4ec394743a60fea82`) and
  the OpenAL 1.1 specification (as S2).

## Gaps found in review

- Reverb on the `-dsound` device path (S2 section 2): how the device is tested for EAX 2.0 support there,
  and whether sections 6 and 7 apply unchanged, is not specified.
- Consistency with S2 section 7.2: the listener position sent to the device is the stored position of
  section 6.1, which changes only when a coordinate differs by 0.001 or more from it, not the camera
  position of every update; S2 does not say so.
