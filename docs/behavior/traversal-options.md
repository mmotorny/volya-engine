# Portal traversal options (first renderer)

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the four options of the
sector traversal through portals in the first (static-lighting) renderer `bin\xrRender_R1.dll`: what each option
does, including the screen-rectangle mode, and which pass uses which (spec V8 of
`docs/behavior/visibility-scope.md`).

Builds on (not repeated): `docs/formats/level.md` (here "LV": portals section 5, the traversal rules 9.2, the
thresholds 9.4), `docs/behavior/visibility-scope.md` (here "VS"), `docs/behavior/dynamic-lights.md` (here "DL",
the light passes 5.3 – 5.4), `docs/behavior/light-shadows-projection.md` (here "LS"), `docs/behavior/portal-fade.md`
(here "PF", spec V9).

Conventions: "the traversal" is that of LV 9.2: it starts in one sector with one frustum and enters further
sectors through portals by rules 1 – 6 there. Every fact carries the id of the check that reproduces it; the checks
are scripts in `checks/traversal-fade/` of the dirty workspace (`run.py` runs them all): `bin_traversal.py` [T1 – T8]
reads `bin\xrRender_R1.dll` by static analysis (read-only decompilation; outputs kept in `out/`);
`bin_portal_frustum.py` [T9 – T13] reads `bin\xrRender_R1.dll` and `bin\XR_3DA.exe` (which holds the frustum
routines the renderer calls) the same way; `model_frustum.py` [M3] evaluates the construction of section 5 on test
polygons. All checks pass.

## 1. Inputs of a traversal

A traversal is started with: the start sector, the start frustum, an eye position, a 4 × 4 matrix (row-vector
convention, clip = (P, 1) · M), and an **options word** of four independent bits [T1, T2]. The options, the eye,
the matrix and the start sector stay fixed for the whole traversal, including every sector entered through
portals [T2]. The eye is the point used by rules 2 and 4 and for the frusta through clipped portals (LV 9.2;
DL 5.4 for the light passes) [T2].

| Bit | Name here | Rule it governs (LV 9.2) |
|---|---|---|
| 1 | occlusion | rule 6 |
| 2 | size | rule 4, the *T*\_min part |
| 8 | fade | rule 4, the *T*\_A collection and the *T*\_B part |
| 4 | screen rectangle | section 3 below |

## 2. The options

### 2.1 Size (bit 2)

With this option the portal's screen measure *m* = *r*² / *D*² × |cos| (LV 9.2 rule 4; *D* measured from the
traversal's eye) is computed for every portal that has passed rules 1 – 3, and the portal is skipped when
*m* < *T*\_min [T4]. Without it no measure is computed and rule 4 is not applied at all [T4, T5].

### 2.2 Fade (bit 8)

Only together with bit 2 (it acts on the measure that bit 2 computes) [T5]:

- a portal with *m* < *T*\_A is appended, with its *m*, to the frame's **fade list** (PF); then
- a portal with *m* < *T*\_B is skipped [T5].

Both happen after the *T*\_min test and before the clipping of rule 5 and the occlusion test of rule 6 [T5]. So
the fade list also takes portals that are then skipped for *m* < *T*\_B, and portals that rules 5 or 6 would skip.
A portal is marked as passed (rule 1) only when the traversal enters its target sector through it; a portal that
is collected but not passed is examined, and collected, again each further time its sector is entered in the
same traversal and the portal passes rules 1 – 3 for that entry's frustum [T5].

Starting a traversal with bit 8 empties the fade list first, whether or not bit 2 is set [T5]. Without bit 8 the
list is neither emptied nor appended to, and the *T*\_B test is not applied [T5].

LV 9.2 rule 4 states the *T*\_B skip without condition; it applies only with bit 8 (and bit 2). For the camera
pass, which has both (section 4), LV 9.2 is right as stated.

The thresholds are those of LV 9.4: *T*\_A from `r1_ssa_lod_a`, *T*\_B from `r1_ssa_lod_b`, *T*\_min = 12.25 / *S*;
the camera pass computes them at its start, and nothing else sets *T*\_A or *T*\_B [T5].

### 2.3 Occlusion (bit 1)

With this option, after rule 5, the clipped portal polygon is tested against the occlusion grid (VS 3.3, the
polygon shape) and the portal is skipped when the test reports it hidden [T6]. In the screen-rectangle mode the
rectangle test replaces the polygon test (section 3) [T6]. Without bit 1 no occlusion test is made for portals
[T6]. How the polygon and rectangle tests decide is for spec V4 (VS 3.3).

