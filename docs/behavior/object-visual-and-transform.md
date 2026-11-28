# An object's visual and transform

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of where an in-game
object's visual comes from and where it is placed: which visual name an object of `spawns\all.spawn` has
when it comes online, how a visual name finds its model file, which kinds have no model, and how the stored
position and angles become the object's transform. The model files themselves are specified in
`docs/formats/skeleton-ogf.md`; which objects are online in `docs/behavior/offline-simulation.md` section 6.
The transform is the one the object has when it comes online, including the height correction that kinds using
the level graph get then (section 4.1); animation, later movement (AI, physics) and the online/offline switch
itself are out of scope. What could not be established is marked **unestablished**; what is inferred rather than
read from the program or the data is marked **believed**.

Builds on (not repeated here):

- `docs/formats/sav.md`: the object records of `spawns\all.spawn` (section 7: spawn packet, object state
  blocks, layouts per class id). Field and block names below ("Visual block", "Trader-like", "health", …) are
  that specification's.
- `docs/formats/sav.md`: locating the records (section 10.1), the spawn packet's position and angles
  (section 7.2).
- `docs/behavior/world-coordinates.md`: the world coordinate system (section 1).
- `docs/formats/ltx-config.md` (section 10: the path aliases `$level$` and `$game_meshes$`).
- `docs/behavior/model-shading.md` section 2: the pose at rest (every vertex drawn at its stored position
  transformed by *X*), used for the world boxes of section 5.
- `docs/behavior/offline-simulation.md` (cited as `offsim`): `level.ai` (section 3: header, the 21-byte level
  vertex, its position, column, row and cell), the cross table (section 2), the Object block's game vertex and
  level vertex (section 5.1), the test "position off the level's grid" (section 5.2).
- `docs/behavior/script-api-level.md` section 3.1: the level vertex's links (bytes 0–11) and that bytes 14–15
  hold a packed plane (its unpacking is section 4.2 here).
- `docs/formats/ogf.md` section 2: the model file header's bounding sphere.

Conventions: little-endian; matrices in the row-vector convention (a point `p` is transformed as
`[p 1] · M`; `M[r][c]`, rows and columns counted from 0); angles in radians. A record's "section" is the
configuration section named in its spawn packet, its "class id" that section's `class` value. "The actor" is
the record with section `actor`.

**Checks.** Bracketed ids [OV-*n*] name the checks of the dirty workspace's
`checks/level-objects-split/run_all.py` (decoders `sp.py`, `refs.py`, `chars.py`, `named.py`, `xform.py`,
`orders.py` beside it; OV-14 runs checks of `checks/character-identity/checks.py`). Bracketed ids [CP-*n*] name the checks of
`checks/creature-spawn-position/run_all.py` (helpers `trader.py`, `f32calc.py`, `clsids.py`, `slots_all.py`
beside it).

## 1. Looking up a visual name

A visual name *n* (e.g. `physics\door\door_trader`, `meshes\brkbl#0`) names a model file. Let *n*′ be *n*
with `.ogf` appended if *n* contains no `.`, else *n*. The file is the first that exists of: *n* itself (as
given), `$level$` + *n*′ (the current level's directory, e.g. `levels\l01_escape\`), `$game_meshes$` + *n*′
(`meshes\`). Not found anywhere is fatal ("Can't find model file '%s'.") [OV-3]. Lookup is case-insensitive
(the archive tree is; record names such as `Weapons\vodka\vodka` occur).

Of the 5,373 records of all levels with a non-empty visual name, 406 resolve to their level directory (the
breakables' `meshes\brkbl#*` of each level) and 4,967 to `meshes\`; all resolve; they name 819 distinct files
[OV-1].

## 2. Source of the name

The visual name is decided when the simulation object is created from its record; the online counterpart
takes the simulation object's name (for every kind, alive or dead at a New Game; item 4):

1. The configuration section's `visual` key, if present, is set first (its extension, if any, removed and the
   name lower-cased).
2. The object state is then read in the order of its blocks (sav.md 7.4.2). A Visual block replaces the name
   with the record's visual name (used as stored).
3. A **Trader-like** block selects the object's specific character when it is read, and setting a
   specific character replaces the name with that specific character's non-empty `<visual>` (extension
   removed, lower-cased); the choice and the set are `docs/behavior/character-identity.md` (cited as
   `identity`), sections 2 to 4. Consequences:
   - **trader** (`AI_TRD_S`, layout Object, Visual, Trader-like) and **player** (Creature, then Trader-like):
     the specific character's visual wins over the record's;
   - **stalker** (`AI_STL_S`, layout Trader-like first, then Monster-like with its Visual block): the record's
     Visual block is read after the selection and restores the record's visual name; but creating the object
     then runs its supplies step, which sets a specific character again (`identity` 4.1, 4.2), and its
     `<visual>` replaces the name once more. Nothing sets it back (`identity` 4.2). **Disputed in review:**
     the evidence for "nothing sets it back" covers only the model-name setter that the set uses; reading a
     Visual block stores the name without it, and that this never runs again on an existing stalker is not
     checked, so this is believed. So after a New Game a
     stalker shows the visual of the specific character that step set, not its record's; one loaded from a
     save shows its saved visual name (`identity` 4.3) [OV-14].
