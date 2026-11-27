# The physics world and its step

Functional specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the physics world the
game simulates in: when it is made and removed, the parameters it gives the shipped physics library, gravity
for every body (with the no-gravity zone), where the simulation runs in a frame, how many fixed steps a frame
runs, what one step does and in which order, how bodies are grouped and which solver solves a group, the
console variables `ph_frequency` and `ph_iterations`, freezing and unfreezing, and the diagnostic output. It is
specification PH-2 of `docs/behavior/physics-scope.md` (section 3). What objects put into the world (shells,
character bodies), their contacts, resting, joints and impulses are the other specs of that scope and are only
named here.

It builds on (not repeated):

- `docs/behavior/physics-scope.md` (cited as `scope`): terms *body*, *shell*, *joint*, *step*, *frame*; the
  library (section 1.1);
- `docs/formats/level-cform.md` (cited as `cform`): the contact properties built from the world's spring and
  damping (section 4.2), the step `h` (section 6.1);
- `docs/behavior/player-movement.md` (cited as `movement`): the display frame time `dt_f` (section 3.2:
  smoothed, at most 0.1 s, 0 while paused), the character body, its pending push and gravity 19.62 m/s²
  (section 6.1);
- `docs/behavior/console-commands.md` 4.2 and `docs/behavior/console-scope.md`: parsing and status of
  `ph_frequency` and `ph_iterations`;
- `docs/behavior/environment-hits.md` 6.5 (cited as `hits`) and `docs/behavior/zone-idle-effects.md` 6.1: the
  no-gravity zone and the zone update;
- `docs/behavior/display-mode.md` B.4: the statistics overlay;
- `docs/behavior/console-log.md` 3.8: what a fatal error writes.

Conventions:

- Axes: y is up. `h` is the step length in seconds. Values are single precision.
- An **object** of the world is anything the world steps as a unit: a shell, or the body of a character (the
  player's or another's).
- A **group** is the set of bodies and joints the library solves together in one step (section 4.2).
- Statements are **static** (read from the program and the shipped library; the game was not run). Inferred
  statements are marked **believed**; what could not be established is marked **unestablished**.
- Bracketed ids are the checks reproducing a statement: functions of `checks.py` in the dirty workspace's
  `checks/physics-world/` (`python3 checks.py` runs them all; `export.py` re-creates the evidence they read).

## 1. The world

### 1.1 Lifetime

There is one world per loaded level. It is made during level loading, in the stage whose caption is
`st_client_spawning` (`level-loading`), before the client waits for the server and receives the level's
objects, so every object of the level enters an existing world [W1]. Nothing else makes a world [W1b]. It is
removed when the level is destroyed, after the log line `- Destroying level`, and at no other time [W1c];
removing it drops all contacts, shuts the library down and takes the world out of the frame (section 2.1)
[W1d]. Between levels (main menu) there is no world.

### 1.2 What making it sets

When the world is made:

- gravity becomes **(0, −19.62, 0)** m/s², from a stored 19.62 (`movement` 6.1); nothing changes it later
  [P1, P1b];
- the world's **error reduction** `E` and **constraint-force mixing** `C` (the library's terms; `cform` 4.2)
  are set from the step `h` in force at that moment: `E = S·h / (S·h + D)`, `C = 1 / (S·h + D)`, with the
  world spring `S` and damping `D` of `cform` 4.2 (24,000,002 and 400,000; derived when the program loads
  from those two constants and the initial step 0.01) [P2, P2b]. At `h = 0.01`: `E = 0.375`,
  `C = 1.5625 × 10⁻⁶` [P2c];
- a **floor** height: the lowest y of the level's bounding box minus 30 m (section 4.5) [S4]; the box is the
  level's extent (the one the level-map maker of the demo recorder names), **believed** the bounds of the
  level's geometry [S4e];
- an empty list of per-step actions (section 4.3) [S3];
- the rest-pass counter 0, so the first step makes a rest pass (section 4.4) [S1c];
- a library setting (an "auto-enable depth") is set to 100,000,000; no code of the library reads it except
  its own getter, so it has no effect [P4].

