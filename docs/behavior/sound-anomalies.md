# Anomaly sounds

Behaviour specification S15 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`):
the sounds of the anomalies of the first level, `levels\l01_escape\` — idle, awaking, accumulation, blowout,
hits, entrances, artefacts thrown out and bodies torn — in its full scope: the 178 anomaly records of class ids
`Z_MBALD`, `ZS_MBALD`, `ZS_MINCE` and `Z_RADIO`, their 8 sections, the zone sections reached from the data in
play by reference (scope section 2), and the key `sound` the scope leaves open. For each sound it states which
key names it, when the zone creates it, when and where it plays and when it stops. What could not be
established is marked **unestablished**; what is inferred rather than read from the program or the data is
marked **believed**.

Builds on (not repeated):

- `docs/formats/sound-ogg-params.md` (S1): a sound name and its file, the file's min and max distance and
  base volume, the AI sound type (section 7.2, the value −1 meaning "the file's").
- `docs/behavior/sound-positional.md` (S2): creating a sound in a volume category (section 3.1), **play at
  position** with the flags looped and 2D and a delay (section 3.2), restarting a sound that still has an
  emitter (section 4.3), stopping at once (section 4.4), discarding a sound (section 3.1); everything about
  how a playing sound is heard (attenuation, occlusion, voices).
- `docs/behavior/zone-idle-effects.md` (specification P9 of `docs/behavior/particles-and-lights-scope.md`, in
  preparation; cited as **P9**): the zone object and its matrix *X* with position **p** (row 3), its five
  states (section 2), creation (section 3.1), the idle start and stop (sections 4.1, 4.2), the zone update
  and its elapsed time, fast mode and the scheduled update (section 6), the switching rule and how a change of
  state travels to the game's server and back (section 7). P9 owns the zones' on/off state; this
  specification only adds what the sounds need.
- `docs/behavior/offline-simulation.md` (which objects are present, section 6.5), `docs/formats/sav.md` (the records),
  `docs/formats/ltx-config.md` (key lookup with inheritance; reading a missing key is fatal).
- `docs/behavior/particle-lifecycle.md` section 2.1 (the random generator, 15-bit values 0 … 32767).

Conventions: times in milliseconds (ms); "the state timer" *τ* is the zone's time in its current state, and
*τ*₋ its value before the latest zone update (section 5.1). A key's value "names" the file
`sounds\<value>.ogg` (S1). LTX values are read after inheritance.

**Checks.** Every claim carries a tag; the checks are scripts in the dirty workspace,
`checks/sound-anomalies/`: `bin_zone.py` (B1 – B17, static analysis of the shipped program files),
`data.py` (D1 – D9, the game data), `model.py` (M1 – M4, a model of section 5 written from this text);
`run_all.py` runs them all (`ghidra.py` and `extract.py`, run first, prepare the evidence).

## 1. The records and their behaviours

On the first level, 178 records have one of the class ids `Z_MBALD`, `ZS_MBALD`, `ZS_MINCE`, `Z_RADIO`; no
other record there has a class id beginning `Z_` or `ZS_`; each record's class id is its section's `class`
[D1]:

| Class id | Section | Records |
|---|---|---|
| `ZS_MBALD` | `zone_mosquito_bald_weak` | 85 |
| `ZS_MBALD` | `zone_mosquito_bald_weak_noart` | 2 |
| `ZS_MBALD` | `zone_witches_galantine` | 9 |
| `ZS_MINCE` | `zone_mincer_weak_noart` | 4 |
| `Z_MBALD` | `zone_flame_small` | 11 |
| `Z_MBALD` | `zone_mine_field` | 49 |
| `Z_RADIO` | `zone_radioactive_average` | 1 |
| `Z_RADIO` | `zone_radioactive_killing` | 17 |

The class ids use three behaviours (P9 section 1.1 for the scripted ids), named here after their class ids:

- the **`Z_MBALD` behaviour**: `Z_MBALD`, `ZS_MBALD`;
- the **`ZS_MINCE` behaviour**: `ZS_MINCE`, and `ZS_GALAN` (of `zone_gravi_zone`, reached by reference,
  section 7.2), with the engine ids `Z_MINCER` and `Z_GALANT`;
- the **`Z_RADIO` behaviour**: `Z_RADIO`.

They share everything below unless a difference is stated [B1, B6, B10, B11, B13].

## 2. The sounds of a zone

### 2.1 Keys and creation

When the zone object is loaded from its section, it creates up to eight sounds, each a separate sound of
its own (so two keys naming the same file give two independent sounds) [B1]:

| Key | Read | Sound |
|---|---|---|
| `idle_sound` | if present | idle |
| `awake_sound` | if present | awaking |
| `accum_sound` | if present | accumulation |
| `blowout_sound` | if present | blowout |
| `hit_sound` | if present | hit |
| `entrance_sound` | if present | entrance |
| `artefact_born_sound` | if present, and only when `spawn_blowout_artefacts` is on | artefact thrown out |
| `body_tearing_sound` | always, by the `ZS_MINCE` behaviour only (missing: fatal) | body torn |

Every one is created in the **effects** category with the AI sound type −1, so the file's own AI sound type
applies (S1 section 7.2) [B1, B4]. A key that is absent (or not read) leaves its sound without data, and
playing a sound without data does nothing [B4]. The key names occur nowhere else in `xrGame.dll`, and in no
shipped script [B1, D8].

The `ZS_MINCE` behaviour also reads the required float `tele_height` (m), used for the body-torn sound's
position (section 7) [B1].

### 2.2 Times read with them

- `blowout_time`, `awaking_time`, `accamulate_time`: required (P9 section 2) [B2].
- `blowout_sound_time` (ms): optional, unsigned, 0 when absent. If it is greater than `blowout_time`
  (compared as signed integers), it is set to `blowout_time` and a line is written to the log [B2].
- `blowout_explosion_time` (ms): optional, 0 when absent, limited to `blowout_time` the same way [B2].

### 2.3 Common rules of playing

- Every zone sound is played with **play at position** (S2 section 3.2), with **no owner object**, delay 0,
  not 2D; only the idle sound is looped [B5, B7, B8, B11, B12, B13, B15, B16].
- Each sound has at most one emitter. Playing it again while its emitter exists restarts that emitter from its
  first sample at the new position (S2 sections 3.2, 4.3); so when several objects trigger the same sound in
  one update, only the last position is heard, from the start.
- Apart from the idle sound (section 3), no zone sound is ever stopped: each plays to its end, whatever the
  zone does meanwhile [B5, B6, B17].
- The whole of the zones' code plays sounds in exactly the places of sections 3 – 7 [B17].
- When the zone object is destroyed (with the level), its sounds are discarded (S2 section 3.1, which stops
  their emitters); the `ZS_MINCE` behaviour's body-torn sound is released without that discard [B14]; whether
  its emitter is stopped then is **unestablished** (it is not looped, so it ends by itself).

## 3. Changes of state

A change of state is requested by the zone (P9 sections 2, 6.2, 7) and takes effect when it comes back from
the game's server (P9 section 7). **Requesting** a change sets the state timer *τ* and *τ*₋ to 0 at once
[B9]. On **arrival** of a change to state *n*, in this order [B6]:

1. if the zone is idle: the idle stop (P9 section 4.2), which stops the idle sound at once [B5];
2. if *n* is idle: the idle start (P9 section 4.1), which plays the idle sound at **p**, looped, from its first
   sample (a new emitter) [B5]; if *n* is accumulation: the accumulation sound; if *n* is awaking: the
   awaking sound (below);
3. the state becomes *n*; *τ* and *τ*₋ are 0.

So a change to idle while already idle (P9 section 7 sends such repeats) stops the idle sound and starts it
again from its first sample [B5, B6].

- **Awaking sound** (`awake_sound`): on the arrival of awaking, after the awaking effect, if the sound has
  data: played once at **p** (the zone's position at that moment), not looped [B7].
- **Accumulation sound** (`accum_sound`): the same, on the arrival of accumulation [B7].
- The **idle sound** is P9's (sections 4.1 – 4.3): played at **p** on every idle start, never moved, stopped at
  every idle stop.

Enabling a disabled zone plays no sound but the idle sound (the entrance work done on enabling runs while
the zone is still disabled, section 6.1) [B12].

## 4. The state steps

The steps that request the changes (P9 sections 2, 6.1) [B10]:

- awaking: once *τ* ≥ `awaking_time`, request blowout;
- blowout: section 5;
- accumulation: once *τ* ≥ `accamulate_time`, request idle, or blowout again when the zone still has an
  object inside that counts (P9 section 2);
- the comparisons are of signed integers.

## 5. The blowout

### 5.1 The state timer

At every zone update (P9 section 6.1) with elapsed time *Δ*: *τ*₋ = *τ*, then *τ* = *τ* + *Δ*; then, unless
the zone is disabled, the state's step runs [B9]. In fast mode *Δ* is the frame's time; otherwise the zone
update runs inside the scheduled update with its interval (P9 section 6.2).

### 5.2 The blowout step

At each zone update in the blowout state [B10]:

1. if *τ* ≥ `blowout_time`: accumulation is requested, which sets *τ* and *τ*₋ to 0 (section 3);
2. the **timed events** run (section 5.3) with the current *τ*₋ and *τ*. (The `Z_MBALD` behaviour runs them
   in step 2 only while *τ* < `blowout_time`, and after a request only once per blowout; the `Z_RADIO`
   behaviour only while *τ* < `blowout_time`. After a request *τ*₋ = *τ* = 0, so none of them fires either
   way, and the rule is the same for the three behaviours.)
3. `ZS_MINCE` only: the zone then affects the objects inside (section 5.5) at every blowout step.

### 5.3 The timed events

A timed event with time *t* fires at the update with *τ*₋ ≤ *t* < *τ* (unsigned comparisons) [B8]. In this
order [B8]:

1. the blowout particles (`blowout_particles_time`), 2. the blowout light (`blowout_light_time`) — neither is
   sound;
3. **the blowout sound**, at *t* = `blowout_sound_time`: played once at **p**, not looped [B8];
4. the wind (`blowout_wind_time_start`), not sound;
5. **the explosion**, at *t* = `blowout_explosion_time`: the zone affects the objects inside (section 5.4),
   then it may throw out an artefact (section 5.6) [B8].

Together with section 5.2: the blowout sound plays at the first update of the blowout state at which
*τ* > `blowout_sound_time` while *τ* < `blowout_time`, *τ* counted from the arrival of the blowout state. A
zone whose updates come `blowout_time` or more apart (out of fast mode, far from the camera) never plays it,
nor does its explosion happen [M3]. With `blowout_sound_time` absent (0), the sound plays at the first update
with *τ* > 0 [M4].

The state remains blowout until the requested accumulation arrives; if more zone updates run before it
arrives, they start again from *τ* = 0 and may fire the events again (for instance a `blowout_sound_time` of
0 fires at the next update with *Δ* > 0). How many updates that is depends on the round trip, which is
**unestablished** (P9 section 7).

### 5.4 Hits and the hit sound

When the zone affects the objects inside, it does so at most once per frame and not while the game is paused
(believed: the condition read is the device's pause), for each object inside except those carrying one
flag of their own [B11]. For each such object, the `Z_MBALD` behaviour (at the explosion) and the `ZS_MINCE`
behaviour (at the blowout step whose update crosses `blowout_explosion_time`, for objects it does not
throw) compute a hit power; **if the power is greater than 0.01** (`f32`) the zone sends the hit to the
object and then plays the **hit sound** once at the object's position (row 3 of its matrix at that moment),
not looped [B11]. The `Z_RADIO` behaviour plays no hit sound [B11]. How the power is computed, which objects
count and the hit itself belong to a specification of anomaly behaviour (none approved;
`docs/behavior/player-hits.md` for its effect on the player).

### 5.5 The `ZS_MINCE` behaviour's other affecting

At its other blowout steps the `ZS_MINCE` behaviour moves objects inside rather than hitting them; this plays
no sound [B11, B17].

### 5.6 The artefact thrown out

A zone with `spawn_blowout_artefacts` on can hold artefacts: an artefact becomes held when an ownership
event from the server makes it the zone's child, which hides and disables it [B15]. At the explosion, if the
zone holds at least one, a value *k* (0 … 32767) is drawn from the generator of particle-lifecycle.md 2.1; if
*k* · *q* ≤ `artefact_spawn_probability`, *q* the `f32` nearest to 1/32767, the last held artefact is
removed from the zone's list and, if it is still the zone's child and a condition on the zone object holds
that this specification does not establish (believed always met in a single-player game), a release event
naming it is sent to the server [B15]. When the release arrives, the artefact is detached; unless the event marks it as
about to be destroyed, it is **thrown out**: placed at **p** + (0, `artefact_spawn_height`, 0), shown with
`artefact_spawn_particles`, and the **artefact-born sound** is played once at that point, not looped, then
the artefact is pushed (`throw_out_power`) [B15]. This is the only place the sound plays [B15]. When and how
the server gives a zone its artefacts is **unestablished** (offline simulation; the sections' keys
`min_artefact_count`, `max_artefact_count`, `artefact_spawn_idle`, `artefact_spawn_rnd` are not read by the
zone object, believed theirs).

## 6. Entrances

### 6.1 Objects coming inside

When the zone's contact test (P9 section 6.2) reports an object as newly inside (it is then added to the
zone's objects inside; believed: a new contact of that test), the **entrance sound** is played once at the
object's position, not looped, unless the zone is disabled [B12]. The ignore keys (`ignore_nonalive`,
`ignore_small`, `ignore_artefacts`) only mark the object in the zone's list; they do not prevent the
entrance sound [B12]. When a disabled zone is enabled, the objects already inside get no entrance sound
[B12]. Which objects the contact test reports belongs to the anomaly behaviour (P9's follow-up F1; none
approved).

### 6.2 Hits on the zone

When the zone object itself receives a hit (a hit event addressed to it), in any state, the **entrance
sound** is played once at the hit's point: the point the hit carries in the zone's own space, transformed by
*X* (row-vector convention: the point times the rotation rows plus **p**) [B13]. Which hits reach a zone
(believed: weapons' and explosions'; S18, S20) is **unestablished**.

## 7. Bodies torn

### 7.1 The body-torn sound (`ZS_MINCE` behaviour)

When the zone object is created, the behaviour fixes the point **b** = **p** + (0, `tele_height`, 0), with
**p** at creation; it never changes **b** [B16]. When a new object with a jointed physics body is created whose
record names this zone as the object to notify — the pieces of a body the zone destroyed — the zone is
notified once for that object, and then plays its tearing particles on it, the **body-torn sound** once at
**b**, not looped, and sends a hit to it [B16]. The name to notify is cleared after the notification, so each
created piece notifies once [B16]. How the zone comes to be named (believed: as the source of the hit that
destroyed the body) and which bodies can be torn are **unestablished** (characters and monsters, S24).
The `Z_MBALD` behaviour does not read `body_tearing_sound` even where a section has it (`zone_teleport`,
section 9.2) [B1, D6].

### 7.2 Sections reached by reference

The zone sections the data in play reaches only by reference (scope section 2) have no record on the first
level; were such an object present, sections 2 – 7 apply to it as to the records of section 1 [D5]:

| Section | Class | Sounds (file under `sounds\`) |
|---|---|---|
| `zone_gravi_zone` | `ZS_GALAN` | idle `anomaly\gravi_idle00`, blowout `anomaly\gravi_blowout6`, hit `anomaly\anomaly_gravy_hit1`, entrance `anomaly\gravi_idle01`, artefact `anomaly\anomaly_gravy_hit1`, body `anomaly\anomaly_body_tear_1`; `blowout_time` 2500, `blowout_explosion_time` 2100, no `blowout_sound_time`, `spawn_blowout_artefacts` on, `tele_height` 0.05 |
| `zone_mincer` | `ZS_MINCE` | as `zone_mincer_weak_noart` (section 9.1), but `spawn_blowout_artefacts` on |
| `zone_mosquito_bald` | `ZS_MBALD` | as `zone_mosquito_bald_weak`; `blowout_sound_time` 100, `blowout_explosion_time` 200, `blowout_time` 500 |
| `zone_radioactive` | `Z_RADIO` | none |

## 8. The key `sound`

Six of the eight sections have `sound = zone_mosquito_bald` (the file exists); `zone_flame_small` and
`zone_mine_field` have the key with an empty value [D2]. No program file reads a zone section's `sound`: of
the 15 X-Ray program files, only `xrGame.dll` (11 references) and `XR_3DA.exe` (4) refer to a string
`sound`, none refers into a longer string ending in it, and every one of these references reads something
else (the section `[sound]`, a model's `[sound]` section, a thunderbolt section, the weather's ambient effect
sections, XML elements, script names) [B3]. No shipped script reads it: the string `"sound"` occurs in three
scripts, as a key of a script logic section, a table key, and the section `[sound]` of a restrictor's logic
[D8]. So the key is **believed** unused: a zone plays no sound from it.

## 9. The first level

### 9.1 What each section plays

| Section (records) | Idle | Awaking, accumulation | Blowout (at *τ*) | Hit | Entrance | Artefact thrown out | Body torn |
|---|---|---|---|---|---|---|---|
| `zone_mosquito_bald_weak` (85) | `anomaly\gravi_rumble1` | — | `anomaly\anomaly_gravy_blast1` (> 100) | `anomaly\anomaly_gravy_hit1` | `anomaly\anomaly_gravy_hit1` | `anomaly\pux_blast` | — |
| `zone_mosquito_bald_weak_noart` (2) | as above | — | as above | as above | as above | — (key present, not read) | — |
| `zone_witches_galantine` (9) | `anomaly\electra_idle1` | — | `anomaly\electra_blast1` (> 0) | `anomaly\electra_hit` | `anomaly\electra_hit` | — | — |
| `zone_mincer_weak_noart` (4) | `anomaly\gravi_idle01` | `anomaly\gravi_idle01`, `anomaly\gravi_idle01` | `anomaly\anomaly_mincer_blowout` (> 100) | `anomaly\anomaly_gravy_hit1` | `anomaly\gravi_idle01` | — (key present, not read) | `anomaly\anomaly_body_tear_1` |
| `zone_flame_small` (11) | `ambient\fire2` | — | `ambient\fire2` (> 0) | `ambient\fire2` | `ambient\fire2` | — | — |
| `zone_mine_field` (49) | — | — | `weapons\f1_explode` (> 150) | `anomaly\anomaly_gravy_hit1` | `weapons\mine_on` | — | — |
| `zone_radioactive_average` (1), `_killing` (17) | — | — | — | — | — | — | — |

[D2, D3, D7] "(> *t*)": the blowout sound plays at the first blowout update with *τ* > *t* (section 5.3).
All the files exist [D2]. `spawn_blowout_artefacts` is on only in `zone_mosquito_bald_weak`
[D2]. Counted by record: idle 111, blowout, hit and entrance 160 each, artefact thrown out 85, awaking,
accumulation and body torn 4 each; the 18 radiation zones play nothing [D7].

Times (ms) [D3]:

| Section | `awaking_time` | `blowout_time` | `accamulate_time` | `blowout_sound_time` | `blowout_explosion_time` | other |
|---|---|---|---|---|---|---|
| `zone_mosquito_bald_weak`, `_noart` | 80 | 500 | 1000 | 100 | 200 | `artefact_spawn_probability` 0.09, `artefact_spawn_height` 3 |
| `zone_witches_galantine` | 100 | 1000 | 1000 | — | — | |
| `zone_mincer_weak_noart` | 50 | 10000 | 4000 | 100 | 5999 | `tele_height` 10 |
| `zone_flame_small` | 0 | 1000 | 0 | — | — | |
| `zone_mine_field` | 50 | 1000 | 100000 | 150 | 150 | |
| `zone_radioactive_average`, `_killing` | 100 | 100 | 100 | — | — | |

At the start of a New Game every zone is idle (P9 section 8), so the only anomaly sounds heard until
something enters or hits a zone are the 111 idle sounds, the galantines' switched on and off by their times (P9
section 8).

### 9.2 Sound files

The S1 parameters and lengths (S2: *D* = ⌊*N* · 1000 / 44100⌋) of the 13 files; all mono, 44,100 Hz, with a
version 3 record [D4]:

| File (`sounds\…`) | Min (m) | Max (m) | Base volume | AI type | Samples *N* | Length (ms) |
|---|---|---|---|---|---|---|
| `ambient\fire2` | 0.3 | 15 | 0.5 | 0 | 143,098 | 3,244 |
| `anomaly\anomaly_body_tear_1` | 2 | 100 | 2.0 | 0 | 293,677 | 6,659 |
| `anomaly\anomaly_gravy_blast1` | 40 | 200 | 1.0 | 0 | 71,960 | 1,631 |
| `anomaly\anomaly_gravy_hit1` | 10 | 60 | 1.0 | 0 | 41,991 | 952 |
| `anomaly\anomaly_mincer_blowout` | 40 | 200 | 1.0 | 0 | 453,351 | 10,280 |
| `anomaly\electra_blast1` | 30 | 200 | 1.0 | 0 | 183,295 | 4,156 |
| `anomaly\electra_hit` | 3 | 45 | 1.0 | 0 | 19,512 | 442 |
| `anomaly\electra_idle1` | 4 | 60 | 0.5 | 0 | 240,641 | 5,456 |
| `anomaly\gravi_idle01` | 4 | 60 | 1.0 | 0 | 142,723 | 3,236 |
| `anomaly\gravi_rumble1` | 5 | 60 | 1.0 | 0 | 111,309 | 2,524 |
| `anomaly\pux_blast` | 1 | 15 | 1.0 | 0 | 107,200 | 2,430 |
| `weapons\f1_explode` | 20 | 500 | 2.0 | 0x08000100 | 115,632 | 2,622 |
| `weapons\mine_on` | 1 | 50 | 1.0 | 0x08000100 | 6,781 | 153 |

## 10. Verification facts

### 10.1 All levels

- Records of the class ids of the three behaviours (`Z_MBALD` 402, `ZS_MBALD` 442, `ZS_MINCE` 106,
  `ZS_GALAN` 124, `Z_RADIO` 275): 1,349 on 17 levels, of 36 sections [D6].
- Their sections name 27 distinct sound files, all present, all mono 44,100 Hz with version 3 records [D6].
- Records whose section has the key: `idle_sound` 973, `blowout_sound` 1,075, `hit_sound` 1,074,
  `entrance_sound` 1,074, `artefact_born_sound` 456, `awake_sound` 106, `accum_sound` 106,
  `body_tearing_sound` 254, of which 230 are of the `ZS_MINCE` behaviour (`ZS_MINCE`, `ZS_GALAN`) and 24 are
  `zone_teleport` records (`Z_MBALD`), which do not read it [D6].
- `blowout_sound_time` is present in 13 sections and below `blowout_time` in each, so the clamping of section
  2.2 never applies; 18 sections have `blowout_sound` without it [D6].
- `artefact_born_sound` with `spawn_blowout_artefacts` off: the three `_noart` sections [D6].
- `zone_monolith` (`Z_RADIO`) has `idle_sound` and `blowout_sound` (both `anomaly\monolith_idle`): the
  `Z_RADIO` behaviour plays them by the common rules [D6, B1].
- No first-level anomaly record has a parent [D9].

### 10.2 Worked examples

Computed by `model.py` from section 5, assuming each requested change arrives before the next update:

| Case | Result |
|---|---|
| `zone_mine_field`, updates of 16 ms | blowout sound and hits at the 10th update (*τ* = 160); accumulation requested at the 63rd [M1] |
| `zone_mosquito_bald_weak`, updates of 33 ms | blowout sound at the 4th update (*τ* = 132), hits and artefact release at the 7th (231), accumulation requested at the 16th [M2] |
| `zone_mosquito_bald_weak` 70 m from the camera, out of fast mode (scheduled every 369 ms, P9 6.2) | blowout sound, hits and artefact release at the first update (*τ* = 369) [M3] |
| the same at 100 m (every 515 ms ≥ `blowout_time` 500) | no blowout sound, no hits, no artefact [M3] |
| `zone_witches_galantine`, 16 ms | blowout sound and hits at the first update [M4] |
| `zone_mincer_weak_noart`, 16 ms | blowout sound at *τ* = 112, hits at 6,000, accumulation requested at the 625th update [M4] |

## 11. Unestablished

- The number of updates a requested change of state takes to arrive, and so whether the timed events of the
  blowout can fire twice (section 5.3; P9 section 7).
- How the hit power is computed and which objects inside are hit, thrown or counted (section 5.4); which
  objects the contact test reports (section 6.1); the per-object flag that excludes objects from being
  affected (section 5.4): the anomaly behaviour, none approved.
- When and how the server gives a zone its artefacts, and the zone condition of the release (section 5.6).
- Which hits reach a zone object (section 6.2).
- How a zone comes to be named by the pieces of a torn body, and which bodies can be torn (section 7.1);
  whether the body-torn sound is stopped when the zone is destroyed (section 2.3).
- That the affecting skips paused frames because of the device's pause (believed, section 5.4).
- That no program or script uses the key `sound` beyond the references examined (believed, section 8).

## 12. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives `gamedata.db0` … `gamedata.dbd`;
  the shipped program files of `bin\` (`xrGame.dll`, `XR_3DA.exe`, `xrSound.dll` in detail; the other X-Ray
  program files searched for strings).
- Data examined: every record of `spawns\all.spawn` (decoded per sav.md); the zone sections through
  `config\system.ltx` with inheritance; the 27 sound files the anomaly sections name (comment headers and
  lengths); the 439 shipped scripts.
- Analysis: static analysis of the shipped program files (Ghidra 12.2, headless decompilation, instruction
  listings, reference searches): the zones' loading of their sounds, the state changes, the blowout's timed
  events, the hits dealt by the zones, the contact and hit entries, the artefact release and throw, the
  tearing notification, the sound library's creation and play; byte searches of all program files for
  references to strings ending in `sound`. A throwaway model of section 5 written from this text. No
  execution or debugging of the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/sound-anomalies/` reproduce every fact.

## Gaps found in review

- The program's own string `zone_mosquito_bald`, which the scoping specification (section 2, program files) assigns
  to S15, is not addressed. Established in review by static analysis of `xrGame.dll`: the string occurs there once,
  as the name the program gives the class id `Z_MBALD` among its object kinds; no sound is played from it.
