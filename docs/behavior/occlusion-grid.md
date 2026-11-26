# Occlusion grid: drawing the occluders each frame

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the first renderer
(`bin\xrRender_R1.dll`), of how the triangles of a level's occlusion mesh are drawn each frame into the small
depth grid that occlusion tests read: when the grid is drawn, which triangles are drawn and in which order, their
facing, clipping and projection, how one triangle is turned into grid cells and their depths (including the use of
neighbour triangles at shared edges), the finishing pass, the depth encoding, the random skip counts and the
coarser grids. It is specification V3 of the scoping specification VS (section 9) and replaces VS 3.2 where they
differ. How shapes are tested against the grid is specification V4.

Conventions: matrices act on row vectors; **VP** is the frame's view-projection (VP-spec section 1); for a point
**p**, (x, y, z, w) = (**p**, 1) · **VP**. The camera position is **E**. "Depth" in the grid is z / w of a point
(VP-spec section 2: 0 at the near value *n*, 1 at the far value *f*); smaller is nearer. The **frame number** is a
counter the engine advances by one at the start of each frame [G3]. All arithmetic of the original is single
precision (IEEE 754 `f32`). **Disputed in review:** not all of it: the normalisation of frustum planes (so the
near-side clipping plane of section 4.3) takes the square root, the reciprocal and the products in the x87
floating-point unit and rounds to single precision only when storing each component; the x87 precision in
effect there is **unestablished**, so the plane can differ from a single-precision computation in the last bit. floor and ceil are the usual functions, except that the original turns −0.0 into
floor −1 and ceil 1 [N1]. Abbreviations of approved specifications: VS `docs/behavior/visibility-scope.md`, HOM
`docs/formats/level-hom.md`, VP-spec `docs/behavior/view-projection.md`.

**Checks.** Every claim carries a tag. The checks are scripts in `checks/occlusion-grid/` of the dirty workspace
(`run.py` runs them all): `binary.py` [G…] reads saved read-only static analysis (headless decompilation and
disassembly) of `bin\xrRender_R1.dll`, `bin\XR_3DA.exe`, `bin\xrCDB.dll`, `bin\xrCore.dll` and `bin\xrGame.dll`,
and constants from the binaries' data; `model_checks.py` [N…, M…] checks numeric facts and the worked examples
of section 9, which are computed by `model.py`, a single-precision model of the drawing written from that analysis
(not a measurement of the running game). All checks pass.

## 1. Where the grid lives

- The drawing works on two buffers of 68 × 68 cells, row by row from the top: a **depth buffer** of reals and an
  **owner buffer** that names, per cell, the triangle that last wrote it, or none [G1, G12]. Buffer cell (row *r*,
  column *c*) covers the screen square *c* ≤ *s*ₓ < *c* + 1, *r* ≤ *s*ᵧ < *r* + 1 in **buffer coordinates**
  (the screen coordinates of section 4.3 shifted by +2, section 5), which put the 64 × 64 screen at rows and
  columns 2 … 65.
- The **grid** that tests read is 64 × 64 signed 32-bit integers; grid cell (*i*, *j*) is buffer cell
  (*i* + 2, *j* + 2), row *i* counted from the top of the screen and column *j* from the left [G13, G9]. The
  border of two buffer cells on every side receives the parts of triangles just off screen (sections 5, 6) and is
  read by the finishing pass (section 7.1).

## 2. When the grid is drawn

- **Each camera pass** (VS 1: one per frame, the level's render step runs the camera pass and then draws the
  frame [G2]) draws the grid right after building its view frustum (left, right, top, bottom and far planes of
  **VP**, VP-spec 5.2), whether or not the grid was already drawn in this frame [G2].
