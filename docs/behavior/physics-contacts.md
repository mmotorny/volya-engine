# Contacts of movable bodies

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the contacts the physics
world gathers in a step: which pairs of objects are collided (bounds overlap, the motion rays of fast objects,
the collision filters), how contacts of box, sphere and cylinder geometries are made with the level's static
mesh and with each other (count, point, normal, depth), the contact properties of every pair of materials, the
liquid damping, the bounce, and which bodies a contact joins. It is PH-4 of `docs/behavior/physics-scope.md`
(section 3); the world's solver takes the contacts this spec makes (`world` 3, item 3; 4.2). Section 7 states
what the first case the game needs, the trader's door leaf against the player's body at the Escape bunker,
takes.

Builds on (not repeated):

- `docs/behavior/physics-scope.md` (cited as `scope`): terms *body*, *shell*, *joint*, *step*, *frame*; the
  model user-data keys (1.3);
- `docs/behavior/physics-world.md` (cited as `world`): *object*, *group*, *active* objects, the order of a step
  (3), the groups, their limits and the solver (4.2), resting shells (4.4);
- `docs/behavior/physics-library.md` (cited as `library`): the shipped library, *public 0.5.0*, its colliders
  and their changes (6), the contact rows (5.2);
- `docs/behavior/physics-shells.md` (cited as `shells`): elements, their geometries and materials (2.2), the
  collide keys and the collision group number (5.2);
- PH-5, resting and waking (`physics-resting.md`, cited as `resting`): a shell activated at rest;
- `docs/behavior/step-assistance-movable-bodies.md` (cited as `step`): the player's body's touch flag and its
  rules for contacts with movable bodies;
- `docs/formats/level-cform.md` (cited as `cform`): the static mesh, its triangles' front side (2.3), the game
  materials and their flags (3), the contact properties (4.2), the player's body (5);
- `docs/behavior/trader-door.md` (cited as `door`).

Conventions:

- A **geometry** is one collision shape (the library's term): a box, a sphere or a cylinder. A geometry lies on
  an element's body, or on none (static geometry). Each object of the world has one **collision space** holding
  its geometries (`shells` 2.2). The library's own names (ERP, CFM, `dContact`, `dCollide`, the first and second
  geometry of a contact, *public 0.5.0*) are its published interface.
