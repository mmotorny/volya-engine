# Hanging-lamp lights

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of specification P13 of the approved `particles-and-lights-scope`: every hanging
lamp (`O_HLAMP` record of `spawns\all.spawn`) over time — when its lights are on or off (health, hits,
breaking, switching by the shipped scripts), their colour with the colour animator, range, point or spot
light, the shadow bit, the projected texture, the ambient light, their placement from the light and ambient
bones each frame, and the models' `hanging_lamp_definition` sections; it also states the meaning of every
light-flag bit, including the bits other than 0x02 – 0x20. The glows of lamps are handed to `glows.md`. What
could not be established is marked **unestablished**; what is inferred rather than read from the program or
the data is marked **believed**.

Builds on (not repeated):

- `docs/behavior/offline-simulation.md` (here "OS"): which objects are online (section 6.5, including the
  renderer match of the light flags, 6.5.2).
- `docs/behavior/object-visual-and-transform.md` (here "OV"): the lamp's visual (sections 1, 2) and its
  transform *X* (section 4).
- `docs/formats/skeleton-ogf.md` (here "SK"): model files, bones and the bind pose (sections 1, 4).
- `docs/formats/sav.md` section 7.4.2: the hanging-lamp block and its field names, used below.
- `docs/behavior/dynamic-lights.md` (here "DL"): the light record a lamp's light is (section 2: fields,
  update thresholds, the shadow setting and projected texture having no effect), which lights are drawn
  and how they light geometry (sections 4 – 6).
- `docs/behavior/glows.md`: the lamp glow (section 2.2) and how glows are drawn.
- `docs/formats/lanims-xr.md` (here "LA"): finding a colour animation by name (section 4), its value layout
  (5) and its colour at a time (6.2).
- `docs/formats/ogf.md`: the model file container.
- `docs/behavior/script-environment.md`: the script runtime; `docs/behavior/sound-rain-thunder.md` section
  3.1: the engine's global clock *T* in seconds (real time, stopped while the game is paused).

Conventions: matrices in the row-vector convention (a point **p** is transformed as [**p** 1] · M; rows
counted from 0), as in OV. "f32" is IEEE 754 single precision; *K* is the f32 nearest to 1/255 (bit pattern
`0x3B808081`). "The lamp" is the in-game object a lamp record becomes while it is online. Every fact carries
the name of its check, a script in `checks/hanging-lamps/` of the dirty workspace (`run_all.py` runs them all;
`gen.py`, run first, produces the static-analysis outputs the binary checks read): `hl_bin.py` (binaries),
`hl_data.py` (records, models, `lanims.xr`), `hl_scripts.py` (scripts and configuration), cited by id.

## 1. Overview

1. A lamp record that comes online (OS section 6.5) creates a **main light**, optionally a **glow** and an
   **ambient light**, all as light records of DL section 2, and starts **on** if its health is above 0
   (section 3).
2. Each frame, while its health is above 0 and its main light is on, the lamp places its lights at its
   light and ambient bones and, if it has a colour animator, sets their colour from it (section 4).
3. The lights go off and on together: off when a hit brings the health to 0 or below, when the light bone
   breaks away, or when a script switches the lamp off; on when a script switches it on (section 5).
4. None of this state is kept: a lamp that comes online again starts from its record (section 8).

## 2. The record and the light flags

### 2.1 Fields

The hanging-lamp block of sav.md 7.4.2, field by field, as the lamp uses it when it comes online:

| Field | Use |
|---|---|
| colour (`u32`) | spawn colour of the main light (section 3.2): red = bits 16 – 23, green = bits 8 – 15, blue = bits 0 – 7; bits 24 – 31 unused |
| brightness | factor of every colour the lamp sets (sections 3.2, 4.3) |
| colour animator | an entry of `lanims.xr`, found by name (LA section 4); empty or not found: no animation |
| range | the main light's range |
| light flags (`u16`) | section 2.2 |
| startup animation | not used by the lamp (believed: the lamp plays the motion `idle`, section 3.4) |
| fixed bones | the physics shell's fixed bones (section 3.4) |
| health | the lamp's starting health (section 5) |
| virtual size | not used |
| ambient radius, ambient power | the ambient light's range, and its colour factor |
| ambient texture, light texture | handed to the ambient and the main light as projected-texture names; no effect in the first renderer (DL section 2.5) |
| light bone, ambient bone | bones of the lamp's model that place the main and the ambient light (section 4.2) |
| spot cone angle | the main light's cone (radians), also set when the light is a point light |
| glow texture, glow radius | the glow (section 3.3) |

Every field except the virtual size is read when the lamp comes online; the record's colour is not read
again afterwards. Checks: `hl_bin.py` HL-B1, HL-B2.

### 2.2 Light-flag bits

| Bit | Meaning |
|---|---|
| 0x01 | the lamp gets a physics shell (section 3.4) |
| 0x02 | the main light's shadow setting (DL section 3.2); no effect in the first renderer (DL section 2.5) |
| 0x04 | the lamp exists for the first renderer (OS section 6.5.2) |
| 0x08 | the lamp exists for the second renderer (OS section 6.5.2); otherwise no effect |
| 0x10 | the main light is a spot light; clear: a point light |
| 0x20 | the lamp has an ambient light |
| 0x40, 0x80, 0x100 … 0x8000 | no effect: the game reads no bit of the flags other than the six above |

A lamp with neither 0x04 nor 0x08 is a fatal error (OS section 6.5.2). In the data the bits above 0x20 occur
only together, as 0xB540, in 11 records (flag values 0xB55C, 0xB55E, 0xB57C, 0xB57D); every record has 0x08,
36 have 0x04. Checks: `hl_bin.py` HL-B1, HL-B5, HL-B7; `hl_data.py` HL-D1.

## 3. Coming online

### 3.1 Lights created

With *L* the light flags (all values f32):

- **Main light**: shadow setting = (*L* & 0x02 ≠ 0); type spot if *L* & 0x10, else point; range = range;
  colour = the spawn colour *c* (3.2); cone = spot cone angle; projected texture = light texture.
- **Glow**: only if the glow texture is non-empty (glows.md section 2.2): texture = glow texture, colour = *c*,
  radius = glow radius.
- **Ambient light**: only if *L* & 0x20: type point, shadow setting off, colour = *c* × ambient power (red,
  green and blue each; alpha stays 1), range = ambient radius, projected texture = ambient texture.

All are created off (DL section 2.1); section 3.4 switches them. Checks: `hl_bin.py` HL-B1.

### 3.2 Spawn colour

Each channel *v* (0 – 255) of the record's colour gives the f32 value (*v* × *K*) × brightness, each product
rounded to f32; alpha = 1. Example: colour `0x00FFFFFF`, brightness 0.8 (f32): *c* = (0.8, 0.8, 0.8, 1).
Checks: `hl_bin.py` HL-B0, HL-B1; `hl_data.py` HL-D3.

### 3.3 Glow

The glow is as glows.md section 2.2 states: on exactly while the lamp's lights are on, placed at the main
light's position by section 4.2, no direction. Its colour is set with the lights' colour (3.1, 4.3) and has
no visible effect (glows.md section 1). Checks: `hl_bin.py` HL-B1, HL-B2.

### 3.4 Model, physics and start state

In this order:

1. If the record names a model (OV sections 1, 2), the light bone and the ambient bone are looked up by name in
   it; a name that is empty or not a bone of the model gives "no bone" (believed; every shipped record with a
   model names bones of it, section 9.1). A lamp without a model has no bones.
2. The physics skeleton state of the record is applied (which bones are visible and the root bone; sav.md
   Skeleton block); no shipped record holds a saved physics state (Skeleton flags 0).
3. If the model is animated, it plays the motion `idle` as a cycle; how motions move bones is outside this
   specification (**unestablished** here).
