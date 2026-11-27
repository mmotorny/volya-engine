# Run-to-run variation of the original's frames

What can make two runs of the shipped game produce different frames when both receive the same inputs, the
same clock readings and the same frame stepping, and what environment control removes it. It is written for
the capture tooling of the original on Windows (a frame clock that replaces every clock function the game's
modules import, holds the game after each presented frame and makes `Sleep` advance the clock only by its
argument), and for the three views that still differ under it: the start view in the trader's bunker
(around the trader's pose), the caged lamp under the vault (small bright specks) and the outdoor view above
the start (foliage and ground speckle).

Statements are **established** (from static analysis of the shipped binaries or the game's scripts, with a
check, section 9), **believed** (with the reason) or **unestablished**. Nothing here was measured by running
the game.

## 1. Summary

Four mechanisms reach those views [T1 – T21]:

1. **The shared random generator's seed comes from the processor's time-stamp counter** (section 2). One
   32-bit generator state, shared by the engine executable, the first renderer, the game library, the
   particle library and the sound library, is set at start-up of the core library to the low 32 bits of a
   time-stamp-counter read. Everything that draws from it differs between runs: detail-object (grass)
   refresh scheduling, skeleton bounds refresh, particle emission, rain and lightning, weather ambient
   choices, AI vision ray jitter, and other renderer and game uses.
2. **The secondary thread races the main thread** (section 3). Each frame a block of work (the physics
   world's step, network-message processing, sound processing, A-Life, path building, monster and stalker
   updates, a script garbage-collection step) runs on the engine's secondary thread *while the main thread
   draws the frame*, or, when the secondary thread has not started it by the time drawing ends, on the main
   thread after drawing. Which happens, and how the two threads' draws from the unsynchronised shared
   generator interleave, depends on scheduling.
3. **The C runtime's random generator is per thread, and the physics library draws from it** (section 4).
   The physics step draws from the C runtime's `rand` of the thread it runs on; scripts' `math.random`
   (which picks the trader's body and head animations) draws from the main thread's. When the physics step
   moves to the main thread (mechanism 2), the scripts' sequence changes.
4. **Clock reads made off the main thread, and time budgets** (section 5). The capture clock advances with
   the main thread's reads and sleeps only; work timed by a performance-counter timer (A-Life's per-frame
   quantum) reads a different clock depending on the thread it runs on.

The smallest set of interventions believed to make the three views identical (section 7): a constant value
for the one time-stamp-counter read that seeds the shared generator, and one ordering rule: each frame's
secondary pass runs to completion while the main thread waits in its per-frame zero-length sleep.

## 2. The time-stamp counter

### 2.1 Where it is read

The shipped modules read the time-stamp counter (the `rdtsc` instruction) at nine places, and nowhere else
[T1]: five in the core library, two in the engine executable and two in the game library. The first
renderer, the particle, sound, collision, physics, script, network and skinning libraries and the C runtime
contain none. No module reads it through an API.

| Module | Reads | What the value feeds |
|---|---|---|
| core library | 1 | the shared generator's seed (section 2.2) [T2] |
| core library | 4 | the counter's calibration (section 2.3) [T3] |
| engine executable | 2 | a value stored with each render target when it is created [T4] |
| game library | 2 | the script profiler's timer (section 2.3) [T3] |

### 2.2 The shared generator

One generator state of 32 bits is shared by the engine executable, the first renderer, the game library,
the particle library and the sound library (each imports it from the core library) [T2]. A draw sets
*s* ← (*s* · 214,013 + 2,531,011) mod 2³² and returns (*s* ≫ 16) AND 32,767; each of those modules has
one routine that does this [T2]. The state is set once, during the core library's start-up (in the same
routine that records the floating-point control words), to the low 32 bits of a time-stamp-counter read
[T2]. It is not seeded again by anything (believed: no other write to it was found). Draws are not
synchronised between threads (no lock is taken around a draw) [T2], so draws made by two threads at once
interleave in scheduling order.

Its consumers include, among others not individually identified [T2, T15, T17, T21]:

- **detail objects (grass):** each visible slot is re-tested for occlusion after 10 + (rand mod 15) frames,
  and its per-instance refresh frame is the current frame plus rand mod 15 [T15] (the outdoor view's
  foliage and ground cover); the placement of the instances in a slot uses a separate generator seeded from
  the slot's own coordinates and does not vary [T15];
