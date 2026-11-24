# Level loading (scoping specification)

Functional specification that scopes loading the first single-player level of S.T.A.L.K.E.R.: Shadow of
Chernobyl (Russian edition) far enough to show its static world: what "New Game" does observably up to the
first in-game frame, which level is chosen and why, the complete set of files read to show that level's
static world grouped by role, a structural overview of every game-own format involved (enough to plan one
format specification each), the rendering configuration choices, and a recommended order of per-format
specifications. Full format specifications are out of scope; where a fact could not be established it is
marked **unestablished**.

Conventions:

- Paths are stored paths in the combined game-data tree (archive specification); `\` separates
  directories. The archive holding the winning copy (later archive wins) is given as `db0` … `dbd`.
- Sizes are in bytes. Numbers are little-endian. "Chunked" means the container described in section 5.1.
- Path aliases (`$level$`, `$game_textures$`, …) are those of `fsgame.ltx` (LTX specification, section 10).
- Text files are Windows-1251.
- Formats already specified elsewhere and only referenced here: archives (`db-archive`), LTX
  (`ltx-config`), UI XML and string tables (`ui-layout`, `text-and-fonts`), the title menu (`title-menu`),
  startup and the intro-movie sequences (`startup`). DDS (DirectDraw Surface), Ogg Theora/Vorbis, Lua and
  HLSL are public formats/languages.

## 1. Scope

M3 target: from the title menu's New Game, load the first level and show its static world in 3D (geometry,
its textures and materials, lighting as stored in the data, sky) with a free camera at the player's start
position.

Out of scope unless stated: characters, items, AI, physics, sound, scripting and gameplay. Where the shown
world depends on them it is said so (sections 2.2, 2.5, 2.6, 7).

## 2. New Game, observably

### 2.1 From the menu to the server command

The title menu's New Game opens the difficulty page; choosing a difficulty (title-menu specification,
section on actions) executes, in order, the console commands:

1. `g_game_difficulty gd_novice` (or `gd_stalker`, `gd_veteran`, `gd_master`);
2. `disconnect`, only if a single-player game is already running;
3. `start server(all/single/alife/new) client(localhost)`;
4. `main_menu off`.

Source: `scripts\ui_main_menu.script` (dbb, 8,447 bytes) and `config\ui\ui_mm_main.xml`.

The four `/`-separated server parameters are, in order: the spawn-file name `all`, the game type `single`,
the simulation mode `alife`, and `new` (a new game rather than a saved one).

### 2.2 Which level is loaded and why

The level is **not** named by the command. It is derived from data as follows (established from the
binaries' behavior and confirmed against the data):

1. The spawn file is `$game_spawn$` + first parameter + `.spawn`, i.e. `spawns\all.spawn` (dbb, 4,474,073
   bytes). A missing file is fatal ("Can't find spawn file:").
2. The game's start objects come from that file (section 5.10). Among its 8,641 objects exactly one has
   section `actor` (object name `level_prefix_actor_0001`).
3. The actor's record names a **game-graph vertex**: vertex 4.
4. `game.graph` (dbb, 8,128,486 bytes; section 5.11) gives each vertex a level id; vertex 4 has level id 1.
   The graph's level table maps level id 1 to level name `l01_escape` (section `level01`).
5. The engine loads the level directory of that name: `levels\l01_escape\`.

Consistency evidence: vertex 4's level-local point is (−247.103, −24.788, −134.425), 0.38 m from the actor's
stored position (section 2.5); the 16-byte identifier in the header of `spawns\all.spawn` equals the one in
the header of `game.graph` (`13 8f 12 1e e1 b4 7f 4f 88 fc d4 dc 9e 7c a7 0c`).

The same mapping exists as configuration: `config\game_levels.ltx` (dbb, 11,749 bytes) `[level01]`
`name = L01_Escape`, `id = 01`, `offset = 3000.0, 1000.0, 0.0`, matching the graph's level table
(`l01_escape`, id 1, offset (3000, 1000, 0)). Which of the two the engine consults for the name is
irrelevant here: they agree.

**Level list.** At startup the engine lists the subdirectories of `$game_levels$`. A subdirectory is a
loadable level only if it contains all four of `level`, `level.ltx`, `level.geom` and `level.cform`; levels
are then addressed by that name. Loading a level sets `$level$` to that directory
(`levels\l01_escape\`), so later `$level$` paths resolve there.

### 2.3 What the player sees until the first in-game frame

1. The menu closes (`main_menu off`) and the **loading screen** (section 2.4) appears and is redrawn as each
   loading stage begins.
2. Loading proceeds through the stages of section 2.4 (server start, new-game creation, level files,
   objects, textures, synchronisation).
3. After the last stage the loading image stays drawn over the first 30 rendered frames of the running
   level (sound muted; loading-screen specification, section 2). During those frames, as soon as the level
   is loaded and at most 2 of the 30 frames remain, and because the fourth server parameter is `new`, the
   **intro movie sequence** `<intro_game>` of `config\ui\ui_movies.xml` (dbb,
   13,247 bytes) starts: `textures\intro\intro_stalker.ogm` (db6, 120,077,638 bytes; 512 × 288, 182.2 s;
   startup specification) over the background `intro\intro_back`, with sound
   `characters_voice\scenario\video\intro_stalker` (`_l`/`_r` pair), `pause_state on` (game time paused
   while it plays). The sequence semantics (skipping, sound pair, window) are those of the startup
   specification. The command-line switch `-nointro` suppresses it.
   Details of the trigger are in the loading-screen specification, section 7.
4. When the sequence ends or is skipped, the level is visible from the player's first-person view.

For M3 the movie is not required: the target is the level view of step 4.

### 2.4 Loading screen

Elements (virtual screen space 1024 × 768 units, scaled to the window):

| Element | Data source |
|---|---|
| Background | `textures\ui\ui_load.dds` (db8, 1,048,704 bytes, 1024 × 1024 DXT5); its top 1024 × 768 texel region is stretched over the whole screen; drawn with shader `hud\default` |
| Level picture | `textures\intro\intro_<level>.dds` where `<level>` is the level directory name: `textures\intro\intro_l01_escape.dds` (db6, 65,664 bytes, 512 × 256 DXT1). If that file does not exist, `textures\intro\intro_no_start_picture.dds` (db6, 65,664 bytes). Drawn with shader `font` |
| Progress strip | 40 cells (41 vertex columns) that fill from both ends as stages begin, relative to an expected total (fill pattern: loading-screen specification, section 4.2): 17 when the game type is single-player and the program's own command line contains `alife` (e.g. a `-start server(…/alife/…)` switch); otherwise 14, which is the case for New Game from the menu |
| Stage caption | The current stage's text, centered, colour 0xFF9D8C78 (ARGB), font `[ui_font_graffiti19_russian]` of `config\fonts.ltx` (font texture selection per the text-and-fonts specification) |

Exact rectangles, the fill pattern and caption formatting are specified by the loading-screen
specification (`docs/behavior/loading-screen.md`), which refines this section.

Stage captions are string-table ids resolved through the string tables (text-and-fonts specification); all
are in `config\text\rus\ui_st_other.xml` (dbb, 6,609 bytes). The shown caption is the translation followed by
`...` (three full stops); for `st_client_connecting_to` the translation, a space and the client address
(`localhost`) are shown, followed by `...` (loading-screen specification, section 4.3):

| id | Russian text |
|---|---|
| `st_server_starting` | Сервер: Старт |
| `st_loading_alife_simulator` | Сервер: Загрузка симуляции жизни |
| `st_creating_new_game` | Создание новой игры |
| `st_server_connecting` | Сервер: Соединяемся |
| `st_client_connecting_to` | Клиент: Соединение с |
| `st_opening_stream` | Открытие потока |
| `st_loading_cform` | Загрузка формы объектов |
| `st_loading_shaders` | Загрузка шейдеров |
| `st_loading_geometry` | Загрузка геометрии |
| `st_loading_spatial_db` | Загрузка базы пространств |
| `st_loading_details` | Загрузка детальных объектов |
| `st_loading_sectors_portals` | Загрузка секторов и порталов |
| `st_loading_ai_objects` | Загрузка ИИ объектов |
| `st_client_spawning` | Клиент: Создание |
| `st_loading_textures` | Загрузка текстур |
| `st_client_synchronising` | Клиент: Синхронизация |
| `st_prefetching_objects` | Кэширование объектов |

Order: `st_prefetching_objects` is the first caption, shown only when the game type changes (as it does on
the first New Game after startup); then the server stages (`st_server_starting`, then
`st_loading_alife_simulator`, then `st_creating_new_game`, then `st_server_connecting`); then
`st_client_connecting_to`; the level stages are, in this order, `st_opening_stream`
(the file `level` is opened), `st_loading_cform`, `st_loading_shaders`, `st_loading_geometry`,
`st_loading_spatial_db`, `st_loading_details`, `st_loading_sectors_portals`, then `st_loading_ai_objects`;
then `st_client_spawning` and `st_loading_textures`, and last `st_client_synchronising`. The file `level.ltx` is read before the
`st_opening_stream` caption is set, i.e. under `st_client_connecting_to`.

What each stage reads is given in section 3. Two observable consequences: texture files are read in the
`st_loading_textures` stage, not when shaders are created; after `st_client_synchronising` the loading image
is drawn over the first 30 rendered frames of the running level (sound muted), then disappears.

### 2.5 Start position and view direction

Source: the actor's record in `spawns\all.spawn` (section 5.10).

| Field | Value |
|---|---|
| Position (x, y, z) | (−246.7263, −24.7933, −134.4339) |
| Stored angles (three floats, radians) | (0.0043165, −1.3962545, −0.0006867) |
| Game-graph vertex | 4 |
| Level-graph node | 11713 |

Coordinates are level-local metres with **y vertical**: the level's collision bounding box (from
`level.cform`) spans x −678.9 … 766.6, y −38.7 … 113.9, z −633.8 … 811.7, and `levels\l01_escape\level.ltx`
`[level_map]` gives the map's horizontal extent as `x1 = -490.7`, `x2 = 504.8`, `z1 = -1081`, `z2 = 1114.2`.

**Unestablished:** how the stored angles map to the first-person camera's heading and pitch (the second
angle, −1.396 rad ≈ −80°, is the only significant one and is presumably the yaw about the vertical axis; sign
and zero direction are not established), and the eye height above the stored position (the actor
configuration `config\creatures\actor.ltx` (dbb) has `camera_height_factor = 0.85`; what it multiplies is not
established). Both belong to the spawn/actor specification. The handedness of the coordinate system is
**unestablished**.

The start position lies inside two of the level's environment modifiers (section 5.8, records 1 and 6 at
distances 1.5 m and 0.8 m with radii 4 m), so the first frame is affected by them.

### 2.6 Time of day and weather at the start

- Game start time comes from `config\alife.ltx` (dbb, 8,758 bytes), section `[alife]`:
  `start_time = 05:30:00` (the value after `;` is a comment) and `start_date = 01.05.2012`, parsed as
  `h:m:s` and `d.m.y`. `time_factor = 10`.
- The weather of a level is chosen by game script (`scripts\level_weathers.script`, dbb, 2,082 bytes): it
  reads key `weathers` of the level's section in `config\game.ltx`'s include
  `config\game_maps_single.ltx` (dbb, 6,712 bytes): `[l01_escape]` `weathers = default`, and sets that
  weather immediately. No `postprocess` key exists for `l01_escape`.
- `config\weathers\environment.ltx` (dbb, 940 bytes; included by `config\system.ltx`) maps weather names to
  sections: `[weathers] default = sect_default_weather`; `[sect_default_weather]` in
  `config\weathers\weather_default.ltx` (dbb, 16,608 bytes) lists one section per hour (`01:00:00 =
  default_weather_01` … `00:00:00 = default_weather_00`).
- At 05:30 the neighbouring hourly sections are `[default_weather_05]` (sky `sky\sky_11_cube`) and
  `[default_weather_06]` (sky `sky\sky_2_cube`); both use clouds `sky\sky_oblaka`, `far_plane = 350`,
  `fog_distance = 350`, `fog_density = 0.9`. How the engine combines neighbouring sections in time is for the
  weather specification (**unestablished** here).

For M3 the script can be replaced by its result: weather `default` at 05:30.

## 3. Files read to show the level's static world

"Need" refers to a first static render equivalent to the game's first renderer (section 6): **R** =
required, **O** = optional (improves fidelity), **N** = not needed for the static world (read by the game for
out-of-scope systems). "Kind": **own** = game-specific format, **pub** = public format (possibly with
game conventions). Level-directory files are in `levels\l01_escape\`.

### 3.1 Level directory (81 files, 160,538,212 bytes; all in db0 except where noted)

| File | Size | Kind | Role | Need | Read by stage |
|---|---|---|---|---|---|
| `level` | 4,094,604 | own, chunked | main level file: header, shader/texture table, static visual tree, portals, sectors, lights, glows | R (chunks 1, 2, 3); O (4, 6, 7, 8) | `st_opening_stream` … `st_loading_sectors_portals` |
| `level.geom` | 56,995,500 | own, chunked | vertex and index buffers of the static geometry | R | `st_loading_geometry` |
| `level.geomx` | 13,401,518 | own, chunked (same container as `level.geom`) | position-only buffers; read only by the second renderer | N for the first renderer | `st_loading_geometry` |
| `level.ltx` | 300 | LTX | level configuration; must exist (fatal otherwise); content is only `[level_map]` (PDA map) | R (existence) | before `st_opening_stream` (under `st_client_connecting_to`) |
| `level.cform` | 30,383,712 | own, header + tables | collision mesh; must exist; its bounding box sizes the engine's spatial structures | O (bounds only) | `st_loading_cform` |
| `level.env_mod` | 588 | own, chunked | environment modifier spheres (fog/colour overrides) | O | after `st_loading_sectors_portals` (if the file exists) |
| `level.details` | 8,392,388 | own, chunked | detail objects (grass etc.): models and placement grid | O | `st_loading_details` |
| `build_details.dds` | 349,672 | pub (DDS 1024 × 512 DXT1) | texture of the detail models (named in `level.details`) | O | `st_loading_textures` |
| `level.hom` | 36,180 | own, chunked | occluder mesh for occlusion culling | O | `st_loading_sectors_portals` |
| `lmap#1_1.dds` … `lmap#5_2.dds` (10 files) | 1,048,704 each | pub (DDS 1024 × 1024 DXT5, 1 level) | light maps, in pairs `_1`/`_2` | R | `st_loading_textures` |
| `terrain\terrain_escape.dds` | 4,194,432 | pub (DDS 2048 × 2048 DXT5) | terrain base texture | R | `st_loading_textures` |
| `terrain\terrain_escape_lm.dds` | 4,194,432 | pub (DDS 2048 × 2048 DXT5) | terrain light map | R | `st_loading_textures` |
| `level_lods.dds` | 5,592,560 | pub (DDS 2048 × 2048 DXT5, 12 levels) | billboard atlas for distant trees/objects (shader `details\lod`) | O | `st_loading_textures` |
| `level_lods_nm.dds` | 5,592,560 | pub (DDS, same shape) | companion of `level_lods`; no reference by name found in the data or binaries | N (**unestablished** use) | — |
| `level.ps_static` | 83 | own, chunked | static particle emitters (1 record, `tests\muhi`); needs `particles.xr` | O | during level load (if the file exists) |
| `meshes\brkbl#0.ogf` … `brkbl#48.ogf` (49 files) | 376,593 total | own (`.ogf`, chunked) | models of breakable objects; spawned as objects | N (objects) | object spawning |
| `level.snd_static` | 435 | own, chunked | static sound sources | N (sound) | |
| `level.som` | 1,384 | own, chunked | sound occlusion mesh | N (sound) | |
| `level.ai` | 12,505,535 | own, header + tables | AI navigation grid | N (AI) | `st_loading_ai_objects` |
| `level.gct` | 3,573,054 (dbb) | own, chunked | cross table between navigation grid and game graph | N (AI) | |
| `level.game` | 45,163 (dbb) | own, chunked | named way paths of the level | N (scripting/AI) | `st_loading_ai_objects` |
| `level.spawn` | 205,242 | own, chunked | per-level object list (1,123 records); not needed: the objects of this start come from `spawns\all.spawn` | N | |
| `level.wallmarks` | 100,650 | own, chunked | static decals; no reference by name found in the binaries | N (**unestablished**) | |
| `level.graph` | 14,587 | own | no reference by name found in the binaries | N | |

