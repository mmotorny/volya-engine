# Occlusion tests: boxes, polygons and screen rectangles against the grid

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the first renderer
(`bin\xrRender_R1.dll`), of how shapes are tested against the occlusion grid that specification OG draws: the box,
polygon and screen-rectangle tests, the exact depth comparison and the rounding of rectangle edges, every condition
under which a shape counts as visible without a test, the caching of results per item and its random counts, which
item uses which test, and whether the frustum tests made before them use any margin. It is specification V4 of the
scoping specification VS (section 9) and replaces VS 3.3 where they differ; it also settles the two gaps listed
under "Gaps found in review" of VS (sections 2.5 and 7).

Conventions: matrices act on row vectors; **VP** is the frame's view-projection (VP-spec section 1); for a point
**p**, (x, y, z, w) = (**p**, 1) · M for a matrix M. The camera position is **E**. A shape's **depth** is z / w, as in
the grid (OG conventions: 0 at the near value *n*, 1 at the far value *f*). The **frame number** *F* is the counter
of OG conventions. All arithmetic is single precision (IEEE 754 `f32`) unless stated; "floor" and "ceil" are the
original's conversions of OG conventions (−0.0 gives floor −1, ceil 1). The grid is OG's 64 × 64 grid of signed
32-bit integers, cell (*i*, *j*) in row *i* from the top and column *j* from the left; an empty cell holds
2³⁰ = 1,073,741,824. Abbreviations of approved specifications: VS `docs/behavior/visibility-scope.md`, OG
`docs/behavior/occlusion-grid.md`, VP-spec `docs/behavior/view-projection.md`, LV `docs/formats/level.md`, OGF
`docs/formats/ogf.md`, TO `docs/behavior/traversal-options.md`, DL `docs/behavior/dynamic-lights.md`, DO
`docs/behavior/detail-objects.md`, GL `docs/behavior/glows.md`, LS `docs/behavior/light-shadows-projection.md`, PD
`docs/behavior/particle-drawing.md`, PL `docs/behavior/particle-lifecycle.md`.

