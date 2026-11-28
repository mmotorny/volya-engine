# Climbing ladders and its sounds

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of S12 of
`docs/behavior/level-sound-scope.md`: whether climbing a ladder makes a sound, and which. Because the answer
depends on the climbing movement and no approved specification covers it (`player-movement.md` section 10
leaves it out), this document also specifies the climbing movement itself: the ladders of the level, how the
player gets on a ladder, moves on it and gets off, and what climbing changes in the player's physics, movement
state and camera. What could not be established is marked **unestablished**; what is inferred rather than
shown by the program or the data is marked **believed**.

Builds on (not repeated here):

- `docs/behavior/player-movement.md` (cited as `movement`): the movement request and state and the state's
  *climbing* flag (section 3), the run rule (4.1), crouch (4.2), sprint (4.3), jump (4.4), the acceleration
  `a` and its factor `climb_coef` (section 5), the character physics: the physics step, gravity 19.62 m/s²,
  the accepted acceleration `a_k`, the moving test `|a_k| > 0.1`, the driving and side forces, the speed cap,
  the airborne state, the take-off, falling and landing (sections 6.1 – 6.7), the camera yaw and its
  limits (section 8).
- `docs/formats/level-cform.md` (cited as `level-cform`): the materials and their indices (section 3), the
  world's contact parameters (section 4.2), impact speed and ground material (section 4.4), the player's
  body: body centre, radius `r`, mass (section 5.1), the contact rules and the traction `T` (section 5.4).
- `docs/behavior/player-footsteps.md` (cited as `footsteps`): the leg cycle chosen while climbing (section
  2.1), the step timing and the step sound (sections 3, 4).
- `docs/behavior/player-cycle-lengths.md` (S10): the cycles of the player's model.
- `docs/formats/material-pairs.md` (S9): the pair of two materials and its lists.
- `docs/behavior/sound-positional.md` (S2) and the other sound specifications: how a sound plays (nothing
  here plays one).
- `docs/formats/sav.md` (section 7: the records of `spawns\all.spawn`, the Object and Shape blocks),
  `docs/behavior/offline-simulation.md` (a record's level, sections 5.1, 6.1; switching online, 6.5.2 –
  6.5.3), `docs/behavior/object-visual-and-transform.md` (kinds without a model, section 3; the object
  transform, section 4), `docs/behavior/world-coordinates.md` (world axes, +y up, section 1; the camera's yaw,
  pitch and view direction, sections 2 – 3).

Conventions:

- Units: metres, seconds, kilograms, radians. Vectors are world `(x, y, z)`. `u·v` is the dot product, `|v|`
  the length, `v̂ = v / |v|` (for `|v| < 0.0000001`, `v̂` is the zero vector).
- **P** is the player's body centre (`level-cform` section 5.1: the player's position, its feet, plus
  `(0, r, 0)`); **r** = 0.35 is the body radius there (0 while the player has no body); **m** = 80 kg the
  body mass; **g** = 19.62 m/s² the gravity of `movement` section 6.1.
- **a** is the acceleration the body holds, `a_k` of `movement` section 6.1; "input" means `a` is not zero.
- **c** is the first-person camera's view direction, `(cos p · sin y, −sin p, cos p · cos y)` with yaw `y`
  and pitch `p` (world-coordinates sections 2, 3; positive pitch looks down), so `c.y = −sin p`.
- `hcos(u, v)` is the cosine of the horizontal angle between `u` and `v`:
  `(u.x·v.x + u.z·v.z) / sqrt((u.x² + u.z²)·(v.x² + v.z²))`; a test on it fails when the denominator is 0.
- "Step" is one step of the physics simulation (`movement` section 6.1); "frame" one rendered frame.
- "The clock" is the step clock of `footsteps` section 3.1 (milliseconds, unsigned).

**Checks.** Every claim carries a tag. The checks are scripts in the dirty workspace, `checks/climbing/`:
`extract.py` extracts the game data into `x/`, `export.py` regenerates the binary evidence in `ev/`
(headless Ghidra exports of `bin\xrGame.dll`); `run_all.py` runs `check_data.py` (tags D1 – D10: the game
data), `check_bin.py` (B1 – B32: the program) and `model.py` (M1 – M8: an implementation of sections 1.2, 2
and 3 written from this text, run on the records).

## 1. Ladders

### 1.1 Records

