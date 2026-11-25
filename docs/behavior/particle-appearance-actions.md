# Particle lifetime and appearance actions

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of what the five action types
`killold` (10), `sink` (19), `targetcolor` (23), `targetsize` (24) and `targetrotate` (25) of `particles.xr` do to
an effect's particles in one update step. It is specification P4 of `docs/behavior/particles-and-lights-scope.md`.
What could not be established is marked **unestablished**; what is inferred rather than shown by the data or the
program is marked **believed**.

Builds on (not repeated): `docs/formats/particles-xr.md` (P1: the action records, their fields and section numbers
cited as "P1 4.3" etc.); `particle-lifecycle.md` (P2: the effect's particles and their attributes, the update step
and its time *dt*, the order in which an effect's actions run, the placement of an effect, the containment test of
the region kinds, the particle death notification); `particle-motion-actions.md` (P3: the motion actions). How a particle's
age advances is P2's and P3's; `killold` compares the age as it is when `killold` runs. Attribute names below (position, size,
rotation, colour, alpha, age) are P2's.

Conventions: `f32` is IEEE single precision. "Rounded" means rounded to the nearest `f32`, ties to even. ⌊*x*⌋ is
the largest integer not above *x*. Vectors are written (x, y, z). Every fact carries the name of the check that
reproduces it, a script in `checks/particle-appearance-actions/` of the dirty workspace; `run_all.py` runs them all
(`bin_setup.py` first makes the instruction listings the `bin_*.py` and `emu_*.py` checks read). The `emu_*.py`
checks run the shipped program's own instructions for each action in an instruction interpreter (`emu.py`) on
random and chosen particles, and compare the result, bit for bit, with a model written from this text (`lib.py`,
functions `model_*` and `to_byte`).

## 1. Common rules

### 1.1 Running

- Each of the five actions changes the particles of its effect when P2's update step runs it (P1 4.2: every action
  of a list, in list order); sections 2 – 7 state what one run does (`bin_routines.py`, `emu_*.py`).
- The step time *dt* is used by `targetcolor`, `targetsize` and `targetrotate` only; `killold` and `sink` do not
  depend on it (`emu_killold.py`, `emu_sink.py`).
- None of the five reads the action flag word (P1 4.2): the results are the same with flag words 0, 0x2, 0x3 and
  0xFFFFFFFF (`emu_*.py`).
- Placing the effect (P2) changes nothing for `killold`, `targetcolor`, `targetsize` and `targetrotate`. For `sink`,
  placing turns the stored region into the region that is tested; the test always uses the region as placed
  (`bin_routines.py`). When an effect has never been placed the placed region equals the stored one (the stored
  region is copied into it when the record is read; `bin_routines.py`).
- Type id 26 runs exactly as `targetrotate` (P1 4.4; `bin_routines.py`).

### 1.2 Arithmetic

Each operation below is done in `f32` and rounded, in the order written, unless a step says otherwise
(`emu_*.py`). The program does the `targetsize` step and part of the `targetcolor` step with single-precision SSE
instructions, and the `targetrotate` step and the rest of `targetcolor` on the x87 unit (`emu_targetsize.py`,
`bin_routines.py`). The x87 results equal per-operation `f32` rounding when the x87 unit runs with 24-bit
precision; the game's core library sets that mode (24-bit precision, round to nearest) at start-up, and the first
renderer switches back to it after its own 64-bit computations (`bin_fpu.py`). That the mode is in effect when the
actions run is **believed** (the thread that runs them was not examined). With 64-bit precision, `targetrotate`
would differ in the last bit in about two thirds of random cases, `targetcolor` in none of 2,400
(`emu_targetrotate.py`, `emu_targetcolor.py`). The core library's thread start-up also sets the SSE
flush-to-zero mode, and denormals-are-zero where the processor supports it (`bin_fpu.py`): results smaller than
2⁻¹²⁶ in magnitude are **believed** to become 0. The test vectors of section 8 avoid such values.

## 2. Removing particles (`killold` and `sink`)

Both actions remove the particles that meet their test (sections 3 and 4), in this way (`emu_killold.py`,
`emu_sink.py`, `vectors.py`):

1. The particles are visited from the last (index *n* − 1) to the first (index 0), each once.
2. A visited particle that meets the test is first reported to P2's death notification, with its index and its
   attributes as they are.
3. Then the last particle of the list (all its attributes) is moved into the removed particle's place, and the
   count drops by one. When the removed particle is the last one, nothing is moved.

