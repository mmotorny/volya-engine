# Detector sounds

Behaviour specification S21 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`):
the sounds the anomaly detector makes near anomalies, in its full scope: which objects are detectors and
which configuration they read, when a detector is switched on, which zones it notices, what decides when
and how fast it ticks, and what each tick plays. The detector of the first level, `levels\l01_escape\`, is
the `detector_simple` of the player's starting inventory. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated):

- `docs/formats/sound-ogg-params.md` (S1): a sound name and its file, the file's min and max distance and
  base volume, the AI sound type and its flag names (section 7.2), the reports to the AI (section 7.1).
- `docs/behavior/sound-positional.md` (S2): creating a sound in a volume category (section 3.1),
  **play at position** with the flags looped and 2D and a delay (section 3.2), the play volume *v* and
  stopping at once (sections 3.3, 4.4), discarding a sound (section 3.1), 2D emitters (section 7.3),
  everything about how a playing sound is heard.
- `docs/behavior/zone-idle-effects.md` (P9): the zone object, its matrix *X* with position **p** (row 3),
  its states (section 2; **disabled** is one of them), its creation (section 3.1), the **shape sphere**
  (section 6.2), the frame's elapsed milliseconds and the per-frame pass, which does not run while the game
  is paused (section 6.1), the scheduled updates of objects (section 6.2).
- `docs/behavior/sound-anomalies.md` (S15): the zones' own sounds; the class ids of the zones.
- `docs/behavior/player-state.md`: the starting inventory (section 3.1) and the placement of items in
  slots, belt and backpack (section 3.3).
- `docs/behavior/particle-lifecycle.md` section 2.1: the shared random generator, *k* = 0 … 32767.
- `docs/formats/ltx-config.md`: key lookup with inheritance; reading a missing key is fatal; booleans.
- `docs/formats/lanims-xr.md` section 6.2 (precision): the game computes in single precision, rounding to
  nearest.

Conventions: "f32" is IEEE 754 single precision, and every operation written with f32 values rounds to
f32. Times are in milliseconds (ms) unless stated. A key's value "names" the file `sounds\<value>.ogg`
(S1). LTX values are read after inheritance. *n* numbers the entries of a detector's table (section 2.1).

**Checks.** Every claim carries a tag; the checks are scripts in the dirty workspace,
`checks/sound-detector/`: `bin_det.py` (B1 – B16, static analysis of `xrGame.dll`), `bin_engine.py`
(E1 – E8, static analysis of `XR_3DA.exe` and the processing of objects), `data.py` (D1 – D8, the game
data), `model.py` (M1 – M4, a model of sections 5 and 6 written from this text); `run_all.py` runs them all
(`ghidra.py` and `extract.py`, run first, prepare the evidence).

## 1. Detectors

Objects of class id `D_SIMDET` are detectors [B1]. Five sections have this class [D1]:

| Section | `radius` (m) | `slot` | Table | Map locations | `night_vision_particle` |
|---|---|---|---|---|---|
| `detector_simple` | 10 | 8 | section 7.1 | — | — |
| `detector_advances` | 10 | 8 | as `detector_simple`, one sound per entry, all `detectors\DA-2_beep1` | `anomaly_zone_location` | `damage_fx\heli_burn` |
| `detector_elite` | 10 | 8 | as `detector_advances`, but entry 3 `detectors\geiger_click3` | `anomaly_zone_location` | `damage_fx\heli_burn` |
| `mp_detector_advances` | 10 | 8 | as `detector_elite` | `anomaly_zone_location` | `damage_fx\heli_burn` |
| `kruglov_flash` | 10 | — | as `detector_elite` | — | — |

[D1, D2] (Table: the same four zone classes and frequencies in all five, section 7.1.) No record of
`spawns\all.spawn` has the class id `D_SIMDET` [D1]. On the first level the one detector is the
`detector_simple` of the player's starting inventory (player-state.md section 3.1). Seven calls in six
scripts (`task_level2` … `task_level7`) create a `detector_simple` for the player's character [D8]; these
are scripts' (not followed here); such a detector behaves as below.

## 2. Configuration

### 2.1 The table of zone classes

When the detector is loaded from its section *S* it reads [B2]:

- `radius` (required float): the detector radius *ρ* (m);
- for *n* = 1, 2, 3, … as long as the key `zone_class_<n>` exists (the first missing number ends the
  table): an **entry** *n*:
  - `zone_class_<n>` names a section; that section's `class` (required) is the entry's **zone class id**
    (an 8-character class id such as `ZS_MBALD`);
  - `zone_min_freq_<n>` and `zone_max_freq_<n>` (both required floats): the frequencies *f*min and *f*max
    (ticks per second); the program asserts *f*min < *f*max (an assertion failure otherwise);
  - the **sound list** of `zone_sound_<n>_` (section 2.2);
  - `zone_map_location_<n>` (optional): not sound (section 9).
- `night_vision_particle` (optional) and `ef_detector_type` (required): not sound (section 9).

The entries are kept by zone class id; an entry naming a class id that an earlier entry already named
replaces that entry's frequencies and sound list (**believed** from the program; no section repeats a class
id [D2]) [B2]. These key names are read nowhere else in the program [B2] and by no shipped script [D3].

### 2.2 Sound lists

The sound list of a key prefix *K* (here `zone_sound_<n>_`) reads the keys *K*, *K*`1`, *K*`2`, … (*K*
followed by the decimal number) as long as the key exists; the first missing one ends the list [B3]. An
empty list (no key *K*) is an assertion failure [B3]. Each value is a comma-separated list [B3]:

1. the sound name;
2. the **volume** (float; 1 when the item is absent or empty);
3. the **delay** in seconds (float; 0 when absent or empty).

Each sound is created in the **effects** category with the AI sound type `item` (0x40000000; S1 section
7.2), so the file's own AI sound type is not used [B3]. No detector's sound value has a comma: every sound
has volume 1 and delay 0 [D2].

### 2.3 Keys not read

The detector sections' keys `noise` and `buzzer` (both `zone_mosquito_bald`, a sound file, S15) and
`buzzer_radius` (1.5) are read by no program file and no shipped script [D3, B15]: no program file contains
the string `buzzer`; the one whole string `noise` of `xrGame.dll` names a property of the scripts'
post-processing interface [B15]; no script reads a key `noise` [D3]. So the detector plays no sound from
them (**believed**, as the string search is the evidence).

## 3. On, off and carrying

- A detector is **off** when it is created [B4].
- Whenever the inventory puts it into a **slot** or onto the **belt**, it is switched **on**; whenever it
  puts it into the **backpack**, it is switched **off** (player-state.md section 3.3 for where an item goes)
  [B5]. Taking it out of the inventory (dropping it) does not switch it [B5]. So the starting
  `detector_simple`, placed in slot 8, is on from the start; `kruglov_flash`, which has no `slot` key and no
  `belt` key, would go to the backpack and stay off.
- Its **holder** is its parent object when that parent is of the player character's class (class id
  `O_ACTOR`), and none otherwise (another character's detector has no holder) [B6]. The holder is set when
  the detector gets a parent and cleared when it is created and when it loses its parent [B6, B8].
- When the detector loses its parent (it is dropped or otherwise let go), every sound of its lists is
  stopped at once and its zone list and range list (section 4) are emptied [B8].
- When the detector object is destroyed, every sound of its lists is discarded (S2 section 3.1) [B14].

## 4. Zones in range

### 4.1 When the range test runs

The detector has scheduled updates (P9 section 6.2: the general object schedule; the detector's code does
not change its interval limits, **believed** that none of its general parts does either) [E7]. At each one,
while the detector is on and has a parent [B7]:

1. the detector's position becomes its parent's position (row 3 of the parent's matrix);
2. if the parent is the object the level's player controls (**believed**: the player's character), the
   **range test** runs at that position **q** with the radius *ρ* [B7].

### 4.2 The range test

The detector keeps a **range list** of objects and a **zone list** of zones, each zone with a **counter**
*c* (ms) [B9]. A range test [E1, E2]:

1. finds every object of the level that is enabled and has a collision shape (E4; every zone is enabled when
   it is created, E4), other than objects marked as being destroyed (**believed**; the mark is cleared when
   an object is created, E1), whose bounding sphere (centre **c**, radius *R*) satisfies, in f32,
   ((*q*x − *c*x)² + (*q*z − *c*z)²) + (*q*y − *c*y)² < (*R* + *ρ*)² [E2]. A zone's
   bounding sphere is its shape sphere (P9 section 6.2) with its centre placed by *X* [B11, E3]. (The
   search first gathers candidates by a box about **q**; that it never removes an object that passes the
   sphere test is **believed**.)
2. Of those, only zones count (objects of any zone kind; script and smart zones and other objects never
   count) [B9]. Each counting zone not yet in the range list is added to it, and then, **unless its state is
   disabled**, added to the zone list with *c* = 0 [E1, B9].
3. Each zone of the range list that was not found, or is marked as being destroyed, is removed from the
   range list and from the zone list [E1, B10].

A zone stays in the range list as long as it is found, and is added only when it comes into range: so a
zone that is disabled when it comes into range does not get into the zone list until it has left range and
come back, even if it is enabled meanwhile; and a zone already in the zone list stays there when it becomes
disabled [E1, B9]. No list is changed while the detector is off or not carried by the controlled object
(the range test does not run), except as section 3 states [B7, B8].

## 5. Ticking

### 5.1 When

Every frame, in the per-frame pass over the game's objects (P9 section 6.1: not while the game is paused),
the detector updates its zone list if it is on, has a parent and has a holder (section 3) [B11, E5]. That
the detector takes part in the per-frame pass while it is in the inventory is **believed**: objects take part
from their creation, and taking an item into a slot adds and removes one part of the same count (E6). Each
zone of the zone list is updated in turn (the order is **unestablished**; it matters only for the shared
random generator, section 6) [B11].

### 5.2 The rate

For a zone *Z* of the zone list, *Z* is skipped (its counter unchanged) when the detector's table has no
entry for *Z*'s class id, or when *Z*'s section has `visible_by_detector` off (a required boolean, read
when the zone is loaded) [B11]. Otherwise, with the entry's *f*min, *f*max, **h** the holder's position and
**p** the zone's position (rows 3 of their matrices), *R* the zone's shape-sphere radius, all f32 [B11]:

1. *s* = √((*h*x − *p*x)² + (*h*y − *p*y)² + (*h*z − *p*z)²) (summed x, y, z);
2. *d* = *s* − *R* · 0.8 (the f32 nearest to 0.8, 0x3F4CCCCD); *d* = 0 when *d* < 0;
3. *u* = 1 − *d* / *ρ*, clamped to 0 … 1;
4. *f* = (((((*f*max − *f*min) · *u*) · *u*) · *u*) · *u*) + *f*min.

So *f* = *f*max while the holder is within 0.8 *R* of the zone's position, falls with the fourth power of
*u*, and is *f*min from 0.8 *R* + *ρ* on, while the range test keeps the zone up to *R* + *ρ* from the shape
sphere's centre [M1]. (On the first level a zone's position is its shape sphere's centre, D6.)

### 5.3 The counter

Then, with *T* = 1000 / *f* (f32 division) and *Δ* the frame's elapsed milliseconds (P9 section 6.1) [B12]:

- if *c* > *T* (compared exactly; *c* is an unsigned 32-bit integer): *c* = 0 and the zone **ticks**
  (section 6);
- otherwise *c* = *c* + *Δ*.

A frame with *c* = *T* exactly does not tick [M4]. With constant *Δ* and *f*, the first tick comes at the
(⌊*T* / *Δ*⌋ + 1)-th per-frame update after the one that first finds *c* = 0, counting that one as the 0th,
and then every ⌊*T* / *Δ*⌋ + 2 frames [M2]. Each zone has its own counter, so several zones in range tick
independently.

## 6. The tick

A tick of zone *Z* plays the sound list of *Z*'s entry [B12, B13]:

1. every sound of that list is **stopped at once** (S2 section 4.4) [B13, B8];
2. one sound is chosen: index *k* mod *N*, *k* drawn from the shared generator (particle-lifecycle.md 2.1)
   and *N* the list's length [B13];
3. it is played with **play at position** (S2 section 3.2): **2D**, not looped, at the position (0, 0, 0)
   (relative to the listener: at the listener), with the sound's delay (section 2.2), associated with the
   detector object [B12, B13];
4. if it then has an emitter, its play volume *v* is set to the sound's volume (section 2.2) [B13].

So the ticks of all zones of one entry share one list: each tick cuts off the previous tick of the same
entry; ticks of different entries overlap. A 2D emitter at (0, 0, 0) has *δ'* = 0, within every file's min
distance, so it is heard at full level for its volume (S2 section 7.3); its volume is that of the effects
category (S2 section 5.4). The sounds' AI type is `item`; reports to the AI are S24's (S1 section 7.1).

## 7. The first level

### 7.1 `detector_simple`

| *n* | `zone_class_<n>` | Zone class id | *f*min | *f*max | Sounds (in list order) |
|---|---|---|---|---|---|
| 1 | `zone_mosquito_bald` | `ZS_MBALD` | 0.1 | 40 | `detectors\DA-2_beep1` ×4 |
| 2 | `zone_mincer` | `ZS_MINCE` | 0.1 | 20 | `detectors\DA-2_beep1` |
| 3 | `zone_radioactive` | `Z_RADIO` | 1 | 20 | `detectors\geiger_1`, `geiger_2`, `geiger_3`, `geiger_4`, `geiger_5`, `geiger_6`, `geiger_1`, `geiger_1`, `geiger_1` |
| 4 | `zone_gravi_zone` | `ZS_GALAN` | 0.5 | 30 | `detectors\DA-2_beep1` |

[D2] *ρ* = 10 m; 15 `zone_sound_…` keys; every volume 1, every delay 0; no map locations [D1, D2].

### 7.2 The zones

`visible_by_detector` is on in `zone_mosquito_bald_weak`, `zone_mosquito_bald_weak_noart`,
`zone_witches_galantine`, `zone_mincer_weak_noart`, `zone_radioactive_average` and
`zone_radioactive_killing`, and off in `zone_flame_small` and `zone_mine_field` [D5]. So of the 178 zone
records (S15 section 1), 118 tick: entry 1 for the 96 `ZS_MBALD` zones, entry 2 for the 4 `ZS_MINCE`, entry 3
for the 18 `Z_RADIO`; the 60 `Z_MBALD` zones (flames and mines) never tick, as neither their class id is in
the table nor are they visible [D5]. Every first-level zone has one sphere centred at its own origin, so its
shape sphere's centre is its position **p**; the 118 radii are 1.086 … 7.71 m [D6]. From the position of the
actor's record (start-position.md section 2) the nearest ticking zone is a `zone_mincer_weak_noart` 88.10 m
away, so no zone is in range at the start of a New Game [D6].

### 7.3 Sound files

All mono, 44,100 Hz, with a version 3 record; S2 lengths *D* = ⌊*N* · 1000 / 44100⌋ [D4]:

| File (`sounds\detectors\…`) | Min (m) | Max (m) | Base volume | AI type (file) | Samples *N* | Length (ms) |
|---|---|---|---|---|---|---|
| `da-2_beep1` | 1 | 300 | 1.0 | 0 | 5,952 | 134 |
| `geiger_1` | 1 | 300 | 1.0 | 0 | 4,604 | 104 |
| `geiger_2` | 1 | 300 | 1.0 | 0 | 4,607 | 104 |
| `geiger_3` | 1 | 300 | 1.0 | 0 | 5,190 | 117 |
| `geiger_4` | 1 | 300 | 1.0 | 0 | 5,367 | 121 |
| `geiger_5` | 1 | 300 | 1.0 | 0 | 7,696 | 174 |
| `geiger_6` | 1 | 300 | 1.0 | 0 | 5,424 | 122 |
| `geiger_click3` | 1 | 300 | 1.0 | 0 | 4,824 | 109 |

(`detectors\DA-2_beep1` names `da-2_beep1.ogg`; the last file is used by the other detectors only.)

## 8. Verification facts

### 8.1 All levels

- 866 of the 1,443 zone records tick for `detector_simple`: `ZS_MBALD` 361, `Z_RADIO` 275, `ZS_GALAN`
  124, `ZS_MINCE` 106, on 16 levels [D7].
- 81 records of a class id in the table have `visible_by_detector` off: sections `zone_buzz`,
  `zone_buzz_average`, `zone_zharka_static_average`, `zone_zharka_static_strong`,
  `zone_zharka_static_weak` (all `ZS_MBALD`) [D7].
- Every zone record's section has `visible_by_detector`; of the 62 sections with a `Z_`/`ZS_` class, only
  `zone_team_base` lacks it [D7].
- In all five detector sections no class id repeats and *f*min < *f*max [D2].

### 8.2 Worked examples

Computed by `model.py` from sections 5 and 6 (`detector_simple`, *ρ* = 10):

| Case | Result |
|---|---|
| entry 1, *R* = 2.7388 (a `zone_mosquito_bald_weak`): *s* ≤ 2.191 / 5 / 10 / ≥ 12.192 m | *f* = 40 / 10.769 / 0.19195 / 0.1 [M1] |
| the same, frames of 16 ms, at the zone | ticks at frames 2, 5, 8, …: every 48 ms [M2] |
| at 5 m (*T* = 92.856) | frames 6, 13, 20, …: every 112 ms [M2] |
| at 10 m (*T* = 5,209.6) | frames 326, 653, …: every 5,232 ms [M2] |
| at *f*min 0.1 (*T* = 10,000) | frames 626, 1,253, …: every 10,032 ms [M2] |
| entry 3, *R* = 6.0904 (a `zone_radioactive_killing`), *s* = 3 | *f* = 20, *T* = 50: every 5 frames at 16 ms (80 ms), every 3 frames at 33 ms (99 ms) [M3] |
| *f* = 40 (*T* = 25) | every 2 frames at 26 ms, every 3 frames at 25 ms (*c* = *T* does not tick) [M4] |

## 9. Not sound

The map locations (`zone_map_location_<n>`) and the night-vision particles (`night_vision_particle`,
shown on zones while the player's night vision is on) play no sound [B16]; they belong to specifications
of the map and the torch.

## 10. Unestablished

- The order in which the zones of the zone list are updated (section 5.1).
- That the detector takes part in the per-frame pass while in the inventory (believed, section 5.1), and how
  other inventory operations change that.
- That the range test's box search removes nothing the sphere test keeps; what marks an object as being
  destroyed (section 4.2).
- That the object the level's player controls is the player's character (believed, section 4.1).
- That no general part of the detector changes its schedule interval (believed, section 4.1).
- That `noise`, `buzzer` and `buzzer_radius` are used nowhere (believed from the string search, section
  2.3).
- What follows an assertion failure (sections 2.1, 2.2).

## 11. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives `gamedata.db0` … `gamedata.dbd`;
  the shipped program files of `bin\` (`xrGame.dll` and `XR_3DA.exe` in detail, `xrSound.dll` as examined
  for S2 and S15; all program files searched for strings).
- Data examined: the detector and zone sections through `config\system.ltx` with inheritance; every record
  of `spawns\all.spawn` (decoded per sav.md) and the zone blocks of the zone records; the `sounds\detectors\`
  files (comment headers and lengths); the 439 shipped scripts.
- Analysis: static analysis of the shipped program files (Ghidra 12.2, headless decompilation, instruction
  listings, reference and string searches): the detector's loading, switching, scheduled and per-frame
  updates and its playing of sound lists; the engine's range test, object search, objects' bounding spheres,
  enabling and per-frame processing; the inventory's placement. A throwaway model of sections 5 and 6
  written from this text. No execution or debugging of the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/sound-detector/` reproduce every fact.

## Gaps found in review

- What the per-frame update does with a zone of the zone list that is destroyed between two range tests (section 4.2 removes it only at the next range test) is not stated.
- Whether saving and loading a game keeps the detector's on state, its range and zone lists and the zones' counters, or rebuilds them from the placement of section 3 and the next range test, is not stated.
