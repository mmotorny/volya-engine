# Hits without AI: falls, anomalies and radiation zones

Functional specification of when falls, the ground the player stands on, anomaly zones and radiation zones
hit the player character of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) in a single-player game,
and with what: the hit's type, power, bone, direction, position and impulse, when it is made and how it
repeats, and the data each comes from. What a hit does once it arrives (immunities, outfit, conditions,
wounds) is `docs/behavior/player-hits.md` (cited as `hits`) and is not repeated. It is specification PS-2 of
`docs/behavior/narrowed-specs-scope.md` (section 4.3).

It builds on:

- `hits`: the hit (section 2: type `t`, power `P`, bone `b`, armour piercing `ap`, source) and its
  application (sections 3 to 7);
- `docs/behavior/player-movement.md` (cited as `movement`): the movement update and `dt_m` (section 3.2),
  the falling and landing states (section 6.7), the hand-over of the acceleration with its pending vector
  (section 6.1), gravity (section 6.6);
- `docs/formats/level-cform.md` (cited as `cform`): the impact speed and the deadly ground materials
  (sections 3.2, 4.4);
- `docs/behavior/zone-idle-effects.md` (cited as `idle`): the zone records, the zone's five states and the
  event by which a state changes (sections 2, 7), creation (3.1), the zone update and the scheduled update
  with fast mode and the shape sphere (6.1, 6.2), the switching rule that disables a zone (7);
- `docs/behavior/offline-simulation.md`: which objects are online (section 6.5);
  `docs/behavior/object-visual-and-transform.md`: the object transform `X` (section 4);
- `docs/formats/sav.md` (cited as `sav`): the Zone block with its maximum power (section 7.4.1) and the
  layouts per class id (7.4.2);
- `docs/formats/ltx-config.md`: sections, inheritance; a missing key read without a default is fatal.

Conventions:

- "Configuration" is the `system.ltx` tree (winning copies); keys are read from the zone record's section.
- **The player** is the player character; its **position** is its feet (`movement`).
- `clamp(x, a, b)` limits `x` to `[a, b]`; `|v|` is a vector's length. Times are in milliseconds (ms) unless
  stated.
- Statements are **static** (read from the program and the data; the game was not run). Inferred statements
  are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are the checks reproducing the statement: `B…` in `bin.py` (the program), `D…` in `data.py`
  (configuration, `spawns\all.spawn`, models, scripts), `M…` in `model.py` (numbers computed from this
  text). All are in the dirty workspace's `checks/environment-hits/`; `run.py` runs them.

## 1. The zone records

### 1.1 Class ids and kinds

`spawns\all.spawn` has 1,443 zone records (class ids beginning `Z`) [D1]. Each class id gives one of five
**kinds** of behaviour; the scripted ids use the kind of an engine id (`idle` section 1.1) [D7, B19]:

| Kind | Class ids (records) | Hits the player | Section |
|---|---|---|---|
| bald | `Z_MBALD` (402), `ZS_MBALD` (442), `Z_TORRID` (1) | at the explosion moment of each blowout | 6.1 |
| burning fuzz | `ZS_BFUZZ` (92) | at the explosion moment of each blowout | 6.2 |
| radiation | `Z_RADIO` (275) | at the explosion moment of each blowout | 6.3 |
| mincer | `ZS_GALAN` (124), `ZS_MINCE` (106) | at the explosion moment, inside the core only | 6.4 |
| no-gravity | `Z_NOGRAV` (1) | never | 6.5 |

`Z_TORRID` differs from the other bald ids only in that its zone moves (`idle` section 5) [B11]. The sections
per class id [D1]: `Z_RADIO`: `zone_radioactive` 40, `zone_radioactive_weak` 77, `zone_radioactive_average`
65, `zone_radioactive_strong` 61, `zone_radioactive_killing` 31, `zone_monolith` 1; `Z_MBALD`:
`zone_flame_small` 265 (the fires of `config\misc\zone_kampfire.ltx`), `zone_mine_field` 102,
`zone_teleport` 24, `zone_campfire_grill` 5, `zone_flame` 3, `zone_emi` 2, `zone_zhar` 1; `ZS_MBALD` 14
sections, `ZS_GALAN` 4, `ZS_MINCE` 5, `ZS_BFUZZ` 2 (`zone_burning_fuzz1` 21, `zone_burning_fuzz_strong` 71),
`Z_TORRID` `fireball_zone`, `Z_NOGRAV` `zone_no_gravity`. No zone record has an owner [D1].

