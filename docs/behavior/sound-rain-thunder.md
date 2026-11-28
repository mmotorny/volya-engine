# Rain and thunder sounds

Specification S8 of the level-sound scoping specification (`docs/behavior/level-sound-scope.md`): the
rain sound and the thunder sounds of the weather. It covers the weather keys that drive them
(`rain_density`, `thunderbolt`, `bolt_period`, `bolt_duration` of the weather sections, the thunderbolt
collections and bolt sections of `config\weathers\thunderbolt.ltx`), the rule that starts, places,
sets the volume of and stops the looped rain sound, the rule that schedules lightning bolts and plays
their thunder, and the values of this edition, with the first level's default weather in detail. The
pictures of the rain and of the lightning (drops, splashes, the bolt model, its flash of the sky, sun
and fog colours) are not sound and not covered here. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked
**believed**.

Builds on (not repeated here):

- S1, `docs/formats/sound-ogg-params.md`: a sound name *n* names `sounds\<n>.ogg`; each file's min
  distance *m*, max distance *M*, base volume *b* and AI sound type; the reports to the AI.
- S2, `docs/behavior/sound-positional.md`: creating a sound with a volume category (section 3.1);
  **play** and **play independently** with the flags looped and 2D and a delay (section 3.2); the
  emitter controls position, range and play volume *v* (section 3.3); **stop at once** and **stop all**
  (section 4.4); pausing (section 4.5); the volume step and the effects category (sections 5.3, 5.4);
  distance attenuation and the listener, the camera's eye (sections 7.2, 7.4).
- S4, the ambient sounds of the weather (`sound-weather-ambient.md`, the specification of the
  level-sound split for the weather's ambient sets): its section 3.1 (the engine's **global clock**) and
  section 3.2 (the **shared generator** of random numbers *r*, 0 … 32,767).
- S6, sound occlusion (`sound-occlusion.md`, the specification of the level-sound split for occlusion):
  the occlusion target of every 3D emitter. It applies to the rain and thunder emitters as to any other
  3D emitter and is not repeated here.
- `docs/behavior/sky.md` sections 1 and 3 and `docs/behavior/static-shading.md` (here "SS") section 9:
  the weather in effect, its two **current sections** A and B (the hourly sections bracketing the game
  time), their blend weight *t*, the blended sun direction L_sun_dir (SS 9.1, 9.2) and the blended far
  plane (SS 9.2); `docs/behavior/level-loading.md` section 2.6 (the first level's weather, `default`).
- `docs/behavior/object-projected-light.md` sections 5.2 (step 1) and 5.3: an object's **sky visibility**
  *h*; `docs/behavior/object-visual-and-transform.md` section 4: an object's transform.
- `docs/formats/level-cform.md`: the level's static collision mesh.
- `docs/formats/ltx-config.md`: reading the files (floats: the longest leading decimal number, so
  `4.5f` reads 4.5; vectors of floats; lines of a section).

Conventions: *T* is the global clock of S4 section 3.1 in seconds; *dt* is the engine's **frame time**
in seconds (section 3.1). U(*a*, *b*) is *a* + *r* · (*b* − *a*) / 32,767 for a new draw *r* of the
shared generator, computed in 32-bit float, so a value *a* … *b*. lerp(*x*, *y*, *t*) = *x* · (1 − *t*)
+ *y* · *t*. Angles are in radians unless marked as degrees. Positions are in the world coordinates of
the level files (metres, +y up); the **camera position** *E* is the camera's eye of the frame (S2
section 7.2).

**Checks.** Every factual claim carries a tag. The checks are scripts in the dirty workspace,
`checks/sound-rain-thunder/`: `extract.py` extracts the configuration, script, rain and thunder sound
and lightning model files from the archives; `export.py` regenerates the evidence in `ev/` (headless
Ghidra exports of the shipped program files); `run_all.py` runs `check_data.py` (tags D1 – D7: the game
data, decoded by readers written from this text and S1), `check_bin.py` (tags B1 – B17: the program
files) and `model.py` (tags M1 – M7: an implementation of sections 3 – 6 written from this text,
reproducing the worked examples of section 8).

## 1. Overview

- The weather sections carry a **rain density** and may name a **thunderbolt collection** with a bolt
  period and a bolt duration (section 2). Both are blended between the current sections A and B like
  the weather's other values (section 2.3).