- Ladders are the records of `spawns\all.spawn` whose section is `climable_object`, the only section of
  class `O_CLMBL`; that section has no key other than `class` [D1]. There are 232 such records in all
  levels and 14 on `l01_escape` (others: `l04_darkvalley` 43, `l12_stancia_2` 28, `l03_agroprom` 25,
  `l05_bar` 20, `l10_radar` 20, `l06_rostok` 19, `l07_military` 17, `l08_yantar` 17, `l08u_brainlab` 7,
  `l03u_agr_underground` 6, `l12_stancia` 5, `l12u_sarcofag` 5, `l10u_bunker` 4, `l11_pripyat` 2) [D1].
- Each holds the Object block and the Shape block (`sav.md` 7.4); in every one the Shape block has exactly
  one shape, a box (type 1), whose four rows have all elements zero except element 0 of row 0, element 1
  of row 1 and element 2 of row 2; custom data is empty and the state flags are 0xFFFFFFBF [D2].
- A ladder never switches offline and, with state flag 0x02 set, switches online regardless of distance
  (`offline-simulation` sections 6.5.2, 6.5.3), so all ladders of the level are present from the first frame [D2].
  A ladder draws nothing (`object-visual-and-transform` section 3).

### 1.2 The ladder's frame

When a ladder comes online the game derives its frame from the record [B1 – B4]:

1. `R0`, `R1`, `R2` are the rows of the record's rotation and `p` its position (`object-visual-and-transform`
   section 4). The **extents** `e0, e1, e2` are element 0 of row 0, element 1 of row 1 and element 2 of row 2 of
   the first shape's box; nothing else of the shape is used [B1]. (In the shipped data they are the box's
   half-sizes along `R0`, `R1`, `R2`; section 5.2 shows the lower ends lying on the ground.)