### 1.2 Keys

Read when the zone object is loaded or comes online, by every kind (the mincer and burning fuzz keys by
those kinds), all without a default (missing: fatal) unless stated [B2, B3, B4, B7]:

| Key | Used by | Meaning (section) |
|---|---|---|
| `hit_type` | bald, burning fuzz, mincer | the hit's type `t` (5): a name of `hits` section 2, compared ignoring case; `burn` 0, `shock` 1, `strike` 2, `wound` 3, `radiation` 4, `telepatic` 5, `chemical_burn` 6, `explosion` 7, `fire_wound` 8, `wound_2` 9; any other name, `physic_strike` included, is fatal ("Unsupported hit type!") |
| `attenuation` | all | `a` of the relative power (4.3) |
| `effective_radius` | bald, burning fuzz, radiation | `e` of the relative power (4.3) |
| `hit_impulse_scale` | bald, burning fuzz, mincer | `s` of the impulse (6.1, 6.4) |
| `period` | radiation | `u32`, ms: the period (6.3) |
| `awaking_time`, `blowout_time`, `accamulate_time` | all | `s32`, ms: the state times (5) |
| `blowout_explosion_time` | all | `u32`, ms: the explosion moment (5); 0 when absent; a value above `blowout_time` is replaced by `blowout_time` with an error message |
| `ignore_nonalive`, `ignore_small`, `ignore_artefacts`, `disable_time`, `disable_time_small` | all | which objects count (4.2); none applies to the player |
| `throw_in_atten`, `throw_in_impulse_alive`, `blowout_radius_percent`, `actor_blowout_radius_percent` | mincer | 6.4 |
| `min_speed_to_react` | burning fuzz | 5.1 |

The **maximum power** `M` of a zone is the `float` of its record's Zone block (`sav` 7.4.1), read when the
zone comes online [B3]. The keys `min_start_power` and `max_start_power` do not set it in a single-player
game: `min_start_power` is only the value a newly made zone object holds until its Zone block is read (so
it is believed to be the maximum power of a zone created during play rather than read from a record), and
`max_start_power` is read only by a server step of the multiplayer game kinds [B3]. In 1,347 of the 1,443
records `M` lies outside `min_start_power … max_start_power` of its section [D3].

### 1.3 Section values

Times in ms; "—" absent (explosion moment 0). `M`: the records' maximum powers with their counts [D2, D3].