**Checks.** Every claim carries a tag. The checks are scripts in `checks/occlusion-tests/` of the dirty workspace
(`run.py` runs them all): `binary.py` [B…] reads saved read-only static analysis (headless decompilation and
disassembly) of `bin\xrRender_R1.dll`, `bin\XR_3DA.exe` and `bin\xrGame.dll` and constants from the binaries' data;
`check_emu.py` [E…] runs the shipped test routines, instruction by instruction from their saved disassembly, in a
small interpreter of the instructions they use, and compares every result with `model.py`, a single-precision model
written from this specification, on random scenes, shapes and grids (including points at the near limit, shapes off
screen and stored values at the comparison threshold); `check_empty_box.py` [E4] runs the shipped object-box code
of the camera pass and the box test in the same interpreter with single-precision results exact to the bit, NaNs
included (`emu_nan.py`), on empty boxes and random cameras, transforms and grids, against the rule of section 6;
`model_checks.py` [M…] checks numeric facts and the worked
examples of section 9 (`examples.py`, which takes the grid of OG example 1 from OG's own model). All checks pass.

## 1. The tests' matrix

- The tests do not project with **VP** directly but with a **tests' matrix** A whose four columns are
  ½ (x + w), ½ (w − y), z and w of **VP**: for each row *r*, A[*r*][0] = ½ VP[*r*][0] + ½ VP[*r*][3],
  A[*r*][1] = ½ VP[*r*][3] − ½ VP[*r*][1], A[*r*][2] = VP[*r*][2], A[*r*][3] = VP[*r*][3], each a single rounding [B1].
- For a point **p**, with (*u*′, *v*′, *z*, *w*) = (**p**, 1) · A: the reciprocal *q* = 1 / *w* is computed first, then
  *u* = *u*′ · *q*, *v* = *v*′ · *q* and the depth *z* · *q* [B2, E1, E2]. So *u* = ½ (x/w + 1) and
  *v* = ½ (1 − y/w): 0 … 1 across the screen, *u* to the right, *v* downward; 64 *u* is OG's screen coordinate.
- A is rebuilt by a drawing of the grid (OG 2) only when the occluder search of that drawing (OG 3) returned at
  least one candidate (before the skip counts are applied); a drawing whose search returns nothing leaves A as it
  was [B1]. A test uses A as it is when the test runs.
- Before A is first built it is **believed** all zero: it lies in the part of the program's data that holds no file
  bytes, which the system's loader fills with zeros (documented for the executable format), and nothing else found
  writes it [B9]. With A all zero every box and polygon test reports visible untested (section 3, *z* = 0). Whether
  A is reset when a level is loaded is **unestablished**.

## 2. The tests

### 2.1 The grid scan (the screen-rectangle test)

Every test ends in one **scan** of a rectangle (*u*₀, *v*₀, *u*₁, *v*₁) with a depth *d* [B2, E1 – E3]:

1. **Edges.** For a coordinate *c*, its cell index is floor(*s*) with *s* = 64 *c* + 0.5 (the product is exact, the
   sum is rounded to single precision). Columns run from *j*₀ = index(*u*₀) clamped to 0 … 63 to
   *j*₁ = index(*u*₁), raised to *j*₀ when smaller and lowered to 63 when larger; rows likewise from *v*₀ and *v*₁
   [B2]. So the cells read are those that the rectangle shifted right and down by half a cell,
   [64 *u*₀ + 0.5, 64 *u*₁ + 0.5] × [64 *v*₀ + 0.5, 64 *v*₁ + 0.5], touches (cell *j* covers *j* ≤ *s* < *j* + 1),
   clamped to the grid; at least one cell is always read, also for a rectangle wholly off screen (its nearest edge
   cell) [B2, M4].
2. **Threshold.** *t* = ceil(*d* × 2³⁰) + 1, a signed 32-bit integer (the product is exact) [B2].
3. **Early drawing.** If the grid has not been drawn in this frame, it is drawn now (OG 2, "early drawing"; skipped
   while the main menu is shown). This happens after steps 1 – 2, so a test made before the frame's grid is drawn
   projects with the A left by the previous drawing and reads the grid drawn from the current **VP** [B2].
4. **Read.** The cells are read row by row from row *i*₀, each row from column *j*₀; the shape is **visible** at the
   first cell whose value is **greater** than *t* (signed comparison), and **hidden** when no cell is [B2].

So a cell makes the shape visible exactly when its stored value exceeds ceil(*d* × 2³⁰) + 1. Since a stored value is
floor(*D* × 2³⁰) of the cell's depth *D* (OG 7.2):

- for *d* ≥ 1/64 (every shape whose nearest point is farther than view depth 0.2032 at *n* 0.2, *f* 100) [M2], a cell makes the shape
  visible exactly when *D* > *d* (strictly) [M1];
- for smaller *d* the cell's code must exceed the shape's by at least 2: e.g. *d* = 0.01 against *D* one
  single-precision step farther is hidden, three steps farther visible [M1];
- an empty cell (2³⁰) makes every shape with *d* < 1 visible (*d* = 0.99999994: *t* = 1,073,741,761) and none with
  *d* = 1 (*t* = 1,073,741,825) [M1]; a cell with a negative value (such as a depth clamped to −1.99) makes no shape
  visible, since *d* > 0 for every shape that reaches the scan (section 3) [M1].

The scan with a rectangle and depth given directly is the **screen-rectangle test** [B2]. Its only user is the
traversal's screen-rectangle mode (TO 3, which no pass of this renderer uses): it passes the portal's rectangle
(*x*₀, *y*₀, *x*₁, *y*₁) as (*u*₀, *v*₀, *u*₁, *v*₁) and *z*\_min as *d* [B2, B3, B10]. (This settles TO 3's
"which inputs the rectangle test is given".)

### 2.2 The box test

For an axis-aligned box with corners **a** (smallest) and **b** (largest) [B2, E1]:

1. Each of the 8 corners is projected (section 1). If any corner has *z* < 10⁻⁵ (*z* the third coordinate before
   division; a NaN does not count), the box is **visible** without a test [B2].
2. Otherwise *u*₀, *u*₁ are the smallest and largest *u* of the corners, *v*₀, *v*₁ those of *v*, and *d* the
   smallest depth; then the scan of 2.1 [B2, E1].

The box test itself does not look at whether occlusion is on (section 4); every one of its uses does [B3, B10].

### 2.3 The polygon test

For a polygon of points **P**₀ … **P**ₘ₋₁ (*m* ≥ 1; the portals give the clipped portal polygon, at least 3
points, TO 2.3) [B2, E2]:

1. While occlusion is off (section 4) the polygon is **visible** without a test [B2].
2. Each point is projected; if any has *z* < 10⁻⁵ the polygon is **visible** without a test [B2].
3. Otherwise the rectangle and depth are taken as for the box (2.2) and scanned (2.1) [B2, E2].

The polygon test is not cached: it is made each time the traversal tests a portal (TO 2.3) [B6].

### 2.4 Arithmetic

Each of *u*′, *v*′, *z*, *w* is ((first product + second product) + third product) + the translation entry
A[3][col], one product per axis of the point (A[axis][col] × coordinate), added in this order of axes [E1, E2]:

| Test, point | *u*′ | *v*′ | *z* | *w* |
|---|---|---|---|---|
| box, every corner | x, z, y | z, y, x | z, y, x | z, y, x |
| polygon, first point | y, z, x | x, y, z | x, y, z | x, y, z |
| polygon, later points | z, y, x | x, z, y | z, y, x | x, z, y |

Extremes: the first point sets the smallest and largest *u* (and *v*); each later point lowers the smallest when
smaller, otherwise raises the largest when larger; the depth keeps the smallest [E1, E2]. The model with these orders
gives the shipped routines' rectangles, thresholds and results in all 1,800 random cases of the checks [E1 – E3].
The index conversion is floor for every value the shapes produce (|*s*| < 2³¹; beyond that the original's
conversion does not give floor) [B2]. **Disputed in review:** not for every value: the empty box of section 6 gives
NaN coordinates and depths, for which the conversions give no floor or ceil: a NaN coordinate (sign bit set) gives
the index −2 (so the clamped indices 0), and a NaN depth gives −1 for ceil(*d* × 2³⁰), so the threshold 0; an
infinite coordinate gives the index 1 for +∞ and −1 for −∞.

### 2.5 The tests offered to the rest of the program

The renderer offers three of these tests to the rest of the program [B4]:

- the polygon test (2.3);
- a **box test with a camera check**: **visible** without a test while occlusion is off (section 4); **visible**
  without a test when the camera position **E** lies within the box on every axis (**a** ≤ **E** ≤ **b**, inclusive);
  otherwise the box test of 2.2, not cached [B4];
- the cached test of section 5 for a visual's data.

So the further condition of VS's first review gap is: **occlusion off**. No call to these three through the
engine's or the game's renderer pointer was found (searched in the game's decompiled code and the engine's
disassembly) [B4]; which parts of the program use them is **unestablished**.

## 3. The near limit

The untested-visible rule of 2.2 and 2.3 (*z* < 10⁻⁵) depends on the projection the frame was drawn with. With the
projection of VP-spec 2, *z* = *f* / (*f* − *n*) · (view depth − *n*), so a point is untested-visible when its view
depth is below *n* + 10⁻⁵ (*f* − *n*) / *f*, or it is behind the eye: about 0.20001 for *n* 0.2, *f* 100 (at view depth
0.2 exactly *z* is 1.5 × 10⁻⁸ in single precision; at 0.20002 it is 2.0 × 10⁻⁵) [M2], and about 0.01001 while the
player's view uses the computed near value 0.01 (VP-spec 3.1). The value applies to the A in use (section 1). A box
around the camera always has a corner behind the eye and is never hidden (VS 6). **Disputed in review:** this holds only when A was built from the current camera; a test that projects
with an A from an earlier drawing (section 1; 2.1 step 3) may find every corner of a box around the current camera
in front of the earlier camera's near limit and then scans it, so such a box is not guaranteed to be visible.

## 4. Whether occlusion is on

The renderer keeps one switch, **occlusion on** [B3]:

- off when the renderer is made [B3];
- at the start of each camera pass, before the grid is drawn, set on exactly when the level has an occlusion mesh
  (VS 3.2) [B3];
- during the frame's drawing, set off just before the light passes (DL 5.1: spot lights, then point lights) and the
  wall-mark pass that follows them (LS-P1), and set back to "the level has an occlusion mesh" after them [B3].

So between frames it is on exactly when the level has an occlusion mesh (as DO 5.2 states for the detail pass), and
during the light passes and the wall-mark pass no test hides anything. The switch is checked by every use: the
cached test (section 5), the inline cached tests of objects, lights and detail entries, the polygon test, the
screen-rectangle mode and the box test with a camera check; while it is off each of them reports visible without a
test and changes no cached value [B3, B4, B5].

## 5. Caching

Static visuals, objects, lights and detail entries cache their results [B5]. Each item keeps a **next-test frame**
*N* and the frame of its last test. When the item is reached in frame *F* [B5]:

1. if *F* < *N* (compared unsigned), it is **visible** without a test;
2. otherwise, if occlusion is off (section 4), it is **visible** without a test;
3. otherwise its box is tested (2.2). After a visible result *N* becomes *F* + 10 + (*k* mod 15), *k* one value of
   the shared generator, taken only after a visible result; after a hidden result *N* becomes *F* + 1. The frame of
   the test is recorded as *F* (what reads it is outside this specification) [B5].

In cases 1 and 2 nothing is stored. So after a visible result the item is untested (visible) in frames
*F* + 1 … *F* + 9 + (*k* mod 15), 9 to 23 frames, and tested again in frame *F* + 10 … *F* + 24; after a hidden result
it is tested again in the next frame in which it is reached. An item hidden by its test and reached again in the
same frame counts as **visible** the second time (*F* < *F* + 1) [M4]: e.g. a static visual of a sector reached with
two frusta (LV 9.2, 9.3), hidden at its first reach, is drawn at the second.

**The generator.** Every cached test takes its value from the engine's one shared generator of OG 4.1
(*s* ← *s* × 214013 + 2531011 mod 2³², value (*s* >> 16) & 0x7fff), the same generator that gives the occluder skip
counts (OG 4.1) and the detail objects' values (DO 5.2), shared with other parts of the renderer and the engine
[B5]. The values are drawn in the order the tests are made; the seed is **unestablished** (OG 10).

**Where the values live and how they start** [B5, B8]:

| Item | Kept in | *N* when made |
|---|---|---|
| static visual | the visual (each node of a sector's visual hierarchy has its own) | 0; a visual made as a copy of another starts with that one's values |
| object | its model's top visual (the same fields) | as for visuals; whether two objects can share one visual's values is **unestablished** |
| light | the light | not set when the light is made: **unestablished** |
| detail entry | the entry | 0 |

With *N* = 0 the item is tested the first time it is reached.

## 6. Which item uses which test

| Item | Tests before the occlusion test | Occlusion test | Cached |
|---|---|---|---|
| Static visual: every node of the visual hierarchy of a visited sector, for each frustum the sector was reached with (LV 9.3, OGF 8.1) | the sphere-and-box frustum test (section 7) against that frustum; below a node wholly inside it, none | box test of the visual's bounding box | yes [B6] |
| Object (renderable; particle effects are objects, PD 7.1) | its sector visited and its spatial sphere not outside one of that sector's frusta (VS 5) | box test of the box below | yes [B6] |
| Light (DL 4) | as objects | box test of *C* ± *R* on each axis, *C* and *R* the centre and radius of its spatial sphere, rebuilt before each test | yes [B6] |
| Detail entry (DO 5.2) | the group's sphere-and-box test; when the group is partly inside, the entry's own | box test of the entry's box | yes [B6] |
| Portal (camera pass, TO 2.3) | traversal rules 1 – 5 | polygon test of the clipped portal polygon | no [B6] |
| Portal, screen-rectangle mode (TO 3; unused) | TO 3 | screen-rectangle test | no [B3] |
| Glow | — | none: rays (GL 5) | — |

These are all uses of the grid by the renderer, besides the three tests offered to the rest of the program
(2.5) [B10]. For a static visual the frustum test comes first and the occlusion test next; a node hidden by either
is not descended [B6]. A light whose record has no sector is skipped before its test (the check of
`docs/behavior/object-visibility.md` 5, made again; no light reaches it without one); whether the record is
enabled is not tested at this point; a hidden light is not listed (DL 4) [B5].

**An object's box** [B6]: with the object's transform rows **i**, **j**, **k** (axes) and **c** (translation) and its
visual's bounding box (**a**, **b**), each component of the transformed corner **p** = ((**k** *a*_z + **j** *a*_y) +
**i** *a*_x) + **c**; then, starting with min = max = **p**, for the axes in the order **i** × (*b*_x − *a*_x),
**j** × (*b*_y − *a*_y), **k** × (*b*_z − *a*_z), each component of that product is added to the max when its sign
bit is clear and to the min when it is set. This is the smallest axis-aligned box around the transformed box. (The
order of additions here is from the static analysis; it gives the same box as the shipped code, run in the
interpreter, for finite boxes and empty ones [E4].)

**An empty box** [E4]. A visual whose box is still empty, each minimum 3.4028235 × 10³⁸ and each maximum its negative
(an effect instance before its first particles, PL 8.1), gives an object's box with infinite and NaN values. The
arithmetic above is IEEE 754 single precision: each extent *b* − *a* overflows to −∞; an axis entry times −∞ is −∞
for a positive entry, +∞ for a negative one, and NaN for a zero entry of either sign; that NaN counts as **sign bit
set**, so it is added to the min (the original's processor gives a NaN with the sign bit set for an invalid
operation; documented for its SSE instructions); any sum with a NaN is NaN. For every transform with at least one
entry of **i**, **j**, **k** zero or positive, the box test (2.2) of this box gives:

- **visible** without a test when one of its 8 corners has *z* < 10⁻⁵ (2.2 step 1; a NaN *z* does not count);
- otherwise the scan (2.1) reads only cell (0, 0), the top-left one, with threshold *t* = 0: **visible** when that
  cell's value is greater than 0 (as an empty cell's is), **hidden** when it is 0 or negative.