### 1.3 The library's parameters

The shipped library keeps a single set of solver parameters for everything it simulates: its setters ignore
the world they are given, and the game passes none [P3]. The game never calls the library's own world
creation, so where the game sets nothing the library's initial values apply [P3b]. The parameters in force
in play:

| Parameter | Value | Set by |
|---|---|---|
| gravity | (0, −19.62, 0) m/s² | world creation [P1] |
| error reduction `E`, constraint-force mixing `C` | from the step at world creation (1.2) | world creation [P2b] |
| iteration count of the iterative solver | `ph_iterations` once that command has run; 20 before (section 2.4) | console [F5b], library [P3c] |
| relaxation factor of the iterative solver | 1.1 | library [P3c] |
| contact surface layer | 0.001 m | library [P3c] |
| limit on a contact's correcting speed | infinite (no limit) | library, when it loads [P3c] |

The game changes none of the last three [P3d]. The iterative solver reads the iteration count and the
relaxation factor [P3e]; the meaning of the relaxation factor (an over-relaxation weight of the iterative
solution) is **believed** from the library's name for the setting. A contact's correcting speed is
`max(0, depth − 0.001) × (1/h) × e`, `e` the contact's error reduction (`cform` 4.2), with no upper limit;
both kinds of contact the game makes (the library's and its second, game-defined kind, `scope` 1.1) follow
this rule [P3f, P3g].

`E` and `C` are also the values the library gives every joint row that does not set its own (**believed**
from the library's stepping code reading them, [P4b]; which joints set their own is PH-7). Because only world
creation sets them, a later change of `ph_frequency` leaves `E` and `C` at the values of the step the level
started with [P2d]; the world spring and damping are recomputed from the new step (coming out as the same
24,000,002 and 400,000, `cform` 4.2), so the contact properties of `cform` 4.2 follow the new step through `h`
[P2e].

### 1.4 Gravity for every body

Every body has its own gravity switch, on when the body is made; the library applies the world's gravity to
every body whose switch is on [G1, G1b]. The game switches it for bodies of several of its own procedures (31
places) [G1c]; which bodies and when belongs to the specs of those behaviours. Among them, the search for a
free place (section 5) switches off the gravity of its own probing body [G1d]; dragging (PH-16) is
**believed** to be another. Apart from those switches and the no-gravity zone
below, every body falls with 19.62 m/s².

**The no-gravity zone** (`Z_NOGRAV`, `hits` 6.5). For every object in its list at each zone update, and for
an object when it is added to the list, gravity is switched **off**; for an object taken off the list it is
switched **on** again. Objects with one particular object flag are skipped (its meaning is
**unestablished**) [G2]. Switching acts as follows [G2b, G2c, G2d]:

- An object with an active shell: the gravity of all the shell's bodies is switched. When switching off and
  the shell is active, one element of the shell chosen at random (uniformly among its elements, skipped when
  that element is not active) gets an impulse of size `M · g · h` along a random unit direction, where `M` is
  the shell's total mass and `g` = 19.62; the direction's angle to the z axis is uniform in 0 … π and its
  angle about that axis uniform in 0 … 2π (so not uniform over the sphere). The impulse is applied like a
  hit impulse on that element (PH-9) together with a second random vector (same construction of direction,
  length `r` times a size the element gives, `r` uniform in 0 … 1) passed as its point; how that point acts
  is **unestablished**. So a floating object keeps receiving pushes, one per zone update.
  **Disputed in review:** the push is not given on every switch-off. After switching, the push follows only
  when the body of the shell's first element still has its gravity on; an element's switch is skipped while
  that element is not active or carries one element flag (meaning unestablished), so "the gravity of all the
  shell's bodies is switched" holds only for its active, unflagged elements, and in the ordinary case (the
  first element switched off) no push is given.
- An object without an active shell (a character): its movement's gravity switch is set. When switching off,
  unless the object's entry in the zone's list carries a mark or the character body is in one internal state
  (meanings **unestablished**), `m · g` along a direction given by its body (or (0, 1, 0) when it has none)
  is added to its pending push (**believed** the pending vector of `movement` 6.1; PH-11), `m` the
  movement's mass. **Disputed in review:** the mark on the object's list entry suppresses the whole of this
  case, not only the push: with the mark neither the movement's gravity switch is set nor the push added
  (also when the object leaves the list); only the character body's internal state suppresses the push alone.

## 2. The frame

### 2.1 Where the world runs

A frame runs two sequences of per-frame work, each in order of priority, highest first [F2]:

1. **The first sequence** runs within the frame's time update on the main thread [F2b]. Its members include,
   in this order, the application, the persistent environment, input and the level's frame [F6]; the
   level's frame processes the messages from the server (objects appearing) and updates every game object
   [F6b, F2e].
2. **The second sequence** runs after the first, on a second thread, while the main thread draws the frame;
   the main thread waits for it before the next frame begins, and runs it itself when the second thread has
   not run it for the frame [F2c, F2d]. In it the level's server-side update runs first (not while the game is
   paused), then the physics world [F1c].

