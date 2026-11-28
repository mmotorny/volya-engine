# Motion playback

Behaviour specification TC1 of the trader-and-crows scoping specification
(`docs/behavior/trader-and-crows-scope.md`, section 3), for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): how the engine plays the motions of an animated skeleton model over time. It covers, in full:
starting a cycle on one partition or on all of them; the values a playback runs with (speed, power, accrue,
falloff, the options of the flags); advancing a playback's time; what flags bits 1 – 3 do at a motion's end;
the notification at a motion's end; fading a new cycle in and the old ones out; how the playbacks on one bone
are blended into its pose (weights, the four channels and their factors); fx motions; the limits on the
number of playbacks; and worked examples on the trader's and the crow's motions. It is not split: one
review round can check it (section 9 reproduces every rule on data). Which motion an object plays, when,
and what it does when notified are the specs of those objects (trader: TC2, TC3; crow: TC5, TC7). What could
not be established is marked **unestablished**; what is inferred rather than read from the program or the
data is marked **believed**.

Builds on (not repeated):

- `docs/formats/skeletal-motions.md`: motion sets, motion definitions and their stored speed, power, accrue,
  falloff and flags (section 2), the quantization of those four values to 16-bit numbers *q* (section 2),
  motion keys and the key count *N* (section 3), motion names (4.2), partitions and the start bone of an fx
  motion (4.3), the evaluation of one motion at a time *t* into a rotation **q** and a translation **p** per
  bone (5.1 – 5.3), the local and current bone matrices (5.4). "Cycle" and "fx motion" are its terms (flags
  bit 0).
- `docs/behavior/script-api-engine-services.md` (spec B): `CKinematicsAnimated:PlayCycle(name)` as used by
  scripts, the cycle lookup by name and its failure (5.2); the device's `frame` and `time_global()` and the
  constant-step mode (3.2, 3.3).
- `docs/behavior/level-objects.md`: bones, parents, the root bone, skinning with the current matrices (4.4,
  4.8).

Conventions as in skeletal-motions: times in seconds unless stated, f32(*x*) is *x* rounded to single
precision, quaternions are (*x*, *y*, *z*, *w*). Bracketed ids ([P1] …) name the check that backs the
statement next to them: the functions of `checks.py` in the dirty workspace's `checks/motion-playback/`
(binary evidence exported by `export.py` into `ev/`, constants read from the shipped `bin\XR_3DA.exe`,
motion data decoded with the skeletal-motions decoder, and `pb.py`, a playback model written from this
text).

## 1. Terms

- A **playback** is one motion being played on a model: it has a **time** *t* (the time at which the motion
  is evaluated, skeletal-motions 5), a **weight** *a*, a **target** *P* (the weight a fade-in stops at), an
  **accrue** rate, a **falloff** rate, a **speed**, a **state** (fading in or fading out), whether it is
  **advancing**, the option **stop at end**, the option **fade at end**, a **channel** (0 … 3) and an
  optional **notification** (a function of the code that started it, with a value of that code's choosing)
  [P2, P3].
- A cycle's playback belongs to one partition (0 … 3) and is **applied** to the bones named by that
  partition's bone entries; an fx playback is applied to its start bone and every bone below it (all
  descendants) [P3, P11].
- Each bone keeps the list of playbacks applied to it, in the order they were applied [P3, P11].
- The **tracks update** of a model advances all its playbacks (section 4).

## 2. The values of a playback

### 2.1 From the definition

The quantized numbers *q* of skeletal-motions section 2 are turned back into values as follows [P1]:

| Value | Rule | Stored 1.0 | Stored 2.0 |
|---|---|---|---|
| speed, power | f32(*q* · f32(0.0015259022)) (= *q* / 655.35) | 0.99946594 | 1.99893188 |
| accrue, falloff | f32(*q* · f32(0.0022888533)) (= 1.5 · *q* / 655.35) | 1.49919891 | 2.99839783 |

