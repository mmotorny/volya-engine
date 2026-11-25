# Detail level and size thresholds of objects (first renderer)

Behaviour specification of what `r__geometry_lod` and the per-frame size thresholds derived from it do to
**objects** (the online objects of `docs/behavior/level-objects.md`, whose visuals are skeleton models) when
the first (static-lighting) renderer runs: which of a model's meshes are skipped as too small, which detail
window a skinned progressive mesh draws, when a model would be replaced by a simpler visual, and the other
places the same thresholds act that no approved specification describes (the order in which pass groups are
drawn, and surface marks). What could not be established is marked **unestablished**; what is inferred
rather than read from the program or the data is marked **believed**.

Builds on (not repeated here):

- `docs/formats/level.md` section 9.4 (here "LV 9.4"): the screen measure *S* and the thresholds *T*\_min,
  *T*\_A, *T*\_B, *P*\_lo, *P*\_hi.
- `docs/formats/ogf.md` (here "OGF"): the visual header (section 2), the detail windows and their selection
  (section 6), the static visuals' use of the thresholds (section 8).
- `docs/behavior/level-objects.md` (here "LO"): model files (section 4), the object transform *X* (3.4),
  child meshes and their windows (4.5), shading and the element choice (5).
- `docs/behavior/static-shading.md` (here "SS"): drawing order per frame (10.3), selection (10.2).

Conventions as in those specifications: little-endian; matrices in the row-vector convention (a point
**p** is transformed as [**p** 1] · *X*); clamp(*x*, *a*, *b*) limits *x* to *a* … *b*. "The camera
position" **E** is the eye position of the frame. A model's **parts** are its child meshes (LO 4.5, types 5
and 4). The **measure** of a sphere (centre **C**, radius *r*) is *m* = *r* / (|**E** − **C**|² + 10⁻⁵).

**Checks.** Bracketed ids [ODL-*n*] name the checks of the dirty workspace's script
`odl/checks/run.py` (helpers `lib2.py`, `scan.py`); it reads the shipped binaries (constants, code
references, read-only static analysis) and every `.ogf` file extracted from the archives. All checks pass.

## 1. Thresholds and when a change takes effect

- Once per frame, at the start of the renderer's visibility pass, the thresholds of LV 9.4 are derived from
  the current `r__geometry_lod`, `r1_ssa_lod_a`, `r1_ssa_lod_b`, field of view and render-target size
  [ODL-1]. `r__geometry_lod` is read nowhere else [ODL-1]. A new value therefore takes effect at once: the
  next frame's tests use it.
- The same pass derives one more value, the **order threshold** *T*\_O = 1024 / *S*, used only to order
  pass groups (section 5) [ODL-1]. A further value, (8 / 0.75)² / *S*, is derived every frame and used by
  nothing [ODL-1].
- Every measure below is recomputed every frame from the camera position of that frame (sections 2 – 4).

With the defaults (`r__geometry_lod` 1, `r1_ssa_lod_a` 64, `r1_ssa_lod_b` 48): *T*\_min = 12.25 / *S*,
*T*\_B = 256 / *S*, *T*\_A ≈ 455.11 / *S*, *P*\_lo ≈ 455.11 / *S*, *P*\_hi ≈ 7281.78 / *S*,
*T*\_O = 1024 / *S*. Raising `r__geometry_lod` raises *S* and so lowers every threshold: smaller or more
distant parts are drawn, and at higher detail.

## 2. A model: replacement visual

When an object's model (type 10 or 3) is drawn [ODL-2]:

1. If the model has a **replacement visual** and the model's measure computed with half its radius,
   0.5 *R* / (|**E** − **C**′|² + 10⁻⁵), is below *T*\_A, the replacement visual is drawn in place of the
   model (by the rules of sections 2 and 3 applied to it) and nothing else of the model is drawn. **C**′ is
   the centre of the model's bounding sphere transformed by *X*, *R* its radius.
2. Otherwise the model's parts are drawn by section 3.

A model has a replacement visual only when its file has a top-level chunk 23, a name string naming the
visual [ODL-2]. **No shipped model file has chunk 23** (all 1,581 `.ogf` files of the archives) [ODL-3], so
in the shipped game rule 1 never applies and every drawn model goes on to its parts. The model's own bounding
sphere used in rule 1 is its header sphere, which the program re-fits to the posed bones from time to time
[ODL-4]; the re-fitting rule is **unestablished** and does not matter for the shipped data.

## 3. A model part: the small-part test

This answers LO 5.1. Each part of a drawn model is tested on its own [ODL-4]:

- Its sphere is the part's header sphere (chunk 1, bytes 28 – 43: centre and radius, OGF 2), in model space,
  as stored in the file [ODL-4]; the centre is transformed by the object transform, **C**′ = [**C** 1] · *X*;
  the radius *r* is used as stored (not scaled by *X*) [ODL-4].
- Its measure is *m* = *r* / (|**E** − **C**′|² + 10⁻⁵) [ODL-4].
- The part is **not drawn** when *m* ≤ *T*\_min; otherwise it is queued for drawing with *m* kept for
  section 4 [ODL-4].
- A part is queued at most once per traversal of the scene [ODL-4]. The test comes before the part is sorted
  into the frame's lists, so it applies equally to the first-person view's lists [ODL-4] (the transform the
  first-person models carry is outside this specification).

