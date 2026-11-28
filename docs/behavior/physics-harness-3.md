# The physics library against the shipped one, round 3: the arithmetic behind the remaining differences

Behavioural and functional findings, for the shipped `bin\ode.dll`, of the arithmetic behind the differences
round 2's harness run still showed, so that our ODE 0.5.0 build can be patched to give its results bit for bit.
It extends `docs/behavior/physics-harness-2.md` (cited as `round 2`), `docs/behavior/physics-harness.md`
(`round 1`) and `docs/behavior/physics-library.md` (`library`), with their conventions: public ODE 0.5.0's API
names and concepts, single precision, "h" the step length, a 3-term sum written by its grouping (`(x + y) + z`
"source order", `(y + z) + x` the last two first, `(x + z) + y` the first and last first), each operation rounded
to single precision and flushed (the game's environment, `library` 7.2) unless marked **x87**. Scenario names
are the harness's (`tests/odeharness/scenarios.c`), the round-3 scenarios included (`finite`, `mass-ccylinder`,
`pairs-posed`, `solve`, `held`, `held-nodaz`, `set-rotation`, `quaternion`, `rel-force`, `angular-mass`).

**x87**: an operation the shipped library does on the x87 unit, at the 24-bit precision the game sets
(`library` 7.2). It rounds the significand as single precision does, but its exponent range is wider: it is
never flushed, and a result below the normal range is rounded twice (to 24 bits, then to the denormal grid when
stored). In the normal range it equals the single-precision operation.

How the findings were established. Unlike rounds 1 and 2, every rule here was first **read from the shipped
library's instructions** (static analysis of `bin\ode.dll`: the dataflow of every stored value, which makes the
grouping of each sum and product explicit; where code loops, the loop counts follow from the sizes). The
decompiler's printed expressions were not relied on for grouping: they flatten nested sums and misstate
several. Then each rule was checked against the black-box results:

- **static**: read from the instructions; a check holds the stated rule to the expressions read;
- **reproduced**: a model with the rule recomputes, bit for bit, every recorded output of that kind in the named
  scenario (from recorded or regenerated inputs, or from the shipped library's own recorded state after the
  previous step); the same model with our build's rule recomputes our build's outputs, and each side's rule is
  shown to miss some of the other side's, so the rule is discriminating;
- **believed** and **unestablished** as before.

Our build is public 0.5.0 with `third_party/ode`'s patches as they stand after round 2. Bracketed ids are
check scripts (section 11).

## 1. Summary

| Scenario | Round-2 difference | Cause (section) | Now |
|---|---|---|---|
| free | the body with a finite-rotation axis | the finite rotation about an axis (7.1) | reproduced, every step |
| pairs, rays, pairs-posed | every collider but box–plane's deepest contact; box–box contact counts | the colliders' arithmetic (2), and box–box's added contacts' face centre (2.4) | reproduced, every case |
| contacts, contacts-nodaz, heavy | from the first steps | the colliders (2), the capped-cylinder mass (7.2), the QuickStep solve (3) | reproduced, every step after the first |
| contacts-exact, exact-step | the exact stepper's solve | the colliders (2), the exact stepper (4) | reproduced, every step |
| joints, joints-nodaz | the solvers and the joint rows | the joint rows (5), both steppers (3, 4) | reproduced, every step, both steppers |
| worlds | the jointed bodies | the ball rows (5), QuickStep (3) | reproduced, every step (6) |
| denormal-nodaz | ours flushes | `dNormalize4` unflushed (7.5) | reproduced |
| finite, mass-ccylinder, quaternion, rel-force, angular-mass, set-rotation | (round 3) | 7.1 – 7.6 | reproduced, every case |
| solve | (round 3) | 3, 4 | reproduced |
| held, held-nodaz | (round 3) | 3, 4, 5.3 | reproduced, every case, each with its own flushing (6) |

## 2. Colliders (#887)

The colliders' arithmetic was read from the shipped library's instructions (**static**) and then checked
against the recorded results: the pairs scenario (every pair kind, at most 1 and at most 8 contacts asked
for), the rays scenario (box and capped-cylinder targets), every pile scenario's contacts, and the round-3
run's posed pairs (`pairs-posed`: sizes varied, geometries away from the origin, planes of general normals,
every geometry's state read back and recorded; our outputs from the runner of that run). Inputs not
recorded (rotations, positions) were regenerated with the scenarios' own generator and round 2's 2.2; a pile
step's geometries were placed at that side's recorded body states of the step before. **Reproduced** means the
rule recomputes every recorded result bit for bit (count, every position, normal and depth); the same models
with our build's rule (public 0.5.0 with `third_party/ode` applied) reproduce every result of ours, and the
counts of the other side's results each rule gets right are printed by the checks.

Notation as round 2: a dot product `a · b` is a 3-term sum of the products of components x, y, z; "source
order" is `(x + y) + z`, "`(y + z) + x`" adds the last two products first. Products and differences inside a
term are as public 0.5.0 unless stated. "R ᵀ v" (a vector in a box's frame) has component i = column i of the
rotation · v; "R v" has component i = row i · v.

### Results

| Kind | Shipped rule: shipped / ours | Our rule: ours / shipped | Check |
|---|---|---|---|
| box–box (3000) | 3000 / 990 | 3000 / 990 | K1 |
| sphere–box (3000) | 3000 / 2022 | 3000 / 2022 | K2 |
| capped cylinder–box (3000) | 3000 / 2070 | 3000 / 2070 | K3 |
| two capped cylinders (3000) | 3000 / 2646 | 3000 / 2646 | K4 |
| box–plane (3000) | 3000 / 3000 | 3000 / 3000 | K5 |
| ray–box (1000) | 1000 / 980 | 1000 / 980 | K6 |
| ray–capped cylinder (1000) | 1000 / 991 | 1000 / 991 | K7 |
| pile steps after the first: contacts, contacts-nodaz (399 each) | 399 / 18 | 399 / 33 | K8 |
| contacts-exact (199), heavy (149) | 199 / 189, 149 / 146 | 199 / 194, 149 / 146 | K8 |
| round-3 posed pairs, per kind: box–box, sphere–box, capped cylinder–box, two capped cylinders, box–plane, sphere–plane, capped cylinder–plane (1200 each) | 1200 each / 590, 806, 1124, 1156, 880, 940, 898 | 1200 each / the same counts | K13 |
| posed rays: box, capped cylinder, sphere (600 each) | 600 each / 594, 594, 600 | 600 each / 594, 594, 600 | K13 |

The pile scenarios' contact lists (the simple space visits the bodies' geometries in creation order, then the
plane) are reproduced whole, the contact counts included: the contacts scenario's count difference of round 2
(three contacts against two for a box pair) is the box–box rule below [K8].

### 2.1 Sphere–box (reproduced [K2])

- The sphere centre's coordinates in the box's frame, `t = R ᵀ (sphere centre − box centre)`: each component
  summed `(y + z) + x`.
- Centre outside the box: the clamped point `q = R t`, each component `(y + z) + x`; `r` = the offset minus
  `q`; depth = radius − √(r · r), the squares summed `(y + z) + x` (the square root, the sum and the subtraction
  are x87 arithmetic at 24-bit precision, which rounds as single precision does in the normal range); the
  contact point `q` + box centre; the normal `r` normalised as `library` 3.5 says (squared length in source
  order, times 1 / √).