The rectangle and depth do not depend on the other corners: in every such case the smallest corner (**a**) has an
infinite or NaN coordinate, so a scanned box's rectangle edges and depth are all NaN, which the original turns into
the cell indices 0 and the threshold 0 [E4]. The result is cached like any other (section 5): a visible result
takes a generator value and sets *N* = *F* + 10 + (*k* mod 15), a hidden one sets *N* = *F* + 1. With the identity
axes and a translation of magnitude below 10³⁰ the object's box is NaN on every axis at the min and
3.4028235 × 10³⁸ on every axis at the max, so only its largest corner can be below the near limit; with A all zero
(section 1) that corner has *z* = 0 and the box is visible without a test [E4].

## 7. Frustum tests: no margin

For VS's second review gap: the frustum tests made before the occlusion test use **no margin or tolerance**; their
comparisons are exact in single precision against the planes of VP-spec 5.1 (unit normal **n**, offset *d*; a point's
value **n** · **p** + *d*, positive outside) [B7]:

- **Sphere** (objects', lights' and glows' spatial spheres against each frustum their sector was reached with, VS 5;
  also each entry of the spatial query): outside when **n** · **c** + *d* > *r* for some plane (strictly); otherwise
  it passes [B7].
- **Sphere and box** (static visuals, detail groups and entries): for each plane still to test, with
  *s* = **n** · **c** + *d*: *s* > *r*: outside; |*s*| < *r*: the box decides: outside when its corner of smallest
  plane value has a value > 0, the plane passed wholly when its corner of largest value has a value ≤ 0, else the
  plane remains; otherwise (|*s*| ≥ *r*, including *s* = *r* exactly) the plane is passed wholly [B7, M3]. The
  corners are chosen by the signs of the normal's components (a sign bit set counts as negative) [B7, M3]. The
  result is outside, partly inside (some plane remains) or wholly inside (none remains); a wholly-inside node's
  children are not frustum-tested (section 6).
