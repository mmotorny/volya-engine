# The physics library against the shipped one, round 4: the joints at general rotations

Behavioural and functional findings, for the shipped `bin\ode.dll`, of the arithmetic behind the differences
the round-4 harness run showed in its one differing scenario, `joints-posed`, so that our ODE 0.5.0 build can be
patched to give its results bit for bit. It extends `docs/behavior/physics-harness-3.md` (cited as `round 3`),
`docs/behavior/physics-harness-2.md` (`round 2`), `docs/behavior/physics-harness.md` (`round 1`) and
`docs/behavior/physics-library.md` (`library`), with their conventions: public ODE 0.5.0's API names and
concepts, single precision, "h" the step length, a 3-term sum written by its grouping (`(x + y) + z` "source
order", `(y + z) + x` the last two first, `(x + z) + y` the first and last first), each operation rounded to
single precision and flushed (the game's environment, `library` 7.2) unless marked **x87** (round 3's meaning:
done by the x87 unit at the 24-bit precision the game sets, rounding the significand as single precision does,
never flushed; in the normal range equal to the single-precision operation, and no value of this round's
computations leaves the normal range). "R v" is a vector rotated by a body's rotation (component i = row i · v),
"Rᵀ v" one taken into the body's frame (component i = column i · v); "rows `(y + z) + x`" means each of the three
components is summed so.

The scenario (`tests/odeharness/scenarios.c`, `joints-posed`): 2,700 cases, each a world of its own with two box
bodies at general positions and rotations and one joint; the kinds in turn (ball, hinge, slider, universal,
hinge-2, fixed by `dJointSetFixed`, fixed by `dJointSetFixedQuaternionPos`, angular motor in user mode with
three axes, in Euler mode), attached to two bodies, to one body with the world second, or to one body given
second with the world first (hinge-2 always two); the setters with an anchor near the first body and general
axes, limits on half the cases; then the joint's getters, the bodies moved (turned, shifted, given velocities),
the getters again, one step (QuickStep or the exact stepper) and the bodies, the joint's feedback and the getters
after it. Of its 577,800 recorded words, 14,348 differed between the shipped library and our build (body 4,211,
feedback 3,158, joint getters 6,979) [P0]; every other scenario of the run was identical.

How the findings were established, as in round 3:

- **static**: read from the shipped library's instructions (the dataflow of every stored value, read
  symbolically, which makes the grouping of each sum and product explicit and keeps the x87 and SSE operations
  apart); a check holds the stated rule to the expressions read, or evaluates the expressions read on random
  inputs and compares them with the rule bit for bit [Q1 – Q8];
- **reproduced**: a model with the rule recomputes, bit for bit, every recorded output of that kind in the
  scenario: the getters from the recorded bodies and the joint's stored values as the setters (with their rules)
  leave them, each step from the recorded bodies before it through round 3's steppers (whose rules our build
  already has) with the run time's `rand` carried from case to case; the same model with our build's rule
  recomputes our build's outputs; and the shipped rule with one rule given our build's arithmetic misses some of
  the shipped records, so every rule is needed [P1 – P3];
- **believed** and **unestablished** as before.

Our build is public 0.5.0 with `third_party/ode`'s patches as they stand after round 3. Bracketed ids are check
scripts (section 12).

## 1. Summary

Counts are of the scenario's 2,700 cases: per kind 900 getter records (three per case) and 300 steps (both
bodies and the feedback).