2. The rows are given three roles, **axis** (along the ladder), **side** (across it) and **normal**
   (through it), by comparing the extents [B2]:

   | Condition | axis | side | normal |
   |---|---|---|---|
   | `e0 < e1` and `e2 ≤ e0` | `R1` | `R0` | `R2` |
   | `e0 < e1` and `e0 < e2 ≤ e1` | `R1` | `R2` | `R0` |
   | `e0 < e1` and `e2 > e1` | `R2` | `R1` | `R0` |
   | `e0 ≥ e1` and `e2 ≤ e1` | `R0` | `R1` | `R2` |
   | `e0 ≥ e1` and `e1 < e2 < e0` | `R0` | `R2` | `R1` |
   | `e0 ≥ e1` and `e2 ≥ e0` | `R2` | `R0` | `R1` |

   With `e_a`, `e_s`, `e_n` the extents of the rows chosen: if `e_n < 0.2` (strictly), `e_n` becomes 0.2
   and the centre is moved back along the normal row: **C** = `p − 0.2 · R_n`; otherwise **C** = `p`
   [B2]. (The front face, through `C + N`, then passes through the record's position `p`.)
3. **A** = `R_a · e_a`, **S** = `R_s · e_s`, **N** = `R_n · e_n`. If `A.y < 0` (strictly), **A** and **S**
   are both negated; **N** is never negated [B3]. `Â`, `Ŝ`, `N̂` are their directions.
4. The **lower front point** is **B** = `C − A + N` and the **upper front point** **T** = `C + A + N`. The
   **front** of a ladder is the side **N** points to [B5, B6].

The ladder's collision body is one box centred at **C** with the record's rotation (its axes `R0`, `R1`,
`R2`) and half-sizes `e0, e1, e2` as modified in step 2, each raised to at least 0.005; its material is
`materials\fake_ladders` (id 82, index 47; flags 0x00000C20; friction 1.0) [B4, D3]. The ladder's
radius (the largest extent, before step 2) is kept with it; its use was not traced (believed: the ladder's
bounding sphere, section 2.4) [B4].

### 1.3 What collides with a ladder

A contact between the ladder's box and another body is kept only when the other body is of the character
kind (the player's body is; believed: every character's) **and** that character is in front of the ladder by the
predicate *front*(−0.1) of section 2.1 (using that character's body centre and radius); every other contact
with the ladder's box is dropped [B5]. So objects pass through ladders, and a character is held off a
ladder's front face but passes through it from behind or once it is close to the ladder's axis.

## 2. The climbing state

### 2.1 Quantities and predicates

For a ladder (section 1.2) and the player's body centre **P**:

- **Q** = `C + Â · (Â·(P − C))`, the point of the ladder's axis line nearest to **P**; `d_axis = |Q − P|`;
  `û` the direction of `Q − P` [B6].
- `δ = N̂·(Q − P)` (negative when **P** is in front); **v_n** = `N̂ · δ` [B6].
- `lat = |Ŝ·(Q − P)|`; `h_low = Â·(P − B)`, the height of **P** above the lower end along the axis;
  `h_top = Â·(T − P)`, the distance from **P** up to the upper end [B6].
- `d_B = |B − P|`, `d_T = |T − P|`; `ŵ_B`, `ŵ_T` the directions from **P** to **B** and to **T** [B6].

Predicates [B6]:

- *front*(`f`): `−δ > |N| + r/2 + f`.
- *in range*: `|δ| < |N| + r + 0.05` and `lat < |S|` and `h_low > −0.2` and `h_top + r > 0`.
- *facing*(`v`): `hcos(c, v) > 0.9238795` (the camera's heading within 22.5° of `v`'s).

The **intent** `k` [B7]: with `γ = a·v_n` (positive when the input pushes towards the ladder),
`k = (c.y + 0.34202015) · γ` if `γ > 0.001`, else `k = γ`. So pushing towards the ladder means up while the
camera looks less than 20° below the horizontal (`c.y > −sin 20°`) and down when it looks lower; no input
(`γ = 0`) or input away from the ladder means down.

### 2.2 States

The player's body keeps one ladder (or none) and one of seven **climbing states** [B8]:

| State | Meaning used here |
|---|---|
| free | no ladder (the state at creation) |
| near | a ladder was found near the player |
| at the top | near the ladder's upper end, not climbing |
| at the bottom | near the ladder's lower end, not climbing |
| climbing up | on the ladder, climbing up |
| climbing down | on the ladder, climbing down (also hanging still) |
| leaving | getting off the ladder sideways or by a jump |

The player **is climbing** in *climbing up* and *climbing down* [B8, B23].

### 2.3 Changing state

A change of state to `X` is tried; it is **refused** when, since the last accepted change, the body centre has
moved no more than `d_min` and the clock has advanced by no more than `t_min` milliseconds (unsigned), where
`(d_min, t_min)` is (2.0 m, 3,000 ms) for the change *leaving* → *free* and (0, 0) for every other change (so
another change is refused only at the same position in the same millisecond) [B9]. On an accepted change [B9]:

- entering *climbing up* or *climbing down* from any other state switches gravity **off** for the body;
  leaving them for any other state switches it **on** again (between the two, it stays off);
- the clock value and the body centre are recorded (for the next refusal test), and the state becomes `X`.

### 2.4 Finding a ladder

The physics world tells a ladder about each character whose physics object it finds near its own (believed:
bounding volumes overlapping; the test was not traced) [B10]. For the player, with that ladder `L` [B11]:

- nothing happens if `L` is already the player's ladder, or if `d_axis` for `L` is more than 1.5 m, or if the
  player has a ladder and `d_axis` for `L` is larger than for that ladder;
- otherwise a change to *near* is tried (section 2.3), and `L` becomes the player's ladder whether or not the
  change was accepted.

### 2.5 Each step

At the start of every physics step, before the forces of section 3, if the player has a ladder the rules of its
state are applied once, in this order (each "→ X" is a change tried as in section 2.3; later rules of the same
step see the state as it then is, but stay those of the state the step began in) [B12 – B18]:

- **free**: the ladder is dropped [B12].
- **near** [B13]:
  1. if *front*(0) and *in range* and *facing*(**v_n**): → *climbing up* if `k > 0`, else → *climbing down*
     (if that change is refused, nothing else happens in this step);
  2. otherwise, if `d_T > d_B`: → *at the bottom* if `d_B < r + 0.3`; if `d_T ≤ d_B`: → *at the top* if
     `d_T < r + 0.3`.
- **at the top** [B14]:
  1. if *in range* and `c.y < −0.15707964` (looking down by more than about 9°) and `|δ| > r/3` and
     *front*(0.1): → *climbing down*;
  2. if `d_T − r > 1.5`: → *free*.
- **at the bottom** [B15]:
  1. if *in range* and *facing*(`ŵ_B`) and `a·ŵ_B > 0` and `k > 0` and *front*(0): → *climbing up*;
  2. if `d_B − r > 1.5`: → *free*.
- **climbing up** [B16]:
  1. if `k < 0` and *front*(0): → *climbing down*;
  2. if `d_axis ≥ 0.0000001` and `|a| ≥ 0.0000001` and `|N̂·â| < 0.70710677` (input more than 45° away from
     the ladder's normal, e.g. sideways): → *leaving*;
  3. if `h_top + r < 0.1`: → *at the top*;
  4. the common rule below.
- **climbing down** [B17]:
  1. if `k > 0` and *front*(0): → *climbing up*;
  2. as rule 2 of *climbing up*: → *leaving*;
  3. if `h_low − r < 0.1`: → *at the bottom*;
  4. the common rule below;
  5. if `h_top < −r`: → *free*;
  6. if the body's velocity has `v.y > 0.0000001`, a force `(0, −m·g, 0)` acts in this step.
- **common rule** of both climbing states [B16, B17]: if `d_axis − r > 1.5`: → *free*. Then, if
  `|a| < 0.0000001` and `N·û < 0`, a force `û · m·g` (towards the ladder's axis) acts in this step.
- **leaving** [B18]: if `d_T > d_B`: → *at the bottom* if `d_B < r + 0.3`; if `d_T ≤ d_B`: → *at the top* if
  `d_T < r + 0.3`. Then → *free* (refused within 2 m and 3 s of entering *leaving*, and refused in the same
  step after a change made just before).

Consequences: a player standing in front of a ladder, within its sides and close to its face (body centre
between about 0.375 and 0.6 m in front of the axis for a thin ladder), and facing it, takes it in a step after
the one that found it; with
the camera level and pushing towards it they climb up, with no input or the camera low they hang and climb
down. Sideways input lets go. At the top end the state becomes *at the top* and the ordinary movement takes
over; looking down there takes the ladder again downwards. Section 5.3 gives worked cases.

## 3. Movement while climbing

### 3.1 Physics step

In each step in which the player is climbing [B19]:

- gravity does not act on the body (section 2.3);
- the traction `T` used by the movement forces (`level-cform` section 5.4, `movement` 6.2) is 1, whatever
  the contacts;
- if the body was moving (`|a_k| > 0.1`) in the previous step and is not now, its velocity is set to zero;
- the body is not airborne: the airborne state ends on the first climbing step and is not entered while
  climbing (`movement` section 6.5).

### 3.2 Driving force

The driving force of `movement` section 6.2 is replaced or redirected as follows [B20, B21]:

- **Climbing up:** the force is `F = ĉ · 50 · T · m` = 4,000 N with `ĉ` the direction of `Â + û` (up and
  towards the axis), in place of the ground rules.
- **Climbing down:** if *front*(0), `F = ĉ · 50 · T · m` with `ĉ` the direction of `û − Â`; otherwise the
  ground rules of `movement` 6.2 apply as usual, except that the step-assistance factor 4 is not applied.
- **At the top / at the bottom:** if *facing*(`ŵ`) and `d ≥ 0.001` and `a·ŵ > 0`, with `ŵ = ŵ_T`, `d = d_T`
  at the top and `ŵ = ŵ_B`, `d = d_B` at the bottom, the ground rules use the horizontal direction of `ŵ`
  in place of that of the input (so the player is steered onto the end of the ladder); otherwise they are
  unchanged.
- In every case the force acts only while the body is moving (`|a_k| > 0.1`), as in `movement` 6.2, and the
  side damping force of `movement` 6.2 acts across it.
- The forces of section 2.5 (holding towards the axis without input; pulling down while moving up when
  climbing down) act in addition.

### 3.3 Speed

`a` while climbing is that of `movement` section 5 including the factor `climb_coef` = 0.7 of `[actor]`
[D4]; sprinting and crouching are not possible (`movement` sections 4.2, 4.3) and leaning or aiming do not
force walking (`movement` 4.1). While climbing the body is not airborne and not *loose* (`movement` section
6.3), so its whole velocity is capped at `0.1 · |a|` [B19, D4, M3]:

| Mode | `|a|` | Speed cap (m/s) |
|---|---|---|
| walk, forward or sideways | 11.9 | 1.19 |
| walk, back | 9.52 | 0.952 |
| run, forward or sideways (default) | 24.99 | 2.499 |
| run, back | 21.42 | 2.142 |

### 3.4 Contacts

While climbing and moving (`|a_k| > 0.1`), each contact of the player is first given friction 0, an
error-reduction parameter equal to the world's (`level-cform` section 4.2) and twice the world's
constraint-force mixing (believed: the two values read are those of `level-cform` 4.2); the rules of
`level-cform` section 5.4 then apply as usual [B22].

### 3.5 Jumping off

A jump while climbing is accepted (`movement` section 6.6) [B24]. Its take-off sets the body's velocity to
`3 · ĵ`, where `ĵ` = `N̂` unless the input direction `â` (`a` unchanged if `|a|² ≤ 1.1754944 × 10⁻³⁸`) has
`|â·Ŝ| > 0.70710677`, in which case `ĵ` is the direction of `N̂ ± Ŝ` (the sign of `â·Ŝ`); `jump_speed` is not
used [B24]. In the step that applies the take-off the body becomes airborne and the climbing state changes
to *leaving* (section 2.3, so gravity acts again) [B24].

### 3.6 Movement state, camera and body

- The movement state's *climbing* flag (`movement` section 3) is set on each frame after the physics on which
  the player is climbing, and cleared otherwise [B23]. Its effects in `movement` (run rule, crouch, sprint,
  jump, `climb_coef`) and in `footsteps` section 2.1 (the climbing leg cycles) apply.
- **On the frame the flag is set** (it was clear before), the sprint flag of the state is cleared and the
  camera's yaw is limited [B25, B26]: with `ψ` the yaw of the direction `−N̂` of the ladder (the body is
  turned to face it; if `|N̂.y| > 0.70710677` the body keeps its facing and `ψ` is that facing's yaw), both
  `ψ` and the camera yaw `y` are brought into `[−π, π]` (a value outside is replaced by `2π · frac(x / 2π)`,
  then reduced by 2π if above π), and `d` = the difference, brought into `[−π, π]` by adding or subtracting
  2π once. If `−1 < d < 1`, the yaw is limited to `[y + d − 1, y + d + 1]`, with `y` here the camera yaw as
  stored (not brought into `[−π, π]`); otherwise it is not limited.
- While the yaw is limited, every change of yaw by mouse or keys (`movement` section 8) is followed by
  clamping it into the limits [B27].
- **On the frame the flag is cleared**, the yaw limits are reset to none [B25].
- While the flag is set and the yaw is **not** limited, the acceleration offered to the body is zero (the
  player cannot move on the ladder; e.g. after taking it from the top while facing more than 1 rad away
  from it) [B28].
- While the flag is set (and the player is alive), the body's own orientation (its transform, not the
  heading of `movement` section 5 item 3) is set each frame to face `−N̂` when `|N̂.y| ≤ 0.70710677`, instead of
  following the body heading [B25]. The first-person view does not show the body.
- The limit of ±1 rad is a constant of the program: the section `[actor_ladder_cam]` of `config\system.ltx`
  (`lim_yaw = -1.0, 1.0`) is not named by any program file [D5].
- When the flag changes, a message with the code 4 and the new value is sent to the game's own server
  process (only for the player, while alive); what it causes is **unestablished** [B29].

### 3.7 Taking a ladder while airborne

If the player becomes climbing while airborne, that step counts as the step on which the airborne body first
touches something (`movement` section 6.7): falling and jump flags are cleared and, if the state is falling
and the impact speed exceeds 4 m/s, the landing flag is set [B30]. Which contacts give the impact speed in
that case (whether the ladder's box counts) is **unestablished**.

## 4. Sound

Climbing makes **no sound of its own**:

- Nothing of the ladder, of the climbing states or of the climbing movement plays or names a sound: the
  section `climable_object` has no keys but `class`, the strings of `bin\xrGame.dll` containing `climb` are only
  `climb_coef` and `state_climb` (an interface icon) and those containing `ladder` only
  `materials\fake_ladders`, the code
  of sections 1 – 3 reaches no sound function, and no script mentions climbing or ladders except the script
  interface listing `scripts\lua_help.script` (the ladder's class id and server class) [D1, D5, D10, B31].
- **Footsteps.** While climbing, the leg cycle is one of the climbing set (`footsteps` section 2.1) unless a
  landing flag is set; no line of the step table names a cycle beginning `cl_`, so no step sounds [D6]. The
  pair of the player's material and the ladder's material, `creatures\actor` / `materials\fake_ladders`
  (pair 1251, no link), has an empty step list [D7], so a step whose ground material is the ladder's would be
  silent too.
- **Contacts.** The same pair has empty collide sound, particle and mark lists [D7]: a contact of the player's
  body with a ladder's box has no collide sound.
- **Landing onto a ladder.** If a landing flag is set as in section 3.7, the leg cycle is `norm_jump_end`
  (`footsteps` section 2.1; a hard landing's `norm_jump_end_1` has no steps) and its two steps (volume 1.3)
  are due 66 and 70 ms later (`footsteps` section 5); the player is not airborne while climbing, so they are
  played with the step sound of the pair of the player and the **ground material**. Whether a
  contact with a ladder's box sets the ground material (`level-cform` section 4.4) was not established: the
  player's contact handling writes a value kept with the player's own shape that the level-mesh collision
  sets, and whether box contacts change it was not traced [B32]. So whether those two steps are silent
  (ground material `materials\fake_ladders`) or sound with the material last touched is **unestablished**.
- In the states *near*, *at the top*, *at the bottom* and *leaving* the player walks by the ordinary rules,
  with ordinary footsteps (`footsteps`).

## 5. Verification facts

### 5.1 The ladders of `l01_escape`

From the records by section 1.2 (`sel` = the rows chosen as axis, side, normal; `cl` = normal extent
raised to 0.2; `fl` = A and S negated; length `2|A|` and width `2|S|`; points rounded to 1 mm) [M1, D2]:

| Record | sel | cl | fl | length | width | B | T | N̂ |
|---|---|---|---|---|---|---|---|---|
| `clmbl#0` | 0 1 2 | 1 | 0 | 7.4548 | 1.1297 | (−113.386, −30.453, −362.589) | (−113.358, −24.024, −358.814) | (−0.0137, 0.5064, −0.8622) |
| `clmbl#1` | 0 2 1 | 1 | 1 | 7.2228 | 1.1309 | (−137.362, −30.163, −396.856) | (−141.086, −23.974, −396.854) | (0.8569, 0.5155, −0.0047) |
| `clmbl#2` | 0 1 2 | 1 | 1 | 3.3328 | 0.8813 | (111.241, −2.834, 4.935) | (111.240, 0.448, 4.358) | (−0.0006, 0.1733, 0.9849) |
| `clmbl#3` | 0 1 2 | 1 | 0 | 4.3985 | 0.7951 | (114.545, −7.176, −34.215) | (114.545, −2.844, −33.451) | (0.0051, 0.1736, −0.9848) |
| `clmbl#4` | 0 1 2 | 1 | 0 | 4.4686 | 0.7948 | (109.317, −7.497, −10.301) | (109.317, −3.096, −9.525) | (−0.0013, 0.1736, −0.9848) |
| `clmbl#5` | 0 1 2 | 1 | 1 | 3.3962 | 0.7954 | (136.920, 0.235, 341.164) | (136.918, 3.579, 340.571) | (−0.0609, 0.1743, 0.9828) |
| `clmbl#6` | 0 2 1 | 1 | 1 | 5.5504 | 0.7227 | (26.590, 15.645, 659.211) | (23.772, 20.427, 659.211) | (0.8611, 0.5073, 0.0348) |
| `clmbl#7` | 0 1 2 | 1 | 1 | 3.8320 | 0.6591 | (−220.359, −19.627, −131.601) | (−220.364, −15.857, −132.285) | (0.0932, 0.1778, 0.9796) |
| `clmbl#8` | 0 2 1 | 1 | 1 | 3.2438 | 0.7950 | (−198.995, −19.694, −134.260) | (−198.149, −16.563, −134.262) | (−0.9653, 0.2610, 0.0027) |
| `clmbl#9` | 0 1 2 | 1 | 0 | 3.2442 | 0.7940 | (−220.382, −20.134, −170.983) | (−220.384, −17.002, −170.136) | (0.0027, 0.2611, −0.9653) |
| `clmbl#10` | 0 1 2 | 1 | 1 | 2.2075 | 0.4977 | (−185.404, −21.616, −132.995) | (−185.406, −19.490, −133.592) | (−0.0761, 0.2696, 0.9600) |
| `clmbl#11` | 0 2 1 | 1 | 1 | 2.2066 | 0.4960 | (−219.125, −22.055, −157.392) | (−219.719, −19.930, −157.392) | (0.9632, 0.2687, −0.0000) |
| `clmbl#12` | 0 1 2 | 1 | 1 | 3.4175 | 0.8967 | (136.919, 0.213, 341.185) | (136.928, 3.578, 340.585) | (0.0873, 0.1745, 0.9808) |
| `clmbl#13` | 0 2 1 | 1 | 0 | 15.2500 | 0.2532 | (130.008, 0.169, 342.977) | (145.258, 0.169, 342.981) | (0.0000, 1.0000, −0.0002) |

All but `clmbl#13` lean back by about 10° – 31° (`N̂.y` 0.17 – 0.52), the front facing up the slope;
`clmbl#13` lies flat with its front up [M1].

### 5.2 Invariants of all 232 ladders

- No two extents of a box are equal; the axis is row 0 in all 232 (side row 1 and normal row 2 in 109,
  side row 2 and normal row 1 in 123); the normal extent is below 0.2 in 229 (the smallest stored extent is
  0); `A` and `S` are negated in 176; the largest extent is 15.0668 [D8, M2].
- On `l01_escape`, for all 14 ladders a triangle of `level.cform` lies within 0.16 m vertically of **B**, at
  **B**'s horizontal position [D9]: the extents are half-sizes and **B** is the foot of the ladder.

### 5.3 Worked cases

Computed by `model.py` from sections 2 and 3 for `clmbl#1`, with **P** = `B + 0.5·Â + 0.36·N̂` (in front,
in range, `δ = −0.56`), the camera level and turned to face the horizontal direction of `−N̂`, the ladder
found at clock 0 and the state rules applied at clock 1 (at clock 0 the change would be refused, section
2.3) [M4 – M8]:

| Case | Result |
|---|---|
| state *near*, running towards the ladder (`a = 24.99 · ĉ_cam`) | `k` = 4.1014 → *climbing up* [M4] |
| the same with the camera 30° down | `k` = −1.8945 → *climbing down* [M4] |
| the same with no input | `k` = 0 → *climbing down* [M4] |
| *climbing up*, input along `Ŝ` | → *leaving* (gravity on); → *free* refused 1,000 and 3,000 ms after entering *leaving* with the body moved 1 m, accepted at 3,001 ms [M5] |
| *climbing up* with **P** at `h_top` = 0.2, −0.2, −0.3 | stays, stays, → *at the top* (the change needs the body centre more than 0.25 m above **T** along the axis) [M6] |
| *climbing up*, drive direction | `ĉ` = direction of `Â + û` = (−0.9704, 0.2414, 0.0035) [M7] |
| jump from the ladder, input along `Ŝ` | velocity `3 · dir(N̂ + Ŝ)` = (1.8090, 1.0887, −2.1312) [M8] |

## 6. Unestablished and believed

- When the physics world tells a ladder that a character is near (section 2.4); the ladder's radius's use.
- Whether contacts with a ladder's box give the impact speed (section 3.7) or set the ground material, and so
  whether two landing steps sound when a falling player takes a ladder (section 4).
- What the message sent when the climbing flag changes causes (section 3.6).
- The role of material flag 0x20, set only on `materials\fake_ladders` (`level-cform` section 3.2): no test of
  it was found in the code of this specification.
- Believed: the world parameters of section 3.4 are those of `level-cform` section 4.2.

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG): archives
  `gamedata.db0` … `gamedata.dbd` (later archives win); binary `bin\xrGame.dll`.
- Data examined: `spawns\all.spawn`, `game.graph`, `config\system.ltx` and its includes,
  `config\creatures\actor.ltx`, `config\creatures\m_stalker.ltx`, `gamemtl.xr`,
  `levels\l01_escape\level.cform`, the shipped scripts (text search).
- Analysis: static analysis (headless decompilation and disassembly with Ghidra 12.2) of the ladder object's
  set-up and collision filter, the ladder queries, the player body's climbing states and their per-step rules,
  the player body's driving force, step update and contact handling, the jump, the player's movement state,
  acceleration, camera yaw limits and body facing; constants read from the binary. No execution or debugging.
  A model written from sections 1.2, 2 and 3 reproduces section 5 from the records.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Whether the climbing state and the player's ladder are kept in a saved game, or what they are after loading one.
- What happens to the climbing state, the gravity switch and the yaw limits when the player dies while climbing.