- **Early drawing.** An occlusion test made when the grid has not been drawn in the current frame (its recorded
  frame number differs from the current one) first draws it, with a frustum built the same way from the **VP** of
  that moment [G2]. This is skipped while the **main menu is shown**: the test then uses the grid as it is [G2,
  G3]. ("Shown" is the menu's own active state, set when the menu is opened and cleared when it is closed [G3].)
  The early drawing is serialised by a lock, so tests from several threads draw it at most once at a time [G2].
- So a frame can draw the grid twice: early, for a test made before the camera pass, and again in the pass. The
  second drawing starts from cleared buffers but sees the skip counts the first one set (section 3) [G2, G5, G6].
- Drawing does nothing when the level has no occlusion mesh (HOM 1); otherwise it records the current frame
  number as the frame the grid was drawn in [G1].
- Each drawing: clear both buffers (every depth 1.0, every owner none) [G1]; draw the selected triangles in order
  (sections 3 – 6); finish (section 7) [G1].

## 3. Which triangles

- **Candidates** come from the occlusion mesh's search structure (HOM 3, step 4) queried with the frustum of
  section 2: the structure's nodes are tested against the frustum's planes, and every triangle of every leaf that is
  not rejected is a candidate, without a test of the triangle itself [G4]. So every triangle that meets the frustum
  is a candidate, and some that do not may be too; which ones depends on how the structure is built, which is
  **unestablished** here. Such extra candidates can still write cells (in the border, edge cells next to cells a
  neighbour owns, section 6) and each takes a random value (section 4.1), so the exact set matters for matching
  the original exactly.
- **Skip counts.** Each triangle keeps a frame number, 0 when the level loads (HOM 3, step 3). A candidate is
  dropped when that number is greater than the current frame number (compared unsigned); the rest are kept [G5].
- **Order.** The kept candidates are sorted by the squared distance from **E** to the triangle's **centre**
  (HOM 3, step 3: ((**v**₀ + **v**₁) + **v**₂) × ⅓ of the merged vertices), **ascending: nearest first** [G5].
  The squared distance is (Eₓ − cₓ)² + (Eᵧ − cᵧ)² + (E_z − c_z)². The sort is not stable (partitioning, with
  insertion sort for short runs and heap sort past a depth limit) [G5], so the order of triangles with equal keys
  depends on the order the search returns them: **unestablished**.

## 4. Each triangle

For each kept triangle in order [G6, G7, G8, G9]:

### 4.1 The random skip count

- First, one value *k* is taken from the engine's shared random generator, for every kept triangle whether or
  not it is then skipped [G6]. The generator is shared with other parts of the renderer and the engine [G6]: its
  state *s* (32 bits) advances as *s* ← *s* × 214013 + 2531011 (mod 2³²) and the value is (*s* >> 16) & 0x7fff
  (0 … 32767) [G6, N2]. Its seed is not needed here and is **unestablished**.
- The **skip frame** is *F* + 3 + (*k* mod 7), *F* the current frame number [G6]. When the triangle is skipped
  (below) its frame number becomes the skip frame; when it is drawn its frame number is left unchanged [G6]. A
  triangle skipped in frame *F* is therefore dropped from every drawing in frames *F* … *F* + 2 + (*k* mod 7)
  (including a second drawing in frame *F* itself) and is a candidate again from frame *F* + 3 + (*k* mod 7): it
  misses the next 2 … 8 frames (VS 3.2's "3 … 9" counts differently) [G5, G6, M3].

### 4.2 Facing

- A triangle whose two-sided flag is non-zero is always drawn; one with flag 0 is drawn only when **n** · **E** +
  *d* > 0 (strictly), with the plane (**n**, *d*) of HOM 2.1 computed from the merged vertices; otherwise it is
  **skipped** [G7, M4].

### 4.3 Clipping and projection

- The triangle (**v**₀, **v**₁, **v**₂ of the merged mesh, in record order) is clipped as a polygon against one
  plane only, the near side of **VP**: the plane whose normal and offset (*n*ₓ, *n*ᵧ, *n*_z, *d*) are minus the
  sum of the w column and the z column of **VP**, normalised to a unit normal (VP-spec 5.1, bit 0x10: inside when
  z ≥ −w; at view depth *n* · *f* / (2*f* − *n*), about 0.1 for *n* = 0.2) [G8]. No side or far clipping is done [G8].
- Clipping rule [G8]: the plane value of a point is **n**·**p** + *d*; a point is inside when that value is negative
  (its sign bit is set, so −0.0 is inside and +0.0 outside). The polygon's edges are taken in order (**p**ᵢ to
  **p**ᵢ₊₁, the last back to the first); an edge whose two ends differ by less than 10⁻⁷ on every axis is ignored
  (neither end nor a crossing is emitted for it); otherwise an inside **p**ᵢ is emitted, and when exactly one end is
  inside the crossing point **p**ᵢ + *t*(**p**ᵢ₊₁ − **p**ᵢ), *t* = −value(**p**ᵢ) / (**n**·(**p**ᵢ₊₁ − **p**ᵢ)), is
  emitted after it (no point when that denominator is 0). Fewer than three points left: the triangle is
  **clipped away** and **skipped** [G8, M5].
- Each point of the clipped polygon is mapped to **buffer-ready screen coordinates** with the matrix whose four
  columns are 32 (x + w), 32 (w − y), z and w of **VP**, divided by its own w [G9]:
  *s*ₓ = 32 (x/w + 1), *s*ᵧ = 32 (1 − y/w), *s*_z = z/w. So the screen runs from 0 to 64 in both directions, *s*ᵧ
  downwards from the top edge [G9].
- The polygon (**P**₀, **P**₁, …, **P**ₘ₋₁) is drawn as the fan of triangles (**P**₀, **P**ₖ, **P**ₖ₊₁),
  *k* = 1 … *m* − 2, in that order, each by section 5 [G9]. The number of cell writes (section 6) of all of them
  is summed; when it is 0 the triangle **covers no cell** and is **skipped** [G9, M3]. A cell write counts
  whether the cell is on the screen or in the border, and a later write of the same cell counts again [G9, G12].
  A triangle entirely hidden behind triangles drawn before it writes no cell and is skipped like one off screen
  [M3].

## 5. Rows of one triangle

For one triangle of the fan, with screen points (*s*ₓ, *s*ᵧ, *s*_z) [G10, G11]:

- The three points are ordered by *s*ᵧ into **top A**, **middle B** and **bottom C** (points with equal *s*ᵧ
  keep their listed order **P**₀, **P**ₖ, **P**ₖ₊₁) and their x and y are shifted by +2 (into buffer coordinates); depths are unchanged [G10].
- A **row** *r* is the strip *r* ≤ y ≤ *r* + 1 of the buffer. Rows are drawn in two **halves**: the upper half uses
  the edges A→B and A→C, the lower half A→C and B→C [G10]. Which half draws the row that contains B depends on the
  fractional part *φ* = B.y − floor(B.y): with *φ* > 0.5 the upper half takes it, otherwise the lower half does
  [G10]. With *t* = 1 for the half that takes it and 0 for the other:
  - upper half: rows ceil(A.y) … floor(B.y) − 1 + *t*, and one row less when that last row is ≥ floor(C.y);
  - lower half: rows ceil(B.y) − *t* … floor(C.y), but starting no higher than ceil(A.y) (when ceil(B.y) − *t* <
    ceil(A.y) the first row is one lower).
  Both ends are clamped to 0 … 67, and a half draws nothing unless its first row is **strictly** less than its
  last; otherwise it draws every row from first to last inclusive [G10]. (So a half of a single row draws
  nothing, and with B.y a whole number the row B.y − 1 can be drawn by both halves.)
- In each half, one edge is the **left** one: in the upper half A→B when the slope of A→C (its x change per row) is
  greater than that of A→B, otherwise A→C; in the lower half A→C when its slope is greater than that of B→C,
  otherwise B→C [G11]. The other edge is the **right** one.
- For each edge P→Q of a half, at row *r*: x(*r*) = ((*r* − P.y) × (1 / (Q.y − P.y))) × (Q.x − P.x) + P.x, and
  z(*r*) likewise with the depths; per row x advances by the slope (Q.x − P.x) × (1 / (Q.y − P.y)) and z by
  (Q.z − P.z) × (1 / (Q.y − P.y)). The first row's values are computed this way and later rows by adding the
  per-row steps [G11]. On row *r* an edge's **extent** is the range of its x over the strip: from x(*r*) to
  x(*r*) + slope, given to the row as its centre x(*r*) + slope/2 and half-width slope/2 [G11].

## 6. Cells of one row

On row *r*, with the left edge at x_L = x(*r*) and depth z_L, the right edge at x_R and depth z_R, the left edge's
extent [L₋, L₊] and the right edge's [R₋, R₊] (each extent as the smaller and the larger of centre ± half-width)
[G12]:

- **Range.** *c*₀ = floor(L₋) − 1 and *c*₁ = ceil(R₊) + 1, each clamped to 1 … 66. Nothing is drawn when
  *c*₀ ≥ *c*₁. Then *a* = ceil(L₊) and *b* = floor(R₋), each clamped to 0 … 67, *lo* = min(*a*, *b*),
  *hi* = max(*a*, *b*) [G12].
- **Depth.** The depth at column *c* is the linear interpolation along the row between (x_L, z_L) and (x_R, z_R),
  taken at x = *c*: in exact terms, the triangle's screen-space depth at the top-left corner (*c*, *r*) of the cell
  [G12, M6]. The original computes it at *c*₀ and *c*₁ as ((*c* − x_L) × (1 / (x_R − x_L))) × (z_R − z_L) + z_L,
  then steps by (z(*c*₁) − z(*c*₀)) / (*c*₁ − *c*₀) per column, adding from *c*₀ rightwards and subtracting from
  *c*₁ leftwards [G12].
- **Interior.** Every cell *c* with max(*c*₀, *lo*) ≤ *c* < *b* is written when its depth is strictly less than the
  stored depth [G12]. These are the cells lying wholly between the two edges' extents.
- **Left edge cells**, *c*₀ ≤ *c* < *lo*, taken left to right: cell *c* is written only when the cell to its left
  (*c* − 1) is owned by this triangle or by one of its neighbours (HOM 3, step 2; a missing neighbour matches no
  cell), and then with the depth (2 × depth(*c* − 1) + depth at *c*) × ⅓, when that is strictly less than the
  stored depth [G12, G7].
- **Right edge cells**, *hi* ≤ *c* ≤ *c*₁ − 1, taken right to left: the same with the cell to the right (*c* + 1)
  [G12].
- Cells with *lo* ≤ *c* < *hi* not covered by these rules (when *a* > *b*, the extents overlap) are not written
  [G12].
- **A write** stores the depth, makes this triangle (the occluder triangle of the mesh, for every triangle of
  its fan) the owner and counts one write [G12].

So a triangle alone covers only the cells inside its edges (one cell of margin to each side is examined); the
cells its edges cross are filled only next to cells already owned by itself or by a neighbour, which closes the
seams between neighbouring triangles drawn one after another [M1, M2]. The first of two neighbours leaves its edge
cells empty; the second fills them from the first's cells (with a depth weighted two to one towards them) [M1].
The owner rule is the **use of neighbour triangles at shared edges**; triangles that are not linked as neighbours
leave a crack of empty cells between them [M2].

## 7. Finishing

### 7.1 The gap fill

After all triangles, the grid cells are visited row by row from the top and, within a row, left to right; for
grid cell (*i*, *j*), let X be its buffer cell, U the cell above it, D₁ the cell below and D₂ the one below that
[G13]:

- if U has an owner *T*, and D₁ is owned by *T* or one of *T*'s neighbours, the candidate depth is
  (depth(U) + depth(D₁)) × 0.5; otherwise, if D₂ is owned by *T* or one of its neighbours, it is
  (depth(U) + depth(D₂)) × 0.5; otherwise there is none;
- when the candidate is strictly less than depth(X), X takes it and *T* becomes X's owner [G13, M7].

Because X is changed in place, a filled cell serves as U for the cell below it in the next row, so a gap of two
cells is filled in two steps [G13]. Cells written by a triangle can also be lowered by this rule, on a plane by rounding-level
amounts (in example 6, 16 cells by at most 6 × 10⁻⁸) [M6].

### 7.2 Encoding

Each grid cell's depth is clamped to −1.99 … 1.99 (a value that compares false, such as NaN, becomes −1.99),
multiplied by 2³⁰ and rounded down to a signed 32-bit integer (floor; −0.0 gives −1) [G13, N1]. An empty cell
(depth 1.0) is 2³⁰ = 1,073,741,824; depth 0.5 is 536,870,912. The grid is complete for this drawing only after
this step [G13].

### 7.3 The coarser grids

From the 64 × 64 grid, grids of 32 × 32, 16 × 16 and 8 × 8 integers are made, each cell the largest (farthest) of
the four cells under it in the next finer grid [G13]; so a coarse cell is the farthest of all 64-grid cells it
covers. **Nothing reads them**: the only references to them are in the step that makes them, and no other place in
the renderer holds an address inside them [G14]. The tests read the 64 × 64 grid [G14]. An implementation
needs no coarse grids.

## 8. Notes for an implementation

- The original's arithmetic is single precision throughout (**Disputed in review:** except the frustum plane
  normalisation, see the conventions); the floor and ceiling of coordinates make coverage
  sensitive to the last bit of a vertex that lands on a whole cell boundary, and the gap fill to rounding of
  depths. The examples avoid boundaries.
