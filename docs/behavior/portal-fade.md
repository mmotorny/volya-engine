# Portal fade (first renderer)

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the darkening fade that
the first (static-lighting) renderer `bin\xrRender_R1.dll` draws over small portals: which portals, the geometry
drawn, its colour and alpha as a function of size and distance, the shader and blending, its place in the frame,
and the settings (spec V9 of `docs/behavior/visibility-scope.md`).

Builds on (not repeated): `docs/formats/level.md` (here "LV": portals section 5, traversal 9.2, thresholds 9.4),
`docs/behavior/traversal-options.md` (here "TO"), `docs/behavior/visibility-scope.md` (here "VS"),
`docs/behavior/static-shading.md` (here "SS": scripted definitions 3.4, weather values 9.2, fog 9.3, frame order
10.3), `docs/behavior/particle-drawing.md` (here "PD", 7.4), `docs/behavior/dynamic-lights.md` (here "DL").

Conventions: **E** is the camera position; *m* is a portal's screen measure of LV 9.2 rule 4; *T*\_min, *T*\_A,
*T*\_B and *S* are those of LV 9.4. Every fact carries the id of the check that reproduces it; the checks are
scripts in `checks/traversal-fade/` of the dirty workspace (`run.py` runs them all): `bin_fade.py` [F…] and
`bin_traversal.py` [T…] read the shipped binaries by static analysis (read-only decompilation; outputs kept in
`out/`); `data_shader.py` [D…] reads the game data (merged archive view); `model.py` [M…] is arithmetic. All checks
pass.

## 1. Which portals

The frame has one **fade list** of entries (portal, *m*). Only the camera pass's traversal fills it (its fade
option, TO 2.2) [T1, T5, F2]:

- a portal is appended when the traversal examines it, it passes LV 9.2 rules 1 – 3, and
  *T*\_min ≤ *m* < *T*\_A [T5, F2];
- this happens before the *T*\_B test, the clipping of rule 5 and the occlusion test of rule 6, so the list
  holds portals whose far sector is entered (*T*\_B ≤ *m*, when rules 5 and 6 pass) as well as portals whose far
  sector is not entered through them (*m* < *T*\_B, or rejected by rule 5 or 6) [T5];
- a portal can be appended more than once in a frame: once for every entry of its sector during which it is
  examined and not yet passed (TO 2.2) [T5]; each entry is drawn.

The list is emptied at the start of every camera traversal and after it is drawn [T5, F2]. When the camera pass
makes no traversal (no camera sector, VS 6) the list stays empty and nothing is drawn [F2, T5].

## 2. Geometry

For each entry, the portal's own polygon (LV 5: its *count* vertices **v**₀ … **v**\_{count−1}, unclipped, in world
coordinates) is drawn as the fan of *count* − 2 triangles (**v**₀, **v**ᵢ, **v**ᵢ₊₁), *i* = 1 … *count* − 2 [F4].
All entries go into one triangle list drawn with one draw call, world transform identity [F4]. A vertex is a
position (three floats) and a 32-bit colour (Direct3D vertex format position + diffuse, 16 bytes); all vertices
of one entry share one colour [F4, F5].

### 2.1 Order

Before drawing, the entries are sorted by the squared distance from the **traversal eye** to the portal's
bounding-sphere centre (LV 5), increasing: the nearest first [F3]. The sort is not stable; the order of equal
distances is **unestablished** [F3]. The traversal eye is the eye of the last traversal started in the frame
before the fade is drawn [F3, T2]: **E** when no dynamic-light pass ran a traversal in the frame; otherwise the
eye O (DL 5.3) of the last light pass, since the light phases run after the camera pass and before the fade
(section 5) [F1, F3, T1]. (The camera pass runs before the frame's drawing, VS 1; **believed**: once per frame.)

Since the fade is blended and does not write depth (section 4), the order matters only where fades overlap on
screen.

## 3. Colour and alpha

With the current weather's blended `ambient` colour *a* = (*a*\_r, *a*\_g, *a*\_b) (SS 9.2, `L_ambient.rgb`; the
value at the time the fade is drawn) [F5]:

- R = clamp(⌊255 · *a*\_r⌋, 0, 255), likewise G and B [F5, M1];
- alpha A = clamp(⌊(1 − (*m* − *T*\_B) / (*T*\_A − *T*\_B)) · 255.5⌋, 0, 255) [F5, M1], with the entry's *m* and
  the frame's *T*\_A, *T*\_B; computed in single precision.

⌊·⌋ here is the binary's float-to-integer conversion: truncation toward zero for positive values; negative values
give 0 or less and are clamped to 0; +∞ gives 1 [M1].

With *T*\_A > *T*\_B (the defaults: *T*\_A / *T*\_B = (64 / 48)² ≈ 1.778) [M2]:

| *m* | A |
|---|---|
| *T*\_min ≤ *m* ≤ *T*\_B | 255 (opaque) |
| *T*\_B < *m* < *T*\_A | 255.5 · (*T*\_A − *m*) / (*T*\_A − *T*\_B), truncated (127 midway) |
| *m* ≥ *T*\_A, or *m* < *T*\_min | not drawn |

With *T*\_A < *T*\_B every entry gets A = 0; with *T*\_A = *T*\_B every entry gets A = 1 (division by zero gives
+∞ before the conversion) [M2] (both modelled from the binary's arithmetic).

**As a function of distance.** For a portal of radius *r* (LV 5) seen at distance *D* from the camera to its
centre under the absolute cosine *c* (LV 9.2 rule 4), *m* = *r*² *c* / *D*². Let *D*\_X = *r* √(*c* / *T*\_X)
for X = A, B, min (*D*\_A < *D*\_B < *D*\_min with the defaults). The fade is not drawn closer than *D*\_A; its
alpha rises from 0 at *D*\_A to 255 at *D*\_B, linearly in 1 / *D*²; it is opaque from *D*\_B to *D*\_min, where the
portal's far sector is also no longer entered through it (from *D*\_B on); beyond *D*\_min neither the fade nor the
far sector is drawn through that portal [T4, T5, M2]. (*D* and *c* are those of the traversal, i.e. from **E**.)

## 4. Shader and blending

The fade is drawn with the engine shader `portal`, created once with no textures, element 0 [F6]. It is a scripted
definition: `shaders\r1\portal.s` defines only `normal`, and no library definition of that name exists (SS 3.4)
[D1, D4]. Its single pass [D1]:

- programs `portal` (vertex) and `simple_color` (pixel);
- fog on; depth test on (less-or-equal), depth write off;
- blending source-alpha / inverse-source-alpha;
- sorting priority 3, strict (this has no effect: the fade is drawn directly at its place in the frame, not
  through the sorted lists, section 5) [F1, F6].

Vertex program `portal.vs`: clip position = view-projection `m_VP` applied to the world position; colour passed
unchanged; fog factor F = dot((P, 1), `fog_plane`) (SS 9.3) [D2]. Pixel program `simple_color.ps`: output = the
interpolated vertex colour (rgba) [D3]. The fixed fog unit then applies SS 9.3 (C ← F·C + (1 − F)·fog_rgb, alpha
unchanged). Result in the frame: dest ← C · A/255 + dest · (1 − A/255).

During the draw triangle culling is off (Direct3D render state 22 = 1), and afterwards it is set back to
counter-clockwise culling (value 3) [F6]. Which view-projection `m_VP` holds at that moment is **believed** the
camera's (the passes drawn just before it are the camera's non-sorted passes, section 5), **unestablished** here.

## 5. Place in the frame

The fade is drawn once per frame by the frame drawing, in the order of SS 10.3 and PD 7.4 [F1]:

1. … the dynamic-light phases (spot, then point; DL), then the non-sorted passes of priorities 2 – 3 (SS 10.3
   step 5);
2. **the portal fade**;
3. the strict-sorted passes (SS 10.3 step 6), glows, lens flares and the environment's final drawing [F1].

This settles PD 7.4's "believed" geometry between its steps 4 and 5: it is the portal fade [F1].

## 6. Settings

| Setting | Effect on the fade |
|---|---|
| `r1_ssa_lod_a` (default 64) | *T*\_A: the measure (distance) at which the fade starts [T5] |
| `r1_ssa_lod_b` (default 48) | *T*\_B: where it becomes opaque, and where the far sector stops being entered [T5] |
| `r__geometry_lod`, field of view, render-target size | *S*, scaling all thresholds (LV 9.4) |
| weather `ambient`, fog values | colour (section 3), fog (section 4) |

No setting switches the fade off by itself (the first renderer has no console variable for portals, VS 7);
`r1_ssa_lod_a` < `r1_ssa_lod_b` makes it invisible (A = 0), and equal values make it nearly so (A = 1) [M2].

## 7. Unestablished

- The order of entries at equal distance (section 2.1).
- Which view-projection the vertex program receives (section 4; believed the camera's).

## 8. Provenance

- Date: 2026-11-25.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\xrRender_R1.dll`,
  `bin\XR_3DA.exe`; archives `gamedata.db0` … `gamedata.dbd` (merged view).
- Data examined: `shaders\r1\portal.s`, `portal.vs`, `simple_color.ps`, `common.h`; `shaders.xr`.
- Analysis: static analysis of the shipped binaries (headless decompilation and disassembly, cross-references,
  constants read from the binaries' data); reading of the shader scripts and programs as data; arithmetic.