| Section | `hit_type` | `M` (records) | `e` | `a` | `s` | awaking, blowout, accumulation | explosion |
|---|---|---|---|---|---|---|---|
| `zone_flame_small` | burn | 15 (265) | 1.3 | 1 | 0 | 0, 1000, 0 | — |
| `zone_flame` | burn | 15 (3) | 1.3 | 1 | 0 | 1000, 300, 1000 | — |
| `zone_campfire_grill` | burn | 0.3 (3), 15 (2) | 1.0 | 1 | 0 | 500, 1000, 0 | — |
| `zone_zhar` | burn | 0.3 (1) | 1.3 | 1 | 0 | 10, 50, 10 | — |
| `zone_mine_field` | explosion | 0 (49), 200 (53) | 1 | 1 | 0.15 | 50, 1000, 100000 | 150 |
| `zone_teleport` | strike | 0 (24) | 1 | 1 | 0 | 0, 1000, 500 | 800 |
| `zone_emi` | strike | 1000 (2) | 1 | 1 | 0.01 | 0, 6000, 5000 | 5000 |
| `zone_mosquito_bald` | strike | 0 (7), 20 (22), 60 (1) | 0.85 | 1 | 0.3 | 50, 500, 1000 | 200 |
| `zone_mosquito_bald_weak` | strike | 0 (22), 0.4 (64), 0.49 (1) | 0.65 | 1 | 0.1 | 80, 500, 1000 | 200 |
| `zone_mosquito_bald_weak_noart` | strike | 0.25 (41) | 0.65 | 1 | 0.1 | 80, 500, 1000 | 200 |
| `zone_mosquito_bald_average` | strike | 0 (33), 1.19 (6) | 0.75 | 1 | 0.2 | 50, 500, 1000 | 200 |
| `zone_mosquito_bald_strong` | strike | 1.6 (5), 1.8 (11), 1.99 (8), 160 (1) | 0.8 | 1 | 0.2 | 30, 500, 1000 | 200 |
| `zone_mosquito_bald_strong_noart` | strike | 1.99 (4) | 0.8 | 1 | 0.2 | 30, 500, 1000 | 200 |
| `zone_witches_galantine` | shock | 0 (9), 10 (2), 15 (2), 25 (1), 30 (21), 90 (19), 200 (8), 500 (58) | 0.75 | 1 | 0.03 | 100, 1000, 1000 | — |
| `zone_witches_galantine_average` | shock | 1.19 (9) | 0.75 | 1 | 0.03 | 100, 1000, 1000 | — |
| `zone_witches_galantine_strong` | shock | 1.8 (6) | 0.75 | 1 | 0.03 | 100, 1000, 1000 | — |
| `zone_buzz` | chemical_burn | 0.99 (1), 90 (22) | 1.0 | 1 | 0 | 50, 100, 500 | — |
| `zone_buzz_average` | chemical_burn | 0.49 (1) | 1.0 | 1 | 0 | 50, 100, 500 | — |
| `zone_zharka_static_weak` | burn | 0.3 (4), 0.49 (1) | 1.3 | 1 | 0 | 100, 8000, 1000 | — |
| `zone_zharka_static_average` | burn | 0.9 (25), 1.19 (13), 90 (7) | 1.3 | 1 | 0 | 100, 8000, 1000 | — |
| `zone_zharka_static_strong` | burn | 1.99 (7) | 1.3 | 1 | 0 | 100, 8000, 1000 | — |
| `fireball_zone` (`Z_TORRID`) | burn | 0.9 (1) | 2.75 | 1 | 0 | 100, 300, 100 | — |
| `zone_burning_fuzz1` | chemical_burn | 25 (7), 30 (4), 300 (10) | 0.75 | 1 | 0 | 50, 1000, 5000 | — |
| `zone_burning_fuzz_strong` | chemical_burn | 90 (71) | 0.75 | 1 | 0 | 50, 1000, 5000 | — |
| `zone_radioactive` | radiation | 0 (40) | 1.00 | 0.1 | 0.01 | 100, 100, 500 | — |
| `zone_radioactive_weak` | radiation | 0 (1), 0.005 (13), 0.006 (50), 0.01 (3), 0.6 (10) | 1.00 | 0.1 | 0.01 | 100, 100, 100 | — |
| `zone_radioactive_average` | radiation | 0 (5), 0.009 (15), 0.01 (41), 0.03 (4) | 1.00 | 0.1 | 0.01 | 100, 100, 100 | — |
| `zone_radioactive_strong` | radiation | 0.049 (11), 0.05 (45), 25 (2), 30 (2), 50 (1) | 1.00 | 0.1 | 0.01 | 100, 100, 100 | — |
| `zone_radioactive_killing` | radiation | 0.05 (31) | 1.00 | 0.1 | 0.01 | 100, 100, 100 | — |
| `zone_monolith` (`Z_RADIO`) | telepatic | 0.9 (1) | 1.00 | 0.1 | 0.01 | 1, 9000, 1 | — |
| `zone_gravi_zone_weak` | strike | 0 (39), 0.25 (3), 0.49 (2) | 0.7 | 1 | 0.1 | 200, 2500, 1000 | 2100 |
| `zone_gravi_zone` | strike | 15 (4), 25 (29), 50 (18), 155 (2) | 0.75 | 1 | 0.2 | 300, 2500, 1000 | 2100 |
| `zone_gravi_zone_average` | strike | 1.19 (7) | 0.8 | 1 | 0.1 | 100, 2500, 1000 | 2100 |
| `zone_gravi_zone_strong` | strike | 1.9 (15), 1.99 (5) | 0.9 | 1 | 0.3 | 50, 2500, 1000 | 2100 |
| `zone_mincer_weak` | strike | 0 (3) | 0.75 | 1 | 0.1 | 50, 10000, 4000 | 5999 |
| `zone_mincer_weak_noart` | strike | 0.4 (51), 0.49 (4) | 0.75 | 1 | 0.1 | 50, 10000, 4000 | 5999 |
| `zone_mincer` | strike | 0 (8), 50 (16) | 0.95 | 1 | 0.1 | 50, 10000, 4000 | 5999 |
| `zone_mincer_average` | strike | 0 (5), 1.19 (2) | 0.8 | 1 | 0.1 | 50, 10000, 4000 | 5999 |
| `zone_mincer_strong` | strike | 2.4 (15), 2.49 (1), 90 (1) | 1 | 1 | 1 | 25, 10000, 1000 | 5999 |
| `zone_no_gravity` (`Z_NOGRAV`) | shock | 0.9 (1) | 0.75 | 1 | 0.5 | 500, 1000, 5000 | — |