- Rows clamp to the buffer (0 … 67) and columns to 1 … 66 (sections 5, 6): the interior cells of geometry off screen
  fall in the two-cell border, and its edge cells can reach one cell further.
- Drawing a triangle leaves its skip frame as it was; only skipping changes it (4.1).

## 9. Worked examples

All use **VP** of VP-spec 2 with fov 90, aspect value 1, *n* 0.2, *f* 100 and the identity view (camera at the
origin looking along +z, +y up, +x to the right; so *s*ₓ = 32 (x/z + 1), *s*ᵧ = 32 (1 − y/z)), **E** = (0, 0, 0),
two-sided triangles unless stated, the candidates given in the order listed, and fresh skip frames (0). "Rows" and
"columns" are grid cells; "written" means the encoded value is below 2³⁰. Computed with the model [M…]; values agree
with an exact plane depth to 2 × 10⁻⁷ [M6].

**Example 1: a square of two neighbouring triangles** [M1]. Triangle 0: (−4.3, −3.7, 10), (5.1, −3.7, 10),
(5.1, 4.9, 10); triangle 1: (−4.3, −3.7, 10), (5.1, 4.9, 10), (−4.3, 4.9, 10). They share the diagonal edge and
are each other's neighbour (triangle 0's edge 2, triangle 1's edge 0). On screen the square spans *s*ₓ 18.24 …
48.32, *s*ᵧ 16.32 … 43.84, depth 0.98196405 (in single precision). Triangle 0's centre is nearer, so it is drawn
first (353 writes), then triangle 1 (399). Written: row 17 columns 19 … 45; rows 18 … 42 columns 19 … 47; 752
cells, each 1,054,375,872. (Row 17's last two cells, 46 and 47, are left edge cells of triangle 0;
when triangle 0 was drawn, the cell to their left had no owner yet; for triangle 1 they are right edge cells
whose right-hand neighbour cell has no owner.) The 32-cell grid: cell (15, 15) = 1,054,375,872, cells (8, 9) and
(8, 10) = 2³⁰; the 8-cell grid has 6 written cells, rows 3 – 4, columns 3 – 5.