- **The spatial query** of the camera pass (VS 5) tests the index's nodes as boxes of the node's centre ± **twice**
  its half-size, and each entry of an accepted node by its sphere as above [B7]. Whether every entry's sphere lies
  within its node's doubled box is the index's insertion rule (spec V6, **unestablished** here).

## 8. Notes for an implementation

- A renderer without occlusion draws a superset (VS 8); with it, the original's results follow from: the grid (OG),
  A (section 1), the scan (2.1), the untested-visible rules (2.2, 2.3, 3, 4, 5) and the cache with the shared
  generator (5).
- Matching the original's random counts needs the shared generator's sequence, taken in the original's order by
  every user (OG 4.1, DO, section 5); the seed is unestablished.
- Edge rounding, the threshold and the projection's single-precision arithmetic (2.4) decide only cases within
  rounding of a cell edge or of a stored depth.

## 9. Worked examples

All use **VP** of VP-spec 2 with fov 90, aspect value 1, *n* 0.2, *f* 100 and the identity view (camera at the
origin looking along +z, +y up), so 64 *u* = 32 (x/z + 1) and 64 *v* = 32 (1 − y/z); and, unless stated, the grid of
OG example 1 (a square at view depth 10 written in rows 18 … 42 at columns 19 … 47, each cell 1,054,375,872 = depth
0.98196405; columns 18 and 48 empty in rows 20 … 40). Values from the model [M4]; the box and polygon results also from
the shipped routines [M1, M4].

