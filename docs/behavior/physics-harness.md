# The physics library against the shipped one: what differs and why

Behavioural findings from running the black-box physics harness (`tests/odeharness/`, release
`odeharness-bb67eb79aa9a`) on the shipped `bin\ode.dll` and comparing the results with our build's outputs
that ship with the runner. It extends `docs/behavior/physics-library.md` (cited as `library`) and uses its
conventions: public ODE 0.5.0 API names, single precision, "h" the step length. Scenario names are the
harness's (`scenarios.c`).

How the findings were established: every statement below comes from the shipped library's own API outputs
for the harness's synthetic inputs. A statement marked **reproduced** means a check script recomputes the
shipped library's output, bit for bit, from the recorded inputs (or from the shipped library's own state
after the previous step) with the rule stated, across every case of that kind in the scenario. The same
scripts recompute our build's outputs with our build's rule, which confirms that the inputs and the method
are right. Bracketed ids are those scripts, in the dirty workspace on the Windows PC; the shipped library's
results never left that PC (docs/clean-room.md).

Conventions for sums: "a 3-term sum in source order" means `(t0 + t1) + t2`, each operation rounded to
single precision. Every rule below is about the order and grouping of single-precision operations; the
mathematical formulas are those of public 0.5.0 and of `library`.

## 1. Summary

| Scenario | Result | Cause (section) |
|---|---|---|
| library | one value differs | load-time auto-disable linear threshold (2) |
| x87 | identical | - |
| normalise | differs | `dNormalize3_slow` arithmetic (3) |
| free | differs from the first step | QuickStep linear update (4.1); quaternion update (4.2); angular update (4.3, open) |
| joints, joints-nodaz | differ from the first step | as free, plus the solver (6); denormals (7) |
| contacts, contacts-nodaz | differ from the first step | QuickStep linear update (4.1) and the rest of the step (6); later a contact count differs as a consequence |
| contacts-exact | identical for the first eight steps, then differs | open (6) |
| heavy | differs from the first step | as contacts (6) |
| pairs | differs, including contact counts for two boxes | rotation from axis and angle or the colliders (5.3, open) |
| rays | differs | padding words of the ray's rotation (5.1); ray–sphere arithmetic (5.2); box and capped-cylinder targets (5.3, open) |
| worlds | differs from the first step | as free (4); the lone body's `dxStepBody` (4.2) |

Denormals-are-zero (#769): **yes**, it changes the shipped library's results, in the joints scenario only
(section 7).

## 2. Load-time values (library)

At load, before any `dWorldCreate`, the library-wide auto-disable linear threshold as read back by
`dWorldGetAutoDisableLinearThreshold` is **the same value that `dWorldCreate` later sets** (public 0.5.0's
default threshold), not 0. Every other load-time value and every value after `dWorldCreate` already agrees
[L1].

Required: the library's load-time state holds the default auto-disable linear threshold, so that the getter
returns before any world is made what it returns after `dWorldCreate`.

## 3. `dNormalize3_slow` (normalise)

**Reproduced** for every non-zero input vector of the scenario [N1]. With `k` the component of largest
magnitude `m` and `o1`, `o2` the other two:

1. `inv = 1 / m` (one rounded division);
2. the two other components are multiplied by `inv`; component `k` is set to exactly +1 or −1, the sign of
   the original component (it is not computed);
3. `L = (o1² + o2²) + 1`: the two other squares are added first, then 1;
4. `s = 1 / sqrt(L)`: the square root rounded to single precision, then a rounded division;
5. each of the three components is multiplied by `s`.

A zero vector gives (1, 0, 0), as `library` 3.5 says; that already agrees.

Ours divides the components by `m` instead of multiplying by its reciprocal, computes component `k` instead
of setting it, and sums the squares in index order; that rounds differently in about two vectors in five.

Required: exactly the five steps above. Which component counts as largest when two magnitudes are equal was
not exercised (**unestablished**).

## 4. Body integration (free, worlds; also the first step of every stepped scenario)

### 4.1 QuickStep: the linear velocity

**Reproduced** for every QuickStep step after the first of every body of the free scenario, from the
shipped library's own state after the previous step [F1]. The accumulated force `f` (the added forces, the
relative force rotated by the body's rotation, then the body's weight `mass × gravity` added for bodies with
gravity) changes the linear velocity as

    lvel += h × (invMass × f)        (per component; invMass times f first, then times h)

Ours computes `(h × invMass) × f`, which rounds differently for a few percent of the bodies' steps.
The position update (`pos += h × lvel` with the new velocity) and the exact stepper's linear update
(`lvel += invMass × (h × f)`) already agree on both sides [F1].

Required: under `dWorldQuickStep`, the product `invMass × f` is rounded first and then multiplied by `h`.

### 4.2 `dxStepBody`: the quaternion update without finite rotation

