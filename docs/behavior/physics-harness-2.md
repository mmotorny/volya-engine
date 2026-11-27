# The physics library against the shipped one, round 2: what differs and why

Behavioural findings from running the black-box physics harness (`tests/odeharness/`, release
`odeharness-56caa53187a3`) on the shipped `bin\ode.dll` and comparing the results with our build's outputs
that ship with the runner. It extends `docs/behavior/physics-harness.md` (cited as `round 1`) and
`docs/behavior/physics-library.md` (cited as `library`), with their conventions: public ODE 0.5.0 API names,
single precision, "h" the step length. Scenario names are the harness's (`scenarios.c`); the new scenarios
(angular, rotation, exact-step, normalise-ties, denormal, denormal-nodaz) target round 1's section 8.

How the findings were established: every statement comes from the shipped library's own API outputs for the
harness's synthetic inputs. Inputs the harness does not record (masses' totals and sides, axes and angles of
rotations given to geometries and bodies, forces) were regenerated from the scenarios' own input generator
and checked against the inputs the harness does record. **Reproduced** means a check recomputes, bit for bit,
the shipped library's output for every case of that kind in the scenario, from the recorded or regenerated
inputs (or the shipped library's own state after the previous step), with the rule stated; the same check
recomputes our build's outputs with our build's rule, and reports how many cases the other side's rule gets
right, which shows the rule is discriminating. Bracketed ids are check scripts (section 7).

Conventions for sums: a 3-term sum of terms indexed x, y, z (or 0, 1, 2) is written by its grouping: "source
order" is `(x + y) + z`; "`(y + z) + x`" adds the last two first, then the first; "`(x + z) + y`" adds the
first and last, then the middle one. Each operation is rounded to single precision unless a rule says
otherwise. "Flushing" means a denormal result of a single-precision operation becomes a zero of the same sign
and a denormal operand is read as zero (the game's environment, `library` 7.2).

## 1. Summary

| Scenario | Result | Cause (section) |
|---|---|---|
| library | identical | round 1's load-time fix holds |
| x87 | identical | - |
| normalise, normalise-ties | identical | the tie rule agrees (2.8) |
| free | differs | mass (2.1); rotation from axis and angle (2.2); set rotation (2.3); angular update (2.4); torque of a relative force (2.5); finite rotation (2.6) |
| angular | differs | mass (2.1); set rotation (2.3); angular update (2.4) |
| rotation | differs | rotation from axis and angle (2.2); set rotation (2.3) |
| denormal, denormal-nodaz | differ | mass (2.1); set rotation (2.3); angular update (2.4); flushing (2.9) |
| worlds | differs | mass (2.1), and for the jointed bodies the solver (open, 4) |
| joints, joints-nodaz | differ from the first step | as free, plus the solver (open, 4); denormals (2.9) |
| contacts, contacts-nodaz, heavy, contacts-exact | differ | mass (2.1), rotations (2.2, 2.3), angular update (2.4), colliders (2.7) and the solvers (open, 4) |
| exact-step | differs | mass (2.1) and rotations change the inputs of almost every trial; the exact stepper's solve (open, 4) |
| pairs | differs, including contact counts | rotations (2.2) and the colliders' own arithmetic (2.7; open beyond the box–plane deepest contact) |
| rays | differs for box and capped-cylinder targets only | rotations (2.2) and the ray colliders' own arithmetic (open, 4) |

Round 1's open angular-update question is settled for both steppers and for **our build too** [A3]: round 1's
dispute (our own angular update not shown reproduced) is resolved, and most of what remained of round 1's
angular differences comes from the mass functions (2.1), which round 1 had not examined.

## 2. Rules established

### 2.1 Mass functions (angular; used by every stepped scenario)

The diagonal inertia set by `dMassSetSphereTotal` and `dMassSetBoxTotal` differs; the total, the centre and
every other entry of the `dMass` agree [M1]. **Reproduced** for every mass of the angular scenario:

- Sphere (total t, radius r): shipped `((t × r) × r) × 0.4`, the constant 0.4 rounded to single precision.
  Ours computes `((0.4 × t) × r) × r`.
- Box (total t; for each axis, p and q the other two sides): with `S = p² + q²` (two rounded squares, one
  rounded sum), shipped `(S × t) × K`, K being 1/12 rounded to single precision. Ours computes `(t / 12) × S`.

Required: exactly these orders. `dMassSetCappedCylinderTotal` (used by the pile scenarios) is not recorded by
any scenario (**unestablished**).

### 2.2 `dRFromAxisAndAngle` (rotation)

**Reproduced** for every case of the rotation scenario, the ten special axes included [R1]. Both sides: the
axis's squared length summed in index order, r = 1 / sqrt(length) (rounded square root, rounded division),
half the angle, the quaternion (cos(half), axis × k), then the quaternion-to-rotation conversion below
(2.3). The difference is k:

- shipped: k is sin(half) times r **rounded once**: the sine is not rounded to single precision before the
  multiplication (believed: an x87 sine kept in its register; the cosine is rounded, as if stored);
- ours: the sine is rounded to single precision first, then multiplied by r.

The padding entries of the returned rotation are zero on both sides (the scenario zeroes the array first;
whether the call writes them is not distinguished). The rotation read back from a geometry after
`dGeomSetRotation` is the rotation given, on both sides [R1]: setting a geometry's rotation changes nothing.

Required: the scale of the quaternion's vector part is the unrounded sine times r, rounded once.

### 2.3 Setting a body's rotation, and the rotation from a quaternion (rotation, angular, denormal)

The rotation rebuilt from a quaternion (`dBodyGetRotation` after any set or step) agrees on both sides and is
**reproduced** by round 1's conversion (rule of round 1 [Q2]) for every body of the rotation scenario [Q1].

`dBodySetRotation`'s quaternion differs, only for rotations whose trace is not negative. **Reproduced** for
every such rotation [Q3]: t = trace + 1, s = sqrt(t), q0 = s / 2, the other three components the
antisymmetric differences times 0.5 / s, then the quaternion normalised (4-term squared length in index order,
times 1 / sqrt). The trace is summed:

- shipped: `(R11 + R22) + R00` (the last two diagonal entries first);
- ours: `(R00 + R11) + R22`.

For a negative trace both sides agree on every rotation of the scenario [Q3]. How "0.5 / s" is formed
(a reciprocal, or 1 / (2s), or 0.25 / q0) is not distinguished by normal-range values; the checks use 0.5 / s.

The normalisation of this quaternion behaves as if computed with a 24-bit significand and a wider exponent
range, and stored afterwards: a denormal result is rounded twice (first to 24 significant bits, then to the
denormal grid) and is not flushed, even in the game's environment. **Reproduced** for every case of both
denormal runs; three cases of the unflushed run tell it from plain single-precision rounding [D1]. In the
game's environment the components it normalises are already flushed, so this shows only where a component
lies within one rounding of the smallest normal value (**believed**, not exercised).

Required: the trace order above; and the double rounding, unflushed, of the normalisation's products.

### 2.4 The angular velocity update, both steppers (angular; also free, worlds, denormal)

**Reproduced** for every single step of the angular scenario, the whole body after the step (position,
quaternion, rotation, both velocities), both steppers, both sides [A3]. A lone body without joints, from the
inertia I of its `dMass` (diagonal in these scenarios), its rotation R, angular velocity w and accumulated
torque T:

1. The body-frame inverse inertia (diagonal): each entry `(1 / c) / c`, c = sqrt(I entry). Both sides.
2. The world-frame inverse inertia M and world-frame inertia J: element (i, j) is the 3-term sum over k of
   `R[i][k] × (D[k] × R[j][k])` (D the diagonal: the inverse inertia for M, the inertia for J).
3. The gyroscopic term: L = J w (3-term sums), torque T − (w × L).
4. The update: w += M (h × torque), each row a 3-term sum.

Both steppers of ours use source order everywhere. The shipped library's orders (the sums of each row; every
element of a row of M or J uses that row's order):

| Sum | QuickStep (`dWorldQuickStep`) | Exact stepper (`dWorldStep`) |
|---|---|---|
| M, rows 0 / 1 / 2 | `(y + z) + x` / `(y + z) + x` / `(y + z) + x` | `(x + z) + y` / `(y + z) + x` / `(y + z) + x` |
| J, rows 0 / 1 / 2 | `(x + z) + y` / `(y + z) + x` / `(y + z) + x` | `(y + z) + x` for every row |
| L = J w, rows 0 / 1 / 2 | `(x + z) + y` / `(x + z) + y` / `(y + z) + x` | source order for every row |
| update M (h × torque), rows 0 / 1 / 2 | source order / `(x + z) + y` / source order | `(y + z) + x` for every row |

(QuickStep's update row 0 is equally reproduced with `(x + z) + y`; the two are not distinguished.)

At the identity rotation every one of these sums has at most one non-zero term, which is why the angular
scenario's identity cases agree whenever the masses agree [A1]. The position update and the quaternion update
(round 1 section 4.2, with its third-component order, which our build already follows) are reproduced on both
sides [A3, W1].

Required: the orders of the table for the shipped library.

### 2.5 The torque of a relative force at a relative point (free)

**Reproduced** for every step after the first of every body of the free scenario without finite rotation,
both steppers [F1]. `dBodyAddRelForceAtRelPos` rotates the force and the point by the body's rotation (each
a row-by-vector 3-term sum) and adds the point × force cross product to the torque, after the torque added by
`dBodyAddTorque`. The force's rotation order is not distinguished by this scenario (its middle component is
zero); the point's rotation is summed `(y + z) + x` per row by the shipped library, in source order by ours.
The linear update is round 1's 4.1, which our build already follows.

Required: the point rotated with `(y + z) + x` rows.

### 2.6 Finite rotation without an axis (free, body 1)

**Reproduced** for every step after the first [F1]. With wlen = |w| (3-term length in index order, rounded
square root), th = wlen × (h / 2), and k = (sin th / th) × (h / 2), the rotation quaternion is
(cos th, w × k), multiplied on the left of the body's quaternion, then normalised (4-term length in index
order, times 1 / sqrt). The product's components are 4-term sums of their usual terms (for component 1:
a0 b1, a1 b0, a2 b3, then minus a3 b2; the same pattern for 2 and 3).