| Kind | Differing in the round-4 run | Cause (section) | Shipped rule | Our build's rule on the shipped records |
|---|---|---|---|---|
| ball, fixed (both setters) | nothing | — (round 3's rules, now at general rotations: 2) | 900 getter records, 300 steps each kind | identical |
| hinge | one-body cases: the angle after the move and the step; steps with limits | the angle with one body (3) | 900, 300 | 874, 296 |
| slider | the position (one body), the position rate; one-body steps | 4 | 900, 300 | 636, 258 |
| universal | the angles and their rates | 5 | 900, 300 | 449, 300 |
| hinge-2 | the angle, both rates; most steps | 6 | 900, 300 | 46, 81 |
| angular motor, user mode | the axes relative to a body; steps | 7.1 | 900, 300 | 137, 223 |
| angular motor, Euler mode | the axes, the angles; most steps | 7.2 | 900, 300 | 94, 76 |

All 2,700 steps are reproduced by the shipped rule (our build's rule: 2,134); our build's rule reproduces every
one of our build's getter records and steps [P1, P2].

## 2. Round 3's rules at general rotations

Round 3 established the setters' arithmetic statically only (its review's gap 2: the joints scenario sets
every joint up at identity rotations). Here every setter runs at general rotations, and with round 3's rules
(its 5.1 rule 1 and 5.2 item 15, `round 3` 8 items 17 and 25), which our build already has, every getter record
and every step of the ball, hinge with two bodies, two-body slider and both fixed setters is reproduced
[P1, P2]. Each of those rules is needed (the shipped rule with that rule as public 0.5.0 has it) [P3]:

| Rule (`round 3`) | Without it: getter records, steps (of the kinds using it) |
|---|---|
| the anchor setters' Rᵀ (anchor − body position), rows `(y + z) + x` (ball, hinge, universal, hinge-2) | 2,369 of 3,600; 439 of 1,200 |
| the axis setters' Rᵀ axis, rows `(y + z) + x` (hinge, slider, universal) | 1,154 of 2,700; 397 of 900 |
| the anchor and axis getters' R v, rows `(y + z) + x` (ball to hinge-2) | 1,326 of 4,500; 1,479 of 1,500 |
| `dJointSetHingeAxis`'s relative rotation, scalar part 3, 0, 2, 1 | 855 of 900; 295 of 300 |
| `dJointSetSliderAxis`'s relative rotation, scalar part 0, 3, 2, 1 | 900 of 900; 274 of 300 |
| `dJointSetSliderAxis`'s stored offset `(y + z) + x` | 768 of 900; 249 of 300 |
| `dJointSetFixed`'s relative rotation, scalar part 3, 0, 2, 1 | (no getters); 275 of 300 |
| `dJointSetFixed`'s stored offset `(y + z) + x` | (no getters); 246 of 300 |

Round 3's rows (its 5.1) reproduce every step of every kind at these
rotations, the hinge-2 rows with their limit included; round 3's steppers are unchanged.

## 3. The hinge: the angle with one body

`dJointGetHingeAngle` and the hinge's limit test (which measures with the same arithmetic [Q8]) with one body
(attached to one body and the world, in either order): the relative rotation is the body's quaternion
conjugated times the stored one (public 0.5.0's product with one body; its terms grouped differently but giving
the same values, as round 3 5.1 rule 5 says), and from it the angle **by the same arithmetic as with two bodies**
(round 3, 5.2 rule 12): the sine of the half angle `√((q3² + q2²) + q1²)` on the x87, the sign test's dot of the
vector part with the stored axis `(y + z) + x`, the angle twice the x87 arc tangent, reduced by 2π above π, then
`−0 − θ` (static: the one-body product joins the two-body path's code before the angle [Q3]; the expressions
read equal the rule on 3,000 random inputs, public 0.5.0's on 1,887 [Q3]); the getter then negates it when the
joint's bodies were swapped (as public). Our build uses public 0.5.0's angle with one body (squares in source
order, the dot in source order). Reproduced: every hinge getter record and step [P1, P2]; with our build's angle
with one body, 874 of 900 records and 296 of 300 steps [P3].

## 4. The slider

1. **`dJointGetSliderPosition` with one body** (and the slider's limit test, which calls it [Q8]): the stored
   axis rotated by body 1 (rows `(y + z) + x`, round 3), c = body 1's position − the stored offset, and the result
   `(c2 ax2 + c1 ax1) + c0 ax0` on the x87, `(y + z) + x`: the same dot as with two bodies (round 3, 5.2 rule 14)
   (static [Q1]). Our build: source order. Without it: 800 of 900 records, 292 of 300 steps [P3].
2. **`dJointGetSliderPositionRate`**: the rotated axis (as above) dotted with body 1's linear velocity, and with
   two bodies minus its dot with body 2's: both dots `(y + z) + x` and the subtraction on the x87 (static [Q1]).
   Our build: both dots in source order. Without it: 706 of 900 records [P3].
3. **The rows with one body** (the slider attached to one body and the world): rows 0 to 2 are round 3's fixed
   orientation rows with one body; rows 3 and 4 have the plane-space vectors p and q of the rotated axis as body
   1's linear Jacobian (public 0.5.0's, no angular part), and right-hand sides `k × (p · o)` and `k × (q · o)`
   with o = the stored offset − body 1's position and k = erp × fps: **`p · o` in source order, `q · o`
   `(y + z) + x`** (static [Q2]); the limit row as public (linear, one body). Our build: both dots in source
   order. Without the second: 266 of 300 steps [P3].

## 5. The universal joint

1. **The setters' relative rotations** (`dJointSetUniversalAnchor`, `dJointSetUniversalAxis1`,
   `dJointSetUniversalAxis2` each recompute them, from the axes then stored): the two axes in world coordinates
   (rotated by their bodies, rows `(y + z) + x`, round 3; an axis of a missing body as stored); for relative
   rotation 1 the rotation from axis 1 and axis 2 (`dRFrom2Axes`) converted to a quaternion (round 3 7.5's
   conversion), then body 1's quaternion conjugated times it, the **scalar part adding the products of components
   0, 1, 3, 2** in that order; for relative rotation 2 the rotation from axis 2 and axis 1 converted likewise, then
   body 2's quaternion conjugated times it, the **scalar part adding the products of components 1, 3, 2, 0**; with
   no body 2, relative rotation 2 is the converted rotation itself. The vector parts as public 0.5.0 (static
   [Q4]). Our build: both scalar parts in index order. Without it: 724 of 900 records [P3].
2. **`dRFrom2Axes`**: as public 0.5.0, with both lengths the x87 square root of the squares summed in index order,
   each stored (static [Q4]), and the reciprocals and the rest single precision as public (reproduced through
   items 1 and 3 [P1]).
3. **`dJointGetUniversalAngle1` and `dJointGetUniversalAngle2`** (with the joint's bodies swapped, each returns
   the other's, as public): the cross rotations as in item 1 (from the current axes and bodies), the relative
   rotation as public 0.5.0 (body's quaternion conjugated times the cross rotation, times the stored relative
   rotation conjugated; scalar parts in index order), and the angle from it: the sine of the half angle
   `√((q1² + q2²) + q3²)` on the x87 (index order, as public), the sign test's dot of the vector part with the
   stored axis (axis 1, or axis 2 for angle 2) **`(y + z) + x`**, then round 3's tail (twice the x87 arc tangent,
   reduced by 2π above π, `−0 − θ`); angle 2 is then `−0 −` that (static [Q4]). Reproduced: every angle [P1]. The
   sign dot's order is **not distinguished** by the data (our build's source order gives the same 900 records)
   [P3]; it is a requirement all the same. The universal joint's limit test measures with the same two functions
   (static [Q8]; the scenario has no universal limits).