- Centre inside: the normal is `R` times the unit vector of the nearest face, each component `(y + z) + x`
  (one non-zero term: only a zero's sign can depend on it); depth and point as public 0.5.0.

### 2.2 Capped cylinder–box (reproduced [K3])

The cylinder's segment (centre ± half the length along its axis, as public 0.5.0) against the box:

- The segment's start and direction in the box's frame, `s = R ᵀ (start − box centre)` and
  `v = R ᵀ (end − start)`: component 0 in source order, components 1 and 2 `(y + z) + x`.
- The derivative of the squared distance after a clipping boundary is the three terms summed in source
  order, starting from the first term (public 0.5.0 adds them to an initial zero; the same except for a
  zero's sign); the initial derivative is the three products subtracted in index order from −0.
- The box's closest point: `R` times the clamped point, row 0 in source order, rows 1 and 2 `(y + z) + x`,
  then the box centre added.
- The contact from the two points (the segment point with the cylinder's radius, the box point with radius
  0): the distance's squares summed in source order; normal = difference × (1 / distance); point and depth as
  public 0.5.0.

### 2.3 Two capped cylinders (reproduced [K4]; the near-parallel case **static** [K11])

- `axis1 · axis2`, which chooses the near-parallel case, is summed `(y + z) + x`.
- General case, the closest points of the two segments: every dot product (the four end-to-end projections
  on each segment, both squared lengths, the products with the trial normals, and `axis1 · axis2` there)
  summed `(y + z) + x`; the rest as public 0.5.0, the contact by the two-sphere rule above (squares in source
  order).
- Near-parallel axes (not exercised by the recorded data): `axis1 · (centre1 − centre2)` summed
  `(y + z) + x`; when two contacts are made, the first by the two-sphere rule above, the second with the
  distance's squares summed `(y + z) + x`; the single contact in the middle of the overlap also with
  `(y + z) + x`.

### 2.4 Box–box (reproduced [K1]; 385 cases with an edge–edge axis, 702 with a face axis)

With `p` = centre 2 − centre 1, `A`, `B` the half sides, `Rij` = column i of rotation 1 · column j of rotation 2
and `Qij = |Rij|` (public 0.5.0's names):

- `pp = R1 ᵀ p`: component 0 in source order, components 1 and 2 `(y + z) + x`. `R11` in source order, the
  other eight `Rij` `(y + z) + x`. The tests on box 2's axes use column j of rotation 2 · `p`, `(y + z) + x`.
- The fifteen separation tests (computed in x87 at 24-bit precision [K12]) sum their four terms as:

  | Test | Shipped | Public 0.5.0 and ours |
  |---|---|---|
  | 1 | `(B0Q11 + (B1Q12 + B2Q13)) + A0` | `((A0 + B0Q11) + B1Q12) + B2Q13` |
  | 2 | `((B2Q23 + B1Q22) + B0Q21) + A1` | `((A1 + B0Q21) + B1Q22) + B2Q23` |
  | 3 | `(B0Q31 + (B1Q32 + B2Q33)) + A2` | `((A2 + B0Q31) + B1Q32) + B2Q33` |
  | 4 | `((A1Q21 + A2Q31) + A0Q11) + B0` | `((A0Q11 + A1Q21) + A2Q31) + B0` |
  | 5 | `((A2Q32 + A1Q22) + A0Q12) + B1` | `((A0Q12 + A1Q22) + A2Q32) + B1` |
  | 6 | `((A1Q23 + A2Q33) + A0Q13) + B2` | `((A0Q13 + A1Q23) + A2Q33) + B2` |
  | 7 | `((B2Q12 + B1Q13) + A1Q31) + A2Q21` | `((A1Q31 + A2Q21) + B1Q13) + B2Q12` |
  | 8 | `(A1Q32 + (B2Q11 + B0Q13)) + A2Q22` | `((A1Q32 + A2Q22) + B0Q13) + B2Q11` |
  | 9 | `((A1Q33 + B1Q11) + A2Q23) + B0Q12` | `((A1Q33 + A2Q23) + B0Q12) + B1Q11` |
  | 10 | `((B2Q22 + B1Q23) + A0Q31) + A2Q11` | `((A0Q31 + A2Q11) + B1Q23) + B2Q22` |
  | 11 | `((B0Q23 + B2Q21) + A0Q32) + A2Q12` | `((A0Q32 + A2Q12) + B0Q23) + B2Q21` |
  | 12 | `A2Q13 + ((B1Q21 + B0Q22) + A0Q33)` | `((A0Q33 + A2Q13) + B0Q22) + B1Q21` |
  | 13 | `A1Q11 + ((B2Q32 + B1Q33) + A0Q21)` | `((A0Q21 + A1Q11) + B1Q33) + B2Q32` |
  | 14 | `((B2Q31 + B0Q33) + A0Q22) + A1Q12` | `((A0Q22 + A1Q12) + B0Q33) + B2Q31` |
  | 15 | `(A0Q23 + (B0Q32 + B1Q31)) + A1Q13` | `((A0Q23 + A1Q13) + B0Q32) + B1Q31` |

- In the edge tests (7 – 15) the separation and the three normal components are **multiplied by** the
  reciprocal of the length (one rounded `1 / l`), not divided by the length. The normal of an edge axis is
  rotation 1 times that vector, row 0 in source order, rows 1 and 2 `(y + z) + x` (one term is zero).
- Edge–edge contact: the signs that place each box's edge (normal · axis j of each box) are summed
  `(x + z) + y`. The added contacts (`library` 6.2 item 2) use `(y + z) + x` for every dot product: the choice
  of each box's axes M and S (**static**: not distinguished by the data), the side test of the first candidate
  of each box, the line's crossing of the face plane (the line point, the face centre and the direction, each
  projected on the face's axis) and the two in-face tests, and both projections on M in the depth.
- **The centre of the last two candidates.** Candidates 5 and 6 (`library` 6.2 item 2: box 1's face on its own
  edge axis k₁, σ = +1 and −1) take the face centre from **box 2's centre**:
  `F = p₂ + σ (s₁[k₁] / 2) axis_k₁ of box 1`; the crossing, the in-face tests (box 1's axes and sides) and the
  depth are otherwise as for the other candidates. Without this the shipped rule gets 2782 of the 3000 results
  [K1]; it is the cause of the pairs' and the contacts scenario's contact-count differences.
  **Disputed in review:** not the contacts scenario's: without it the shipped rule still reproduces the
  contacts of all 399 contacts steps [K1]; it is needed for 218 of the 3,000 pairs results only.
- Face case: the normal in the incident box's frame, all three components `(y + z) + x`; the face centre's
  two coordinates and the four direction cosines (`m11` – `m22`) `(y + z) + x`; of the incident face's corners,
  the third is `(k3 + k1) + c1` and `(k4 + k2) + c2` (public 0.5.0: `(c1 + k1) + k3`, `(c2 + k2) + k4`), the other
  six as public; each clipped point is `(k1 axis_a1 + k2 axis_a2) + centre` per component (public: the centre
  first, then the two terms); its depth `Sa[codeN] − normal2 · point` with the dot `(x + z) + y`. The polygon
  clipping is as public 0.5.0.

### 2.5 Box–plane, sphere–plane, capped cylinder–plane, capped cylinder–sphere

- Box–plane (reproduced [K5, K13]; read from the instructions [K9]): the normal · the box's first axis in source order,
  · the second and third axes `(y + z) + x`; depth = (½ the half-extent sum (round 2's 2.7) + offset) −
  normal · centre, that dot `(y + z) + x`; the further contacts as public 0.5.0. The pairs scenario's plane normal lies
  along a coordinate axis, so only the round-3 run's posed pairs (general normals, boxes away from the origin)
  exercise these orders [K13].
- Sphere–plane: normal · centre `(y + z) + x`; capped cylinder–plane: normal · axis and normal · end point
  `(y + z) + x` (read [K10], reproduced by the posed pairs [K13]); capped cylinder–sphere: axis · (sphere
  centre − cylinder centre) `(y + z) + x` (read [K10]; the piles reproduce it [K8]).

### 2.6 Rays

- Ray–box (reproduced [K6]): the ray's start and direction in the box's frame, every component
  `(y + z) + x`; each slab's two parameters `(±h − s) × (1 / v)` with one rounded reciprocal of the direction's
  component per axis (public 0.5.0 divides).
- Ray–capped cylinder (reproduced [K7]; each of the six sums is needed [K7]): the start's position along the
  axis, the squared distance from the axis `q · q`, `axis · direction`, `A = r · r`, `q · r` (doubled) and the
  hit's position along the axis are all summed `(y + z) + x`; the inside test's squares in source order
  (**static**: not distinguished); the side normal normalised as `library` 3.5; the caps by the ray–sphere rule of
  round 1 (identical on both sides).

### 2.7 Unestablished

- Box–box contact culling when more contacts are found than asked for and more than one is asked for (the
  centroid and angle arithmetic): not exercised (1 or 8 asked for) and not read: **unestablished**.
- The near-parallel capped cylinders: established **static** only [K11]; neither the pairs nor the round-3
  run's posed pairs contain a near-parallel case (a scenario with axes within the tolerance would settle it).
- The pile scenarios' first step (the bodies' poses before it are not recorded; they follow from 2.2 and 2.3).
- The x87 parts (the box–box tests, the sphere–box depth, the near-parallel distances) neither flush nor
  produce single-precision denormals; whether that ever matters is **believed** not (only for values near
  the smallest normal number).

## 3. The QuickStep step (#888)

How the rules were established: each grouping below was read from the shipped library's instructions
(**static**; the decompiler's printed expressions were not relied on, as they misstate several groupings),
then a model of the whole `dWorldQuickStep` step with these rules recomputed the shipped library's recorded
outputs (**reproduced**). The pile scenarios (contacts, heavy) are replayed one step at a time from the shipped
library's own recorded state after the previous step, the contacts taken from the pair records; which bodies a
pair record belongs to follows from the simple space's order (each geometry moved to the front of the space's
list when its body is stepped, so after the first step the list is the bodies in creation order, then the
plane) and the bounding boxes' overlap, checked against the number of pair records of every step. The first
step of each pile is recomputed from the inputs alone. The run time's `rand` starts at 1 and advances by the
shuffle's draws of every earlier step. The same model with our build's rule recomputes our build's own outputs,
which shows the model and the rule are discriminating.

Every rule of round 2 for the QuickStep stepper (masses, the world-frame matrices, the gyroscopic term, the
angular and linear updates, `dxStepBody`) holds and is used unchanged. Beyond them:

1. **The right-hand side's velocity term** (per body, `v / h + M⁻¹ f`): the linear part is the force times the
   inverse mass plus the velocity times 1/h (two products, one sum); the angular part is the world-frame inverse
   inertia times the accumulated torque, rows 0 / 1 / 2 summed source order / `(x + z) + y` / source order (the
   same orders as the final angular update), then plus the angular velocity times 1/h [S2, S1]. Our build sums
   every row in source order.
2. **J times the velocity term**: the 6 products of the first body added one after another in index order, the
   second body's 6 continuing the same chain — public 0.5.0's order, which our build follows [S2].
3. **The constraint rows' inverse-mass products (M⁻¹ Jᵀ)**: the linear part is J times the inverse mass; the
   angular part, for each body of the row, is the world-frame inverse inertia times the row's angular entries,
   every one of the three rows summed `(y + z) + x` [S2, S1]. Our build: source order.
4. **The per-row setup**: unchanged from public 0.5.0: `Ad = w / (Σ (M⁻¹ Jᵀ)·J + cfm)` (the 6 or 12 products
   added one after another, then cfm already multiplied by 1/h), J and b multiplied by Ad, then Ad by cfm; the
   warm start's `fc` as public [S1].
5. **The SOR row update**: `delta = (b − λ Ad) − S₁ − S₂`, where S₁ (and S₂ for a second body) is the body's
   6-term product of the row's scaled J with `fc`, added **from the last product to the first** (products 5 and 4
   first, then 3, 2, 1, 0); our build adds them first to last [S2, S1]. The clamping (`library` 4.3), the
   friction bounds recomputed at each visit, `fc += (M⁻¹ Jᵀ) × delta`, the order of rows and the shuffle at
   iterations 0, 4, 8, … over the whole row order with the run time's `rand` (`library` 4.3) are as our build
   already has them [S1].
6. **The constraint acceleration** is added as `velocity += fc × h` per component, after the non-finite guard
   (`library` 4.3); then the external force as round 1 and 2 say [S1].
7. **The contact joint's rows** (both kinds): as public 0.5.0, including the normal row's right-hand side
   `(1/h × erp) × max(0, depth − surface layer)` (1/h times erp first) and `dPlaneSpace`'s friction directions
   [S2, S1]. Changed: in the **bounce**, each of the four 3-term dot products of the outgoing velocity (each
   body's linear and angular Jacobian with its velocities) is summed `(y + z) + x`; their combination (each
   body's linear plus angular, then the two bodies) is as public [S2]. The bounce raised the normal row's
   right-hand side in 2,450 contacts of the piles; our build's grouping changes that value in 3 of them, without
   changing any recorded step (not distinguished by the data; established statically) [S1]. The special contact's
   normal bound of 100,000 is reproduced (heavy) [S1].
8. **The joint feedback** (the constraint force read back): `cforce × mass` for the linear part; the angular part
   is the world-frame inertia times the angular constraint acceleration, whose row orders depend on the body's
   position in the stepper's body list: while bodies are taken four at a time (the first `4 × ⌊n / 4⌋` bodies of
   a step with n ≥ 4 bodies), the first of each four uses source order in every row, the second source order /
   `(x + z) + y` / source order, the third and fourth `(y + z) + x` in every row; the remaining bodies (all of
   them when n < 4) use source order / `(x + z) + y` / source order [S2]. Our build: source order. The solve scenario's feedback (spheres at the identity, so every product sum has one non-zero term)
   is reproduced but does not distinguish the orders [S3]; the joints scenario, whose nine bodies use every
   case, reproduces every feedback with them [G5].
9. **The quaternion renormalisation in `dxStepBody`**: a squared length that is zero **or NaN** gives the
   identity (both sides, our build included: its own non-finite contacts run is reproduced so) [S1].
10. **`dMassSetCappedCylinderTotal`** (used by the piles): the rule of the capped-cylinder item (#886) is the one
    the piles need; with it, and only with it, the piles reproduce [S1].

Results [S1, S3]:

| Scenario | Steps | Shipped rule | Our build's rule on the shipped steps |
|---|---|---|---|
| contacts (steps 1–399, from the previous recorded state) | 399 (394 with contacts) | 399, every body | 8 |
| contacts, first step from the inputs | 1 | reproduced | — |
| heavy (steps 1–149) | 149 (140 with contacts) | 149, every body | 9 |
| heavy, first step from the inputs | 1 | reproduced | — |
| solve, QuickStep trials (round-3 run) | 200 | 200, bodies and feedback | 21 |

Our build's rule recomputes all of our build's own steps of the same scenarios (contacts 399, heavy 149,
solve 200) [S1, S3]. Which parts matter, the shipped rule with one part as our build has it [S1, S3]: the
capped-cylinder mass (contacts 71 of 399 steps, heavy 37 of 149), item 1 (382, 133), item 3 (22, 142), item 5
(8, 9; solve 21 of 200); items 7's bounce and 8 are not distinguished by these scenarios.

Observation (not a requirement): in our build's own contacts run a body's state becomes non-finite from step
112 on, reproduced by the model with our build's rule; the shipped library's run has no non-finite value [S1].

Unestablished:

- The rows of the other joint kinds are section 5's; the model takes them as a provider of rows.
- The non-finite guard's path (replacing a component and clearing the multipliers) is not exercised by any
  recorded step.

Checks: [S1] `qs_piles.py` (model `qs_model.py`, pile replay `qs_pile_model.py`; the first step uses the
rotation-to-quaternion conversion of `misc_quat.py`); [S2] `qs_static.py` (instruction-level reading with
`sym_model.py`); [S3] `qs_solve.py`.

## 4. The exact stepper's solve (`dWorldStep`; #888)

Scope: everything `dWorldStep` computes for bodies with contact and bilateral joints beyond round 2's angular update (2.4):
the contact joints' rows, J M⁻¹, A = J M⁻¹ Jᵀ with cfm / h on the diagonal, the right-hand side, the Dantzig
LCP with its factor updates and triangular solves, the constraint force Jᵀ λ with the joint feedback, the
non-finite guard and the velocity update. "M" is the world-frame inverse inertia of round 2, 2.4.

How it was established: the groupings were read from the shipped library's instructions (**static**; a
symbolic reading of its scalar single-precision arithmetic, which makes every grouping explicit) and compared
with public 0.5.0 and our build; then a model of the whole step, public 0.5.0's algorithm with the shipped
groupings, recomputes the shipped library's recorded outputs (**reproduced**). The same model with our build's
groupings recomputes our build's outputs, and each side's rule misses the other side's results, so the rule is
discriminating.

### 4.1 Rules established

1. **J M⁻¹, the angular part** (both bodies of a joint): entry k of a row's angular part a is
   `(a0 M0k + a2 M2k) + a1 M1k`, i.e. `(x + z) + y` for every k. Public 0.5.0 and our build: source order.
   **Static** [E6]; **reproduced** [E1, E5], and needed: without it 178 of the 300 exact-step trials [E4].
   The linear part is each entry times the body's inverse mass, as public.
2. **The velocity term of the right-hand side**: the angular half of `v / h + M⁻¹ f` uses M times the
   accumulated torque with every row summed `(x + z) + y`, then adds `avel × (1 / h)`. Public and our build:
   source order. **Static** [E6]; **reproduced** [E1, E5]; without it 298 of 300 [E4]. The linear half
   (`facc × invMass` plus `lvel × (1 / h)`) is as public.
3. **The LCP's step sizes.** The step that makes `w(i)` zero, and every step that makes a non-clamped `w(k)`
   zero (the index-switching case that moves k from N to C), are computed as `(−1 / Δw) × w`: the reciprocal
   of the change with the constant −1, one rounded division, then one rounded product. Public 0.5.0 and our
   build: `−w / Δw`. The steps to a bound (`(hi − x) / dir`, `(lo − x) / dir`, `(lo − x) / Δx`,
   `(hi − x) / Δx`) are divisions as public. **Static** [E6]; **reproduced** [E1, E5]; without it 77 of 300 [E4].
4. **The bounce's outgoing velocity** (contacts with `dContactBounce`): the first body's part is
   `(J1a · avel) + (J1l · lvel)`, each dot product summed last term first (`(z + y) + x`); with a second body,
   `((J2l · lvel₂) + (J2a · avel₂)) + first body's part`, each dot product again `(z + y) + x`. Public and our
   build: `(J1l · lvel + J1a · avel) + (J2l · lvel₂ + J2a · avel₂)` in source order. **Static** [E6]; not
   distinguished by the recorded data (every trial agrees with either order) [E4].

Agreeing with public 0.5.0 and our build (**static** where stated, and **reproduced** as a whole by [E1, E5]):

- the contact rows: normal row `J1l = n`, `J1a = c1 × n`, the second body's negated; the correcting velocity
  `(depth − layer, at least 0) × (fps × erp)` (static [E6]), capped at the maximum correcting velocity; soft CFM;
  bounds 0 to +∞, or to 100,000 for `dJointCreateContactSpecial`; the friction rows along `dPlaneSpace`'s two
  vectors (`dPlaneSpace` as public), bounds ±μ (μ2 for the second with `dContactMu2`), friction index to the
  normal row;
- A: every element a 6-term sum of products in index order, then added to what the element held; off-diagonal
  blocks per body in the body list's order and each body's joint list order; the diagonal plus `cfm × (1 / h)`;
  `rhs = c × (1 / h) − (J1 · t₁ + J2 · t₂)`, each 6-term sum in index order, the second body's sum added first;
- the LCP: public 0.5.0's Dantzig algorithm in its fast form (friction rows moved to the end, the six
  index-switching cases, friction bounds `|hi × λ(normal)|` set when the first friction row is reached); every
  dot product accumulated in index order from 0; the triangular solves in the generated form of blocks of four
  rows (the earlier blocks' terms accumulated in order, the block's own terms subtracted one by one), as our
  build's; the factor's row removal and its rank update as public; systems of up to 39 rows exercised, with
  every index switch, removals of the first, a middle and the last clamped row included;
- the unbounded rows (public 0.5.0's order of the joints: those whose rows are all unbounded first, then those
  with some, then those with none, each group in the world's list order; unbounded rows beyond them moved to
  the front): the leading block factorised by `dFactorLDLT` in its generated form (pairs of rows: each pair's
  part left of the diagonal solved against the factor so far in blocks of two, the terms accumulated in index
  order, then scaled by d and accumulated into the pair's diagonal block; `d = 1 / (diagonal − sum)`, the
  second row's `sum` starting from 0; an odd last row alone) and solved by `dSolveLDLT` (the triangular solve
  above, times d, the transposed solve); with every row unbounded the whole system factorised and solved in
  place. Odd leading blocks (joints) and a full system of six rows (held) exercised;
- the constraint force: each component of `Jᵀ λ` accumulated in row order from 0, written to the feedback and
  added to the body's total in the world's joint order; then plus the external force and torque, times h; the
  non-finite guard (`library` 4.2; not exercised); `lvel += invMass × value`; the angular update of round 2
  (2.4); `dxStepBody` as rounds 1 and 2, its quaternion renormalised with the reciprocal square root and the
  four products on the x87 (24-bit significand, unflushed), each product stored: the same rule as
  `dBodySetQuaternion`'s normalisation in 7.5; needed for one case of held-nodaz [E8].

### 4.2 Per scenario

- **joints** (steps 150 to 299, the exact stepper): every step **reproduced** from the shipped library's state
  after the previous one, every body and every joint's feedback, with section 5's rows [E9]; our
  build's rule reproduces all 150 of our build's and none of the shipped library's; without rule 1, 0 of 150;
  without rule 2, 121; without rule 3, 104 [E9].
- **solve** (round 3; the exact-stepper half, 200 trials): every trial **reproduced**; our build's rule
  reproduces all 200 of ours and 91 of the shipped library's [E7].
- **held, held-nodaz** (round 3; the exact-stepper half, 300 cases each): every case **reproduced**, held with
  flushing and held-nodaz without, the body before the step as recorded and the joint's stored rotation and
  offset as `dJointSetFixedQuaternionPos` sets them; with the other flushing setting 256 of 300 each; our
  build's rule (always flushing) reproduces every case of both of our build's runs [E8].

- **exact-step**: every one of the 300 single steps (both bodies and every contact's feedback) **reproduced**
  by the shipped rule; our build's rule reproduces all 300 of our build's and 56 of the shipped library's [E1–E3].
- **contacts-exact**: every one of the 199 steps after the first **reproduced** from the shipped library's own
  state before it (the pairs' bodies from the simple space's order, the masses regenerated with the capped
  cylinder's rule of [X1]); our build's rule reproduces 20 [E5].

### 4.3 Unestablished

- Rule 4 is established statically; no recorded case distinguishes it.
- The non-finite guard is not exercised by any scenario (its rule is `library` 4.2's).
- The unbounded-row path and `dFactorLDLT` are reproduced through the data only (their instructions were not
  read group by group); the factorisation of an even leading block smaller than the system is not exercised.

Checks: `ex_exact_step.py` [E1–E4], `ex_contacts_exact.py` [E5], `ex_static.py` [E6], `ex_solve.py` [E7],
`ex_held.py` [E8], `ex_joints.py` [E9] (with section 5's `jt_model.py`, `jt_joints_model.py`); helpers
`ex_model.py` (the step), `ex_sse_model.py` (the symbolic reading of the instructions).

## 5. The joints' rows, setters and getters (#888, #889)

Scope: every joint kind other than the contact that the joints and worlds scenarios use: ball, hinge (limits
with bounce; motor), slider (limits), universal, hinge-2 (suspension), fixed (`dJointSetFixed` and
`dJointSetFixedQuaternionPos`), angular motor (user mode, one axis); their rows (the Jacobian, the right-hand
side, CFM and bounds), the limit and motor row, the setters that store relative anchors, axes and relative
rotations, and the getters the joints scenario records. Terms are public 0.5.0's: `fps` = 1 / h, `erp` the
library-wide ERP, `k` the product of the two, the stored anchors `anchor1`, `anchor2`, the stored axes
`axis1`, `axis2`, the relative rotation `qrel`, the stored `offset`; a 3-term sum is written by its grouping
as in round 2. Every operation is single precision with flushing unless marked x87; "x87" means an
operation done at 24-bit precision without flushing, whose results here equal single-precision rounding
(no value of these computations reaches the denormal range).

How established: read from the shipped library's instructions (the dataflow of every stored value; Ghidra's
decompiled C does not show floating-point grouping reliably and was not used for it) [G1 – G3]; then
**reproduced**: the joints scenario's 150 QuickStep steps, each from the shipped library's recorded state
after the previous step, give every body and every joint feedback bit for bit, through section 3's stepper and the rows below, the run time's `rand` carried from step to step and the warm-start
multipliers from the model's own previous step [G5]; the same replay with our build's rules reproduces our
build's 150 steps, and our build's joint rows with the shipped stepper reproduce at most one shipped step.

### 5.1 Rules established

1. **Rotations of stored vectors** (static [G1]; reproduced [G5]). Every rotation of a stored anchor, axis or
   offset by a body's rotation R (R v), and every rotation back into a body's frame (Rᵀ v) in the setters,
   sums each row `(y + z) + x`: the products with the vector's components 1 and 2 first, then the product
   with component 0. This holds for the ball rows' anchors, the hinge's two axes, the slider's axis and
   offset, the universal and hinge-2 axes, the fixed joint's offset, the fixed orientation rows' error
   vector, the anchor setters (Rᵀ (anchor − body position)), the axis setters, the slider's and the fixed
   joint's stored offsets, and `dJointGetHingeAngleRate`'s and `dJointGetSliderPosition`'s axes. Ours: source
   order. Replacing this rule alone by ours reproduces 2 of the 150 steps.
2. **Ball rows** (the ball, hinge and universal joints' ball rows) (static [G1]; reproduced [G5]). Jacobian as public 0.5.0
   (the cross matrices' negated entries are `−0 − a`, equal to `−a`); right-hand side
   `k × (((pos2 + a2) − a1) − pos1)` with two bodies and `k × ((anchor2 − a1) − pos1)` with one, a1 and a2 the
   rotated anchors (rule 1); public's grouping.
3. **Hinge rows 3 and 4** (static; reproduced [G5]): p and q from `dPlaneSpace` of the rotated axis 1
   (`dPlaneSpace` as public 0.5.0 on both sides), b = ax1 × ax2, the right-hand sides `k × (b · p)` and
   `k × (b · q)`, each dot summed `(y + z) + x`. Ours: source order; replacing this alone: 4 of 150 steps.
4. **Slider rows 3 and 4** (two bodies) (static [G2]; reproduced [G5]): the two angular parts of body 2 are
   `(c × p) × 0.5` and `(c × q) × 0.5`, c = pos2 − pos1; the right-hand sides `k × (p · c')` and `k × (q · c')`
   with c' = c + R2 offset, each dot summed **`(x + z) + y`** (components 0 and 2 first, then 1). Ours: source
   order; replacing this alone: 10 of 150 steps. The rows with one body were not read (**unestablished**).
5. **Fixed orientation rows** (slider rows 0 to 2, fixed rows 3 to 5) (static; reproduced [G5]): the
   relative rotation of the two bodies and the error quaternion are public 0.5.0's quaternion products with
   every component summed in public's order (the one-body product's terms are grouped differently but give
   the same values); the error's vector part negated when its scalar part is negative; e = R1 (error's vector
   part) (rule 1); the right-hand sides `(e × k) × 2` (public: `(2 k) × e`; the two differ only where a
   product crosses the edge of the normal range, which the data does not exercise).
6. **Universal row 3** (static; reproduced [G5]): d = ax1 · ax2 summed `(y + z) + x`; p = ax1 × (ax2 − d ax1);
   p normalised with its squared length summed `(y + z) + x` (the threshold rule of `library` 3.5; the
   fast path's reciprocal square root and products on the x87); right-hand side **`−((erp × d) × fps)`**
   (public: `(fps × erp) × (−d)`). Replacing each alone by ours: the dot 2, the length 3, the right-hand side
   16 of 150 steps.
7. **Hinge-2 rows** (static; reproduced [G5]): q = ax1 × ax2; its squared length L summed `(y + z) + x` serves
   both the sine `s = √L` (x87, stored to single precision) and q's normalisation (threshold rule, fast path on
   the x87); the cosine c = ax1 · ax2 summed **`(x + z) + y`**; row 3's right-hand side
   `k × ((c0 × s) − (s0 × c))`; the three suspension-aligned ball rows with the axis ax1 and the
   suspension ERP: Jacobian as public 0.5.0, right-hand sides `(fps × erp1) × (axis · a2 − axis · a1)` and
   `k × (qᵢ · a2 − qᵢ · a1)` for the two plane-space vectors, where the dots with a2 and the axis's dot with a1
   are summed in source order and **the plane-space vectors' dots with a1 `(x + z) + y`** (a1, a2 the rotated
   anchors plus the body positions); `cfm[0]` the suspension CFM. Replacing the length and cosine rule alone:
   1 of 150 steps; the suspension-aligned rows' rule alone: 3 of 150.
8. **Fixed rows 0 to 2** (static [G1]; reproduced [G5]): ofs = R1 offset (rule 1); with two bodies the cross
   matrix of ofs and the right-hand side `k × ((pos2 − pos1) + ofs)`; with one, `k × (offset − pos1)`.
9. **The limit and motor row** (static [G1]; reproduced [G5] for the hinge limit with bounce, the hinge motor,
   the slider limits and the angular motor): as public 0.5.0, with `−((stop_erp × fps) × limit_err)` for the
   limit's right-hand side (equal to public's), and the bounce's joint velocity dots summed `(y + z) + x`
   (static only: replacing them by ours changes none of the 150 steps).
10. **Angular motor, user mode, axis relative to body 1**: no arithmetic on the axis (0.5.0's behaviour,
    `library` 5.1); the row is the motor row of rule 9 [G5].
11. **Limit tests** (static; reproduced [G5]): the hinge's is public 0.5.0's, its angle as rule 12; the
    slider's limit state is cleared first, its position as rule 14. Replacing the hinge angle's arithmetic in
    the test alone: 74 of 150 steps; the slider position's: 21.

### 5.2 Getters and stored values

12. **`dJointGetHingeAngle`** (two bodies) (static; reproduced [G4], every one of the 300 steps, both hinges):
    the relative rotation of the two bodies (public's product, its scalar part in index order) times the
    stored relative rotation conjugated, the **scalar part summing the products of components 0, 3, 1, 2 in
    that order** (the vector parts in public's order); the sine of the half angle `√((q3² + q2²) + q1²)` on the
    x87; the sign test's dot of the vector part with the axis summed `(y + z) + x`; the angle twice the x87
    arc tangent (extended precision, then rounded), reduced by 2π (single precision) when above π, then
    negated as `−0 − θ`. Our rule reproduces 278 and 297 of the 300 shipped values.
13. **`dJointGetHingeAngleRate`** (static [G1]; reproduced [G4]): the rotated axis (rule 1) dotted with each
    body's angular velocity, both dots `(y + z) + x`, body 2's subtracted. Our rule: 202 of 300.
14. **`dJointGetSliderPosition`** (two bodies) (static; reproduced [G4]): c = (pos1 − R2 offset) − pos2 (rule 1
    for the rotations); the result `(c2 ax2 + c1 ax1) + c0 ax0` on the x87 (`(y + z) + x`). Our rule: 240 of 300.
15. **Relative rotations stored by the setters** (static [G3]): `dJointSetHingeAxis` and `dJointSetFixed` sum
    the scalar part of the bodies' relative rotation as the products of components **3, 0, 2, 1** in that
    order, `dJointSetSliderAxis` as **0, 3, 2, 1**; the vector parts in public's order; the stored offsets
    Rᵀ (pos1 − pos2) `(y + z) + x`. The axis setters' squared length is summed in index order (then the
    threshold rule, the fast path on the x87) [G1]. At the identity rotations of the joints scenario's setup
    every one of these is exact, so the data does not test them.

### 5.3 #889: where flushing first happens in the held body's step

**Reproduced** [G6]: the replay of [G5] reproduces the shipped library's joints run with flushing and its
joints-nodaz run without, each for all 150 QuickStep steps, and neither with the other setting. The runs are
identical up to the 28th step and first differ after the 29th. In the 29th step without flushing, the first
denormal value is a **product in the rotation of the error quaternion's vector part by the held body's
rotation (rule 5's e = R1 qerr), in the orientation rows of the fixed joint that holds the body**; the next is
the right-hand side product `e × k` of the same rows. Flushing that rotation's products alone, or that
right-hand side product alone, reproduces the flushed run's step; flushing any other place alone (the
solver's products included) does not. So the difference enters through the held joint's orientation rows'
right-hand side, hence the multipliers and the held body's angular velocity first; it is not the angular
update's products nor the solver's. The round-3 scenario `held` (one body held by a fixed joint to the world, stepped once with and
without flushing) agrees: each of its runs is reproduced with its own setting, by both steppers, and not with
the other (6) [E8, X14].

### 5.4 Unestablished

- The one-body slider rows; the universal joint's angles and relative rotations (its limits), the hinge-2
  angle and its v1, v2 vectors (its limits), the hinge-2 axis setters' stored sine and cosine, and the angular
  motor's Euler mode: not read here (**unestablished**). The scenario exercises none of them.
- The slow paths of the normalisations (below the threshold): `dNormalize3_slow`, as `library` 3.5.

## 6. The other stepped scenarios: worlds and held

- **worlds**: every one of the 40 QuickStep steps of the bodies in the world (two bodies and one added, a ball
  joint between two of them for the first 20 steps, a disabled body stepped as `library` 4.1 says) is
  **reproduced** from the recorded state after the previous step, with section 3's stepper and the ball rows of
  5.1; the library-wide ERP and CFM are `dWorldCreate`'s (the scenario's second world resets them, `library`
  3.1); our build's rules reproduce all 40 of ours [X13]. The body outside every world (`dxStepBody` alone)
  was already identical.
- **held, held-nodaz** (round 3, #889): one body held by a fixed joint to the world at a pose with tiny
  quaternion components, stepped once. Every QuickStep case (300 of each run) is **reproduced** with section 3's
  stepper and 5.1's fixed rows, the run time's `rand` carried from case to case (its state is per thread, not
  per world), held with flushing and held-nodaz without; with the other setting 250 of 300 [X14]. The exact
  stepper's cases likewise (300 of each) [E8]. Our build's rules (always flushing) reproduce every case of both
  of our build's runs. One held-nodaz case of each stepper needs `dxStepBody`'s renormalisation on the x87 (7.1,
  step 5).

## 7. Bodies, masses and rotations (#885, #886, #890 – #893)

How established: each function's arithmetic was read from the shipped library's instructions (a symbolic
reading that gives every stored value's expression with its grouping; where the code loops, the loop counts
followed from the sizes), and the stated rule is checked to give the same expressions (**static**); then the
round-3 run's scenario for the item is recomputed bit for bit from its recorded inputs (**reproduced**), with
the other side's rule shown to miss. "x87" marks an operation done by the x87 unit at 24-bit precision: it
rounds the significand as single precision does but keeps a wider exponent, so it is **not flushed** and a
result below the normal range is rounded twice (to 24 bits, then to the denormal grid when stored).

### 7.1 Finite rotation (#885)

**About a set axis** (static [X2]; reproduced: every step of the free scenario's body with an axis, 199 of 199,
and every case of the round-3 `finite` scenario, 960 of 960 [X2]):

1. `dBodySetFiniteRotationAxis` stores the axis normalised by `library` 3.5's rule (squared length in index
   order; at or above the threshold each component times the x87 reciprocal square root, stored).
2. In `dxStepBody`: hh = h × 0.5; k = axis · w summed **`(y + z) + x`**; the finite part `frv = axis × k`, the rest
   `irv = w − frv`; θ = k × hh (single precision); the cosine of θ (x87, stored);
   `s₀ = sin θ / θ` with the sine **unrounded** and the quotient rounded once (as round 2's 2.6), or
   `1 − (θ × θ) × (1/6)` when |θ| < 10⁻⁴ (compared in double precision); s = s₀ × hh; the rotation
   quaternion (cos θ, s × frv).
3. Its product with the body's quaternion: component 0 in source order, components 1 to 3 first, third,
   second, then the fourth term (round 2's 2.6).
4. The infinitesimal update with irv and the new quaternion, as round 1's 4.2 (its third component's order),
   each derivative times 0.5 then times hh, added.
5. The renormalisation: the squared length in index order (single precision), then the reciprocal of its
   square root and the four products **on the x87** (unflushed), stored.

Our build (public 0.5.0 there) differs in the dot's order, the rounded sine and the product's order: its rule
gets 120 of the free scenario's 199 shipped steps and 824 of the 960 `finite` cases.

**Without an axis** (static [X3]; reproduced [X3, X2]): round 2's 2.6 holds, with these details read from the
instructions: the angle is computed on the x87, `θ = (h × 0.5) × √L` with the squared length L summed
**`(x + z) + y`** (components 0 and 2 first) on the x87; at |θ| < 10⁻⁴ the series is `1 − (θ × θ) × (1/6)` on θ
stored to single precision (as our build); the renormalisation as item 5. The length's order is not distinguished
by any recorded case (the free scenario's 199 steps and the `finite` scenario's small and near-threshold angles
agree with index order too), but the two orders differ in about one sum in four for velocities like the
scenarios' [X3].

### 7.2 `dMassSetCappedCylinderTotal` (#886)

Static [X1]; reproduced: all 3,000 cases of `mass-ccylinder` (ours: 1,291) [X1]; the pile scenarios need it
(QuickStep and exact sections). The total form calls the density form with density 1, then scales:

- P = (density × r) × r (r the radius, l the length);
- cap mass `M2 = (P × r) × C43`, C43 = 4.1887903 (4π/3 in single precision, the single-precision product of 4/3
  and π); cylinder mass `M1 = (P × l) × π` (π in single precision); mass = M2 + M1;
- the two inertia values: across `M2 × (((0.4 × r + 0.375 × l) × r) + (l × l) × 0.25) + M1 × ((r × r) × 0.25 +
  (l × l) × K)`, K = 1/12 in single precision; along the axis `((M2 × 0.4 + M1 × 0.5) × r) × r`; the three
  diagonal entries are the across value, then the direction's entry the along value; every other entry 0;
- the total form: `scale = total / mass` (one division), the mass set to the total, the nine inertia entries
  times scale.

Public 0.5.0 and our build multiply the constants first (`((π × r) × r) × l`, `(((4/3 π) × r) × r) × r`), and
group the brackets as `(0.4 × r) × r + (0.375 × r) × l + (0.25 × l) × l` and `(0.25 × r) × r + (K × l) × l`.

### 7.3 The mass functions the game uses (#893's inputs)

Static [X10, X11]; reproduced through the round-3 `angular-mass` scenario's recorded masses, 1,600 of 1,600
(ours: 707) [X12].

- `dMassRotate(m, R)`: the new inertia is `R (I Rᵀ)`; the body-frame product's entry (0, 0) is summed in
  source order and its other eight entries `(y + z) + x`; each entry of R times it is summed `(y + z) + x`; the
  upper entries are computed and copied to the lower ones; the centre is rotated row by row, each
  `(y + z) + x`. Public and our build: source order.
- `dMassTranslate`, `dMassAdd`: as public 0.5.0 (the shipped arithmetic, evaluated from its reading, equals
  public's on 3,000 synthetic cases each) [X11].
- The density forms: `dMassSetSphere` mass `(((density × r) × r) × r) × C43`, inertia `((mass × r) × r) × 0.4`;
  `dMassSetBox` mass `((density × lx) × ly) × lz`, inertia as round 2's total form; `dMassSetCylinder` mass
  `(((density × r) × r) × l) × π`, across `((l × l) × K + (r × r) × 0.25) × mass`, along `((r × r) × mass) × 0.5`;
  `dMassSetCylinderTotal` the same with the total; `dMassAdjust` `scale = total / mass`, the nine entries
  times it [X11]. Public and our build compute the density forms through the total forms with the constants
  first (`(4/3 π) r³ density`, `lx ly lz density`, `π r² l density`) and the cylinder's inertia as
  `total × ((0.25 × r²) + (K × l) × l)` and `(total × 0.5) × r²`.
  **Disputed in review:** the reproduction [X12] does not back this item: no scenario calls `dMassSetSphere`,
  `dMassSetBox`, `dMassSetCylinder`, `dMassSetCylinderTotal` or `dMassAdjust` (the harness uses only
  `dMassSetSphereTotal`, `dMassSetBoxTotal` and `dMassSetCappedCylinderTotal`); these rules are established
  statically only [X11]. The 1,600 reproduced masses back `dMassRotate`, `dMassTranslate` and `dMassAdd`.

### 7.4 The angular update with a non-diagonal inertia (#893)

Static [X8, X9]; reproduced: every one of the 1,600 steps of `angular-mass`, both steppers, every mass kind
(rotated, translated, both, a second mass added), the angular velocity after the step [X12].

- `dBodySetMass`'s inverse inertia (`dInvertPDMatrix`): public 0.5.0's Cholesky factorisation and solves
  (square roots and their reciprocals on the x87, which in this range round as single precision); equal to
  public's for 3,000 synthetic rotated inertias [X9].
- The steppers' world-frame matrices (round 2's 2.4, step 2) for a general body-frame matrix D: first the
  body-frame product `T[k][j] = Σₗ D[k][l] R[j][l]`, entry (0, 0) in source order, **every other entry
  `(y + z) + x`**, the same in both steppers and for both the inertia and its inverse; then row i of R times T
  with round 2's row orders [X8]. With a diagonal D each of these sums has one non-zero term, which is why round
  2 did not see them. Our build: source order.

### 7.5 The rotation-to-quaternion conversion (#891) and `dBodySetRotation`'s normalisation (#890)

- One conversion routine serves `dBodySetRotation`, `dGeomGetQuaternion` of a geometry without a body, the
  exported `dQfromR` and four joint setters (static [X4]). Trace not negative: the trace `R00 + (R11 + R22)`
  (round 2's 2.3), `s = √(trace + 1)` with the addition and the square root on the x87, stored; q0 = s × 0.5 on
  the x87; the other three the antisymmetric differences times `(1 / s) × 0.5`. Negative trace: the largest
  diagonal entry chosen as public 0.5.0 (ties as public); `s = √((Rii − (Rjj + Rkk)) + 1)` on the x87, with
  `(Rjj + Rkk)` the single-precision sum `R11 + R22` when i = 0 and an x87 sum otherwise; the components as
  public, times `(1 / s) × 0.5`. Reproduced: the rotation scenario's body quaternions of both trace signs
  (2,007 with a trace not negative, 340, 316 and 337 with the largest diagonal entry 0, 1, 2) and the round-3
  `quaternion` scenario's `dQfromR` and `dGeomGetQuaternion`, 3,000 each (ours 2,852) [X4]. Our build uses round
  2's trace only in `dBodySetRotation`.
- `dQFromAxisAndAngle`: round 2's 2.2 quaternion (the cosine of half the angle; each axis component times the
  unrounded sine times the reciprocal square root of the index-order squared length, rounded once);
  `dRFromAxisAndAngle` is this followed by the quaternion-to-rotation conversion (static [X5]); reproduced:
  3,000 of 3,000 (ours 2,282) [X5].
- `dNormalize4` (`dBodySetRotation` after the conversion, `dBodySetQuaternion`): the squared length in index
  order (single precision, flushed), the reciprocal square root and the four products **on the x87**, so
  **not flushed in any environment**, a result below the normal range rounded twice (static [X6]).
  Reproduced: every set rotation of the denormal scenarios (800 each), and the 3 cases of the unflushed run that
  only this rule gets; the round-3 `set-rotation` scenario (flushed, components within one rounding of the
  smallest normal value) is reproduced 3,000 of 3,000 and is identical on both sides: its products never fall
  below the normal range after flushed inputs [X6]. So round 2's review correction 1 is settled: the
  normalisation is unflushed in the game's environment too, and that shows only where a product of a normal
  component and the reciprocal falls below the normal range.

### 7.6 The force and torque functions (#892)

Static [X7]; reproduced: every call of the round-3 `rel-force` scenario, 1,000 per function [X7]. Every vector
given in the body's frame (a force, a torque, a point) is rotated by the body's rotation with each row summed
**`(y + z) + x`**, in `dBodyAddRelForce`, `dBodyAddRelTorque`, `dBodyAddForceAtRelPos`, `dBodyAddRelForceAtPos`
and `dBodyAddRelForceAtRelPos`; a world point is taken relative to the body's position (one subtraction per
component); the torque is the point crossed with the (rotated) force, public's cross product, added to the
accumulator. `dBodyAddForce`, `dBodyAddTorque` and `dBodyAddForceAtPos` have no sum to order. Our build: the
force's and torque's rotations in source order (the point's of `dBodyAddRelForceAtRelPos` already follows round
2): our rule gets 315 to 438 of 1,000 per function with a rotation.

Unestablished:

- `dBodySetFiniteRotationAxis` with an axis whose squared length is below the threshold (`dNormalize3_slow`):
  not exercised.
- The angular update with a non-diagonal inertia and flushing: not exercised (no denormal values).

## 8. What our implementation must do to be bit-exact

Beyond rounds 1 and 2 (which this round confirms wherever our build follows them), our build (public 0.5.0
with `third_party/ode`) must change the following; everything not listed is as our build already computes it.
Each item names its section.

**Colliders (2)**

1. Sphere–box: the box-frame coordinates, the clamped point's rotation and the inside normal with
   `(y + z) + x` rows; the depth's squares `(y + z) + x` (2.1).
2. Capped cylinder–box: the segment's start and direction in the box's frame, components 1 and 2
   `(y + z) + x`; the derivative's terms summed from the first (no initial zero); the box point's rows 1 and 2
   `(y + z) + x`, the centre added last (2.2).
3. The two segments' closest points: every dot product `(y + z) + x`; two capped cylinders' `axis1 · axis2`
   `(y + z) + x`; in the near-parallel case the offset's projection and the second and middle contacts' squared
   distances `(y + z) + x` (2.3).
4. Box–box: `pp`, the `Rij` and the box-2 axis tests as 2.4; the fifteen separation tests' sums as its table;
   the edge tests multiplied by one rounded reciprocal of the length; the edge normal's rows; the edge–edge signs
   `(x + z) + y`; every dot of the added contacts `(y + z) + x`; **box 2's centre as the face centre of the added
   contacts' candidates 5 and 6** (2.4); the face case's frame normal, face coordinates, cosines, the two corner
   sums, the clipped point's order and its depth's `(x + z) + y` (2.4).
5. Box–plane: the products with the box's second and third axes and normal · centre `(y + z) + x`;
   sphere–plane, capped cylinder–plane and capped cylinder–sphere: their dot products `(y + z) + x` (2.5).
6. Ray–box: the frame components `(y + z) + x`, the slab parameters with one rounded reciprocal per axis;
   ray–capped cylinder: the six sums of 2.6 `(y + z) + x`.

**QuickStep (3)**

7. The right-hand side's angular velocity term: M⁻¹ times the torque, rows source order / `(x + z) + y` /
   source order.
8. M⁻¹ Jᵀ: every angular row `(y + z) + x`, both bodies.
9. The SOR row update: each body's 6-term product with `fc` added from the last product to the first.
10. The contact bounce's four dot products `(y + z) + x`.
11. The joint feedback's angular product: the row orders by the body's position in the stepper's list (3,
    item 8).

**The exact stepper (4)**

12. J M⁻¹'s angular entries `(x + z) + y`.
13. The right-hand side's M⁻¹ times the torque, every row `(x + z) + y`.
14. The LCP's two step sizes that zero a `w`: `(−1 / Δw) × w`.
15. The bounce's outgoing velocity as 4, rule 4.

**Joints (5)**

17. Every rotation of a stored anchor, axis or offset (R v, Rᵀ v) in the rows, setters and getters
    `(y + z) + x`.
18. The hinge's `b · p`, `b · q`, the limit row's bounce dots and the hinge angle rate's dots `(y + z) + x`.
19. The slider's right-hand side dots `(x + z) + y`.
20. The universal joint: the axes' dot and p's squared length `(y + z) + x`; the right-hand side
    `−((erp × d) × fps)`.
21. Hinge-2: one squared length `(y + z) + x` for both the sine and the normalisation; the cosine
    `(x + z) + y`; the suspension-aligned ball rows' plane-space dots with a1 `(x + z) + y`.
22. The fixed orientation rows' right-hand side `(e × k) × 2`.
23. `dJointGetHingeAngle`: the scalar part's products in the order 0, 3, 1, 2; the half-angle sine's squares
    `(q3² + q2²) + q1²` on the x87; the sign dot `(y + z) + x`; the reduction and negation of 5.
24. `dJointGetSliderPosition`: the final dot `(y + z) + x` on the x87.
25. The setters' relative rotations' scalar parts in the orders of 5; their stored offsets `(y + z) + x`.

**Bodies, masses and rotations (7)**

26. `dxStepBody` about a set axis: the dot `(y + z) + x`, the unrounded sine, the product's component orders;
    without an axis the squared length `(x + z) + y`; the renormalisation's reciprocal square root and products
    on the x87 (unflushed) (7.1).
27. `dMassSetCappedCylinder` and `dMassSetCappedCylinderTotal` as 7.2.
28. `dMassRotate`, `dMassSetSphere`, `dMassSetBox`, `dMassSetCylinder` and `dMassSetCylinderTotal` as 7.3.
29. Both steppers' body-frame products for the world-frame inertia and inverse inertia: entry (0, 0) source
    order, every other `(y + z) + x` (7.4).
30. The trace `R00 + (R11 + R22)` in every use of the rotation-to-quaternion conversion; `dQFromAxisAndAngle`
    as round 2's 2.2 (7.5).
31. `dNormalize4`'s reciprocal square root and products on the x87: unflushed, a result below the normal range
    rounded twice (7.5).
32. Every body-frame rotation in the force and torque functions `(y + z) + x` (7.6).

Flushing (round 2, 2.9, and #889, 5.3): flushing every single-precision operation, as our build does, is what
the shipped library does wherever its arithmetic is SSE; the x87 operations named in this document are the
exceptions and are never flushed.

## 9. Corrections to earlier specifications

- `library` 6.2 item 2: candidates 5 and 6 of the box–box added contacts take the face centre from box 2's
  centre, not box 1's (2.4).
- `library` 7.1: besides square roots, sines, cosines and arc tangents, the x87 also computes the reciprocal
  square roots and the products of the 4-vector normalisations (`dNormalize4`, `dxStepBody`'s renormalisation),
  the fast path of `library` 3.5's normalisation, the box–box separation tests, some collider distances and
  the getters of 5; these are not flushed (x87, above).
- Round 2's 2.6: without an axis the angular speed's squared length is summed `(x + z) + y` on the x87, not in
  index order (no recorded case tells them apart) (7.1).
- Round 2's 2.3 and its review's correction 1: the normalisation of `dBodySetRotation`'s quaternion is unflushed
  in the game's environment too (7.5).
- Round 2's 2.9 (and its review's correction 2): in the joints scenario the first flushed operation is in the
  held joint's orientation rows (5.3), not the angular update or the solver.

## 10. Unestablished

- Box–box contact culling when more than one but fewer than the found contacts are asked for (2).
- The near-parallel capped cylinders: static only (2.3).
- The non-finite guards of both steppers: not exercised (3, 4).
- The rows not exercised: the slider with one body, the universal joint's angles, the hinge-2 angle and its
  limit vectors, the hinge-2 axis setters' stored sine and cosine, the angular motor's Euler mode (5).
- `dBodySetFiniteRotationAxis` below the normalisation threshold (7.1).
- The pile scenarios' first steps are recomputed only for QuickStep (3); their colliders' first step is not
  (the poses before it are not recorded, 2).
- Statically established but not distinguished by any recorded case (they are requirements all the same): the
  bounce dots (3, 4), the box–box axis choice M and S, the inside test of ray–capped cylinder (2), the length
  order without an axis (7.1), the fixed rows' `(e × k) × 2` and the limit row's bounce dots (5).

## 11. Checks

All in `<workspace>/ph3/checks/`, run together by `python3 -I <workspace>/ph3/checks/run_all.py` (exit 0 when
every check passes; the pile replays take a few minutes). Each prints counts only. Shared helpers: `lib.py`
(records, single-precision operations with optional flushing, the scenarios' input generator, round 2's rotation
rule), `sym_model.py`, `emu_model.py`, `coll_sym_model.py`, `ex_sse_model.py`, `jt_sym_model.py` (symbolic
readings of the shipped library's instructions), `expr_model.py` (one rule as an expression tree or as values),
and the models `coll_model.py`, `qs_model.py`, `qs_pile_model.py`, `ex_model.py`, `jt_model.py`,
`jt_joints_model.py`, `misc_rules_model.py`.

- Colliders: [K1 – K5] `coll_pairs.py`; [K6, K7] `coll_rays.py`; [K8, K1] `coll_piles.py`; [K9 – K12]
  `coll_static.py`; [K13] `coll_posed.py`.
- QuickStep: [S1] `qs_piles.py`; [S2] `qs_static.py`; [S3] `qs_solve.py`.
- Exact stepper: [E1 – E4] `ex_exact_step.py`; [E5] `ex_contacts_exact.py`; [E6] `ex_static.py`; [E7]
  `ex_solve.py`; [E8] `ex_held.py`; [E9] `ex_joints.py`.
- Joints: [G1 – G3] `jt_static.py`; [G4] `jt_getters.py`; [G5] `jt_joints.py`; [G6] `jt_flush.py`.
- Bodies, masses, rotations: [X1] `misc_mass.py`, `misc_round3.py`; [X2, X3] `misc_finite.py`,
  `misc_round3.py`; [X4 – X6] `misc_quat.py`, `misc_round3.py`; [X7] `misc_force.py`, `misc_round3.py`;
  [X8, X9] `misc_inertia.py`; [X10, X11] `misc_masstx.py`; [X12] `misc_angmass.py`; [X13] `misc_worlds.py`;
  [X14] `misc_held.py`.

The x87 sine and cosine are taken as the double-precision functions (unrounded where the shipped library keeps
the x87 value, rounded where it stores it), as in round 2; every check reproducing them passes on every case.

## 12. Provenance

- Date: 2026-11-27. S.T.A.L.K.E.R.: Shadow of Chernobyl (GOG), its `bin\ode.dll` (196,608 bytes).
- Static analysis by a dirty investigator: the library's instructions (an objdump listing) read symbolically,
  straight-line code and loops with their counts, the x87 and SSE operations kept apart; Ghidra decompilation
  only to find functions; constants read from the binary. Compared with public ODE 0.5.0 (tag `0.5.0`) and our
  build's patched source (`third_party/ode`).
- Black-box results: round 2's run (release `odeharness-56caa53187a3`) and round 3's (the ten new scenarios),
  run on the Windows PC as `docs/testing.md` says, the shipped library loaded unmodified and driven through its
  public API with the harness's synthetic inputs; our outputs from our build's runner. Inputs not recorded were
  regenerated with the scenarios' own generator and checked against the recorded ones.
- No execution or debugging of the shipped library; no leaked or derived source; no recollection of either
  program's source used as a fact.

## Gaps found in review

1. **Section 8, item 2:** it omits 2.2's initial derivative of the squared distance, the three products
   subtracted from −0 (public 0.5.0 starts from +0); the two differ only in a zero's sign.
2. **Sections 5.2 and 8, items 17 and 25:** the setters' arithmetic (the anchor and axis setters' rotations into
   the body frame, the relative rotations' scalar-part orders, the stored offsets) is established statically
   only: the joints scenario sets every joint up at identity rotations, so no recorded value tests it; section 10
   does not list it.
3. **Sections 7.2, 7.3 and 8, items 27 and 28:** the density mass forms (`dMassSetCappedCylinder` with a density
   other than 1, `dMassSetSphere`, `dMassSetBox`, `dMassSetCylinder`, `dMassSetCylinderTotal`) and `dMassAdjust`
   are called by no scenario; section 10 does not list them.
4. **Sections 7.5 and 8, item 31:** in the game's environment (flushing) the unflushed normalisation is not told
   apart by any recorded case (the `set-rotation` scenario is identical on both sides); only the run without
   flushing distinguishes it. Section 10 does not list it.