### 2.4 Rule 2 and the touched mark (independent of the options)

Rule 2's second skip compares the target with the current sector and with the traversal's **start sector**, not
with the sector the current one was entered from [T3]: a portal is skipped when its target is the current sector
or the start sector. (LV 9.2 rule 2 says "the sector the current one was entered from"; for a portal into the
start sector reached from a sector two or more portals away the two readings differ.) A portal marked as touched
(LV 9.2 rule 2; set before the traversal by the box around the camera, or around a spot light, DL 5.4) is not
subject to this skip; its mark is cleared when the traversal passes it [T8]. Whether anything else clears the
mark is **unestablished**.

## 3. The screen-rectangle mode (bit 4)

Each entry of a sector carries, besides its frustum, a **screen rectangle** (*x*₀, *y*₀, *x*₁, *y*₁) and a depth
*z*. The start sector is entered with (0, 0, 1, 1) and depth 0 [T7]. Each sector keeps every rectangle it is
entered with, beside every frustum (both lists are emptied at the sector's first entry in a frame) [T7].
**Disputed in review:** not per frame but per traversal: the mark that tells a sector's first entry (and a portal's
passed mark, rule 1) is renewed at every traversal start, camera or light, so each traversal empties the lists
of each sector at its first entry in that traversal, and a light pass's traversal replaces the frusta the camera
pass left in the sectors it visits.

Screen coordinates: a point P is mapped by the traversal's matrix M to clip (x, y, z, w), and then to
*u* = (x / w + 1) / 2, *v* = (1 − y / w) / 2 (so 0 … 1 across the screen, *v* downward) and depth *z* / *w* [T7].

With bit 4, for a portal that is not marked as touched and whose clipped polygon (rule 5) is not empty [T7]:

1. every vertex of the clipped polygon is mapped; let *u*\_min, *u*\_max, *v*\_min, *v*\_max be the extremes and
   *z*\_min the smallest depth [T7];
2. if *z*\_min < 10⁻⁵, the portal is handled as without bit 4 (below) [T7];
3. otherwise the portal's rectangle is the intersection with the current entry's rectangle:
   *x*₀ = max(current *x*₀, *u*\_min), *y*₀ = max(current *y*₀, *v*\_min), *x*₁ = min(current *x*₁, *u*\_max),
   *y*₁ = min(current *y*₁, *v*\_max), with depth *z*\_min (the current depth is not used); the portal is skipped
   when *x*₁ ≤ *x*₀ or *y*₁ ≤ *y*₀ [T7];
4. with bit 1, and only while occlusion is on for the pass (VS 3.2), this rectangle with its depth is tested
   (VS 3.3, the rectangle shape) and the portal is skipped when hidden; with occlusion off for the pass no test is
   made [T6];
5. the target sector is entered with the frustum through the portal (section 5) and this rectangle [T7].

Without bit 4, for a touched portal, and in case 2, the target is entered with the current entry's rectangle and
depth unchanged, and bit 1 applies the polygon test of section 2.3 [T6, T7].

After a traversal with bit 4, every visited sector is given the union of the rectangles it was entered with
(smallest *x*₀ and *y*₀, largest *x*₁ and *y*₁) and the smallest of their depths [T7]. What reads these per-sector
values is **unestablished**; no pass of this renderer sets bit 4 (section 4), so in this renderer the mode never
runs [T1, T7]. Which inputs the rectangle test is given exactly is for spec V4 (**unestablished** here).

## 4. Which pass uses which

The traversal is started from exactly two places [T1]:

| Pass | Start sector, frustum | Eye | Matrix | Options |
|---|---|---|---|---|
| camera pass (VS 1) | camera sector, the view frustum | camera position **E** | the frame's view-projection | occlusion, size, fade (bits 1, 2, 8) |
| light pass, spot and point (DL 5.3 – 5.4) | the light record's sector, the light's frustum | the light's eye O (DL 5.3) | the light's matrix | none (0) |

[T1] So the light passes apply neither rule 4 nor rule 6 (as DL 5.4 states) and do not touch the fade list; the
screen-rectangle mode is used by no pass [T1, T5].