4. **Dead creatures** (stalkers and monsters with health 0) show the same visual name. The section's
   `corpse_visual` is used instead only for a body whose death time is non-zero and lies more than
   `[monsters_common] corpse_remove_game_time_interval` game hours in the past; every dead body of the shipped
   file has death time 0 [OV-6].
5. An empty visual name means no model (245 hanging-lamp records of all levels, all of section
   `lights_hanging_lamp`; the 11 of `l01_escape` are marked for the second renderer only, so they never come
   online with the first renderer, `offline-simulation.md` 6.5) [OV-2].

The visual flags byte of the Visual block (bit 0 set in 15 records of all levels) is copied to the in-game
object; its effect on drawing is **unestablished** [OV-1].

## 3. Kinds without a model

Anomalies (`Z_MBALD`, `ZS_MBALD`, `ZS_MINCE`, `Z_RADIO`), restrictors, smart terrains, the level changer and
climbable objects (`O_CLMBL`, ladders: Shape block only) have no Visual block and draw no model [OV-7].
Anomalies show particle effects named by their sections (`idle_particles`, …), outside this specification.
The actor's own body is not drawn in the first-person view (believed).

## 4. Transform

The object's transform *X* (model space to world) is built from the spawn packet's position **p** and angles
(*a*₀, *a*₁, *a*₂) (sav.md section 7.2). With

    sx = sin a0, cx = cos a0,  sy = sin a1, cy = cos a1,  sz = sin a2, cz = cos a2,

the rotation rows are

    R[0] = ( cz·cy − sz·sx·sy,   −sz·cx,   sz·sx·cy + cz·sy )
    R[1] = ( cz·sx·sy + sz·cy,    cz·cx,   sz·sy − cz·sx·cy )
    R[2] = ( −cx·sy,               sx,     cx·cy            )

and *X* = [[R[0], 0], [R[1], 0], [R[2], 0], [**p**, 1]]: a model-space point **v** goes to world as
**v** · R + **p**. Model space uses the world's axes and units (world-coordinates.md section 1). This is the
transform at spawn for every kind with a model, except that the kinds of section 4.1 may then have the height
of **p** replaced (section 4.1); R is never changed there. (An item whose update packet carries a physics state, Item
update `f` ≠ 0 in sav.md 7.4.1, is believed to take its position and orientation from that state instead;
none of the online items of `l01_escape` has one.)

Evidence (section 5.2): with this rotation the lowest transformed vertex of the first level's objects lies on
the `level.cform` surface below it more often than with any other order or sign of the three elementary
rotations [OV-10, OV-11].

### 4.1 Height correction when an object comes online

When an object comes online (its in-game object is created from its simulation object), the steps run in this
order [CP-1]. This is the one step of creating an in-game object; it is **believed** to be the same at a New
Game, on loading a save and when the switch pass brings an object online (`offsim` 6).

1. R is built from the angles and the position is set to the stored **p** (section 4).
2. The rest of this section applies only to an object without a parent (spawn packet parent id 0xFFFF,
   sav.md 7.2) on a level that has a `level.ai` (`offsim` 3). An object with a parent takes
   its parent's position instead (outside this specification) [CP-11].
3. Its **current level vertex** is set to the Object block's level vertex if that is below *N* (the level's
   vertex count, `offsim` 3), and its game vertex to the Object block's game vertex if that is a valid vertex
   of `game.graph` [CP-1]. Otherwise both keep their initial value (believed: none, i.e. not a valid vertex).