- **Rain:** one looped sound, `ambient\rain`, plays while the blended rain density is at least 0.001.
  It is placed 40 m straight above the camera, so it is heard at full level from above, and its volume
  follows the density and how much open sky the player's object sees (section 4).
- **Thunder:** while the section in effect names a collection, lightning bolts come at random intervals
  of about the bolt period. Each bolt is placed at a random point near the far plane, roughly in the
  sun's compass direction; for half of the bolts a thunder sound of the bolt's description plays at
  that point once, delayed by the distance / 300 m/s; the other half come without sound and are
  followed at once by another bolt (section 5).
- Both are played by the program without scripts: no script names these sounds or keys [D5, B17].
  Raindrops hitting surfaces make no sound [B12].
- Both are 3D sounds of the effects category, not reported to the AI (sections 4.1, 5.5).

## 2. Data

### 2.1 Weather section keys

Every weather section (each hourly section of a weather cycle and each section of a weather effect,
sky.md section 3.2) is read for [B7]:

| Key | Read as | Use |
|---|---|---|
| `rain_density` | float, clamped to 0 … 1 | rain (section 4) |
| `thunderbolt` | text: a collection section name, or empty for none | thunder (section 5) |
| `bolt_period` | float (s); read only when `thunderbolt` is not empty, otherwise 0 | thunder schedule |
| `bolt_duration` | float (s); read only when `thunderbolt` is not empty, otherwise 0 | thunder schedule |

A section without a collection therefore has period and duration 0 whatever its file writes: in this
edition the 20 sections of the default cycle without a collection write `bolt_period = 3.f` (one of them
`7.f`) and `bolt_duration = 0.25f`, which are not read [D2, B7]. Every section of this edition has both
`rain_density` and `thunderbolt` keys [D4]. No other key of a weather section enters these rules (e.g.
`rain_color`, the wind keys) [B2, B9 – B11].

### 2.2 Collections and bolt descriptions (`config\weathers\thunderbolt.ltx`)

- A **collection** is a section whose lines name **bolt sections** (one per line, names without values).
  Each distinct collection name is loaded once, the first time a weather section names it; the same name
  always means the same collection [B7]. Its **descriptions** are its lines in order [B7].