4. **`dJointGetUniversalAngle1Rate`, `dJointGetUniversalAngle2Rate`**: the axis (axis 1 rotated by body 1, axis
   2 rotated by body 2, or as stored without it; swapped with the bodies, as public) dotted with each body's
   angular velocity, **both dots `(y + z) + x`**, body 2's subtracted (static [Q4]). Our build: source order.
   Without it: 571 of 900 records [P3].

The universal joint's rows are round 3's (5.1 rule 6); every step is reproduced [P2].

## 6. Hinge-2

1. **The axis setters' stored sine and cosine** (`dJointSetHinge2Axis1` and `dJointSetHinge2Axis2`, after
   storing the normalised axis in their body's frame, Rᵀ rows `(y + z) + x`, round 3): axis 1 rotated by body 1
   and axis 2 rotated by body 2, rows **`(y + z) + x`**; v = axis 1 × axis 2 (public's cross product); the stored
   sine `√((v2² + v1²) + v0²)` with the squares and sums on the x87 (`(y + z) + x`) and the square root on the x87,
   stored; the stored cosine `axis 1 · axis 2` summed **`(x + z) + y`** (static [Q5]). Our build: the rotations,
   the squares and the cosine in source order. Without the rotations' rule: 114 of 300 steps; without the
   squares' and cosine's: 210 of 300 [P3].
2. **The reference vectors v1 and v2** (made after every hinge-2 setter, as public 0.5.0, including its test
   that leaves them unchanged when an axis is zero or the axes are equal): axis 1 rotated by body 1 and axis 2 by
   body 2, rows `(y + z) + x`; k = axis 1 · axis 2 summed **`(y + z) + x`**; axis 2 − k axis 1 normalised with its
   squared length summed **`(y + z) + x`** (`library` 3.5's threshold rule: below it `dNormalize3_slow`, at or
   above it each component times the x87 reciprocal square root, stored); v1 = R1ᵀ (that vector) and v2 = R1ᵀ
   (axis 1 × it), rows `(y + z) + x` (static: the expressions read equal the rule on 2,000 random inputs [Q5]).
   Our build: every rotation, the dot and the squared length in source order. Without the rotations: 181 of 900
   records, 256 of 300 steps; without the dot: 718 and 289; without the squared length: 831 and 295 [P3].
3. **`dJointGetHinge2Angle1`** (and the hinge-2's limit test, which calls the same arithmetic [Q8]): a = R1ᵀ (R2
   axis 2), both rotations' rows `(y + z) + x`; x = v1 · a and y = v2 · a, each summed `(y + z) + x` on the x87;
   the angle is minus the x87 arc tangent of y over x (extended precision), rounded once to single precision
   (static [Q5]). Our build: the rotations and dots in source order. Without the rotations: 265 of 900 records,
   269 of 300 steps; without the dots: 397 and 279 [P3].
4. **`dJointGetHinge2Angle1Rate`, `dJointGetHinge2Angle2Rate`**: axis 1 rotated by body 1 (for rate 2, axis 2
   rotated by body 2), dotted with each body's angular velocity, **both dots `(y + z) + x`**, body 2's subtracted
   (static [Q5]). Our build: source order. Without it: 566 of 900 records [P3].

The hinge-2 rows are round 3's (5.1 rule 7), with the limit row of axis 1 (round 3's rule 9) at the angle of
item 3; every step, with and without limits, is reproduced [P2].

