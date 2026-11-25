# Particle motion actions

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of what each of the 16 actions of `particles.xr` that move particles does to the live
particles in one step: `move` (12), `gravity` (8), `gravitate` (7), `orbitpoint` (14), `vortex` (29), `randomaccel`
(15), `randomdisplace` (16), `randomvelocity` (17), `damping` (4), `speedlimit` (22), `jet` (9), `turbulence` (30),
`follow` (6), `scatter` (31), `bounce` (1) and `targetvelocity` (27). It is specification P3 of
`docs/behavior/particles-and-lights-scope.md`. What could not be established is marked **unestablished**; what is
inferred rather than read from the program or the data is marked **believed**.

Builds on (not repeated): `docs/formats/particles-xr.md` (P1: the action records, their field layouts of section
4.3 and their field names, the regions of section 5), `docs/behavior/particle-lifecycle.md` (P2: the particle and the
particle list of section 3, the generator of section 2, point generation and containment of a region, sections 4.1
and 4.2, the placement of each action's fields, section 5.2, playing, section 7, and the step, section 8.3, which
runs every action of the list in list order with the step time Δ = 0.033).

Conventions: as P2. A particle has position **x**, previous position **x**ₚ, velocity **w**ₚ and age *a* (P2 3.1);
*n* is the number of live particles and particles are visited in list order, *i* = 0 … *n* − 1. · between vectors
is the dot product, × the cross product, |**a**| the length; **a** *k* scales a vector. Field names are P1's
labels; "placed" values are the values after P2 5.2's placement, which a step always reads in place of the stored
ones. Arithmetic is IEEE single precision (`f32`); some square roots and quotients are formed in higher precision
and rounded once, so an `f32` implementation may differ in the last bits (the checks compare at a relative 10⁻⁵ –
10⁻³). **Disputed in review:** these operations are done on the x87 unit, and whether they are formed in higher
precision depends on its precision mode, which this text does not establish; `particle-appearance-actions.md` 1.2
believes the game runs with 24-bit precision, in which they are rounded to `f32` per operation (an implementation
rounding every operation to `f32` agreed with the shipped steps within the checks' tolerances). Every fact carries the name of the check that reproduces it, a script in `checks/particle-motion-actions/`
of the dirty workspace; `run_all.py` runs them all. `bin_setup.py` prepares instruction listings of the shipped
particle library; `emu.py` runs the library's own step routines on prepared particles by interpreting those
instructions; `ref.py` is a throwaway implementation written from this text; `compare.py` runs both on the same
input. `lib.py` decodes `particles.xr` from P1's text.

## 1. Common rules

- An action changes only the values its section names; every other value of every particle (including size,
  rotation, colour, frame values and, except for `move`, the previous position and the age) is left as it was
  (`steps.py`: all 18 words of each particle are compared).
- No action adds or removes particles. Each action visits the *n* particles live when it runs; the update runs
  every action whatever the count (`manager.py`; `follow` relies on *n* ≥ 1, 3.13).
- The result does not depend on the action flag word (P1 4.2) beyond its use in placement (P2 5.2) (`steps.py`:
  flag words 0, 1, 2, 3 and 0x80000003 give the same results).
- Δ is the step time (0.033, P2 8.3). Every action applies its full change in each step; no step changes any
  value of its action, except `turbulence`'s time (4.3), so nothing else is carried from one step to the next
  (`steps.py`).
- Placement (P2 5.2): `gravity`, `damping`, `follow`, `gravitate`, `move`, `speedlimit` and `turbulence` have no
  placed fields (their values are in world axes whatever the action flag bit 0x2); `targetvelocity`'s velocity
  and `vortex`'s axis are placed as directions; the centres of `orbitpoint`, `scatter`, `vortex` and `jet` as
  points; the regions of `randomaccel`, `randomdisplace`, `randomvelocity` and `jet` without translation, that of
  `bounce` in full (`placement.py`, which runs each type's placement on a test matrix and agrees with P2 5.2).

### 1.1 Distance terms

For the actions with a centre or a partner particle: **d** is the offset named in the action's section, *r*² =
|**d**|² (`f32`) and *r* = √(*r*²).

### 1.2 Maximum radius

`orbitpoint`, `scatter`, `follow`, `gravitate`, `jet` and `vortex` have a maximum radius *R*. Its square *R*² is
formed in `f32`. When *R*² ≥ 1.0000000 × 10¹⁶ (the `f32` value nearest 10¹⁶, 10,000,000,272,564,224; reached for
|*R*| ≥ 10⁸ as `f32`, e.g. the library's 1 × 10¹⁶ and 9.99999987 × 10¹⁴) every particle is affected. Otherwise a
particle is affected only when *r*² < *R*² (`vortex`: *r*² ≤ *R*²); at *r*² = *R*² the others leave it unchanged
(`edges.py`; `steps.py` covers both cases).

### 1.3 Region draws

`randomaccel`, `randomdisplace`, `randomvelocity` and `jet` draw a point **g** from their placed region (P2 4.1)
once for each particle they affect, when that particle is visited, in list order; each draw advances the shared
generator (P2 2.1) as P2 4.1 states for the region's kind (`steps.py`: the shipped steps and `ref.py` drawing in
that order from the same generator state agree for all 11 kinds). `jet` draws only for affected particles (3.7).

## 2. Motion by velocity: `move` (12)

For every particle (`steps.py`, `vectors.py`):

1. **x**ₚ ← **x**;
2. **x** ← **x** + **w**ₚ Δ;
3. *a* ← *a* + Δ.

Of the 16 actions, `move` is the only one that moves a particle by its velocity and the only one that ages it; in an
effect without `move` (6 effects, P1 4.1) they keep their age and move only by `randomdisplace` or `vortex` (or by
actions of other specifications). Where `move` stands in the list decides which velocity it uses: the actions before it in the list act on
the velocity first.

## 3. Forces and velocity changes

### 3.1 `gravity` (8)

**w**ₚ ← **w**ₚ + **g** Δ for every particle, **g** = the acceleration field (not placed, 1) (`steps.py`).

### 3.2 `targetvelocity` (27)

With *k* = scale · Δ and **t** = the placed velocity: **w**ₚ ← **w**ₚ + (**t** − **w**ₚ) *k* for every particle
(`steps.py`). *k* is not limited: for *k* > 1 the velocity passes the target (the library's scales are 0 … 28, so
*k* ≤ 0.924 at Δ = 0.033; 18 records have scale 0 and do nothing; `library.py`).

### 3.3 `damping` (4)

With the factors *f*ₓ, *f*ᵧ, *f*_z (the damping factor per axis) and the bounds *L* (lower) and *H* (upper):
for every particle with *L* ≤ |**w**ₚ|² ≤ *H* (both included), each component of **w**ₚ is multiplied by
1 − (1 − *f*) Δ with its axis' *f*; other particles are unchanged (`steps.py`, `edges.py`). The multiplier is not
limited; it is negative when (1 − *f*) Δ > 1. The library's factors are −20 … 1 (multipliers 0.307 … 1 at
Δ = 0.033; e.g. *f* = −10 gives 0.637) (`library.py`, `edges.py`).

### 3.4 `speedlimit` (22)

With *s* = |**w**ₚ|², the minimum speed *m* and the maximum speed *M* (`steps.py`, `edges.py`):

- if *s* < *m*² and *s* ≠ 0: **w**ₚ ← **w**ₚ · (*m* / √*s*);
- otherwise, if *s* > *M*²: **w**ₚ ← **w**ₚ · (*M* / √*s*);
- otherwise unchanged.

A particle at rest stays at rest. The minimum is tested first, so with *m* > *M* a slow particle ends at speed *m*.
A negative minimum is used as it is: a particle slower than |*m*| is given speed |*m*| in the opposite direction
(e.g. *m* = −1: (0.5, 0, 0) becomes (−1, 0, 0)); 14 of the library's 36 records have *m* < 0 (`library.py`).

### 3.5 `orbitpoint` (14)

**d** = **c** − **x** (**c** the placed centre). For each affected particle (1.2), with *μ* the magnitude and *ε*
the epsilon: **w**ₚ ← **w**ₚ + **d** · (*μ* Δ / (*ε* + *r*² + *r*)) (`steps.py`, `vectors.py`). The divisor is the
sum *ε* + *r*² + *r*, not a product. A particle at the centre is unchanged when *ε* ≠ 0, and its velocity becomes
not-a-number (NaN) when *ε* = 0 (`edges.py`; 3 library records have *ε* = 0, `library.py`).

### 3.6 `scatter` (31)

**d** = **x** − **c** (**c** the placed centre). For each affected particle: **w**ₚ ← **w**ₚ + (**d** / *r*) · (*μ* Δ /
(*ε* + *r*²)) (`steps.py`, `vectors.py`): a push away from the centre along the unit direction. A particle exactly
at the centre gets a NaN velocity (`edges.py`).

### 3.7 `jet` (9)

**d** = **x** − **c** (**c** the placed centre). For each affected particle: **g** = a point drawn from the placed
acceleration region (1.3), then **w**ₚ ← **w**ₚ + **g** · (*μ* Δ / (*r*² + *ε*)) (`steps.py`, `vectors.py`).
**d** gives only the distance, not the direction; the direction comes from the region. With a bounded radius only
affected particles draw a point; with an unbounded one every particle does.

### 3.8 `randomaccel` (15)

For every particle: **g** = a point drawn from the placed region (1.3); **w**ₚ ← **w**ₚ + **g** Δ (`steps.py`).

### 3.9 `randomdisplace` (16)

For every particle: **g** = a point drawn from the placed region; **x** ← **x** + **g** Δ (`steps.py`). The
displacement is scaled by Δ (a sphere of radius 0.3 moves a particle by at most 0.0099 per step); **x**ₚ is not
changed.

### 3.10 `randomvelocity` (17)

For every particle: **w**ₚ ← a point drawn from the placed region (`steps.py`). Not scaled by Δ.

### 3.11 `gravitate` (7)

Every pair of particles attracts each other. For *i* = 0 … *n* − 1 and, for each, *j* = *i* + 1 … *n* − 1 (in these
orders): **d** = **x**ⱼ − **x**ᵢ, *r*² = |**d**|² + 1.0000000 × 10⁻⁷ (the `f32` value of 10⁻⁷, 1.0000000116860974 ×
10⁻⁷, added before the radius test of 1.2), *r* = √(*r*²); if affected: *k* = *μ* Δ / ((*ε* + *r*²) · *r*),
**w**ₚᵢ ← **w**ₚᵢ + **d** *k*, **w**ₚⱼ ← **w**ₚⱼ − **d** *k* (`steps.py`, `vectors.py`). Positions are not changed, so
the order only affects rounding. The added term keeps coincident particles finite (`edges.py`). The work grows with
*n*² (*n* up to the effect's maximum, P1 2).

### 3.12 `follow` (6)

Each particle is drawn towards the next one in the list. For *i* = 0 … *n* − 2: **d** = **x**ᵢ₊₁ − **x**ᵢ; if
affected (1.2): **w**ₚᵢ ← **w**ₚᵢ + **d** · (*μ* Δ / ((*ε* + *r*²) · *r*)) (`steps.py`, `vectors.py`). The last
particle is not changed; with *n* = 1 nothing changes. Two coincident neighbours give the first a NaN velocity
whatever *ε* (`edges.py`).

### 3.13 `follow` with no particle

With *n* = 0 the visit does not stop at the count: it goes on past the end of the list (the range is taken as
0 … 2³² − 2), reading and writing memory beyond the live particles (`edges.py`: the shipped step, run with an empty
list, was still writing after 400,000 instructions). In the game this is believed to end in a crash or memory
corruption (**unestablished** which). It happens in a step where the effect has no live particle when `follow` runs;
the library's two effects with `follow` are `alexmx\fontan` and `explosions\old\ps_0002` (where `killold` runs
before it), and no other record of `particles.xr` names them (`library.py`). An implementation must decide what to
do here; doing nothing matches the case *n* = 1.

### 3.14 `vortex` (29)

Particles are turned about an axis; their velocity is not changed. With **c** the placed centre and **n** the
placed axis (used as it is, not normalised): **d** = **x** − **c**. For each particle with *R*² unbounded or
*r*² ≤ *R*² (1.2) (`steps.py`, `vectors.py`, `edges.py`):

1. **u** = **d** / *r*; *h* = **n** · **u**; **q** = **u** − **n** *h*;
2. θ = *μ* Δ / (*ε* + *r*²) (radians);
3. **x** ← **c** + (**q** cos θ + (**n** × **q**) sin θ + **n** *h*) · *r*.

For a unit axis this turns **x** by θ about the line through **c** along **n**, in the sense that takes **q**
towards **n** × **q** for positive θ (the library has *μ* from −21 to 333, `library.py`); the angle depends on the
distance *r* from the centre, not from the axis. **x**ₚ is not changed. A particle exactly at the centre gets
a NaN position (`edges.py`).

## 4. `turbulence` (30)

`turbulence` turns each particle's velocity by the gradient of a noise field, keeping its speed.

### 4.1 The noise *N*

**Tables.** The noise uses two tables, built once per run of the program, at the first noise evaluation (the first
step of any `turbulence`, its only user), and kept (`noise.py`). The build seeds the C runtime's generator (the second generator of
P2 2.2) with 1 and takes 1,571 values from it, so it also changes what that generator gives afterwards. That
generator advances a 32-bit state *t* ← *t* · 214013 + 2531011 (mod 2³²) and gives (*t* >> 16) & 0x7FFF (`crt.py`:
the particle library imports it from the shipped `bin\msvcr80.dll`, whose code holds these constants).