The world belongs to the second sequence because a device flag is set from the start; no console command and
no code of normal play changes that flag (only a level-map maker of the demo recorder overwrites all the
flags) [F1, F1b]. So, within a frame: the objects' updates of the frame come first, then the frame's physics
steps, and those finish before the next frame's object updates (**believed**: no other order occurs in
play).

### 2.2 Steps per frame

The world keeps an accumulator `a` (seconds; 0 when made). Each frame, unless the world is frozen (section
5), `k · dt_f` is added to `a`; when `a` then is at least `h`, the frame runs `n` steps, `n` the whole part of
`a / h`, and `a` keeps the remainder `a − n · h`; otherwise no step runs and `a` is kept [F3].

`dt_f` is `movement`'s display frame time and `k` a time factor that is always 1 (it starts at 1 and every
write sets 1) [F3b]. While the game is paused `dt_f` is 0, so no steps run [F3c]. With `dt_f ≤ 0.1` a frame
runs at most `0.1 / h` steps (10 at the default step, 20 at `ph_frequency 200`). **Disputed in review:**
since the kept remainder is below `h`, the bound is fewer than `1 + 0.1 / h` steps: `0.1 / h` only when that is
a whole number; `ph_frequency` takes non-whole values (e.g. at 75 a frame can run 8 steps, not 7.5). While a frame's steps run the
world counts as stepping (section 5) [F3d]. While frozen, nothing happens and `a` is unchanged [F3].

### 2.3 `ph_frequency`

Its value, clamped to 50 … 200 (`console-commands` 4.2), sets the step at once: `h = 1 / value` [F5]. If a
world exists, the accumulator becomes `a = dt_f − (whole part of dt_f / h) · h` with the current frame's
`dt_f` (the remainder of the frame time over the new step) [F5]. The world spring and damping follow the new
step; `E` and `C` do not (section 1.3) [P2d, P2e]. The initial step is 0.01 s [P2].

### 2.4 `ph_iterations`