**Example 2: the same without neighbour links** [M2]. As example 1, but the triangles are not neighbours: 700 cells
written; the diagonal leaves a crack of two or three empty cells per row: row 18 columns 19 … 44 and 47; row 30
columns 19 … 31 and 34 … 47; row 42 columns 21 … 47.

**Example 3: a hidden triangle and its skip count** [M3]. Example 1's square plus triangle 2: (−2, −2, 20),
(2, −2, 20), (0, 2, 20), candidates listed 2, 1, 0, frame 100, generator state 12345 (values 7584, 19164, 25795
[N2]). Sorted: 0, 1, 2. Triangles 0 and 1 are drawn as in example 1; triangle 2 lies wholly behind them, writes no
cell and is skipped: 25795 mod 7 = 0, skip frame 100 + 3 + 0 = 103. In frame 101 (and 102) it is dropped before
sorting and takes no random value; triangles 0 and 1 keep skip frame 0.

**Example 4: facing** [M4]. A one-sided triangle (−2, −2, 10), (2, −2, 10), (0, 2, 10): **n** = (0, 0, 1),
*d* = −10, **n**·**E** + *d* = −10, so it is skipped; with frame 50 and generator state 7 the skip frame is 58. With
its vertices in the order (−2, −2, 10), (0, 2, 10), (2, −2, 10), **n** = (0, 0, −1), *d* = 10 and it is drawn.

