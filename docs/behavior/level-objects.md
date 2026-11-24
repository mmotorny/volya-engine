# Spawned objects of the first level at New Game, drawn at rest

Behaviour-and-format specification of the objects of `spawns\all.spawn` that are part of the first
in-game frame of a New Game on `levels\l01_escape\` with the game's first (static-lighting) renderer:
which of the level's objects are present, where each present object's visual comes from and how its
stored position and angles become its transform, the standalone model files (`.ogf`) these visuals use
(skeleton models, bones, bind pose, skinned vertex formats and how the renderer stores them), how they
are shaded, and the lights of hanging lamps (structure only). Animation, AI and physics simulation are
out of scope; every object is described at rest. What could not be established is marked
**unestablished**; what is inferred rather than read from the program or the data is marked
**believed**.

Builds on (not repeated here):

- `docs/formats/sav.md`: the object records of `spawns\all.spawn` (section 7: spawn packet, object
  state blocks, layouts per class id, scripted additions). Field and block names below ("state flags",
  "Visual block", "health", "Trader-like", the hanging-lamp block, …) are that specification's.
- `docs/behavior/start-position.md`: locating the records in `spawns\all.spawn`, the world coordinate
  system, the actor's record (object index 63).
- `docs/formats/ogf.md`: the visual header (chunk 1), progressive detail windows (section 6).
- `docs/behavior/static-shading.md`: `shaders.xr`, scripted shader definitions, texture lists and lookup,
  weather inputs, fog, render-state vocabulary, dynamic light passes.
- `docs/formats/level.md` (screen thresholds, section 9.4); `docs/formats/level-cform.md` (used only for
  evidence); `docs/behavior/level-loading.md` (game graph, section 5.11).

Conventions as in those specifications: little-endian; `u8`/`u16`/`u32`, `s16`, `f32`; `float3` =
3 × `f32`; `stringZ` zero-terminated Windows-1251; "chunk" = `u32 id`, `u32 size`, payload, found by id;
matrices in the row-vector convention (a point `p` is transformed as `[p 1] · M`; `M[r][c]`, rows and
columns counted from 0); angles in radians; floor(*x*) is the largest integer ≤ *x*; clamp(*x*, *a*, *b*)
limits *x* to *a* … *b*. Configuration sections are LTX sections of the `system.ltx` tree; a record's
"section" is the configuration section named in its spawn packet, its "class id" that section's `class`
value. "The level" is `l01_escape`; "the actor" is the record with section `actor`.

## 1. Overview

1. At a New Game every record of `spawns\all.spawn` becomes an object of the offline simulation
   (sav.md section 10). An object belongs to a level through its game-graph vertex (section 2.1).
2. Objects of the actor's level are switched **online** (given an in-game counterpart that can be
   drawn) by a per-object rule of switch capability and distance to the actor (sections 2.2 – 2.5).
   Only online objects can appear in a frame.
3. An online object's visual is a model file named by its record (section 3), placed by the transform
   built from the record's position and angles (section 3.4).
4. Model files are skeleton models: a list of skinned meshes plus a bone hierarchy (section 4). At rest
   every vertex is drawn at its stored position, transformed by the object's transform (section 4.8).
5. The meshes are drawn with model shaders lit by the weather, a per-object sun and sky visibility
   and optional dynamic lights (section 5). Hanging lamps add lights (section 6).

## 2. Which objects are present

### 2.1 Objects of the level

- An object's **level** is the level of its game-graph vertex: the first `u16` of its object state
  (sav.md 7.4.1, block "Object", field "game-graph vertex") indexes the vertex records of
  `game.graph` (level-loading.md 5.11: 42-byte records, level id `u8` at offset 24), and the level id
  names the level in the graph's level table. Every record of the shipped file has an Object block.
- In the shipped `spawns\all.spawn` 870 of the 8,641 records are on `l01_escape` (table in section 7.1).
  All 870 have parent id 0xFFFF (no container).
- Only objects of the actor's level are candidates for switching online (believed: the switch pass
  walks the objects registered on the actor's level).

### 2.2 Switch distances

From section `[alife]` of `config\alife.ltx` (dbb):

| Key | Value | Use |
|---|---|---|
| `switch_distance` | 150 | *d* |
| `switch_factor` | 0.1 | *f* |

The **online distance** is *d* · (1 − *f*) = **135 m**, the **offline distance** *d* · (1 + *f*) =
**165 m**. The engine's distance is the Euclidean distance in three dimensions between the object's
position and the actor's position (at the start: the positions stored in the spawn packets,
start-position.md section 3.1). The scripts can change the switch distance at run time
(`alife():switch_distance`); none does before the first frame of a New Game (believed: no call is
reachable from the start-game callbacks).

### 2.3 Switch capability of an object

Two predicates, **can switch online** (*on*) and **can switch offline** (*off*), are evaluated per
object. With *F* the object's state flags (`u32`, sav.md 7.4.1) and *m* the object's **renderer match**
(below):

- *on* = *m* and (*F* & 0x02 ≠ 0), for every kind;
- *off* = (not *m*) or (*F* & 0x04 ≠ 0), except:
  - for the kinds zone, anomaly, visual anomaly, torrid zone, level changer, restrictor, smart zone,
    team base, breakable and climbable (sav.md 7.4.2): *off* = false;
  - for creatures (player, stalker, monster, crow, phantom, monster group): *off* = ((not *m*) or
    (*F* & 0x04 ≠ 0)) and health > 0 (health from the Creature block). Dead creatures never switch
    offline;
  - for ammunition (`AMMO`, `A_*`): *off* = ((not *m*) or (*F* & 0x04 ≠ 0)) and rounds left ≠ 0
    (the state's `u16` rounds left; 10 or 20 in every ammunition record of the level).

The **renderer match** *m* is true for every kind except the hanging lamp (`O_HLAMP`), for which it is
true only if the lamp's light flags (hanging-lamp block, `u16`) have bit 0x04 set and the first renderer
is in use (bit 0x08 plays the same role for the second renderer). A lamp with neither bit set is a fatal
error ("no renderer type set for hanging-lamp"). So a lamp marked only for the second renderer never
comes online with the first renderer.

Scripted kinds (class ids marked † in sav.md 7.4.2) can replace both predicates from their scripts;
section 2.5 lists the replacements that exist in the shipped scripts.

### 2.4 The rule

For each object of the actor's level, whenever the simulation evaluates it:

- **offline** object: if *on* is false, it stays offline. Otherwise, if *off* is false it switches
  online regardless of distance; otherwise it switches online when its distance to the actor is
  ≤ 135 m.
- **online** object: if *off* is false, it stays online. Otherwise, if *on* is false it switches
  offline; otherwise it switches offline when its distance to the actor is > 165 m.

All objects start offline at a New Game. The simulation spreads the evaluation over updates, limited by
the `[alife]` keys `process_time` (900 µs) and `objects_per_update` (10); how the two limits combine is
**unestablished**. The level's objects are believed to have all been evaluated once during the 30 frames
in which the loading image is still shown (level-loading.md 2.3), so the first visible frame shows the
result of one evaluation at the start positions: an object is present if *on* and (not *off* or
distance ≤ 135 m).

### 2.5 Scripted replacements (shipped scripts)

`scripts\class_registrator.script` binds scripted class ids to classes in `scripts\se_*.script`. The
following replace *on* and/or *off* (static reading of the scripts). Distances computed by these scripts
are measured from the position of the object's level-graph node (`level.vertex_position` of the
Object block's level-graph node) to the actor's current position, not from the record's position.

| Class ids | Script | Replacement |
|---|---|---|
| `AI_STL_S` | `se_stalker.script` | *off*: if the object's job-online value is set (true, false or a condition list), *off* = not that value; else the engine's *off*. *on*: false if the object's `dont_spawn_online` field is true (set by no shipped script); else the job-online value if set; else, if the record's custom data has no `[spawner]` section, the engine's *on*; else (key `cond` of `[spawner]`): if the actor exists and is dead, the current online state; while offline, (the condition list holds) and the engine's *on*; while online, the condition list holds |
| `SM_*` | `se_monster.script` | *off* as for `AI_STL_S`. *on*: the job-online value if set; else, without a `[spawner]` section, the engine's *on*; else, while offline: false if the actor does not exist or is dead; false if the spawner sets `check_distance = true` and the distance is < *r*; otherwise (the `[spawner]` condition holds) and the engine's *on*. While online: true if the actor does not exist or is dead, or if the condition holds; false if `check_distance` is set and the distance is > *r*; else the engine's *on*. *r* is the spawner's `min_distance` if given, else 150. No monster record of the level sets `check_distance` |
| `SCRPTART` | `se_artefact.script` | for sections `af_electra_flash`, `af_electra_moonlight`, `af_electra_sparkler` only (others: engine predicates): *on* = the current online state if the actor does not exist or is dead; true if the artefact has a parent; for an offline artefact: true only if the game hour is 0 … 4 (`utils.electro_art_enabled`) and the distance is at least 50 m; for an online one: true if the hour is 0 … 4 or the distance is < 50 m |
| `SCRPTCAR` | `se_car.script` | *on*: the engine's *on* without a `[spawner]` section, else the `[spawner]` condition |

**State at the start of a New Game** (believed, from the scripts and the records): the job-online
value of every stalker and monster record of the level is "not set" (the job-online byte, sav.md 7.5,
is 2 in all of them). Every `[spawner]` section of the level's records (10 stalkers, 13 monsters) has a
`cond` that requires an information portion (`{+escape_stalker_help}`, `{+esc_fanat_spawn}`,
`{+escape_lager_spawn_killers}`, `{+tutorial_wounded_start}`, `{+escape_stalker_dog_spawn}`,
`{+esc_boar_dogs_restrictor}`, `{+esc_dog_spawn}`); the actor has none of them at the start (they are
given later by dialogs and scripts), so all 23 are absent. The start time is 05:30 (level-loading.md
2.6), so the three electra artefacts of the level are absent.

Smart terrains (`SMRTTRRN`) set a character's job-online value when the character **begins** a job
(`scripts\xr_gulag.script`: an offline character begins when it is at its job's graph point); jobs of
the level's smart terrains are defined with `online = true` (e.g. `scripts\gulag_escape.script`).
Whether any character begins such a job before the first frame, which would bring it online regardless
of distance, is **unestablished**; the counts in section 7.1 assume none does.

### 2.6 Result for `l01_escape`

Applying sections 2.1 – 2.5 to the 870 records of the level gives 671 online objects; per class id in
section 7.1. Of the kinds with a model (section 3): 69 `O_PHYS_S` and 127 `P_DSTRBL` physics objects,
49 breakables, 5 hanging lamps, 5 inventory boxes, 7 items (1 `AMMO`, 1 `ARTEFACT`, 3 `II_BOTTL`,
2 `II_FOOD`), 0 weapons, 0 outfits, 32 stalkers, 1 trader, 1 monster, 55 crows, 1 helicopter.

Characteristic cases: all physics objects, breakables, crows, the helicopter and the 5 online lamps are
online at any distance because their state flags lack bit 0x04 or their kind never switches offline;
the 5 inventory boxes and 7 items are online by distance (≤ 135 m); the 16 soldiers of the checkpoint
and the special-forces group (state flags 0xFFFFFFBB) are online at any distance; the 8 dead stalker
bodies (health 0) are online at any distance; the 8 camp stalkers within 135 m are online by distance;
of the 27 hanging lamps only the 5 with light-flags bit 0x04 come online.

## 3. Where each present object's visual comes from

### 3.1 Looking up a visual name

A visual name *n* (e.g. `physics\door\door_trader`, `meshes\brkbl#0`) names a model file. Let *n*′ be
*n* with `.ogf` appended if *n* contains no `.`, else *n*. The file is the first that exists of: *n*
itself (as given), `$level$` + *n*′ (the level directory `levels\l01_escape\`), `$game_meshes$` + *n*′
(`meshes\`, LTX specification section 10). Not found anywhere is fatal ("Can't find model file
'%s'."). Lookup is case-insensitive (the archive tree is; record names such as `Weapons\vodka\vodka`
occur).

Of the 5,373 records of all levels with a non-empty visual name, 406 resolve to their level directory
(the breakables' `meshes\brkbl#*` of each level) and 4,967 to `meshes\`; all resolve; they name 819
distinct files (section 7.3).

### 3.2 Source of the name

The visual name is decided when the simulation object is created from its record; the online
counterpart takes the simulation object's name (for every kind, alive or dead at a New Game; item 4):

1. The configuration section's `visual` key, if present, is set first (its extension, if any, removed
   and the name lower-cased).
