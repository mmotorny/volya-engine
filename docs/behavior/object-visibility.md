# Object visibility: the dynamic items of the camera pass

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the first renderer
(`bin\xrRender_R1.dll`), of how the camera pass chooses the dynamic items of a frame (objects, effect objects, glows
and light records): what the engine's spatial index holds and how its query of the view frustum works, the sphere
and the sector point of each kind of item (which kinds override the default), the sector and frusta test and its
margin, the occlusion box, the order in which items are handled, and the one item per frame chosen in turn for a
refresh of its light list. It is specification V6 of the scoping specification VS (section 9) and replaces VS 5
where they differ; it also settles the last open item of OT section 10 (section 2.4 here) and the gap listed under
"Gaps found in review" of LS about the round-robin item (section 7).

Conventions: matrices act on row vectors; **E** is the camera position; "the camera pass" is the visibility pass of
VS 1. The **frame number** *F* is the engine's frame counter (OG conventions). An item's **sphere** (centre **c**,
radius *r*) is the sphere it was last placed in the index with (section 2.2). All arithmetic is single precision
(IEEE 754 `f32`). Abbreviations of approved specifications: VS `docs/behavior/visibility-scope.md`, OT
`docs/behavior/occlusion-tests.md`, OG `docs/behavior/occlusion-grid.md`, VP-spec `docs/behavior/view-projection.md`,
TO `docs/behavior/traversal-options.md`, LV `docs/formats/level.md`, DL `docs/behavior/dynamic-lights.md`, GL
`docs/behavior/glows.md`, LS `docs/behavior/light-shadows-projection.md`, PD `docs/behavior/particle-drawing.md`,
ODL `docs/behavior/object-detail-level.md`.

**Checks.** Every claim carries a tag. The checks are scripts in `checks/object-visibility/` of the dirty workspace
(`run.py` runs them all): `binary.py` [B…] reads saved read-only static analysis (headless decompilation and
disassembly, tables of virtual functions, constants and section layout) of `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`
and `bin\xrGame.dll`; `data_levels.py` [L…] reads the shipped `level.cform` files; `model.py` [M…] is a
single-precision model of the spatial index written from section 2 and computes the worked examples. All checks
pass.

## 1. Place in the camera pass

The dynamic part of the camera pass runs only when the pass has a camera sector and the device's dynamic-objects
switch is on (VS 6, 7) [B12]. Its steps, in order [B10, B12]:

1. one query of the engine's spatial index with the view frustum (section 2.4);
2. the results are sorted (section 3);
3. the drawn-object mark is cleared and the game's two HUD callbacks run (LS 3.1, 3.2);
4. the round-robin index is chosen and the level's current entity's light list is refreshed (section 7);
5. each result is handled once, in sorted order (sections 4 – 6).

After the loop come the renderer's shadow and projected-texture steps (LS) [B12]. **Disputed in review:** these two
steps are not part of the dynamic part: they run in every camera pass that has a camera sector, whether or not the
dynamic-objects switch is on (only the query and the loop depend on the switch).

## 2. The spatial index and its query

### 2.1 What the query returns

Each entry of the index carries a **kind word**. The camera pass asks for entries whose kind word has bit 1 (the
**renderable** kind) or bit 2 (the **light** kind) set [B1]. Entries get these bits as follows [B1]:

- **Light records** (DL 2) are made with the light kind. Only active records are in the index (DL 2.4).
- **Glows** (GL) are made with the renderable kind.
- **Game objects**: an object gets the renderable kind when it is made visible and has a model, and when it is
  detached from a parent object (it is then made visible, and gets the kind if it has a model); it loses the kind
  when it is made invisible, when it is attached to a parent (an item taken into an inventory or a hand, for
  example), and when its configuration is loaded. So an object held by another object is not returned; how such
  an object is drawn is outside this specification.
- **Effect objects** (PD 7.1) start with no kind and get the renderable kind when they are first placed in the
  index (with their culling sphere, PD 7.1).