**Reproduced** for every step of the free scenario's bodies without finite rotation and for every
`dxStepBody` call of the worlds scenario's body outside any world [S2] (the new angular velocity `w` and the
previous quaternion `q` are both recorded). The infinitesimal update is public 0.5.0's: `dq = ½ (w ⊗ q)`,
`q += h × dq`, then `q` times `1 / sqrt(q·q)` with the 4-term length summed in index order. Each component
of `w ⊗ q` is a 3-term sum. Three of them are summed in source order. The **third component's (index 2)
derivative**, `−w0·q3 + w1·q0 + w2·q1`, is summed as `(−w0·q3 + w2·q1) + w1·q0`: first and last terms first,
then the middle one. Ours sums it in source order; that is the only difference.

The rotation matrix rebuilt from the quaternion agrees on both sides whenever the quaternions agree
(section 7 has the one exception, denormal values) [Q2]. The finite-rotation path (the free scenario's bodies
1 and 2) was not tested on its own (**unestablished**).

Required: the third quaternion component's derivative is summed in the order above.

### 4.3 The angular velocity update (open)

The angular velocity after a step differs, in both steppers, by rounding (most differences are one unit in
the last place, the third component most often). **Disputed in review:** the draft said our build's angular
update is reproduced exactly by public 0.5.0's formula; no check backs that, and every candidate arithmetic
the checks search, the stated formula included, reproduces only some of our build's own steps. Our build's
angular update arithmetic, as the checks model it, is **unestablished**, so the search's reach on the shipped
library's steps (about half) says nothing about which candidate is closest [F2]. **Open question:** which
arithmetic the shipped library uses for the world-frame inertia, the gyroscopic term or the angular update.
Black-box experiments that separate them (a body with no torque and no angular velocity along two axes, a
sphere mass so that the inertia is a scalar, the gyroscopic term absent at zero angular velocity) need new
harness scenarios.

## 5. Collision and geometry

### 5.1 The ray's rotation padding (rays)

After `dGeomRaySet`, the ray's position and the nine meaningful entries of its rotation agree on both sides
for every ray, and the body carrying a ray is placed identically [R1]. The three padding entries of the 3×4
rotation returned by `dGeomGetRotation` (the fourth word of each row) differ for every ray: in the shipped
library they do not follow from the inputs (a handful of unrelated patterns repeat from ray to ray, as left
over from earlier work); in ours they are fixed. Nothing in the API reads them.

Required: nothing, beyond not relying on the padding words; a bit-exact comparison of rotations must skip
them. Whether anything in the game reads them is **unestablished** (believed not).

### 5.2 Ray against sphere (rays)

**Reproduced** for every ray of the scenario cast at the sphere, through `dCollideRaySphere` and through
`dCollide` alike [R2]. With `P` the ray's start relative to the sphere's centre and `D` its unit direction,
`B = P · D` and `P · P` are 3-term sums summed as **`(y + z) + x`** (the first component's product added
last); the rest (`C = P·P − r²`, `k = B² − C`, the square root, the distance `−B − √k`, falling back to
`−B + √k`, the hit point, the normal from the centre to the hit point flipped when the start is inside, and
its normalisation with the squared length in index order) as ours. Ours sums the two dot products in source
order, which changes the distance in about one hit in four and through it the point and the normal.

Required: in the ray–sphere test (and wherever the same routine serves, `library` 6.2 item 1: capped-cylinder
caps), the two dot products of the ray start are summed `(y + z) + x`.

### 5.3 Rotated boxes and capped cylinders (pairs, rays: open)

Every collision with a box or a capped cylinder in these two scenarios involves a geometry whose rotation
comes from `dRFromAxisAndAngle`, which the harness does not record. Results differ in between a fifth and two
thirds of the pairs for every kind with such a geometry (sphere–box, capped cylinder–box, two capped
cylinders, box–plane), in about two rays in a hundred against the box or the capped cylinder, and the contact
count differs for some box pairs [P0, R2]. Both sides return more contacts for two boxes than the one asked
for (`library` 6.2 item 2).

Our build's box–plane deepest contact is **reproduced** exactly by public 0.5.0's rotation (quaternion from
the axis, its length summed in index order, scaled by the sine of half the angle times the reciprocal square
root, then the quaternion-to-rotation conversion) and the corner sum in source order [P2]. The same rule
reproduces about four in five of the shipped library's deepest box–plane contacts, and the misses are
several units in the last place, more than one rounding of the corner sum. Variants of the axis
normalisation, the quaternion's grouping, a renormalised quaternion, a rotation built directly from the whole
angle, and every corner-sum order, did not reproduce it [P2, P3]. **Open question:** whether the shipped
`dRFromAxisAndAngle` (or the rotation setter) or the colliders themselves round differently; a harness
scenario that records the rotation `dRFromAxisAndAngle` returns, and the geometry's rotation read back after
`dGeomSetRotation`, would settle it. Until then the colliders' own arithmetic against boxes and capped
cylinders is **unestablished**, and the contact-count differences are believed (not shown) to follow from
rounding near the colliders' thresholds.