## 7. The angular motor

### 7.1 Both modes: the axes

1. **`dJointSetAMotorAxis`**: an axis relative to a body (rel 1 or 2; rel 2 taken as 1 when there is no second
   body, as public) is taken into that body's frame, Rᵀ rows **`(y + z) + x`**, then (every axis) normalised with
   its squared length in index order (`library` 3.5's threshold rule) (static [Q6]). Our build: source order.
   Without it: 712 of 1,800 records (both modes), 372 of 600 steps [P3].
2. **`dJointGetAMotorAxis`**: an axis relative to a body rotated by it, rows **`(y + z) + x`**; a global axis as
   stored (static [Q6]). Our build: source order. Without it: 519 of 1,800 records [P3].
3. **The rows' axes**: in user mode, an axis relative to body 2 rotated by body 2, rows **`(y + z) + x`**; an
   axis relative to body 1 is used as stored (0.5.0's rule, `library` 5.1, confirmed: the code rotates it and then
   overwrites it with the stored axis), a global axis as stored. In Euler mode, axis 0 rotated by body 1 and axis
   2 by body 2 (as stored without it), both rows **`(y + z) + x`**, and axis 1 = axis 2 × axis 0 normalised
   (squared length in index order, threshold rule) (static [Q6]). Our build: the rotations in source order.
   Without it: 1,588 of 1,800 records (the Euler angles use these axes), 425 of 600 steps [P3].
4. The rows themselves: per axis i below the axis count, round 3's limit and motor row (rotational) on the
   axis (in Euler mode on axis 1 × axis 2, axis 1 and axis 0 × axis 1 for axes 0, 1, 2, public's cross
   products); the row count's limit test as public 0.5.0 on the stored angles (static [Q8]; reproduced [P2]).

### 7.2 Euler mode

1. **The reference vectors** (made by `dJointSetAMotorMode` and by `dJointSetAMotorAxis` in Euler mode, as public
   0.5.0): with two bodies, reference 1 = R1ᵀ (R2 axis 2) and reference 2 = R2ᵀ (R1 axis 0); with one body,
   reference 1 = R1ᵀ axis 2 and reference 2 = R1 axis 0; every row **`(y + z) + x`** (static [Q7]). Our build:
   source order. Without it: 656 of 900 records, 284 of 300 steps [P3].
2. **The Euler angles** (computed when the step counts the rows, from the axes of 7.1 item 3; `dJointGetAMotorAngle`
   returns the last ones computed): reference 1 rotated by body 1, reference 2 by body 2 (as stored without it),
   rows **`(y + z) + x`**; angle 0 = −atan2(axis 2 · (axis 0 × ref 1), axis 2 · ref 1), angle 1 =
   −atan2(axis 2 · axis 0, axis 2 · (axis 0 × axis 1)), angle 2 = −atan2(ref 2 · axis 1, ref 2 · (axis 1 × axis 2))
   (public 0.5.0's), **each of the six dots `(y + z) + x` on the x87**, the cross products public's (single
   precision), each angle minus the x87 arc tangent rounded once when stored (static [Q7]). Our build: the
   rotations and the dots in source order. Without the references' rotations here: 751 of 900 records, 296 of 300
   steps; without the dots: 644 and 288 [P3].

## 8. Per scenario

- **joints-posed**: every getter record (8,100, of which 1,800 fixed records are empty) and every one of the
  2,700 steps, QuickStep and exact alike, both bodies and the feedback, **reproduced** by the shipped rule; our
  build's rule reproduces all of our build's and 2,134 of the shipped steps [P1, P2]. The run time's `rand` (the
  QuickStep shuffle's) carried through the scenario from 1 keeps every QuickStep case in step.

## 9. What our implementation must do to be bit-exact

Beyond rounds 1 to 3 (which this round confirms at general rotations, section 2), our build (public 0.5.0 with
`third_party/ode`) must change the following; everything not listed is as our build already computes it. Each
item names its section.

**Hinge (3)**

1. The hinge angle with one body (getter and limit test): round 3's two-body tail (the half-angle sine's squares
   `(q3² + q2²) + q1²` on the x87, the sign dot `(y + z) + x`, the reduction and `−0 − θ`) on the one-body
   relative rotation.

**Slider (4)**

2. `dJointGetSliderPosition` with one body: the final dot `(y + z) + x` on the x87.
3. `dJointGetSliderPositionRate`: both dots `(y + z) + x` and the subtraction on the x87.
4. The one-body rows: row 4's right-hand side dot `q · (offset − pos1)` `(y + z) + x` (row 3's stays in source
   order).