**Example 1: behind and in front** [M4]. Box (−1, −1, 12) … (1, 1, 14): 64 *u* from 29.333334 to 34.666668 (both
ways), cells 29 … 35 in rows and columns; *d* = 0.985304 (view depth 12), *t* = 1,057,962,113; every cell read holds
1,054,375,872 ≤ *t*: **hidden**. Box (−1, −1, 8) … (1, 1, 9): cells 28 … 36, *d* = 0.976954, *t* = 1,048,996,289:
**visible**.

**Example 2: rounding of edges** [M4]. Box (−5.025, −3.075, 12) … (5.775, 4.425, 13): 64 *u* 18.6 … 47.4, 64 *v*
20.2 … 40.2; columns 19 … 47, rows 20 … 40. The box reaches 0.4 cell into the empty column 18, but column 18 is not
read: **hidden**. Box (−4.425, −3.075, 12) … (5.85, 4.425, 13): 64 *u* 20.2 … 47.6, so columns 20 … 48; it does not
reach column 48 (which starts at 48) but column 48, empty, is read: **visible**. In single precision
64 *u* = 0.49999997 gives *s* = 1.0 and index 1.

**Example 3: the near limit** [M2, M4]. Box (−1, −1, 0.1) … (1, 1, 14): four corners at view depth 0.1, *z* < 10⁻⁵:
**visible** without a test.