## 6. Constrained steps (joints, contacts, contacts-exact, heavy, worlds)

These scenarios differ from the first step under QuickStep [D1]. The first difference in the contacts
scenario is a body's vertical linear velocity at the first step, which fits section 4.1 (the weight goes
through the linear update); in heavy and in joints, angular quantities differ at the first step too
(section 4.3). The differences then grow through the solver's warm starting and the contacts that follow
from the positions; in contacts a pair later produces a different number of contacts as a consequence. The
exact stepper with contacts (contacts-exact) agrees for its first eight steps and then differs, in the
linear and angular velocity and the orientation of a body. **Open question:** which operation of the exact
stepper or the LCP solve rounds differently; its single-step inputs (the contact joints and the full body
state) can be recorded by the harness but the solve was not emulated. The QuickStep row solve's own
arithmetic is likewise **unestablished** beyond `library` 4.3; once sections 4.1 – 4.3 hold, a rerun of the
harness separates what remains.

## 7. Denormals-are-zero (#769)

**Yes: flush-to-zero and denormals-are-zero change the shipped library's results.** The shipped library's
joints run with them (the game's environment, `library` 7.2) and joints-nodaz without them differ from the
29th step on, only for the body held at a fixed pose by a fixed joint to the world and that joint's
feedback; contacts and contacts-nodaz are identical [Z1]. Ours gives the same results with and without, so
our build does not flush in either run [Z1].

The quaternion-to-rotation conversion follows the flush rule, **reproduced** for every body rotation of both
runs [Q2]: the held body's orientation has quaternion components so small that some of their products fall
below the smallest normal single-precision value; with the game's environment every such product becomes
zero (of the same sign) and any denormal input is read as zero, which reproduces every rotation of the joints
run, and plain rounding reproduces every rotation of joints-nodaz.

**Disputed in review:** the draft said this conversion is where flushing first matters and that the
difference spreads from there to the velocities and the joint's feedback. The runs first differ in the held
body's **angular velocity**, while its position, quaternion and rotation are still identical in both; the
rotation conversion depends on flushing only much later. The first effect of denormals-are-zero is in the
held body's angular velocity update, and the operation that flushes there is **unestablished**.

Required: our implementation flushes every denormal result of a single-precision arithmetic operation to a
zero of the same sign and reads every denormal operand as zero, in the integration and (believed) everywhere
the shipped library uses SSE arithmetic (`library` 7.1); the x87 square roots, sines and cosines are not
flushed (`library` 7.2, and the x87 scenario is identical). Established for the quaternion-to-rotation
conversion; **believed** for the angular velocity update. Only scenarios that reach denormal values show it.

## 8. Unestablished and open

- The angular velocity update's arithmetic in both steppers, ours included (section 4.3).
- The finite-rotation path of `dxStepBody` (section 4.2).
- `dRFromAxisAndAngle` and the colliders against boxes and capped cylinders, and the box–box contact-count
  differences (section 5.3).
- The QuickStep row solve and the exact stepper's LCP beyond the steps above (section 6).
- Which component `dNormalize3_slow` takes as largest when two magnitudes are equal (section 3).
- Whether anything reads the ray rotation's padding words (section 5.1).
- Which operation first produces a denormal in the held body's step (section 7).

## 9. Provenance

- Date: 2026-11-27. A Windows PC; S.T.A.L.K.E.R.: Shadow of Chernobyl (GOG), its
  `bin\ode.dll` (196,608 bytes) loaded unmodified by the harness, with the run time the library loads.
- Harness: release `odeharness-bb67eb79aa9a` (zip SHA-256 checked), run as its README and `docs/testing.md`
  say; exit code 0; compared with `odeharness.exe compare ours results`; rerun once with the same results.
- Analysis: only the API outputs the harness records, for its own synthetic inputs, compared with candidate
  single-precision arithmetic, by a dirty investigator on the PC; reviewed there by a fresh dirty reviewer.
  No debugger, tracing, memory inspection, disassembly or decompilation; nothing from the binary's code; no
  leaked or derived source.

## Gaps found in review

1. Section 7: the origin of the first denormal-dependent difference (the held body's angular velocity, then
   its linear velocity and its joint's feedback) is not explained; which operation produces a denormal there
   is **unestablished**. Believed, not checked: the world-frame inertia of a body whose orientation has very
   small quaternion components. A harness scenario (one such body, no joint, stepped once with and without
   denormals-are-zero, recording the angular velocity) would settle it.
2. The dirty workspace's check runner does not run every cited check (the angular-update searches, the
   single-call body step, the rotation candidates, the box–plane variants), and `spec-lint` was not run on
   the PC. Neither changes a finding.