- Gradients *G*₀ … *G*₅₁₃ (3 `f32` each). For *k* = 0 … 255 in turn: take three values *v*ₓ, *v*ᵧ, *v*_z in that order
  and form **a** = ((*v* mod 512) − 256) / 256 per component; if |**a**|² > 1 discard them and take three more; then
  *G*ₖ = **a** · (1 / |**a**|). (481 triples are taken, 225 of them discarded.) Then *G*₂₅₆₊ₖ = *G*ₖ for
  *k* = 0 … 257.
- Indices *P*₀ … *P*₅₁₃ (integers). Start with *P*ₖ = *k* for *k* = 0 … 255 and *P*₂₅₆ = 0. For *k* = 256, 254, 252, …,
  2 (128 values, even only): take a value *v*, *j* = *v* mod 256, exchange *P*ₖ and *P*ⱼ. Then *P*₂₅₆₊ₖ = *P*ₖ for
  *k* = 0 … 257.

As built, *P*₀ … *P*₂₅₅ holds 0 twice (at 0 and 253) and lacks 253; *P*₀ … *P*₇ = 0, 1, 109, 3, 175, 5, 24, 7;
*G*₀ = (0.761481, −0.178833, 0.623030). MD5 of *P*₀ … *P*₅₁₃ as 514 little-endian `i32`:
`b636928c5f7a237aa0fbaf21637389f3`; of *G*₀ … *G*₅₁₃ as 1,542 little-endian `f32` (x, y, z per entry):
`44b86fb2ff90d4fa8903df93ef531f24` (`noise.py`).