2. The object state is then read in the order of its blocks (sav.md 7.4.2). A Visual block replaces
   the name with the record's visual name (used as stored).
3. A **Trader-like** block selects the object's specific character when it is read: if its "specific
   character" field is empty, one is chosen from the "character profile", which names a `<character>`
   of `config\gameplay\npc_profile.xml` (the profile's own `<specific_character>` if it names one, else
   a random choice among the `<specific_character>` descriptions of the files listed by
   `[profiles] specific_characters_files`, in `config\gameplay\`, whose `<class>` matches the profile's
   and that are not marked `no_random`). If the chosen description has a non-empty `<visual>`, that
   visual (extension removed, lower-cased) replaces the name at that moment. Consequences:
   - **trader** (`AI_TRD_S`, layout Object, Visual, Trader-like) and **player** (Creature, then
     Trader-like): the description's visual wins over the record's;
   - **stalker** (`AI_STL_S`, layout Trader-like first, then Monster-like with its Visual block): the
     record's Visual block is read after the selection and **restores the record's visual name**. A
     stalker shows its record's visual whatever its profile (its specific character still decides
     other properties, outside this specification).
4. **Dead creatures** (stalkers and monsters with health 0) show the same visual name. The section's
   `corpse_visual` is used instead only for a body whose death time is non-zero and lies more than
   `[monsters_common] corpse_remove_game_time_interval` game hours in the past; every dead body of the
   shipped file has death time 0.
5. An empty visual name means no model (245 hanging-lamp records of all levels, all of section
   `lights_hanging_lamp`; the 11 of this level are marked for the second renderer only and are not
   online with the first renderer).

On the level this gives, for every online object, the record's visual name; for the trader, whose
profile `escape_trader` names the description `escape_trader`, the description's `<visual>`
`actors\trader\trader` equals the record's.

The visual flags byte of the Visual block (bit 0 set in 15 records of all levels) is copied to the
in-game object; its effect on drawing is **unestablished**.

### 3.3 Kinds without a model

Anomalies (`Z_MBALD`, `ZS_MBALD`, `ZS_MINCE`, `Z_RADIO`), restrictors, smart terrains, the level changer
and climbable objects (`O_CLMBL`, ladders: Shape block only) have no Visual block and draw no model.
Anomalies show particle effects named by their sections (`idle_particles`, …), out of scope. The actor's
own body is not drawn in the first-person view (believed).

### 3.4 Transform

The object's transform *X* (model space to world) is built from the spawn packet's position **p** and
angles (*a*₀, *a*₁, *a*₂) (start-position.md section 3.1). With

    sx = sin a0, cx = cos a0,  sy = sin a1, cy = cos a1,  sz = sin a2, cz = cos a2,

the rotation rows are

    R[0] = ( cz·cy − sz·sx·sy,   −sz·cx,   sz·sx·cy + cz·sy )
    R[1] = ( cz·sx·sy + sz·cy,    cz·cx,   sz·sy − cz·sx·cy )
    R[2] = ( −cx·sy,               sx,     cx·cy            )

and *X* = [[R[0], 0], [R[1], 0], [R[2], 0], [**p**, 1]]: a model-space point **v** goes to world as
**v** · R + **p**. Model space uses the world's axes and units (start-position.md section 1). This is
the transform at spawn for every kind with a model. (An item whose update packet carries a physics
state, Item update `f` ≠ 0 in sav.md 7.4.1, is believed to take its position and orientation from that
state instead; none of the level's online items has one.)

Evidence (section 7.2): with this rotation the lowest transformed vertex of the level's objects lies on
the `level.cform` surface below it more often than with any other order or sign of the three
elementary rotations.

## 4. Model files (`.ogf`)

### 4.1 Visual types

A model file is a chunk sequence in the visual structure of `ogf.md` (header chunk 1, 44 bytes, version
4; type in byte 1). Each chunk id occurs at most once per level of nesting. Types in the game data (all
1,581 `.ogf` files of the archives, merged view):

| Type | Name used here | Top-level chunks | Files | Referenced by records |
|---|---|---|---|---|
| 10 | skeleton model | 1, 9, 13, 16, 18, optional 17 | 1,249 | 664 |
| 3 | animated skeleton model | 1, 9, 13, 16, 18, optional 17, and either 19 or both 14 and 15 | 330 | 155 |
| 1 | group | 1, 9, 18 | 1 | 0 |
| 0 | mesh | 1, 2, 3, 4 | 1 | 0 |

Child meshes (inside chunk 9):

| Type | Name used here | Chunks | Vertex formats |
|---|---|---|---|
| 5 | skinned mesh | 1, 2, 3, 4 | 1-link, 2-link |
| 4 | skinned progressive mesh | 1, 2, 3, 4, 6 | 1-link, 2-link |
| 0 | mesh | 1, 2, 3, 4 | fixed layout 0x112 (only in the type-1 file) |

Visual types 3, 4, 5 and 10 are what these files add to `ogf.md`. The header's shader index (bytes
2–3) is 0 in every model file and is not used: standalone files name their shader in chunk 2. The
type-1 and top-level type-0 files are referenced by no record and are not needed.

### 4.2 Header boxes

Boxes and spheres are in model space (bind pose). In all 2,480 child meshes of all files the header box
is exactly the box of the mesh's vertex positions, and the header sphere contains every position (within
2e−7). The header box of a skeleton model contains the boxes of all its children.

### 4.3 Skeleton models (types 10 and 3)

| Chunk | Content |
|---|---|
| 1 | header |
| 9 | child meshes: sub-chunks with ids 0, 1, 2, … in order, each a complete visual (section 4.5) |
| 13 | bone names and hierarchy (section 4.4) |
| 16 | per-bone physics and bind data (section 4.4) |
| 17 | user data: LTX text (e.g. `[disable]`, `[icon]`); not needed for drawing |
| 18 | description (source file name, author, times); not needed |
| 19 | (type 3) comma-separated list of motion files; animation, not needed |
| 14, 15 | (type 3) embedded motions and motion parameters; animation, not needed |

A type-3 model draws exactly like a type-10 model at rest (section 4.8).

### 4.4 Bones

**Chunk 13**: `u32` bone count *n*, then *n* records:

| Field | Type |
|---|---|
| bone name | `stringZ` |
| parent name | `stringZ` (empty for the root) |
| bounding box | 15 × `f32`: 3 × 3 rotation (rows), `float3` centre, `float3` half-size, in the bone's own bind space |

Bone index = position in this list (0-based). Every name in the data is lower-case. Exactly one bone has
an empty parent name (the root); every other parent name names a bone of the list. Bone counts: 1 … 61
in all files.

**Chunk 16**: one record per bone, in the order of chunk 13:

| Field | Type | Use here |
|---|---|---|
| version | `u32` | 1 in all files |
| game material | `stringZ` | physics, not needed |
| shape | 112 bytes (`u16` shape type, `u16` flags, a box of 15 `f32`, a sphere of 4 `f32`, a cylinder of 8 `f32`) | physics, not needed |
| joint type | `u32` | physics |
| joint limits | 12 × `f32` | physics |
| spring, damping | 2 × `f32` | physics |
| joint flags | `u32` | physics |
| break force, break torque | 2 × `f32` | physics |
| friction | `f32` | present only if version ≠ 0 |
| **bind rotation** | `float3` (*r*₀, *r*₁, *r*₂) | bind pose |
| **bind position** | `float3` **t** | bind pose |
| mass | `f32` | physics |
| centre of mass | `float3` | physics |

Parsing every chunk 16 of every file with this layout consumes it exactly.

**Bind pose.** Each bone's local bind matrix is *L* = [[R(−*r*₀, −*r*₁, −*r*₂), 0], [**t**, 1]] with
R the rotation of section 3.4 evaluated at the **negated** angles (sx = sin(−*r*₀), …). The bone's
model-space bind matrix is *B* = *L* · *B*(parent) (row-vector convention: the child's local matrix
first), *B*(root) = *L*(root). Evidence: for the 51 bones of `actors\trader\trader.ogf` that own at
least 5 vertices (1-link: its bone; 2-link: *b*₁ if *w* ≥ 0.5, else *b*₀), the centre of the chunk-13
box transformed by *B* lies on average 0.035 m (max 0.158 m) from the mean of the bone's vertices; with
non-negated angles or the other multiplication order the averages are 1.04 … 1.58 m. The bind pose is
not needed to draw at rest (section 4.8).

### 4.5 Child meshes

| Chunk | Content |
|---|---|
| 1 | header (type 5, 4 or 0) |
| 2 | two `stringZ`: texture name, then engine shader name (e.g. `wood\wood_walls8`, `models\model`) |
| 3 | vertices: `u32` format, `u32` count *vc*, then *vc* records (section 4.6) |
| 4 | indices: `u32` count *ic* (a multiple of 3), then *ic* × `u16`; a triangle list |
| 6 | (type 4 only) detail windows, in the layout of `ogf.md` section 6 (4 reserved `u32` = 0, `u32` *n*, *n* × {`u32` offset, `u16` triangles, `u16` vertices}; offset counted in indices) |

Indices are 0-based into this mesh's own vertices. In every child mesh of every file every index is
< *vc*, the smallest index is 0, and for type 4 the windows satisfy every statement of `ogf.md`
section 6 (level 0 ends at *ic* and uses all *vc* vertices; triangles and vertices never increase;
every window lies inside the index list and every index of a window is below its vertex count).

### 4.6 Vertex formats

The vertex formats of a skinned mesh (types 4 and 5) are exactly the two below; any other value there
is fatal ("Invalid vertex type in skinned model '%s'"). Format 0x112 occurs only in type-0 meshes.

**1-link (format 0x12071980)**, 60 bytes:

| Offset | Type | Meaning |
|---|---|---|
| 0 | `float3` | position **P** (model space, bind pose) |
| 12 | `float3` | normal **N** |
| 24 | `float3` | tangent |
| 36 | `float3` | binormal |
| 48 | 2 × `f32` | texture coordinate (u, v) |
| 56 | `u32` | bone index *b* (into chunk 13 of the containing model) |

**2-link (format 0x240E3300)**, 64 bytes:

| Offset | Type | Meaning |
|---|---|---|
| 0 | `u16` | bone index *b*₀ |
| 2 | `u16` | bone index *b*₁ |
| 4 | `float3` | position **P** |
| 16 | `float3` | normal **N** |
| 28 | `float3` | tangent |
| 40 | `float3` | binormal |
| 52 | `f32` | weight *w* of *b*₁ (0 … 1) |
| 56 | 2 × `f32` | texture coordinate (u, v) |

**Fixed layout (format 0x112)**, 32 bytes: `float3` position, `float3` normal, 2 × `f32` texture
coordinate.

Texture coordinates address the texture with the Direct3D convention (v grows downward), as for the
level's surfaces. Tangent and binormal are not used by the first renderer's model programs. Normals: in
the referenced files 931,504 of 931,510 have length 1 within 0.01 and 6 have length 0.

Bone indices are < the model's bone count in every vertex of every file; all 2-link weights lie in
0 … 1 (referenced files: 307,655 vertices with *w* = 0, 123 with *w* = 1, 281,684 in between).

### 4.7 How the first renderer stores skinned vertices

When a skinned mesh is loaded, its vertices are converted for drawing in one of these modes:

- **single bone**: a 1-link mesh whose vertices all name the same bone;
- **hardware 1-link**: other 1-link meshes;
- **hardware 2-link**: 2-link meshes;
- **software**: instead of either hardware mode, a mesh whose largest bone index exceeds (*K* − 22) / 3
  (integer division), *K* the number of vertex shader constants the graphics device reports; the
  vertices stay as floats and are skinned on the CPU. With *K* = 256 (believed: any device the renderer
  runs its programs on) the limit is 78 and no shipped model, at most 61 bones, uses this mode.

In the three hardware modes every vertex is stored quantized; the drawn values are:

| Value | Stored as | Drawn value |
|---|---|---|
| position component *p* | `s16` *q* = clamp(floor(*p* · 2730.5833), −32768, 32767) | *q* · 12 / 32768 |
| normal (also tangent, binormal), first normalised if its squared length exceeds 1.1754944e−38 | per component `u8` *c* = clamp(floor((*n* + 1) · 127.5), 0, 255) | 2 · *c* / 255 − 1 |
| texture coordinate *t* | `s16` *q* = clamp(floor(*t* · 2047.9375), −32768, 32767) | *q* · 16 / 32768 |
| 2-link weight *w* | `u8` *c* = clamp(floor(*w* · 255 + 0.5), 0, 255) | *c* / 255 |

(2730.5833 = 32767 / 12 and 2047.9375 = 32767 / 16, as stored in single precision.) So positions are
drawn within ±12 m of the model origin and texture coordinates within ±16; a component outside is
clamped. Among the referenced files only `meshes\equipments\projector.ogf` (positions up to 16.87 m) and
`meshes\physics\anomaly\artefact_ameba2.ogf` (a texture coordinate of 16.08) exceed these ranges; neither
is used on this level. The vertex programs read the decoded values; a zero normal is drawn as the
direction of (−1, −1, −1) after normalisation. The component order inside the stored normal is believed
to be x, y, z as decoded (the device decodes the packed value as a colour, red = x).

### 4.8 Skinning and the pose at rest

Each bone has a **render matrix** *K* = *B*⁻¹ · *T* where *T* is the bone's current model-space matrix
(row-vector convention: the bind matrix's inverse first). A 1-link vertex is drawn at **P** · *K*[*b*]
(normal **N** · rotation part of *K*[*b*]); a 2-link vertex with the blended matrix
(1 − *w*) · *K*[*b*₀] + *w* · *K*[*b*₁] (the matrices are blended, not the transformed points; element 1
of section 5.3 uses *K*[*b*₀] alone for 2-link vertices). Then the object's transform *X* (section 3.4)
applies.

**At rest** every bone's current matrix equals its bind matrix (*T* = *B*), so every render matrix is the
identity and **every vertex is drawn at its stored position**: world = **P** · *X* (with the precision of
section 4.7). The bone data (chunks 13, 16) is therefore not needed to draw a model at rest.

What the running game shows beyond this is out of scope and differs from the rest pose: characters,
monsters and crows play motions from the first frame; physics objects settle under the physics
simulation; hanging lamps play the motion `idle` if their model has one, and objects with a Skeleton
block may play its startup animation (`$editor` means none).

### 4.9 Detail level of skinned progressive meshes

A type-4 mesh is drawn as one window of section 4.5 (chunk 6), selected like `ogf.md` section 6 from a
detail value. Which detail value the first renderer uses for objects is **unestablished**; level 0 (all
triangles of the window ending at *ic*) is the full-detail mesh.

## 5. Shading

### 5.1 What is drawn

Each child mesh of an online object's model is drawn with its own shader (chunk 2). Very small objects
are believed to be skipped by the same projected-size threshold *T*\_min as static meshes (`level.md`
section 9.4); the exact test for objects is **unestablished**.

### 5.2 Shader resolution

The chunk-2 pair (texture *t*, engine shader *s*) is resolved exactly like an entry `<s>/<t>` of the
level's shader table (static-shading.md sections 1 – 4): a scripted definition `shaders\r1\<s with \
→ _>.s` if it defines `normal` or `l_special`, else the `shaders.xr` definition named *s*; texture
list T[0] = *t*; texture lookup as static-shading.md 4.2. The model definitions use two library classes
not covered by static-shading.md. Their sampler `s_base` takes T[0] by position, as static-shading.md
4.1 states for library definitions, whatever the definition's own `Base Texture` name is (e.g.
`models\antigas_glass` names `act\act_glass_universal`; its meshes still sample T[0]).

**Class `MODEL   `** (properties after the common block: boolean `Use alpha-channel` *α*, integer
`Alpha ref` *ref*):

| Element | Programs (vertex / pixel) | Samplers | States |
|---|---|---|---|
| 0 | `model_def_hq` / `model_def_hq` | `s_base` = T[0], `s_lmap` = the projected light texture (section 5.5) | fog on; depth test and write on; if *α*: blending source-alpha / inverse-source-alpha and alpha test with *ref*, else no blending, no alpha test |
| 1 | `model_def_lq` / `model_def_lq` | `s_base` = T[0] | as element 0 |
| 2, 3 | `model_def_point` / `add_point`, `model_def_spot` / `add_spot` | `s_base`, attenuation textures | dynamic light passes (section 5.6) |

**Class `MODELEbB`** (environment-mapped; properties: texture `Name` = cube environment map *E*,
transform, boolean `Alpha-Blend` *β*):

| Element | Programs | Samplers | States |
|---|---|---|---|
| 0 | `model_env_hq` / `model_env_hq` | `s_base` = T[0], `s_env` = *E* (clamp), `s_lmap` = projected light texture | fog on; depth test on; depth write = not *β*; if *β*: blending source-alpha / inverse-source-alpha with alpha test ref 0 |
| 1 | `model_env_lq` / `model_env_lq` | `s_base`, `s_env` | as element 0 |
| 2, 3 | as `MODEL   ` | | |

Definitions of `shaders.xr` used by the referenced files: `models\model` (`MODEL`, *α* off, ref 32),
`models\model_aref` (*α* on, ref 128), `models\model_fur` (*α* on, 32), `models\transparent` (*α* on,
ref 0, priority 2, strict sorting); `models\weapons`, `models\artefact`, `models\artefact2`,
`models\antigas_glass`, `models\mirror`, `models\water` (`MODELEbB`, *β* off) and `models\window`
(`MODELEbB`, *β* on); *E* is the definition's own cube map, e.g. `sky\sky_cube_weapons` for
`models\weapons` and `models\window`. Scripted (`shaders\r1\models_*.s`):
`models\selflight`, `models\selflightl` (`model_def_lqs` / `simple`: colour = base texel, fog on, no
lighting), `models\lightplanes` (`model_def_lplanes` / `base_lplanes`: additive source-alpha / one, depth
write off, fade by |view direction · normal| as in static-shading.md 8.2), `models\xwindows`,
`models\pautina`, `models\xdistort`, `models\xdistortcolor`, `models\xdistortcolorl`, `models\xanomaly`, `models\xmonolith` (effects
with distortion or blending, drawn with the scripts' passes; their appearance is outside this
specification).

### 5.3 Element choice

An object's meshes are drawn with element 0 if the object is a **projected-light receiver** in the
frame, else element 1. An object is a receiver when it has a model, |camera − *C*| + *R* < 50 · *R*
(*C*, *R* the model's header sphere centre transformed by *X*, and its radius) and fewer than 100
receivers have been taken in the frame (the order in which objects are tested is **unestablished**).

### 5.4 Per-object light inputs

Every object with a model keeps two visibility values, updated at most once per frame:

- **sky visibility** *h*: from the point **o** = *C* + (0, 0.3 *R*, 0), rays of length 50 m are cast in
  these fixed directions (normalised before use), in this order: (0, 1, 0), (0.52573, 0.85065, 0),
  (0.16246, 0.85065, 0.5), (−0.42533, 0.85065, 0.30902), (−0.42533, 0.85065, −0.30902), (0.16246,
  0.85065, −0.5), (0.89443, 0.44721, 0), (0.27639, 0.44721, 0.85065), (−0.72361, 0.44721, 0.52573),
  (−0.72361, 0.44721, −0.52573), (0.27639, 0.44721, −0.85065), (0.68819, 0.52573, 0.5), (−0.26287,
  0.52573, 0.80902), (−0.85065, 0.52573, 0), (−0.26287, 0.52573, −0.80902), (0.68819, 0.52573, −0.5),
  (0.95106, 0, 0.30902), (0.58779, 0, 0.80902), (0, 0, 1), (−0.58779, 0, 0.80902), (−0.95106, 0,
  0.30902), (−0.95106, 0, −0.30902), (−0.58779, 0, −0.80902), (0, 0, −1), (0.58779, 0, −0.80902),
  (0.95106, 0, −0.30902) (26 directions). Five directions are tested per update, cycling through the
  26 in the listed order. A ray tests against the static collision (believed: `level.cform`); its result
  is "free" when it hits nothing. The target value is the fraction of free results among the tested
  directions (once all 26 have been tested, among the 26 latest results).
- **sun visibility** *s*: a ray of length 500 m from **o** towards the sun (direction −L_sun_dir,
  L_sun_dir as static-shading.md section 9), tested against the static collision and other objects
  (believed: the object itself excluded); target 1 if it hits nothing, else 0; tested again after a
  random 6 … 12 updates.

Both values follow their targets once per frame: *v* ← *k* · target + (1 − *k*) · *v*, *k* =
clamp(frame time in s, 0, 1); initial values *h* = 0.5, *s* = 0.2, except that *h* is set to the target
directly at the first update. Converged values (a still frame long after spawn): *h* = fraction of free
directions, *s* = 0 or 1. The values at the first visible frame depend on frame times
(**unestablished**).

The shader constant **L_dynamic_props** = (0.5 *s*, 0.5 *s*, 0.5 *s*, 0.5 *h*).

### 5.5 Colour equations

With **n** = normalize(**N** transformed by the object's rotation), the weather inputs of
static-shading.md section 9 (L_ambient, L_hemi_color, L_sun_color, L_sun_dir) and the game's programs
`shaders\r1\model_def_*.vs/.ps`, `model_env_*`, `common.h`:

- sun term S(**n**) = L_sun_color · max(0, **n** · (−L_sun_dir));
- **model light** *Q* = (**n**.y · 0.5 + 0.5) · L_dynamic_props.w · L_hemi_color.rgb + L_ambient.rgb +
  L_dynamic_props.xyz · S(**n**); a vertex-stage colour output, so the precision rules of
  static-shading.md section 6 apply.

Element 1 (`model_def_lq`): C = 2 · *Q* · B.rgb, A = B.a (B = base texel at (u, v)). Environment-mapped
(`model_env_lq`): base = lerp(E, B.rgb, B.a) with E the cube texel at the reflection of the view
direction (`calc_reflection` of `common.h`) about **n**; C = 2 · *Q* · base, A = B.a.

Element 0 (`model_def_hq`): with *t* the texel of the projected light texture at the object's projected
coordinate, light = lerp(*t*.rgb + S(**n**) · *t*.a, *Q*, φ), C = 2 · light · B.rgb, A = B.a (env variant:
base as above). The fade φ = c + (1 − c) · *g*², *g* = (|camera − *C*′| + *R*) / (50 *R*), *C*′ the
receiver's recorded centre (believed: *C*), c = console value `r1_lmodel_lerp` (range 0 … 0.333;
0.1 in the shipped `user.ltx` and by default). How the projected light texture (a 512 × 512 render
target `$user$projector`) is filled is **unestablished**; a renderer without it can draw every object
with element 1.

Fog, alpha test, blending and drawing order follow static-shading.md sections 9.3 and 10 (objects
with priority 2 or strict sorting are drawn with the sorted passes).

### 5.6 Dynamic lights

With `r1_dlights on` (default) the level's point lights and the hanging lamps' lights (section 6) are
added to models with elements 2 and 3 (static-shading.md section 12 gives the pass equation). Which
lights are applied to which object is **unestablished**; a first static render may omit them.

## 6. Hanging lamps' lights (structure)

A hanging lamp (`O_HLAMP`) record holds, after Object, Visual and Skeleton blocks, the hanging-lamp block
of sav.md 7.4.2. Its fields map to the lamp as follows (light flags *L* = the block's `u16`):

| *L* bit | Meaning |
|---|---|
| 0x01 | physics shell (a lamp with it and no visual logs "flag physics set, but has no visual") |
| 0x02 | the main light casts shadows |
| 0x04 / 0x08 | lamp exists for the first / second renderer (section 2.3) |
| 0x10 | main light is a spot light (cone); clear: point light |
| 0x20 | an additional ambient light exists |

Other bits are set in the data (values on the level: 0x2A, 0x38, 0x3A, 0xB55C, 0xB57C, 0xB57D) with
**unestablished** meaning.

- **Main light**: type spot or point (bit 0x10); colour = (R, G, B) / 255 × brightness, R = bits 16–23,
  G = bits 8–15, B = bits 0–7 of the colour `u32`; range = range; cone angle = spot cone angle (radians);
  projected texture = light texture (empty for most lamps); colour animator = the colour animator name
  (an entry of `lanims.xr`, e.g. `light 40 hz`; animation over time, out of scope).
- **Placement**: each frame the light takes the world matrix *M* = (light bone's current model matrix)
  · *X* (or *X* if the light bone name is not a bone of the visual): its direction is the third row
  (row 2) and its right vector the first row (row 0); its position is believed to be the translation row
  (row 3). At rest the light bone's matrix is its bind matrix (section 4.4).
- **Glow**: if the glow texture is non-empty, a glow with that texture and the glow radius, at the
  light's position (glow drawing as level glows, `level.md` section 8; **unestablished** in detail).
- **Ambient light** (bit 0x20): a point light without shadows, colour = main colour × ambient power,
  range = ambient radius, texture = ambient texture, placed at the ambient bone (empty name in old
  records = the light bone).
- **On/off**: the lights are on only if the lamp's health (block field) > 0.
- The visual is a skeleton model; it plays the motion `idle` if it has one (out of scope, section 4.8).

The 5 lamps online on the level are 3 lamps `physics\light\lampa_01_on` (`trader_light_0000` at 5.7 m,
`trader_light_0003` at 1.5 m, `trader_light_0005` at 241.9 m) and 2 television sets `physics\decor\tv_1`
(`trader_tv_0000` at 2.6 m, `trader_tv_0002` at 286.3 m); the other 22 lamps have light flags without
bit 0x04.
All five are spot lights without shadows (bit 0x10 set, 0x02 clear). The three lamps have light flags
0xB55C (no ambient light, bit 0x20 clear); the television sets have 0xB57C and 0xB57D (ambient light;
`trader_tv_0002` also has bit 0x01).

## 7. Verification facts

`spawns\all.spawn` (dbb, 4,474,073 bytes), `game.graph` (dbb), `config\alife.ltx` (dbb), the model files
of the merged archive view (later archives win). Distances are from the actor's stored position.

### 7.1 Objects of `l01_escape` (870 records) and online objects at the start (671)

| Class id | Records | Online |
|---|---|---|
| `AI_CROW` | 55 | 55 |
| `AI_STL_S` | 63 | 32 |
| `AI_TRD_S` | 1 | 1 |
| `AMMO` | 9 | 1 |
| `ARTEFACT` | 3 | 1 |
| `C_HLCP_S` | 1 | 1 |
| `E_STLK` | 2 | 0 |
| `II_ATTCH` | 1 | 0 |
| `II_BANDG` | 4 | 0 |
| `II_BOTTL` | 17 | 3 |
| `II_EXPLO` | 6 | 0 |
| `II_FOOD` | 15 | 2 |
| `II_MEDKI` | 2 | 0 |
| `LVL_CHNG` | 1 | 1 |
| `O_ACTOR` | 1 | 1 |
| `O_BRKBL` | 49 | 49 |
| `O_CLMBL` | 14 | 14 |
| `O_HLAMP` | 27 | 5 |
| `O_INVBOX` | 25 | 5 |
| `O_PHYS_S` | 69 | 69 |
| `P_DSTRBL` | 127 | 127 |
| `RE_SPAWN` | 9 | 9 |
| `SCRPTART` | 3 | 0 |
| `SMRTTRRN` | 41 | 41 |
| `SM_BOARW` | 10 | 0 |
| `SM_DOG_S` | 48 | 0 |
| `SM_FLESH` | 8 | 1 |
| `SM_P_DOG` | 2 | 0 |
| `SPC_RS_S` | 75 | 75 |
| `WP_BM16`, `WP_LR300`, `WP_PM`, `WP_WALTH` | 1 each | 0 |
| `ZS_MBALD` | 96 | 96 |
| `ZS_MINCE` | 4 | 4 |
| `Z_MBALD` | 60 | 60 |
| `Z_RADIO` | 18 | 18 |

Online objects with a model, without the actor: 352 (32 stalkers, the trader and 319 other objects,
including the 55 crows and the monster). The 319 objects other than stalkers and the trader use 101 distinct model files with 163,064 vertices in total and
388,632 indices at full detail (for type-4 meshes the indices of detail level 0, i.e. 3 × its triangle
count; counted once per object).

The 32 online stalkers: `esc_lager1` … `esc_lager7`, `esc_wolf` (distance 25 … 78 m, state flags
0xFFFFFFBF), `esc_specnaz1` … `6`, `esc_blokpost_soldier1` … `9`, `esc_blokpost_commander` (232 … 267 m,
state flags 0xFFFFFFBB), and the bodies `esc_stalker_corpse`, `esc_tutorial_dead_novice`,
`esc_stalker_corps1`, `esc_stalker_corps1_0000`, `esc_stalker_corps1_0001`, `esc_trup_bratka`,
`esc_stalker_0000`, `esc_trup_scene` (health 0, 166 … 764 m). The online monster is `esc_suicidal_flesh`
(state flags 0xFFFFFFF3, 70 m). Absent because of a `[spawner]` section: 10 stalkers (`esc_fox`,
`esc_stalker_fanat`, `esc_stalker`, `esc_killer1` … `esc_killer8` without `esc_killer5`) and 13 monsters.
Visuals of stalkers as drawn (section 3.2: the records'), e.g. `esc_lager1`
`actors\novice\green_stalker_2`, `esc_lager2` `actors\novice\green_stalker_10`, `esc_specnaz1`
`actors\soldier\soldier_antigas`, `esc_blokpost_commander` `actors\soldier\soldier_beret_1`,
`esc_stalker_corps1_0001` `actors\neytral\stalker_neytral_balon_8`.

### 7.2 Named objects

Positions and angles as stored; matrix rows of section 3.4 rounded to 6 decimals; world boxes of the
transformed vertices in single precision, without the quantization of section 4.7.

- **Object 79** `trader_door` (`physic_object`, `O_PHYS_S`, 1.7 m): visual `physics\door\door_trader` →
  `meshes\physics\door\door_trader.ogf` (96,656 bytes); position (−248.3039, −24.6881, −133.7155),
  angles (0, 4.712372, 0); R[0] = (−0.000017, 0, −1), R[2] = (1, 0, −0.000017). Type 10, 3 bones (`link`
  root, `door`, `lock`; bind positions in model space (0, 1.9743, 0), (0, 1.3675, 0), (1.3061, 0.7045, 0)),
  1 child of type 4, 1-link, 1,177 vertices, 12,285 indices, 19 detail levels, texture `door\door_trader`,
  shader `models\weapons`. Sums over its vertices: position (1078.4926, 1716.3511, 25.1468), normal
  (42.6894, −8.6118, 30.0575), (u, v) (583.1458, 295.4061). World box (−248.4411, −24.6881, −135.1301)
  … (−248.0823, −21.9532, −133.6777).
- **Object 89** `trader_light_0003` (`lights_hanging_lamp`, 1.5 m): visual `physics\light\lampa_01_on`
  (54,863 bytes); position (−245.5421, −23.9448, −134.6337), angles (0, 2.134655, 0); type 10, 7 bones;
  2 children of type 5 (1-link): 699 vertices / 2,268 indices (`prop\prop_iten4`, `models\model`) and
  85 / 288 (`prop\prop_iten4`, `models\selflight`). Lamp block: colour 0x00FFFFFF, brightness 0.8, colour
  animator `light 40 hz`, range 2.0, light flags 0xB55C (spot, first and second renderer, no shadows,
  **no** ambient light: bit 0x20 is clear, so the ambient fields below are present but unused; bit 0x40
  and the bits above 0xFF of unestablished meaning), fixed bones `link`, health 100, virtual size 0.1, ambient radius 2.0, ambient power 0.3, ambient texture
  `lights\lights_duolight02grey`, light texture empty, light bone `bone_lamp`, cone 2.094395, glow
  `glow\glow_orange` radius 0.3, ambient bone `bone_lamp`. World box (−245.6193, −23.9447, −134.7003) …
  (−245.3585, −23.5131, −134.4912).
- **Object 0** `meshes\brkbl#0.ogf` (`breakable_object`, `O_BRKBL`, 178.2 m, online): visual
  `meshes\brkbl#0` → `levels\l01_escape\meshes\brkbl#0.ogf` (11,622 bytes); position (−79.4931,
  −3.1619, −76.9341), angles (0.581842, −1.455716, −0.267673); R[0] = (−0.033655, 0.220967, −0.974700),
  R[1] = (−0.556858, 0.805701, 0.201881), R[2] = (0.829926, 0.549564, 0.095932); type 10, 9 bones, 1 child
  (type 5, 134 vertices, 282 indices, `glas\glas_dirt`, `models\window`). World box (−79.5802, −3.2542,
  −77.1837) … (−79.4313, −3.0329, −76.6842).
- **Object 658** `level_prefix_inventory_box_0003` (`O_INVBOX`, 1.5 m): visual
  `physics\equipment_cache\equipment_box_01_case` (2,183 bytes), type 10, 1 bone, 1 child (24 vertices,
  36 indices, `glow\glow_ghost`, `models\model_aref`). World box (−248.0426, −24.7945, −136.1114) …
  (−246.9847, −24.3651, −135.2594).
- **Object 84** `escape_trader` (`m_trader`, `AI_TRD_S`, 2.1 m): profile `escape_trader` names specific
  character `escape_trader` (`no_random`) with `<visual>` `actors\trader\trader` (same as the record);
  file 417,583 bytes, type 3 (chunk 19: `actors\trader_animation`), 55 bones, root `bip01_pelvis`;
  1 child of type 4, 2-link, 3,338 vertices, 87,048 indices, 1,362 detail levels, `act\act_barman`,
  `models\model`; sum of the 2-link weights 670.5939. Bind positions: `bip01_pelvis` (0, 0.8264, −0.1119),
  `bip01_head` (−0.0001, 1.5192, −0.0752). World box (−244.8210, −24.9555, −134.8202) … (−244.4099,
  −23.2295, −133.1037) (its lowest point is 0.0025 m above the stored position).
- **Object 107** `esc_heli` (`C_HLCP_S`, 232.5 m, online): `physics\vehicles\mi24\veh_mi24_u_01`, type 3,
  29 bones, 3 children (type 4, 1-link: 9,833 vertices / 270,786 indices with `models\weapons`, 92 / 192
  and 418 / 5,340 with `models\transparent`). World box (−329.1732, 16.4293, 75.0037) … (−308.8980,
  22.1571, 91.5756).

**Transform evidence** (section 3.4). For each of the level's objects with a model of kinds physics
object, breakable, inventory box, item, ammunition and weapon (online or not), the lowest transformed
vertex is compared with the highest `level.cform` triangle on the vertical through it (surfaces up to
0.1 m above the vertex counted); "fits" = within 0.05 m. With the rotation of section 3.4: 143 of 196
physics objects (all 196 with such a triangle), 39 of 49 breakables (all 49 with one), 16 of 24 inventory
boxes (of the level's 25 boxes, 24 have one) fit (median distance 0.000 m in each
group). Restricted to the 111 objects that are tilted (|*a*₀| or |*a*₂| > 0.05) and have such a
triangle, 83 fit with the rotation of section 3.4; the best of the 47 other combinations of order and
sign of the three elementary rotations fits 80, the transposed rotation 56 (of 105 with a triangle).

### 7.3 Invariants over the model files

Over the 819 files referenced by the records of all levels (section 3.1) — 664 of type 10, 155 of type 3
(117 with chunk 19, 38 with chunks 14 and 15); child meshes: 581 type 5 with 1-link, 31 type 5 with
2-link, 378 type 4 with 1-link, 250 type 4 with 2-link; 931,510 vertices and 16,298,541 indices in
total — and over all 1,581 `.ogf` files of the archives (2,146,420 vertices, 32,591,853 indices):

- every header has version 4 and shader index 0; top-level types and chunk sets are those of section
  4.1; child chunk sets and vertex formats are those of sections 4.1 and 4.6;
- chunk 9's sub-chunk ids are 0, 1, 2, … in order; every child of a type-3/10 model has type 4 or 5;
- chunk 13 has exactly one root and every parent name exists; chunk 16 has one record per bone, version
  1, and is consumed exactly by the layout of section 4.4;
- vertex chunks have size 8 + 60 *vc* (1-link), 8 + 64 *vc* (2-link) or 8 + 32 *vc* (0x112); index
  chunks 4 + 2 *ic*; *ic* a multiple of 3; every index < *vc* and the smallest is 0;
- every bone index of every vertex is below the model's bone count; 2-link weights lie in 0 … 1;
- each child header box equals the box of its stored positions; child boxes lie inside the model's
  header box;
- type-4 windows satisfy `ogf.md` section 6.

## 8. Unestablished

- Whether smart-terrain jobs force characters online before the first frame (2.5), the exact run-time
  script state at the first switch evaluation (believed as stated in 2.5), and how `process_time` and
  `objects_per_update` limit the switch pass (2.4).
- The effect of the visual flags byte (3.2).
- The detail value used for skinned progressive meshes of objects (4.9); the small-object test (5.1).
- The content of the projected light texture (5.5) and the order in which receivers are chosen (5.3).
- Light-track values at the first frame (5.4); which dynamic lights affect which objects (5.6).
- Meaning of the remaining hanging-lamp light-flag bits; glow drawing details (6).
- The poses shown in the running game (motions, physics) — out of scope (4.8).

## 9. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrRender_R1.dll`; default
  `__support\userdocs\user.ltx`.
- Data examined: `spawns\all.spawn`, `game.graph`, the `config\` tree (`system.ltx` includes,
  `alife.ltx`, `config\gameplay\npc_profile.xml`, `character_desc_*.xml`, `info_*.xml`,
  `dialogs_escape.xml`), `scripts\*.script`, `shaders.xr`, `shaders\r1\*` (model programs, `skin.h`,
  `common.h`, `models_*.s`), every `.ogf` file under `meshes\` and `levels\*\meshes\`,
  `levels\l01_escape\level.cform`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation) for the
  switch predicates of every simulation object kind and the switching rule, the renderer match of
  hanging lamps, the order in which object states are read and where the visual name is set
  (configuration default, Visual block, specific character), the body visual of dead creatures, the
  model-file lookup, the object transform, the model-file loader (vertex formats, skinning modes), the
  renderer's conversion of skinned vertices (scale constants, rounding, clamping, vertex declarations),
  the model shader classes, receiver selection and fade, the per-object light tracking (constants and
  direction table read from the binary), and the hanging-lamp light setup; reading of the game's
  scripts and shader programs as data. A throwaway decoder written from this text (spawn records with
  every block layout, game-graph levels, LTX inheritance, model files) reproduces the numbers of
  section 7 over all records and all model files; the bind-pose and transform conventions were checked
  against the data as described in 4.4 and 7.2.
