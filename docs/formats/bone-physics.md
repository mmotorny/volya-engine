# Bone physics data (chunk 16 of skeleton models)

Format specification of the **bone physics data** of the skeleton models (`.ogf`, visual types 3 and 10) of
S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): every field of chunk 16, what each value means to the game,
which values the shipped models hold, and what happens to the data when a model loads. It is PH-1 of the scoping
specification `docs/behavior/physics-scope.md` (section 3). How shells are built from it in full (bodies, fixed
bones, the objects that use them) is PH-3; joints as simulated, PH-7; contacts, PH-4; breaking, PH-13. Where this
text states how a field is used, it states what the value does, not the whole of the behaviour around it. What
could not be established is marked **unestablished**; what is inferred rather than shown by the data or the
program is marked **believed**.

Builds on (not repeated here):

- `docs/formats/skeleton-ogf.md`: model files, chunk 13 (bone names, parents, bone index = position
  in its list), the layout of chunk 16 (section 4, whose field names this text refines), the bind pose (the
  bind rotation and position fields and how they make each bone's bind matrix), child meshes and vertex formats
  (sections 5 – 6).
- `docs/formats/level-cform.md`: the game material library `gamemtl.xr`, its material names and flags (section
  3.2, the **dynamic** flag 0x100), the world's error reduction and constraint-force mixing (section 4.2) and the
  step length (section 6.1).
- `docs/behavior/physics-scope.md`: the terms **body**, **shell**, **joint**, **step** (introduction), the physics
  types of physics objects (section 1.2).

Conventions: little-endian; `u16`, `u32` unsigned; `f32` IEEE single; `float3` three `f32` (x, y, z); `stringZ`
bytes up to and including a zero byte. Bit values are written as numbers (bit 1 = value 1, bit 2 = value 2, bit
4 = value 4). "The bone's own space" is the bone's bind space of `skeleton-ogf.md` 4 (the space of its chunk-13
box). "Above" a bone is its parent, its parent's parent and so on.

**Checks.** Every claim carries a tag: [D*n*] data checks (`data.py`, over every `.ogf` file of the archives and
`gamemtl.xr`), [B*n*] checks against the shipped binaries (`bin.py`, on exports made by `export.py`). Both are in
the dirty workspace, `checks/bone-physics/`; `run_all.py` regenerates missing exports and runs both.

## 1. The record

Chunk 16 holds one record per bone, in the order of chunk 13, with nothing before, between or after them
[D1, D1c, B1]. All 1,579 skeleton models of the archives have it, every record has version 1, and every chunk is
consumed exactly by this layout (20,685 records) [D1, D1b]. Offsets after the material name are counted from the
byte following its terminating zero.

| Offset | Size | Field | Meaning (section) |
|---|---|---|---|
| 0 | 4 | version, `u32` | if its low 16 bits are 0, the record has no friction field; 1 in every shipped record [B1, D1b] |
| 4 | *n* | material, `stringZ` | name of the bone's game material (6) |
| +0 | 2 | shape type, `u16` | 0 none, 1 box, 2 sphere, 3 cylinder (2.1) |
| +2 | 2 | shape flags, `u16` | bits 1, 2, 4 (2.2) |
| +4 | 60 | box, 15 × `f32` | x axis (3 values), y axis (3), z axis (3), centre `float3`, half sizes `float3` (2.3) |
| +64 | 16 | sphere, 4 × `f32` | centre `float3`, radius (2.3) |
| +80 | 32 | cylinder, 8 × `f32` | centre `float3`, direction `float3`, height, radius (2.3) |
| +112 | 4 | joint type, `u32` | 0 – 5 (3.1) |
| +116 | 48 | limits, 12 × `f32` | three axis records x, y, z, each: low limit, high limit, limit spring factor, limit damping factor (3.2) |
| +164 | 4 | spring, `f32` | joint stiffness factor (3.3) |
| +168 | 4 | damping, `f32` | joint damping factor (3.3) |
| +172 | 4 | joint flags, `u32` | bit 1: breakable (3.4) |
| +176 | 4 | break force, `f32` | (3.5) |
| +180 | 4 | break torque, `f32` | (3.5) |
| +184 | 4 | friction, `f32` | present only if the version's low 16 bits are not 0; not used (4) |
| +188 | 12 | bind rotation, `float3` | `skeleton-ogf.md` 4 |
| +200 | 12 | bind position, `float3` | `skeleton-ogf.md` 4 |
| +212 | 4 | mass, `f32` | (5) |
| +216 | 12 | centre of mass, `float3` | in the bone's own space (5) |