- Entries of the physics simulation are kept in a separate index that this pass does not query [B1].

### 2.2 The sphere of each kind

An entry's sphere is set when it is placed in the index and at each later placement (each update of its place)
[B3, B5]:

- **Light record**: the bounding sphere of DL 2.3 [B4].
- **Glow**: its position and radius (GL) [B4].
- **Effect object**: its culling sphere (PD 7.1).
- **Game object**, by default: the centre of its model's bounding sphere mapped by the object's transform, and that
  sphere's radius as stored (not scaled) [B5]. Kinds of object that differ [B5]:
  - human characters (the player's character, also in multiplayer, and the non-player humans): the default
    centre; the radius is the model's plus the radius of the item in the character's active inventory slot, when
    there is one and it answers a further test of the game's (which items pass is **unestablished**);
  - anomaly zones, level changers and the other space restrictors (14 kinds): the centre of their shape's bounding
    sphere mapped by the object's transform, and that sphere's radius;
  - one further anomaly kind: the object's position, with its shape's radius;
  - ladders: the object's position, with a radius of their own;
  - the multiplayer observer: its position, radius 10⁻⁵;
  - hanging lamps: the default when they have a model, otherwise their position and radius 10⁻⁵.

  Only objects with the renderable kind are returned (2.1), so these rules matter only for such kinds of object
  as have a model; which of them have one is not established here.

### 2.3 Structure of the index

The index is an octree of cubic nodes, each with a centre and a **half-extent** *h* (the cube is centre ± *h* on
every axis) [B11]:

- The root has its centre at the origin and *h* = 1024; it is made once, and later initialisations (each level
  load) keep it; the level box handed to the initialisation is not used [B11].
- **Placing** an entry with sphere (**c**, *r*): if the sphere lies within the root's cube, |*c*ᵢ| ≤ 1024 − *r* on
  every axis (inclusive), it descends from the root; otherwise it is kept at the root, with the root's *h* recorded
  as its node's size [B11]. Descending through a node of half-extent *h*: while *h* > 8 and *r* < *h* / 2
  (strictly), the entry moves into the child cube of half-extent *h* / 2 whose centre is the node's centre moved by
  ± *h* / 2 on each axis, + on an axis where *c*ᵢ is greater than the node centre's coordinate and − otherwise
  (children are made when first needed) [B11]. It is kept at the node where this stops, with **2 *h*** recorded as
  its node's size [B11]. Nodes go down 7 levels, to *h* = 8 [M1].
- **Updating** an entry's place: the sphere is set anew (2.2); the entry stays at its node while its sphere lies
  within the node's centre ± (recorded size) on every axis (inclusive), otherwise it is removed and placed again
  [B11]. Every placement and update also marks the entry for a sector refresh (section 4) [B3].

### 2.4 The query

The query empties its result list, then walks the tree from the root [B1]: a node is tested as the box of its
centre ± **2 *h*** (OT 7) and, unless the box is outside the view frustum, each of its entries whose kind word has
a requested bit is tested by its sphere (OT 7; planes the node's box was wholly inside are not tested again) and
added to the result list when not outside, and its children are walked in the same way [B1]. The view frustum is
the camera pass's (VS 4: left, right, top, bottom and far planes; no near plane).

Every entry kept in the tree lies within its node's centre ± 2 *h*, and a child's doubled box lies within its
parent's [M1]. So the node tests never drop an entry of the tree whose sphere is not outside the view frustum: the
query returns exactly the requested entries whose sphere is not outside the view frustum, with one exception:
entries kept at the root because their sphere is not within ± 1024 are tested only when the root's box (± 2048) is
not outside the frustum [M1, B11]. (This settles OT 10: entries lie within their node's doubled box, except those
outside the root's cube.) The collision boxes of 28 of the 30 shipped levels lie within ± 1024; those of
`l12_stancia` and `l12_stancia_2` reach x = 1398.44 [L1], so in those two levels items beyond x ≈ 1024 are held at
the root. With the camera inside the root's box (± 2048) the root's box is never outside the view frustum, which
passes through the camera (**believed**: the frustum's side planes meet at **E**, so a box containing **E** has
points inside every plane).

## 3. Order

The results are sorted by the squared distance |**c** − **E**|² of their sphere centres, nearest first (a strict
"less than" comparison) [B2]. The sort is not one that keeps the query's order for equal distances [B2]; the order
of entries at equal distance is **unestablished**. Glows, lights, objects and effect objects are sorted together.

## 4. The sector of an entry

Each entry has a sector (none until first found). Before an entry is handled, if it is marked for a sector refresh
(it was placed or updated since its last refresh, section 2.3), its sector is found again from its **sector point**
by the rule the pass uses for the camera (LV 9.1: the downward ray of at most 500 units against portals and
collision triangles), and the mark is cleared; when the rule finds no sector, the previous one is kept [B3]. The
refresh happens only when the entry is reached: an entry that moved but is not returned keeps its mark [B3]. Two
other steps of the renderer refresh an entry's sector the same way before reading it, one when an object is marked
as being drawn (6.1) and one after the loop (both outside this specification); nothing in the engine or the game
does [B3].

The sector point [B4]:

- by default, the centre of the entry's sphere (2.2): glows, effect objects and every kind of game object (no kind
  of the game overrides it);
- **light records** override it: the sector point is the record's position *P* (as DL 2.4 states), which for a
  spot light is not its sphere centre (DL 2.3).

## 5. The sector and frusta test

An entry is skipped when it has no sector, or when its sector was not visited by the camera pass's traversal
(LV 9.2; the sector's mark is that of the traversal, TO 3) [B6]. Otherwise the frusta its sector was reached with
are tried in the order the sector was reached: the first one its sphere is not outside is taken, and the entry is
handled once with it; an entry whose sphere is outside every one of them is skipped [B6]. The sphere test has no
margin: outside when **n** · **c** + *d* > *r* for some plane, strictly (OT 7) [B6]. The frustum taken becomes the
pass's current frustum while the entry is handled [B6]; the objects' drawing does not test against it (6.1).

Unlike a static visual (OT 5), an entry is handled at most once per pass, however many frusta its sector has [B6].

## 6. Handling by kind

### 6.1 Objects

An entry of the renderable kind that answers as a renderable object (every game object and effect object does;
glows do not) [B7]:

1. **Occlusion.** Its box is the smallest axis-aligned box around its model's bounding box under the object's
   transform (OT 6, "an object's box"); it is tested with the cache of OT 5 (kept in the model's top visual; due
   when *F* ≥ *N*, and only while occlusion is on, OT 4); a hidden object is skipped [B7]. With no occlusion mesh
   no object is hidden (OT 4).
2. **Round robin.** If the entry's position in the sorted list is the frame's round-robin index (section 7) and the
   object has renderer data of its own (which holds its light list), its light list is fully updated (LS 4.2) [B7, B10].
3. **Drawing.** The object is marked as the object being drawn (LS 3.1), it draws itself, and the mark is cleared
   [B7]. Game objects hand their model to the renderer through the renderer's call for adding a visual without tests,
   which makes no further frustum or occlusion test of the model's parts (the small-part test of ODL 3 still
   applies); the game never uses the renderer's other call, which adds a visual with frustum and occlusion tests
   [B7].

### 6.2 Glows

An entry of the renderable kind that is not a renderable object is a glow; it is handed to the glow list's
processing of GL 3 (fade and listing; no occlusion test by the grid, GL 5) [B8].

### 6.3 Light records

An entry of the light kind is a light record [B9]:

1. skipped if it has no sector (never the case here, since section 5 already skipped such entries; this is the
   only check before item 2: whether the record is enabled is not tested here) [B9];
2. its box *C* ± *R* (its sphere, DL 2.3) is built anew and tested with the cache of OT 5 (kept in the record);
   a hidden record is skipped;
3. it is listed by DL 4 rules 3 – 4 (not already listed this frame; not excluded, `r1_dlights` on, range at least
   0.1, a sector) [B9].

This matches DL 4; DL 4 rule 2 is the test of item 2.

## 7. The round-robin item

Before the loop, in every camera pass that reaches the dynamic part (section 1) [B10]:

- a counter is increased by 1; with *N* results (all kinds counted), the **round-robin index** is
  counter mod *N*; with no result there is none [B10];
- the light list of the level's current entity (in play, the actor) is fully updated (LS 4.1), when it has
  renderer data of its own [B10].

In the loop, the entry at the round-robin index gets the full update (LS 4.2) only if it is an object that passes
sections 5 and 6.1 step 1 [B7]; if that entry is a glow, a light record, or an object that is skipped, no further
object is updated this frame. An object is updated at most once per frame (so the current entity, if it is also
the round-robin entry, only once) [B10]. (This settles the LS gap: the list counted is every result of the query:
objects, effect objects, glows and light records.)

The counter is set to 0 when a level is unloaded [B10]; before the first unload it is **believed** 0 (it lies in
the part of the program's data that holds no file bytes, which the system's loader fills with zeros, and nothing
else writes it) [B10]. A value checked before this step that could disable it is only ever set to its "on" value,
so the step runs in every such pass [B10].

## 8. Notes for an implementation

- A renderer that draws every visible object whose sphere is not outside the view frustum, ignoring sectors and
  occlusion, draws a superset of what the original draws (VS 8).
- Matching the original's choices exactly needs: the index's root and placement rules (2.3), the sector refresh
  only when an entry is reached (4), the light records' position as sector point (4), the first-frustum rule (5),
  the object box and cache (OT 5 – 6), and the counter of section 7.
- The order of section 3 decides the glow list's order (GL 3), the round-robin item and the order of the shared
  generator's values taken by the cached tests (OT 5).

## 9. Worked examples

**Example 1: placing entries** [M2]. An entry with centre (100, 5, −300) and radius 2 descends through nodes of
half-extent 512, 256, …, 16 and is kept in the node of half-extent 8 centred at (104, 8, −296), recorded size 16;
the same centre with radius 20 stops at the node of half-extent 32 centred at (96, 32, −288) (20 is not below 16),
recorded size 64. With radius 20, a centre at x = 1004 is within the root's cube (1004 ≤ 1024 − 20) and descends;
at x = 1010 it is kept at the root.

**Example 2: an object hidden in its sector** [B6, B7]. An object whose sector was reached with two frusta, its
sphere outside the first and not outside the second, is handled once, with the second; if its occlusion test then
reports it hidden, it is not drawn this frame and is tested again in the next frame in which it is reached (OT 5);
it is not tried again with a later frustum.

**Example 3: the round robin** [M2]. With the counter at 40 before the pass and 7 results, the counter becomes 41
and the index is 6, the farthest result; if that result is a light record, no object besides the current entity
gets a full update in this frame. In the next pass with 7 results the index is 0, the nearest.

## 10. Unestablished

- The order of results at equal distance (section 3).
- Which held items enlarge a character's radius (2.2).
- Which kinds of object with an overridden sphere have a model, and so are ever returned (2.2).
- The counter's value before the first level is unloaded (believed 0) (section 7).

## 11. Provenance

- Date: 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\xrRender_R1.dll`,
  `bin\XR_3DA.exe`, `bin\xrGame.dll`; archives `gamedata.db0` … `gamedata.dbd` (merged view).
- Data examined: `levels\*\level.cform` (header bounding boxes, 30 files).
- Analysis: static analysis of the shipped binaries (headless decompilation and disassembly, read-only; tables of
  virtual functions and run-time type information; constants and section layout read from the binaries); a
  single-precision model of the index's placement rules on random entries; arithmetic.