**Universal (5)**

5. The setters' relative rotations: the scalar parts in the orders 0, 1, 3, 2 (relative rotation 1) and 1, 3, 2,
   0 (relative rotation 2).
6. The angles' sign dot `(y + z) + x` (the squares stay in index order).
7. Both angle rates' dots `(y + z) + x`.

**Hinge-2 (6)**

8. The axis setters' sine and cosine: the axes rotated with rows `(y + z) + x`, the sine's squares
   `(y + z) + x` on the x87, the cosine `(x + z) + y`.
9. The reference vectors: every rotation's rows `(y + z) + x`, axis 1 · axis 2 `(y + z) + x`, the reduced axis 2's
   squared length `(y + z) + x` before the threshold rule.
10. The angle (getter and limit test): both rotations' rows `(y + z) + x`, the two dots `(y + z) + x` on the x87,
    minus the x87 arc tangent rounded once.
11. Both rates' dots `(y + z) + x`.

**Angular motor (7)**

12. `dJointSetAMotorAxis`: Rᵀ rows `(y + z) + x`.
13. `dJointGetAMotorAxis`: R rows `(y + z) + x`.
14. The rows' axes: the rotations by body 2 (user mode) and both Euler rotations, rows `(y + z) + x`.
15. Euler mode: the reference vectors' rotations, rows `(y + z) + x`; in the angles, the references' rotations
    `(y + z) + x` and the six dots `(y + z) + x` on the x87, each angle minus the x87 arc tangent rounded once.