An integer 15 … 50, initially 18 (`console-scope`). Each time the command runs, the value is passed to the library as the iteration count of the iterative solver; nothing else
sets it, so until the command first runs the library's own 20 applies, while the console shows 18 [F5b,
P3c]. That running the line of `user.ltx` at startup passes the value is **believed** (the console runs the
file's lines as commands).

## 3. One step

A step does, in this order [S1]:

1. **Rest pass**, at the first step after the world is made and then every 64th step (section 4.4) [S1c].
2. The step counter (64-bit, from 0 at creation) grows by 1 [S1d].
3. **Contacts.** Every active object gathers its contacts: with the other active objects whose bounds
   overlap its own (and, for objects moving fast, along a ray of their motion), subject to the collision
   filters, and then with the level's geometry [S1b]. Each contact joins the group (section 4.2). How
   contacts are made and filtered is PH-4.
4. Every active object does its own pre-step work (**believed**: forces and control, such as the character
   body's movement forces of `movement` 6.1).
5. Every per-step updater (game parts that hook into each step) does its pre-step work.
6. The per-step actions run (section 4.3).
7. **Solving.** Every group is solved for one step of length `h` (section 4.2).
8. Every active object is reset to its own group and does its post-step work (reading the solved state,
   resting and the floor, sections 4.4 and 4.5); its bounds are then moved to its new place [S1b, S2f].
9. Every per-step updater does its post-step work.
10. All contacts of the step are dropped.
11. A hook is called with the step's start and end time in milliseconds (the program's millisecond clock at
    the frame's start, advanced by `round(1000·h)` per step); the level installs one that does nothing [S6].

## 4. Groups, solving and what the world does after a step

### 4.1 Objects

The world keeps its **active** objects (stepped), its **set-aside** objects (while frozen, section 5), the
per-step updaters (active and set aside) and the list of resting shells (section 4.4) [S1, Z1].

### 4.2 Groups and the solver

Each active object has its own group: its bodies and its own joints. A contact between two objects merges
their groups for the rest of the step, so touching objects are solved together; a contact with the level
joins only the object's group [S2d, S2e]. Limits [S2d, S2e]:

- contacts between two objects are made (and the groups merged) only while the two groups together have fewer
  than 1,500 joints (contacts counted as joints) and fewer than 500 bodies;
- contacts with the level are made only up to 1,500 joints in the group;
- one pair of geometries yields at most 809 contacts.

After the step each group is reset to its object's own bodies and joints [S2f].

**Solver.** A group is solved with the library's **iterative** solver (`ph_iterations` iterations, section
2.4), unless it carries a mark for the **exact** solver and has fewer than 30 joints (contacts included)
[S2, S2b]. No code sets that mark: a group's marks start with two other bits and change only by the per-step
copy, the merge and the clearing of one bit, so every group is solved by the iterative solver (**believed**,
from the absence of any setting found) [S2c].

### 4.3 Per-step actions

The world keeps a list of (condition, action) pairs. At every step each action whose condition holds runs; an
entry is removed when its action or its condition reports it done [S3]. A script's constant force on an
object is such an action: the force (direction × size) acts on the object's shell for the given time,
counted as `whole part of (0.001 · ms / h)` steps of the step counter [S3b] (that it acts at every step of
that time is **believed**). The entries of a shell
are removed when the shell is destroyed; without a world or a shell the script call does nothing, and the
message it composes is not output [S3c]. Which other features use the list is **unestablished**.

### 4.4 Resting shells (the rest pass)

When, after a step, all bodies of a shell are switched off (resting, PH-5), the shell is put on the world's
list of resting shells with a count of 10 [S5]. At each rest pass (every 64 steps, section 3) the count of
every listed shell drops by 1; at 0 the shell's cached level-contact data is emptied and the shell leaves the
list [S5, S5b]. **Disputed in review:** at a rest pass a listed shell whose count is 0 is handled (cache
emptied, delisted); otherwise its count drops by 1. So a shell listed with 10 is handled at the 11th rest pass
after its listing, not at the pass where its count reaches 0. A shell that becomes active again leaves the list [S5c]. Emptying that cache is **believed**
to have no visible effect. How a shell comes to rest and wakes is PH-5. (The stored 64 is the same stored
value `scope` 1.1 names the rest count [S1c]; whether it plays both parts is for PH-5.)

### 4.5 The floor

After a step, a shell whose first element lies below the floor (section 1.2: the level's lowest y − 30 m)
is taken out of the world's active objects: it stops being simulated [S4, S4b]. A character body below the
floor is handed to a handling of its own, whose effect is **unestablished** [S4c]. Nothing else uses the floor
[S4d].

## 5. Freezing

Freezing the world sets all its active objects and all its per-step updaters aside (each set-aside object is
told so); unfreezing tells them and makes them active again [Z1]. While frozen, the frame runs no steps
(section 2.2). A set-aside object can be made active again on its own, so a frozen world steps only the
objects a procedure puts back [Z1b].

The world is frozen only inside these procedures, each of which unfreezes it before it returns, so it is
never frozen across frames [Z1c, Z1d, Z1e]:

- the hand-over of a character to its death shell, which runs 10 steps for that shell alone (PH-15);
- the search for a free place for a shell about to be activated where it may overlap something (e.g. a
  vehicle at spawn): up to 10 rounds of up to 15 steps of a probing box (PH-6, PH-21);
- a character's tests of a new position and of a change of its collision shape (**believed**: the posture
  change of `cform` 5.3; PH-8, PH-22);
- taking a shell out of the simulation, in one case whose condition is **unestablished**: the shell is made
  active and a contacts-only pass runs (contacts gathered and dropped without solving) [Z1g];
- network correction (**believed** not used in single player).

Fatal errors (`console-log` 3.8: a failed check with a message): freezing a frozen world (`already
freezed!!!`) and unfreezing one that is not frozen (`is not freezed!!!`) [Z1]; taking a shell out of the
simulation while the world is stepping (`can not deactivate physics shell during physics processing!!!`),
while it is frozen (`can not deactivate physics shell when ph world is freezed!!!`), or while the shell is
set aside (`can not deactivate freezed !!!`) [Z1f]; activating an object that has no collision geometry
(`trying to activate destroyed or not created object!`) [S5c].

## 6. Diagnostic output

- `!!!TOO MANY PHYSICS STEPS PER FRAME = %d !!!` (`%d` the frame's step count) would be logged for more than
  20 steps in a frame, but only with an internal switch that starts off and that nothing sets: in the shipped
  game it is never logged [F4]. The same holds for `!!!TOO MANY PHYSICS STEPS FOR CORRECTION PREDICTION = %d
  !!!` of network correction [F4b].
- The statistics overlay (`display-mode` B.4) shows three physics timers [D1, D1b]: `Physics` measures the
  world's whole work of the frame (section 2.2); `collider` the contact gathering (section 3, item 3) summed
  over the frame's steps; `solver` the per-step actions and the solving (items 6 and 7) summed over the
  frame's steps, its count being the number of steps in the frame.
- The script messages of section 4.3 are not output.

## 7. Unestablished

- The effect of the handling of a character body below the floor (section 4.5).
- The meaning of the object flag that exempts objects from the no-gravity zone, the point of the zone's random
  push on a shell, and the character state that suppresses its push (section 1.4).
- Which procedures switch the gravity of their bodies, and of which bodies (section 1.4; their own specs).
- Which features besides scripted constant forces use the per-step actions (section 4.3).
- That no code ever marks a group for the exact solver (believed, section 4.2).
- That the frame order of section 2.1 is the only one occurring in play (believed).

## 8. Provenance

- Date: 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); binaries `bin\xrGame.dll`,
  `bin\XR_3DA.exe`, `bin\ode.dll`.
- Analysis: static analysis of `bin\xrGame.dll` and `bin\XR_3DA.exe` (Ghidra 12.2 headless decompilation and
  instruction listings) of the world's creation, removal, per-frame call, step, groups, freezing, per-step
  actions, rest list, floor, the console commands `ph_frequency` and `ph_iterations`, the no-gravity zone's
  gravity switch, the device's frame sequences and threads, and the statistics overlay; disassembly of
  `bin\ode.dll` (the system `objdump`) of its parameter setters and initial values, its load-time
  initialisers, its body gravity switch and its contact rows; constants read from the binaries. No execution or
  debugging; the game was not run.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The accumulator's remainder after a frame's steps, as a fraction of the step (`a / h`), is read by bodies to blend between their states of the last two steps (believed: for what is drawn between steps); not stated.
- When all bodies of a shell are switched off after a step, the shell is also taken out of the world's active objects (it stops being stepped) at the moment it is put on the resting list; section 4.4 states only the listing.
- The steps run inside the procedures of section 5 are whole steps: they advance the step counter, the rest-pass countdown and the millisecond hook, and run the per-step actions of section 4.3, which freezing does not set aside; not stated.
- In the no-gravity zone a character's switch is also passed to its character body (to that body's gravity when the body is active, and its negation to a second setting of the body); what these do is not stated.
