# The player's body and movable bodies

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the contacts between the
player's collision body and the bodies of other objects: other physics shells (physics objects, items, doors) and
other creatures' character bodies. It states which of the player's shapes meet them, the player's contact rules for
such contacts, standing on, pushing and being pushed by them, the step rules then, the three further shapes the
player's body keeps for other creatures, what a contact records for later use (point, direction, cause, the
handler that may change the contact speed, the fall hit's bone), the outer sphere's use and how a ladder learns of
a character. It is PH-8 of `docs/behavior/physics-scope.md` (section 3; the first three items of its 2.1), and the
system home of the player's rules for movable bodies that `physics-contacts` 5 attributes to
`docs/behavior/trader-door.md` 3.3; that door appears here only as a worked example (section 9).

Builds on (not repeated):

- `docs/formats/level-cform.md` (cited as `cform`): the player's four shapes and boxes (5.1, 5.2), the contact
  rules against the level (5.4), the ground and obstacle contacts, the drive (6.2), the step probe and the step
  replacement (6.3), the airborne rules (6.4), the impact speed and the ground material (4.4);
- `docs/behavior/player-movement.md` (cited as `movement`): the stored acceleration `a_k`, the drive, the speed
  cap and the force cap (6.1 – 6.3);
- `docs/behavior/physics-contacts.md` (cited as `contacts`): which pairs are collided and in whose pass (1), the
  collision filter (1.3), contact properties and handling (2), the contact generation of each shape pair (4), the
  player's body as an object and its material `creatures\actor` (5);
- `docs/behavior/step-assistance-movable-bodies.md` (cited as `step`): the touch flag and the step-assistance
  decision while touching a movable body;
- `docs/behavior/physics-world.md` (cited as `world`), `docs/behavior/physics-library.md` (cited as `library`),
  `docs/behavior/physics-shell-shapeless-root.md` (cited as `root`), `docs/behavior/climbing.md` (cited as
  `climbing`), `docs/behavior/environment-hits.md` (cited as `env`), `docs/behavior/physics-scope.md` (cited as
  `scope`; terms *body*, *shell*, *step*).

Conventions:

- A **movable body** is any body other than the player's own; a contact is **with a movable body** when both its
  geometries are attached to bodies (`step` uses the same test). The level mesh and static-geometry objects
  (ladders, unbroken breakables) have no body.
- The **library normal** of a contact is the library's: from its second geometry towards its first (`contacts`,
  conventions). The **toward-body normal** is the normal pointing from the other surface towards the player's
  shape. In a pair met in the player's own pass the player's shapes are the first geometries (`contacts` 1.2, 5),
  so the library normal is the toward-body normal; in a pair met in the other object's pass it is its negation.
- `e` is a contact's error-reduction parameter as it stands when a rule reads it, CFM its constraint-force
  mixing, `μ` its friction (`contacts` 2.2). `r` is the body radius (0.35 for the player, `cform` 5.1), `E` the
  extents of the box the body was made with.
- A creature's **body** is its character body (`cform` 5); "other creatures" are stalkers and monsters.
- Statements are static (read from the shipped binaries and data; the game was not run). Inferred statements are
  marked **believed**; what could not be established is marked **unestablished**; **derived** marks a consequence
  of stated rules.
- Bracketed ids are the checks reproducing a statement: functions of `checks.py` in the dirty workspace's
  `checks/player-body-contacts/` (`python3 run_all.py` re-creates the evidence and runs them all).

## 1. The shapes that meet other objects

### 1.1 The player's body

The player's body is made by the same routine as every other creature's body: one body carrying the bottom
sphere, the torso cylinder, the head sphere and the outer sphere of `cform` 5.1, all in one collision space
[B1]. A creature's physics is made in one of three kinds: the player, stalkers, monsters [B1b, B1c]. The player's
kind adds, in single play only, **three further shapes**, one of each **kind 0, 1, 2**; other game modes destroy
them when the body is made, and the other creatures' bodies have none [X1, X3]:

- each is a cylinder of the game's cylinder kind (`contacts` 4.1), axis vertical, of length `E.y` and of the
  kind's radius, centred `E.y / 2` above the body centre, on the player's body, in the body's collision space; it
  does not meet the level (its "meets the level" mark is cleared, `contacts` 3.1) [X1, X1e];