Round 3's item list stands; with this round its items 17 and 25 (the setters) are reproduced, not only static.

## 10. Corrections to earlier specifications

- `round 3` 5.4 and 10: the one-body slider rows, the universal joint's angles and relative rotations, the
  hinge-2 angle and its v1, v2 vectors, the hinge-2 axis setters' stored sine and cosine and the angular motor's
  Euler mode are established here (sections 4 – 7); they were unestablished.
- `round 3`'s review, gap 2: the setters' arithmetic (round 3's 5.2 item 15 and 8 items 17 and 25) is reproduced
  at general rotations (section 2).
- `library` 5.1 ("the angle, rate, anchor and axis getters ... as public 0.5.0"): the getters' algorithms are
  public 0.5.0's, but their sums are grouped as rounds 3 and 4 say (sections 3 – 7), not in public's order.

## 11. Unestablished

- The normalisations' slow path below the threshold (`dNormalize3_slow`, `library` 3.5) in the hinge-2 reference
  vectors, the angular motor's axes and Euler axis 1: not exercised (every squared length is above it).
- The hinge-2 reference vectors' early return (an axis zero, or the two equal): read as public 0.5.0's, not
  exercised.
- The universal joint's limits (its limit test measures with the getters' functions, section 5, but no scenario
  sets universal limits) and the hinge-2's second-axis motor: not exercised.
- The universal angles' sign dot (5, item 3): static only, not distinguished by the data.

## 12. Checks

All in `<workspace>/ph4/checks/`, run together by `python3 -I <workspace>/ph4/checks/run_all.py` (exit 0 when
every check passes; about two minutes). Each prints counts only. Helpers: `jp_data.py` (the scenario's records per
case and the masses regenerated with the scenario's generator), `jp_model.py` (the setters and getters, shipped
rule and our build's), `jp_step.py` (the step through round 3's `qs_model.py`, `ex_model.py` and `jt_model.py`
with this round's rows), `jp_sym.py` (round 3's symbolic reader of the instructions and the grouping
classifier); round 3's `<workspace>/ph3/checks/` are used unchanged.

- [Q1 – Q8] `jp_static.py`: the groupings read from the instructions (Q1 the slider getters, Q2 the one-body
  slider rows, Q3 the one-body hinge angle, Q4 the universal joint and `dRFrom2Axes`, Q5 hinge-2, Q6 the angular
  motor's axes, Q7 Euler mode, Q8 which arithmetic each kind's limit test and rows use).
- [P0 – P3] `jp_repro.py`: P0 the records and the regenerated inputs, P1 every getter record, P2 every step, P3
  each rule's need (the shipped rule with one rule as our build has it).

The x87 arc tangent is taken as the double-precision function, rounded once where the library stores it (as
round 2 and 3 take the x87 sine, cosine and arc tangent); every check reproducing it passes on every case.

## 13. Provenance

- Date: 2026-11-27. S.T.A.L.K.E.R.: Shadow of Chernobyl (GOG), its `bin\ode.dll` (196,608 bytes).
- Static analysis by a dirty investigator: the library's instructions (an objdump listing) read symbolically,
  the x87 and SSE operations kept apart, and the expressions read evaluated on random inputs where their leaves
  are vectors' components; constants read from the binary. Compared with public ODE 0.5.0 (tag `0.5.0`) and our
  build's patched source (`third_party/ode`).
- Black-box results: the round-4 run (the shipped library loaded unmodified by the harness on the Windows PC and
  driven through its public API with the harness's synthetic inputs, `docs/testing.md`), its `joints-posed`
  results; our outputs from our build's runner (`//tests/odeharness:ours` on main). Inputs not recorded (the
  masses) were regenerated with the scenario's own generator, which also regenerates every recorded setter input.
- No execution or debugging of the shipped library; no leaked or derived source; no recollection of either
  program's source used as a fact.

## Gaps found in review

- `dRFrom2Axes` given an axis of zero length (or a second axis parallel to the first), as the universal setters and angles may call it: neither stated nor exercised.