4. If the object **uses the level graph online** (section 4.3), its current level vertex is replaced by the
   result of the search of section 4.4 for the query point (**p**.x, *y*c, **p**.z), *y*c being the height
   of the centre of the object's model sphere: the model file header's bounding sphere centre (`ogf.md` 2)
   transformed by *X* = [R, **p**] of section 4 (with R about the vertical axis only, as for the trader,
   *y*c = **p**.y + the centre's y). Then its game vertex is set to the game vertex of the cross-table cell of
   the new level vertex (`offsim` 2) [CP-5, CP-6].
5. If the object uses the level graph online, **is not the player** (the player character is never
   corrected) and the cell of its current level vertex contains (**p**.x, **p**.z) — the position is on the
   level's grid (`offsim` 5.2: not more than *c*/2 outside the `level.ai` box, column below *s*, row below
   *r*, column · *r* + row below 0x7FFFFF) and column · *r* + row equals the vertex's packed column and row
   (bytes 16–18) — then

       p.y = h(k, p.x, p.z) + 0.001

   with *k* the current level vertex and *h* its plane height of section 4.2 [CP-1, CP-2]. The constant is the
   single-precision value nearest 0.001. x and z are not changed. Otherwise **p** stays as stored.

Nothing else in the steps of coming online changes the position: the engine's own spawn step that follows
(visual, collision form, registration) does not write it, and the later updates of the online location (on
every move of the object's spatial entry) change only the level vertex and game vertex, never the position
[CP-11]. So the correction is made once each time the object comes online; whatever moves it afterwards
(movement of stalkers and monsters, physics) belongs to those systems.

### 4.2 Plane height of a level vertex

Bytes 14–15 of a level vertex (`script-api-level` 3.1) are a `u16` *w*, a packed direction [CP-4]:

- *a* = bits 7–12 of *w* (0 … 63), *b* = bits 0–6 (0 … 127); if *a* + *b* > 126 then *a* = 127 − *a*,
  *b* = 127 − *b*;
- the direction is (*a*, *b*, 126 − *a* − *b*), each component negated if bit 15 (x), bit 14 (y) or
  bit 13 (z) of *w* is set. (The game multiplies it by a positive factor per value of bits 0–12, the
  reciprocal of the unsigned vector's length, so the unpacked direction is of length 1 up to rounding; the
  height below does not depend on that factor.)

With **v** = (*x*v, *y*v, *z*v) the vertex position of `offsim` 3 (*y*v = *y0* + *q* / 65535 · *h* in single
precision: *q* × (1/65535) × *h* + *y0*) and **n** the direction renormalised (each component multiplied by
√(1 / (*n*y² + *n*x² + *n*z²))), the plane through **v** with normal **n** gives, at (*x*, *z*) [CP-3]:

    if |n.y| < 1e-7:  h = y_v
    else:             h = y_v − (n.x · (x − x_v) + n.z · (z − z_v)) / n.y

The game evaluates the second line in single precision as −(((*n*z·*z* + *n*x·*x*) + *n*y·*y*v) + *d*) / *n*y +
*y*v with *d* = −((*z*v·*n*z + *x*v·*n*x) + *n*y·*y*v), rounding each operation; `f32calc.py` reproduces it.

### 4.3 Which kinds use the level graph online

Decided by the object's in-game kind (sav.md 7.4.2 class ids; a scripted class id as the kind it belongs to)
[CP-9]:

- **always**: stalker (`AI_STL`, `AI_STL_S`), trader (`AI_TRADE`, `AI_TRD_S`), every monster kind (the `AI_*`
  monster ids and every `SM_*`), and the grenades `G_F1` and `G_RGD5`; whatever their state flags;
- **if bit 0x80 of the Object block's state flags is set**: every item kind (artefacts `AF_*`, `ARTEFACT`,
  `SCRPTART`, outfits, ammunition, PDA, documents, torch, detectors, medicine, food, attachments, every weapon
  `W_*` and `WP_*` including `W_STMGUN`), the inventory box `O_INVBOX`, the helicopter (`C_HLCPTR`,
  `C_HLCP_S`), the level changer `LVL_CHNG`, the team base `Z_TEAMBS`, and the player `O_ACTOR` (which step 5
  then excludes); the multiplayer kinds `MP_PLBAG` and `SPECT` likewise;