**Example 4: the depth comparison** [M1]. Box (−0.1, −0.1, 12) … (0.1, 0.1, 12.2), *d* = 0.985304,
*t* = 1,057,962,113, against a grid of −5 everywhere except one cell in its rectangle: that cell 1,057,962,112 or
1,057,962,113: **hidden**; 1,057,962,114: **visible**.

**Example 5: portals** [M4]. The polygon (−2, −2, 12), (2, −2, 12), (2, 2, 12), (−2, 2, 12): *u* and *v* from
0.416667 to 0.583333, *d* = 0.985304: **hidden**. The same at *z* = 8: **visible**. The polygon (−2, −2, 0.15),
(2, −2, 12), (2, 2, 12): a point below the near limit: **visible** without a test.

**Example 6: the box test with a camera check** [M4]. With **E** = (0, 0, 0): the box of example 1 behind the square
is tested: **hidden**; the box (−1, −1, −1) … (1, 1, 14) contains **E**: **visible**; any box while occlusion is
off: **visible**.

**Example 7: the cache** [M4]. A visual with *N* = 0, occlusion on, generator state 12345 (values 7584, 19164, …,
OG example 3). Frame 100: tested, visible; 7584 mod 15 = 9, *N* = 119. Frames 101 … 118: visible untested. Frame
119: tested, hidden, *N* = 120; reached again in frame 119: visible untested. Frame 120: tested, visible;
19164 mod 15 = 9, *N* = 139.

## 10. Unestablished

- The tests' matrix before its first build (believed zero) and whether a level load resets it (section 1).
- Which parts of the program use the three tests offered outside the renderer (2.5).
- The seed of the shared generator, and so the actual random counts in play (section 5).
- A light's next-test frame when it is made; whether two objects can share one visual's cached values (section 5).
- Whether every entry of the spatial index lies within its node's doubled box (section 7; spec V6).
- The box test of an empty box under a transform whose nine axis entries are all negative (section 6).

## 11. Provenance

- Date: 2026-11-25; section 6 "An empty box" added 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\xrRender_R1.dll`,
  `bin\XR_3DA.exe`, `bin\xrGame.dll`.
- Data examined: none beyond the binaries (the examples are synthetic).
- Analysis: static analysis of the shipped binaries (headless decompilation and disassembly, read-only; constants and
  section layout read from the binaries); an interpretation of the shipped test routines' and the object-box code's
  instructions on synthetic inputs (including empty boxes, with the processor's documented NaN results), compared
  with a single-precision model of the described behaviour; arithmetic.

## Gaps found in review

- Sections 7 and 10 keep unestablished whether every entry of the spatial index lies within its node's doubled box;
  `docs/behavior/object-visibility.md` 2.4 establishes it (every entry except those kept at the root), and these
  sections do not reference it.