<!-- spec-lint: constant -->

So accrue and falloff act at about 1.5 times their stored values. For a cycle whose *q*(falloff) was
lowered to *q*(accrue) − 1 (skeletal-motions 2), a stored falloff of 2.0 gives 2.99610901 [P1].

### 2.2 A cycle's playback

When a cycle is started from its definition (section 3), its playback takes [P2]:

- the partition: the definition's "bone or partition" (0xFFFF: every partition, 3.2), unless the starting
  code names a partition itself;
- speed, accrue and falloff from the definition (2.1); the definition's power is **not** used: the target
  *P* of a cycle is always 1;
- stop at end: flags bit 1 (value 2) of the definition;
- fade at end: stop at end and a channel other than 0;
- mixing (whether it fades in), the notification and the channel from the starting code.

The playback starts with *t* = 0, advancing, fading in, and weight *a* = f32(1e−7) when mixing or *a* = 1
otherwise [P2]. Its own falloff rate starts at 0; it is only given one when it is faded out (3.3) [P2].

## 3. Starting a cycle

### 3.1 On one partition

Starting a cycle on partition *i* [P3]:

1. If *i* is 4 or more, or the model has no partition *i* (the motion set defines fewer partitions: the
   partition has no name), nothing is started.
2. On channel 0 only: every playback of partition *i* on channel 0 is ended (3.3): faded out when the new
   one mixes, removed at once otherwise. Playbacks on other channels, and channel-0 playbacks of other
   partitions, are untouched. A playback started on channel 1 – 3 ends nothing.
3. The model's tracks update runs first (section 4; it does nothing when it has already run at the current
   `time_global()`) [P3].
4. The new playback (2.2) is applied to each bone of the partition, in the order of the partition's bone
   entries, and appended to the partition's playbacks [P3].

Starting the same motion again is not special: the old playback is ended as in step 2 and a new one starts
at *t* = 0 [P3].

### 3.2 On all partitions

A definition with 0xFFFF is started on partitions 0, 1, 2, 3 in turn, as four separate starts of 3.1 (those
the model lacks start nothing): each partition gets its own playback with the same values and the same
notification. The starting code is handed no playback back in this case (for one partition it is handed the
new playback, or none when step 1 started nothing) [P4].

### 3.3 Ending a playback: fade out or removal

- **Fade out:** the playback's state becomes fading out and its falloff rate becomes the **new** cycle's
  falloff (2.1); a playback with stop at end also stops advancing (its time stays where it is) [P5]. A
  playback already fading out gets the new rate too. A looping playback (no stop at end) keeps advancing
  while it fades.
- **Removal:** the playback is taken off every bone it was applied to and off its partition's list at once
  [P5].

### 3.4 Who starts cycles

- A script's `PlayCycle(name)` (spec B 5.2) starts the named cycle with the definition's partition, mixing,
  no notification, on channel 0 [P6].
- The game's own code starts cycles the same way, by name or by motion: the trader's global and head motions and
  the crow's states start their cycles with the definition's partition, mixing, on channel 0, with a
  notification where TC2, TC5 and TC7 say [P6]. Every start of a cycle found in the game library uses channel 0
  (**believed**: no start on channels 1 – 3 was found) [P6].

## 4. Advancing: the tracks update

### 4.1 When it runs

A model's tracks update runs when its bone matrices are requested (each request from drawing or game code)
and when a playback is started on it (3.1 step 3); some game code also runs it directly [P7]. Which requests
a particular object makes (in particular whether a model that is not drawn is updated) is the drawing's and
that object's spec: **unestablished** here.

An update does nothing when `time_global()` (spec B 3.3) equals the value at the model's previous update.
Otherwise the elapsed time is Δ = min(`time_global()` − previous value, 66) · 0.001 seconds (the difference
taken as an unsigned 32-bit number), and the previous value becomes `time_global()` [P7]. So:

- a model advances by at most 66 ms per update, whatever time has passed (a model not updated for a while
  resumes from where it was, 66 ms on);
- nothing advances while `time_global()` stands still (the game paused, spec B 3.3); in the constant-step
  mode an update advances by 20 ms for each frame since the previous one (at most 66 ms);
- the value before a model's first update is **unestablished** (**believed** 0, so that the first update
  advances by 66 ms) [P7].

Each cycle playback is advanced at most once per `frame` (it remembers the frame it was last advanced in)
[P7].

### 4.2 What it does to cycles

For each partition 0 … 3 that the model defines, for each of its playbacks in order [P8]:

1. If advancing: *t* += speed · Δ.
2. Fading in: *a* += accrue · *P* · Δ, then at most *P* (= 1). The playback stays in the fading-in state
   after reaching *P*: a cycle keeps full weight until it is ended (3.3).
3. Fading in, with stop at end, and *t* > *L* − f32(1/30), where *L* = f32(*N* · f32(1/30)) is the motion's
   length (skeletal-motions 5.1): this is the motion's **end** (section 5).
4. Fading out: *a* −= *P* · falloff · Δ; when *a* ≤ 0 the playback is removed (3.3).

A looping cycle (no stop at end) is never stopped: *t* grows without bound and the evaluation repeats the
motion (skeletal-motions 5.1, *F* mod *N*) [P8].

### 4.3 What it does to fx motions

After the cycles, each fx playback (section 6) is advanced when it is advancing (it always is): *t* +=
speed · Δ; fading in: *a* += accrue · *P* · speed · Δ, and once *a* ≥ *P* it becomes *P* and the playback
turns to fading out at once; fading out: *a* −= *P* · falloff · speed · Δ, and when *a* ≤ 0 it is taken off
its bones and ends [P9]. Fx rates include the speed, cycle rates do not. An fx playback has no frame check
and no end handling: its time may pass the motion's length, and the evaluation repeats the motion [P9].

## 5. A motion's end and the flags

### 5.1 The end of a cycle with stop at end

At the end found in 4.2 step 3 [P10]:

1. *t* is set to *L* − f32(1/30), i.e. (*N* − 1) / 30: the last key, exactly.
2. If the playback is still advancing and has a notification, the notification is called (5.2).
3. The playback stops advancing.
4. With fade at end (channel 1 – 3): it turns to fading out with falloff rate 2.0 (not scaled; it ends after
   0.5 s at *P* = 1). On channel 0 it keeps its weight and holds the last key until it is ended (3.3).

The test of step 3 of 4.2 is repeated at every update while the playback is fading in, but the
notification is called once: the playback no longer advances afterwards [P10]. A playback that is fading
out has no end: a stop-at-end cycle faded out before its end never notifies [P10].

So the end comes at the first update after which *t* exceeds (*N* − 1) / 30, about (*N* − 1) / 30 / speed
seconds of advancing after the start (section 9.2) [P10].

### 5.2 The notification

The notification is the starting code's function, called with the playback (whose values it may read)
during the tracks update in which the end is reached, before the update goes on to the next playback [P10].
Within one update, playbacks reach their ends in the order of 4.2: partitions 0 … 3, and within a partition
in the order they were started. A cycle started on all partitions (3.2) is notified once per partition, in
the same update when the partitions' playbacks run alike (they always do: same start, same values) [P4,
P10]. What the trader and the crow do when notified is TC2's and TC5's / TC7's.

### 5.3 What each flag bit does