- A **bolt section** gives `lightning_model` (a `meshes\` file; a missing file is an error, believed
  fatal), the
  picture keys `color_anim`, `gradient_top_*`, `gradient_center_*`, and `sound`: a sound name. If
  `sound` is not empty, a sound is created from it, effects category, AI sound type 0 (S2 section 3.1;
  section 5.5) [B7]; with an empty value the description has no sound (none in this edition [D3]).
- The section `[thunderbolt_common]` gives, for all bolts [B8]:

| Key | Read as |
|---|---|
| `altitude` | two floats, degrees: the altitude range *a*₀, *a*₁ of a bolt above the horizon |
| `delta_longitude` | float, degrees: Δ, the spread of a bolt's heading |
| `min_dist_factor` | float, capped at 0.95: *f*, the nearest bolt distance as a fraction of the far plane |
| `second_propability` | float, clamped to 0 … 1: *q*, the chance that a bolt has no sound (section 5.4) |
| `tilt`, `sky_color`, `sun_color`, `fog_color` | the bolt's picture and flash; not sound |

These are read once, when the program starts [B1, B8].

### 2.3 Blending

Every frame the environment blends A and B with the weight *t* (SS 9.2; no environment modifier enters
these values) [B6]:

- rain density ρ = lerp(A.`rain_density`, B.`rain_density`, *t*);
- bolt period *P* = lerp(A.`bolt_period`, B.`bolt_period`, *t*) and bolt duration *D* =
  lerp(A.`bolt_duration`, B.`bolt_duration`, *t*), with 0 for a section without a collection;
- the **collection in effect** is B's if *t* ≥ 0.5, else A's (none if that section names none) [B6];
- the far plane *F* is the blended far plane of SS 9.2 (with modifiers and `rs_vis_distance`) [B6].

So near the edge of a thundery hour the period shrinks towards 0 (section 8.2).

## 3. When the rules run

### 3.1 Clocks

- *T* (seconds) is the global clock of S4 section 3.1: real time, stopped while the game is paused
  [B13].
- The **frame time** *dt* (seconds) is smoothed: each frame *dt* ← 0.9 · (real seconds since the
  previous frame) + 0.1 · *dt*, then limited to at most 0.1; it is 0 on a frame while the game is paused
  [B13]. (With the device flag 0x100, off initially [B16], every frame counts 0.02 s instead [B13].)

### 3.2 The frame

Once per frame, while a level is running and ready and the game is not paused, the environment is
updated: first the blend (section 2.3), then the thunder rule (section 5), then the rain rule
(section 4) [B5]. Neither rule runs without a level [B5, B2]. (The environment also updates on a paused
frame while a device counter is non-zero, believed to be the few precaching frames after a level is
loaded [B5].)

The rain and thunder state below is created when the program starts and kept for the whole run, across
levels [B1]: it is not reset when a level is loaded or left.

## 4. Rain

### 4.1 The sound

The rain sound is created once when the program starts from the name `ambient\rain`, so its file is
`sounds\ambient\rain.ogg` (S1), in the effects category with AI sound type 0 [B1, D1]. The files
`sounds\ambient\rain_l.ogg`, `rain_r.ogg` and `rainonroof.ogg` also exist but nothing names them [D5].
The rain has a state **on** or **off**, off initially [B1].

### 4.2 The rule

Every frame (section 3.2), with ρ the blended rain density [B2]:

1. If the rain is **off**: if ρ < 0.001, nothing happens. Otherwise the rain turns **on**, the sound is
   played (S2 **play**: looped, 3D, no delay), and the new emitter's range is set to min distance 40,
   max distance 80 (replacing the file's) [B2]. Then step 3.
2. If the rain is **on** and ρ < 0.001: it turns **off** and its emitter, if it has one, is stopped at
   once (S2 section 4.4) [B2]. Nothing else this frame.
3. While the sound has an emitter: its position is set to *E* + (0, 40, 0) and its play volume to
   *v* = *k* · ρ · 1.1 (*k*: section 4.3) [B2]. If the sound has no emitter, nothing is set.

The emitter is at the min distance straight above the listener, so S2's distance factor is 1: the rain
is heard at full level, from above (no left – right difference) (believed: the camera position of the
frame is also the listener of S2 section 7.2; a one-frame lag between them changes the distance by the
camera's movement in one frame) [B2, B13]. What reaches the device is S2's volume step with this *v*,
the file's base volume (*b* = 1 [D1]), the effects volume and S6's occlusion.

### 4.3 Sky factor

*k* depends on the sky visibility *h* of the **view object** (the object whose camera is shown; in
normal play the player) (`object-projected-light.md` sections 5.2, 5.3) [B3]:

    k = 1 − 2 · (0.3 − min(h, 0.3))   (= 0.4 + 2 · min(h, 0.3))

so *k* = 1 when at least 30 % of the sky rays are free and falls to 0.4 under a full roof. *k* = 1 when
there is no view object or the program keeps no sky visibility for it (believed: an object without a
model) [B3]. Reading *h* applies `object-projected-light.md` section 5.3's
once-per-frame smoothing step if it has not run yet this frame, without casting rays [B4]. The rays of
the view object are cast by the renderer once per rendered frame whether or not the object's own model
is drawn (also in first-person view), while the device flag 0x400 is set (it is initially [B16]) and a
further condition of the renderer holds that is not traced (believed: always in normal play) [B4]. *h* starts at 0.5 (*k* = 1) [B4]. Facts on the rays that `object-projected-light.md`
section 5.2 marks as believed: they test only the level's static collision mesh (`level-cform.md`), and
a ray is not free when it meets any triangle of it, from either side, at a distance greater than 0 and
at most 50 m (triangles whose plane is within a determinant of 10⁻⁵ of the ray's direction are not met)
[B4]. Which object is the view object in which camera mode is believed (the player in normal play).

### 4.4 Levels and restarts

Because the state is kept across levels (section 3.2) and leaving a level stops every sound at once
(S2 "stop all"), a rain that is on when a level is left stays on without an emitter: on the next level
(or after loading a saved game) the rain sound is silent until ρ falls below 0.001 and rises again
(believed: follows from the rules; not executed) [B1, B2]. The same holds after any stop of the rain's
emitter by other means (e.g. a sound restart of S2) (believed).

## 5. Thunder

### 5.1 State

The thunder keeps, for the whole run [B8, B9]: **enabled** (off initially), the **next bolt time** *N*
(0 initially), and for the bolt in progress: **running** (off initially), its **life** λ and **elapsed**
time *e*.

### 5.2 The rule

Every frame (section 3.2), with *P*, *D* and the collection in effect of section 2.3 [B9]:

1. If enabled differs from "a collection is in effect": enabled takes that value and
   *N* = *T* + *P* + U(−0.5 *P*, 0.5 *P*) (so *T* + 0.5 *P* … *T* + 1.5 *P*). No bolt this frame.
2. Otherwise, if enabled, *T* > *N*, no bolt is running and a collection is in effect: a **bolt** starts
   (section 5.3).
3. Then, if a bolt is running: if *e* > λ it stops running; *e* increases by *dt* [B9].

A bolt runs from the frame it starts until the first frame at which its elapsed time, counted before that
frame's increase, exceeds its life; a new bolt can start only on a later frame [B9].

### 5.3 A bolt

When a bolt starts [B9, B10]:

- running is set, *e* = 0, λ = *D* + U(−0.5 *D*, 0.5 *D*);
- its **description** is item *r* mod *n* of the collection in effect (*n* its number of lines; for the
  collections of this edition, *n* = 4, every description equally likely [D3]);
- its **direction**: altitude α = U(*a*₀, *a*₁) (degrees converted to radians); heading
  φ = U(β − Δ + π, β + Δ + π), where β is the **sun heading** below; direction
  **d** = (−sin φ · cos α, sin α, cos φ · cos α);
- its **distance** *s* = U(*f* · *F*, 0.95 · *F*) and its **point** **p** = *E* + *s* · **d**.

The sun heading β is computed from the x and z components of the blended sun direction L_sun_dir (SS 9.2)
[B10]:

- if |x| < 10⁻⁷ and |z| < 10⁻⁷: β = 0;
- else if |z| < 10⁻⁷: β = π/2 if x ≤ 0, else −π/2;
- else: β = −atan(x / z), plus π if z < 0.

For a sun direction written as altitude and longitude (SS 9.1: L_sun_dir = (−sin λ cos α, sin α,
cos λ cos α)) β equals the longitude (modulo 2π), so the middle heading β + π points horizontally along
−L_sun_dir, towards the sun's side of the sky [M5].

### 5.4 Sound or no sound

Then one more number decides [B11]:

- if *q* > *r* / 32,767 (32-bit float), the bolt has **no sound** and *N* = *T* + *D* + 0.001: another bolt
  starts on the first frame after this one has stopped running and *D* + 0.001 s have passed (section
  5.2);
- otherwise *N* = *T* + *P* + U(−0.3 *P*, 0.3 *P*) and the description's sound is played (section 5.5).

With *q* = 0.5 (this edition [D3]) exactly half of the values *r* give a sound [M7]. So bolts come in
groups: any number of silent bolts each directly after the previous one, ended by one bolt with thunder,
after which the next group starts 0.7 *P* … 1.3 *P* later (believed consequence: one thunder sound per
group, on average two bolts per group).

### 5.5 The thunder sound

The description's sound is played with S2 **play independently** [B11, B15]:

| Setting | Value |
|---|---|
| looped, 2D | no, no (3D) |
| delay | *s* / 300 s (sound travelling at 300 m/s) |
| position | **p** (fixed; it does not follow the camera) |
| range | min distance *s* / 2, max distance 2 *s* (replacing the file's) |
| play volume, frequency | not set: 1, 1 (S2 section 3.2) |

The sound is in the effects category with AI sound type 0, so no report is made to the AI whatever the
file's own type (S1 section 7.1) [B1, B7]. With the camera still at the bolt's distance *s*, S2's
distance factor is (*s*/2) / (*s*/2 + 0.75 · *s*/2) = 1/1.75 = 0.571429 for every bolt [M4].

### 5.6 What is not sound

The bolt's picture (model, tilt, gradients, colour animation of the flash added to the sky, sun and fog
colours, the ray test that shortens the drawn bolt) does not change anything above: the sound uses the
distance *s* and the point **p** of section 5.3, whatever length the drawn bolt gets [B11]. The picture is out of scope.

## 6. Random numbers

Every draw of sections 5.2 – 5.4 is a new draw of S4's shared generator (S4 section 3.2), interleaved with
the other users of the generator [B14]; the sequence cannot be reproduced, and an implementation may use
any generator of uniform integers 0 … 32,767. A bolt draws, in this order: its life, its description, α,
φ, *s*, three numbers for the picture, the sound decision, and (with sound) the next-time jitter [B9 –
B11].

## 7. Data of this edition

### 7.1 Sound files

| File | Samples | Duration (S2) | *m* | *M* | *b* | AI sound type of the file |
|---|---|---|---|---|---|---|
| `sounds\ambient\rain.ogg` | 264,209 | 5,991 ms | 300 | 300 | 1 | 0 |
| `sounds\nature\thunder-0.ogg` | 469,367 | 10,643 ms | 1 | 300 | 1 | 134,217,856 |
| `sounds\nature\thunder-1.ogg` | 461,253 | 10,459 ms | 1 | 300 | 1 | 134,217,856 |
| `sounds\nature\thunder-2.ogg` | 220,768 | 5,006 ms | 1 | 300 | 1 | 134,217,856 |
| `sounds\nature\thunder-3.ogg` | 384,495 | 8,718 ms | 1 | 300 | 1 | 134,217,856 |

All are mono, 44,100 Hz [D1]. The rain's *m* and *M* and the thunder files' *m* and *M* are replaced by
the ranges of sections 4.2 and 5.5. `sounds\anomaly\stancia_thunder.ogg` (the `stancia` collection)
exists [D1].

### 7.2 `config\weathers\thunderbolt.ltx`

`[thunderbolt_common]`: `altitude = 10, 20`, `delta_longitude = 17`, `min_dist_factor = 0.94`, `tilt = 17`,
`second_propability = 0.5`, `sky_color = 0.1`, `sun_color = 0.9`, `fog_color = 0.1` [D3]. Two
collections [D3]:

| Collection | Bolt sections | Sounds |
|---|---|---|
| `thunderbolt_collection_default` | `sect_thunderbolt_00` … `_03` | `nature\thunder-0` … `nature\thunder-3` |
| `thunderbolt_collection_stancia` | `sect_thunderbolt_stancia_00` … `_03` | `anomaly\stancia_thunder` (×3), `nature\thunder-0` |

Every `lightning_model` they name exists [D3].

### 7.3 Weather sections

Over the 123 weather sections of the edition [D4]:

- sections naming a collection: `default` 4, `pripyat` 12, `radar` 12, `rain` 3, `stancia` 2, the effects
  `surge_day` 5 and `p_surge_day` 3; the `stancia` cycle and the two effects use
  `thunderbolt_collection_stancia`, the others `thunderbolt_collection_default`; bolt periods 2, 3, 4.5,
  5, 7, 10, 12 s, durations 0.25, 0.35 s;
- sections with `rain_density` > 0: `default` 4 and `rain` 25 (all of its sections).

### 7.4 The first level's default weather

`l01_escape` uses the cycle `default` = `[sect_default_weather]`, 24 hourly sections (sky.md) [D2].
Rain and thunder keys as read [D2]:

| Section | `rain_density` | `thunderbolt` | *P* | *D* |
|---|---|---|---|---|
| `default_weather_01` | 0.2 | `thunderbolt_collection_default` | 4.5 | 0.35 |
| `default_weather_12` | 0.85 | `thunderbolt_collection_default` | 5 | 0.25 |
| `default_weather_16` | 0.85 | `thunderbolt_collection_default` | 5 | 0.25 |
| `default_weather_17` | 0.05 | `thunderbolt_collection_default` | 7 | 0.25 |
| the other 20 | 0 | empty | 0 | 0 |

Every section has `far_plane = 350` [D7]; the default `user.ltx` sets `rs_vis_distance 1.` [D6].

## 8. Worked examples

Computed by `model.py` from sections 2 – 6 for the default cycle (32-bit float arithmetic) [M1 – M7].

### 8.1 Rain

- The rain is on (ρ ≥ 0.001), to the whole second of game time: 00:00:18 – 01:59:41, 11:00:05 –
  12:59:55, 15:00:05 – 17:58:47 [M1]. A New Game starts at 05:30, dry.
- Play volume *v* with ρ = 0.85 (12:00, 16:00): 0.935 for *h* ≥ 0.3 (also *h* = 0.5, the initial value);
  0.748 for *h* = 0.2; 0.561 for *h* = 0.1; 0.374 for *h* = 0 [M3]. With *h* ≥ 0.3: 0.22 at 01:00
  (ρ = 0.2), 0.055 at 17:00 (ρ = 0.05) [M3].

### 8.2 Thunder

- A collection is in effect 00:30:00 – 01:29:59, 11:30:00 – 12:29:59, 15:30:00 – 17:29:59 [M2].
- *P*, *D* there: 00:30:00 2.25, 0.175; 00:45:00 3.375, 0.2625; 01:00:00 4.5, 0.35; 01:15:00 3.375,
  0.2625; 01:29:59 2.2512, 0.1751; 11:30:00 2.5, 0.125; 12:00:00 5, 0.25; 15:30:00 2.5, 0.125;
  16:00:00 5, 0.25; 16:30:00 6, 0.25; 17:00:00 7, 0.25; 17:29:59 3.5019, 0.1251 [M2].
- With *F* = 350 m (no environment modifier in range): *s* = 329 … 332.5 m, delay 1.09667 … 1.10833 s,
  range *s*/2 = 164.5 … 166.25 m, 2 *s* = 658 … 665 m [M4]. At the first level's start eye, where two
  modifiers give *F* = 254.478 m (sky.md section 4): *s* = 239.209 … 241.754 m [M4].
- At 12:00 (sun 25° below the horizon at longitude 291°) (**Disputed in review:** the section's `sun_dir` is altitude −25°, longitude 291°; L_sun_dir is the direction the light travels (SS 9.1), so the sun itself is 25° above the horizon): β = −69°, headings 94° … 128°; for heading
  111° and altitude 15° **d** = (−0.90177, 0.25882, −0.34616) [M5].
- A schedule with chosen draws (each U(*a*, *b*) = *a* + *u* · (*b* − *a*)), *P* = 5, *D* = 0.25,
  *F* = 350, *dt* = 0.02 s, frames at *T* = 100, 100.02, …: the collection comes into effect at
  *T* = 100 (*u* = 0.5: *N* = 105); bolt at 105.02 (life 0.25, *s* = 329, sound decision *u* = 0.25:
  silent, *N* = 105.271); it stops running at 105.28; bolt at 105.30 (*s* = 332.5, *u* = 0.75: thunder
  with delay 1.1083 s, range 166.25 … 665 m, *N* = 110.30); bolt at 110.32 (*s* = 330.75: thunder, delay
  1.1025 s, range 165.38 … 661.5 m) [M6].

## 9. Verification facts

- Files and parameters: section 7.1 [D1]; `thunderbolt.ltx`: section 7.2 [D3]; sections: 7.3, 7.4 [D2,
  D4].
- No configuration file names `ambient\rain`, `rain_l`, `rain_r` or `rainonroof`; none of the 440
  `scripts\*.script` files names the rain sound, the bolt sounds, `thunderbolt`, `rain_density`,
  `bolt_period` or `bolt_duration`; of the program files only `bin\XR_3DA.exe` names `ambient\rain`
  (once) and none names `thunder-` [D5].
- Invariants: every weather section has `rain_density` within 0 … 1 and a `thunderbolt` key; every named
  collection exists and its sections have `bolt_period` and `bolt_duration`; every bolt section names an
  existing sound and model [D2 – D4].

## 10. Unestablished

- What the device counter is that lets the environment update on paused frames (believed: precaching
  frames after loading) (section 3.2).
- Which object is the view object in camera modes other than the normal first-person view (section 4.3);
  the renderer's further condition for casting the view object's sky rays, and what clears device flag
  0x400 (section 4.3).
- That the rain stays silent after a level change while it is on (believed, section 4.4).
- What sets device flag 0x100 (section 3.1; also S4).
- How the device renders a sound directly above the listener (S2 section 7.2).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives
  `gamedata.db0` … `gamedata.dbd`, the default user settings file `__support\userdocs\user.ltx`, and the
  shipped program files of `bin\`.
- Data examined: `config\weathers\*.ltx` (all weather cycles and effects, `thunderbolt.ltx`),
  `config\game_maps_single.ltx`, every `config\*.ltx` and `scripts\*.script` file (searched for names),
  `sounds\ambient\rain*.ogg`, `sounds\nature\thunder-*.ogg`, `sounds\anomaly\stancia_thunder.ogg`, the
  lightning models' presence.
- Analysis: static analysis (decompilation and disassembly with Ghidra, headless) of the engine
  executable, the sound library, the first renderer, the collision library, the core library and the
  game library; constants read from the program files. No program was executed. Independent throwaway
  readers and a model written from this text decode the weather data and the sound files and reproduce
  sections 7 and 8. Public documents used: the Ogg and Vorbis I specifications.

## Gaps found in review

- A collection section with no lines: what a bolt does when its collection has no descriptions (*n* = 0 in section 5.3) is not stated (none in this edition).