`level.game` and `level.gct` also exist in db0 (45,183 and 3,573,054 bytes); the dbb copies win.

### 3.2 Global files

| File | Archive, size | Kind | Role | Need |
|---|---|---|---|---|
| `shaders.xr` | dba, 162,365 | own, chunked | library of 183 named shader definitions; resolves the engine-shader names of `level` chunk 2 | R |
| `shaders\r1\*.s` (34 named files plus `.s`) | | pub (Lua) with a game-specific vocabulary | scripted shader definitions; take precedence over `shaders.xr` (section 5.13) | R for the 4 level shaders that have one |
| `shaders\r1\*.vs`, `*.ps`, `*.h` and `shaders\shared\common.h` | 146 files in `shaders\r1\`, 123,292 bytes | pub (HLSL source) | vertex/pixel programs of the first renderer; document the vertex-data decoding (section 5.2) | reference (behavior to reproduce) |
| `shaders\r2\…` | 226 files | pub | the second renderer's programs | N for the first renderer |
| `textures\textures.ltx` | db7, 726,435 | LTX | texture descriptions: `[association]` (detail texture and scale per texture), `[specification]` (bump mode, material), `[types]` | O |
| `textures\…\*.dds` named by `level` chunk 2 | 294 files, 58,874,160 bytes (db7 170, db8 73, db5 42, dbb 8, db6 1) | pub (DDS) | base textures of level surfaces | R |
| detail textures named by `textures.ltx` `[association]` for those 294 | 28 files, 15,776,816 bytes | pub (DDS) | detail textures | O |
| `textures\ed\ed_dummy_bump.dds`, `…_bump#.dds` | db5 | pub | stand-ins for missing bump maps | N for the first renderer |
| `config\system.ltx` and includes | dbc, 22,829 | LTX | includes `weathers\environment.ltx`; section `[details]` (detail-object sway parameters) | R (weather) |
| `config\weathers\*.ltx` | dbb | LTX | weather cycles, hourly environment sections, flares, ambient sound sets | R (sky, fog, ambient colours) |
| `textures\sky\sky_11_cube.dds`, `sky_2_cube.dds` | db7, 786,560 each | pub (DDS cube map, 6 faces 512 × 512 DXT1) | sky of the 05:00 and 06:00 sections | R |
| `textures\sky\sky_oblaka.dds` | db7, 1,398,256 | pub (DDS 1024 × 1024 DXT5, 11 levels) | clouds texture | O |
| `textures\sky\*_cube#small.dds` | db7, 24,704 each | pub | small cube maps; use **unestablished** | O |
| `config\game.ltx` → `game_maps_single.ltx`, `game_levels.ltx` | dbb | LTX | level → weather name; level table | R (weather) |
| `config\alife.ltx` | dbb, 8,758 | LTX | start date and time | R (time of day) |
| `spawns\all.spawn` | dbb, 4,474,073 | own, chunked | start objects including the actor | R (start position; level choice) |
| `game.graph` | dbb, 8,128,486 | own, header + tables | game graph: level table and vertices | R (level choice) |
| `lanims.xr` | dba, 6,665 | own, chunked | colour animations (loaded at startup; used by the UI) | N for the static world |
| `particles.xr` | dba, 1,394,071 | own, chunked | particle effects library (for `level.ps_static`) | O |
| `gamemtl.xr`, `senvironment.xr` | dba | own, chunked | game materials (physics/sound), sound environments | N |
| `shaders_xrlc.xr` | dba, 5,472 | own | no reference by name found in the binaries | N |
| loading screen and intro files | section 2.3, 2.4 | pub | | O (loading screen) |