| Bit (value) | Read by | Effect |
|---|---|---|
| 0 (1) | the engine at loading | fx motion instead of cycle (skeletal-motions 2) |
| 1 (2) | the engine, when a cycle is started | stop at end (5.1); with channel 1 – 3 also fade at end |
| 2 (4) | nothing found | none known: **unestablished** (**believed** unused; no reader of it was found in the engine or in the game library's reads of definitions) [P12] |
| 3 (8) | the game library only | none in the engine's playback; game code for the player's own body and for characters' AI motions compares it on the previous and the new motion and then carries the playback time from one to the other; that use belongs to those specs [P12] |

<!-- spec-lint: constant -->

In the data, bit 2 is set in 47 definitions (flags 4 and 6) and bit 3 in 986 (flags 8) (skeletal-motions 2)
[P12].

## 6. Fx motions

An fx motion is started from its definition with a power factor *k* given by the starting code [P11]:

- The fx motion is found by name among the model's fx motions, the last motion set first; not found: fatal
  error, a failed check with the message expression `motion_ID.valid()`, description
  `! MODEL: can't find FX: `, argument the name [P11].
- If the model already has 16 fx playbacks, nothing is started [P11].
- The start bone: the definition's "bone or partition", or the model's root bone for 0xFFFF
  (skeletal-motions 4.3). The playback is applied to that bone and then, depth first, to every bone below it
  [P11].
- Values: *t* = 0, *a* = f32(1e−7), fading in, advancing, channel 0, accrue, falloff and speed from the
  definition (2.1; an fx's falloff is not lowered), target *P* = power (2.1) · *k*; no notification, no stop
  at end [P11].
- Starting an fx ends nothing [P11].

So an fx motion rises for *P* / (accrue · *P* · speed) = 1 / (accrue · speed) seconds and falls for
1 / (falloff · speed) seconds, then ends (4.3). Because fx playbacks are on channel 0, they are blended with
the cycles of the same bones by weight (section 7), and their target *P* may exceed 1.

## 7. The pose of a bone

### 7.1 One channel

For each bone (a bone whose matrices are computed, 7.4), the playbacks applied to it are grouped by
channel, each group keeping the bone's order. Each playback gives a value: the rotation **q** and
translation **p** of its motion at its time *t* (skeletal-motions 5.1 – 5.3) [P13]. The values of one
channel with *n* playbacks are combined into one value by their weights *a* [P13]:

- *n* = 0: **q** = (0, 0, 0, 0), **p** = (0, 0, 0).
- *n* = 1: that playback's value unchanged: its weight plays no part (a single playback fading in from
  1e−7 shows fully at once).
- *n* = 2, values *v*₀, *v*₁ in the bone's order: *w* = *a*₁ / (*a*₀ + *a*₁) (0 when |*a*₀ + *a*₁| <
  f32(1e−7)), clamped to 0 … 1; the result is **interp**(*v*₀, *v*₁, *w*).