- shipped: the sine is not rounded before the division by th; component 0 summed in source order;
  components 1 to 3 summed first term, third term, second term, then the fourth.
- ours: the sine rounded first; every component in source order.

Required: the unrounded sine and the component orders above. Finite rotation about a set axis (body 2): the
velocities are reproduced, the orientation is **unestablished** (no candidate tried reproduced either side).

### 2.7 Box against plane: the deepest contact (pairs)

**Reproduced** for every box–plane pair when at most one contact is asked for, given each side's rotation
(2.2) [P1]: the contact point is the deepest corner (each coordinate a 3-term sum in source order of the
half-sides times the rotation's columns), the normal the plane's, and the depth the plane offset plus the sum
of the three half-extents along the normal (half-side times the absolute value of the normal's component
along each column): shipped `(a1 + a2) + a0`, ours `(a0 + a1) + a2`. Further contacts (at most eight asked
for), and every other pair kind (box–box, sphere–box, capped cylinder–box, two capped cylinders), differ
also where every rotation agrees [P0]: their colliders' own arithmetic is **unestablished**.

Required: the depth order above.

### 2.8 `dNormalize3_slow` with equal largest magnitudes (normalise-ties)

Identical on both sides, and **reproduced** (with round 1 section 3's five steps) only when, among components
of equal largest magnitude, the **lowest index** is taken as the largest [N1]. Taking the highest index, or
comparing strictly in index order with ties going to the later component, misses some vectors.

Required: the lowest index wins a tie. Round 1's normalise scenario and this one are both reproduced with it.

### 2.9 Denormals-are-zero (#769)

The shipped library's runs with and without flushing differ in joints and in denormal, and agree in contacts
[C0]. Ours gives the same with and without, and is **reproduced with flushing in both runs**: our build flushes
whatever the environment [C0, D1].

- In a lone body's step (denormal scenario) **reproduced** with flushing for the flushed run and without for
  the unflushed one, for every case, both steppers [D1]. Removing flushing from one stage at a time [D2]: the
  world-frame inverse inertia, the world-frame inertia, L and the gyroscopic cross product do not depend on it;
  the angular velocity update (h × torque, the inverse inertia times it, the sums), the quaternion update and
  the quaternion-to-rotation conversion do. The exception is the set-rotation normalisation (2.3).
- Joints (carried forward from round 1's review): the runs first differ at the 29th step, in the **held body's
  angular velocity only**, while its position, quaternion and rotation are still identical; nothing but the
  held body and its joint's feedback ever differs [J1].

Required: flushing in the integration as round 1 says, established now for the lone body's angular update,
quaternion update and rotation conversion; not in the set-rotation normalisation.

## 3. Per scenario

- **library, x87, normalise, normalise-ties**: identical [C0].
- **angular**: masses differ (2.1); rotated cases differ already before the step through the set rotation
  (2.3), and after it through the angular update (2.4); identity cases with equal masses agree. All
  reproduced on both sides [M1, A3].
- **rotation**: the rotation from axis and angle differs in about a quarter of the cases (2.2); the body's
  quaternion differs additionally in some cases with equal rotations (2.3) [R1, Q3].
- **denormal, denormal-nodaz**: as angular plus 2.9; all reproduced [D1].
- **free**: from the first step (mass, rotations); every later step of every body reproduced except the
  orientation of the body with a finite-rotation axis [F1]. The first step needs the initial set rotation of
  negative-trace rotations, which both sides compute alike but the check does not recompute (the check starts
  from the first recorded step).
- **worlds**: parameters and the enabled flag agree; the body outside every world, stepped by `dxStepBody`, is
  identical on both sides; the lone bodies are reproduced on both sides (they differ through the box mass);
  the two jointed bodies differ through the mass and the solver [W1].
- **joints, contacts, contacts-nodaz, heavy, contacts-exact**: differ from the first step; every rule of
  section 2 applies to them, the pile scenarios also use capped-cylinder masses (unestablished) and the
  colliders (2.7). The solvers' share is not separated (4).
- **exact-step**: in all but a few trials the recorded inputs (masses, quaternions, rotations) already differ
  between the sides, through 2.1 and 2.3; the trials with identical inputs still differ, as expected from 2.4,
  which feeds the solve. The exact stepper's solve is therefore not yet separated (4).
- **pairs**: differs through rotations and the colliders (2.7); the count of contacts differs only for two
  boxes [P0].
- **rays**: every ray set (position and rotation) and every result against the sphere are identical, and the
  ray on a body places it identically: round 1's ray–sphere difference is gone. Results against the box and the
  capped cylinder differ, partly through their rotation (2.2) and partly where the rotation agrees [Y1].

## 4. Round 1's section 8: settled and still open

Settled:

- The angular velocity update's arithmetic in both steppers, ours included (2.4), together with the mass
  functions it depends on (2.1).
- `dRFromAxisAndAngle` (2.2), and the rotation setters: a geometry's rotation is kept as given; a body's goes
  through 2.3.
- The finite-rotation path without an axis (2.6).
- `dNormalize3_slow`'s tie (2.8).
- Which operations of a lone body's step depend on denormals (2.9).

Still open, with what would settle each:

- Finite rotation about a set axis (2.6): a scenario stepping a lone body once in that mode, recording its
  state before and after (as angular does), with the axis along a coordinate axis and then a general one.
- `dMassSetCappedCylinderTotal`: a scenario recording its `dMass` for recorded inputs.
- The colliders' own arithmetic for box–box, sphere–box, capped cylinder–box, two capped cylinders, the
  box–plane contacts after the deepest, ray–box and ray–capped cylinder, and through them the box–box
  contact-count differences (believed rounding near the thresholds). The recorded data suffices in principle
  (every input is recoverable with 2.2), but the arithmetic was not searched. A scenario that also records
  each geometry's position and rotation as read back would make every case self-contained.
- The QuickStep row solve and the exact stepper's solve. Once our build follows section 2, a rerun of exact-step,
  joints and contacts separates the solves' own differences; a scenario of single steps with sphere masses at
  the identity rotation and one or two contacts would separate them even before.
- Which operation first flushes in the held body's step (joints): **believed** the angular update's products
  (2.9 shows they are the only angular stage that depends on flushing in a lone body), with the solver's
  equivalent products as the alternative. A scenario holding one body with a fixed joint to the world at a pose
  with tiny quaternion components, stepped once with and without flushing, recording the joint's feedback and
  the body, would settle it.
- Whether anything in the game reads the ray rotation's padding words: not a question the harness can answer
  (believed not). The harness now records them as zero.

## 5. What our implementation must do to be bit-exact

Beyond round 1's requirements (sections 2 to 7 there, which this round confirms where our build already
follows them):

1. The sphere and box mass inertia orders of 2.1.
2. `dRFromAxisAndAngle`'s single rounding of the sine times r (2.2).
3. `dBodySetRotation`'s trace order, and its normalisation's unflushed double rounding (2.3).
4. The angular update's orders of 2.4, per stepper.
5. The relative point's rotation order of 2.5.
6. The finite rotation without axis of 2.6.
7. The box–plane depth order of 2.7.
8. The lowest index wins a `dNormalize3_slow` tie (2.8; our build already does).
9. Flushing as 2.9 says (our build already flushes in the integration).

## 6. Rules of thumb (believed, not a requirement)

Many of the shipped library's 3-term sums that differ from ours add the last two terms first (`(y + z) + x`):
the ray–sphere dot products of round 1, the trace, the box–plane depth, the relative point's rotation and most
rows of the world-frame matrices. Others keep source order (the quaternion-to-rotation conversion, the
position update) or use `(x + z) + y`. No order may be assumed for a sum not listed here or in round 1.

## 7. Checks

All in `<workspace>/checks/`, run together by one runner: `python -I <workspace>/checks/run_all.py` (exit 0
when every check passes; it also applies the repository's spec-lint rules to this draft, `spec_lint.py`,
because Bazel is not set up on this PC). Each check prints counts only.

- [C0] `compare_status.py`: per scenario identical or not (NaNs equal, as `compare.c`); the shipped library's
  and our runs with and without flushing.
- [M1] `mass.py`, with `angular_inputs.py` (the angular scenario's regenerated inputs, checked against the
  recorded ones) [A1].
- [Q1] `qtor.py`; [R1] `rfaa.py`; [Q3] `rtoq.py`.
- [A3] `angular.py` (rules in `body_step.py`).
- [D1] `denormal.py`; [D2] `denormal_stages.py`; [J1] `joints_dz.py`.
- [F1] `free.py` (with `free_inputs.py`, `free_step.py`); [W1] `worlds.py`.
- [N1] `normalise_ties.py`; [P1] `boxplane.py`; [P0] the pair kinds that differ with equal rotations are
  counted by `pairs.py` (run on its own: it prints counts, no verdict); [Y1] `rays.py`.

Helpers: `lib.py` (records, single-precision operations with optional flushing, the input generator),
`exact.py` (exact rounding of rationals, to single precision or to a 24-bit significand). The sine and cosine
of the x87 are taken as the double-precision functions rounded (or, unrounded, as a double): every check
reproducing them passes on every case, and the x87 scenario is identical on both sides.

## 8. Provenance

- Date: 2026-11-27. A Windows PC; S.T.A.L.K.E.R.: Shadow of Chernobyl (GOG), its `bin\ode.dll` under
  `<game folder>`, loaded unmodified by the harness, with the run time the library loads.
- Harness: release `odeharness-56caa53187a3` (zip SHA-256 checked against the tag), run as its README and
  `docs/testing.md` say; exit code 0; compared with `odeharness.exe compare ours results`; rerun once with
  byte-identical results.
- Analysis: only the API outputs the harness records, for its own synthetic inputs (regenerated with the
  scenarios' own generator where not recorded), compared with candidate single-precision arithmetic, by a
  dirty investigator on the PC. Our build's behaviour was taken from our outputs and, for what our build
  changes, from our own repository's patches. No debugger, tracing, memory inspection, disassembly or
  decompilation; nothing from the binary's code; no leaked or derived source; no recollection of either
  program's source used as a fact.

## Review: disputed

Verdict: approved; no clean-criteria violation. The draft has no raw output values, numeric tables, dumps,
hex or byte sequences, pseudo-code or anything taken from the binary's code, and no machine details.

What was run: every check of section 7 through `checks/run_all.py` (all pass, spec-lint rules included and
matching the repository's), the harness's compare mode again (the same output as `compare.txt`), a comparison
of the rerun's results with the first run's (byte-identical), and the reviewer's own checks in
`<workspace>/review_checks/` (`rv_exact_step.py`, `rv_rtoq_denorm.py`, `rv_orders.py`, `rv_products.py`,
`rv_mass_share.py`, `rv_worlds_share.py`, `rv_boxplane8.py`), on the recorded outputs only. Confirmed
beyond the draft's checks: every order of the table of 2.4 is the only grouping that reproduces the shipped
library (apart from the undistinguished QuickStep update row 0 it names); the product grouping and the
inverse-inertia form of 2.4's steps 1 and 2 are discriminating; the constants of 2.1 are single precision,
not double.

1. **Section 2.3 (and section 5 item 3): the normalisation unflushed "even in the game's environment".**
   The evidence establishes the double rounding only for the unflushed run. In the flushed run no
   quaternion component reaches the denormal range, and plain single-precision rounding with flushing
   reproduces every case just as well, so whether this normalisation flushes in the game's environment is
   **unestablished**; the "Required" line's "unflushed" holds only for the environment without flushing.
2. **Section 2.9, first bullet (and section 4, the settled item on denormals and the open item on the held
   body): "the world-frame inverse inertia, the world-frame inertia, L and the gyroscopic cross product do
   not depend on it".** The evidence shows only that in the denormal scenario, removing flushing from those
   stages changes no case; whether they flush is not distinguished. It does not establish that they need no
   flushing, nor that the angular update's products are the only angular stage where flushing can matter.
   Correct statement: the angular velocity update, the quaternion update and the rotation conversion are
   shown to flush; the other stages are undistinguished (flush them, as round 1 requires for the integration).
3. **Section 1, the paragraph after the table: "most of what remained of round 1's angular differences comes
   from the mass functions".** No check backs it, and the free scenario contradicts it: replaying the
   shipped library's steps with its mass rule but our build's angular orders removes only about a third of
   the angular-velocity mismatches, and the angular orders alone remove fewer still; both 2.1 and 2.4 are
   needed and neither accounts for most.
4. **Section 3, worlds (and section 1's worlds row): "the lone bodies ... differ through the box mass".**
   The body left alone after its joint is removed differs only because it carries the jointed phase's
   difference: its lone steps are reproduced by either side's rules. The other lone body's difference
   needs both the mass rule (2.1) and the angular update's orders (2.4) to be explained.
5. **Section 3, "joints, contacts, contacts-nodaz, heavy, contacts-exact: differ from the first step".**
   contacts-exact is identical for its first eight steps (as in round 1) and differs from the ninth on; the
   others do differ from the first step.
6. **Section 3, exact-step: "the trials with identical inputs still differ, as expected from 2.4".** Not
   every such trial differs: one trial with identical inputs gives identical results on both sides. That the
   remaining differences come from 2.4 is not checked; with the solve not emulated, their cause is
   **unestablished**.
7. **Section 3, free: "The first step needs the initial set rotation of negative-trace rotations, which both
   sides compute alike".** Most of the free bodies' initial rotations have a non-negative trace, where the
   sides differ (2.3). Correct statement: the first step is not checked because the state before it is not
   recorded; it depends on the initial rotations, through 2.2 and 2.3.
8. **Section 2.7: "Further contacts (at most eight asked for) ... differ also where every rotation agrees".**
   For box against plane, whenever the rotation agrees, the further contacts differ only in pairs whose
   deepest contact also differs (the depth order); no difference of their own is shown. Their arithmetic is
   still not emulated, so it remains **unestablished**, but the evidence does not show it differs. The
   statement holds for the other pair kinds.

## Gaps found in review

1. **Sections 1 and 3, angular:** the rotated cases' differences before the step are attributed to the
   set rotation (2.3) only; the rotations themselves come from `dRFromAxisAndAngle`, so 2.2 contributes
   too (some rotated cases with equal masses already differ before the step).
2. **Section 2.3:** for rotations with a negative trace no rule is stated or reproduced; the evidence shows
   only that both sides agree when given the same rotation.
3. **Section 2.7:** the box sits at the origin and the plane's normal lies along a coordinate axis, so
   neither the box centre's distance to the plane nor the summation of the normal's dot product with each
   column is exercised; the stated depth and corner rules are established for that configuration only.
4. **Section 2.6:** only angular speeds well away from zero are exercised; finite rotation at small angles
   is not covered.
5. **Section 7:** several statements have no check that prints them: the angular scenario's identity cases
   agreeing whenever the masses agree (cited as [A1], which does not test it), the few unflushed cases that
   tell the double rounding from plain rounding (2.3, cited as [D1], which does not count them), the
   rotation scenario's share of differing cases (`rotation_data.py`, not in the runner), contacts-exact's
   first differing step, and every exact-step statement of section 3. The reviewer's checks confirm the
   first two.