"+*k*" is the offset counted from the shape type; a record with friction is 4 + *n* + 228 bytes long. The
shape's three parts (box, sphere, cylinder) are all stored whatever the type; only the part of the shape type is
used [B8].

**Without chunk 16** (none of the shipped skeleton models): every bone keeps shape type 0, flags 0, joint type 0,
limits 0 with factors 1, spring 1, damping 1, joint flags 0, break force and torque 0; friction, mass and centre
of mass are left unset (their values then are **unestablished**) [B1b, B1c].

**Space.** Shapes, the centre of mass and the joint anchor are in the bone's own space: a bone's body and shape
are placed by its bind transform composed with those above it [B4j]. In the data, the centre of a used shape lies
within 1 mm of the centre of the bone's chunk-13 box on more than half of the 11,327 bones with a used shape and
within 0.1 m on 87.6 % [D4].

## 2. Shape

### 2.1 Types

| Type | Shape | Used part | Bones |
|---|---|---|---|
| 0 | none: the bone adds no body, shape or mass of its own (3.1) | — | 7,343 |
| 1 | box | box | 8,976 |
| 2 | sphere | sphere | 2,490 |
| 3 | cylinder | cylinder | 1,876 |

[B8, B8b, B4, D2]. No other value occurs in the data [D2]; what another value would do is **unestablished**.

### 2.2 Flags

| Bit | Meaning | Bones |
|---|---|---|
| 1 | no hit marks: the marks that hits leave drawn on the model (decals) skip this bone [B3] | 474 |
| 2 | removable: when every visible bone of an object carries it, the object is put on a removal timer [B10]; when the game checks this and the time are for PH-13 | 4,801 |
| 4 | no physics: the bone is treated as having shape type 0 (no body, shape or mass of its own) [B4, B4c] | 3,593 |

Values in the data: 0, 1, 2, 4, 5, 6; per type: type 0 with 0, 1, 2, 4, 5; type 1 with 0, 1, 2, 4, 5, 6; type 2 with
0, 1, 4; type 3 with 0, 1, 4 [D2b, D2c]. No other bit occurs, and no test of another bit was found. A **used
shape** is one of type 1 – 3 without bit 4: 11,327 bones [D2d].

### 2.3 Values

- **Box**: three axis vectors (the box's x, y and z directions in the bone's own space), then its centre, then
  its three half sizes along those axes [B8f]. The edge lengths of the box are twice the half sizes; for the
  collision geometry each half size is first raised to at least 0.005 [B8c, B8d]. In the data the three axes are
  orthonormal within 2.2 × 10⁻⁵ on every box (right-handed on 8,971, left-handed on 5); half sizes are ≥ 0, 0 on 46
  boxes and below 0.005 on 2,955 [D3, D3b].
- **Sphere**: centre, radius [B8d, B8f]. Radius > 0 on every sphere [D3c].
- **Cylinder**: centre, unit direction of its axis, height (length along the axis), radius [B8d, B8f]. Direction of
  length 1 within 10⁻³, height and radius > 0 on every cylinder [D3d]. The cylinder geometry is the game's own
  (`physics-scope.md` 1.1; PH-4).