4. If *L* & 0x01 and the lamp has a model, a physics shell is built from it, with the fixed bones of the
   comma-separated fixed-bones field (empty: the model's root bone); a name that is not a bone is fatal
   ("wrong fixed bone"). With 0x01 and no model the game logs "! WARNING: lamp, obj name [%s],flag physics
   set, but has no visual" (name = the record's name) and builds none. How the shell moves the bones is
   outside this specification (**unestablished** here).
   **Disputed in review:** the order of this list is wrong for the physics shell: the shell is built while
   the physics skeleton state of step 2 is applied, before the motion of step 3; only the "no visual" warning
   comes at this point of the order.
5. If health > 0 the lamp is switched **on**, otherwise **off** (section 5.1).
6. The lamp is drawn only if it has a model, and takes part in collisions only if it has a model (believed:
   only then does it have a collision shape, and so only then can it be hit).

Checks: `hl_bin.py` HL-B1, HL-B5.

## 4. Each frame

### 4.1 When

The lamp's frame update runs once per frame while the lamp's **update count** (section 5.1) is non-zero and
the game is not paused. In it, after its physics shell (if any) is processed (outside this specification), the lamp does what follows only
if its health is above 0 and its main light is on; otherwise its lights keep their last placement and
colour. Checks: `hl_bin.py` HL-B2, HL-B3.

### 4.2 Placement

The model's bones are first brought up to date for the frame. With *X* the lamp's transform (OV section 4)
and *B*(*b*) the current model-space matrix of bone *b* (at rest its bind matrix, SK section 4):

- *M* = *B*(light bone) · *X*, or *M* = *X* when there is no light bone.
- Main light: direction = row 2 of *M*, second ("right") vector = row 0, position = row 3 (the record stores
  the direction normalised and applies the update thresholds of DL section 2.2).
- Glow (if any): position = row 3 of *M*.
- Ambient light (if any): with *M*′ = *M* when the ambient bone is the light bone, else *B*(ambient bone) ·
  *X*, or *X* when there is no ambient bone: direction = row 2, right = row 0, position = row 3 of *M*′.

Checks: `hl_bin.py` HL-B2; `hl_data.py` HL-D3.

### 4.3 Colour animation

Only if the lamp has a colour animator: with *T* the global clock in seconds at that frame and *a* the
animator's colour at time *T* (LA section 6.2; red = bits 16 – 23, green = 8 – 15, blue = 0 – 7, LA section
5), and *k* = brightness × *K* rounded to f32:

- main light colour = (red × *k*, green × *k*, blue × *k*, 1), each rounded to f32 (the animator's alpha is
  not used);
- the glow's colour = the same;
- ambient light colour = each of the main light's three channels × ambient power, rounded to f32, alpha 1.

So the record's colour only gives the colour until the first such frame; a lamp without animator keeps its
spawn colour. All lamps with the same animator are in step, since all use *T*; the value of *T* at the first
frame is that of the global clock then (**unestablished** as a number). Checks: `hl_bin.py` HL-B2;
`hl_data.py` HL-D2, HL-D5.

## 5. On and off

### 5.1 Switching

- **Switching on**: the main light, the glow and the ambient light are switched on (DL section 2.4); the light
  bone and all bones below it in the model are made visible; 1 is added to the update count.
- **Switching off**: the three are switched off; the light bone and all bones below it are made invisible
  (so parts of the model attached to it, e.g. the bone `konus` under `bone_lamp` in
  `physics\light\lampa_01_on`, are hidden; how the renderer omits hidden bones' geometry is **unestablished**
  here); then, only if the lamp has no physics shell, 1 is subtracted from the update count.
- The **update count** is an 8-bit count starting at 0 that wraps around. The frame update (4.1) starts when
  the count goes from 0 to 1 and stops when it reaches 0; a count that wraps past 0 to 255 does not start it,
  and a later addition that brings it to 0 does not start it either.