**Value at a point (x, y, z).** For each coordinate *c* (`noise.py`, `ref.py`):

- *t* = *c* + 10000 rounded to `f32`; *b* = floor(*t*); *ρ* = *t* − *b* (0 ≤ *ρ* < 1); *b*₀ = *b* mod 256 (0 … 255,
  also for negative *b*), *b*₁ = (*b*₀ + 1) mod 256; *σ* = *ρ*² (3 − 2*ρ*).

Since *t* is an `f32` near 10,000, *ρ* moves in steps of 2⁻¹⁰ (≈ 0.001) for −1,808 ≤ *c* < 6,384 (coarser further
out): the noise is a fine staircase in each coordinate, which an implementation in wider precision does not
reproduce. With the corner values, for *i*, *j*, *k* ∈ {0, 1},

- *U*ᵢⱼₖ = *G*[*P*[*P*[*b*ₓ,ᵢ] + *b*ᵧ,ⱼ] + *b*_z,ₖ] · (*ρ*ₓ − *i*, *ρ*ᵧ − *j*, *ρ*_z − *k*),

where *b*ₓ,ᵢ is the x coordinate's *b*ᵢ, and with *L*(*s*, *a*, *b*) = *a* + *s* (*b* − *a*):

- *N* = 1.5 · *L*(*σ*_z, *L*(*σ*ᵧ, *L*(*σ*ₓ, *U*₀₀₀, *U*₁₀₀), *L*(*σ*ₓ, *U*₀₁₀, *U*₁₁₀)), *L*(*σ*ᵧ, *L*(*σ*ₓ, *U*₀₀₁,
  *U*₁₀₁), *L*(*σ*ₓ, *U*₀₁₁, *U*₁₁₁))).