- **Volumes** (used to share a body's mass between its shapes, section 5): box 8 · *hx* · *hy* · *hz* (with the
  raised half sizes), sphere 4/3 · π · *r*³, cylinder π · *r*² · *h* [B8e].

## 3. Joint

### 3.1 Joint types

A bone's joint connects its body to the body **above it**: the body of the nearest bone above that has one (bones
without a body pass their children on to it) [B4, B5f]. The joint's first body is that one, the second the bone's
own [B5f]. A root bone, and any bone with no body above it, has no joint: its joint type is ignored (0 on 913 root
bones, 4 on 646, 1 on 12, 2 on 8) [B4c, B5f, D5b].

| Type | Meaning | Bones |
|---|---|---|
| 0 | **rigid**: no body of its own; the bone's used shape and its mass are added to the body above (a bone without a used shape adds nothing); with joint-flag bit 1 it is a breakable part of that body (3.5) | 12,982 |
| 1 | **ball**: free rotation about the anchor; no limits; spring and damping apply | 48 |
| 2 | **jointed by its limits**: a hinge or a three-axis joint, chosen by which axes have equal limits (3.2) | 2,854 |
| 3 | **two-axis hinge**: rotation about the bone's x axis (limited by the x limits) and its z axis (no limits) | 70 |
| 4 | **none**: the bone has its own body but no joint (a free body) | 4,710 |
| 5 | **slider**: sliding along one axis, limited by the x limits, with rotation about the same axis limited by the y limits | 21 |

[B4, B4b, B4c, B4i, B6, B6b, B6c, B6d, D5]. A bone of joint type 1 – 5 with no used shape gets no body either,
and its joint is then not built (its children attach above) [B4]. No joint type above 5 occurs in the data [D5]; what one
would do is **unestablished**. Every bone of the 646 level-local breakable models
(`levels\<level>\meshes\brkbl#<n>.ogf`) has joint type 4 and joint-flag bit 1, and no other model has type 4 on
every bone [D9, D9b].