`period` is 1 in every zone section [D2]. 245 records have `M` = 0; a zone with `M` = 0 never hits the player
(its power is 0, below every threshold of section 6) [D3]. Mincer-kind values: `throw_in_atten`,
`blowout_radius_percent`, `actor_blowout_radius_percent`, `throw_in_impulse_alive`: `zone_gravi_zone` 2,
0.8, 0.5, 400; `zone_mincer` 1.5, 0.3, 0.3, 1000; `zone_mincer_strong` 1.2, 0.6, 0.6, 600 [D2] (the other
sections in `data.py`). Burning fuzz `min_speed_to_react`: 7.01 (`zone_burning_fuzz1`), 1.50
(`zone_burning_fuzz_strong`) [D2].

## 2. How a hit reaches the player

Every hit of this specification is sent as a **hit event** to the player's object and applied when the event
arrives [B1]:

- A zone builds the event only while the game runs a local server with at least one client, which a
  single-player game always does (`hits` section 6). The sender sends it to the game's server, which is believed to
  pass it back unchanged; on its arrival the player first records the hit (hitter, weapon, bone, health;
  used by other features) and then applies it as `hits` section 4 states [B1, B18]. How many frames the
  round trip takes is **unestablished** (believed: one or two, as for the zones' state events, `idle`
  section 7).
- The event carries: the **type** `t`; the **power** `P`; the **bone** (a bone number of the player's model,
  or none); the **direction**, a vector (sent compressed); the **position**, a point; the **impulse**, a
  number; the **armour piercing** `ap`, always 0 here; the **hitter** and the **weapon**, object ids [B1].
- `hits` uses `t`, `P`, `b` and `ap`. The direction, position and impulse act through the physics
  (the impulse of the hit, `hits` section 7), which is outside this specification (section 8).

## 3. Falls and deadly ground

### 3.1 The damage fraction

In every movement update of the player (`movement` section 3.2) its movement computes a **damage
fraction** `f`, starting from 0 [B18]:

1. with `v` the step's contact speed (the impact speed of `cform` section 4.4; that it is this value is
   believed there), if `v > ph_crash_speed_min`: `f = (v − ph_crash_speed_min) / (ph_crash_speed_max −
   ph_crash_speed_min)` (`[actor]`: 14 and 30 [D5]), with no upper limit;
2. if the last ground material touched (`cform` section 4.4) is **deadly** (`cform` 3.2: `materials\death`,
   damage rate 30, and `materials\earth_death`, 150): `f ← f + dt_f × damage rate`, `dt_f` the frame time
   in seconds (`movement` section 3.2).

The contact speed is read once per update and cleared [B18]. A further handler of the player's physics body
may change `v` and `f` before they are kept; whether the player has one is **unestablished** (believed: no).
The same `v` decides the landing flags of `movement` section 6.7.

### 3.2 The fall hit

Then, in the same movement update, if the player is local, alive (health > 0), `|f| ≥ 1e−7`, and the player
is the object the level is controlled through (believed: always outside scripted control and vehicles), a
hit event is sent to the player itself with [B18]:

| Field | Value |
|---|---|
| power | `f` |
| type | 4 (`radiation`) when the last ground material touched is deadly; otherwise 2 (`strike`) |
| bone | the bone of the player's model that a ray from the contact point along the hit direction meets; if it meets none, the bone whose position is nearest the contact point (the ray's length is **unestablished**) |
| direction, position | the direction and point of the contact as the player's physics body records them (physics; **unestablished**) |
| impulse | 0 |
| armour piercing | 0 |
| hitter, weapon | the object the contact records as causing the damage; the player itself when none is recorded |

So a landing makes one hit in the update of the impact, and standing on deadly ground makes one hit in every
movement update while it is the last ground touched. Examples [M4]: impact speeds 15, 20, 30, 46 m/s give
`f` = 0.0625, 0.375, 1, 2; without the drag of `movement` section 6.3, a fall from rest reaches 14 m/s after
4.995 m and 30 m/s after 22.94 m (gravity 19.62 m/s²); at a frame of 0.02 s, `materials\earth_death` gives
3.0 per frame and `materials\death` 0.6. A strike hit also sets the slowdown of `hits` section 4.1.