The particle moved into place was visited already, so it is not tested again in the same step. The other
particles keep their order. Example (`vectors.py`, V1): ages 0.5, 2.0, 3.0, 1.0, 2.5 (indices 0 – 4) with a test
"age ≥ 2" give the notices (index 4, age 2.5), (2, 3.0), (1, 2.0), and the list afterwards is the particles that
had index 0 and 3, in that order.

## 3. `killold` (10)

Fields (P1 4.3): age limit *L*, kill-younger switch *S*.

A particle is "younger" when *L* > its age; when *L* or the age is NaN it is not younger, and a particle whose age
equals *L* is not younger. The particle is removed when *S* = 1 and it is younger, or when *S* = 0 and it is not
younger. With any other value of *S* no particle is removed (`emu_killold.py`).

So with *S* = 0 (1,069 of the library's 1,070 records) a particle is removed once its age reaches *L*; with *S* =
1 every particle younger than *L* is removed (`data.py`).

## 4. `sink` (19)

Fields (P1 4.3): kill-inside switch *S*, region.

A particle is "inside" when its position is inside the placed region by P2's containment test (P2 states which
kinds can contain a point; kind 8, the Gaussian cloud, answers at random). The particle is removed when *S* = 1 and
it is inside, or when *S* = 0 and it is not inside. With any other value of *S* no particle is removed
(`emu_sink.py`). Consequently, with *S* = 0 and a region kind that never contains a point, every particle is removed
in the first step (`emu_sink.py`). The library uses kinds 3 (plane), 4 (box), 5 (sphere), 6 (cylinder) and 7 (cone)
in `sink` (P1 5.2; section 9).

## 5. `targetcolor` (23)

Fields (P1 4.3): colour (*c*ₓ, *c*ᵧ, *c*_z), alpha *a*, scale *s*.

### 5.1 Channels

A particle's colour and alpha are four channels, each an integer 0 – 255 (P2). The target of the red channel is
*c*ₓ, of green *c*ᵧ, of blue *c*_z, of alpha *a* (`emu_targetcolor.py`).

### 5.2 Step

*k* = *s* × *dt*, rounded once per step. For each particle and each of its four channels with integer value *b* and
target *t*:

1. *x* = *b* × *q*, rounded, where *q* is the `f32` nearest to 1/255 (bit pattern 0x3B808081, 0.003921568859…);
2. *y* = *x* + (*t* − *x*) × *k*, with the subtraction, the product and the sum each rounded in that order;
3. *z* = *y* × 255, rounded;
4. the channel's new value is *z* converted by 5.3.

The four channels are computed from the values they had before the step (`emu_targetcolor.py`). *k* is not
limited: with *k* > 1 a channel overshoots its target and the conversion clamps it (`vectors.py`, V3b).

### 5.3 Conversion of *z* to a channel value

For |*z*| < 2³¹ the value is ⌊*z*⌋, then limited to 0 … 255: below 0 gives 0, above 255 gives 255 (so −0 and any
negative *z* give 0, 255.99 gives 255) (`emu_convert.py`).

