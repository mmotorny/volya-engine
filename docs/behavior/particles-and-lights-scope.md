# Particle effects and dynamic lights (scoping specification)

Scoping specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of the area of issues #99 and #116: particle effects (the effect data in full),
the fires and similar effects present on the first level (`levels\l01_escape\`) at the start of a New Game,
effects elsewhere, and dynamic lights (their data, their simulation and drawing, how they light the world),
including lamps and dynamic lighting beyond the first level's start. Section 1 lists what the data holds and
what is present at the start; section 2 proposes a split into small specs, one format or one behaviour each;
section 3 gives the completeness pass and the follow-up scoping spec that splits the remainder. It states no
format layout or behaviour in detail. What could not be established is marked **unestablished**.

Builds on (not repeated): `docs/formats/level.md` (chunks 6 and 7), `docs/formats/ogf.md`,
`docs/behavior/static-shading.md`, `docs/behavior/offline-simulation.md` (which objects are online at the
start, 6.5, 9.1), `docs/behavior/object-visual-and-transform.md` (the object transform, 4),
`docs/behavior/hanging-lamps.md` (the hanging-lamp structure), `docs/behavior/level-loading.md` (section 5.9, the
`level.ps_static` record layout), `docs/formats/sav.md` (`spawns\all.spawn` records),
`docs/formats/chunks.md`, `docs/formats/ltx-config.md`, `docs/behavior/sky.md`,
`docs/behavior/script-loading.md`, `docs/behavior/script-environment.md`,
`docs/behavior/options-screens.md`.

"Scripts" means the game's shipped Lua scripts (`scripts\*.script`). Shader definition files
(`shaders\r1\*.s`) are data, handled as in static-shading.md. "Region" names the 68-byte records inside
action records that describe a point, line, sphere or other shape (the data holds no word for them).
Every fact carries the name of the check that reproduces it, a script in `checks/particles-and-lights-scope/`
of the dirty workspace (`run_all.py` runs them all; `extract_all.py`, run first, extracts every non-media
file of the merged archives). Values of configuration keys are after LTX inheritance and comment removal.

Sounds are not in this area: the zones' idle sounds (`idle_sound`) and the sounds of the weather's ambient
effects belong to the level sound area; P9 and P11 state the on/off state and timing those sounds follow.

## 1. The data and what is present

### 1.1 The effect library

`particles.xr` (db4 and dba, identical: 1,394,071 bytes, CRC-32 0x4E93C6F1; `archive.py`) holds 1,075
effects and 252 groups with disjoint names (`particles.py`).

- Effect records use the chunk ids 1 – 8 and 33 – 37 (`library.py`). Chunks 6, 7, 8, 33, 34 and 37 occur
  in an effect exactly when its flag word has bit 0x400, 0x1, 0x4000, 0x10000, 0x40000 and 0x8000
  respectively (`gates.py`). Chunk 33 (12 bytes) occurs in 71 effects; bit 0x20000 is set in 40 effects
  (31 of them with chunk 33) and bit 0x80000 in 2 (both with chunk 33) (`gates.py`). The union of the
  effects' flag words is 0xFFFC01 (`library.py`).
- Every effect's action list is consumed exactly when each action type is given one fixed record size;
  the library uses 22 action types, ids 1, 4, 6, 7, 8, 9, 10, 12, 14, 15, 16, 17, 19, 21, 22, 23, 24, 25,
  27, 29, 30, 31, and 11 region kinds, ids 0 – 10 (`library.py`). Chunk 36 of the effects pairs each id
  with one word (after removing trailing digits): 1 `bounce`, 4 `damping`, 6 `follow`, 7 `gravitate`,
  8 `gravity`, 9 `jet`, 10 `killold`, 12 `move`, 14 `orbitpoint`, 15 `randomaccel`, 16 `randomdisplace`,
  17 `randomvelocity`, 19 `sink`, 21 `source`, 22 `speedlimit`, 23 `targetcolor`, 24 `targetsize`,
  25 `targetrotate`, 27 `targetvelocity`, 29 `vortex`, 30 `turbulence`, 31 `scatter` (`action_names.py`).
  These words name the actions below.
- Group records use chunk ids 1 – 6, each in all 252 groups; the groups have 1,047 members, whose flag
  words take 17 distinct values (`library.py`).
- The effects name 6 engine shaders: `particles\add` (383 effects), `particles\blend` (322),
  `particles\alpha_add` (213), `particles\xdistort` (102), `particles\xadd` (49), `particles\set` (1)
  (`library.py`); all six are definitions in `shaders.xr`, and `shaders\r1\particle.vs`, `particle.ps` and
  `particles_xdistort.s` are in db4 (`shaders.py`).

### 1.2 Effect sources

On the first level at the start:

| Source | Count on the level | Effect or group named | Check |
|---|---|---|---|
| `level.ps_static` (83 bytes, one chunk) | 1 | effect `tests\muhi` | `level.py` |
| zones of section `zone_mosquito_bald_weak` / `_noart` | 85 / 2 | group `anomaly2\bold_idle` (key `idle_particles`) | `zones.py` |
| zones of section `zone_mine_field` | 49 | effect `static\mine` | `zones.py` |
| zones of section `zone_flame_small` (the fires) | 11 | group `explosions\campfire_05` | `zones.py` |
| zones of section `zone_witches_galantine` | 9 | group `anomaly2\electra_idle` | `zones.py` |
| zones of section `zone_mincer_weak_noart` | 4 | group `anomaly2\mincer_idle` | `zones.py` |
| zones of `zone_radioactive_killing` / `_average` | 17 / 1 | none | `zones.py` |
| the online artefact `esc_af_medusa` (section `af_medusa`) | 1 | group `anomaly2\artefact\artefact_gravi` (key `particles`) | `objects.py`, `particles.py` |
| the weather's ambient effects | — | the weather sections of the start (sky.md: A `default_weather_05`, B `default_weather_06`) name, in `config\weathers\env_ambient.ltx`, `ambient_env_tuman` (effects `nature\fog_foggy_00`, `nature\fog_stormy_01`; period 5, 10) and `ambient_env_morning` (`nature\fog_foggy_00`; period 30, 60) | `weather.py` |

Every zone record of the level has state-flags bit 0x02 (`zones.py`); offline-simulation.md 6.5.2 – 6.5.3 bring
all of them online. Only the 9 `zone_witches_galantine` records have non-zero enabled and disabled times in
their Zone block (enabled 9; disabled 15 or 21), so only they switch on and off over time (`zones.py`).

The `level.ps_static`, zone and artefact roots, with every effect named in any of their groups' member
slots, name 22 effects; those use 14 of the action types (ids 8, 10, 12, 14, 15, 16, 19, 21, 22, 23, 24,
25, 29, 30), 5 region kinds (0, 1, 5, 7, 9) and the shaders `particles\add`, `blend`, `alpha_add`,
`xdistort` (`particles.py`). `tests\muhi` uses the action types 10, 12, 14, 16, 21, 25, 29
(`particles.py`).

Elsewhere in the data:

- 12 levels have a `level.ps_static`, with 133 records naming 22 distinct effects or groups, all in the
  library (`ps_static_all.py`).
- Over all levels, 1,167 records of 7 class ids and 33 sections name an idle effect (`idle_particles`);
  for 49 of them the section has `idle_light = on`, for the others `off` (`zones_all.py`).
- Over all levels, 69 artefact records (class ids `ARTEFACT`, `SCRPTART`) of 21 sections; 62 name an effect
  (`particles`), 30 have `lights_enabled = true` (`artefacts_all.py`).
- `env_ambient.ltx` has 10 effect sections with a `particles` key (`weather.py`).
- Section 3 lists every other place in the data that names an effect.

### 1.3 Light sources

| Source | Data | Check |
|---|---|---|
| `level` chunk 6 (first level) | 41 records: 1 directional (the sun), 40 point | `level.py` |
| `level` chunk 7 (first level) | 22 glows | `level.py` |
| the 5 online hanging lamps of the first level (models: 3 `physics\light\lampa_01_on`, 2 `physics\decor\tv_1`) | colour animators `light 40 hz` (the 3 lamps) and `tv light 1` (the 2 sets), glow texture `glow\glow_orange`, no projected texture, health > 0 | `lamps.py` |
| hanging lamps of all levels | 745 records on 18 levels; light-flag bits: 0x02 in 627, 0x04 in 36, 0x10 in 223, 0x20 in 680; 36 with a projected texture; 11 distinct colour animators, all in `lanims.xr`; the models `physics\decor\tv_1.ogf` and `physics\light\rope_projector_01.ogf` carry a `hanging_lamp_definition` section in their user data | `lamps_all.py`, `searchlight.py` |
| searchlights | 1 record of class id `O_SEARCH` (`mil_search_light`, section `search_light`, level `l07_military`, model `equipments\projector`, script binding `bind_physic_object.init`); the models `equipments\projector.ogf` and `equipments\camera.ogf` carry a `projector_definition` section (in `projector.ogf` with, among others, `spot_texture`, `spot_angle`, `glow_texture`, `glow_radius`, `guide_bone`, `rotation_bone_y`, `rotation_bone_x`) | `searchlight.py` |
| zone idle lights | every zone section of the first level has `idle_light = off`; 1.2 gives all levels | `zones.py`, `zones_all.py` |
| artefact lights | `af_medusa` has `lights_enabled = false`; 1.2 gives all levels | `objects.py` |
| the actor's torch | the actor record's custom data spawns `device_torch` (section in `config\misc\items.ltx`: model `equipments\light_night.ogf`, 73,740 bytes in db4); `config\models\objects\light_night.ltx` holds one section, `torch_definition` (keys `range`, `range_r2`, `color`, `color_r2`, `omni_range`, `omni_range_r2`, `omni_color`, `omni_color_r2`, `color_animator`, `spot_texture`, `spot_angle`, `glow_texture`, `glow_radius`, `guide_bone`); the shipped controls bind the command `torch` to the key `kL` | `objects.py`, `files.py`, `archive.py`, `torch_def.py`, `settings.py` |
| characters' torches | switched by the shipped scripts: `sr_light.script`, called from `xr_motivator.script`; `xr_light.script` also switches torches but nothing calls it; `enable.script`, `state_mgr_animation.script` and `xr_comm_ptr.script` also switch attached items; setting `ai_use_torch_dynamic_lights` (on in `rspec_default.ltx`; a check box of the advanced video options, options-screens.md) | `torch_scripts.py`, `settings.py` |
| thunderbolts | `config\weathers\thunderbolt.ltx`: 8 sections with a `lightning_model`, all with colour animation `weathers\thunderbolt_00` (an entry of `lanims.xr`); none is set at the start (sky.md) | `weather.py`, `lanims.py` |

`lanims.xr` (db0 and dba, identical: 6,665 bytes, CRC-32 0xBFA9BD87; `archive.py`) holds 65 named entries,
among them `light 40 hz`, `tv light 1`, `koster_00` and `weathers\thunderbolt_00` (`lanims.py`;
`koster_00` is named by `zone_flame_small`'s `idle_light_anim` and the helicopter's
`light_color_animmator`, `zones.py`, `objects.py`). A New Game starts at game time 05:30:00
(`config\alife.ltx` `start_time`; `torch_scripts.py`). At the start rain density is 0 (sky.md).

### 1.4 Which parts are data-driven and which scripted

- The zone keys `idle_particles`, `idle_light`, `idle_sound` and the artefact key `lights_enabled` occur in
  no shipped script and occur as strings in `bin\xrGame.dll` (`torch_scripts.py`): the zones' and
  artefacts' idle effects and lights are read from configuration by the program, not by scripts.
- Scripts create effects in five files: `bind_physic_object.script`, `sr_particle.script`,
  `xr_effects.script` (the effect object), `particle_flame.script` (a scripted-action particle) and
  `particle_test.script`. The last two are named by no other script and no configuration file, so they are
  never loaded (script-loading.md section 2.2) (`torch_scripts.py`). Scripts switch hanging lamps only in
  `xr_effects.script` (`torch_scripts.py`).
- No custom data of the first level's 870 records other than the actor's (which spawns `device_torch`), and
  no logic file such data names, mentions particles, lights, torches, lamp switching or the light and
  particle restrictor schemes (`logic.py`); none of the 10 scripts whose names start with `esc`, `escape`
  or `gulag_escape` names them (`torch_scripts.py`). Whether a script starts an effect or switches a light
  on the level before the first frame by another route is **unestablished**; none of these routes does.

## 2. Proposed specs

### 2.1 The split

Each is one format or one behaviour, reviewable alone. "Needs scripts" refers to section 1.4. The owner of
each optional effect chunk is named; if a spec's investigation finds that a chunk's behaviour belongs to
another spec, the owner changes with the user's agreement.

| # | Title | Description (data and observable behaviour) | Game files | Depends on | Needs scripts |
|---|---|---|---|---|---|
| P1 | `particles.xr` (format) | the whole library: container, every effect chunk and the flag bits of 1.1, the record layouts of all 22 action types and 11 region kinds, group records with every member field and flag value; verification over all 1,075 effects and 252 groups | `particles.xr` | chunks.md | no |
| P2 | Particle effect lifecycle and emission | an effect instance over time: play, stop, time limit (chunk 8), update step, placement by a transform and a velocity; particle birth by `source` (21) and the sampling, containment and transformation of all 11 region kinds; observable as particle counts, positions, velocities, sizes, colours, ages per step | `particles.xr` | P1; object-visual-and-transform.md (matrix convention) | no |
| P3 | Particle motion actions | the per-step effect of the 16 actions that move particles: `move` (12), `gravity` (8), `gravitate` (7), `orbitpoint` (14), `vortex` (29), `randomaccel` (15), `randomdisplace` (16), `randomvelocity` (17), `damping` (4), `speedlimit` (22), `jet` (9), `turbulence` (30), `follow` (6), `scatter` (31), `bounce` (1), `targetvelocity` (27) | `particles.xr` | P1, P2 | no |
| P4 | Particle lifetime and appearance actions | the per-step effect of `killold` (10), `sink` (19), `targetcolor` (23), `targetsize` (24), `targetrotate` (25) | `particles.xr` | P1, P2 | no |
| P5 | Effect chunk 33 option | what chunk 33 (12 bytes) and the flag bits 0x10000, 0x20000 and 0x80000 change in a playing effect (meaning **unestablished**); if it involves the level's collision geometry, P5 also depends on `docs/formats/level-cform.md` | `particles.xr` | P1, P2, P3 | no |
| P6 | Particle groups | a group over time: which members play, their start and stop times, the group time limit, the effects a member names in its other slots and when they start, for every member flag value of 1.1 | `particles.xr` | P1, P2 | no |
| P7 | Particle drawing | how particles become sprites: texture frames and their animation (chunk 6), shader and texture (chunk 7), alignment (chunk 37), stretching with speed (chunk 34), culling; the 6 engine shaders of 1.1 including the distortion effects; order and visibility within the frame | `particles.xr`, `shaders.xr`, `shaders\r1\particle.vs`, `particle.ps`, `particles_xdistort.s`, the textures the effects name | P1, P2; static-shading.md (shader resolution, drawing order) | no |
| P8 | Effects placed by the level | for every `level.ps_static` (record layout in level-loading.md 5.9): where each effect is placed and when it starts | the 12 `level.ps_static` files, `particles.xr` | P1, P2; level-loading.md | no |
| P9 | Zone idle effects and lights | for every record naming an idle effect (1.2): placement by the zone's transform, start on coming online, the switching rule of the Zone block's times and stop and restart on switching, the idle light (`idle_light`, `idle_light_anim`, `idle_light_height`, `idle_light_range`, `idle_light_range_delta`) and its on/off; states the on/off state the zones' idle sounds follow | `spawns\all.spawn` (zone records), the zone sections in `config\misc\zone_*.ltx` (on the first level: `zone_mosquitobald.ltx`, `zone_minefield.ltx`, `zone_kampfire.ltx`, `zone_witchesgalantine.ltx`, `zone_mincer.ltx`, `zone_radioactive.ltx`; `files.py`), `particles.xr`, `lanims.xr` | P2, P6, P12, P16; offline-simulation.md, object-visual-and-transform.md, sav.md, ltx-config.md | no |
| P10 | Artefact idle effects and lights | for every artefact section (1.2): whether and how the artefact shows the effect its section names (placement, start, stop, lying or carried) and its light (`lights_enabled`, `trail_light_color`, `trail_light_range`) | `spawns\all.spawn` (artefact records), `config\misc\artefacts.ltx` (holds `af_medusa`; `files.py`), `particles.xr` | P2, P6, P16; offline-simulation.md, object-visual-and-transform.md | no |
| P11 | Weather ambient effects | the effects of the `env_ambient` sections the weather names (1.2): which effect plays when (`effect_period`, `life_time`), where (`offset`) and with what wind (`wind_gust_factor`); states the timing their sounds follow | `config\weathers\env_ambient.ltx`, `weather_*.ltx`, `particles.xr` | P2, P6; sky.md (which weather sections are in effect) | no |
| P12 | `lanims.xr` (format and evaluation) | light colour animations: entry layout, and the colour an entry gives at a time, for all 65 entries | `lanims.xr` | chunks.md | no |
| P13 | Hanging-lamp lights | every hanging lamp of 1.3 over time: on/off (health, and switching by `xr_effects.script` on other levels), colour with the colour animator, range, point or spot light, the shadow bit, the projected texture, the ambient light, placement from the light and ambient bones each frame, the models' `hanging_lamp_definition` sections; hands its glows to P15 | `spawns\all.spawn` (lamp records), the lamps' `.ogf` models, projected-light textures | P12, P15, P16; hanging-lamps.md, ogf.md, script-environment.md (switching) | only for switching (none on the first level, 1.4) |
| P14 | Searchlights | the `O_SEARCH` object: its light and glow from its model's `projector_definition`, its aiming by the rotation bones and guide bone, what its script binding changes, and whether anything uses `camera.ogf`'s `projector_definition` | `spawns\all.spawn` (the record), `meshes\equipments\projector.ogf`, `camera.ogf`, section `search_light`, `scripts\bind_physic_object.script` | P12, P15, P16; ogf.md, script-environment.md | yes (its binding) |
| P15 | Glows | drawing of the levels' glows (chunk 7) and the glows of lamps, searchlights and torches: sprite, size, fading, occlusion, the setting `r1_glows_per_frame` | `level` chunk 7, `shaders.xr`, glow textures | static-shading.md 7.5, level.md section 8, options-screens.md | no |
| P16 | Dynamic lights on geometry | which dynamic lights (the levels' chunk-6 point lights; lamp, searchlight, zone, artefact and torch lights) light which static geometry and which objects, the point and spot passes (static-shading.md section 12) with their attenuation textures, the settings `r1_dlights` and `r1_dlights_clip`; settles what static-shading.md 12 and the earlier object specification's 5.6 (now model-shading.md 8) leave **unestablished** | `level` chunk 6, the 20 programs `shaders\r1\*_point.*` and `*_spot.*` (`files.py`), attenuation textures | static-shading.md, model-shading.md, level.md section 7 | no |
| P17 | Shadow-casting lights | what the lamp light-flag bit 0x02 (627 lamps) and any other shadow setting change in the first renderer | lamp records, as found | P16 | no |
| P18 | Projected-texture lights | how a light's projected texture (36 lamps; the searchlight's and torch's `spot_texture`) changes what it lights in the first renderer | lamp records, the models' light sections, the projected textures | P16 | no |
| P19 | Objects' projected light texture | how the 512 × 512 texture that objects' element 0 samples (the earlier object specification's 5.5, **unestablished** there; now model-shading.md 7) is filled, and when element 0 is used | as found | P16; model-shading.md | no |
| P20 | Torches | the actor's torch (its light's data, `torch_definition`, switching with the `torch` command, its state at the start) and characters' torches (switched by the scripts of 1.3; `ai_use_torch_dynamic_lights`); hands their glows to P15 | section `device_torch`, `meshes\equipments\light_night.ogf`, `config\models\objects\light_night.ltx`, `scripts\sr_light.script`, `xr_motivator.script`, `config\rspec_*.ltx`, `config\default_controls.ltx` | P15, P16; script-environment.md | yes (characters' torches); no (actor's torch) |
| P21 | Thunderbolts | drawing of a thunderbolt (its lightning model and colour animation) and the light it gives, which sky.md leaves out | `config\weathers\thunderbolt.ltx`, the models its `lightning_model` keys name, `lanims.xr` | P12, P16; sky.md | no |

Suggested order (each after its dependencies): P1, P2, P3, P4, P7, P8 (the first level's `level.ps_static`
effect `tests\muhi` uses only action types of P2, P3 and P4, 1.2), P6, P5, P11; P12, P15, P16, P13, P14,
P20, P17, P18, P19, P21; then P9 and P10. P12, P15 and P16 do not depend on P1 – P8 and may run in parallel
with them.

### 2.2 Size

P1 covers 22 action types and 11 region kinds; if its draft exceeds what one review round can check, it
splits by data into the effect and group records and the action and region records. P3, with 16 actions,
splits likewise into forces (`gravity`, `gravitate`, `orbitpoint`, `vortex`, `jet`, `turbulence`,
`randomaccel`) and the rest. P16 splits, if needed, into the point and spot passes with their attenuation
textures, and the choice of which lights reach which geometry.

## 3. Completeness pass and the follow-up

`coverage.py` scans every non-media file of the merged archives (4,028 files; `extract_all.py`) and lists
every item that names an effect or group of `particles.xr` or holds light data: every configuration line
whose value names an effect (72 distinct keys) or whose key names light data (39 distinct keys), every
other file naming an effect (29), the light sections of models' user data, the `all.spawn` record kinds of
1.2 and 1.3, `lanims.xr` and the level chunks 6 and 7. It assigns each item to exactly one owner:

| Owner | Items |
|---|---|
| P1 – P7 (`particles.xr`) | 1 |
| P8 | 12 |
| P9 | 90 |
| P10 | 74 |
| P11 | 10 |
| P12 | 1 |
| P13 | 3 |
| P14 | 3 |
| P15 | 1 |
| P16 – P18 (chunk 6) | 1 |
| P20 | 6 |
| P21 | 16 |
| never loaded (`particle_flame.script`, `particle_test.script`) | 2 |
| not a light (keys that are section names, e.g. item names) | 50 |
| F1 | 996 |

**F1, Gameplay-triggered effects and lights (scoping)**, a follow-up scoping spec under #116, owns every
item no other owner takes. By the scan these are: configuration keys of `config\creatures` (monsters and
the helicopter; e.g. `hidden_particles`, `particle_invisible`, `particles_invisible_tracks`,
`hugekick_particles`, `flame_particles_*`, and their `light_*` keys), `config\weapons` (shots, shells,
grenades, flash lights), `config\models` (vehicles and their headlights, destroyable objects),
`config\mp`, `config\system.ltx`, `config\scripts` (logic that starts effects), and the `config\misc` keys
other than P9's and P10's (zones' entrance, hit, blowout, tearing, telekinesis, accumulation, awakening and
artefact-birth effects and blowout lights; artefact birth stages; items' breaking and night-vision
effects); `gamemtl.xr` (material impact effects); effect names in 7 model files, 6 `level.spawn` files and
`spawns\all.spawn` record data; and the vehicles' model light sections (`coverage.py`). Some of these show
while a state holds (e.g. a monster's invisibility) rather than on a single event; all depend on gameplay
state or events (hits, shots, monster states, anomaly reactions, scripts) that no approved spec covers, so
F1 splits them once that behaviour is scoped. Their effects are drawn with P1 – P7 and their lights lit
with P16 – P18.

## 4. Unestablished

The meaning of chunk 33 and of the effect flag bits (P1, P2, P5, P7) and of the member flag values (P6);
how the actor's torch reaches `torch_definition` (P20); any script route not covered by 1.4.

## 5. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binary `bin\xrGame.dll` (string search only).
- Data examined: archive listings and every non-media file of the merged archives; in detail
  `particles.xr`, `shaders.xr`, `lanims.xr`, `gamemtl.xr` (effect names only); `level` (chunks 6, 7) of
  `l01_escape` and every `level.ps_static`; `spawns\all.spawn` and `game.graph` (records of all levels,
  the custom data of `l01_escape`'s records and the logic files it names); the configuration; model user
  data; the shipped scripts.
- Analysis: parsing of the data with throwaway tools written from the approved specifications listed above
  and from this text; text search in scripts, configuration, models and the binary. The checks in
  `checks/particles-and-lights-scope/` reproduce every fact.

## Gaps found in review

- The effect flag bits 0x800, 0x1000, 0x2000, 0x100000, 0x200000, 0x400000 and 0x800000 (set in 421, 339,
  95, 51, 126, 12 and 1 effects) are assigned to no behaviour spec; section 4 names their meaning only
  collectively.
- The flag word that every action record carries (values combining 0x1, 0x2, 0x20000000 and 0x80000000)
  is not mentioned, and no spec is named for what it changes.
- Chunk 36 of an effect can name action ids that its action list (chunk 4) lacks (e.g. id 5, `explosion`,
  in one effect); no spec states how chunk 36 relates to the action list.
- The program itself names two effects of the library (`anomaly\galantine`, `vehiclefx\exhaust_1`); the
  completeness pass scans only archive files, so these have no owner.
- P8's dependencies omit P6, although 16 of the 22 names in the levels' `level.ps_static` files are groups.
- P21's lightning models are `.dm` files, whose format no approved or proposed spec covers.
- The first level's online helicopter (section `helicopter`) names effects and light keys (e.g.
  `smoke_particle`, `light_color_animmator`); sections 1.2 and 1.3 do not state whether any of them shows at
  the start (F1 owns them).
- The hanging-lamp light-flag bits other than 0x02, 0x04, 0x08, 0x10 and 0x20 (set in the data, meaning
  unestablished in the earlier object specification's section 6; now hanging-lamps.md 2.2) are assigned to no spec.
- What the first renderer does with the level's chunk-6 directional (sun) record is assigned to no spec;
  P16 names only the point lights.
- Whether `enable.script`, `state_mgr_animation.script` and `xr_comm_ptr.script` switch torches is not
  settled, and P20's game files do not list them.