- **never**: crow, phantom, the bolt `II_BOLT`, the vehicles `C_NIVA` and `SCRPTCAR`, physics objects
  (`O_PHYSIC`, `O_PHYS_S`, `P_DSTRBL`), `P_SKELET`, breakable, climbable, hanging lamp, searchlight
  `O_SEARCH`, `SCRPTOBJ`, `W_MOUNTD`, every zone and anomaly kind, restrictors (`SPACE_RS`, `SPC_RS_S`,
  `SCRIPTZN`), smart zones (`SMRTZONE`, `SMRTTRRN`, `RE_SPAWN`), and the rockets (`G_RPG7`, `G_FAKE`).

The scripted switcher `O_SWITCH`, whose online class is a script class over the plain game object, is
**believed** to take the rule of bit 0x80: the scripting binding's wrapper of the plain game object has that
rule [CP-9 lists the wrapper with the bit-0x80 kinds in `slots_all.py`], and the switcher's script defines no
replacement.

This is the online object's own rule; it differs from the simulation's (`offsim` 5.1), e.g. the trader always
uses the level graph online whatever bit 0x80 says.

### 4.4 Finding the level vertex for a point

Input: the start vertex *k*0 (the current level vertex, possibly not below *N*) and the query point
**q** = (*x*, *y*, *z*). Heights *h*(*k*) are plane heights of section 4.2 at (*x*, *z*). [CP-7, CP-8]

1. If **q** is off the level's grid (`offsim` 5.2), go to step 4.
2. If *k*0 < *N* and the cell of *k*0 contains (*x*, *z*), the result is *k*0.
3. Let *k*1 be the **cell vertex** of (*x*, *z*): among the level vertices whose packed column and row equal
   the point's (several may be stacked at different heights), the
   one whose plane height at (*x*, *z*) is the highest at or below *y*; if every one is above *y*, the lowest
   one; on equal heights the earlier one in file order. (In all 18 levels the vertices are stored sorted by
   packed column and row [CP-12].) If there is none, go to step 4. Otherwise:
   - if *k*0 is not below *N*, the result is *k*1;
   - else, with *h*0 = *h*(*k*0) (the plane of *k*0 extended to (*x*, *z*)) and *h*1 = *h*(*k*1): if *y* ≤ *h*0
     and *y* > *h*1, the result is *k*1; if *y* > *h*0 and *y* > *h*1 and *h*0 − *h*1 ≤ 1 (computed as
     (*y* − *h*1) − (*y* − *h*0) ≤ 1), the result is *k*1; in the other cases go to step 4.
4. **Fallback.** If *k*0 < *N*: the result is the one of *k*0 and its linked neighbours (`script-api-level`
   3.1, links below *N*, in link order after *k*0) nearest to **q**, a later one replacing the best only when
   strictly nearer; if *k*0 is not below *N*: the one of all level vertices nearest to **q**. "Nearest" is
   measured to the vertex's cell outline: the four corners of its cell (*x*v ± *c*/2, *z*v ± *c*/2) placed on
   its plane, joined by four edges, and the distance from **q** to the nearest point of those edges
   (**believed**: read from the structure of the computation; the tie rules and the exact order of the
   segment tests are not checked).

A result from step 4 need not contain (**p**.x, **p**.z) in its cell; then step 5 of 4.1 makes no correction.

### 4.5 Worked example: the trader at a New Game on `l01_escape`

Object 84 `escape_trader` (`m_trader`, class id `AI_TRD_S`, no parent): stored position **p** = (−244.6927,
−24.9580, −133.9600), angles (0, 1.549194, 0); Object block game vertex 0, level vertex 594,270, state flags
0xFFFFFF7B (bit 0x80 clear, which does not matter for a trader) [CP-10].

- `level.ai` of `l01_escape`: *N* = 595,499, *c* = 0.7, *h* = 67.247513, box minimum (−283.5, −37.482147,
  −480.9), row length *r* = 1,697, column count *s* = 994.
- Step 3: level vertex 594,270 is valid (its position (368.9, 15.0219, −39.9), far away). Step 4: the model
  `actors\trader\trader` has sphere centre (0.0000034, 0.865506, −0.078219); R turns about the vertical axis
  only, so *y*c = −24.9580 + 0.8655 = −24.0925. The search: (*x*, *z*) is in column 55, row 496 (packed
  93,831), not the cell of 594,270. Two vertices share that cell: 12,441 (position (−245.0, −19.595654,
  −133.7), unpacked direction (3, 120, −3), plane height at **p** −19.609836) and 12,442 (position (−245.0,
  −24.867916, −133.7), direction (0, 125, 1), plane height −24.865836). The cell vertex is 12,442 (the only
  one at or below *y*c). With *h*0 = −17.665663 (the plane of 594,270 at **p**): *y*c ≤ *h*0 and *y*c > *h*1,
  so the result is 12,442.