- **skeletons:** each animated model recomputes its bounds after a count of bone updates; the count starts
  again at rand mod 31 after each recomputation, after 32 updates [T17];
- **particles:** the particle library's emission domains (positions, velocities and the like of new
  particles) draw from it (the caged lamp's specks are believed to be particles or rain drops, whose
  positions both come from it);
- **rain and lightning:** drop birth, renewal and hits, and the lightning bolt's timing and shape;
- **weather ambients:** sound and effect choices and periods (sound-weather-ambient.md section 3.2);
- **AI vision:** each vision ray is jittered by a random direction [T21];
- **the first renderer:** further uses in its frame rendering (one takes rand mod 60,000 against the
  global time in milliseconds) and in its gamma set-up, not identified further (unestablished);
- **the game library:** about a hundred draw sites (AI, monsters, items, effects), not identified further.

### 2.3 The calibration and the profiler

At start-up the core library raises the process priority, waits for one tick of `timeGetTime`, reads the
counter, waits until `timeGetTime` has advanced by 1,000, reads it again, and derives the counter's
frequency; it also times 256 pairs of back-to-back counter reads and 256 back-to-back `QueryPerformanceCounter`
pairs for their overheads, and reads `QueryPerformanceFrequency` [T3]. Under the capture clock the
`timeGetTime` wait ends after a fixed number of reads, but the counter's difference over it is real time,
so the derived counter frequency varies. Its only reader outside the core library is the game library's
script profiler, which converts its own counter readings with it [T3]; the scripts use that profiler only
in a profiling script (believed: not run by the game). The game's timers use `QueryPerformanceCounter`
(section 5), so neither the calibration nor the profiler reaches the frames (believed).

The `QueryPerformanceCounter` overhead measured here is subtracted from every timer reading of the core
library's timers [T3]; under the capture clock it is a fixed function of the clock's read count (believed).

### 2.4 Render targets

Each render target records a counter reading when it is created [T4]; no reader of that value was found
(unestablished whether one exists; believed not to reach the frames).

## 3. The secondary thread

### 3.1 The frame loop

The engine's frame loop runs on the main thread, which the program binds to the first processor at start-up
(`SetThreadAffinityMask` with mask 1); the secondary thread, created by the engine's thread helper (the C
runtime's `_beginthread`, which calls `CreateThread`), is not bound [T16, T5]. The two threads share two
critical sections, A and B. The main thread holds A outside the window below. Each frame [T6]:

1. the main thread runs the frame's update (time step, objects, scheduler, scripts) and builds the view and
   projection;
2. it enters B, leaves A and calls `Sleep(0)`;
3. it draws the frame and presents it;
4. it enters A and leaves B;
5. if the secondary thread has not run the frame's secondary pass (it records the frame number when it
   starts the pass), the main thread runs the pass itself, now [T6].

The secondary thread loops: enter A; record the frame number; run the frame's queued jobs (section 3.3) and
clear the queue; run the secondary sequence (section 3.2); leave A; enter and leave B (so it waits for the
main thread's step 4); enter A again [T6]. So when it wins A at step 2 its pass runs *concurrently with the
main thread's drawing* (step 3); when it does not win A before step 4 the pass runs on the main thread after
drawing. Both outcomes occur by scheduling alone.

Other waits of the main thread's loop: `Sleep(100)` while the application is not active and `Sleep(1)` after
a frame while its window is not active, and, at quit, `Sleep(0)` in a loop until the secondary thread has
seen the quit request and ended [T6].

### 3.2 The secondary sequence

What runs in the secondary sequence rather than in the main thread's frame update is chosen by device flags,
whose initial value is 0x1c641 [T7]:

| Flag | Initially | Runs in the secondary sequence when set |
|---|---|---|
| 0x4000 | set | sound processing [T7] |
| 0x8000 | set | the physics world's step (registered when a level's physics world is created) [T7] |
| 0x10000 | set | network-message processing between the local client and server [T7] |

So with the shipped defaults the physics step, sound processing and message processing run on the
secondary thread, or on the main thread after drawing (section 3.1). Whether a configuration file or
command line changes these flags is unestablished (no console command named for them was found).

### 3.3 The queued jobs