The joint's **anchor** is the origin of the bone's own body [B6, B5e] (**believed**: that is the bone's bind
origin, since the body is placed by the bone's bind transform [B4j]). The axes the joint takes from the data are
the axes of the bone's own body, as built [B5d, B6]. The slider's axis, which the data does not set, is the z axis
of the first body's frame (**believed** from the default axis and its frame [B5b, B5c, B5d]).

### 3.2 Limits and their axes

The 12 values are three **axis records**, for x, y and z in this order, each: **low limit**, **high limit**,
**limit spring factor**, **limit damping factor** [B5d, B6]. Limits are angles in radians, except the x limits of
joint type 5, which bound the slide (**believed** metres) [B6c, D5e].

- For joint type 2, the two limits of an axis are **equal** when |low − high| < 10⁻⁵ [B6f] (such an axis is
  locked).
- An axis is given its limits only if high − low < 2π (single precision); a range of exactly 2π (−π … π) leaves
  the axis free [B6g, D6b].
- The limits handed to the library are kept within −π … 0 (low) and 0 … π (high): a bound outside its range is
  moved inside and the other bound shifted by the same amount [B7e]; this holds for every joint that has limits,
  the slide of joint type 5 included [B7g]. One bone has a slide limit below −π (bone `door` of
  `meshes\physics\door\door_yantar_u_01.ogf`, −3.6 … 0), which therefore becomes −π … 0.4584 [D5h, B7e].
  In the data, joint types 1 – 5 always have low ≤ 0 ≤ high, and joint types 1 – 4 have every limit within
  −π … π [D5c, D5d].

**Joint type 2** is built by which axes have equal limits [B6e]:

| Equal axes | Joint | Axis 1 | Axis 2 | Axis 3 |
|---|---|---|---|---|
| y and z | hinge | x | — | — |
| x and z | hinge | y | — | — |
| x and y, or all three | hinge | z | — | — |
| only z | three-axis | x | z | y |
| only y | three-axis | z | y | x |
| none, or only x | three-axis | z | x | y |

Each axis gets the limits and limit factors of the axis record of that name [B6e]; a hinge has one axis, the
two-axis hinge and the slider two, the three-axis joint three [B7f]. The hinge turns about that bone axis. The **three-axis** joint is a ball joint with an angular motor whose first and third axes are the bone
axes named [B5, B6e]; its second axis is derived from the other two (**believed**: the axis perpendicular to both,
as in the library's own Euler mode; PH-7 states it). **Disputed in review:** no check backs the reason given for this belief (how the library derives that axis); treat the second axis as unestablished. In the data: 1,081 type-2 bones have no equal axes, 119 only
x, 51 only y, 1,322 only z, 28 x and y, 164 x and z, 76 y and z, 13 all three [D6].

**Joint type 3**: axis 1 is the bone's x axis with the x limits and x limit factors; axis 2 is its z axis, without
limits [B6b]. **Joint type 5**: x limits on the slide, y limits on the rotation about the slide axis [B6c]; in the
data its x limits lie within −3.6 … 1.9 and its y and z limits are 0 [D5e]. Example: bone `door` of
`meshes\physics\door\door_trader.ogf` (bones `link`, `door`, `lock`) has joint type 2 with x and z limits 0,
so it is a hinge about the bone's y axis, limited to −1.9199 … 0 rad; bone 1 of
`meshes\physics\door\door_garbage_01.ogf` is a slider limited to −2 … 0 [D14, D14b].

Joint types 0 and 4 do not read the limits, spring or damping; in the data their values there are arbitrary on
1,094 rigid bones (12 not finite or above 10⁶) and fixed (limits 0, factors 1, spring and damping 1) on every type-4
bone [D5f, D5g].

### 3.3 Spring and damping

**Spring** and **damping** scale the stiffness of the joint (its error reduction and constraint-force mixing, the
library's softening of a constraint) against the world's own [B7, B7b]. With the step length *dt* (0.01 s,
`level-cform.md` 6.1):

- error reduction = *Ks* · spring · *dt* / (*Ks* · spring · *dt* + *Kd* · damping), constraint-force mixing =
  1 / (*Ks* · spring · *dt* + *Kd* · damping);
- *Ks* = 2.4 × 10⁷ (single precision) and *Kd* = 4 × 10⁵ for all joints except the two-axis hinge (joint type
  3): with spring 1 and damping 1 the joint is exactly as stiff as the world (`level-cform.md` 4.2) [B7b];
- the two-axis hinge uses *Ks* = 20,000, *Kd* = 1,000 [B7].

The **limit spring factor** and **limit damping factor** of an axis set, by the same formula with the same *Ks*,
*Kd*, how hard that axis's limits are (the error reduction and mixing at its stops); the two-axis hinge ignores
them and has stops with error reduction 1 and mixing 0 [B7c, B7d]. In the data: joint types 1 – 3 have spring
0.2 … 100 and damping 0 … 100; joint types 2 and 3 have limit spring factors 0.2 … 10 and limit damping factors
0 … 100 [D6c, D6d].

### 3.4 Joint flags

Only **bit 1** is tested: the bone is **breakable** [B11b]. A bone is breakable when it has bit 1, has a body above
it, and is not a rigid bone without a used shape [B4e]. A breakable bone's index above 63 is fatal ("ower 64 bones
in breacable are not supported") [B4e]; no shipped model has more than 61 bones [D8c].

**Load-time check** [B2, B2b, B2c]. When a model loads, if any bone has bit 1 and a joint type other than 4:

1. The bones are divided into **parts**: walking the hierarchy depth-first from the root (children in bone order),
   the root's part is 0, each bone with bit 1 starts a new part with the next number (1, 2, …), and every other
   bone is in the part of its parent.
2. For each child mesh, its bones are those named by the vertices its triangles use (a 1-link vertex names one
   bone, a 2-link vertex both, whatever the weight).
3. If the bones of any child mesh lie in more than one part, bit 1 is cleared on every bone of the model.

In the data, values other than 0 and 1 occur only on 6 rigid bones, with high bits only (0x54000000 on 3,
0x5F000000 on 2, 0x2E000000 on 1) [D7]. Bit 1 is on 201 bones of joint types 0 – 3 in 30 models, each with break
force and torque ≥ 1, and on every type-4 bone [D7b, D7d, D9]. The check clears bit 1 in 3 of the 30 models
(`meshes\physics\decor\kozel_01.ogf`, `meshes\physics\rope\krane_box_escape.ogf`,
`meshes\physics\vehicles\niva\include_test_niva.ogf`), leaving 182 breakable bones: 173 rigid, 7 of type 2, 2 of
type 1 [D8, D8b].

### 3.5 Break force and break torque

Used only by breakable bones [B4f]:

- A **breakable jointed** bone (types 1 – 3, 5) has its joint keep the squares of both values [B4g]. At a step the
  joint breaks when any of the two forces and two torques it applies to its two bodies has a squared size above
  100 × break force² (that is, a size above 10 × break force); break torque takes no part in this test [B4h]
  (whether anything else reads it for jointed bones is **unestablished**). What breaking does is PH-13.
- A bone of joint type 4 has no joint, so bit 1 makes nothing breakable there [B6d, B4f].
- A **breakable rigid** bone (type 0) becomes a part of the body above that can split off; both values go with it
  and both take part in the test that splits it [B4f]; **Disputed in review:** B4f shows only that both values are
  stored with the part; that both take part in the splitting test is unbacked (unestablished); the test itself is PH-13.

Units are **believed** to be newtons and newton-metres (the world's units). 74 bones without bit 1 carry a break
force ≥ 1, which nothing reads [D7c].

## 4. Friction

Read when present and never used: no other read of it was found in `XR_3DA.exe` or `xrGame.dll` [B11]. In the
data it is 0 on 14,310 bones and 1 on 2,581, and 1,107 bones hold values that are not finite or exceed 10⁶ [D10].

## 5. Mass and centre of mass

- **A bone with its own body**: the body's mass is the bone's mass, shared between the body's shapes in proportion
  to their volumes (one density), with the bone's centre of mass as the body's reference point [B8h].
- **A rigid bone**: its shape, carrying the bone's mass, is added to the body above about the bone's centre of mass;
  that body's centre of mass becomes the mass-weighted mean of the two [B8g]. A resulting mass that is not valid
  is fatal ("bad bone mass params") [B8g].
- A bone without a used shape contributes no mass [B4, B4c].

Units are **believed** to be kilograms and metres. In the data, mass is finite and > 0 on every bone, 7.7 × 10⁻⁵ …
8,000; the commonest mass of a bone with a used shape is 10 (1,363 of 11,327 bones) [D10b, D10c]. The centre of
mass lies within 1 mm of the shape's centre on 5,224 of the 11,327 bones with a used shape (median distance
7.8 mm) [D11].

## 6. Material

When a skeleton model is registered by the game, each bone's material name is looked up in the game material
library by name, ignoring case; the bone's material is the one found; an empty name gives `default_object`
[B9]. A bone's material must have the **dynamic** flag (0x100): otherwise the game stops with the fatal error
"Required dynamic game material" (for `default_object`: "'default_object' - must be dynamic") [B9]. A name the
library lacks is **unestablished** (none occurs). The bone's material is the material of hits on that bone,
and a shell made from the model is given its root bone's material (one case shown) [B9b]; contacts are PH-4
and PH-12.

In the data: 22 distinct names, none empty, all lower-case, all materials of `gamemtl.xr` with the dynamic flag;
`objects\dead_body` 9,683 bones, `objects\glass` 4,816, `default_object` 2,048, `objects\monster_body` 1,270,
`creatures\phantom` 560, `objects\clothes` 474, `objects\large_furniture` 335, `objects\large_metal_trash` 282,
`objects\tin_can` 262, `objects\car_cabine` 191, `objects\large_weapon` 184, `objects\barrel` 142,
`objects\small_weapon` 101, `objects\metal_box` 89, `objects\car_wheel` 85, `creatures\small` 46, `creatures\human`
42, `objects\small_metal_trash` 31, `objects\concrete_box` 29, `objects\bottle` 9, `objects\small_box` 4,
`objects\knife` 2 [D12, D12b, D12c].

## 7. Other readers of the limits

Besides joints, the limits are read by:

- mounted weapons, the vehicle gun and the helicopter's gun, which read the bones named by
  `[mounted_weapon_definition]` `rotate_x_bone`, `rotate_y_bone` (helicopter: `[helicopter_definition]`
  `wpn_rotate_x_bone`, `wpn_rotate_y_bone`): the x limits of one bone and the y limits of another are kept as
  the gun's turning range [B12, B12b, B12c, B12d] (**believed**: the x limits of the rotate-x bone and the y
  limits of the rotate-y bone);
- the positioning of characters' arms and legs (set up with the arm and leg bone names), which takes π − limit
  kept within 0 … 2π [B12e] (**believed**: the reach of characters' limbs; it is not part of the physics).

## 8. Roots and building

A shell built from the bone data (physics type 3 of `physics-scope.md` 1.2) fails fatally if the root bone has no
used shape (`physics-scope.md` 1.2). 284 models have a root without a used shape (e.g.
`meshes\actors\bandit\stalker_bandit_1.ogf`) [D13]; which objects build shells from them is PH-3 and
PH-15. Bones that are hidden when the shell is built are passed over, their children still built [B4d].

## 9. Unestablished and believed

- Shape types above 3 and joint types above 5 (none in the data); the values of friction, mass and centre of mass
  when chunk 16 is absent.
- When the game checks shape-flag bit 2 and the removal time (PH-13).
- A material name not in the library.
- Whether anything reads the break torque of a jointed bone (3.5).
- **Believed**: the anchor is the bone's bind origin; which gun bone gives which limits (section 7); the
  slider's axis is the z axis of the first body's frame; the second axis of the three-axis joint is perpendicular to the other two; the units (metres for the slider's
  limits, newtons, newton-metres, kilograms); the limb reader of section 7 is the characters' limb positioning.

## Provenance

- Date: 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`
  (merged view); binaries `bin\XR_3DA.exe`, `bin\xrGame.dll`, `bin\xrRender_R1.dll`.
- Data examined: every `.ogf` file of the archives (`meshes\` and `levels\*\meshes\`; 1,579 skeleton models,
  20,685 bone records), `gamemtl.xr`.
- Analysis: a throwaway reader written from `skeleton-ogf.md` 3 – 6 and this text reads every model's chunks
  9, 13 and 16 and reproduces the counts of sections 1 – 8; static analysis of the shipped binaries (headless
  disassembly and decompilation) for the model loader's reading of chunk 16 and its defaults, the load-time check
  of breakable bones and the assignment of faces to bones, the test that places hit marks on models, the
  building of bodies, shapes, masses and joints from the bone data (including a symbolic walk of the joint-type cases over their disassembly),
  the joint kinds and their default axes, the stiffness conversions and the world constants they derive from, the
  limit handling, the breaking test of joints, the material lookup, the removal test, and every reader of the
  friction, joint-flag and limit fields found by field offset.

## Gaps found in review

- Whether the origin of a bone's own body, at which its joint's anchor lies (3.1), is the bone's bind origin or its centre of mass (section 5 calls the centre of mass the body's reference point) is not established.
- Whether a cylinder extends half its height on each side of its centre along its direction is not stated.
- What the game does with the 5 left-handed boxes (2.3) is not stated.
