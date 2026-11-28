# Zone idle effects and lights

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of the effects, lights and sounds that zones (anomalies, fires, mine fields) show
while they rest: for every `spawns\all.spawn` record whose section names an idle effect (`idle_particles`),
including the fires in barrels of `config\misc\zone_kampfire.ltx` on the first level (`levels\l01_escape\`),
where the effect is placed, when it starts as the zone comes online, the switching rule of the Zone block's
times and the stop and restart it causes, the idle light (`idle_light`, `idle_light_anim`, `idle_light_height`,
`idle_light_range`, `idle_light_range_delta`) and its on/off, and the on/off state the zones' idle sounds
follow. It is specification P9 of `docs/behavior/particles-and-lights-scope.md`. What could not be established
is marked **unestablished**; what is inferred rather than read from the program or the data is marked
**believed**.

Builds on (not repeated):

- `docs/behavior/particle-lifecycle.md` (P2): an effect instance, its placement by a matrix and a velocity in
  world mode (section 5), play and stop, deferred or immediate (section 7), its update with elapsed
  milliseconds (section 8), the random generator (section 2.1).
- `docs/behavior/particle-groups.md` (P6): the same operations on a group; 12 of the 15 idle effects are groups (section 9.1 below).
- `docs/formats/lanims-xr.md` (P12): finding an entry by name (section 4), the colour of an entry at a time
  (section 6.2), the value with red and blue exchanged (section 5).
- `docs/behavior/dynamic-lights.md` (P16): the light record (its
  default type point, its colour, range, position and active state, the rules by which a stored value
  changes, which lights are drawn and how they light the world).
- `docs/behavior/glows.md` (P15): the sources of glows (section 2); zones are not among them.
- `docs/behavior/offline-simulation.md`: which objects are online at the start (sections 6.5, 9.1);
  `docs/behavior/object-visual-and-transform.md`: the object transform *X* (section 4).
- `docs/formats/sav.md`: the record layouts, in particular the Zone block (section 7.4.1) and the zone kinds
  (section 7.4.2).
- `docs/formats/ltx-config.md`: key lookup with inheritance; reading a missing key is fatal.
- `docs/behavior/sound-positional.md` (S2): creating a sound in the effects category (section 3.1), play at
  position with the flags looped and 2D and a delay (section 3.2), stop at once (section 4.4).
- `docs/behavior/sky.md`: the global clock *T* in seconds.

Conventions: matrices in the row-vector convention of object-visual-and-transform.md (row 3 of *X* is the position **p**).
Times in the Zone block are in seconds, all others in milliseconds (ms) unless stated. *C* is the global clock
of sky.md in whole milliseconds and *T* the same clock in seconds; both are read once per frame from the same
timer, which runs at real time multiplied by the engine's time factor (1 unless changed); with one device
flag set (what sets it is **unestablished**) both advance by exactly 20 ms per frame instead (`bin_engine.py`
E1). *E* is the camera position of the frame (E1). "u32" arithmetic wraps modulo 2³². Every fact carries the name of
its check, a script in `checks/zone-idle-effects/` of the dirty workspace; `run_all.py` runs them all
(`ghidra.py`, run first, prepares the static analysis the `bin_*.py` checks read).

## 1. The records

### 1.1 Which records

The records of `spawns\all.spawn` whose section has the key `idle_particles` (after inheritance): 1,167
records of 33 sections on 17 levels, of 7 class ids (`data.py` D1):

| Class id | Records | Zone kind (sav.md 7.4.2) |
|---|---|---|
| `ZS_MBALD` | 442 | anomaly |
| `Z_MBALD` | 401 | anomaly |
| `ZS_GALAN` | 124 | anomaly |
| `ZS_MINCE` | 106 | anomaly |
| `ZS_BFUZZ` | 92 | visual anomaly |
| `Z_TORRID` | 1 | torrid zone |
| `Z_RADIO` | 1 | anomaly |

Each record's class id is its section's `class` (D1). The scripted ids use the in-game behaviour of an engine
id: `ZS_MBALD` that of `Z_MBALD`, `ZS_GALAN` and `ZS_MINCE` that of `Z_MINCER` (which `Z_GALANT` shares), and
`ZS_BFUZZ` that of `Z_BFUZZ`; the shipped `scripts\class_registrator.script` registers them with the names
under which the program exposes these behaviours to scripts (`bin_zone.py` Z1). Everything this
specification states holds for all seven ids; where one differs, it says so. The only differences concern
`Z_TORRID`, whose zone moves (section 5), and the kinds' own additions of section 6.3 (Z3, Z19). No shipped
script names an idle key (`data.py` D10).

### 1.2 Keys read

When a zone object is made from its section, these keys are read (Z2; ltx-config.md):

| Key | Kind | Read | Meaning |
|---|---|---|---|
| `idle_particles` | string | if present | name of an effect or group of `particles.xr` (section 4.5) |
| `idle_sound` | string | if present | a sound name; the zone creates a sound of the effects category from it (section 4.3) |
| `idle_light` | boolean | always (missing: fatal) | whether the zone has an idle light |
| `idle_light_range` *R* | float | when `idle_light` is on | light range (m) |
| `idle_light_range_delta` Δ | float | when `idle_light` is on | range variation (m) |
| `idle_light_anim` | string | when `idle_light` is on | a colour animation, found in `lanims.xr` by name (lanims-xr.md 4) |
| `idle_light_height` *h* | float | when `idle_light` is on | height of the light above the zone (m) |

When `idle_light` is off, the four `idle_light_*` keys are not read; 6 sections with `idle_light = off` still
carry them, among them `zone_flame_small` (D4). The key `idle_particles_dont_stop` concerns the effects a
zone shows on objects inside it; it does not change the zone's own idle effect (Z2), and belongs with those
effects to the follow-up F1 of the scope.

### 1.3 The Zone block's times

Of the Zone block (sav.md 7.4.1) this specification uses the enabled time *e*, the disabled time *d* and the
start-time shift *s*, each a `u32` in seconds; when the zone object is created it keeps *e*′ = 1000 *e*,
*d*′ = 1000 *d*, *s*′ = 1000 *s* (u32 products) and the creation clock *C*₀ = *C* (Z4). The owner id is
0xFFFFFFFF (none) in all 1,167 records (D6).

## 2. The zone's states

A zone object is always in one of five states: **idle**, **awaking**, **blowout**, **accumulation** and
**disabled**. The idle effect, the idle light and the idle sound are on exactly while the zone is idle:
every change into idle starts them (section 4.1) and every change out of idle stops them (section 4.2)
(`bin_zone.py` Z7).

- Within the zones' own behaviour, a zone enters **disabled** and leaves it only by the switching rule
  (section 7) (`bin_zone.py` Z20); the script interface's `enable_anomaly` and `disable_anomaly`, whose
  effect is not examined here, are called by no shipped script (`data.py` D10).
- An idle zone changes to **awaking** when its contact test (section 6.2) has found an object inside that
  counts; it then passes through blowout and accumulation (after `awaking_time`, `blowout_time`,
  `accamulate_time` ms of its state timer) and returns to idle or blows out again (Z10). Which objects count,
  and what the zone does to them, is the zones' reaction to objects, owned by the follow-up F1 of the scope;
  here it matters only that the idle effects are off from the change to awaking until the zone is idle again.

## 3. Coming online and going

### 3.1 Creation

Zone objects never switch offline (offline-simulation.md 6.5.2), so each zone record of the actor's level is
created once, when it first comes online, and kept until the level ends. On creation, in this order (Z4):

1. the object's matrix *X* is built from the record's position and angles as for every object
   (object-visual-and-transform.md 4) (`bin_engine.py` E4);
2. the times of section 1.3 are kept and the switching rule is armed when *e*′ ≠ 0 and *d*′ ≠ 0 (section 7);
3. if `idle_light` is on, a light record is created for the zone (section 4.4);
4. the idle start (section 4.1) runs;
5. the state is idle and the state timer 0;
6. the zone is given per-frame updates (section 6.1).

So a zone starts its idle effect, light and sound the moment it comes online, whatever its times; a zone
whose time says otherwise is disabled at its first switching test (section 7).

### 3.2 Destruction

When the zone object is destroyed (with the level): the idle stop (section 4.2) runs, its light records are
released, and its effect object is destroyed (`bin_zone.py` Z13).

## 4. Idle start and stop

### 4.1 Idle start

In this order (Z5):

1. **Sound.** If the zone has an idle sound: play at position **p** (row 3 of *X* at this moment), looped,
   not 2D, delay 0, without an owner object (S2 3.2).
2. **Effect.** If the section names an idle effect:
   1. at the first start only, an effect object is made from the name, without automatic removal (section
      4.5);
   2. at every start, its instance is placed by *X* with velocity (0, 0, 0) in world mode (P2 5.1) and
      played (section 4.5).
3. **Light.** If the zone has an idle light (section 4.4): its range is set to *R*, its position to
   **p** + (0, *h*, 0), and it is made active.

The light's colour is not set here; it is set by the zone's first update (section 6.1), which runs in the
same frame or the next (believed: no frame is drawn between a zone's creation and its first update; the
light record's default colour of P16 would show otherwise).

### 4.2 Idle stop

In this order (`bin_zone.py` Z6):

1. **Sound**: if it is playing, stopped at once (S2 4.4).
2. **Effect**: if the effect object exists, its instance is stopped immediately (P2 7: every particle
   removed at once; a group as P6 states), and the effect object is kept for the next start.
3. **Light**: if any, made inactive.

### 4.3 The idle sound

Created when the zone object is loaded, from `idle_sound`, in the effects category (Z2, E7). Besides the start
and stop above nothing touches it: it is never moved, so it stays at the position of the last start, also for
the moving zone (Z16). Its playback, volume and occlusion are S2's and the level sound area's (scope,
"Sounds are not in this area").

### 4.4 The idle light

- **Record.** Created on the zone object's creation when `idle_light` is on; its shadow setting is set on,
  and its type is never set, so it is the light record's default, point (P16) (Z4, Z18). It is released on
  destruction (section 3.2). The zone creates no glow for it (Z15).
- **Active** exactly from an idle start to the next idle stop (sections 4.1, 4.2).
- **While active**, at every update of the zone (section 6.1), in this order (`bin_zone.py` Z11):
  1. the animation's colour at *T* is taken (lanims-xr.md 6.2) as the value with red and blue exchanged,
     and its bytes 0, 1, 2 are each multiplied by the `f32` nearest to 1/255 (0x3B808081): these are the
     red, green and blue of the light (so the light's colour is the entry's red, green, blue / 255); the
     alpha is 1 (E2);
  2. one value *k* (0 … 32767) is drawn from the generator of P2 section 2.1 (Z11) and the range is set to
     *R* + Δ · (*k* · *q* − 1), *q* the `f32` nearest to 2/32767, in `f32` (so *R* − Δ … *R* + Δ);
  3. the colour is set;
  4. the position is set to **p** + (0, *h*, 0) (**p** of the current *X*).
  Whether the stored range actually changes follows P16's rule for range updates.
- The moving zone (section 5) additionally puts the light at **p**, without *h*, after each of its updates.

### 4.5 The effect object

The zone's effect is held by an effect object of the game, which owns one renderer instance (`bin_engine.py`
E5, E6):

- **Made** from the name: the name is looked up among the effects of `particles.xr` first and among its
  groups second; a name that is neither fails an assertion (E6). The instance is then an effect (P2) or a
  group (P6).
- **Placement**: only as the zone places it (sections 4.1, 5); the effect object's own play and updates do
  not place the instance (E5).
- **Play**: the instance is played (P2 7 / P6), and the instance is at once updated with 33 ms (one step,
  P2 8.2) (E5).
- **Updates**: the instance is updated with the milliseconds elapsed on *C* since its previous update
  (nothing when 0), at each of the effect object's scheduled updates and each time it is drawn (E5). Its
  schedule: next due = now + clamp(30 + trunc(495 · *σ*), 30, 525) ms with *σ* = 0.005 · |*E* − *c*| (`f32`
  0x3BA3D70A), *c* the centre of the instance's sphere (P2 8.2, 8.3) (E3, E5); a scheduler pass may also end early on a
  time budget whose size is **unestablished**, postponing due updates (E3). An instance that is not playing
  is updated as P2 8.2 states (nothing but its box).
- **Destroyed** with the zone (section 3.2).

## 5. The moving zone (`Z_TORRID`)

The one `Z_TORRID` record (`fireball_zone` on `l04u_labx18`, group `anomaly2\heat_02`, idle light on, D9)
moves along a motion (sav.md 7.4.2, torrid zone). At each of its updates (section 6.1), after the general
update, its motion advances by the update's milliseconds and replaces *X*; then (`bin_zone.py` Z14):

1. Δ*t* = (*C* − the clock of the previous such step) / 1000 s (u32 difference, as `f32`);
2. the velocity **w** = (**p** − **p**₀) / Δ*t*, where **p**₀ is the zone's position at its creation (it is
   never renewed); **w** = (0, 0, 0) when |Δ*t*| < 10⁻⁷;
3. if the effect object exists, its instance is placed by *X* and **w** in world mode (P2 5);
4. each active light of the zone is put at **p** (no height).

So the effect follows the moving zone. **w** grows with the distance from the creation point rather than
being the zone's speed; it reaches the effect only through the parent motion of its sources (P2 5.2), which
is 0 in every source of the 7 effects that `heat_02`'s members name (D9), and through whatever P6 does with a
group's velocity. When the switching rule enables this zone, its motion restarts from its beginning (Z14).

## 6. Updates of a zone

### 6.1 The zone update

A zone has a **zone update** with an elapsed time *Δ* ms (`bin_zone.py` Z10):

1. the state timer grows by *Δ*;
2. unless the zone is disabled: the light step of section 4.4 (when the light is active), then the state's
   step; the idle state's step is the switching test (section 7).

The zone update runs either every frame or with the scheduled update (Z12):

- **Fast mode** (every frame): in each frame's pass over the game's objects (which does not run while the
  game is paused), the zone update runs with that frame's elapsed milliseconds (E1, E3, E9).
- Otherwise it runs inside each scheduled update (section 6.2) with the scheduler's elapsed time.

A zone starts in fast mode (section 3.1).

### 6.2 The scheduled update

The zone also has scheduled updates (`bin_zone.py` Z12, Z17; `bin_engine.py` E3). Its schedule: next due =
now + clamp(30 + trunc(970 · *σ*), 30, 1000) ms, *σ* = 0.005 · |*E* − **p**| (`f32` 0x3BA3D70A); the elapsed
time it is given is the milliseconds since its previous scheduled update, at most 1000 (1 when 0). (These
are the general limits of game objects, 20 and 1000 ms, which the zone keeps; believed: no class between the
zone and the general object changes them.) Each scheduled update:

1. unless the zone is disabled:
   1. the contact test (section 2) with the zone's shape sphere (below);
   2. if the zone is idle and the contact test counts an object inside, the change to awaking (section 2);
   3. **fast mode** is on while |*E* − **c**| − *r* ≤ 50 m, (**c**, *r*) the shape sphere placed by *X*, or
      when the record's custom data has `always_fast` in a section `[fast_mode]` (no record has it, D6);
      otherwise off;
   4. if fast mode is off, the zone update (section 6.1) runs with the scheduler's elapsed time;
2. in every state, the switching test (section 7).

The **shape sphere**: for a zone with a single sphere shape, that sphere; otherwise the sphere about the
axis-aligned box (in the zone's own space) of all its shapes, a box shape counting with its 8 corners at its
centre ± ½ of each of its three axes: centre the box's centre, radius half its diagonal (`bin_engine.py` E8;
D3: 1,052 records have one sphere, 92 a sphere and a box, 23 one box; all 160 of the first level one
sphere).

### 6.3 Kinds

`ZS_GALAN` and `ZS_MINCE` add work of their own to their idle-state step, after the switching test;
`Z_TORRID` and `Z_RADIO` wrap the zone update in work of their own (section 5 for the torrid zone); apart
from section 5, none of this touches the idle effects, light or sound (Z3, Z19).

## 7. The switching rule

Armed when *e*′ ≠ 0 and *d*′ ≠ 0 (section 1.3; `bin_zone.py` Z4). It is tested at each scheduled update and
at each zone update in the idle state (section 6). A test computes (Z9)

  *t* = (*C* − *C*₀ + *s*′) mod (*d*′ + *e*′)   (u32 throughout)

and then:

- if the zone is **disabled** and *t* < *e*′: the zone is **enabled**;
- if the zone is **idle** and *t* ≥ *e*′: the zone is **disabled**;
- in any other case nothing happens (in particular, a zone in awaking, blowout or accumulation is not
  disabled until it is idle again).

So in every cycle of *e* + *d* seconds the zone is on for the first *e* and off for the last *d*, the cycles
counted from its creation shifted by *s* seconds.

**Enabling** and **disabling** do not change the state at once: the zone sends an object event naming the
new state (idle, or disabled) to the game's server, which sends it back unchanged to every client; on its
arrival the zone changes to that state, starting or stopping the idle effects as section 2 states (Z7, Z8).
Until it arrives, further tests send the event again; a repeated change to the state the zone is already in
changes nothing for its idle effects (Z7). **Disputed in review:** this holds for a repeated change to disabled only. A change to idle that arrives while the zone is already idle runs the idle stop and then the idle start (section 4): the effect's particles are removed at once and the effect is played again, the sound is stopped and played again at the current position, and the light is made inactive and set again to range *R*, its position and active; the state timer is reset to 0. Whether a second enabling event can arrive this way depends on the round trip (unestablished). How many frames the round trip takes is **unestablished**
(believed: one or two in a single-player game). No other request of the zone asks for the disabled state (Z20).

## 8. The first level at the start

- 160 records (`data.py` D8): `zone_mosquito_bald_weak` 85 and `_noart` 2 (`ZS_MBALD`, group
  `anomaly2\bold_idle`), `zone_mine_field` 49 (`Z_MBALD`, effect `static\mine`), `zone_flame_small` 11 (the
  fires, `Z_MBALD`, group `explosions\campfire_05`), `zone_witches_galantine` 9 (`ZS_MBALD`, group
  `anomaly2\electra_idle`), `zone_mincer_weak_noart` 4 (`ZS_MINCE`, group `anomaly2\mincer_idle`). All have one
  sphere shape. All come online at the start (offline-simulation.md 6.5, 9.1) and start their idle effect.
- No idle light: every section of the level has `idle_light = off`; the fires' `koster_00` is not read (D4,
  D8).
- 111 idle sounds play, looped, at the zones' positions: every record but the 49 mine fields
  (`zone_flame_small`: `ambient\fire2`) (D5, D8).
- Only the 9 galantines have the switching rule (D6, D8):

  | Record | *e*, *d*, *s* (s) | At creation | On (s after creation, first 60 s) |
  |---|---|---|---|
  | `esc_zone_witches_galantine` | 9, 15, 0 | on | 0–9, 24–33, 48–57 |
  | `esc_zone_witches_galantine_0000` | 9, 15, 18 | off | 6–15, 30–39, 54–60 |
  | `esc_zone_witches_galantine_0001` | 9, 15, 12 | off | 12–21, 36–45 |
  | `esc_zone_witches_galantine_0002` | 9, 15, 6 | on | 0–3, 18–27, 42–51 |
  | `esc_zone_witches_galantine_0003` | 9, 21, 6 | on | 0–3, 24–33, 54–60 |
  | `esc_zone_witches_galantine_0004` | 9, 21, 12 | off | 18–27, 48–57 |
  | `esc_zone_witches_galantine_0005` | 9, 21, 18 | off | 12–21, 42–51 |
  | `esc_zone_witches_galantine_0006` | 9, 21, 24 | off | 6–15, 36–45 |
  | `esc_zone_witches_galantine_0007` | 9, 21, 0 | on | 0–9, 30–39 |

  (`model.py` M1, M2.) The 5 marked off start their idle effect and sound on creation and stop them when
  their disabling arrives (section 7); all 9 are believed to be created in the same frame, so their windows
  share *C*₀, and the first visible frame (level-loading.md 2.3, the loading image covers the first frames)
  is believed to show the 5 already off.
- Objects inside zones at the start: the contact test never counts the 49 online breakables (`O_BRKBL`)
  (`bin_zone.py` Z21); of the other 304 online objects with a model, one has a bounding sphere (its model's
  header sphere placed by its transform) that meets a zone's sphere: the wooden box `esc_physic_destroyable_object_0038` (`P_DSTRBL`,
  `physics\box\box_wood_01`) overlaps `esc_zone_mosquito_bald_weak_0026` by 0.274 m; the next nearest is
  0.552 m apart (D11). Whether the box counts for that zone's contact test, and so whether this zone wakes at
  the start and interrupts its idle effect, is **unestablished** (zone reactions, F1). No other online object
  meets a zone at the start.

## 9. Verification facts

### 9.1 All levels

- 1,167 records, 33 sections, 17 levels, classes as in section 1.1 (D1). By level: `l10_radar` 168,
  `l01_escape` 160, …
- 15 distinct idle effects, all in `particles.xr`: 12 groups (916 records) and 3 effects (251 records):
  `anomaly2\pux_idle` (92), `static\buble_distort00` (57), `static\mine` (102) (D2). Most common:
  `explosions\campfire_05` 265, `anomaly2\bold_idle` 226.
- Idle lights: 4 sections, 49 records (`ZS_MBALD` 24, `Z_MBALD` 24, `Z_TORRID` 1) (D4):

  | Section | *R* | Δ | *h* | Animation |
  |---|---|---|---|---|
  | `fireball_zone` | 15.0 | 0.2 | 0.75 | `koster_01` |
  | `zone_buzz`, `zone_buzz_average` | 3.0 | 0.25 | 0.70 | `light_green_02` |
  | `zone_teleport` | 15.0 | 0.25 | 1.50 | `teleport_stancia_in_01` |

  Every animation named is an entry of `lanims.xr`; every `idle_light` value is `on` or `off`.
- Idle sounds: 973 of the 1,167 (10 names); with `zone_zhar` (one `Z_MBALD` record with an idle sound and no
  idle effect) 974 zone records have one; the mine fields and burning fuzz have none (D5).
- Switching rule armed in 18 records: 9 galantines on `l01_escape` (*e* 9, *d* 15 or 21), 6
  `zone_witches_galantine_strong` on `l03u_agr_underground` (2, 10; shifts 0, 2, 4, 6, 8, 10), and on
  `l11_pripyat` 2 `zone_mosquito_bald_strong` (17, 3 and 23, 2) and 1 `zone_mincer_strong` (8, 2); all
  others have *e* = *d* = *s* = 0 (D6).
- `idle_particles_dont_stop = true` in 7 sections (fires and heat zones) (D7).

### 9.2 Worked values

- Range of section 4.4: *k* = 0 gives *R* − Δ and *k* = 32767 gives *R* + Δ: `zone_buzz` 2.75 … 3.25,
  `zone_teleport` 14.75 … 15.25, `fireball_zone` 14.8 … 15.2 (M3).
- Colour of section 4.4: `light_green_02` at *T* = 0 (frame 0) gives (0.6, 0.835294, 0.219608), alpha 1;
  `koster_01` at *T* = 1.3 s (frame 26) (0.847059, 0.611765, 0.258824); `teleport_stancia_in_01` at
  *T* = 0.5 s (frame 15) (0.756863, 0.921569, 0.992157) (M4).
- Scheduled updates of a zone at distance *D* from the camera: *D* = 0, 10, 50, 100, 199, 200, 500 m give
  30, 78, 272, 515, 995, 1000, 1000 ms; of an effect object at 0, 50, 100, 200, 400 m: 30, 153, 277, 525,
  525 ms (M5).

## 10. Unestablished

- Whether the wooden box of section 8 wakes `esc_zone_mosquito_bald_weak_0026` at the start (F1).
- The number of frames an enabling or disabling event takes to come back (section 7).
- The size of the scheduler's time budget (section 4.5, 6.2).
- Whether an intermediate class changes the zone's scheduling limits (believed not, section 6.2).
- Whether a frame is drawn between a zone's creation and its first update (section 4.1).
- An empty `idle_particles` value (none occurs).
- An `idle_light_anim` naming no entry of `lanims.xr` (none occurs, D4).

## 11. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`, `bin\xrRender_R1.dll`, `bin\xrSound.dll`.
- Data examined: every record of `spawns\all.spawn` (decoded per sav.md) and the levels of `game.graph`;
  the zone sections of `config\misc\zone_*.ltx` through `system.ltx` with inheritance; the effect and group
  names of `particles.xr` and the action lists of the effects of `anomaly2\heat_02`; `lanims.xr`; the shipped
  scripts; the header spheres of the first level's online models.
- Analysis: static analysis of the shipped binaries (Ghidra 12.2, headless decompilation and instruction
  listings): the zones' loading, creation, idle start and stop, state changes and their events, per-frame and
  scheduled updates, light step, moving zone and destruction; the game's effect object; the device clock, the
  colour-animation lookup, objects' scheduling and per-frame processing, the object transform, shape bounds;
  the renderer's light record and effect creation by name; the sound calls. Parsing of the data with
  throwaway tools written from the approved specifications and this text (`model.py` implements sections 4.4,
  6.2 and 7). No execution or debugging of the game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/zone-idle-effects/` reproduce every fact.

## Gaps found in review

- The owner id of the Zone block is stated (none in every record) but not what an owner changes: with an owner, the zone gets a deadline 40 s after its creation that a game-mode-dependent step at the end of each scheduled update tests; with no owner the deadline is the largest `u32` and that step never acts.