The test is the one static meshes use (OGF 8.3), applied per part rather than per object: of one model, a
small part (e.g. a separate mesh with a small sphere) can be skipped while larger parts are still drawn.
Solved for the distance, a part of radius *r* is drawn while |**E** − **C**′|² < *r* · *S* / 12.25 − 10⁻⁵
(defaults).

## 4. Detail level of a part

This answers LO 4.9. When a queued part is drawn, its **detail value** is
*q* = √(clamp((*m* − *P*\_lo) / (*P*\_hi − *P*\_lo), 0, 1)), from the measure kept in section 3 [ODL-5]. The
same value is computed for the non-sorted passes and for the lists drawn entry by entry: the strict-sorted
passes and two further lists (**believed**: among them the first-person view's) [ODL-5]. It is the formula of OGF 8.3 for static progressive
meshes.

- A **skinned progressive mesh** (type 4) draws window *j* of its chunk 6 (LO 4.5), with *n* the number of
  windows: *j* = floor((1 − *q*) × (*n* − 1) + 0.5) [ODL-6], exactly as OGF 6 for static progressive
  meshes [ODL-6]. Window *j* is drawn with its own offset, triangle count and vertex count (OGF 6) [ODL-6].
  So *q* = 1 (*m* ≥ *P*\_hi) draws window 0, the full mesh; *q* = 0 (*m* ≤ *P*\_lo) draws window *n* − 1,
  the coarsest.
- A **skinned mesh** (type 5) always draws its whole index list [ODL-6].

Model files: 1,160 type-4 parts in 526 files; 252 of them have a single window (they draw the same
triangles at every detail value); the largest has 4,069 windows [ODL-3].

With the defaults, *q* reaches 1 when |**E** − **C**′|² ≤ *r* · *S* / 7281.78 − 10⁻⁵ and is 0 beyond
|**E** − **C**′|² ≥ *r* · *S* / 455.11 − 10⁻⁵.

## 5. Order of pass groups

When the frame's groups of non-sorted passes are ordered for drawing (SS 10.3 step 1 and the matching step
for objects), the order threshold *T*\_O decides [ODL-7]:

**Disputed in review:** not only step 1: the same ordering and drawing of static and object groups runs
for both stages of non-sorted passes, SS 10.3 step 1 (priority 0 – 1) and step 5 (priority 2 – 3).

- groups whose measure is above *T*\_O are drawn first, larger measure first [ODL-7];
- groups whose measure is at most *T*\_O are drawn after them, in an order by their pass states (the
  comparison is **unestablished**) [ODL-7].

The same ordering is used for the static groups and the object groups [ODL-7]. A group's measure is
**believed** to be the largest measure of the parts queued into it in the frame. In some cases decided by the
group structure all groups are ordered by measure alone [ODL-7]; which cases those are is **unestablished**.
`r__geometry_lod` therefore moves groups between the two halves. This changes only the drawing order; for
opaque, depth-tested passes it is **believed** not to change the picture.

## 6. Surface marks

Marks (decal geometry laid onto the level's surfaces and onto models; **believed** to be added by the game
at run time — their creation, shape and lifetime are outside this specification) are subject to one size
test, for both kinds [ODL-8]: with *r* the mark's bounding-sphere radius and *D* the distance from the camera
position to its centre, a mark is drawn only when *r*² / *D*² ≥ *T*\_min / 4 (no 10⁻⁵ term). The test uses
the current *T*\_min, so `r__geometry_lod` changes which marks are drawn at once.

## 7. Other uses of the thresholds (already specified)

The remaining uses of *T*\_min, *T*\_A and *T*\_B are in approved specifications [ODL-9]:

| Use | Specification |
|---|---|
| Static meshes, LOD billboard groups (cross-fade and fade), progressive detail | OGF 8, SS 10.2, LV 9.4 |
| Portal size test and portal fade | LV 9.2 |
| Detail objects (grass) | `docs/behavior/detail-objects.md` |
| Particle effects (same part test as section 3) | `docs/behavior/particle-drawing.md` 7.2 |
| Dynamic light passes, shadow casters (repeat of the camera's test) | `docs/behavior/dynamic-lights.md`, `docs/behavior/light-shadows-projection.md` |

## 8. Unestablished

- The re-fitting of a model's own bounding sphere to its bones (section 2); it matters only for replacement
  visuals, which no shipped file has.
- The state comparison ordering the small pass groups, the cases in which all groups are ordered by measure,
  and the exact measure of a group (section 5).
- How marks are created and placed (section 6).
- The transform of first-person models (section 3).
- Nothing was executed: all statements come from static analysis of the binaries and the data.

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; binaries `bin\xrRender_R1.dll` and `bin\XR_3DA.exe`; the model files `meshes\**\*.ogf` of the
  archives `gamedata.db0` … `gamedata.dbd` extracted with the project's archive tool (1,581 files).
- Binary analysis: static analysis with Ghidra (headless, read-only decompilation and disassembly) of the
  renderer's per-frame threshold derivation, its traversal of dynamic visuals, its queueing and drawing of
  parts, the skinned meshes' drawing, the ordering of pass groups and the drawing of marks, and of the
  engine's model loading, visual header loading and bone update; constants and code references read
  directly from the files. No execution or debugging.
- Data analysis: chunk structure of every model file (top-level chunk ids per type, type-4 windows).
- Approved specifications of the repository read for coverage (section 7).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

None.