- the radius is 0.1 when made; the player's loading sets kind 0 from `stalker_restrictor_radius`, kind 1 from
  `stalker_small_restrictor_radius` and kind 2 from `medium_monster_restrictor_radius` of the player's section,
  each only when the key exists [X1, X1b]. `config\creatures\actor.ltx`, `[actor]`: `.55;0.55;0.75;0.9`, `.55;`
  and `0.1`, read as numbers (the text after `;` is a comment): **0.55, 0.55, 0.1** [X1c];
- changing the box (crouching, `cform` 5.2, 5.3) resizes the four shapes only; the cylinders keep the size of
  the body's making [X1d]. The body is made with the movement's current box, which loading leaves at box 0
  (`ph_box0_size` 0.35, 0.9, 0.35: `E.y` = 1.8) [X1c, X1d]. So, standing or crouching, the three cylinders span
  **0.35 … 2.15 m** above the feet (centre 1.25 m) [N2] (**believed** that the body is not made again with
  another box).

### 1.2 The player's object box

The player's object box (`contacts` 1.1: the bounding box of its collision space) is that of all its shapes: the
four of `cform` 5.1 and, in single play, the three cylinders [B1, X1]. This corrects `contacts` 5, which names
the four shapes only. Standing, it reaches 0.7 m from the body axis (the outer sphere) and from the feet to
2.15 m above them (the cylinders) (**derived**; that the space's box is the union of its shapes' boxes as in
public 0.5.0 is **believed**).

### 1.3 Which callbacks a contact passes through

For a contact between one of the player's shapes and another object's shape, after the shapes' own callbacks
(`contacts` 2.3, item 3; the outer sphere's, section 6; the cylinders', section 3.5), the two objects' contact
callbacks run, the second geometry's object's first (`contacts` 2.3, item 5). A shell's does nothing
(`contacts` 2.3). The player's (single play) [C1]:

1. if the material of either side has the actor-obstacle flag (0x1000, `cform` 3.2), the contact is made;
2. a contact of one of the three cylinders is handled as section 3.5 states and goes no further;
3. every other contact, made or not, is handed to the contact rules (section 2; `step` 2.1).

Another creature's callback applies that creature's own contact rules (the same rules as section 2, for its own
body) to the same contact, then section 3.4's additions [X3]. So a contact between the player and another creature
goes through both bodies' rules, in the order of the callbacks.