## 4. The objects in a zone

### 4.1 Which objects a zone holds

At each scheduled update of a zone that is not disabled (`idle` section 6.2) the zone refreshes its list of
objects: candidates are the objects found near its shape sphere (`idle` 6.2; believed: those whose bounds
meet it), and an object is in the list while all of these hold [B5, B6]:

- it is not itself a zone or space restrictor, not a breakable object, and has a model;
- it is not the zone;
- its zone shapes meet the object's bounds;
- the object's own answer to the contact is yes.

The **player** answers yes when its position, as a sphere of radius 0.001 m, is inside one of the zone's
shapes (world space, by the transform `X`): a sphere shape when the distance of the centres is less than its
radius + 0.001; a box shape when the position is within 0.001 m of the inside of all six faces (the box of
`sav` 7.4.1, faces at ± ½ of each axis) [B5]. So the player is in a zone's list exactly while its feet are
inside a zone shape (the bounds test is believed implied by it).

**Radiation zones** hold only the player: their contact test accepts only an object answering as the player
character, then applies the shapes test and the player's answer above [B5].

An object enters the list at the scheduled update at which it first passes and leaves it at the first that
it fails [B5, B6] (believed: the general contact bookkeeping of objects). The scheduled updates of a zone
near the camera come every 30 ms and more (`idle` section 6.2: 30 ms at 0 m, 78 ms at 10 m).

### 4.2 Whether an object counts

When an object enters, its record notes [B6]: **small** when its radius is below 0.6 m; **non-alive** when
it is not a living thing or its health is 0 or less; it is **excluded** when it is small and
`ignore_small` is on, non-alive and `ignore_nonalive` on, or an artefact and `ignore_artefacts` on. At each
scheduled update its time in the zone grows by the elapsed time, and when that passes `disable_time`
(`disable_time_small` for a small object; −1: never) and the object is non-alive at that moment, it becomes
excluded. **Something counts** when at least one record of the list is not excluded [B6].

The player is never excluded: its radius is the bounding-sphere radius of its model (1.3099 … 1.3768 m for
the 16 models it can wear, `stalker_novice` 1.3399 m [D4]) plus, when the item in its active slot has one, a
radius of that item (believed: the item's model radius) [B17], so it is never small; it is a living thing,
and excluded by time only when not alive. So while the player is in a zone's list (4.1), something counts.

### 4.3 Relative power

The hits of zones scale the maximum power `M` by a **relative power** of a distance `d` [B4]:

    k(d) = clamp(1 − a · (d / R)², 0, 1)  for d ≤ R;   k(d) = 0  for d > R

with `a` the section's `attenuation` and the **reach** `R` = `r` × `effective_radius` (bald, burning fuzz,
radiation kinds) or `r` × `throw_in_atten` (mincer kind), `r` the radius of the zone's shape sphere
(`idle` section 6.2) [B4]. With `a` = 1, `k` falls from 1 at the centre to 0 at `R`; with `a` = 0.1
(radiation sections) from 1 to 0.9 at `R`, then 0 [M1].

## 5. The zone's cycle

The states, their change by event and the zone update are those of `idle` sections 2, 6 and 7; this section
adds when they change and when the zone affects the objects. Each zone update first keeps the previous
**timer** `t0`, adds the elapsed time to the timer `t` (both reset to 0 when a state change arrives), then,
unless the zone is disabled, runs its state's step [B8]:

1. **Idle.** At each scheduled update (4.1) of an idle zone the **wake test** runs: for every kind but the
   burning fuzz, if something counts (4.2) a change to awaking is requested [B6, B7]. The burning fuzz
   requests it when a living thing in its list moves faster than `min_speed_to_react` (the length of its
   movement velocity), whether or not it counts [B7].