- Step 5: the cell of 12,442 contains (**p**.x, **p**.z); **p**.y = −24.865836 + 0.001 = **−24.864836**
  (single precision −24.864835739). The trader's transform is section 5.2's with this height: his model is
  drawn 0.0932 m above the stored position (0.0922 m from his vertex's plane being above **p** at his x and z,
  0.001 m from the constant), and every world coordinate of his model moves up by that amount.

## 5. Worked example: the first frame of a New Game on `l01_escape`

`spawns\all.spawn` (dbb, 4,474,073 bytes), the model files of the merged archive view (later archives win) [OV-13].
Which objects are online at the start: `offline-simulation.md` section 9 (671 of the level's 870 records).
Distances are from the actor's stored position.

### 5.1 Visuals of the online objects

Online objects with a model, without the actor: 352 (32 stalkers, the trader and 319 other objects,
including the 55 crows and the monster). The 319 objects other than stalkers and the trader use 101 distinct
model files with 163,064 vertices in total and 388,632 indices at full detail (for type-4 meshes the indices
of detail level 0, i.e. 3 × its triangle count; counted once per object) [OV-9]. For every online object
other than the stalkers the visual is the record's visual name (section 2); for the trader, whose profile
`escape_trader` names the specific character `escape_trader` (`no_random`), that specific character's
`<visual>` `actors\trader\trader` equals the record's [OV-4]. A stalker shows the `<visual>` of the
specific character its supplies step chose (section 2 item 3), one of those its profile offers
(`identity` 4.2); which one depends on the random choice. Records' visuals [OV-5] and the visuals offered
[OV-14], e.g.: `esc_lager1` (record `actors\novice\green_stalker_2`) and `esc_lager2` (record
`actors\novice\green_stalker_10`), profile `esc_stalker_novice`: one of `actors\novice\green_stalker_2`,
`_3`, `_4` and `_5`, so `esc_lager2` never shows its record's; `esc_specnaz1` (record
`actors\soldier\soldier_antigas`): always `actors\soldier\soldier_spetsnaz`; `esc_blokpost_commander`:
always `actors\soldier\soldier_beret_1`, its record's; `esc_stalker_corps1_0001` (record
`actors\neytral\stalker_neytral_balon_8`): one of that, `actors\neytral\stalker_neytral_hood_2` and
`actors\neytral\stalker_neytral_hood_uzor`.

### 5.2 Named objects

Positions and angles as stored; matrix rows of section 4 rounded to 6 decimals; world boxes of the
transformed vertices in single precision, without the quantization of `model-shading.md` section 1. Model
file contents: `skeleton-ogf.md` section 7.2. [OV-8]

- **Object 79** `trader_door` (`physic_object`, `O_PHYS_S`, 1.7 m): visual `physics\door\door_trader` →
  `meshes\physics\door\door_trader.ogf`; position (−248.3039, −24.6881, −133.7155), angles (0, 4.712372, 0);
  R[0] = (−0.000017, 0, −1), R[2] = (1, 0, −0.000017). World box (−248.4411, −24.6881, −135.1301) …
  (−248.0823, −21.9532, −133.6777).
- **Object 89** `trader_light_0003` (`lights_hanging_lamp`, 1.5 m): visual `physics\light\lampa_01_on`;
  position (−245.5421, −23.9448, −134.6337), angles (0, 2.134655, 0). World box (−245.6193, −23.9447,
  −134.7003) … (−245.3585, −23.5131, −134.4912).
- **Object 0** `meshes\brkbl#0.ogf` (`breakable_object`, `O_BRKBL`, 178.2 m, online): visual
  `meshes\brkbl#0` → `levels\l01_escape\meshes\brkbl#0.ogf`; position (−79.4931, −3.1619, −76.9341), angles
  (0.581842, −1.455716, −0.267673); R[0] = (−0.033655, 0.220967, −0.974700), R[1] = (−0.556858, 0.805701,
  0.201881), R[2] = (0.829926, 0.549564, 0.095932). World box (−79.5802, −3.2542, −77.1837) … (−79.4313,
  −3.0329, −76.6842).