No shipped dynamic material (29, the materials of shells' elements and creatures) is pass-through,
actor-obstacle, deadly or liquid; the actor-obstacle flag is on `materials\fake` only [G2]. So item 1 never acts
for a movable body's shape, and the pass-through branches of the rules do not occur for movable bodies
(**believed** for every movable shape: that each carries a dynamic material). This settles the open point of
`step` 2.1's review note on actor-obstacle materials for movable bodies.

## 2. The contact rules for a contact with a movable body

### 2.1 What the rules distinguish

The rules take the player's shape to be the contact's first geometry when that is one of the four shapes, else the
second; the material they use is the other side's [R1]. A contact is with a movable body when both geometries have
a body [R1]. Contacts with movable bodies follow the rules of `cform` 5.4 and 6.3 and `step` with the differences
of 2.2.

### 2.2 The rules

For a contact handed to the rules, in this order [R4]:

1. **Contact flags and touch flag.** A made contact first sets the step's contact flags, so it is "a contact" for
   the departure point and the airborne rules of `cform` 6.4 [R4, R4b]; the touch flag is set (`step` 2.1) whether
   or not the contact is made; the step replacement's test (`cform` 6.3) runs; a contact not made then ends here
   [R4].
2. **Drop and step replacement** use the toward-body normal in either order of the pair: the bottom sphere's
   contact whose toward-body normal points down is dropped (unless the replacement acts), and the replacement's
   tests and its new normal (straight up from the surface towards the body) are in toward-body terms [R3b].
3. **Head sphere**: its rule of `cform` 5.4 (friction 0, its own softening with 0.5, "blocked") applies as for the
   level [R4].
4. **Impact.** The movable-body impact speed of section 5.1 is computed and may be recorded [R4, I1].
5. **Ground material** (section 5.6) [G1].
6. **Traction**: the contact's value is **1** (`cform` 5.4 already states it) [R2b].
7. **Count**: the contact is counted (`step` 3.3) [R4].
8. **Ground and obstacle**: chosen among these contacts as among the level's, by the toward-body normal: the library
   normal taken as it is when the player's shape is the first geometry and negated when it is the second [R3].
9. **Normal softening**: with `n` the **library normal**, `q = 20.1 − 19.1·n.y`, `f = 1 / ((1 − e)·q + k·e)`,
   CFM × `f`, ERP = `k·f·e`, where **k = 5** for a contact with a movable body and 0.5 otherwise [R2]. (For the
   level `n` is the negated toward-body normal, which gives `cform` 5.4's `20.1 + 19.1·n.y` with the toward-body
   normal.)
10. **Friction**: 0 while movement is requested (`|a_k| > 0.1`); otherwise `(1 + 3·s)·T·μ`, `s` 1 during step
    assistance and 0 otherwise, `T` the running traction (`cform` 5.4) [R2b].
11. **Impact by velocity**: the velocity impact of `cform` 4.4 is computed for this contact too, and may be
    recorded (section 5.1) [I2, R4].

While climbing and moving, `climbing` 3.4's rule acts first on these contacts as on the level's.

### 2.3 The normal's sign

Rules 2, 8 and the impacts (section 5) correct for the pair's order; the normal softening (rule 9) does not: it
reads the library normal's `y`, so its value for one surface depends on whose pass met the pair (**derived**). With
`creatures\actor` (the player's material, `contacts` 5) against a material of friction, spring and damping 1 at
`h` = 0.01: `e` = 0.324324, CFM 1.689 × 10⁻⁶ before the rules (`contacts` 7); after rule 9 [N1]:

| Surface | Library normal `y` | ERP | CFM |
|---|---|---|---|
| vertical face | 0 | 0.10667 | 1.111 × 10⁻⁷ |
| top of a body, pair in the player's pass | +1 | 0.70588 | 7.353 × 10⁻⁷ |
| top of a body, pair in the other's pass | −1 | 0.057692 | 6.01 × 10⁻⁸ |
| (a level wall, k = 0.5) | 0 | 0.011799 | — |

Which pass meets a pair is `contacts` 1.1's: the player's when its body comes first among the active objects.

## 3. Standing on, pushing and being pushed

### 3.1 Standing on a movable body

- A contact of the bottom sphere with the top of a body is a candidate ground contact like a floor's (rule 8);
  when its toward-body normal has `y > 0.70710677` the ground is walkable and the drive follows its plane
  (`cform` 6.2, `movement` 6.2) (**derived**).
- The body's own work (its pre-step, post-step and drive) reads and drives the player's own body only: it looks up
  no other body and reads no other body's motion [P1]. So the player is not carried by a moving body except
  through the contact forces: with friction 0 while moving, and `(1 + 3·s)·T·μ` with `T` at least 1 while not
  moving (**derived** from rules 6 and 10).
- The ground material becomes the other shape's material while the bottom sphere touches it (section 5.6) [G1].
- The impact of landing on a body is section 5.1's [I1, I2].

### 3.2 Pushing

A made contact becomes a contact joint attached to both bodies (`contacts` 2.3, item 7): the other body receives
the reaction, and a resting shell is woken by the first kept contact (`root` 4.2). Nothing else in the player's
code pushes another body [P1]. How far a body yields under the player's movement forces (`movement` 6.2, with `T`
= 1 for these contacts) is the solver's result (**unestablished** statically).

### 3.3 Being pushed

The player's body receives the reaction of every contact joint attached to it (`contacts` 2.3, item 7). After each
step its velocity is limited by `movement` 6.3 (to `0.1·|a|` while moving on the ground, 150 m/s otherwise, with
that section's position reset), so a push while walking on the ground raises the player's speed at most to that
limit; a push while standing still is limited only by 150 m/s (**derived**). The pending vector from other
objects' impulses (`movement` 6.1) is PH-11's.

### 3.4 Other creatures

Another creature's body is made like the player's (the four shapes on a body, 1.1) [B1], so its shapes are movable
bodies for the player's rules, and the player's for its rules (this settles the **believed** point of `step` 2.1).
A contact between a player's shape and a creature's shape goes through both callbacks (1.3), the second
geometry's object's first: each body's rules act on the same contact, each for its own shape, so the softening of
rule 9 and the friction of rule 10 are applied by both, the second on the first's result (**derived**). The
creature's callback then sets the contact's friction to 0 while that creature moves or is in either of two further
states of its body (**unestablished** which), and, when both geometries belong to character bodies, sets that
creature's touch flag and clears its obstacle contact for the step [X3]. The impact of section 5.1 uses the
creature's body velocity and mass.

### 3.5 The three cylinders and other creatures

Each creature's body has a **spacing type** (0, 1, 2, 3 or 4): the player's 4; stalkers' 0; monsters' 3; a creature
section's key `actor_restrictor` sets it at loading by the tokens `actor` 4, `medium_monster` 2, `stalker` 0, `none`
3 [B1b, X2c]. In the data only `medium_monster` occurs, in `config\creatures\m_dog.ltx`, `m_boar.ltx` and
`m_burer.ltx` (and the sections that inherit it) [X2c]. Each body also has a setting that is off for stalkers'
bodies and on for every other creature's, the player's included, when the body is made [B1b] (**believed**: nothing
changes it later).

A contact of a cylinder of kind `k` [X2]:

- its own callback marks it not made unless both geometries have bodies and both belong to character bodies (a
  shell's and a ladder's objects are not character bodies, so a cylinder contact with them is never made [X2]);
  then, first, the player's kind-0 rule below runs for `k` = 0, and the contact is made exactly when the other
  creature's spacing type equals `k` (that creature also gets a mark of its own, **unestablished** use, PH-22);
- **kind-0 rule**: when the other creature's type is 0 and its body radius is below 1.5 × the kind-0 radius
  (0.825), its type and its requested type become 1; when its type is 1 and its radius is above 0.825, its
  requested type becomes 0 and, if the contact's depth is at most 0.05, its type too [X2b]. How a creature's
  requested type later becomes its type is PH-22's (**unestablished** here).

The player's object callback then, for every cylinder contact, sets the step's "blocked" flag and the contact's
friction to 0; such contacts never reach the rules (no touch flag, no count) [C1]. A cylinder contact still made:

- with a creature whose setting is off (a stalker): a **capped contact joint** (`library` 5.2, item 5: normal force
  at most 100,000) is made in the step's contact group, added to the head of the player's merged group (waking the
  player's body), and attached to the player's body **alone**, the other side the fixed world; the contact is then
  marked not made, so no ordinary joint follows, and the step's traction so far is multiplied by **0.1** [C1b];
- with a creature whose setting is on: the callback leaves it made, so it becomes an ordinary contact joint on both
  bodies (`contacts` 2.3) [C1b].

Consequences (**derived**): a stalker (type 0) whose shape reaches into the player's kind-0 cylinder is switched to
type 1, and the kind-1 cylinder (radius 0.55) then holds the player away from it at that radius, pushing only the
player; monsters of type 2 meet only the kind-2 cylinder (radius 0.1, about the player's axis), as an ordinary
contact; monsters of type 3, every shell and every ladder meet the cylinders only as "blocked" sources.
**Disputed in review:** the switch of a stalker to type 1 holds only when that stalker's body radius is below
0.825, and this specification states no stalker body radius; treat it as unestablished (PH-22).

## 4. The step rules then

`step` holds while touching movable bodies, with these additions:

- **"Blocked" sources.** Besides the head sphere, the outer sphere and the probe (`step` 3.2), every contact of the
  three cylinders sets it (3.5): any movable shape reaching within 0.55 m of the player's axis between 0.35 and 2.15 m above
  the feet, other creatures' included [C1, X1]. These are the "three further shapes" of `step` 2.1. Its review note
  on them is corrected: the capped joint is attached to the player's body alone, not to the other's, under the
  conditions of 3.5, and the traction factor is 0.1 [C1b].
- **The touch flag** is also set by another creature's outer sphere reaching a player's shape (the creature's
  sphere marks the contact not made; it still reaches the player's rules, rule 1) (**derived**).
- In the step after the last touching step (touching through the previous-step flag alone, `step` 2.2), "blocked"
  is clear unless set by another source, so `step` 3.2's switch-on and recording can act on that step's other
  contacts (**derived**; `step`'s review gap).

## 5. The contact record

### 5.1 The impact speeds

Each made contact handed to the rules gives an impact speed, compared with the **kept impact** (section 5.3):

- **by velocity** (every made contact): `cform` 4.4's rule, with the player's velocity only and the other
  material's friction and impact factor [I2]; the movement update's contact speed is the kept impact (5.3) [I4b];
- **by energy** (contacts with a movable body, before the velocity one): with `n` the library normal,
  `a = n·v₁` and `b = n·v₂` (`v₁` the player's velocity, `v₂` the other body's), and only while the two approach
  (`a ≤ b` when the player's shape is the first geometry, `b ≤ a` when it is the second):
  `W = ½·K·m₁·a² + ½·m₂·b² − ½·(m₁·a + m₂·b)² / (m₁ + m₂)`, `m₁` the player's mass (80), `m₂` the other body's;
  the speed is `√(2W / m₁)` × the other material's impact factor, 0 when `W ≤ 0` [I1]. `K` is the square of
  `ph_collision_damage_factor` of the creature's section (`[actor]`: 1.0, so `K` = 1) [I1b]. With `K` = 1 this is
  `|a − b|·√(m₂ / (m₁ + m₂))` × factor: the player at 3 m/s into a resting 10 kg body gives 1.0 × factor [N1b].

### 5.2 What is recorded

A speed larger than the kept impact replaces it and records [I1, I2, I3]:

- the whole contact: point, library normal, depth, the two geometries;
- the **sign**: +1 when the player's shape is the first geometry, −1 when the second;
- by energy: "from a movable body", the other object's **id** and its **handler** (section 5.5) — unless the other
  object is marked for destruction (then nothing is recorded and the kept impact stays) [I3];
- by velocity: not "from a movable body" and no object; the handler is left as it was [I2].

### 5.3 Its lifetime

The kept impact is not cleared by the steps: it is the largest since the last movement update (`movement` 3.2),
which reads it and clears it; at the end of that update the object id and the handler are cleared too [I3b]. The
point, normal, sign and "from a movable body" stay until the next record (**derived**), so a fall hit sent for
deadly ground in an update without contacts (`env` 3.2) carries the last ones recorded. This settles `cform` 4.4's
believed hand-over: the movement update's contact speed is this kept impact [I3b, I4b].

### 5.4 How the fall hit reads it

`env` 3.2's fields [I4, I5]:

- **direction**: the recorded normal × the sign, which is the toward-body normal (pointing from the other surface
  into the player);
- **position**: the recorded point;
- **cause** (hitter and weapon): the player itself unless the record is "from a movable body"; then the object the
  recorded object names as responsible (its own answer, when it exists, has one and is not marked for destruction),
  else the player itself. A physics object names none, so for it the cause is the player [I4, I4c]; what other
  kinds of objects name is **unestablished**;
- **bone**: a ray from the recorded point along the direction, of length `r` (0.35 m), tested against the player's
  own collision form (its model's bones); the id of the first element it hits (**believed**: a bone number); if
  none, the bone whose position (**believed**: in the current pose) is nearest the point (squared distance in the model's frame; the first bone
  on ties) [I5]. This settles `env` 3.2's ray length.

### 5.5 The handler

After computing the contact speed and the damage fraction (`env` 3.1), the movement update calls the recorded
handler when there is one, giving it the crash speeds and the speed and fraction, which it may change, before they
are used [I4b]. So the handler `env` 3.1 leaves open is not the player's: it is supplied by the movable object of the
record. A physics object (`physic_object` and destroyable kinds) supplies none from its spawn on; its own setter,
which requires a shell, can set one, and which code calls it was not traced (**unestablished**; none found as a
direct reference) [I4c]. The handlers of other kinds of objects are **unestablished**.

### 5.6 The ground material

A contact of the bottom sphere with a movable body offers that body's shape material; every other contact offers the
bottom sphere's last level-triangle material. The first contact handled in a step, and any later one while the kept
material is solid, replaces it: by the other side's material when that is pass-through, else by the offered one
[G1]. So standing on a body, the ground material (`cform` 4.4, footsteps, deadly ground) is the body's material;
no dynamic material is deadly [G2] (**derived**).

## 6. The outer sphere

- It does not meet the level (`cform` 5.1, `contacts` 3.1).
- Its own callback marks each of its contacts not made and sets "blocked" (`step` 2.1).
- Its contacts reach the rules (1.3): they set the touch flag for a movable body; a contact with a pass-through
  material, not made, records a velocity impact and offers the ground material before the rules end, which does
  not occur for movable bodies (1.3, **derived**); every other outer-sphere contact ends after rule 1 [R4, G1].
- It counts in the player's object box (1.2), so it widens the box query of the player's pass and with it the
  ladder notification (section 7).

## 7. The ladder notification

For every pair met in an object's collision pass (`contacts` 1.1, item 2: the box query, the "to be collided" mark
and the filter), the other object of the pair is told of it first, before the contact limits and before any contact
is made [X4]. A ladder's object, told by a character body, hands itself to that body's ladder finding (`climbing`
2.4); a character body told by a ladder does nothing [X4]. So the player learns of a ladder in every step in which
the player's pass meets the ladder's object: when the player's object box (1.2) and the ladder's box overlap by the
spatial index's test (`contacts` 1.1, **unestablished** in detail), whatever contacts follow. This replaces
`climbing` 2.4's belief (bounding volumes overlapping; the ladder's radius is not used by it).

A ladder object woken by a kept contact (`contacts` 1.2) is listed for that step only and takes itself off in its
post-step work [X4b]; that its mark is set again then, so that the player's pass meets it in the next step, is
**believed** (`root` 4.1's rule for objects active in a step).

## 8. Corrections and settlements

- `contacts` 5: the player's object box includes the three cylinders in single play (1.2).
- `step` 2.1: the three further shapes are the cylinders of 1.1; their contacts and the review note on them: 3.5,
  4. Other creatures' shapes are on bodies (3.4). No dynamic material has the actor-obstacle flag (1.3).
- `trader-door` 3.3: the player's material is `creatures\actor` (`contacts` 5), not index 0, so the door's example
  values are section 9's.
- `cform` 4.4: the movement update's contact speed is the kept impact of 5.3; the ground material refinement of 5.6.
- `env` 3.1, 3.2: the handler (5.5), the fields and the bone ray's length (5.4).
- `climbing` 2.4: section 7.

## 9. Worked example: the trader's door

What the rules give for the leaf of the trader's door on `l01_escape` (`contacts` 7):

- the bottom sphere, torso cylinder and head sphere give made contacts with the leaf's box; the outer sphere and,
  when the leaf is within 0.55 m of the player's axis, the cylinders give "blocked" only (the cylinders' contacts
  with a shell are never made) (sections 3.5, 6);
- the touch flag is set from the first overlap of the outer sphere (`step` 5);
- materials `creatures\actor` and `objects\large_metal_trash`: μ = 0.8, ERP 0.324324 and CFM 1.689 × 10⁻⁶ before the
  rules (`contacts` 7); on the leaf's vertical face, after rule 9, ERP 0.10667 and CFM 1.111 × 10⁻⁷ [N1];
- traction 1; friction 0 while walking: the walking player slides along the leaf as along a frictionless wall,
  whose push the door's motor and stop resist (`trader-door` 2.4); while standing still, friction 0.8 × (1 or 4);
- made contacts join the player's body and the leaf's body, waking the door's shell (`root` 4.2);
- an energy impact uses the leaf body's mass and velocity; the door is a physics object, so the fall hit's cause
  for a record from it is the player (5.4).

## 10. Unestablished and believed

- That the body is not made again with a box other than box 0 (1.1); that the object box is the union of the shapes'
  boxes (1.2); that every movable shape carries a dynamic material (1.3).
- How far a pushed body yields (3.2); the two further states in which another creature's callback zeroes the
  friction (3.4).
- The other creature's mark set by a cylinder test and how its requested spacing type becomes its type (3.5,
  PH-22).
- Which code sets a physics object's handler; the handlers and the responsible objects named by other kinds of
  objects (5.4, 5.5).
- The spatial index's box test (7); that a woken ladder's mark is set again after its step (7).

## 11. Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`
  (merged view); binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`.
- Data examined: `gamemtl.xr` (every material's flags), the configuration tree from `config\system.ltx` (the
  sections `[actor]`, every section with `actor_restrictor`), `config\creatures\actor.ltx`.
- Analysis: static analysis of the binaries (headless decompilation and instruction listings with Ghidra) of the
  creatures' body creation and its kinds, the player's three further shapes (creation, radii, callbacks, removal),
  the player's and other creatures' object contact callbacks, the contact rules and their treatment of movable
  bodies, the step replacement's normal, the impact computations and the contact record, its readers in the
  movement update and the fall hit, the bone ray, the ground-material record, the handler getters and setter of
  physics objects, the engine's object destruction flag, the pair handling and the ladder's notification, the
  body's pre-step, post-step and drive; constants read from the binary. No execution or debugging; the game was not
  run. The checks in `checks/player-body-contacts/` reproduce every fact.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Whether a ladder's "to be collided" mark is set before any contact with it (when it enters the spatial index), so that the player's pass meets a ladder that was never woken; section 7 covers only the mark after a woken step.
- The player's rules for a contact with a static-geometry object (no body: ladders, unbroken breakables): the normal's sign in rule 9's softening, the drop and step replacement, and the ground material it offers.
- Sections 8 and 9 cite `trader-door` (3.3 and 2.4, the door's motor and stop); once that document is gone they need the system specification that holds the door's joint, motor and stop.