*N* is 0 at whole-number points. Values (`noise.py`):

| Point | *N* |
|---|---|
| (0.3, 0.2, 0.1) | −0.0067847 |
| (0.5, 0.5, 0.5) | −0.1018033 |
| (1.25, −2.5, 3.75) | 0.3115477 |
| (10.1, 20.2, 30.3) | −0.3992659 |
| (−7.3, 0.6, 2.2) | 0.4517409 |
| (−10000.5, 0.25, 0.5) | 0.0762179 |
| (123.456, −78.9, 0.001) | −0.2198590 |

The rule holds for −2³¹ ≤ *t* < 2³¹ (checked on random points of −30,000 … 30,000 and at ±2³⁰, −2³¹); for *t* ≥ 2³¹,
*t* ≤ −2³², ±10²⁰ and infinity the program gives NaN; what it gives between −2³² and −2³¹ is **unestablished**
(`noise.py`).

### 4.2 The fractal sum *F*

For a point **q**, with the frequency *f*₀ and the octave count *K* (`steps.py`, `ref.py`):

*F*(**q**) = Σₖ₌₀^(*K*−1) *N*(*f*ₖ **q**) / *f*ₖ, with *f*ₖ₊₁ = *f*ₖ · 2.059 (the `f32` value 2.0590000).

*F* = 0 when *K* ≤ 0. The sample point *f*ₖ **q** (each component rounded to `f32`), each frequency and each partial
sum are `f32`.

### 4.3 The step

The action has a time τ of its own: it is not part of the record (reading the record leaves it unset); play sets it
to 0 (P2 7), and the step advances it (`manager.py`). In each step (`steps.py`, `manager.py`, `vectors.py`):

1. once, before the particles: τ ← τ + Δ (`f32`);
2. for every particle, with *o* the sampling offset, *μ* the magnitude and **δ** the drift:
   1. **q** = **x** + **δ** τ (per component, `f32`);
   2. *F*₀ = *F*(**q**), *F*ₓ = *F*(**q** + (*o*, 0, 0)), *F*ᵧ = *F*(**q** + (0, *o*, 0)), *F*_z = *F*(**q** + (0, 0, *o*))
      (the offset points rounded to `f32`);
   3. **e** = *μ* · (*f*₀ *F*ₓ − *f*₀ *F*₀, *f*₀ *F*ᵧ − *f*₀ *F*₀, *f*₀ *F*_z − *f*₀ *F*₀);
   4. *s* = |**w**ₚ| (before the change); **w**′ = **w**ₚ + **e**; **w**ₚ ← **w**′ · (*s* / |**w**′|).

The speed is kept and only the direction changes; the change is not scaled by Δ (it is per step). A particle at rest
stays at rest; when **w**′ = 0 (a particle at rest where **e** = 0, e.g. with *K* ≤ 0) the velocity becomes NaN
(`edges.py`). Positions, **x**ₚ and ages are not changed. The library's records have *K* = 1 … 4; 55 of 58 have a
non-zero drift, so their field moves with τ (`library.py`).

## 5. `bounce` (1)