- *n* ≥ 3: the playbacks are ordered by decreasing weight (equal weights keep the bone's order); the result
  starts as the first value with sum *s* = its weight; for each next value *v* with weight *a*: *s*′ = *s* +
  *a*, *w* = *a* / *s*′ (0 when |*s*′| < f32(1e−7)), result = **interp**(result, *v*, *w*), *s* = *s*′.

**interp**(*v*ₐ, *v*_b, *w*) is the key interpolation of skeletal-motions 5.2 applied to the two rotations
with the weight *w* in place of *g* (same dot-product sign rule, the same ε, the same arccos polynomial,
**q**ₐ weighted by sin(θ − θ*w*) / sin θ, **q**_b by σ · sin(θ*w*) / sin θ; not normalised), and the
translations mixed linearly: **p** = (1 − *w*) · **p**ₐ + *w* · **p**_b [P13].

### 7.2 Channels 1 – 3

A channel other than 0 adds to channel 0 [P14]:

- Each of its playbacks' values is first made relative to that motion's key 0 on the bone: **q**′ = **q** ⊗
  conj(**q**₀), **p**′ = **p** − **p**₀, where **q**₀ is the bone's decoded rotation key 0 and **p**₀ the
  translation of key 0 (skeletal-motions 3), conj(*x*, *y*, *z*, *w*) = (−*x*, −*y*, −*z*, *w*); then the
  channel's values are combined as in 7.1.
- The channels are applied in the order 1, 2, 3, skipping channels with no playback on the bone: with *f*
  the channel's **factor** and (**r**, **d**) its combined value, **q** ← scale(**r**, *f*) ⊗ **q** and
  **p** ← **p** + *f* · **d**, starting from channel 0's value.
- scale(**r**, *f*): with *h* = arccos(*r*_w): if *h* ≤ f32(1e−5), the identity (0, 0, 0, 1); otherwise
  the rotation about the axis (*r*_x, *r*_y, *r*_z) / sin *h* by the angle 2*h* · *f*: (axis · sin(*h f*),
  cos(*h f*)), then normalised unless its length is below f32(1e−5).
- The product ⊗ is (*l* ⊗ *b*): *x* = *l*_w *b*_x + *l*_x *b*_w + *l*_y *b*_z − *l*_z *b*_y,
  *y* = *l*_w *b*_y + *l*_y *b*_w + *l*_z *b*_x − *l*_x *b*_z, *z* = *l*_w *b*_z + *l*_z *b*_w +
  *l*_x *b*_y − *l*_y *b*_x, *w* = *l*_w *b*_w − *l*_x *b*_x − *l*_y *b*_y − *l*_z *b*_z.
- The four channel factors of a model start at 1 and are set per channel by game code [P14]. Channel 0's
  factor is not used.
  **Disputed in review:** "set per channel by game code" is not backed: the checks show only that the
  engine offers a per-channel setter; no call of it was found in the game library (section 11 believes
  none), so which code sets the factors, if any, is unestablished.

### 7.3 The local matrix

The final (**q**, **p**) gives the bone's local matrix *L* by the table of skeletal-motions 5.4 (**q** not
normalised) and *T* = *L* · *T*(parent) [P13]. A bone with no playback at all has **q** = (0, 0, 0, 0) and
**p** = 0, so *L* is the identity: the bone sits at its parent's origin with its parent's orientation
(this answers the gap of skeletal-motions about bones a motion is not applied to) [P13].

### 7.4 When bone matrices are computed

A request for a model's bone matrices runs the tracks update (4.1) whenever `time_global()` has changed
since the model's previous request; the matrices themselves are recomputed only when the request insists or
at least 100 ms of `time_global()` have passed since they were last computed [P15]. A bone is computed only
while it is visible, and game code may attach a callback to a bone that replaces or follows its result
(skeletal-motions 5.5); neither is part of this spec [P13].
**Disputed in review:** the gate is the time the matrices were last computed, not the previous request: a
request does nothing at all (no tracks update, no recomputation, even when it insists) when `time_global()`
equals the time of the last computation; otherwise it runs the tracks update and recomputes when it insists
or at least 100 ms have passed since the last computation.

## 8. Limits

- A model has 256 playbacks in all (cycles and fx, of every partition and channel, including those fading
  out); starting one more is fatal: "Too many blended motions requisted" (sic) [P16].
- At most 16 fx playbacks at once (section 6) [P11].
- A bone holds at most 16 playbacks. When a 17th is applied: a playback with fade at end is not applied to
  that bone; any other replaces, on that bone only, the one with the smallest weight (the first of equal
  smallest), which keeps playing on its other bones and in its partition's list [P16].

## 9. Verification facts

Values come from `pb.py` (this text, in double precision with single-precision rounding where the text
says f32) on the motions decoded by the skeletal-motions decoder; positions are row 3 of *T* (model space)
and agree within 2e−4 with a single-precision implementation.
**Disputed in review:** the agreement with a single-precision implementation is not backed: the checks
contain no such implementation; the 2e−4 tolerance is only that of the checks against the values below.

### 9.1 The trader's and the crow's definitions