During its frame update the game library queues jobs for the frame's secondary pass [T8]:

- the movement path builder's path computations;
- the A-Life simulator's update;
- monsters' and stalkers' perception and look updates (when AI flag 0x10 is set) [T8], which cast the
  vision rays of section 2.2;
- three level jobs (when AI flags 0x20, 0x40 and 0x80 are set), one of them a step of the scripts'
  garbage collector [T8].

The AI flags start at 0x1ff, so all of these are queued [T8]; otherwise each runs in the main thread's
frame update instead (the monster update) or is not queued (believed for the level jobs).

### 3.4 What the race changes

- **Shared-generator order.** Draws by the secondary pass (vision rays, and believed others in the jobs and
  the physics step) interleave with the main thread's drawing-time draws (detail objects, skeletons,
  particles) in scheduling order, so every later draw differs [T2, T15, T17, T21].
- **The C runtime's per-thread generator** (section 4).
- **Torn state.** While the pass runs concurrently, drawing reads object state (positions, bone transforms
  of physics-driven bodies) that the physics step and message processing are changing (believed from the
  structure; not traced per object).
- **Clock and budgets** (section 5).
- **Heap order.** Allocation order, and so heap addresses, differs when the two threads allocate at once
  (believed). Sort orders that fall back on container order keyed by addresses then differ: the first
  renderer sorts its draw groups by a float key with an unstable sort over containers keyed by state
  pointers (section 6), which changes only the order of draws with equal keys (believed small).

### 3.5 Other threads

The collision library builds its models on a separate thread only with the command-line switch `-mt_cdb`,
otherwise on the calling thread [T19]; the game library's only thread refreshes the internet server list
(the multiplayer menu) [T19]. Threads of the Direct3D driver and of the system are not examined
(unestablished). A New Game starts a single-player server (`start server(all/single/alife/new)`), which
makes the client and server connect directly; that skips a wait loop of `Sleep(5)` until network
synchronisation, whose iteration count would otherwise depend on another thread [T13, T14].

## 4. The C runtime's generator

The C runtime's `rand` keeps its state in the calling thread's runtime data: 1 when a thread first uses the
runtime, changed by `srand` on that thread only [T9]. Its users [T9 – T12]:

- **scripts:** `math.random` draws `rand` and `math.randomseed` calls `srand` [T10]; scripts run on the
  main thread (believed). The trader's script picks its body animation, and on each request its head
  animation, with `math.random` [T13]. One script function reseeds with the game clock (milliseconds of
  game time) before drawing, in `_g.script` and in an older copy of that file the archives hold under an
  empty name [T13]; no script reads the wall clock (`os.time`, `os.clock`) [T13];
- **the physics library:** its iterative step and its fast step draw `rand` [T11]; the game library uses
  the iterative and the exact step [T11]. It draws on the thread running the physics world's step
  (section 3.2);
- **noise generators:** the engine's noise generators call `srand` with their own seed at first use and
  fill their tables from `rand`; the particle library fills its noise table after `srand(1)` [T12]; both
  reset the calling thread's sequence;
- the engine executable's noise initialisation and one game-library routine also draw `rand`.

So the main thread's sequence, which picks the trader's animations, is deterministic only if the physics
step never runs on the main thread, or always does at the same points (section 3.1, step 5).

## 5. Clocks and threads