2. **Awaking.** When `t ≥ awaking_time`: change to blowout [B8].
3. **Blowout.** The **explosion moment** is the update with `t0 ≤ blowout_explosion_time < t` (so with 0,
   the blowout's first update with elapsed time > 0); at it the zone **affects the objects** (5.1) [B9].
   The kinds differ [B9]:
   - bald: at each update before `blowout_time` the timed events (the explosion moment among them) are
     checked; at the first update with `t ≥ blowout_time` the change to accumulation is requested and the
     timed events are checked once more for that update;
   - burning fuzz and radiation: the timed events are checked at each update before `blowout_time`; at
     `t ≥ blowout_time` the change to accumulation is requested;
   - mincer: at every update the timed events are checked and the zone affects the objects (6.4, with the
     explosion hit only at the explosion moment); at `t ≥ blowout_time` the change to accumulation is
     requested.
4. **Accumulation.** When `t ≥ accamulate_time`: change to idle if nothing counts, otherwise straight back
   to blowout (no awaking) [B8].

A requested change takes effect when its event arrives (`idle` section 7); until then the state and its
timer go on. A disabled zone (`idle` section 7) neither refreshes its list nor runs a step.

So while the player stays in a zone that has woken, the zone hits it once per blowout, the blowouts
following each other every `blowout_time` + `accamulate_time` plus the event round trips. Examples [M7],
with 10 ms frames and every change taking effect at the next update: `zone_mosquito_bald` hits 260, 1760,
3260 ms after it starts awaking, then every 1500 ms; `zone_flame_small` 20, 1030, 2040 ms, every 1010 ms.

### 5.1 Affecting the objects

When a zone affects the objects (at most once per frame; not while one device counter is non-zero, whose
meaning is **unestablished**, believed: the frames of the level's start), it applies its kind's **effect**
(section 6) to every object of its list not marked as being destroyed, then sets its **period timer** to 0
[B10]. After each step of a non-disabled zone the period timer grows by the update's elapsed time while
something counts (to at most `period`) and is 0 otherwise [B15].

## 6. Hits by kind

### 6.1 Bald kind (`Z_MBALD`, `ZS_MBALD`, `Z_TORRID`)

The effect on an object that is not excluded [B11]:

1. **Direction**: three values `r1`, `r2`, `r3` (0 … 32767) are drawn in this order from the game's
   shared 15-bit random generator (the one of `hits` section 4.6) and `(r3/32767 − 0.5, r2/32767, r1/32767 − 0.5)` is
   normalised: a direction with a non-negative vertical part [M8].
2. **Distance**: `d = max(|p − c| − ρ, 0)`, `p` the object's position, `c` the centre of the zone's shape
   sphere placed by `X`, `ρ` the object's radius (the player's: section 4.2).
3. **Power** `P = k(d) · M` (section 4.3).
4. If `P > 0.01`, a hit event with: type `hit_type`; power `P`; bone 0 (`bip01_pelvis` in all 16 of the
   player's models [D4]: `h` 1.0, `w` 1.5 in `[actor_damage]` [D5]); the direction of item 1; position
   (0, 0, 0); impulse `m · s · P`, `m` the object's mass (the player's while alive: `[actor]` `ph_mass` = 80
   [B17, D5]) and `s` = `hit_impulse_scale`; hitter and weapon the zone. The period timer is then 0.

Examples [M2, M6]: a first-level fire (`zone_flame_small`, shape radius 0.642 m, `R` = 0.8346 m, `M` = 15):
`P` = 15 while the player (`stalker_novice`, empty hands) is within 1.3399 m of the fire's centre, 11.25 at
1.3399 + `R`/2, 0 beyond 1.3399 + `R`; with the New Game outfit this burn hit takes 13.5 health at
`gd_master` and 6.75 at `gd_novice`. Impulses: `zone_mosquito_bald` with `M` 20 gives 480, `zone_mine_field`
with 200 gives 2400.

### 6.2 Burning fuzz (`ZS_BFUZZ`)

As section 6.1 except that the distance is `d = sqrt((p.x − c.x)² + p.y² + (p.z − c.z)²)`: the vertical
part is the position's own height, not its height above `c`, and no radius is subtracted [B14]. So the fuzz
can hit only where the player is within `R` of height 0 [D6]: for 79 of the 92 records no point of the shape
sphere lies that close, so they never hit; 13 can (`l04_darkvalley` 5, `l11_pripyat` 8).
**Disputed in review:** the check [D6] tests the shape sphere, which only bounds the zone's shapes; that
a point inside the shapes themselves lies within `R` of height 0 is not shown for these 13 records, so 13
is an upper bound (the 79 that never hit are established).

### 6.3 Radiation kind (`Z_RADIO`)

The effect on the player (the only object a radiation zone holds, 4.1), in a single-player game, when the
period timer is at least `period` [B12]:

1. `d = |p − c|` (no radius subtracted), `P = k(d) · M`;
2. if `P > 1e−5`, a hit event with: type 4 (`radiation`) whatever `hit_type` says (also for
   `zone_monolith`, whose `hit_type` is `telepatic`); power `P`; no bone; direction and position (0, 0, 0);
   impulse 0; hitter and weapon the zone.

The radiation kind makes its other hits (on entering, at scheduled updates) only in the other game kinds
[B12]. With `period` = 1 ms the period condition holds at a blowout whenever, since the previous one, the
zone has had an update of at least 1 ms with something counting (5.1) (believed: always while the player
stays). So in a radiation zone the player is hit once per blowout and gains `P2` radiation (`hits`
section 4.5). The hit's power `P`: `zone_radioactive_killing` (`M` 0.05) 0.05 at the centre and 0.045 at
`d` = `R`; `zone_radioactive_weak` (0.006) 0.00585 at `R`/2 [M3]; blowouts follow every 200 ms
(600 ms in `zone_radioactive`) plus the round trips [M3].

### 6.4 Mincer kind (`ZS_GALAN`, `ZS_MINCE`)

The effect on an object (excluded or not), at every blowout update (5) [B13]:

1. `c` is the zone's position (row 3 of `X`; its horizontal part as it was when the zone came online); `v = c − p`, `d = |v|`; the direction is `v/d` (towards
   the centre), (0, 1, 0) when `d < 1e−7`; `ρ0` is the shape-sphere radius `r`;