- **Object 658** `level_prefix_inventory_box_0003` (`O_INVBOX`, 1.5 m): visual
  `physics\equipment_cache\equipment_box_01_case`. World box (−248.0426, −24.7945, −136.1114) … (−246.9847,
  −24.3651, −135.2594).
- **Object 84** `escape_trader` (`m_trader`, `AI_TRD_S`, 2.1 m): visual `actors\trader\trader`. World box
  at the stored position (−244.8210, −24.9555, −134.8202) … (−244.4099, −23.2295, −133.1037) (its lowest point
  is 0.0025 m above the stored position); online his height is corrected by section 4.1 to −24.864836
  (section 4.5), which raises this box by 0.0932 m.
- **Object 107** `esc_heli` (`C_HLCP_S`, 232.5 m, online): `physics\vehicles\mi24\veh_mi24_u_01`. World box
  (−329.1732, 16.4293, 75.0037) … (−308.8980, 22.1571, 91.5756).

**Transform evidence** (section 4). For each of the level's objects with a model of kinds physics object,
breakable, inventory box, item, ammunition and weapon (online or not), the lowest transformed vertex is
compared with the highest `level.cform` triangle on the vertical through it (surfaces up to 0.1 m above the
vertex counted); "fits" = within 0.05 m. With the rotation of section 4: 143 of 196 physics objects (all 196
with such a triangle), 39 of 49 breakables (all 49 with one), 16 of 24 inventory boxes (of the level's 25
boxes, 24 have one) fit (median distance 0.000 m in each group) [OV-10, OV-12]. Restricted to the 111 objects
that are tilted (|*a*₀| or |*a*₂| > 0.05) and have such a triangle, 83 fit with the rotation of section 4;
the best of the 47 other combinations of order and sign of the three elementary rotations fits 80, the
transposed rotation 56 (of 105 with a triangle) [OV-10, OV-11].

## 6. Unestablished

- The effect of the visual flags byte (section 2).
- The transform of items with a stored physics state (believed, section 4).
- The initial current level vertex and game vertex of 4.1 step 3 when the Object block's are invalid
  (believed: none).
- The model sphere centre used in 4.1 step 4: believed to be the one the model file's header stores, also for
  skeletal models (not checked that loading keeps it unchanged); for an object without a model it is not
  established.
- The distance of 4.4 step 4 and its ties (believed).
- That nothing outside the steps of 4.1 sets the trader's position while he is online: checked only for the
  trader's own code and the online location updates (section 4.1), believed for the rest.

## 7. Provenance

- Date: 2026-11-24 (as sections 3 and 7.1 – 7.2 of the earlier object specification); moved into this specification
  unchanged on 2026-11-27 when that specification was split by system.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`.
- The stalkers' visual (section 2 item 3, section 5.1) was corrected on 2026-11-27 from the static analysis
  of `identity` (its checks, run by OV-14).
- Data examined: `spawns\all.spawn`, `game.graph`, the `config\` tree (`system.ltx` includes,
  `config\gameplay\npc_profile.xml`, `character_desc_*.xml`), every `.ogf` file under `meshes\` and
  `levels\*\meshes\`, `levels\l01_escape\level.cform`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation) of the order in
  which object states are read and where the visual name is set (configuration default, Visual block, specific
  character), the body visual of dead creatures, the model-file lookup and the object transform; a throwaway
  decoder written from this text (spawn records with every block layout, LTX inheritance, model files)
  reproduces the numbers of section 5; the transform convention was checked against the data as described in
  section 5.2.
- Sections 4.1 – 4.5 (height correction) were added on 2026-11-28 from static analysis (headless disassembly
  and decompilation) of `bin\xrGame.dll` (the online object's spawn step, the location validation and the
  level-graph vertex search, the plane height, the per-kind predicates read for every engine and scripted
  class id of the object factory), `bin\xrCore.dll` (the packed-direction unpacking and its factor table) and
  `bin\XR_3DA.exe` (the object's sphere centre and its spawn step), with data from `spawns\all.spawn`,
  `meshes\actors\trader\trader.ogf` and `levels\l01_escape\level.ai`; the worked example is reproduced by
  a throwaway single-precision replica written from this text.
- No leaked source code, derived projects or third-party documentation of the engine were used.