`bounce` changes the velocity of particles that would cross its surface region in this step; positions are not
changed (the list's `move` then uses the new velocity). With **n** = the placed region's **p**₂ (not normalised),
*ψ* the tangential factor, *β* the resilience and *κ* the cutoff, a particle **bounces** with the normal **n**
when (`steps.py`, `vectors.py`, `edges.py`):

- *w*ₙ = **w**ₚ · **n**; **t** = **w**ₚ − **n** *w*ₙ; if |**t**|² > *κ*: **t** ← **t** *ψ*; **w**ₚ ← **t** − **n**
  (*w*ₙ *β*).

### 5.1 Flat kinds: plane (3), triangle (2), rectangle (10), disc (9)

With the offset *e* = *r*₁ (kinds 2, 3, 10) or *r*₃ (kind 9) of the placed region: *d*₀ = **n** · **x** + *e* and
*d*₁ = **n** · (**x** + **w**ₚ Δ) + *e*. Only particles with *d*₀ · *d*₁ < 0 can bounce (a particle starting or
ending exactly on the plane does not). Then:

- plane: it bounces;
- otherwise, the crossing point relative to **p**₁ is **h** = **x** + **w**ₚ τ′ − **p**₁ with τ′ = −*d*₀ / *w*ₙ
  (*w*ₙ = **w**ₚ · **n**);
  - triangle: with **c** = **u** × **v**, α = **h** · (**v** × **c**) / |**c**|² and γ = **h** · (**c** × **u**) /
    |**c**|² (the coordinates of **h** along **u** and **v**), it bounces when α ≥ 0, γ ≥ 0 and α + γ ≤ 1;
  - rectangle: with α, γ as for the triangle, it bounces when 0 ≤ α ≤ 1 and 0 ≤ γ ≤ 1;
  - disc: it bounces when *r*₂² ≤ |**h**|² ≤ *r*₁².

### 5.2 Sphere (5)

With **x**′ = **x** + **w**ₚ Δ: only particles with **x**′ inside the placed region (P2 4.2: *r*₄ ≤ |**x**′ −
**p**₁|² ≤ *r*₃) can bounce. Let **m** = **x** − **p**₁, divided by its length when |**m**|² > FLT_MIN; *w*ₘ =
**w**ₚ · **m**; **t** = **w**ₚ − **m** *w*ₘ:

- **x** not inside (the particle enters): if |**t**|² > *κ*: **t** ← **t** *ψ*; **w**ₚ ← **t** − **m** (*w*ₘ *β*);
- **x** inside and *w*ₘ < 0 (moving towards the centre): **w**ₚ ← **t** − **m** *w*ₘ (the radial part reversed,
  without resilience or tangential factor);
- **x** inside and *w*ₘ ≥ 0: unchanged.

### 5.3 Other kinds

For a region of kind 0, 1, 4, 6, 7, 8 or above 10, `bounce` does nothing (`steps.py`). The library's 15 records use
kinds 2 (1), 3 (1), 5 (6), 9 (6) and 10 (1) (`library.py`).

## 6. Verification

### 6.1 Test vectors

(`vectors.py`: each case is run on the shipped step and with `ref.py`; both give the values below to a relative
10⁻⁵.) The standard particles, as (position, velocity, age): 0: ((1, 2, 3), (0.5, −1, 2), 0.25); 1: ((−2, 0.5, 1),
(3, 0, −1), 1); 2: ((0.5, −1, −0.5), (0, 0, 0), 0). The bounce particles B: 0: ((0.5, 0.2, 0.5), (1, −1, 0));
1: ((1.8, 0.1, 0.5), (0, −2, 0)); 2: ((0.1, 0.3, 0.1), (0, −1, 0)). The sphere-bounce particles C: 0: ((1.2, 0, 0),
(−2, 0.5, 0)); 1: ((0, 0.5, 0), (0, −1, 0)); 2: ((0, 0, 0.95), (0, 0, 1)). Placement is the identity; Δ = 0.033
except where stated; the generator state (P2 2.1) before the step is 1 except where stated. Regions are given as
kind, then the fields used: "point **p**" is kind 0 with **p**₁ = **p**; "sphere 0.3" is kind 5 with **p**₁ = 0,
*r*₁ 0.3, *r*₂ 0, *r*₃ 0.09, *r*₄ 0.

| Case | Values |
|---|---|
| move | — |
| gravity | acceleration (0, −5, 0) |
| gravitate | magnitude 3, epsilon 0.001, maximum radius 7 |
| orbitpoint | centre (0, 0, 0), magnitude 1, epsilon 0.001, maximum radius 10¹⁶ |
| orbitpoint, bounded | centre (0, 3, 0), magnitude 12, epsilon 0.01, maximum radius 3.4 |
| vortex | centre (0, 3, 0), axis (0, 1, 0), magnitude 70, epsilon 50, maximum radius 170 |
| vortex, bounded | centre (0, 0, 0), axis (0.70710677, 0, 0.70710677), magnitude 21, epsilon 0.001, maximum radius 3 |
| randomaccel, point | point (0, 4, 0) |
| randomaccel, sphere | sphere 0.3, generator state 12345 |
| randomdisplace, sphere | sphere 0.3, generator state 12345 |
| randomvelocity, point | point (1, 2, 3) |
| damping | factors (−10, −10, −10), lower bound 1, upper bound 10³² |
| speedlimit | minimum −1, maximum 2 |
| speedlimit, minimum | minimum 5, maximum 8 |
| jet | centre (0, 2, 0), region point (0, 1, 0), magnitude 1, epsilon 0.001, maximum radius 3 |
| turbulence | frequency 1, octaves 2, magnitude 4, offset 0.01, drift (1, 1, 1), τ 0 before the step (0.033 after) |
| turbulence, 4 octaves | frequency 0.5, octaves 4, magnitude 50, offset 0.37, drift (1, −3, 1), τ 2 before the step (2.033 after) |
| follow | magnitude 1, epsilon 0.001, maximum radius 10¹⁶ |
| scatter | centre (0, 0, 0), magnitude 2000, epsilon 22, maximum radius 3 |
| targetvelocity | velocity (0, 1, 0), scale 5 |
| bounce, plane | particles B, Δ 0.5; plane **p**₁ 0, **p**₂ (0, 1, 0), *r*₁ 0; tangential 1, resilience 0.45, cutoff 1 |
| bounce, disc | particles B, Δ 0.5; disc **p**₁ 0, **p**₂ (0, 1, 0), *r*₁ 1, *r*₂ 0.25, *r*₃ 0; tangential 0.8, resilience 0.7, cutoff 0 |
| bounce, triangle | particles B, Δ 0.5; triangle **p**₁ 0, **p**₂ (0, 1, 0), **u** (0, 0, 2), **v** (2, 0, 0), *r*₁ 0; tangential 1.1, resilience 0.3, cutoff 0 |
| bounce, rectangle | as the triangle, kind 10 |
| bounce, sphere | particles C, Δ 0.5; sphere **p**₁ 0, *r*₁ 1, *r*₂ 0, *r*₃ 1, *r*₄ 0; tangential 0.5, resilience 0.5, cutoff 0 |

Results (position, velocity); for `move` the previous positions are the old positions and the ages become 0.283,
1.033, 0.033:

| Case | Particle | Position | Velocity |
|---|---|---|---|
| move | 0 | (1.0165, 1.967, 3.066) | (0.5, -1, 2) |
| move | 1 | (-1.901, 0.5, 0.967) | (3, 0, -1) |
| move | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| gravity | 0 | (1, 2, 3) | (0.5, -1.165, 2) |
| gravity | 1 | (-2, 0.5, 1) | (3, -0.165, -1) |
| gravity | 2 | (0.5, -1, -0.5) | (0, -0.165, 0) |
| gravitate | 0 | (1, 2, 3) | (0.494517, -1.00547, 1.9932) |
| gravitate | 1 | (-2, 0.5, 1) | (3.01201, -0.00171942, -1.00089) |
| gravitate | 2 | (0.5, -1, -0.5) | (-0.00652487, 0.00719188, 0.00768839) |
| orbitpoint | 0 | (1, 2, 3) | (0.49814, -1.00372, 1.99442) |
| orbitpoint | 1 | (-2, 0.5, 1) | (3.00875, -0.00218767, -1.00438) |
| orbitpoint | 2 | (0.5, -1, -0.5) | (-0.00605339, 0.0121068, 0.00605339) |
| orbitpoint, bounded | 0 | (1, 2, 3) | (0.472359, -0.972359, 1.91708) |
| orbitpoint, bounded | 1 | (-2, 0.5, 1) | (3.05419, 0.0677428, -1.0271) |
| orbitpoint, bounded | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| vortex | 0 | (1.11286, 2, 2.95999) | (0.5, -1, 2) |
| vortex | 1 | (-1.96087, 0.5, 1.0747) | (3, 0, -1) |
| vortex | 2 | (0.482333, -1, -0.517063) | (0, 0, 0) |
| vortex, bounded | 0 | (1, 2, 3) | (0.5, -1, 2) |
| vortex, bounded | 1 | (-2.03348, 0.216503, 1.03348) | (3, 0, -1) |
| vortex, bounded | 2 | (0.76264, -0.58031, -0.76264) | (0, 0, 0) |
| randomaccel, point | 0 | (1, 2, 3) | (0.5, -0.868, 2) |
| randomaccel, point | 1 | (-2, 0.5, 1) | (3, 0.132, -1) |
| randomaccel, point | 2 | (0.5, -1, -0.5) | (0, 0.132, 0) |
| randomaccel, sphere | 0 | (1, 2, 3) | (0.504773, -0.99859, 1.99554) |
| randomaccel, sphere | 1 | (-2, 0.5, 1) | (3.00108, 0.000681665, -1.00102) |
| randomaccel, sphere | 2 | (0.5, -1, -0.5) | (-0.000437152, 0.00215446, 0.00389386) |
| randomdisplace, sphere | 0 | (1.00477, 2.00141, 2.99554) | (0.5, -1, 2) |
| randomdisplace, sphere | 1 | (-1.99892, 0.500682, 0.998975) | (3, 0, -1) |
| randomdisplace, sphere | 2 | (0.499563, -0.997846, -0.496106) | (0, 0, 0) |
| randomvelocity, point | 0 | (1, 2, 3) | (1, 2, 3) |
| randomvelocity, point | 1 | (-2, 0.5, 1) | (1, 2, 3) |
| randomvelocity, point | 2 | (0.5, -1, -0.5) | (1, 2, 3) |
| damping | 0 | (1, 2, 3) | (0.3185, -0.637, 1.274) |
| damping | 1 | (-2, 0.5, 1) | (1.911, 0, -0.637) |
| damping | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| speedlimit | 0 | (1, 2, 3) | (0.436436, -0.872872, 1.74574) |
| speedlimit | 1 | (-2, 0.5, 1) | (1.89737, 0, -0.632456) |
| speedlimit | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| speedlimit, minimum | 0 | (1, 2, 3) | (1.09109, -2.18218, 4.36436) |
| speedlimit, minimum | 1 | (-2, 0.5, 1) | (4.74342, 0, -1.58114) |
| speedlimit, minimum | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| jet | 0 | (1, 2, 3) | (0.5, -1, 2) |
| jet | 1 | (-2, 0.5, 1) | (3, 0.0045511, -1) |
| jet | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| turbulence | 0 | (1, 2, 3) | (0.542374, -1.02518, 1.97606) |
| turbulence | 1 | (-2, 0.5, 1) | (3.01198, 0.034908, -0.962697) |
| turbulence | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| turbulence, 4 octaves | 0 | (1, 2, 3) | (1.01284, -1.37607, -1.52662) |
| turbulence, 4 octaves | 1 | (-2, 0.5, 1) | (2.76068, 0.979295, -1.19147) |
| turbulence, 4 octaves | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| follow | 0 | (1, 2, 3) | (0.498338, -1.00083, 1.99889) |
| follow | 1 | (-2, 0.5, 1) | (3.00234, -0.00140427, -1.0014) |
| follow | 2 | (0.5, -1, -0.5) | (0, 0, 0) |
| scatter | 0 | (1, 2, 3) | (0.5, -1, 2) |
| scatter | 1 | (-2, 0.5, 1) | (0.885889, 0.528528, 0.0570555) |
| scatter | 2 | (0.5, -1, -0.5) | (1.14657, -2.29314, -1.14657) |
| targetvelocity | 0 | (1, 2, 3) | (0.4175, -0.67, 1.67) |
| targetvelocity | 1 | (-2, 0.5, 1) | (2.505, 0.165, -0.835) |
| targetvelocity | 2 | (0.5, -1, -0.5) | (0, 0.165, 0) |
| bounce, plane | 0 | (0.5, 0.2, 0.5) | (1, 0.45, 0) |
| bounce, plane | 1 | (1.8, 0.1, 0.5) | (0, 0.9, 0) |
| bounce, plane | 2 | (0.1, 0.3, 0.1) | (0, 0.45, 0) |
| bounce, disc | 0 | (0.5, 0.2, 0.5) | (0.8, 0.7, 0) |
| bounce, disc | 1 | (1.8, 0.1, 0.5) | (0, -2, 0) |
| bounce, disc | 2 | (0.1, 0.3, 0.1) | (0, -1, 0) |
| bounce, triangle | 0 | (0.5, 0.2, 0.5) | (1.1, 0.3, 0) |
| bounce, triangle | 1 | (1.8, 0.1, 0.5) | (0, -2, 0) |
| bounce, triangle | 2 | (0.1, 0.3, 0.1) | (0, 0.3, 0) |
| bounce, rectangle | 0 | (0.5, 0.2, 0.5) | (1.1, 0.3, 0) |
| bounce, rectangle | 1 | (1.8, 0.1, 0.5) | (0, 0.6, 0) |
| bounce, rectangle | 2 | (0.1, 0.3, 0.1) | (0, 0.3, 0) |
| bounce, sphere | 0 | (1.2, 0, 0) | (1, 0.25, 0) |
| bounce, sphere | 1 | (0, 0.5, 0) | (0, 1, 0) |
| bounce, sphere | 2 | (0, 0, 0.95) | (0, 0, 1) |

Boundary cases (`edges.py`): a particle at distance exactly the maximum radius is changed by `vortex` only;
`speedlimit` with minimum −1 turns (0.5, 0, 0) into (−1, 0, 0) and leaves (3, 0, 0); with minimum 5 and maximum 3,
(0, 0, 4) becomes (0, 0, 5); `damping` with bounds 4 … 16 changes speeds² 4 and 16 but not 3.99 and 16.01; a
particle at (0, 0.4, 0) with velocity (1, −1, 0) and the plane of the plane case with tangential 1, resilience 0.5
gets (1, 0.5, 0), with tangential 0.3 and cutoff 0.5 gets (0.3, 0.5, 0).

### 6.2 Facts on `particles.xr`

(`library.py`, decoding with P1's text.) The library holds 2,409 records of the 16 types (counts per type: P1
4.1). Every particle is affected (1.2) in 152 of 174 `orbitpoint` records, 11 of 17 `gravitate`, 2 of 15
`scatter`, 1 of 2 `follow`, and in no `jet` or `vortex` record. `speedlimit` has a negative minimum in 14 records;
`vortex` magnitudes include −21, −12, −10 and −7; epsilon is 0 in 3 `orbitpoint` and 9 `vortex` records; the
`damping` factors are −20, −18, −17, −10, −8, −5, −2 and 1; `turbulence` octave counts are 1 (22 records), 2 (24),
3 (2) and 4 (10); 217 `gravity` records have action flag bit 0x2, which changes nothing for them (1). `follow`
stands in `alexmx\fontan` (list `source`, `gravity`, `bounce`, `sink`, `move`, `bounce`, `follow`, `jet`, `scatter`,
`scatter`, `targetsize`, `killold`) and `explosions\old\ps_0002` (`source`, `killold`, `targetcolor`, `vortex`,
`targetsize`, `targetrotate`, `gravity`, `follow`, `move`).

### 6.3 Every record of the library

(`library.py`.) Each of the 2,409 records is applied once, as stored, to the standard particles of 6.1 (fresh for
each record), with Δ = 0.033, generator state 1 before each record and, for `turbulence`, τ = 0 before the step. Per
type, the sums over all its records and the three particles of the resulting position and velocity components
(no record gives a NaN):

| Type | Records | Sum of positions (x, y, z) | Sum of velocities (x, y, z) |
|---|---|---|---|
| `move` | 1070 | -411.415, 1569.69, 3780.31 | 3745, -1070, 1070 |
| `gravity` | 310 | -155, 465, 1085 | 1085.2, -402.612, 308.362 |
| `gravitate` | 17 | -8.5, 25.5, 59.5 | 59.5, -17, 17 |
| `orbitpoint` | 174 | -87, 261, 609 | 616.622, -122.616, 153.888 |
| `vortex` | 133 | -55.4706, 199.412, 468.249 | 465.5, -133, 133 |
| `randomaccel` | 35 | -17.5, 52.5, 122.5 | 116.719, -32.334, 35.2177 |
| `randomdisplace` | 33 | -24.91, 51.3686, 116.893 | 115.5, -33, 33 |
| `randomvelocity` | 2 | -1, 3, 7 | 10.6791, 10.6791, 10.6791 |
| `damping` | 14 | -7, 21, 49 | 34.0675, -10.271, 10.898 |
| `speedlimit` | 36 | -18, 54, 126 | 147.148, -50.8238, 61.0689 |
| `jet` | 1 | -0.5, 1.5, 3.5 | 3.5, -0.998635, 1 |
| `turbulence` | 58 | -29, 87, 203 | 173.373, -51.8733, 47.1742 |
| `follow` | 2 | -1, 3, 7 | 7.00068, -2.00224, 1.99749 |
| `scatter` | 15 | -7.5, 22.5, 52.5 | 64.3555, -95.8072, 7.38167 |
| `bounce` | 15 | -7.5, 22.5, 52.5 | 47.1667, -13.6667, 17.6667 |
| `targetvelocity` | 494 | -247, 741, 1729 | 1470.66, -239.603, 522.283 |

### 6.4 The first level's effect

`tests\muhi` (named by the first level's `level.ps_static`, P1 8) has the motion records `orbitpoint`,
`orbitpoint`, `randomdisplace`, `vortex` and `move` (list positions 3, 4, 5, 6, 8). Applied in that order, as
stored, to the standard particles in one step (Δ = 0.033, generator state 1 at the start, drawn on through the
step), they give (position, previous position, velocity, age) (`library.py`):

| Effect | Particle | Position | Previous position | Velocity | Age |
|---|---|---|---|---|---|
| muhi | 0 | (1.12492, 1.9676, 3.01896) | (1.10853, 2.00086, 2.95334) | (0.496622, -1.00806, 1.98856) | 0.283 |
| muhi | 1 | (-1.86881, 0.498834, 1.04605) | (-1.96839, 0.49897, 1.07928) | (3.01769, -0.00413191, -1.0071) | 1.033 |
| muhi | 2 | (0.476914, -0.995208, -0.511129) | (0.477187, -0.995973, -0.511621) | (-0.00827028, 0.0231912, 0.0149209) | 0.033 |

## 7. Unestablished

- What the game does when `follow` runs with no live particle (believed a crash or memory corruption) (3.13).
- The noise's value for −2³² < *t* < −2³¹ (4.1).
- `turbulence`'s time τ before the effect is first played (it is never read then, since only playing effects are
  updated, P2 8.2; believed not observable).

## 8. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): `particles.xr` from `gamedata.db4` (1,394,071
  bytes, CRC-32 0x4E93C6F1); binaries `bin\xrParticles.dll` (MD5 `22bad4d258e6211c6ea7cc8dc8d7ca55`) and
  `bin\msvcr80.dll` (MD5 `e4fece18310e23b1d8fee993e35e7a6f`).
- Data examined: every action record of the 16 types in `particles.xr`, decoded by a throwaway parser written from
  P1's text (`lib.py`).
- Analysis: static analysis (Ghidra 12.2, headless) of the particle library: the step of each of the 16 action
  types, their placement, the noise and its tables, and how the library runs, plays and reads action lists; the C runtime's exported generator (static). The library's own instructions for these steps were then run
  by an interpreter of their instruction listings on prepared inputs (`emu.py`), and every rule of this text was
  reproduced by a throwaway implementation written from it (`ref.py`), compared on random and boundary inputs and on
  every record of the library. No execution or debugging of the game itself.
- No leaked source code, derived projects or third-party documentation of the engine were used. The checks in
  `checks/particle-motion-actions/` reproduce every fact.

## Gaps found in review

- The effect of the SSE flush-to-zero and denormals-are-zero modes (`particle-appearance-actions.md` 1.2) on these steps is not stated (believed: results smaller than 2⁻¹²⁶ in magnitude become 0).
- Whether the game ever plays `alexmx\fontan` or `explosions\old\ps_0002` (named only in `particles.xr`), and so whether the case of 3.13 can arise, is not stated.