- **Every clock is reached through an import table.** The game's modules read time only through imported
  `QueryPerformanceCounter`, `QueryPerformanceFrequency`, `GetTickCount`, `timeGetTime`,
  `GetSystemTimeAsFileTime` and `GetLocalTime` (and the C runtime's time functions, which reach the system
  through the C runtime's own imports: `clock` reads `GetSystemTimeAsFileTime`) [T5]. No module looks up a
  clock or thread-creation function by name at run time: the names of `GetProcAddress` lookups found are
  the Direct3D factory and the libraries' entry points (believed from their strings), and no clock
  function's name appears in any module beyond its import or export entries [T5]. No delay-load imports
  name clocks [T5]. The C runtime is a separate module with its own imports of `CreateThread`,
  `QueryPerformanceCounter`, `GetSystemTimeAsFileTime`, `GetTickCount`, `GetLocalTime` and
  `GetTimeZoneInformation` [T5]; it must be among the modules whose imports the capture clock replaces.
- **Timers.** The core library's timers read `QueryPerformanceCounter` and convert with
  `QueryPerformanceFrequency` [T3]; the frame time, the global time, the scheduler's budget and the A-Life
  quantum all use them.
- **Off-main-thread reads.** The capture clock advances with the main thread's reads and sleeps only, so a
  read made on the secondary thread returns whatever the clock holds at that moment of the main thread's
  progress. The A-Life update stops processing objects for the frame when its performance-counter timer
  passes its quantum [T20]; on the secondary thread under the capture clock that timer stands still while
  the main thread waits, but moves with the main thread's reads while both run, and on the main thread
  (section 3.1, step 5) it moves with the pass's own reads. Its object count per frame then varies with the
  race (believed; it affects offline characters, and online ones through switching).

## 6. Other sources examined

- **Seeds from time or addresses.** The game's scripts reseed only from game time [T13]; the detail
  objects' placement seed comes from slot coordinates [T15]; the noise generators' seeds are their own
  constants or 1 [T12]. No seed from a memory address was found (unestablished for the game library's
  generators other than the shared one).
- **Addresses.** The game's libraries all prefer the same base address and none opts into address
  randomisation, so all but the first loaded are relocated by the loader [T18]; together with the system
  heap's placement, pointer values can differ between runs (believed). They reach the frames only through
  sort orders of equal keys (section 3.4).
- **Uninitialised memory:** unestablished.
- **Hardware queries:** the processor-feature queries (`cpuid`) at start-up select code paths and are the
  same on one machine (believed); the Direct3D device's capabilities are the same on one machine and driver
  (believed).
- **Time zone:** the C runtime reads the time zone for local-time conversions (saved-game names and logs;
  believed not to reach the frames).
- **Sound off.** With sound off, whether the sound library still runs its processing in the secondary
  sequence, and how the trader's speech ends (which requests its next head animation), is unestablished.

## 7. Interventions

Stated as environment control, smallest first. The first two are believed sufficient for the three views;
the rest close what remains.

1. **The seeding time-stamp-counter read returns a constant.** The first `rdtsc` executed in the process
   (during the core library's start-up, before any draw) must give the same low 32 bits in every run; any
   constant works. The other eight reads need not be controlled for these views (section 2.3, 2.4).
   **Disputed in review:** the seeding read is not the first counter read executed: the core library runs its
   calibration (section 2.3) first in the same start-up, which executes 514 counter reads (one on each side
   of the 1,000 ms wait, then 256 back-to-back pairs); the seeding read is the next one, the 515th counter
   read the game's modules execute, and the read counts before it are fixed constants. Counter reads made in
   the process by system libraries and drivers were not examined. The read to control is the core library's
   one made right after its calibration, not the first in the process.
   `rdtsc` is an instruction, not an import, so this needs instruction-level control (the processor's
   time-stamp-disable control, which only the operating system can set, or a virtual machine that supplies
   the counter), or, equivalently, writing a constant into the generator's state after the core library
   has started and before the engine's first frame.
   **Disputed in review:** writing into the generator's state is not environment control (it changes the
   program's memory and needs knowing where that state is), and its equivalence is unbacked: any draw made
   between the seeding and the write would still use the run's own seed, and none was shown absent (the
   first renderer's gamma set-up, section 2.2, is a candidate).
2. **Serialise the secondary pass at the main thread's frame sleep.** When the main thread calls
   `Sleep(0)` in its frame loop (after leaving critical section A, section 3.1 step 2), return only once the
   secondary thread has completed its pass, that is, once it is blocked in `EnterCriticalSection` (on B,
   which the main thread holds) or has ended. The pass then always runs on the secondary thread, entirely
   between the main thread's update and its drawing, with the main thread (and so the capture clock)
   standing still. This removes the shared-generator interleaving, keeps the physics library's `rand` on
   the secondary thread so the main thread's sequence (the trader's animations) is fixed, removes torn
   reads during drawing and the main thread's take-over at step 5, and fixes the A-Life quantum (its timer
   stands still). The same rule must let the quit wait (`Sleep(0)` until the secondary thread ends)
   proceed. The critical sections are entered through the core library's imports of `EnterCriticalSection`
   and `LeaveCriticalSection`, and the thread is created through the C runtime's import of `CreateThread`,
   so both can be observed by the same import replacement the capture clock uses.
   **Disputed in review:** "blocked in `EnterCriticalSection` on B" does not mean the pass is done. After
   the main thread leaves B (section 3.1 step 4) the secondary thread, waiting on B, may not have taken it
   yet when the main thread enters B again at the next frame's step 2 (Windows documents no order in which
   waiting threads obtain a critical section); it then waits on B through the whole frame without running
   that frame's pass, the stated condition holds at once, and the main thread takes the pass over at step 5.
   The ordering needs two waits: the main thread's entry to B at step 2 proceeds only once the secondary
   thread has entered and left B since the main thread last left it (it is then waiting on A or has ended);
   and the frame's `Sleep(0)` returns only once the secondary thread has entered A since the main thread
   left it and is then waiting on B (or has ended). The core library's critical-section wrapper enters and
   leaves through the system's `EnterCriticalSection` and `LeaveCriticalSection` directly (checked in
   review).