2. the **core fraction** is `actor_blowout_radius_percent` for the player (class id `O_ACTOR`) and
   `blowout_radius_percent` for other objects;
3. if `d / r` is above the core fraction and the object is local (believed: always in single-player): the
   object is **pulled** with distance `d`;
4. otherwise, at the explosion moment: the **explosion hit** with distance `d`: `P = k(d) · M` (`R` = `r` ×
   `throw_in_atten`); if `P > 0.01`, a hit event with type `hit_type`, power `P`, bone 0, the direction of
   item 1, position (0, 0, 0), impulse `m · s · P` (`m` as in 6.1), hitter and weapon the zone; then the
   period timer is 0;
5. otherwise: the object is pulled with distance core fraction × `r`.

**Pulled** with distance `x`: a living thing with health > 0 that is not the player is also hit (type
`hit_type`, power `k(x) · M`, bone 0, impulse 0, at every such update); then, for any living thing with
health > 0, the player included, `throw_in_impulse_alive × k(x)^5 × direction` is added to its movement's
pending vector (`movement` section 6.1, "hand-over of the acceleration"); an object without health but with
a physics body is pushed through its physics instead [B13]. The player is therefore never hit by the pull,
only by the explosion hit, and only when within the core (`d ≤ actor_blowout_radius_percent × r`) at the
explosion moment. Examples [M5]: `zone_mincer` (`M` 50, `throw_in_atten` 1.5, player core 0.3) gives 50 at
the centre and 48 at the core's edge; `zone_mincer_strong` (2.4, 1.2, 0.6) 1.8 at the edge; the pull of
`zone_gravi_zone` at `d` = `r` adds 400 × 0.75⁵ = 94.92.

On the change to blowout the mincer kind also starts a telekinesis on every listed object, and in the idle
state it moves listed objects with physics bodies; both act through the physics (section 8) and make no hit
[B13].

### 6.5 No-gravity kind (`Z_NOGRAV`)

Its zone update only switches the gravity of the objects in its list (and of objects entering and leaving);
it has no states and makes no hit [B16].

## 7. Scripts

The scripted ids `ZS_MBALD`, `ZS_GALAN`, `ZS_MINCE`, `ZS_BFUZZ` have script classes in
`scripts\se_zones.script` that add only timed artefact spawning (`artefact_spawn_idle`,
`artefact_spawn_rnd`) and the saved time of the last spawn; they make no hit [D7]. No shipped script reads a
zone hit key (`hit_type` occurs only as a parameter name in three helicopter scripts) [D7]. Hits that
scripts make through space restrictors (for example in `scripts\sr_aes_deadzone.script`) belong to
PS-17, not here.

## 8. Dependencies outside this specification