- **Contrib cylinder** is the file `contrib/dCylinder/dCylinder.cpp` of the public ODE 0.5.0 release (a
  flat-ended cylinder contributed to ODE; the game's cylinder kind follows its structure, section 4.3).
- A contact's **normal** is the library's: it points from the contact's second geometry towards its first
  (public 0.5.0's convention, e.g. sphere–sphere: first centre minus second centre) [T2]. "Front normal" and
  "front side" of a triangle are `cform` 2.3's.
- Statements are static (read from the shipped binaries and data; the game was not run). Inferred statements are
  marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are the checks reproducing a statement: functions of `checks.py` in the dirty workspace's
  `checks/physics-contacts/` (`python3 checks.py` runs them all); ids `SR-…` are run by `shapeless_root.py` in
  the same folder (`python3 shapeless_root.py`).

## 1. Which pairs are collided

### 1.1 An object's collision pass

At the contact phase of each step (`world` 3, item 3) every active object, in the order of the active objects,
runs its collision pass; the player's body and shells run the same pass [SR-B11b]; only the active objects run
passes [SR-B11d]. It does, in this order [P1]:

1. **Motion rays**, only for an object carrying the ray mark (1.4).
2. **Other objects**: a box query of the physics objects' spatial index with the object's box; every object
   returned, other than itself, whose "to be collided" mark is set and which the collision filter (1.3) does
   not exclude, is collided with it as a pair (1.2) [P2].
3. **The level**, unless the object's class carries *ignore static* (1.3): its collision space is collided with
   the level geometry (section 3) [P1, P7].
4. Its own "to be collided" mark is cleared, so objects whose passes come later in the step skip it: each pair
   is collided once per step, in the pass of whichever of the two comes first among the active objects (1.5:
   an object woken by a contact is appended at the end) [SR-B11b].

**The object's box** is the bounding box of its collision space (for a shell and for a character body alike):
its centre and its half extents per axis are what the query uses; the largest half extent is kept as its
radius (believed: for the index's own bookkeeping) [P2]. Which objects a box query returns is the spatial
index's own test (in the engine's executable, not read here; believed: those whose indexed bounds meet the box;
**unestablished** in detail).

### 1.2 A pair

For a pair met in the pass of object A, B being the other [P6]:

- B is told of the pair first (shells and character bodies do nothing then).
- With `jA`, `jB` the joint counts of the two groups so far (contacts counted as joints, `world` 4.2) and `bA`,
  `bB` their body counts: contacts are made only when `1500 − jA − jB > 0` and `bA + bB < 500`, and then at
  most `1500 − jA − jB` of them.
- The library collides **A's collision space first and B's second**, asking for at most 810 contacts and using
  at most the first 809 (`world` 4.2) [P6]. The library collides space with space as public 0.5.0 does (every
  geometry of one with every geometry of the other whose bounds overlap; `door` 3.1).
- Each contact is then handled as section 2 states; a contact still "made" at the end becomes a contact joint.
- If at least one contact joint was made, B's group is merged into A's for the rest of the step (`world` 4.2;
  1.6 for the solver requests), and B, if not active, is made active (woken; 1.5) [P6, T2, SR-B12].

### 1.3 The collision filter

Each object carries a **class word** of bits and a **group number** (0 when made) [P4]:

| Bit | Meaning here | Set |
|---|---|---|
| 0x1 | has a group number | with the number, from the counter of `[collide] not_collide_parts` (`shells` 5.2) |
| 0x2 | ignore static | `[collide] ignore_static` when the shell has fixed bones (`shells` 5.2) |
| 0x4 | dynamic object | for every shell and character body when made; cleared for the static-geometry objects (breakables before they break, climbable objects) |
| 0x8 | ignore dynamic objects | only in game modes other than single play: a death shell, and every other shell object except rockets and grenades in a routine of theirs (**believed**: their activation) [P10] |
| 0x10 | character | when a character body is made |
| 0x20 | ignore characters | see below |
| 0x40, 0x80 | small object, ignore small objects | `[collide]` / `[collide_parts]` `small_object`, `ignore_small_objects`; a death shell gets 0x80 |
| 0x100 | a third class (**believed**: ragdolls) | by a setter of its own; which objects call it: **unestablished** |
| 0x200 | ignore the class 0x100 | `[collide] ignore_ragdoll` |

The filter, with `a` and `b` the two class words [P3]:

- if both have 0x1: the pair is collided only when the two group numbers differ;
- otherwise the pair is excluded when `((b & 0x114) << 1) & a & 0x228` or `((a & 0x114) << 1) & b & 0x228` is
  not 0. So a bit 0x8, 0x20 or 0x200 of one object excludes the other object having 0x4, 0x10 or 0x100
  respectively.

The masks are fixed (set when the world is made) [P3]. Bits 0x40 and 0x80 are written but never read: the
small-object keys have **no effect** [P5]. Bit 0x2 is read only by the pass (1.1, item 3) [P5]; neither the ray
pass nor the level pass applies the filter [P11, P1].

**Ignore characters (0x20).** A shell can carry a mark "ignore characters once at rest". A shell is made without
it, and only a death shell gets it: when activated in single play; nothing else gives a shell the mark [P10, R1,
R2]. A shell carrying the mark gets bit 0x20 each time, after a step in which it was active, none of its bodies is
left switched on (`world` 4.4), so also when it comes to rest again after being woken; it gets it too when it is
switched off as a whole (as below the floor, `world` 4.5), when it is taken out of the world (deactivated), and
when one of its elements is given a state with its body switched off while that body is on [R4]. Being activated
switched off (`resting` 1) does not give it: such a shell gets bit 0x20 only once it has been woken and then comes
to rest or is switched off [R4]. Whether code outside the physics part of the game library runs that routine too
is **unestablished**. Besides, a death shell gets bit 0x20 at once in one state of the dying character (meaning
**unestablished**) (PH-15) [P10]. Bit 0x20 is set in no other way, and nothing clears it [R3, R6].

In the data `[collide]` reaches the class only as `shells` 5.2 states (`ignore_static` 704 records,
`not_collide_parts` 39 models, `ignore_ragdoll` 3 records); `small_object` keys change nothing (above).

### 1.4 Motion rays of fast objects

A shell gets the **ray mark** from one function, which also lists every geometry of the shell for the rays; a
thrown missile (the bolt, grenades) calls it when launched; other callers are **unestablished** (PH-17), and no
character body gets the mark [P9].

Each geometry keeps a **previous centre**: none when the geometry is made (stored as −∞), and set to the
geometry's centre at the end of each of its collisions with the level (section 3) [P8]. For each listed
geometry with a previous centre whose distance `L` from the current centre is at least 10⁻⁵ m [P8]:

- a ray from the current centre towards the previous centre, of length `L`, is cast through the physics
  objects' spatial index;
- every object it returns, other than the object itself, whose "to be collided" mark is set, is collided as a
  pair (1.2) with a ray geometry standing for that geometry: the ray (from the current centre, towards the
  previous one, length `L`) as the first geometry, the other object's space as the second. No filter applies
  [P11].
- Ray–box and ray–sphere contacts are the library's (public 0.5.0's ray colliders; point, normal and depth as
  they give them); ray–cylinder contacts come from a test of the game's own whose rule is **unestablished**,
  with their normals negated. In every case the contact's first geometry is then replaced by the moving
  geometry, so the contact acts on the moving object's body and the other's [P8, P11].

So a fast object meets objects it passed through between two steps. Rays do not meet the level. An object
with `ignore static` never stores a previous centre, so it casts no rays (derived).

### 1.5 Resting objects: the "to be collided" mark and waking

An object's **"to be collided" mark** is set whenever its box is updated: after every step it was active in,
after its post-step work (in which a shell that has come to rest leaves the active objects, `world` 4.4) [SR-B11],
and when a shell is entered in the physics objects' spatial index, which it is whether activated switched on or
off (`resting` 1) [SR-B8d]. Only the object's own collision pass clears it [SR-B11c], and only active objects run
passes (1.1) [SR-B11d].

So a resting shell keeps its mark set, and every active object whose box overlaps its box (as last updated) is
collided with it at every step, exactly as with an active one (subject to the filter, 1.3).

**Waking.** When a pair yields at least one contact joint, the two groups are merged for the step and, if the
other object is not active, it is made active [SR-B12]: taken off the resting list (`world` 4.4) and appended to
the end of the active objects, so it runs its own collision pass later in the same step and is solved with the
merged group [SR-B12c]. The count is that of contact joints made: a contact the callbacks drop (2.3) makes none
[SR-B12b]. That stepping the merged group switches its bodies back on is `library` 4.1's rule (every listed
body's disabled flag is cleared).

### 1.6 The merge and the solver requests

A merge adds the other group's own requests (the exact-solver request of a shell with fixed bones, `shells` 3.5,
among them) to the colliding object's own requests, which the per-step copy keeps from then on [SR-B13, SR-B4c]
(**believed** consequence: from that step on, every group the colliding object heads with fewer than 30 joints is
solved by the exact stepper). Which solver solves a merged group in the step of the merge, and in later steps,
depends on which object's group heads it (**unestablished**).