- `actors\trader_animation` (the trader's motions, two partitions `torso` and `head`): all 65 definitions
  have flags 2 (stop at end), stored power 1, accrue 2 and falloff 2; speed 1 except `talk_kind_0`,
  `talk_glad_0`, `talk_1`, `talk_0`, `talk_angry_0`, `talk_2`, `talk_3` with speed 2; 34 on all partitions,
  22 on `torso` (0), 9 on `head` (1) [P17]. Effective: speed 0.99946594 (or 1.99893188), accrue 2.99839783,
  falloff 2.99610901, target 1. With updates of 20 ms, a mixing start reaches full weight at its 17th update
  (1e−7 + 17 · 0.02 · 2.99839783 ≥ 1, then clamped to 1), and a playback it fades out from weight 1 is removed
  at the 17th update (1 − 17 · 0.02 · 2.99610901 < 0) [P18].
- `monsters\crow\crow` (one partition `default`): `norm_idle`, `idle`, `norm_death_idle`, `norm_fly_fwd`,
  `norm_idle_0` have flags 0 (looping); `norm_death_0` … `_2` and `norm_death_dead_0` … `_2` flags 2 (stop
  at end); `hit` flags 1 (fx, start bone 0, the root, stored power 2); all on partition 0, stored speed 1,
  accrue 2, falloff 2 [P17]. `hit`: effective power 1.99893188, falloff 2.99839783 (not lowered), rising
  for 0.33369 s and falling for 0.33369 s at speed 0.99946594 [P17].

### 9.2 Ends of the trader's motions

With one tracks update every 20 ms (constant-step mode) starting right after the start, a playback of speed
0.99946594 reaches its end at the first update *k* with *k* · 0.02 · 0.99946594 > (*N* − 1) / 30 [P18]:

| Motion | *N* | (*N* − 1) / 30 | End at update | Notifications |
|---|---|---|---|---|
| `idle_2` (all partitions) | 240 | 7.96667 | 399 (7.98 s) | 2 (partitions 0 and 1) |
| `spinka_idle` (all) | 50 | 1.63333 | 82 (1.64 s) | 2 |
| `vstuplenie` (`torso`) | 701 | 23.33333 | 1168 (23.36 s) | 1 |
| `vstuplenie_head` (`head`) | 701 | 23.33333 | 1168 (23.36 s) | 1 |
| `talk_0` (`head`, speed 1.99893188) | 30 | 0.96667 | 25 (0.50 s) | 1 |

### 9.3 A cross-fade on the trader

`actors\trader\trader` with `actors\trader_animation`; one update every 20 ms; `idle_2` started with mixing
(nothing else playing) right after update 0; `spinka_idle` started with mixing right after update 100.
Bone `bip01_head` [P19]:

| Update | Playbacks (per partition: time, weight, state) | `bip01_head` |
|---|---|---|
| 1 | `idle_2` 0.01999, 0.05997, in | (−0.0293, 1.1716, −0.3809) |
| 100 | `idle_2` 1.99893, 1, out; `spinka_idle` 0, 1e−7, in | (−0.0025, 1.1712, −0.3770) |
| 105 | `idle_2` 1.99893, 0.70039, out; `spinka_idle` 0.09995, 0.29984, in | (−0.0095, 1.1764, −0.3666) |
| 117 | `spinka_idle` 0.33982, 1, in (`idle_2` removed) | (−0.0142, 1.2143, −0.2443) |
| 120 | `spinka_idle` 0.39979, 1, in | (−0.0117, 1.2179, −0.2191) |
| 182 | `spinka_idle` 1.63333, 1, in, no longer advancing; notified for partitions 0 and 1 | (−0.0320, 1.1590, 0.0967) |

`idle_2` at update 1 shows fully although its weight is 0.06 (7.1, *n* = 1); at update 100 its time stopped
at 1.99893 (a stop-at-end playback faded out stops advancing, 3.3).

### 9.4 A cross-fade and an fx motion on the crow

`monsters\crow\crow`; one update every 20 ms; `norm_fly_fwd` started with mixing right after update 0,
`norm_idle` with mixing right after update 50, the fx `hit` (*k* = 1) right after update 80. Bones `root`
and `right_leg2` [P20]:

| Update | Playbacks (time, weight, state) | `root` | `right_leg2` |
|---|---|---|---|
| 25 | `norm_fly_fwd` 0.49973, 1, in | (0, 0.0274, 0) | (0.0380, 0.0047, −0.0537) |
| 55 | `norm_fly_fwd` 1.09941, 0.70039, out; `norm_idle` 0.09995, 0.29984, in | (0, 0.0217, 0) | (0.0346, −0.0016, −0.0537) |
| 70 | `norm_idle` 0.39979, 1, in | (0, 0, 0) | (0.0266, −0.0247, −0.0535) |
| 85 | `norm_idle` 0.69963, 1; `hit` 0.09995, 0.59904, in | (0, 0.0810, 0) | (0.0101, 0.1078, −0.0643) |
| 97 | `norm_idle` 0.93950, 1; `hit` 0.33982, 1.99893, out | (0, 0.1388, 0) | (−0.0011, 0.2030, −0.0370) |
| 113 | `norm_idle` 1.25933, 1; `hit` 0.65965, 0.08201, out | (0, 0.0068, 0) | (0.0235, −0.0092, −0.0595) |
| 114 | `norm_idle` 1.27932, 1 (`hit` ended) | (0, 0, 0) | (0.0266, −0.0247, −0.0535) |

`norm_fly_fwd` keeps advancing while it fades out (a looping playback, 3.3); its time 1.09941 is past its
length 0.8 and the evaluation repeats (4.2). Update 25 (time 0.49973) agrees to four decimals with skeletal-motions 6.2 at *t* = 0.5.

## 10. Not covered

- Which motions objects play and what they do when notified (TC2 – TC7 and the specs of other objects).
- Game code that changes a playback's values itself (time, speed, weight) after starting it, such as the
  carrying of time under flags bit 3 (5.3).
- The marks of version-4 motion sets (none shipped, skeletal-motions 8).
- Bone visibility and bone callbacks (skeletal-motions 5.5).
- Which requests for bone matrices drawing and game code make (4.1, 7.4).

## 11. Unestablished

- The value of a model's previous update time before its first update (4.1; **believed** 0).
- Which requests for bone matrices are made for a model that is not drawn, so whether its playbacks advance
  (4.1).
- The meaning of flags bit 2 (5.3; **believed** unused).
- Whether any shipped code starts a cycle on channels 1 – 3 or sets a channel factor (3.4; **believed** not).

## 12. Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (later archives win); binaries `bin\XR_3DA.exe` and `bin\xrGame.dll`.
- Data examined: `meshes\actors\trader_animation.omf`, `meshes\actors\trader\trader.ogf`,
  `meshes\monsters\crow\crow.ogf`; the approved specifications listed above.
- Analysis: static analysis of the shipped binaries (headless disassembly and decompilation with Ghidra) of
  the engine's starting of cycles and fx motions, fading, the tracks update, the per-bone blending and its
  channels, the bone-matrix request and its timing, the playback limits, and the device's frame and time
  values; in the game library, the starts of cycles of the trader, the crow and the script call, the readers
  of definition flags and the callers of the tracks update; constants read from the binary. No execution or
  debugging. A playback model written from this text (`pb.py`) reproduces section 9.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The value of a playback's last-advanced frame when it is started (starting does not set it; a playback
  reusing the place of an earlier one may carry that one's frame), so whether a playback started in a frame
  is advanced by an update later in the same frame.
- Playbacks started or ended by a notification during a tracks update: whether a playback started then is
  advanced or tested in the same update, and how the update goes on over the partition's changed list.