- **Physics**: the impact speed `v` and the contact's direction, point and cause (section 3), the effect of
  a hit's impulse and direction on the player, the pending vector of the mincer pull, telekinesis, the push
  of objects without health, the gravity switch of the no-gravity zone; and the positions that decide the
  distances (the player's movement).
- **AI**: none. No hit of this specification needs AI.
- **Visual and sound effects** of zones (blowout particles, lights, sounds, wind, the hit particles on
  objects) and artefact spawning are other work (#116).

## 9. Verification facts

From the data [D1–D8]:

- 1,443 zone records; per class id: `ZS_MBALD` 442, `Z_MBALD` 402, `Z_RADIO` 275, `ZS_GALAN` 124,
  `ZS_MINCE` 106, `ZS_BFUZZ` 92, `Z_TORRID` 1, `Z_NOGRAV` 1; all record versions 118; no owner.
- 245 records have maximum power 0; 1,347 lie outside their section's `min_start_power … max_start_power`.
- First level (`l01_escape`), 178 zone records: `zone_mosquito_bald_weak` 0 (20), 0.4 (64), 0.49 (1);
  `zone_mosquito_bald_weak_noart` 0.25 (2); `zone_witches_galantine` 0 (9); `zone_mincer_weak_noart` 0.4
  (4); `zone_flame_small` 15 (11, each one sphere of radius 0.642 m); `zone_mine_field` 0 (49);
  `zone_radioactive_average` 0.009 (1); `zone_radioactive_killing` 0.05 (17). So on the first level the mine
  fields and the galantines never hit.
- The player's 16 models: bone 0 `bip01_pelvis`; bounding-sphere radius 1.3099 … 1.3768 m.
- `[actor]` `ph_mass` 80, `ph_crash_speed_min` 14, `ph_crash_speed_max` 30.

Computed from this text [M1–M8]: the relative power, fire, radiation, fall, mincer, impulse, timing and
direction examples of sections 3 to 6.

## 10. Unestablished

- The frames an event's round trip takes, and that the server passes a hit back unchanged (believed).
- Whether a handler of the player's physics body changes the contact speed and damage fraction (believed
  not); the contact's direction, point and cause, and the ray length of the fall hit's bone (physics).
- The meaning of the device counter that stops zones affecting objects (believed: level start frames).
- The radius of the item in the active slot added to the player's radius (believed: the item's model
  radius), and for which items.
- That the zone's bounds test is implied by the player's point test, and the candidate search of the list
  (believed).
- That the player is always local and the controlled object outside scripted control (believed).
- The effect on the player of the mincer kind's telekinesis (believed: none while alive).
- The maximum power of a zone created during play (believed: `min_start_power`).

## 11. Provenance

- Date: 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`.
- Data examined: every zone record of `spawns\all.spawn` (decoded per `sav`) with its level; the zone
  sections of `config\misc\zone_*.ltx` through `system.ltx` with inheritance; `config\creatures\actor.ltx`;
  the player's models (`actors\hero\*.ogf`, `actors\dolg\stalker_do_nauchniy.ogf`: header and bones); the
  shipped scripts.
- Analysis: static analysis of the binaries (Ghidra 12.2 headless decompilation and instruction listings)
  of the zones' loading and spawning, contact test and object records, scheduled and per-frame updates,
  state steps and timed events, the effect of each kind on an object and the hits they send; the hit event
  (building, sending, reading, handing to the player); the server zone's reading of its Zone block and the
  readers of the start-power keys; the player's radius, mass, contact answer and movement update, the
  damage fraction, the fall hit and its contact record; configuration values were read from a fresh
  extraction with an independent LTX reader, and the numbers of sections 3 to 6 were recomputed by a
  throwaway implementation of this text. The program was not executed.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The script interface of game objects offers, for a zone, reading and setting its maximum power and enabling and disabling it
  (`get_anomaly_power`, `set_anomaly_power`, `enable_anomaly`, `disable_anomaly`, listed in
  `scripts\lua_help.script`); what each does to the hits of this specification is not stated (no other
  shipped script calls them).
- Section 1.2 lists only the keys the hits use: the mincer kind also reads, without a default (missing:
  fatal), `throw_in_impulse`, `tele_height`, `time_to_tele`, `tele_pause`, `tearing_particles`,
  `throw_out_impulse`, `torn_particles` and `body_tearing_sound` (`tele_particles_big` only when present),
  for its physics and visual effects; whether the bald, burning fuzz and radiation kinds read further keys
  without a default is not stated.