## 2. Contact properties and what a contact joins

### 2.1 Materials

The two materials of a contact (`cform` 3) are found as follows [C2]:

- a geometry on an object carries its element's material (`shells` 2.2);
- a contact with the level carries its triangle's material, written by the level collision into the contact
  (section 3); the level geometry is always a contact's first geometry (1.1, item 3).

### 2.2 The properties

Every contact, with the level or between objects, with materials `A` and `B`, step `h`, the world spring `S`
and damping `D` (`cform` 4.2) [C1]:

- mode: soft ERP, soft CFM, friction pyramid in both directions (the library's `dContactSoftERP`,
  `dContactSoftCFM`, `dContactApprox1`);
- friction `μ = A.friction × B.friction`;
- `k = h × A.spring × B.spring × S`, `d = A.damping × B.damping × D`; ERP `k / (k + d)`, CFM `1 / (k + d)`;
- **bounce** only when both materials are bouncy (flag 0x4): bounce mode on, bounce `min(A.bounce, B.bounce)`,
  bounce threshold speed `max(A.bounce start speed, B.bounce start speed)`; otherwise no bounce mode (the unused
  fields hold 0.01 and 1.5).

This is `cform` 4.2's rule, now for every pair, two dynamic materials included [C1].

### 2.3 Handling a contact

