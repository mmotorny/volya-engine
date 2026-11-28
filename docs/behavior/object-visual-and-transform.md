# An object's visual and transform

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of where an in-game
object's visual comes from and where it is placed: which visual name an object of `spawns\all.spawn` has
when it comes online, how a visual name finds its model file, which kinds have no model, and how the stored
position and angles become the object's transform. The model files themselves are specified in
`docs/formats/skeleton-ogf.md`; which objects are online in `docs/behavior/offline-simulation.md` section 6.
Animation, AI and physics simulation are out of scope: the transform is the one at spawn. What could not be
established is marked **unestablished**; what is inferred rather than read from the program or the data is
marked **believed**.

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

Conventions: little-endian; matrices in the row-vector convention (a point `p` is transformed as
`[p 1] · M`; `M[r][c]`, rows and columns counted from 0); angles in radians. A record's "section" is the
configuration section named in its spawn packet, its "class id" that section's `class` value. "The actor" is
the record with section `actor`.

**Checks.** Bracketed ids [OV-*n*] name the checks of the dirty workspace's
`checks/level-objects-split/run_all.py` (decoders `sp.py`, `refs.py`, `chars.py`, `named.py`, `xform.py`,
`orders.py` beside it).

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
3. A **Trader-like** block selects the object's specific character when it is read: if its "specific
   character" field is empty, one is chosen from the "character profile", which names a `<character>` of
   `config\gameplay\npc_profile.xml` (the profile's own `<specific_character>` if it names one, else a random
   choice among the `<specific_character>` descriptions of the files listed by
   `[profiles] specific_characters_files`, in `config\gameplay\`, whose `<class>` matches the profile's and
   that are not marked `no_random`). If the chosen description has a non-empty `<visual>`, that visual
   (extension removed, lower-cased) replaces the name at that moment. Consequences:
   - **trader** (`AI_TRD_S`, layout Object, Visual, Trader-like) and **player** (Creature, then Trader-like):
     the description's visual wins over the record's;
   - **stalker** (`AI_STL_S`, layout Trader-like first, then Monster-like with its Visual block): the record's
     Visual block is read after the selection and **restores the record's visual name**. A stalker shows its
     record's visual whatever its profile (its specific character still decides other properties, outside
     this specification).
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
transform at spawn for every kind with a model. (An item whose update packet carries a physics state, Item
update `f` ≠ 0 in sav.md 7.4.1, is believed to take its position and orientation from that state instead;
none of the online items of `l01_escape` has one.)

Evidence (section 5.2): with this rotation the lowest transformed vertex of the first level's objects lies on
the `level.cform` surface below it more often than with any other order or sign of the three elementary
rotations [OV-10, OV-11].

## 5. Worked example: the first frame of a New Game on `l01_escape`

`spawns\all.spawn` (dbb, 4,474,073 bytes), the model files of the merged archive view (later archives win) [OV-13].
Which objects are online at the start: `offline-simulation.md` section 9 (671 of the level's 870 records).
Distances are from the actor's stored position.

### 5.1 Visuals of the online objects

Online objects with a model, without the actor: 352 (32 stalkers, the trader and 319 other objects,
including the 55 crows and the monster). The 319 objects other than stalkers and the trader use 101 distinct
model files with 163,064 vertices in total and 388,632 indices at full detail (for type-4 meshes the indices
of detail level 0, i.e. 3 × its triangle count; counted once per object) [OV-9]. For every online object the
visual is the record's visual name (section 2); for the trader, whose profile `escape_trader` names the
description `escape_trader` (`no_random`), the description's `<visual>` `actors\trader\trader` equals the
record's [OV-4]. Visuals of stalkers as drawn (section 2: the records'), e.g. `esc_lager1`
`actors\novice\green_stalker_2`, `esc_lager2` `actors\novice\green_stalker_10`, `esc_specnaz1`
`actors\soldier\soldier_antigas`, `esc_blokpost_commander` `actors\soldier\soldier_beret_1`,
`esc_stalker_corps1_0001` `actors\neytral\stalker_neytral_balon_8` [OV-5].

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
  (−244.8210, −24.9555, −134.8202) … (−244.4099, −23.2295, −133.1037) (its lowest point is 0.0025 m above the
  stored position).
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

## 7. Provenance

- Date: 2026-11-24 (as sections 3 and 7.1 – 7.2 of the earlier object specification); moved into this specification
  unchanged on 2026-11-27 when that specification was split by system.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`.
- Data examined: `spawns\all.spawn`, `game.graph`, the `config\` tree (`system.ltx` includes,
  `config\gameplay\npc_profile.xml`, `character_desc_*.xml`), every `.ogf` file under `meshes\` and
  `levels\*\meshes\`, `levels\l01_escape\level.cform`.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation) of the order in
  which object states are read and where the visual name is set (configuration default, Visual block, specific
  character), the body visual of dead creatures, the model-file lookup and the object transform; a throwaway
  decoder written from this text (spawn records with every block layout, LTX inheritance, model files)
  reproduces the numbers of section 5; the transform convention was checked against the data as described in
  section 5.2.
- No leaked source code, derived projects or third-party documentation of the engine were used.