For the first level a minimal static render needs: `level` (chunks 1–3), `level.geom`, `level.ltx`
(existence), the 307 distinct textures named in `level` chunk 2 (13 from the level directory, 294 from
`textures\`) plus `build_details` named by `level.details` (optional),
`shaders.xr` plus the 4 matching `shaders\r1\*.s` scripts (to know how each surface combines its textures),
and the weather/sky data. The start position needs `spawns\all.spawn` (and `game.graph` to derive the level),
or can be taken from this document's values.

## 4. Dependency graph

```
title menu ──console command "start server(all/…)"──▶ spawns\all.spawn          (name: parameter 1 + ".spawn")
spawns\all.spawn ──header identifier (equal 16 bytes)──▶ game.graph
spawns\all.spawn ──actor record: game-graph vertex index──▶ game.graph vertex ──level id──▶ game.graph level table ──name──▶ levels\<name>\
levels\<name>\ (level list) ──must contain──▶ level, level.ltx, level.geom, level.cform
textures\intro\intro_<name>.dds ◀──name derived from the level directory name
level chunk 3 (visuals) ──u16 shader index──▶ level chunk 2 (shader/texture table)
level chunk 3 (visuals) ──buffer index + ranges──▶ level.geom chunks 9 (vertices), 10 (indices), 11 (LOD records)
level chunk 3 (visuals) ──buffer index + ranges (second geometry)──▶ level.geomx      (second renderer only)
level chunk 3 (visuals) ──u32 child indices──▶ level chunk 3 (visuals)
level chunk 8 (sectors) ──u32 root visual index──▶ level chunk 3;   ──u16 portal indices──▶ level chunk 4
level chunk 4 (portals) ──u16 sector indices──▶ level chunk 8
level chunk 7 (glows) ──u16 shader index──▶ level chunk 2
level chunk 2 entry "<engine shader>/<tex>[,<tex>…]" ──shader name──▶ shaders\r1\<name with \→_>.s, else shaders.xr definition
                                                   ──texture names──▶ $level$<tex>.dds, else textures\<tex>.dds
shaders.xr definition / .s script ──program names──▶ shaders\r1\<program>.vs/.ps (fallback shaders\<program>…)
texture name ──key──▶ textures\textures.ltx [association]/[specification] ──▶ detail / bump textures
level.details model ──shader name + texture name──▶ shaders.xr / .s;  $level$ or textures\ (e.g. build_details)
level.ps_static record ──effect name──▶ particles.xr
config\game_maps_single.ltx [<level>] weathers ──weather name──▶ environment.ltx [weathers] ──section──▶ hourly sections ──▶ textures\sky\*.dds
config\alife.ltx [alife] start_time ──time of day──▶ choice of hourly sections
```

Reference kinds: by **name** (string, resolved through path aliases and fixed extensions), by **index**
(integer into a table of another file or chunk), and by **identifier** (16-byte value that must match).

Texture name resolution (all renderers): a texture name is looked up as `$level$<name>.dds`, then in the
saved-games folder (`$game_saves$`), then as `$game_textures$<name>.dds`; not found anywhere is fatal
("Can't find texture"). Names containing `_bump` follow a separate rule (bump maps; not used by the
first level's shader table).
For `l01_escape` all 307 distinct texture names of `level` chunk 2 resolve; the 13 level-directory ones are
`lmap#1_1` … `lmap#5_2`, `terrain\terrain_escape`, `terrain\terrain_escape_lm` and `level_lods`. In addition
`level.details` names `build_details`, which also resolves to the level directory.

Model name resolution (`.ogf`): the name as given if it already names an existing file; otherwise
`$level$<name>.ogf`, then `$game_meshes$<name>.ogf` (".ogf" appended when the
name has no extension); not found is fatal.

## 5. Structural overview of the game-own formats

### 5.1 The chunk container (shared)

Used by `level`, `level.geom`, `level.geomx`, `level.details`, `level.hom`, `level.env_mod`,
`level.ps_static`, `level.snd_static`, `level.som`, `level.wallmarks`, `level.game`, `level.gct`,
`level.spawn`, `spawns\all.spawn`, `shaders.xr`, `gamemtl.xr`, `senvironment.xr`, `lanims.xr`,
`particles.xr` and `.ogf` models.

A file (or a chunk's payload) is a sequence of chunks, each `u32 id`, `u32 size`, then `size` payload bytes;
chunks follow each other without padding. Payloads are often themselves chunk sequences ("sub-chunks"),
typically numbered 0, 1, 2, … for lists. Lookup is by id. In the top-level chunks of all files examined for
this level the high bit of the id is clear; whether the container supports compressed chunks is
**unestablished**.

`level.cform`, `level.ai`, `level.graph` and `game.graph` are **not** chunked (header plus tables).

### 5.2 `level`

Chunked; top-level chunks of `l01_escape`:

| id | Size | Content |
|---|---|---|
| 1 | 4 | header: u16 version = 14 (any other value is fatal), u16 = 1 (meaning **unestablished**) |
| 2 | 17,864 | shader table: u32 count (396), then that many zero-terminated strings. Entry 0 is empty. Each other entry is `<engine shader>/<texture>[,<texture>…]` (split at the first `/`; textures comma-separated). 18 distinct engine shaders; e.g. `default/mtl\mtl_bochka_02,lmap#5_1,lmap#5_2` (base texture + light-map pair), `levels\escape_grass/terrain\terrain_escape,terrain\terrain_escape_lm`, `details\lod/level_lods`. Required (missing is fatal: "Level doesn't builded correctly.") |
| 3 | 4,065,388 | static visuals: sub-chunks 0 … 11537, one per visual, in `.ogf` structure (section 5.4) |
| 4 | 5,520 | portals: 69 records of 80 bytes: u16 sector, u16 sector, 6 × float3 vertices, u32 vertex count (4 … 6 used) |
| 6 | 4,428 | lights: 41 records of 108 bytes: u32 (1 for the sun, 2 for the others; meaning **unestablished**) followed by a 104-byte record laid out as Direct3D 9's public `D3DLIGHT9` (type, diffuse, specular, ambient, position, direction, range, falloff, three attenuations, theta, phi). One record must be directional (type 3), the sun ("Where is sun?" otherwise); here direction (0.840, −0.423, 0.340). The other 40 are point lights (type 1) |
| 7 | 396 | glows: 22 records of 18 bytes: float3 position, float radius, u16 index into chunk 2. Required by the first renderer |
| 8 | 948 | sectors: 24 sub-chunks; each has sub-chunk 1 (u16 portal indices into chunk 4) and sub-chunk 2 (u32 root visual index into chunk 3) |

The 24 sector root visuals are exactly the 24 visuals that are no other visual's child; every one of the
11,538 visuals is reachable from them. A first static render can therefore draw all visuals reachable from
the roots without using portals.

### 5.3 `level.geom` and `level.geomx` (shared container)

Chunked:

| id | `level.geom` | `level.geomx` | Content |
|---|---|---|---|
| 1 | 4 (u16 14, u16 18) | 4 (u16 14, u16 267) | header: version 14; second value meaning **unestablished** |
| 9 | 37,295,312 | 5,673,404 | vertex buffers: u32 count, then per buffer: a vertex declaration as an array of Direct3D 9 `D3DVERTEXELEMENT9` records (8 bytes each) ending with the end marker (stream 0xFF), then u32 vertex count, then vertex data (stride = declaration size) |
| 10 | 19,330,928 | 7,728,074 | index buffers: u32 count, then per buffer u32 index count and that many u16 indices |
| 11 | 369,224 | 4 | progressive-mesh (sliding-window LOD) records: u32 count (97 / 0), then per record 4 × u32, u32 n and n × 8 bytes |

`level.geom` has 19 vertex buffers (≤ 65,535 vertices each) and 10 index buffers; all vertices are 32 bytes,
in three layouts:

| Layout | Buffers | Elements (offset: type, usage) |
|---|---|---|
| A | 0–4 | 0: FLOAT3 position; 12: D3DCOLOR normal; 16: D3DCOLOR tangent; 20: D3DCOLOR binormal; 24: SHORT4 texcoord 0 |
| B | 5, 7, 8, 11 | as A to 20; 24: D3DCOLOR colour; 28: SHORT2 texcoord |
| C | 6, 9, 10, 12–18 | as A to 20; 24: SHORT2 texcoord 0; 28: SHORT2 texcoord 1 |

`level.geomx` has 8 vertex buffers with only FLOAT3 position (12 bytes) and 4 index buffers.

Decoding of packed values is given by the game's own vertex programs (game data): `shaders\shared\common.h`
decodes a base texture coordinate as (stored pair + (tangent.w, binormal.w)) × 32/32768 and a light-map
coordinate as stored pair × 1/32768; normals are unpacked from the colour-packed bytes. The geometry
specification should take these definitions from those files.

### 5.4 Visuals (`.ogf` structure; `level` chunk 3 and `*.ogf` files)

Each visual is a chunk sequence; chunk 1 is a 44-byte header: u8 format version (4 everywhere), u8 visual
type, u16 shader index (into `level` chunk 2; 393 distinct values used), float3 × 2 bounding box, float4
bounding sphere (centre, radius).

Visual types and chunk sets in `l01_escape` (count):

| Type | Count | Chunks present | Role (from structure) |
|---|---|---|---|
| 0 | 2,861 | 1, 21 (278) or 1, 21, 22 (2,583) | plain mesh |
| 1 | 1,917 | 1, 10 | group: chunk 10 = u32 n + n × u32 child visual indices |
| 2 | 411 | 1, 21, 6 (+ 22 for 384) | mesh with progressive LOD (chunk 6 = LOD data) |
| 6 | 1,839 | 1, 10, 11 | group with a distant billboard (chunk 11, 896 bytes) |
| 7 | 2,822 | 1, 21, 12 | tree/vegetation mesh (chunk 12, 104 bytes = transform/parameters) |
| 11 | 1,688 | 1, 21, 12, 20 | vegetation with progressive LOD (chunk 20 = u32 index into `level.geom` chunk 11) |

Chunk 21 (24 bytes, 6 × u32) references geometry in `level.geom`: by data consistency vertex-buffer index,
first vertex, vertex count, index-buffer index, first index, index count (e.g. (5, 31041, 128, 5, 583479,
1611); the index count is always a multiple of 3 in the samples checked). Chunk 22 contains a nested chunk 21
referencing `level.geomx` instead (second renderer). The model files `levels\l01_escape\meshes\*.ogf` use
the same container with their own geometry chunks (e.g. `brkbl#0.ogf`: chunks 1, 0x12, 9, 0xD, 0x10); a
`.ogf` specification should cover both uses.

### 5.5 `level.details`

Chunked: chunk 0 (24 bytes: u32 version = 3 (required), u32 model count 21, i32 x offset 340, i32 z offset
317, u32 x size 724, u32 z size 723 — field names by data consistency); chunk 1 = 21 sub-chunks, each a small
model beginning with two zero-terminated strings (shader `details\blend`, texture `build_details`); chunk 2 =
placement grid, 8,375,232 bytes = 724 × 723 slots of 16 bytes. Also uses `config\system.ltx` `[details]`
(sway parameters).

### 5.6 `level.hom`

Chunked: chunk 0 (u32 version 0), chunk 1 = 904 records of 40 bytes (three float3 vertices and a u32).
Occlusion culling only.

### 5.7 `level.cform`

Header (36 bytes: u32 version = 4 (required), u32 vertex count 698,013, u32 triangle count 1,375,470,
float3 × 2 bounding box) followed by the vertices (12 bytes each) and the triangles (16 bytes each);
36 + 698,013 × 12 + 1,375,470 × 16 = 30,383,712 = file size. Needed by the engine at load (bounds); only its
header matters for a static render.

### 5.8 `level.env_mod`

Chunked: chunks 0 … 6, each 76 bytes = 19 floats read in this grouping: float3 position, float radius,
float, float, float3, float, float3, float3, float3. The modifiers override environment values (fog, colours)
near their position; the meaning of the individual fields is for the environment specification
(**unestablished** here).

### 5.9 `level.ps_static`

Chunked: one chunk per emitter: zero-terminated effect name (from `particles.xr`) and a 64-byte 4 × 4 float
transform.

### 5.10 `spawns\all.spawn`

Chunked:

| id | Size | Content |
|---|---|---|
| 0 | 44 | header: u32 version = 8 (required), 16 bytes (identifier), 16 bytes (equal to `game.graph`'s identifier), u32 object count 8,641, u32 18 (meaning **unestablished**) |
| 1 | 2,552,770 | objects: sub-chunk 0 = u32 count (8,641); sub-chunk 1 = one sub-chunk per object, each with sub-chunk 0 (2 bytes) and sub-chunk 1 = two chunks holding a spawn record (u16 length prefix, then the record) and an update record; sub-chunk 2 empty |
| 2 | 850,944 | u32 count 42,547, then 42,547 records of 20 bytes (role **unestablished**) |
| 3 | 1,070,283 | named point sets (way paths); sub-chunk 0 = u32 count, sub-chunk 1 = data |

Spawn record (only the fields needed here; the layout is for the spawn specification): u16 message type 1,
zero-terminated section name (`actor`), zero-terminated object name, two bytes, float3 position, float3
angles, … , then the record's version (118 here), … and, in a sub-block, u16 game-graph vertex, float, u32,
u32 level-graph node, u32 flags and the zero-terminated custom-data text. The vertex/node values of the actor
(4, 11,713) are identified by the same field pattern in other objects' records and by the coordinate check of
section 2.2.

### 5.11 `game.graph`

Header plus tables (not chunked): u8 version 8, u16 vertex count 3,512, u32 edge count 8,630, u32 point count
396,380, 16-byte identifier, u8 level count 29, then 29 level records (zero-terminated name, float3 offset,
u8 level id, zero-terminated section name, 16-byte identifier), then 3,512 vertex records of 42 bytes
(float3 level-local point, float3 global point, u8 level id at offset 24, remaining 17 bytes
**unestablished**), then further tables. Level-id 1 owns 252 vertices.

### 5.12 `shaders.xr`

Chunked: chunk 0 empty; chunk 1 (3,277 bytes) holds records beginning with short names `0000`, `0001`, …
(role **unestablished**); chunk 2 = 183 sub-chunks, one shader definition each: an 8-byte class tag (stored
byte-reversed, e.g. stored `"   TES_S"` reads `S_SET   `) followed by the definition's name and its
parameters; chunk 3 = u32 count 183 and the 183 names, zero-terminated, sorted.

Class tags of the 18 engine shaders used by `l01_escape`:

| Engine shader | Tag | Size | Entries in chunk 2 of `level` |
|---|---|---|---|
| `default` | `LM      ` | 404 | 250 |
| `def_shaders\def_vertex` | `V       ` | 404 | 54 |
| `def_shaders\def_objects_lod`, `flora\leaf_wave`, `flora\trunk_wave` | `D_TREE  ` | 443 | 37, 9, 5 |
| `def_shaders\def_aref`, `def_shaders\def_trans` | `LM_AREF ` | 450 | 4, 5 |
| `def_shaders\def_aref_v`, `def_shaders\def_trans_v` | `V_AREF  ` | 450 | 3, 5 |
| `levels\escape_asfalt`, `…_asfalt_noblend`, `…_grass`, `…_sand` | `BmmDold ` | 862 | 1 each |
| `effects\glow` | `S_SET   ` | 1,213 | 3 |
| `selflight`, `effects\lightplanes` | `V       ` (overridden by script) | 404 | 3, 1 |
| `effects\wallmarkblend`, `details\lod` | `S_SET   ` (overridden by script) | 1,213 | 11, 1 |

Tag counts in the whole library: `BmmDold ` 69, `S_SET   ` 51, `MODEL   ` 23, `MODELEbB` 11, `V_AREF  ` 5,
`LmEbB   ` 5, `PARTICLE` 5, `D_TREE  ` 3, `V       ` 3, `LM_AREF ` 2, `D_STILL ` 2, `LM      ` 1,
`E_SEL   ` 1, `E_WIRE  ` 1, `SH_WORLD` 1. A shader specification needs only the tags used by the level (and
`S_SET   ` for UI/sky).

### 5.13 Shader scripts (`shaders\r1\*.s`, Lua)

At device creation every `shaders\<renderer>\*.s` file (`r1` for the first renderer) is loaded as a Lua
module named after the file (the file `.s` itself becomes the global module). An engine shader name is
first converted by replacing `\` with `_`; if a module of that name exists and provides the expected
entry points, it defines the shader, otherwise the `shaders.xr` definition does. For `l01_escape` this
applies to `selflight`, `effects_lightplanes`, `effects_wallmarkblend` and `details_lod`. Program files named
by definitions are read from `shaders\<renderer>\<name>` with fallback `shaders\<name>`.

### 5.14 Other global own formats (not needed for the static world)

`lanims.xr`, `particles.xr`, `gamemtl.xr`, `senvironment.xr` are chunked (section 5.1). `level.ai`,
`level.gct`, `level.game`, `level.snd_static`, `level.som`, `level.spawn`, `level.wallmarks` belong to AI,
sound, scripting and objects.

### 5.15 Sky

The sky is drawn as a fixed shape built into the program (12 vertices, 20 triangles, coordinates ±1 with a
ring of four vertices at height 0), textured with two cube maps that are blended; that they are the two
neighbouring hourly sections' `sky_texture` is presumed (**unestablished**). The clouds are drawn with the
`clouds` shader. No sky mesh file is read. The weather specification should define the shape and blending.

## 6. Rendering configuration

The game ships two renderers (`bin\xrRender_R1.dll`, `bin\xrRender_R2.dll`); one is chosen at startup:

- Console variable `renderer` with values `renderer_r1`, `renderer_r2a`, `renderer_r2`. The command-line
  switch `-r2a` forces `renderer_r2a`, else `-r2` forces `renderer_r2`, else the value from `user.ltx`
  applies (`renderer renderer_r1` in the GOG default `__support\userdocs\user.ltx`, copied to the user's
  documents folder by the installer).
- `renderer_r2a` and `renderer_r2` load the second renderer; if it cannot be loaded ("incompatible
  hardware") or the value is `renderer_r1`, the first renderer is used. The two second-renderer values
  differ in one flag passed to it; the visual effect is **unestablished**.
- The first renderer draws the lighting stored in the data: light maps (`lmap#*`), the terrain light map
  and per-vertex data, combined with the hourly environment's hemisphere and sun colours (its vertex program
  `shaders\r1\lmap.vs` passes the light-map coordinate plus hemisphere and sun terms); it reads
  `level.geom` but not `level.geomx`, and needs `level` chunk 7 (glows). The second renderer additionally
  reads `level.geomx`, uses its own program set `shaders\r2\`, and does not read chunk 7; its lighting model
  is out of scope.
- `rspec_*.ltx` presets and `user.ltx` hold quality settings (`r__detail_density 0.2`, `r__geometry_lod 1.`,
  `rs_vis_distance 1.`, `r1_dlights on`, … in the default file); they change detail density, LOD and dynamic
  lights, not the stored static lighting.

M3 matches the default configuration: the first renderer.

## 7. Where the static world depends on out-of-scope systems

- **Level choice and start position** come from the object system (`spawns\all.spawn`, `game.graph`).
  Either parse the needed parts (sections 5.10, 5.11) or use this document's values for the first level.
- **Weather** is selected by script; replace with `default` (section 2.6).
- **Time of day** comes from the simulation settings (`config\alife.ltx`).
- **Camera** placement relative to the actor (eye height, angle mapping) is part of the actor; unestablished
  (section 2.5).
- Breakable objects (`meshes\brkbl#*.ogf`), other objects, details' animation, particles and sound are not
  needed for a first static frame.

## 8. Recommended order of per-format specifications

1. **Chunk container** (section 5.1) — tiny; everything else builds on it.
2. **`level.geom` / `level.geomx`** — vertex declarations, buffers, packed-attribute decoding (with the
   game's `shaders\shared\common.h` definitions).
3. **`level` file + visuals (`.ogf` structure in chunk 3)** — header, shader table, visual types 0/1/2/6/7/11,
   geometry references, sectors/portals; with 1–2 a first untextured draw is possible.
4. **Shader table and `shaders.xr`/`.s` behavior for the 18 engine shaders** (tags `LM`, `V`, `LM_AREF`,
   `V_AREF`, `D_TREE`, `BmmDold`, `S_SET` and the 4 scripts): how base texture, light-map pair, terrain
   light map and vertex lighting combine in the first renderer. With DDS loading (public) this gives the
   textured, lit static world.
5. **Weather/environment and sky** (`environment.ltx` family, hourly blending, sky shape, fog, ambient,
   `level.env_mod`).
6. **Start position**: the needed parts of `spawns\all.spawn` and `game.graph`, plus the actor camera
   mapping (resolves section 2.5).
7. **Loading screen** (section 2.4 geometry).
8. Optional fidelity: `level.details` (+ `[details]`), glows/lights chunks, `level.hom`, `textures.ltx`
   detail textures, `level.ps_static` + `particles.xr`.

## 9. Unestablished (summary)

- Camera heading/pitch from the actor's stored angles; eye height; coordinate handedness.
- Meaning of: `level` header second u16; `level.geom(x)` header second u16; light-record leading u32;
  `level.env_mod` fields after the radius; `shaders.xr` chunk 1; `all.spawn` header last u32 and chunk 2;
  most of the game-graph vertex record.
- Use of `level_lods_nm.dds`, `*_cube#small.dds`, `level.wallmarks`, `level.graph`, `shaders_xrlc.xr`.
- Visual effect of `renderer_r2a` vs `renderer_r2`.
- Whether the chunk container supports compressed chunks.

## 10. Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), 14 archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrRender_R1.dll`, `bin\xrRender_R2.dll`
  (32-bit Windows).
- Data examined: archive listings merged case-insensitively with later-archive precedence (25,684 file entries; 28,153 including
  directory entries); extracted
  `levels\l01_escape\` (81 files), `config\`, `scripts\`, `spawns\`, `shaders\`, root `*.xr` files,
  `game.graph`, `textures\textures.ltx` and the textures named above; the game's own shader sources
  (`shaders\shared\common.h`, `shaders\r1\lmap.vs`) and scripts were read as data. All sizes, counts, chunk ids, record
  sizes and values were measured on these files (chunk walks, record parsing, cross-checks such as file-size
  equations, root/children reachability, identifier equality, vertex-to-actor distance).
- Binaries: static analysis (Ghidra 12.2, headless) of the four binaries above, limited to: which files and
  path aliases are opened during level load and in which stage, which chunks are required, record sizes
  checked by the loaders, the level-choice rule, renderer selection, texture/model/shader-script lookup
  order, loading-screen elements and stage captions, and the new-game intro trigger. Binary data tables were
  read for constants (renderer value names, loading-screen colour and stage totals, sky shape size). No
  execution or debugging was performed.
- No leaked source code, derived projects, or third-party documentation of the engine were used. Public
  references: Direct3D 9 structure layouts (`D3DVERTEXELEMENT9`, `D3DLIGHT9`), DDS.