For each contact the library returned, in order [C2, C3, C4]:

1. Properties as 2.2. A contact with the level whose material is **pass-through** (0x80) starts as not made;
   every other contact starts as made.
2. **Liquid** (2.4).
3. The second geometry's own contact callbacks, then the first's (each can change the contact and whether it is
   made; the player's shapes have some, `step` 2.1).
4. Each geometry's crossed-triangle records (section 3.3) whose triangle is pass-through are dropped.
5. The second geometry's object's contact callback, then the first's: a shell's does nothing; the player's
   body applies its rules (`cform` 5.4, `step`, PH-8).
6. If either geometry still holds a crossed-triangle record, the contact's friction becomes **infinite** [C4].
7. If the contact is made and fewer than the pair's limit (1.2) have been made, it becomes a **contact joint** of
   the step's contact group (emptied after the step, `world` 3, item 10), put into the colliding group, and
   attached to the bodies of its first and second geometry [C3]. A geometry without a body (the level, a
   static-geometry object) gives no body: the joint then holds the other body against the fixed world.

The joint is of the library's ordinary contact kind; the capped kind (`library` 5.2, item 5) is made only by
the player's body's callback (PH-8) [C3].

### 2.4 Liquid damping

A contact with the level of a **liquid** material (flag bit 31, `cform` 3.2), whose other geometry holds no
crossed-triangle record [C5]:

- if that geometry has no body, the game stops with a fatal error ("static - static collision !!!");
- unless the material has flag 0x200 (none shipped), it is skipped for a geometry on an object with the ray mark
  (1.4);
- otherwise the body gets the damping value `1 − liquid factor` (`cform` 3.1, chunk 0x1004), the largest of the
  step's values kept, with that contact and its material recorded.

In its pre-step work (`world` 3, item 4) every object pushes each body holding such a value `q` (a shell: those
of its elements that are active; the player's body: its own) with the force `−k · v`, `v` the body's velocity,
`k = q² × 10000 × |v|`, at most (body mass) / `h`; no force when a quantity of the rule (believed: `k`) is below
10⁻⁷; when the material is not pass-through, the component of the force along the recorded contact's normal is
removed (only the part across it acts); then the value is dropped [C5]. The only shipped liquid material is
`materials\water` (liquid factor 0.8, so `q` = 0.2; pass-through, so the whole force acts; `cform` 3.2). This
settles the conditions `cform` 3.2 and 8 leave open (the "touching body's state" is the crossed records and the
ray mark).

## 3. Contacts with the level mesh

### 3.1 The level geometry

One geometry of a game-defined kind, made with the world, stands for the whole static mesh `level.cform`
(`cform` 2) [P7]. It collides with spheres, boxes and the game's cylinders only (no other kind gets contacts
from it), and only with a geometry whose "meets the level" mark is set: every geometry's is set when made; the
player's outer sphere clears it (`cform` 5.1) [L1]. Its contacts carry the level geometry first, the shape
second, so their normals point from the shape towards the surface (into the mesh); each carries the triangle's
material (2.1) [C2, L5].

### 3.2 The triangles near a geometry

A query box around the geometry's centre with half extents per world axis `i` [L2]:

| Shape | Half extent `e_i` |
|---|---|
| sphere radius `r` | `r + 0.04·|v_i|` |
| box sides `L_j`, rotation `R` | `0.5·Σ_j |R_ij·L_j| + 0.01`, plus `0.04·|v_i|` when on a body |
| cylinder radius `r`, length `l` (axis = column 1 of `R`) | `r·√(R_i0² + R_i2²) + 0.5·|R_i1·l| + 0.04·|v_i|` |

(`v` the body's velocity.) Each geometry keeps a list of triangles: it is fetched again, by a box query of the
static mesh (the engine's own query of its collision model) with half extents `1.3·e_i` about the current
centre, when the geometry has no previous centre or its query box (centre ± `e`) is not inside the box last
fetched; otherwise the kept list is used [L3]. Every triangle of the list is then tested for overlap with the
query box; only those passing take part (the test's exact rule: **unestablished**, believed a box–triangle
overlap test) [L4].

### 3.3 Front side, crossing and push-out

For each triangle, `d` is the signed distance of the geometry's centre from the triangle's plane along its
front normal [L4]:

- **In front** (`d ≥ 0`): the triangle gives **ordinary contacts** (3.4), provided room for 10 more contacts is
  left and the geometry is not *inside* (below); one more exception applies while a crossing is being found in
  the same pass and the geometry has a previous centre (then also no ordinary contact) [L4].
- **Behind** (`d < 0`): no ordinary contact. If the previous centre was behind the plane too (`d_prev < 0`) and
  the geometry is not inside, the triangle is ignored (`cform` 4.1's rule). Otherwise, if the shape overlaps
  the triangle (a second, closer test), the triangle is a **crossing candidate**: its push-out depth is
  `p = s(n) − d`, `s(n)` the shape's support along the front normal (3.5).

A geometry keeps up to two **crossed-triangle records** (a first and a second), from step to step [L4]:

- at each collision, a record whose triangle no longer has the centre behind it (`d ≥ 0`) is dropped;
- a geometry holding a record is **inside**: it makes no ordinary contacts, only push-out contacts;
- from the candidates, the one of least push-out depth becomes the first record, and a candidate whose front
  normal makes with the recorded triangles' normals an angle below 135° (normal dot product above −0.70710677)
  can become the second [L4]; the exact rules of choosing (the candidates' further tests, the second record's
  choice, and the case of no previous centre, in which the triangles in front are also consulted) are
  **unestablished**.
- **push-out contacts** of a record, `f` its triangle's front normal: normal `−f` (into the mesh, 3.1), depth
  `s(f) − d`, only when that depth is not negative: a sphere 1 contact at `centre − f·r`; a box up to 3 at its
  vertices deepest along `−f`; a cylinder up to 3 on its rim, the second and third
  only when `|c| ≤ 0.70710677` (`c` = axis · normal) [L7]. Two records give up to the sum, within the room
  left.

So a shape that crosses a triangle's plane in a step is pushed back out through that triangle, and keeps being
pushed out until its centre is in front again. While it holds a record, every contact of that geometry, with
the level or with objects, has infinite friction (2.3, item 6).

### 3.4 Ordinary contacts

**Sphere** (radius `r`), one contact per triangle at most [L6]:

- depth `r − d`; none when negative;
- with `u` a unit direction from the surface towards the centre, the contact has normal `−u` and point
  `centre − u·r`;
- if the centre's projection onto the plane lies inside the triangle: `u` = the front normal, depth `r − d`;
- otherwise the triangle's edges are tested in order (v0–v1, v1–v2, v2–v0), then its vertices: the first
  whose closest point lies within `r` of the centre gives the contact, `u` from that closest point towards the
  centre (when the centre is on it: (0, 1, 0)), depth `r − distance`;
- **duplicates**: an edge used marks that edge in every later triangle of the list sharing it with the opposite
  direction (the neighbour of consistent winding); a vertex used marks it in every later triangle using it; a
  triangle with any marked edge gives no edge or vertex contact, one with a marked vertex no vertex contact.

**Box** (half sizes `h_k`, axes `a_k`): a separating-axis test against the triangle [L5]:

- axes: the front normal (code 0; depth `Σ h_k·|a_k·n| − d`, none when negative); each box axis `a_k` with each
  triangle vertex (codes 1 – 9), considered only when all three vertices lie on one side of the centre along
  `a_k`; the nine products of the box axes with the triangle's edges (codes 10 – 18), whose depth, multiplied by
  1.05, must be smaller than the best so far to be chosen; any axis that separates ends the test with no
  contact; the axis of least depth is chosen;
- code 0: contacts at box vertices: the vertex deepest along the normal and further vertices along the edges
  from it, each kept only when its depth is not negative (up to 4);
- codes 1 – 9: one contact at that triangle vertex, normal ± that box axis;
- codes 10 – 18: one contact (its point: **unestablished**), normal the cross axis;
- all contacts are dropped when the chosen normal points along the front normal (dot product above 0);
- the contacts carry the depth of the chosen axis; the exact per-vertex depths of the code-0 case are
  **unestablished**.

**Cylinder**: the same frame (query, front side, records); its ordinary per-triangle contacts are made by a
test of the game's own whose rules are **unestablished** (up to 3 contacts, with rim points 120° apart as in
4.3, **believed**).

A geometry may carry a **triangle callback**, called for every triangle that gave contacts (which geometries
carry one: **unestablished**) [L5].

### 3.5 Support along a normal

`s(n)` for a sphere `r`; for a box `Σ_k h_k·|a_k·n|`; for a cylinder `(l/2)·|c| + r·√(1 − c²)`, `c = axis · n`
limited to at most 1 [L7].

## 4. Contacts between objects

### 4.1 The shapes

Boxes and spheres are the library's (a box made with sides twice its half sizes). Cylinders are a game-defined
kind: radius and length, the axis along the geometry's y axis; bounds per axis `r·√(R_i0² + R_i2²) +
0.5·|R_i1·l|`; it has colliders for boxes, spheres, cylinders and planes [O1]. Geometries offset from their body
sit in the library's geometry transforms (public 0.5.0) (believed for shells; `shells` 2.2).

### 4.2 Box and sphere pairs

Sphere–sphere, sphere–box and box–box are the library's: public 0.5.0 with the changes of `library` 6.2 (normal
normalisation; box–box edge–edge contacts, 1 to 7) (`library` 6.1, 6.2) [O1]. Their counts, points and depths are
public 0.5.0's.

### 4.3 Cylinder pairs

The cylinder kind has the collider set and the wrappers of contrib cylinder (each contact's normal the
negated separating axis, first geometry the cylinder) [O2]. Against contrib cylinder [O2]:

- **cylinder–sphere**: the same three axes (the cylinder's axis; the direction from the axis to the sphere's
  centre; the direction from the nearest rim point to the sphere's centre), one contact at the sphere's surface,
  but with **half** the cylinder's length where contrib uses the whole, and the rim point's side chosen from the
  separating direction itself (contrib reads a normal not yet set);
- **cylinder–box**: the same eleven axis codes (0 – 10) and single contact, except that for codes 1 – 3 (a box
  face against the cylinder's side) up to two more contacts are added on the rim, at the points turned 120°
  (cosine −0.5, sine 0.8660254) either way from the first, each kept only when its depth is above 0, under a
  test of a quantity against 0.70710677 (its definition: **unestablished**);
- **cylinder–cylinder** and **cylinder–plane**: the game's own routines; their axes and contacts are
  **unestablished** (believed after contrib's).

## 5. The player's body

The player's body is an object of 1.1 like the others: class bits 0x4 and 0x10, group number 0 [P4, P10]. Its
box is that of its four shapes, the outer sphere included. In a pair met in its own pass its shapes are the
first geometries, so contact normals point from the movable body towards the player's shape; in a pair met in
the other object's pass (that object active and earlier in the active objects) the order and the normal's sign
are reversed. Its contact rules for movable bodies (the touch flag, traction 1, the normal softening) are PH-8's
(`door` 3.3, `step`). It has no motion rays [P9]. The liquid damping reaches it (2.4: through its pre-step)
[C5]. Its class word never gets 0x8, 0x20 or 0x200 (only shells' setters write them) [R3, R6].

**Material.** Its four shapes get material index 0 (`default`) when the body is made (`cform` 5.1) [M1]. The
actor's spawn then, in one routine, (re)builds the body and gives its four shapes the material named by the
`material` key of the actor's section, looked up by name without regard to case (a section without the key is a
fatal error) [M1 – M3]: `config\creatures\actor.ltx`, section `actor`, `material = creatures\actor` (index 53)
[M5]. The material is kept with the body's settings, and a later rebuild of the body through those settings
gives it again [M1, M2]; nothing else sets the shapes' material [M4]. So the body carries `creatures\actor` from
the spawn on (**believed**: no step runs between the two parts of that routine). `creatures\actor`: friction 0.8,
damping 1, spring 0.8; not bouncy, not pass-through, not liquid [M5]. This settles `cform` 5.1's open point.

## 6. Which bodies a contact joins (summary)

- level contacts: the shape's body alone, against the fixed world (2.3, item 7); the joint goes into the
  shape's object's group;
- pair contacts: both bodies; the pair's groups merge for the step and a resting object is woken (1.2);
- ray contacts: the moving geometry's body and the other's (1.4);
- contacts with static-geometry objects (no body): the moving body alone.

## 7. The trader's door against the player's body

What the game does, from the data and sections 1 – 6:

1. **Classes.** The door object's shell: 0x4 (made), 0x2 (`ignore_static`, with its fixed bone `link`), no group
   number (its model has no user data, `shells` 10); the player's body: 0x4 | 0x10. The filter passes [P3, T1].
   Neither word changes later: the door's shell (of the ordinary shell kind [R5]) never gets the "ignore
   characters once at rest" mark, which only death shells get (1.3), so it never gets bit 0x20, whether it rests
   from the start (`resting` 4), has been woken or has come to rest again; 0x8 is not set in single play; the
   player's body never gets 0x8, 0x20 or 0x200 (section 5) [R1 – R6]. So the filter passes at every step, and the
   closed door at rest blocks the player's body from the moment its shell is entered in the index, as an active
   door does (2. below). The door's shell never meets the level; its frame element has no geometry (`shells` 3.5), so the leaf's box is
   its only geometry.
2. **When.** The door's shell starts at rest (`resting` 4) with its "to be collided" mark set (1.5); every step in
   which the player's box overlaps the door's (the outer sphere's reach included), the player's pass collides
   the pair. After the door wakes it is appended after the player's body among the active objects, so the pair
   stays in the player's pass as long as the player's body is earlier in the list (it is, unless it was itself
   re-activated later).
3. **Geometry.** Bottom, head and outer spheres against the leaf's box: library sphere–box, at most 1 contact each;
   torso cylinder against the box: the game's cylinder–box, up to 3 contacts (4.3). Normals from the leaf
   towards the player's shape.
4. **Properties.** Materials `creatures\actor` (the player's, section 5) and `objects\large_metal_trash`:
   μ = 0.8; `k = h × 0.8 × 1 × S`, `d = 1 × 1 × D`, so ERP 0.3243 and CFM 1.689 × 10⁻⁶ at `h` = 0.01 (before the
   player's rules); no bounce (`creatures\actor` is not bouncy), not pass-through, not liquid [T1, M5].
   Friction becomes infinite only if one of the
   player's shapes holds a crossed-triangle record from the floor (3.3).
5. **Handling.** The player's shapes' callbacks (the outer sphere's), then the player's body rules (PH-8);
   kept contacts join the player's body and the leaf's body, in the player's group; the door's group merges in
   (carrying its exact-solver request, 1.6) and the door is woken by the first kept contact [C3, T2] (1.5): the
   player's bottom sphere, torso cylinder or head sphere touching the resting door wakes it in that step (their
   contacts are kept, `door` 3.1); the outer sphere's overlap alone does not (its contacts only set the "blocked"
   flag, `door` 3.1) [SR-B12b]. From the merge on, the player's body itself requests the exact solver
   (**believed**, 1.6).
6. **Not involved.** Motion rays (neither object has the ray mark), liquid damping, the level collision of the
   leaf, the group limits (two small groups).

So an implementation needs: the pass and filter of section 1 (the player's pass at least), sphere–box (library)
and cylinder–box (4.3) contacts, the properties of 2.2, the joint attachment and wake of 1.2 and 2.3, and the
player's rules of PH-8.

## 8. Corrections to approved specifications

- `door` 3.2, which takes only active objects as collided: resting objects are collided too (1.5).
- `world` 4.2 ("no code sets the exact-solver mark"): a merge carries a group's exact-solver request into the
  colliding object's requests (1.6); fixed bones set it (`shells` 3.5).

## 9. Unestablished and believed

- The spatial index's box and ray query tests (1.1, 1.4).
- Which objects set class bit 0x100, the character state that sets 0x20 at once, whether code outside the
  physics part of the game library runs the routine giving a marked shell bit 0x20 (1.3); which other objects get
  the ray mark (1.4).
- The ray–cylinder test (1.4).
- The level: the triangle overlap tests (3.2); the crossing candidates' rules and the second record's choice
  (3.3); the box's cross-axis contact point and code-0 per-vertex depths (3.4); the cylinder's ordinary contacts
  (3.4); which geometries carry triangle callbacks.
- Cylinder–box's 0.70710677 test quantity; cylinder–cylinder and cylinder–plane (4.3).
- Which solver solves a merged group, by which object heads it (1.6); believed: an object that merged a group
  requesting the exact solver requests it from then on (the player's body after merging with the trader's door,
  section 7).
- **Believed**: geometries offset from their body sit in geometry transforms (4.1); the liquid rule's 10⁻⁷ test
  is on `k` (2.4); the object box's radius serves the index (1.1); no step runs inside the actor's spawn routine
  that sets the player's material (5).

## 10. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (merged view); binary `bin\xrGame.dll`.
- Data examined: `gamemtl.xr` (materials `default`, `objects\large_metal_trash`, `creatures\actor`),
  `config\creatures\actor.ltx` (section `actor`), the trader's door record and model (through the trader-door
  checks' readers).
- Public material: ODE 0.5.0 as released (its source, `include/ode/contact.h`, the collision utilities and
  `contrib/dCylinder/dCylinder.cpp`).
- Analysis: static analysis of `bin\xrGame.dll` (headless decompilation and instruction listings with Ghidra) of
  the objects' collision pass, the spatial queries, the class word and its setters, the filter and its masks,
  the pair and level collisions, the contact handling, its properties, callbacks and joint creation, the liquid
  record and force, the motion rays and the ray geometry, the level geometry kind and its colliders for spheres,
  boxes and cylinders (query, triangle cache, front side, crossed-triangle records, per-triangle contacts,
  push-out contacts), the cylinder kind and its colliders, the death shell's and missiles' class settings;
  the "ignore characters once at rest" mark (its one setter and every call through its slot), the routine giving
  bit 0x20 and its callers, every write of the class word, the shell kind of a type-3 physics object; the
  player's body's material (its setter and every call through its slot, the section's material key, the actor's
  spawn); the "to be collided" mark's setting and clearing, the box update after a step, the entry of a shell in
  the spatial index, waking by a contact and the merge of the groups' requests; constants read from the binary.
  No execution or debugging; the game was not run. The checks in `checks/physics-contacts/` reproduce every fact
  (the `SR-…` checks, moved here with sections 1.1, 1.2, 1.5, 1.6 and 7's facts from an earlier specification of
  the trader's door's shell, through `shapeless_root.py`).
- No leaked source code, derived projects or third-party documentation of the engine were used.