For |*z*| ≥ 2³¹, infinities and NaNs (unreachable with the library's values unless *k* exceeds about 4.7 × 10⁵, `data.py`), let
*e* be the biased exponent of *z* (bits 23 – 30 of its bit pattern), *m* its fraction (bits 0 – 22) and *h* =
(158 − *e*) mod 32 (`emu_convert.py`, checked over every exponent and both signs):

- *z* positive (sign bit clear): 0 when *h* = 0; otherwise the smaller of 255 and ⌊(2³¹ + 256 *m*) / 2ʰ⌋. For
  example, 2³¹ ≤ *z* < 2³² gives 0; +∞ and a NaN with the sign bit clear give 1.
- *z* negative (sign bit set): 255 when *h* = 0 and *m* ≠ 0; 0 otherwise (among them −2³¹ exactly, −∞).

## 6. `targetsize` (24)

Fields (P1 4.3): size (*t*ₓ, *t*ᵧ, *t*_z), scale (*s*ₓ, *s*ᵧ, *s*_z).

For each axis *i*: *k*ᵢ = *s*ᵢ × *dt*, rounded once per step; then for each particle, with size component *r*ᵢ:
*r*ᵢ ← *r*ᵢ + (*t*ᵢ − *r*ᵢ) × *k*ᵢ, with the subtraction, the product and the sum each rounded in that order
(`emu_targetsize.py`). The step uses only single-precision SSE instructions, so it does not depend on the x87 mode
(`emu_targetsize.py`). Negative and overshooting values are kept as computed; nothing is clamped.

## 7. `targetrotate` (25)

Fields (P1 4.3): rotation (*t*ₓ, *t*ᵧ, *t*_z), scale *s*.

### 7.1 Step

*k* = *s* × *dt*, rounded once per step. For each particle, with rotation (*r*ₓ, *r*ᵧ, *r*_z):

1. the factor *f* is *k* when *r*ₓ ≥ 0 (including −0), and −*k* when *r*ₓ < 0 or *r*ₓ is NaN;
2. for each component *i* (x, y and z, all with the same *f*): *r*ᵢ ← *r*ᵢ + (|*t*ᵢ| − |*r*ᵢ|) × *f*, with the
   subtraction, the product and the sum each rounded in that order (section 1.2 on the x87 mode).

(`emu_targetrotate.py`.)

### 7.2 Consequences

- The x component moves toward |*t*ₓ| when it is not negative and toward −|*t*ₓ| when it is negative: the sign of the
  target's x is ignored, and *r*ₓ keeps its sign while it approaches the target's magnitude (for *k* ≤ 1)
  (`emu_targetrotate.py`).
- The y and z components use the factor chosen by the sign of *r*ₓ, not by their own sign. For example, with
  target (0, 0, 0) and *k* = 0.5, rotation (1, −1, 0) becomes (0.5, −1.5, 0) and (−1, −1, 2) becomes
  (−0.5, −0.5, 3) (`emu_targetrotate.py`). In the library the target's y and z are 0 in 507 of the 543 records
  (section 9).

## 8. Test vectors

Computed by the model of this text and by the shipped instructions, which agree bit for bit (`vectors.py`; the
expected values are in `vectors_expected.json`). Particles not mentioned have zero attributes except as stated;
*dt* is given per vector. Hexadecimal values of sizes and rotations are `f32` bit patterns; colours are written as
the four channels alpha, red, green, blue in hexadecimal, e.g. 80 FF 40 00.

- V1 `killold`, *L* = 2, ages 0.5, 2.0, 3.0, 1.0, 2.5 (indices 0 – 4):
  - *S* = 0: notices (4, 2.5), (2, 3.0), (1, 2.0); afterwards the former particles 0, 3 (ages 0.5, 1.0).
  - *S* = 1: notices (3, 1.0), (0, 0.5); afterwards the former particles 4, 1, 2 (ages 2.5, 2.0, 3.0).
- V2 `sink`, sphere (kind 5) with **p**₁ (0, 0, 0), *r*₁ 1, *r*₂ 0, *r*₃ 1, *r*₄ 0; positions (0, 0, 0), (2, 0, 0),
  (0.5, 0.5, 0.5), (1, 0, 0), (0, −3, 0) (indices 0 – 4):
  - *S* = 1: notices for indices 3, 2, 0; afterwards the former particles 4, 1.
  - *S* = 0: notices for indices 4, 1; afterwards the former particles 0, 3, 2.
- V3a `targetcolor`, colour (0.5, 0.5, 0.5), *a* 1, *s* 2, *dt* 0.05: 80 FF 40 00 → 8C F2 46 0C;
  00 00 00 00 → 19 0C 0C 0C; FF FF FF FF → FF F2 F2 F2.
- V3b `targetcolor`, colour (1, 0, 0.25), *a* 18, *s* 22, *dt* 0.05 (*k* = 1.1, overshoot): the same three
  particles → FF FF 00 46, FF FF 00 46, FF FF 00 2C.
- V3c `targetcolor`, colour (1, 0.5, 0), *a* 0, *s* 3, *dt* 0.1, six steps from FF 00 00 00: B2 4C 26 00,
  7C 81 40 00, 56 A6 53 00, 3C C0 60 00, 2A D2 69 00, 1D DF 6F 00.
- V4 `targetsize`, size (1, 2, 0.5), scale (1, 2, 3), *dt* 0.02: size (0.01, 0.01, 0) → (3CF41F21, 3DB78034,
  3CF5C28F) ≈ (0.0298, 0.0896, 0.03); size (3, −1, 200) → (403D70A4, BF6147AE, 433C07AE) ≈ (2.96, −0.88, 188.03).
- V5 `targetrotate` with the values of `tests\muhi`'s record (rotation (−11.092285, 0, 0) as stored, *s* 0.03; P1
  section 8), *dt* 0.02, three steps; rotations after each step:

| Start | Step 1 | Step 2 | Step 3 |
|---|---|---|---|
| (100, 0, 0) | (42C7E4B0, 0, 0) | (42C7C964, 0, 0) | (42C7AE1C, 0, 0) ≈ (99.840057, 0, 0) |
| (−100, 0, 0) | (C2C7E4B0, 0, 0) | (C2C7C964, 0, 0) | (C2C7AE1C, 0, 0) ≈ (−99.840057, 0, 0) |
| (5, −2, 3) | (40A01DF2, C00013A9, 403FE282) | (40A03BDF, C0002755, 403FC509) | (40A059C8, C0003B04, 403FA794) ≈ (5.0109596, −2.0036020, 2.9946032) |
| (−5, −2, 3) | (C0A01DF2, BFFFD8AE, 40401D7E) | (C0A03BDF, BFFFB162, 40403B00) | (C0A059C8, BFFF8A1C, 40405887) ≈ (−5.0109596, −1.9964023, 3.0054033) |

## 9. The library's records

In `particles.xr` (`data.py`; decoded as P1 describes):

| Action | Records | Effects with two | Action flag words |
|---|---|---|---|
| `killold` | 1,070 | 1 | 0 in all |
| `sink` | 89 | 0 | 0x2 (77), 0x3 (7), 0 (5) |
| `targetcolor` | 976 | 0 | 0 in all |
| `targetsize` | 747 | 8 | 0 in all |
| `targetrotate` | 543 | 1 | 0 in all |

- `killold`: age limits 0.01 … 100,000; kill-younger 0 in 1,069 records, 1 in the one of
  `ghoul\fx-11-anomaly_wild-red-lights`.
- `sink`: (kill-inside, region kind): (1, plane) 29, (0, plane) 22, (1, sphere) 16, (1, box) 10, (0, cylinder) 6,
  (0, cone) 6.
- `targetcolor`: colour components 0 … 1; alpha 0 … 18 (above 1 in 3 records); scale 0 … 22 (0 in 4).
- `targetsize`: size components 0 … 200 (z at most 150); scale components 0 … 100, all three 0 in 5 records.
- `targetrotate`: x of the rotation −163.598 … 174.533, negative in 185 records; y or z non-zero in 36; scale
  0 … 30 (0 in 7).

## 10. Unestablished

- Whether the 24-bit x87 mode is in effect on the thread that runs the actions (believed; 1.2), and so the last bit
  of `targetrotate` results and of the green, blue and alpha arithmetic of `targetcolor`.
- The effect of the SSE flush-to-zero and denormals-are-zero modes on these steps (believed: tiny results become 0;
  1.2).
- Which NaN (sign) a `targetcolor` step produces from NaN inputs, and so the channel value it then gets (5.3 gives
  the value for each sign).

## 11. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); `particles.xr` from `gamedata.db4`;
  binaries `bin\xrParticles.dll` (MD5 `22bad4d258e6211c6ea7cc8dc8d7ca55`), `bin\xrRender_R1.dll` (MD5
  `2bf95c8376b735daa6d82b8944894be0`), `bin\xrCore.dll` (MD5 `eab71f90f01575eca3500e623873e57c`).
- Data examined: every action record of the five types in `particles.xr`, decoded by the P1 checks' throwaway
  decoder written from `docs/formats/particles-xr.md`.
- Analysis: static analysis (Ghidra 12.2, headless) of the particle library's per-step, placement and load
  routines of the five actions, its particle copy and containment test; of the core library's floating-point mode
  set-up and the first renderer's mode switch; execution of the particle library's own instructions for each
  action in an instruction interpreter written for these checks, on random and chosen inputs, compared with a
  model written from this text. No debugging of the running game.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/particle-appearance-actions/` reproduce every fact.

## Gaps found in review

- The text does not say outright that each action changes only the attributes it names (`killold` and `sink` only remove particles; `targetcolor` only the colour, `targetsize` only the size, `targetrotate` only the rotation) and leaves every other attribute, and for the three target actions the particle count, unchanged; the program's routines read and write only those.
- The 24-bit x87 precision mode limits only the significand, not the exponent range (processor documentation): an intermediate of the x87 parts of `targetcolor` and `targetrotate` that would overflow or be subnormal in `f32` stays finite or normal until it is stored, so section 1.2's equivalence with per-operation `f32` rounding holds only while intermediates stay in the normal `f32` range (always so with the library's values).
- `docs/formats/particles-xr.md` 4.3 describes the `killold` kill-younger switch as "0 or not"; section 3 here shows that only 0 and 1 act and any other value removes nothing, and particles-xr.md does not say so.