**Example 5: near clipping** [M5]. Triangle (−1, 0, 0.05), (1, 0, 0.05), (0, 1, 2), frame 10, generator state 3.
The near-side plane is **n** = (0, 0, −1), *d* = 0.1001001 (view depth 0.1001). Clipped polygon: (0.97431, 0.02569,
0.1001), (0, 1, 2), (−0.97431, 0.02569, 0.1001), one fan triangle; its screen points reach far beyond the screen
(*s*ₓ about 343 and −279) at depth about −1.0. 396 writes; written: rows 17 … 22, all 64 columns.

**Example 6: depth across a triangle** [M6]. Triangle (−3.1, −2.05, 8), (4.07, −1.13, 8), (0.53, 3.41, 12), frame
1. Screen points (19.6, 40.2, 0.976954), (48.28, 36.52, 0.976954), (33.413334, 22.906668, 0.985304). 181 writes;
written rows 24 … 38: row 24 column 33; 25: 32 … 34; 26: 31 … 35; 27: 31 … 36; 28: 30 … 37; 29: 29 … 39; 30:
28 … 40; 31: 27 … 41; 32: 27 … 42; 33: 26 … 43; 34: 25 … 44; 35: 24 … 45; 36: 23 … 46; 37: 23 … 35; 38: 22 … 27.
Encoded values: (25, 33) 1,056,783,488; (30, 28) 1,054,265,792; (30, 33) 1,053,895,168; (30, 39) 1,053,450,560;
(35, 33) 1,051,006,848; (35, 45) 1,050,117,568, each the plane's depth at the cell's top-left corner
(*s*ₓ = *j*, *s*ᵧ = *i*) × 2³⁰.

