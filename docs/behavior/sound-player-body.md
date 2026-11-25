# The player's body sounds

Specification S17 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`): the sounds
of the player character's own body in a single-player game. It covers the pain sounds that a hit plays, by hit
type; heavy breathing; the heartbeat; the tinnitus of explosions together with the shock that lowers the volume
of all effects sounds while it rings (and the visual effect that times it); and the death sounds, which the
same configuration read creates and which the scoping specification did not list. What makes hits, death
itself and the drawing of visual effects are not specified here. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated here):

- `docs/behavior/player-hits.md` (cited as `hits`): the hit and its type `t` and power `P` (section 2), hit
  type names (section 2), the order of applying a hit (section 4), the wounds and the **total bleeding**
  (section 5.2), and the 15-bit random generator shared by the whole game (section 4.6). Section 7 of `hits`
  lists the effects specified here.
- `docs/behavior/player-state.md` (cited as `state`): health (section 2.3), the **limping** state and its
  thresholds (section 4.3).
- `docs/behavior/player-movement.md` (cited as `movement`): the eye height (section 7).
- `docs/behavior/sound-positional.md` (cited as `S2`): sounds, emitters, the play requests (play at
  position, looped, 2D, delay), the emitter controls (position, play volume *v*), stopping at once, the volume
  categories and the **effects factor** *F* (section 5.4), listener-relative 2D emitters (section 7.3) and the
  device distance factor (section 7.1). This specification settles S2's unestablished item "when the shock
  effect lowers the effects factor and by how much".
- `docs/formats/sound-ogg-params.md` (cited as `S1`): the file parameters (min distance *m*, max distance
  *M*, base volume *b*, AI sound type and its names, max AI distance) and the rule that a sound created
  without an AI sound type takes the file's (section 7.2).
- `docs/behavior/sound-weather-ambient.md` section 3.1 (the **global clock**: real time in milliseconds,
  taken once per frame, stopped while the game is paused) and `docs/behavior/sound-rain-thunder.md` section
  3.1 (the smoothed **frame time** *dt*, 0 on a paused frame).
- `docs/formats/ltx-config.md`: configuration files, sections, inheritance, values.

Conventions:

- "Configuration" is the `system.ltx` tree (winning copies of the combined archives). Sound names name files
  `sounds\<name>.ogg` (S1).
- *T* is the global clock in seconds and Δ the number of milliseconds by which it advanced since the previous
  frame; *dt* is the frame time in seconds.
- The **controlled character** is the object the level treats as driven by the player's input; the **viewed
  character** is the object the level's camera follows. In normal play both are the player character
  (believed; the same notion of "the character the player controls" as `hits` section 4.1).
- "Alive" means health above 0.
- Statements are **static** (read from the program and the data; the game was not run).
- Bracketed ids are the checks that reproduce the statement next to them: `D…` in `data.py` (game data),
  `B…` in `binary.py` (the program files), `M…` in `model.py` (numbers computed from this text). All are in
  the dirty workspace's `checks/sound-player-body/`; `run_all.py` extracts the files (`extract.py`),
  prepares the program evidence (`ghidra.py`) and runs them all.

## 1. Configuration and files

`config\creatures\actor.ltx`, section `[actor]` (the player's section) [D1]:

| Key | Value | Used in |
|---|---|---|
| `hit_sounds` | `actor_hit_snds` | 2, 3 |
| `heavy_breath_snd` | `actor\breath_1` | 2, 4 |
| `heavy_blood_snd` | `affects\heartbeat` | 2, 4 |

Section `[actor_hit_snds]` has exactly one key per hit type, named by the type's name (`hits` section 2), and
no other key; each value is a comma-separated list of sound names [D2]:

| `t` | Key | Sounds |
|---|---|---|
| 0 | `burn` | `actor\pain1`, `actor\pain2`, `actor\pain3` |
| 1 | `shock` | the same three |
| 2 | `strike` | the same three |
| 3 | `wound` | the same three |
| 4 | `radiation` | `monsters\biting\def_0` |
| 5 | `telepatic` | `actor\pain1`, `actor\pain2`, `actor\pain3` (never played, section 3) |
| 6 | `chemical_burn` | `actor\pain1`, `actor\pain2`, `actor\pain3` |
| 7 | `explosion` | `affects\tinnitus3a` |
| 8 | `fire_wound` | `actor\bullet_hit_1`, `actor\bullet_hit_2`, `actor\bullet_hit_3`, `actor\bullet_hit_4` |
| 9 | `wound_2` | `actor\hurt1`, `actor\hurt2`, `actor\hurt3`, `actor\hurt4` |
| 10 | `physic_strike` | `actor\pain1`, `actor\pain2`, `actor\pain3` |

These are 13 distinct files, all present [D2]. The death sounds are the files `sounds\actor\die0.ogg` …
`die3.ogg`, all present [D4] (section 2).

Section `[snd_shock_effector]` (`config\misc\postprocess.ltx`) has exactly `pp_eff_name = snd_shock.ppe` and
`pp_eff_cyclic = 1`, and no `cam_eff_name`; the file `anims\snd_shock.ppe` exists, and no level directory has
a file of that name [D3] (section 5).

The files' parameters (S1; every one is mono, 44,100 Hz, with a version 3 record; duration *D* as in S2)
[D5]:

| File (under `sounds\`) | *m* | *M* | *b* | AI sound type | Max AI | *D* (ms) |
|---|---|---|---|---|---|---|
| `actor\pain1`, `pain2`, `pain3` | 1 | 50 | 1 | 0x20010000 `monster_injure` | 25 | 354, 459, 334 |
| `monsters\biting\def_0` | 1 | 300 | 1 | 0 | 300 | 128 |
| `actor\bullet_hit_1` … `_4` | 1 | 5 | 1 | 0 | 5 | 169, 160, 176, 217 |
| `affects\tinnitus3a` | 1 | 300 | 1 | 0 | 300 | 6,000 |
| `actor\hurt1` … `hurt4` | 1 | 30 | 1 | 0x20010000 `monster_injure` | 30 | 623, 264, 244, 366 |
| `actor\breath_1` | 1 | 73 | 1 | 0x20004000 `monster_talk` | 10 | 2,202 |
| `affects\heartbeat` | 1 | 300 | 1 | 0 | 300 | 3,376 |
| `actor\die0` … `die3` | 1 | 30 | 1 | 0x20020000 `monster_die` | 30 | 1,359, 1,826, 1,526, 808 |

The three keys of `[actor]` are read by the program only when it reads the player's configuration; the name
`actor_hit_snds` occurs only in the data [B7]. The program files contain the string `affects\tinnitus3a`
once, but it belongs to the set-up of a monster (next to the sounds under `monsters\controller\`), not to the
player (S24): the player's tinnitus is named only by `[actor_hit_snds]` `explosion` [B7]. Of the 439 shipped
scripts, only `sr_teleport.script` (once) and `xr_effects.script` (twice) name one of the files above,
`affects\tinnitus3a`, as a script sound of their own (S24); none names the sections or keys above [D6].

## 2. Creating the sounds

When the program reads the player's configuration section (believed: once, when the player object is created,
before it comes online), it creates these sounds, all in the **effects** category (S2 section 3.1) [B1 – B4]:

1. **Hit sounds.** For each hit type `t` = 0 … 10: the key named by the type's name in the section named by
   `hit_sounds` (read without a default: a missing `hit_sounds` is an error) is tested; its value is split at
   commas and each item becomes one sound of the list of type `t`, in order. These sounds are created without
   an AI sound type of their own, so each takes its file's (S1 section 7.2) [B1, B4]. Without the key the
   program reads the empty text; whether that gives an empty list is **unestablished** (every type has a key
   in this edition) [B21].
2. **Breathing and heartbeat.** The sound named by `heavy_breath_snd` and the sound named by
   `heavy_blood_snd`, both read without a default, both with the AI sound type 0x20010000 (`monster_injure`),
   overriding the files' [B2].
3. **Death sounds.** The four sounds `<name>\die0` … `<name>\die3`, with the AI sound type 0x20020000
   (`monster_die`) [B3]. `<name>` is the player object's name at that moment, which the engine has just set to
   the section name (the program's own object set-up, run first, sets the name to the section) [B3]; for the
   player's section `actor` the sounds are `actor\die0` … `actor\die3`.

When the player object is destroyed, the breathing and heartbeat sounds are discarded, which stops their
emitters at once (S2 section 3.1) [B11, B4]; that the hit and death sounds are discarded in the same way is
believed.

## 3. Hit sounds

Every hit `(t, P)` on the player in a single-player game (`hits` section 2) first does the following, at the
moment the hit arrives, before the effects of `hits` section 4 [B5, B20]:

1. Let **alive** be "health above 0" with the health before this hit [B5].
2. If the list of type `t` is empty, or `t` is 5 (`telepatic`), nothing of this section happens [B5].
3. **Pick:** `s` = the item `r mod n` of the list (counting from 0), where `n` is the list's length and `r`
   the next value 0 … 32767 of the shared 15-bit generator (`hits` section 4.6; it advances the state
   *x* ← *x* · 214013 + 2531011 modulo 2³² and yields bits 16 … 30 of the new state) [B5]. The generator is
   stepped whenever this item is reached, also when no sound then plays.
4. **Busy:** the list is busy when any of its sounds currently has an emitter (S2: from its play until its
   emitter ends) [B5].
5. **Explosion** (`t` = 7) [B5]:
   - If the player is not the controlled character, nothing further of this section happens (no sound).
   - Otherwise: if `s` has an emitter, its play volume *v* is set to 10 (S2 section 3.3); then, if no shock is
     running, a shock starts (section 5) with the length *L* = the duration *D* of `s` in milliseconds and the
     power `P` as the hit arrived (the same value as the slowdown of `hits` section 4.1 uses).
6. **Play:** if alive and the list is not busy, `s` is played at a position (S2 section 3.2): the player's
   position raised by the eye height *h* = (height of the current collision box) × `camera_height_factor`
   (the computation of `movement` section 7: 1.53 m standing), not looped, 3D, without delay, associated with
   the player object [B5, B6]. The position is not updated afterwards.

Consequences [M6, M7]:

- At most one sound per hit type plays at a time; a hit of a type whose sound is still playing is silent.
  Sounds of different types (for example `pain` and `bullet_hit`, which are different lists) can overlap.
- Radiation hits (`t` = 4) play `monsters\biting\def_0` (128 ms); `chemical_burn`, `burn`, `shock`, `strike`,
  `wound` and `physic_strike` share the three `pain` files but are separate lists, so each has its own busy
  test.
- The sound starts at the eye, where the listener (the camera) is, so it is within its min distance of 1 m
  and heard at full level (S2 section 7.4) while the player stays near that point.
- From a generator state of 0 the next `r` is 38: a `fire_wound` hit picks `actor\bullet_hit_3`, a `pain` list
  picks `actor\pain3`.

A dead player (health 0 or less before the hit) hears no hit sound, but an explosion on the controlled
character still starts a shock, which then ends at the next frame (section 5.2).

## 4. Heavy breathing and heartbeat

At each scheduled update of the player character (its interval is **unestablished**), while the player is the
controlled character, the program does in this order [B8]:

1. **Breathing.** If the limping state is on (`state` section 4.3, re-evaluated by this test) and the player
   is alive: if the breathing sound has no emitter, it is played at position (0, 1.75, 0), looped and 2D,
   without delay, associated with the player object; if it has one, its position is set to (0, 1.75, 0).
   Otherwise (not limping, or not alive), if it has an emitter, it is stopped at once [B8, B4].
2. **Heartbeat.** Let *B* be the total bleeding (`hits` section 5.2: the mean size of the wounds, 0 without
   wounds) [B8]. If *B* > 0.6: the heartbeat sound is played or has its position set exactly as the breathing
   in item 1, and then its play volume *v* is set to *B* + 0.25. Otherwise, if it has an emitter, it is
   stopped at once [B8].
3. If the player is not alive and the heartbeat sound has an emitter, it is stopped at once [B8].

While the player is not the controlled character none of this runs: the two sounds keep their state.

Being 2D, both are placed relative to the listener, 1.75 m above it (S2 section 7.3); their min distance is 1,
so the device's distance factor is *g* = 1 / (1 + 0.75 × 0.75) = 0.64 for both [M3]. The limping state turns
on when power or health falls below 0.1 and off when both are above 0.2 [D7], so the breathing starts and stops
with it. The heartbeat follows the wounds: it starts once the mean wound size exceeds 0.6 and stops at once
when the healing of wounds (`hits` sections 5.3, 5.4) brings it to 0.6 or below. Examples (*B*, heartbeat) [M4]:
one wound of size 0.7: plays at *v* = 0.95; wounds of sizes 0.5 and 0.6: silent; wounds of sizes 1.0 and 0.4:
*v* = 0.95; one wound of size 1.2: *v* = 1.45 (the device gain is limited to 1, S2 section 7.1); one wound of
size exactly 0.6: silent.

## 5. The shock of an explosion

A shock lowers the effects factor *F* (S2 section 5.4) while the tinnitus rings and restores it afterwards.
At most one shock runs at a time. It is started only by section 3 item 5.

### 5.1 Start

With the length *L* (ms) and the power `P` [B12]:

- *p* = clamp(`P`, 0.1, 1.5); the **life** ℓ = 4 · *p* seconds; the **end time** *E* = *T* + ℓ;
- the **saved factor** *F*₀ = *F* (its value at this moment); the **elapsed time** *e* = 0 ms;
- *F* ← 0.1 · *F*₀;
- the effects named by `[snd_shock_effector]` are started for the player: a visual post-process effect from
  the file named by `pp_eff_name`, cyclic if `pp_eff_cyclic` is true; and a camera animation from
  `cam_eff_name`, if that key exists [B16]. With the data of section 1 this is the visual effect
  `snd_shock.ppe`, cyclic, and no camera animation [D3]. The file is looked up in the level's directory first,
  then among the game's animations (`anims\`) [B17].

A shock started while one runs does not happen: a second explosion neither restarts nor extends it (but sets
the tinnitus's play volume to 10 if it is still playing, section 3 item 5).

### 5.2 Each frame

In the player's per-frame update (believed: every frame while the player object is processed), if the player
is the viewed character [B13, B14]:

1. *e* ← *e* + Δ;
2. with *x* = 2 · *e* / *L* − 1: if *x* > 0, *F* ← 0.1 · *F*₀ + (*F*₀ − 0.1 · *F*₀) · *x* (otherwise *F* is
   left as it is);
3. if neither the visual effect nor the camera animation is still running, or the player is not alive, the
   shock ends (section 5.4).

If the player is not the viewed character in a frame, the shock ends at once [B14].

### 5.3 The visual effect

The visual effect is **valid** while *e* ≤ *L* [B16]. At each update of the player's camera effects (believed:
each frame) the visual effect, if not valid, is removed; otherwise it takes its step, and it is removed when
the step ends it [B18]. The step [B16, B17]:

- its intensity is first set from the shock: *f* = clamp((*E* − *T*) / 8, 0, 1) (computed as
  ((*E* − *T*) / ℓ) · ℓ · 0.125);
- then *I* = clamp(*f* + (1 − *f*) · *dt*, 0.0001, 1);
- the step ends the effect when |*I* − 0.0001| < 10⁻⁷.

The animation of the file repeats while the effect runs (cyclic). How *I* and the file's values are drawn, and
the `.ppe` format, are not part of this specification (**unestablished** here; a visual specification).
In which order the camera-effect update and the per-frame update of section 5.2 run within a frame is
**unestablished**; either way the shock ends within about one frame after *e* exceeds *L*.

### 5.4 End

When a shock ends, *F* ← *F*₀ (the saved factor), and the visual effect and camera animation are removed if
they are still running [B15]. Besides section 5.2, a shock ends when the player dies (section 6) and when the
player object goes offline [B9, B11]; when the player comes online no shock is running [B10].

### 5.5 Consequences

With the tinnitus (*L* = 6,000 ms) and *F*₀ = 1, at frames of 20 ms [M1]: *F* = 0.1 until *e* = 3,000 ms, 0.106
at 3,020 ms, 0.55 at 4,500 ms and 1.0 at 6,000 ms; at the first frame past *L* (*e* = 6,020 ms) *F* = 1.006,
and the shock then ends (visual effect no longer valid) and *F* returns to 1. With *F*₀ = 0.8 it runs from
0.08 to 0.8. The life does not affect *F*; it sets only the visual intensity: ℓ = 0.4, 2, 4 and 6 s for `P` =
0.05, 0.5, 1 and 2, with *f* = 0.05, 0.25, 0.5 and 0.75 at the start [M2]. After the life *f* is 0 and
*I* = *dt* (0.016 at *dt* = 0.016 s), which does not end the effect; only a frame with *dt* = 0 (a paused frame)
would bring *I* to its floor and end it early [M2].

The tinnitus itself is an effects sound, so it is quieted by the shock it starts: at its start its level
*b* · *v* · *c* is 0.1 with `snd_volume_eff` = 1 (S2 section 5.4); a second explosion while it rings sets
*v* = 10, which brings it to 1.0 [M5]. The elapsed time follows the global clock (stopped while paused) and the
tinnitus the sound clock (whose emitters the pause also freezes, S2 section 4.5), so the two stay in step
(believed).

## 6. Death

When the player dies (the program's handling of the player's death; death itself is outside this
specification, and that this handling runs once, when health reaches 0, is believed) [B9]:

1. one of the four death sounds is played at the player's position (its feet), not looped, 3D, without delay,
   associated with the player object; the sound is `die<i>` with *i* = `r mod 4`, `r` the next value of the
   shared generator (section 3 item 3);
2. the breathing and the heartbeat are stopped at once;
3. in a single-player game the program starts the UI sequence `game_over` (one sequence of
   `config\ui\game_tutorials.xml` [D8]; S24, not covered here);
4. a running shock ends (section 5.4).

When the player comes online with health 0 or less (a dead player), the breathing is stopped at once [B10].

## 7. Verification facts

From the data [D1 – D8]:

- `[actor]`: `hit_sounds = actor_hit_snds`, `heavy_breath_snd = actor\breath_1`,
  `heavy_blood_snd = affects\heartbeat`, `camera_height_factor = 0.85`.
- `[actor_hit_snds]`: 11 keys, exactly the hit type names, naming 13 distinct files, all present; the lists of
  section 1.
- `[snd_shock_effector]`: 2 keys, `pp_eff_name = snd_shock.ppe`, `pp_eff_cyclic = 1`; `anims\snd_shock.ppe`
  exists.
- The 19 files of section 1 (13 hit sounds, breathing, heartbeat, four death sounds) have the parameters and
  durations of the table of section 1; `affects\tinnitus3a` has 264,600 samples (6,000 ms), `actor\breath_1`
  97,152 (2,202 ms), `affects\heartbeat` 148,892 (3,376 ms).
- `[actor_condition]` limping thresholds: `limping_power_begin` 0.1, `limping_power_end` 0.2,
  `limping_health_begin` 0.1, `limping_health_end` 0.2.
- Of 439 scripts, 2 name `affects\tinnitus3a` (3 occurrences); none names another file, section or key of
  this specification.

Computed from this specification [M1 – M7]: the factor curve of section 5.5; the lives and start intensities
of section 5.5; *g* = 0.64 for the 2D sounds at 1.75 m; the heartbeat cases of section 4; the tinnitus level
0.1, and 1.0 after a second explosion; `r` = 38 from state 0; the standing eye height 1.53 m.

Invariants: every hit-sound list is non-empty [D2]; while a shock runs *F* ≥ 0.1 · *F*₀, and after it ends
*F* = *F*₀ [M1].

## 8. Unestablished

- The interval of the player's scheduled update (section 4) and whether the per-frame and camera-effect
  updates run on every frame (sections 5.2, 5.3: believed), and their order within a frame (section 5.3).
- Whether a missing hit-type key gives an empty list (section 2; not needed for this edition's data).
- That the hit and death sounds are discarded with the player object (section 2: believed).
- That the controlled and the viewed character are the player character in normal play (conventions:
  believed).
- How the visual effect is drawn, and the `.ppe` format (section 5.3).
- What the AI does with the reports of these sounds (S24).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`, and the shipped program files of `bin\` (`xrGame.dll`, `xrSound.dll`,
  `XR_3DA.exe`).
- Data examined: `config\creatures\actor.ltx` (`[actor]`, `[actor_hit_snds]`, `[actor_condition]`),
  `config\misc\postprocess.ltx` (`[snd_shock_effector]`), the 19 sound files of section 1 (Ogg pages, Vorbis
  identification and comment headers), the archive listing (`anims\snd_shock.ppe`, the level directories) and
  the 439 shipped scripts.
- Analysis: static analysis (decompilation and disassembly with Ghidra, headless) of the program's reading of
  the player's configuration, the player's hit handling, scheduled and per-frame updates, coming online, going
  offline and death, the limping test and the total bleeding, the shock and its visual and camera effects,
  the camera-effect update, the engine's object set-up and frame clocks, and the sound library's create, play,
  stop and volume operations and sound durations; reading constants from the program files. No program was
  executed. The worked examples were recomputed by a throwaway model written from this text.
- No leaked source code, derived projects or third-party documentation of the engine were used.