- Switching changes neither the health nor anything else; switching on a lamp whose health is 0 or below
  turns its lights on without placing or colouring them (4.1).
- Coming online with health ≤ 0: 1 is added to the count, then the lamp is switched off.

Checks: `hl_bin.py` HL-B1, HL-B3.

### 5.2 Hits

When the lamp is hit, with *p* the hit's power and *h* the health before the hit:

- a hit on the light bone sets the health to 0; any other hit subtracts 100 × *p* (f32);
- if *h* > 0 and the new health is ≤ 0, the lamp is switched off.

The hit is also passed on to the physics shell (if any) and to the object's general hit handling (believed to
reach the script logic's hit callback of `bind_physic_object.script`, section 6.3); where
hits come from and their power and bone belong to the specifications of weapons and other hit sources (F1 of
the scoping specification). No hit occurs before the first frame. **Disputed in review:** unbacked; no check establishes it. Checks: `hl_bin.py` HL-B0, HL-B4.

### 5.3 Breaking

When part of the lamp's physics skeleton breaks off (it becomes a new object of section
`ph_skeleton_object`, and its bones are made invisible in the lamp), the lamp is switched off if its light
bone is then invisible. When such a break happens is outside this specification (**unestablished** here).
Checks: `hl_bin.py` HL-B5.

### 5.4 Going offline

Every shipped lamp record has state flags 0x02 set and 0x04 clear (OS section 6.5.2), so a lamp of the actor's
level that comes online stays online for the whole stay on the level, at any distance. Checks: `hl_data.py`
HL-D1.

## 6. Switching by scripts

### 6.1 Script interface

- `game_object:get_hanging_lamp()`: the lamp of a game object; for an object that is not a lamp it reports
  the script error "CGameObject : it is not a lamp!" (what a script error does belongs to the script
  specifications).
- The script class `hanging_lamp` has `turn_on()` and `turn_off()`: switching on and off of section 5.1.

Checks: `hl_bin.py` HL-B6; `hl_scripts.py` HL-S4.

### 6.2 `xr_effects.script`

Four functions of the shipped `scripts\xr_effects.script`, called as effects of the script logic's condition
lists (`=name(arguments)`):

- `turn_on(p)` / `turn_off(p)`: the lamp of the online object of the record whose story id is p[1] (found
  with `level_object_by_sid` of `_g.script`); if there is no such online object, `abort("TURN_ON [%s]. Target
  object does not exist", …)` (or `TURN_OFF`) with the calling object's name.
- `turn_on_object()` / `turn_off_object()`: the lamp of the object whose logic calls it.

No other shipped script calls `get_hanging_lamp`, the only way a script reaches the lamp's `turn_on` and `turn_off`. Checks: `hl_scripts.py` HL-S1,
HL-S2.

### 6.3 Shipped uses

Every lamp record has section `lights_hanging_lamp` (`config\system.ltx`: class `O_HLAMP`, script binding
`bind_physic_object.init`); the binding gives a record script logic only if its custom data has a `[logic]`
section. No lamp of the first level has logic, and no logic switches a lamp there (scoping section 1.4).
Elsewhere:

| Logic (data) | Switches | Lamps (story id: record name) | Level |
|---|---|---|---|
| `config\scripts\agr\agr_knockdown.ltx` (custom data of `level_prefix_space_restrictor_0000`) | `turn_off(399)` leaving its first state `sr_idle@start`; `turn_on(399)` from the 2,000 ms timer of `sr_idle@2`; `turn_off(399)` from the 4,000 ms timer of the last state `sr_idle@last` | 399: `kat_lights_hanging_lamp` | `l03u_agr_underground` |
| `config\scripts\yan_labx16_switcher1.ltx` (`lab_primary_switcher_0000`) | `turn_off(908)`, `turn_off(912)`, `turn_on(911)` from a 500 ms timer | 908 `lab_switcher_hanging_lamp`, 911 `…_green_0000`, 912 `…_red_0000` | `l08u_brainlab` |
| `yan_labx16_switcher2.ltx` (`lab_primary_switcher_0001`) | `turn_off(909)`, `turn_off(914)`, `turn_on(913)` | 909, 913, 914 | `l08u_brainlab` |
| `yan_labx16_switcher3.ltx` (`lab_primary_switcher_0002`) | `turn_off(910)`, `turn_off(916)`, `turn_on(915)` | 910, 915, 916 | `l08u_brainlab` |
| `yan_labx16_switcher_primary.ltx` (`lab_primary_switcher_0003`) | `turn_off(911)`, `turn_off(913)`, `turn_off(915)` (two states) | 911, 913, 915 | `l08u_brainlab` |
| `yan_labx16_switcher.ltx` | `turn_off(904)` | none: no record has story id 904, and no record or file names this logic file | — |
| custom data of 6 lamp records `lights_gi_r2_*` | `turn_off_object` when an info portion set by a hit on bone 1 – 4 is held | themselves | `l03u_agr_underground` |

The ten lamps with story ids exist for the first renderer and have no model; the six `lights_gi_r2_*` lamps
exist only for the second renderer, so they never come online with the first. Nine lamp records swing by the
logic scheme `ph_oscillate` (joint `provod`, force 1.5, period 500); one of them, `light_sharnir_1_0001` of
`l12u_sarcofag`, exists for the first renderer, and its light follows the swinging bones by section 4.2. When
the logic's states, timers and info portions fire, and what `ph_oscillate` does, belong to the specifications
of the script logic (**unestablished** here). Checks: `hl_scripts.py` HL-S2, HL-S3.

## 7. The models' `hanging_lamp_definition`

Two shipped models carry a `hanging_lamp_definition` section in their user data (chunk 17):

- `meshes\physics\decor\tv_1.ogf`: `guide_bone = bone_lamp`, `spot_texture = lights\lights_duolight02grey`,
  `spot_angle = 120`, `glow_texture = glow\glow_Yellow`, `glow_radius = 1`;
- `meshes\physics\light\rope_projector_01.ogf`: the same keys with `spot_angle = 60`, `glow_radius = 2`; no
  record uses this model.