Shadow passes make no traversal: the traversal is started only by the two passes above, and the light pass only
for the spot and point light phases [T1]; the projected object shadows (LS) take their casters from the camera
pass (LS 3.1) and their receivers from a collision query (LS 5). During the light phases occlusion is switched
off for the pass and restored after them (on exactly when `level.hom` exists, VS 7) [T6] (it has no effect on the
traversal there, whose bit 1 is off).

The eye stored by a traversal remains after it ends; the portal fade's ordering reads it (PF) [T2].

## 5. The frustum through a portal

Every portal the traversal enters through, touched or not and in every pass, gives its target sector a new frustum
built the same way from the portal polygon clipped by rule 5, the traversal's eye **E** and the traversal's matrix
M; a touched portal gets no special case, and no entry keeps the current frustum as it was [T9]. The new frustum
does not inherit the current frustum's planes; the current frustum acts only through the clipping of the polygon
[T10]. Its planes, each with its normal scaled to unit length and a point inside when its plane value
**n**·**p** + *d* is negative [T11]:

1. **Polygon replacement.** A clipped polygon with more than six vertices is first replaced by four vertices: the
   rectangle bounding it in its own plane (normal as in item 2), with one pair of sides parallel to the reference
   axis (0, 1, 0) made orthogonal to that normal, or to (1, 0, 0) instead when |*n*\_y| > 0.99 [T10, T13].
2. **Portal plane.** The plane through the polygon's first vertex **v**₀ with normal
   (**v**₀ − **v**₁) × (**v**₀ − **v**₂); the portal's stored plane (LV section 5) is not used here [T9, T10]. When
   **E** lies strictly on the negative side of it, the polygon's vertex order is reversed and the plane taken again
   from the reversed polygon [T10]. So, with **E** off the plane, the space on the far side of the portal plane from
   **E** is inside, whichever side of the portal the eye is on [T10, M3].
3. **Edge planes.** For each edge (**v**ᵢ, **v**ᵢ₊₁) of the polygon in that order, closing with (**v**ₙ₋₁, **v**₀), the
   plane through **E** with normal (**E** − **v**ᵢ) × (**E** − **v**ᵢ₊₁) [T10].
4. Then the portal plane of item 2 [T10].
5. **Far plane.** The plane *z* − *w* = 0 of M: normal (M₀₂ − M₀₃, M₁₂ − M₁₃, M₂₂ − M₂₃), *d* = M₃₂ − M₃₃
   (row-vector convention, section 1), normalised; inside where clip *z* < *w* [T10, M3].

In items 2 and 3, a normal of length zero (for an edge plane: **E** on the edge's line) becomes (0, 1, 0), its
plane still through the point given; any other normal, however short, is scaled to unit length [T12].

The camera pass's view frustum is built from the same matrix it gives the traversal, with five planes: the four
sides (clip −*w* < *x* < *w*, −*w* < *y* < *w*) and the far plane *z* < *w* in the same form as item 5; it has no
near plane [T11]. So every frustum of the camera pass's traversal, through any number of portals, has the view
frustum's far plane, and in place of a near plane the plane of the last portal passed [T10, T11].

Consequences for a touched portal (the eye within the box of LV 9.2 rule 2) [M3]: with **E** off the portal's
plane, the new frustum is the cone from **E** through the clipped polygon beyond the portal plane as seen from
**E**; when **E** is already past the plane on the target's side, that cone opens toward the current sector's side,
away from the target. With **E** exactly in the plane and inside the polygon, every edge plane coincides with the
portal plane up to sign, and the frustum is the half-space on one side of the portal plane (cut by the far plane),
the side set by the clipped polygon's vertex order.

LV 9.2 describes the narrowed frustum as "the planes through the camera and the polygon's edges"; it also has the
portal plane and the far plane (items 4, 5), and the polygon replacement of item 1 [T10].

## 6. Unestablished

- What reads the per-sector union rectangle and depth of the screen-rectangle mode (section 3).
- The exact inputs and comparison of the rectangle and polygon occlusion tests (spec V4).
- Whether anything besides passing a portal clears its touched mark (section 2.4).
- The vertex order of the stored portal polygons relative to their stored plane, and so which side the frustum of
  section 5 takes when the eye lies exactly in a portal's plane.

## 7. Provenance

- Date: 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\xrRender_R1.dll` and
  `bin\XR_3DA.exe`.
- Analysis: static analysis of the shipped binaries (headless decompilation and disassembly, cross-references,
  constants read from the binaries' data).