**Example 7: the gap fill** [M7]. Points p₀ (1.76, 4.63, 10), p₁ (−2.49, −0.44, 10), p₂ (0.93, −1.8, 10),
p₃ (−1.36, −1.87, 10); triangle 0 (p₀, p₁, p₂), triangle 1 (p₀, p₂, p₃), neighbours across p₀ p₂. Triangle 1 is
nearer and drawn first; each writes 46 cells. After drawing, cell (35, 29) is empty, (34, 29) is owned by
triangle 0 and (36, 29) by triangle 1; the gap fill gives (35, 29) the mean depth and owner 0, and it is the only
cell filled. Written: row 20 column 36; 21: 35 … 36; 22: 34 … 35; 23: 33 … 35; 24 – 25: 32 … 35; 26: 31 … 35;
27: 30 … 35; 28: 29 … 35; 29: 28 … 34; 30 – 31: 27 … 34; 32 – 33: 26 … 34; 34 – 36: 29 … 34.

**Example 8: the generator** [N2]. From state 0 the first values are 38, 7719, 21238, 2437, 8855.

## 10. Unestablished

- Which triangles beyond those meeting the frustum the search returns (the build of the search structure)
  (section 3).
- The order of triangles with equal sort keys (section 3).
- The seed of the shared random generator, and so the actual skip counts in play (section 4.1).
- The exact order of additions in the original's single-precision sums (the model's agree with it to rounding);
  results that depend on it are boundary and gap-fill cases only (section 8).

## 11. Provenance

- Date: 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\xrRender_R1.dll`,
  `bin\XR_3DA.exe`, `bin\xrCDB.dll`, `bin\xrCore.dll`, `bin\xrGame.dll`.
- Data examined: none beyond the binaries (the examples are synthetic).
- Analysis: static analysis of the shipped binaries (headless decompilation and disassembly, read-only; constants
  read from the binaries' data); a single-precision model of the described behaviour for the worked examples;
  arithmetic.

## Gaps found in review

- The x87 precision in effect when frustum planes are normalised, and so the exact bits of the near-side
  clipping plane (section 4.3).