The section has no effect: no file of `bin\` contains the name (in single- or two-byte characters) and no
script or configuration file names it. The lamp takes all its values from its record; for example all 7
`tv_1` records name `glow\glow_orange`, not the section's glow. Checks: `hl_bin.py` HL-B8; `hl_data.py`
HL-D4; `hl_scripts.py` HL-S4.

## 8. What is not kept

- The lamp sends no state to its record and writes nothing back into it: health, on/off, the update count
  and the lights' state exist only while the lamp is online (believed for other parts of the program: the
  lamp's own code never writes the record's health).
- A saved game keeps for a lamp only the state of its script logic and of its physics skeleton (including
  which bones are visible).
- So when a lamp comes online again (after a saved game is loaded, or on returning to its level) it starts
  from its record by section 3: on if the record health is above 0, even after it was shot out or switched
  off; a lamp switched by logic is switched again only when its logic fires again.

Checks: `hl_bin.py` HL-B9.

## 9. Verification facts

### 9.1 All shipped records

- 745 lamp records on 18 levels, all of version 118; 16 distinct flag values; bit counts 0x01 10, 0x02 627,
  0x04 36, 0x08 745, 0x10 223, 0x20 680, and 0x40, 0x100, 0x400, 0x1000, 0x2000, 0x8000 11 each.
- Health 100 in 744 records, 30 in `trader_light_0005` (`l01_escape`); virtual size 0.1 in 744.
- 15 models; every one has a bone `bone_lamp`; only `physics\decor\tv_1` and `physics\light\lampa_01_on` lack
  `bone_omni`; every record with a model names a light bone and an ambient bone of its model. One model is
  animated (`physics\light\new_light\light_alarm_1`, with a motion `idle`); the others are not.
- Physics-shell records (bit 0x01): fixed bones `link` in 9, empty in 1. Skeleton flags are 0 in every record;
  empty light or ambient bones occur only in records without a model.
- The 11 colour animators named by records are all entries of `lanims.xr` (exact names).

### 9.2 First-renderer lamps

36 records: `l01_escape` 5, `l03_agroprom` 1, `l03u_agr_underground` 5, `l05_bar` 1, `l07_military` 3,
`l08u_brainlab` 11, `l10u_bunker` 1, `l12_stancia_2` 2, `l12u_control_monolith` 5, `l12u_sarcofag` 2. Of them
11 have no model, 27 have a spot main light, 21 an ambient light, 12 the shadow bit, 2 a physics shell;
animators `light\light alarm_idle` 9, `tv light 1` 7, `light 40 hz` 4, `light\light_omni_white_idle` 4,
`light alarm 1` 3, none 9; glows `glow\glow_orange` 11, `glow\glow_ghost` 9, none 16; one light texture
(`lights\lights_spot_wire_02`, `light_sharnir_1_0001`). Checks: `hl_data.py` HL-D2.

### 9.3 First level at the start

Of the 27 lamp records of `l01_escape`, 5 have light-flags bit 0x04 and come online at the start (OS
section 9.1); the other 22 do not, 11 of them without a model; the flag values on the level are 0x2A, 0x38,
0x3A, 0xB55C, 0xB57C and 0xB57D [HLO-1, HLO-2]. The 5 online lamps are 3 lamps `physics\light\lampa_01_on`
(`trader_light_0000` at 5.7 m, `trader_light_0003` at 1.5 m, `trader_light_0005` at 241.9 m from the actor's
stored position) and 2 television sets `physics\decor\tv_1` (`trader_tv_0000` at 2.6 m, `trader_tv_0002` at
286.3 m) [HLO-1]. All five are spot lights without shadows (bit 0x10 set, 0x02 clear); the three lamps have
no ambient light (bit 0x20 clear), the television sets have one (`trader_tv_0002` also has bit 0x01).
For example the block of `trader_light_0003`: colour 0x00FFFFFF, brightness 0.8, colour animator
`light 40 hz`, range 2.0, light flags 0xB55C, fixed bones `link`, health 100, virtual size 0.1, ambient radius
2.0, ambient power 0.3, ambient texture `lights\lights_duolight02grey` (present but unused, bit 0x20 being
clear), light texture empty, light bone `bone_lamp`, cone 2.094395, glow `glow\glow_orange` radius 0.3,
ambient bone `bone_lamp` [HLO-3]. (Checks [HLO-*n*]: `checks/level-objects-split/run_all.py` of the dirty
workspace.)

The five online lamps all have colour `0x00FFFFFF`, light bone and ambient bone `bone_lamp`,
glow `glow\glow_orange`, cone 120°, health > 0, no logic; all come online on:

| Record | Flags | Brightness | Animator | Range | Glow radius | Spawn colour | Main light at rest: position; direction |
|---|---|---|---|---|---|---|---|
| `trader_light_0000` | 0xB55C | 1.0 | `light 40 hz` | 1 | 0.3 | (1, 1, 1) | (−241.2864, −23.7312, −135.4352); (−0.4217, −0.7547, 0.5026) |
| `trader_light_0003` | 0xB55C | 0.8 | `light 40 hz` | 2 | 0.3 | (0.8, 0.8, 0.8) | (−245.4443, −23.6726, −134.5719); (0.5545, −0.7547, 0.3506) |
| `trader_light_0005` | 0xB55C | 0.6 | `light 40 hz` | 1.5 | 0.3 | (0.6, 0.6, 0.6) | (−140.7902, −28.3921, −351.9944); (0.0, −0.7547, −0.6561) |
| `trader_tv_0000` | 0xB57C | 1.0 | `tv light 1` | 3 | 1.0 | (1, 1, 1) | (−245.2665, −22.9093, −133.0806); (0.6428, 0.0, −0.7660) |
| `trader_tv_0002` | 0xB57D | 0.8 | `tv light 1` | 3 | 1.0 | (0.8, 0.8, 0.8) | (−119.8046, −29.1813, −390.8865); (−0.2406, 0.0, 0.9706) |

The television sets also have an ambient light (radius 1, power 0.2) at the same matrix as the main light;
`trader_tv_0002` has a physics shell with empty fixed bones. Positions are row 3 and directions row 2 of
*B*(`bone_lamp`) · *X* with the bind pose (within 6·10⁻⁵). Checks: `hl_data.py` HL-D3.

Colours after the first animated frame (section 4.3; LA section 8), at sample times *T*:

| Animator, brightness | *T* (s) | Frame | Entry colour | Main light colour (R, G, B) |
|---|---|---|---|---|
| `light 40 hz`, 0.8 | 0.35 | 3 | `0x00FFDEAB` | (0.800000, 0.696471, 0.536471) |
| `light 40 hz`, 0.6 | 0 | 0 | `0x00FFD5A5` | (0.600000, 0.501176, 0.388235) |
| `tv light 1`, 1.0 | 0.5 | 7 | `0x00427DAF` | (0.258824, 0.490196, 0.686275) |
| `tv light 1`, 0.8 | 2.0 | 30 | `0x0050AFFD` | (0.250980, 0.549020, 0.793726) |

Checks: `hl_data.py` HL-D5.

## 10. Statements of earlier specifications settled here

None remain: the lamp statements of section 6 of the former specification of level objects, which this
specification settled, are stated in sections 2 – 5 and 9.3.

## 11. Unestablished

- How motions (`idle`), the physics shell and the `ph_oscillate` scheme move the bones (sections 3.4, 6.3).
- How the renderer omits the geometry of invisible bones (section 5.1).
- Where hits come from and their power and bone (section 5.2); when a physics skeleton breaks (5.3).
- When the script logic's states, timers and info portions fire, and what a script error does (section 6).
- The global clock's value at the first frame (section 4.3).
- Whether any part of the program other than the lamp writes the record's health (section 8), or changes the
  update count (section 5.1).
- The behaviour with the second renderer (outside this specification).

## 12. Provenance

- Date: 2026-11-24; revised 2026-11-27: references to the former specification of level objects replaced by the
  specifications its content moved to when it was split by system, and its first-level lamp facts (section 6 and 7.2 there)
  moved into section 9.3 unchanged.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll` and `bin\XR_3DA.exe` (static analysis), all files under `bin\`
  (string search).
- Data examined: the 745 hanging-lamp records of `spawns\all.spawn` and the custom data of all records;
  the 15 lamp models and `meshes\physics\light\rope_projector_01.ogf`; `lanims.xr`; `config\system.ltx`, the
  logic files under `config\scripts\`; `scripts\xr_effects.script`, `_g.script`, `bind_physic_object.script`,
  `lua_help.script`; every script, configuration and model file for the names used above.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation with Ghidra
  12.2): the reading of the lamp record, the creation of its lights and glow, its per-frame update, its
  switching, hit and break handling, its save data, its script interface, the per-frame object update and
  the colour-animation lookup; text search in binaries, scripts and configuration; parsing of the data with
  throwaway tools written from the approved specifications and this text (`hl.py` decodes the lamp block
  from the field list of section 2.1, places the lights by section 4.2 and colours them by section 4.3). No
  binary was executed. The checks in `checks/hanging-lamps/` reproduce every fact.

## Gaps found in review

- A record whose model is not a skeleton model (visual type 10 or 3) is a fatal error when the lamp comes online; section 3.4 does not state it (every shipped lamp model is type 10 or 3).
- Besides the pause, the per-frame object update of section 4.1 is skipped in a frame in which a global time value (believed: the frame's time step) is not above 10⁻⁷; this condition is not stated.