3. **Keep the window active** for the whole run, so that the loop's `Sleep(100)` and `Sleep(1)` branches
   (section 3.1) are never taken.
4. **Keep the loader's addresses fixed** (the same load order and no other module injected at different
   addresses) if equal-key draw orders still differ (section 3.4; believed unnecessary).

An alternative to rule 2 is to clear device flags 0x4000, 0x8000 and 0x10000 and AI flags 0x10 – 0x80,
which moves that work into the main thread's frame update; it changes the order of work against the shipped
game, and the queue's other users (path building, A-Life) still go to the secondary pass, so rule 2 is still
needed (believed).

## 8. Unestablished

- Which of the three views each mechanism reaches most (no run was made); the specks' source.
- The identities of most of the game library's and the renderer's shared-generator draws.
- Whether the secondary pass's jobs and the physics step draw from the shared generator themselves (the
  vision rays do).
- Threads of the Direct3D driver and the system; uninitialised memory.
- With sound off, what sound processing does each frame.
- Whether anything reads the counter value stored with render targets.

## 9. Checks

All checks are in `determinism/checks/` of the dirty workspace; `run_checks.py` runs them all (headless
Ghidra, read-only, on the shipped binaries; and the game's extracted scripts).

| Id | Claim |
|---|---|
| T1 | the nine time-stamp-counter reads by module, none elsewhere |
| T2 | the shared generator: one state, its seed, its draw, its importers |
| T3 | the counter calibration, the performance-counter timers and the profiler as the calibration's only outside reader |
| T4 | render targets store a counter reading |
| T5 | clocks and thread creation only through import tables; the C runtime's own imports |
| T6 | the frame loop's lock hand-over, sleeps, secondary pass and take-over |
| T7 | device flags' initial value; sound, physics and network work in the secondary sequence |
| T8 | the game library's queued jobs and the AI flags |
| T9 | the C runtime's per-thread `rand` |
| T10 | script `math.random` and `math.randomseed` |
| T11 | the physics library's `rand` draws, the steps the game uses |
| T12 | the noise generators' `srand` |
| T13 | the trader script's animation choice; script seeds; New Game's server options |
| T14 | the single-player direct connection skipping the synchronisation sleep loop |
| T15 | detail slots' placement seed and refresh scheduling |
| T16 | the main thread's processor binding |
| T17 | skeleton bounds refresh scheduling |
| T18 | the libraries' shared preferred base without address randomisation |
| T19 | the collision library's and the game library's threads |
| T20 | the A-Life time quantum |
| T21 | AI vision ray jitter |

## Gaps found in review

- Section 3.5 omits the network library's time-synchronisation thread: the client starts it on connection
  when the direct connection is off, and a console command that resynchronises the client starts it again;
  a New Game's direct connection skips it at connection.
- If the instruction-level control gives every counter read the same constant (not only the seeding one),
  the calibration derives a counter frequency of zero; what that does in the core library and its start-up
  log is not examined.

## Provenance

- Date: 2026-11-27. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, GOG edition, the shipped `bin\` program
  files (engine executable, core, first renderer, game, particle, sound, collision, physics, script and
  network libraries, skinning library, the C runtime) and the game scripts extracted from the archives.
- Analysis: static analysis only (disassembly and decompilation with Ghidra, headless; reading import,
  export and header tables and initial data of the binaries; reading the scripts). Nothing was executed.
