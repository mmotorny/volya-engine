# Memory

Where the engine's memory goes, by lifetime and owner; the design it follows (the original's memory architecture where it can be matched: its memory manager's pools, its files read in place, the same data loaded under the same captions; one heap, owners that hold few large blocks, no level's resources outliving it); the migration to it. Numbers are for a New Game of `l01_escape` on the Russian edition unless a level is named, and are marked measured (with where), documented (with the source) or believed. K is 1,024 bytes (the original's figures), MB 10^6 bytes (native counts), MiB 2^20 (the browser's heap).

## Goal and rule

Our memory figures are the original's, counted the same way, so they compare directly. The design follows the approved spec's functional description of the original's memory manager and figures (docs/behavior/console-diagnostics.md 2.1, 2.1.1, 2.4, 2.4.1), never its code: implementers work from that text only (docs/clean-room.md). The original's figures are hard budgets, with no headroom:

- each caption's growth of `* phase cmem` (the next caption's figure less its own; the load's end for the last);
- the end-of-load *h* (the C runtime heap's in-use bytes, equal to its last `phase cmem`) and *t* (the textures);
- the file cache's, our own figure for what the original maps ([File-backed data](#file-backed-data)).

Anything above a budget is filed as an issue and investigated. Budgets are still measured and reported, never release gates (AGENTS.md). Matching the original is what is approved; every deviation is the user's decision, case by case, recorded in [Deviations](#deviations). A figure the spec leaves unestablished is not a budget until it is established.

## Limits

- The product runs on an iPhone, where iOS kills the page's process at its per-process limit: 2,528 MB on the reference iPhone (measured: its Jetsam report, #443). Every ANGLE Metal resource and drawing buffer is attributed to that process (documented, #443), so the limit covers the wasm heaps, JavaScript, WebKit and the GPU's resources together ([The iPhone](#the-iphone)).
- A wasm memory never shrinks (documented: WebAssembly has no instruction to release memory): the heap's high-water mark is the page's for its life. Free space in it is only worth something to later allocations that fit it. The original's pools never return their blocks either (console-diagnostics.md 2.1.1).
- WebAssembly cannot map a file into its memory (documented: the core specification's linear memory only grows, by `memory.grow`; mapping is the Memory control proposal's, at phase 1 of the WebAssembly proposals list, shipped nowhere). A file's bytes reach the heap only by being copied in.
- GPU resources are released to the browser when deleted; the GPU process frees them some time after (documented for WebGL; when exactly is the implementation's).

## Measuring

| Tool | Where | What |
|---|---|---|
| `crates/volya-engine/tests/loading_memory.rs` | native, counting allocator | Each loading stage's peak above its start, what it keeps, allocations made and blocks kept; the program's data; what outlives the level. |
| `crates/volya-engine/tests/level_memory.rs` | native, counting allocator | Every shipped level's own files: the collision world's and the detail objects' heap, the `level` file's read peak, and as stored the geometry, the navigation file and the textures its surfaces name. |
| `render-level` suite's "a level frame's heap traffic" (`volya_engine::selftest::frame_memory`) | browser, the test build's counting allocator | Allocations and bytes a frame of the running level makes on the wasm heap, by part. |
| `heap_log` op (test builds) | browser | The wasm heap and the most live under each loading caption. |
| `heap_map` op (test builds) | browser | Live KiB per MiB of the heap and every live block of 256 KiB or more, by address. |
| `heap_record` op (test builds) | browser | At the load's most live: live bytes by block size and the largest blocks; every allocation that grew the wasm memory since the reset, by size and address. |
| The console's `stat_memory`, `dump_resources` and `* phase cmem` (docs/behavior/console-diagnostics.md; in the game's log) | browser, every build | *h* and `phase cmem` as the original counts them ([The allocator](#the-allocator)); *g* and *r* the game scripts' and the renderer's pools; *c* the wasm memory, the rest of 4 GiB free; *t* by the original's rule ([Textures and the GPU](#textures-and-the-gpu)); the colour targets, the programs (as vertex and as fragment shaders) and the vertex arrays (as vertex declarations). The original's other figures (*p*, *e*) have no counterpart and print 0. The file cache's figure is `level_stats`' ([File-backed data](#file-backed-data)). |
| `level_stats` op | browser | The wasm heaps (engine, audio worker); the GPU world's buffers and textures; everything the context holds (`gpu_live`), split into the level's (`gpu_level`) and what the renderer keeps across games (`gpu_kept`), and by owner (`gpu_owners`: the level, the renderer's own, the targets, the particles', the lights', the images, the videos). |
| `game-level` and `game-memory` suites' JS memory | Chrome | `performance.measureUserAgentSpecificMemory()` after the New Game (cross-origin isolated pages only, documented, MDN): each context's JavaScript, the wasm memories not among it. |
| `footprint` of the Simulator's processes | iOS Simulator | Per-process footprints; ETC2 textures there are held as RGBA8, so they don't show the device's (#443). |
| The original's log under Wine (`xtask original --new-game --quit`, docs/testing.md) | Wine | Its `phase cmem` per caption and its end-of-load `stat_memory`: the budgets ([The original](#the-original)). |

`bazel test //tests/browser:game_level_chrome //tests/browser:game_memory_chrome //tests/browser:render_level_chrome` reports the browser's numbers; the native tests with game data (`--config=measure` for meaningful counts). The budgets are judged on the browser's figures: wasm32's pointers and `usize` are 4 bytes, as in the original's 32-bit process; native's 8-byte ones make pointer-heavy data larger there, so native figures guide and the browser's are compared.

## Inventory

Native figures: `loading_memory.rs` and `level_memory.rs`, release, requested bytes (the allocator's overhead not in them). Browser figures: headless Chrome, one run of `game-level`, `game-memory` and `render-level`.

### Program: from the launch to closing the page

| Owner | Live | Blocks |
|---|---|---|
| The archive tree (`DbVfs`: a region of every name once and a node table, each directory's children a sorted run); building it peaks 7.0 MB above its start, one archive's table at a time | 1.3 MB | 18 |
| The system configuration (`Config`: one text buffer of every name, key and value; tables of sections, entries and each section's run of entry indices) | 1.2 MB | 4 |
| The string tables (`StringTable`: one buffer of every key and text, the entries sorted by key) | 1.2 MB | 2 |
| The particles' effect library (`particles.xr`, `Library`: one buffer of every name, a table of every effect's actions and one of every group's members), read once at launch (`GameData`); its blocks are the boxed `source` actions | 1.4 MB | 1,095 |
| The UI's data | 0.2 MB | |
| The renderer and its programs; the menus' textures and video frames | about 6 MiB (believed) | |
| The audio worker's wasm instance | 2 MiB | |
| JavaScript: the audio worker 2.9, the page 1.4–1.8 (and its canvas 0.9), the engine worker 1.6, shared 0.7 MiB | 8 MiB | |

All measured natively but the last three (`level_stats`, JS memory). The program's data holds 4,087 of the heap's blocks before a game (452 with the archive tree alone). The compressed sound files are in JavaScript, not a wasm heap: the engine worker reads each into a JavaScript array a piece at a time (256 KiB) and the audio worker keeps it (#491). Before the first New Game the engine's heap is 16–17 MiB with 11 live (measured, `game-level`).

### Session: a game, from its New Game to its end

The offline simulation's registry (6.3 MB, 54,735 blocks), the game graph and paths (1.8 MB) and the player character, made under `st_server_connecting`: 15.6 MB with the current level's navigation (below), 83,764 blocks, from 537,041 allocations (measured, native). Of the registry, the slot table of 8,646 objects of 392 bytes is about 3.4 MB in one block (the rare large kinds, among them the crows, the visual anomalies and the trader, and every skeleton's saved bone state, which no New Game record has, are boxed, #498); the records' own strings and lists (every kind's names, section, custom data, restrictions, character fields) 2.4 MB in 51,204 blocks, the section and the name alone 17,282 of them; travel 0.2 MB in 2,698. The switch predicates' inputs hold no copy of a record's names (#485). The script runtime's state joins this lifetime when the game's scripts run (`lua_alloc` is Rust's allocator, docs/architecture/scripts.md).

### Level: from its loading to leaving it

| Owner | `l01_escape` | The largest level |
|---|---|---|
| The collision world (vertices, triangles, each triangle's material and sector in 2 bytes (#490), tree) | 29.4 MB in 4 blocks (largest 10.5 MiB) | 44.5 MB, `l11_pripyat`; 39.4 `l10_radar`, 34.4 `l07_military` |
| The navigation positions (6 of each vertex's 21 bytes) | 7.1 MB, one block (`level.ai` 12.5 MB) | `l01_escape`'s `level.ai` is the largest |
| What the `level` file's stages keep (the shader table; the static draw list and spheres) | 3.4 MB | |
| The detail objects (slot grid, compact, and cache) | 5.6 MB | 11.9 MB, `l12_stancia` and `l12_stancia_2` |
| The objects' models, weather, the level's sounds, with the objects' CPU geometry until uploaded (12.3 MB in 2 blocks: every detail window's indices, object-detail-level.md 4) | 13.9 MB, 6,744 blocks | |
| The load in all at its end, the session included | 71.7 MB, 111,983 blocks above the program's data | `l11_pripyat` about 70 MB (believed: `l01_escape`'s with its own collision, details and navigation file) |

Measured natively. After the level is dropped, nothing of it outlives it: 0.0 MB and 3 blocks above the load's start (29.4 MB in 7 blocks while the collision world's arrays were kept as spares for the next world, before #688).

### Loading transients

Each stage's peak above its start and what it keeps, measured natively (MB): `st_server_connecting` 15.6 and 15.6 (the spawn file and the graph streamed, #403); `st_opening_stream` 8.3 and 7.9 (the `level` file streamed, #483: 12.6 read whole; `l11_pripyat` 8.2, 12.9 whole); `st_loading_cform` 30.3 and 30.1; `st_loading_spatial_db` 6.2, freeing 1.9 (it keeps the progressive visuals' detail windows, 0.7); `st_loading_details` 5.6 and 5.6; `st_client_spawning` 19.7 and 13.9; `st_loading_textures` 2.4 and 0.3. The load peaks 77.5 MB above its start against 71.7 kept. By count the transients are the many: `st_client_spawning` makes 2.52 million allocations to keep 6,744 blocks, `st_server_connecting` 537,041 to keep 83,764.

In the browser the level's preparation adds the texture read buffer (one file, up to 4.5 MB) and the band being transcoded (without BC); WebGL copies what is written at each call (docs/architecture/rendering.md).

### Frame

A frame of the running level, on the wasm heap (the `render-level` suite, 600 frames after 60): 3 allocations and 1.6 KB (at most 103): the update (the detail objects' pass among it) 2.6 and 1.6 KB, the sounds 0.0, the scene 0.0, the draw list 0.1 and 0.0 KB (60 allocations and 69.3 KB before #516); live memory grew 0.51 MB over the 600 frames. What is left are the effects' particle lists and the detail cache's lists growing toward their most, and the child effects that births and deaths start (traced natively; the growth believed theirs, #679). The passes' lists (the camera pass's dynamic items among them), the lights', glows' and simulation's, the effects drawn and the particle sprites' scratch are kept and cleared (#488). Frusta hold their planes inline, and placing an effect reuses its actions' boxes. What a frame's scene shares (the camera pass's lists, `Seen`; the detail objects' frame; the particle draws, their sprites' vertices in one list; the light passes; the glows) is the level's, held in an `Arc` and refilled in place once the frame drawn with it lets it go; the presenter keys what it derives from the pass by the pass, not by holding it. The draw list is given back once drawn (`Presenter::recycle`) and its lists, the particle batches' vertex lists among them, serve the next frame's (#516). In the browser the heap stays flat while the level runs (88–89 MiB before the intro, while it plays and after 20 s of walking), so the frame's blocks reuse their own holes.

On an iPhone the heap grew while a level ran (74.9 → 78 MiB about 110 s into the first game, 82 → 86.6 about 180 s into the second; its diagnostic log): at the level's first music track (`music\amb01`, whose files no longer pass through the heap, #491; in the iOS Simulator the most live went 62.2 → 69.8 MiB at its start before and after that change) and under the in-game menu, which holds 5.7 MB more while it shows (measured there). Its two videos' read-ahead windows and decoders are made at each opening and freed at its closing: 6.1 MB live while it shows, 7.6 at most, the heap not grown by an opening (measured, `game-memory`, headless Chrome and the iOS Simulator). Kept for the program from the title menu instead, an opening took 0.5 MB, but the heap after the first New Game grew by 4–5 MiB (89 → 93–94 MiB, the relaunched game's 94 → 99), as the load no longer reused their space; not taken (#517, rule 4).

### The GPU

| Owner | `l01_escape` | The largest level |
|---|---|---|
| The level's textures, as stored (BC) | 127 MiB (the objects' 22.5 MiB of it; 458 textures) | Its surfaces' alone: 91.2 MB `l07_military`, 83.3 for `l01_escape` |
| The level's buffers (geometry, the objects', draw records) | 70.8 MiB (the objects' 11.3: every detail window's indices among them, 16-bit, each draw reading from one of the vertex arrays' first vertices, 19.2 with 32-bit indices, #610; the static world's 12.7) | `level.geom` 109.7 MB `l12_stancia_2` against 57.0 |
| The level in all (`gpu_level`) | 462 textures, 143 MiB; 60 buffers, 79 MiB; 51 vertex arrays | |
| The video mode's targets (`targets`): the frame, the offscreen frame, the supersampled target with its distortion and depth, the world's depth | 5 textures: 19 MiB at 1024 × 768, 22 at 1280 × 720, 37 at 1448 × 1086, 52 at 2868 × 1320; 19 again at 1024 × 768 after a game at 2868 × 1320 | |
| The renderer's own (`renderer`, `particles`, `lights`) | 7 textures, 2 MiB; 9 buffers, 2 MiB (the particles' 2 MiB of it) | |
| The menus' images and the videos (`images`, `video`), while drawn | in the title menu 6 textures, 2 MiB, and 8 textures, 3 MiB; in the level 1 texture, 1 MiB | |
| The canvas's drawing buffers | up to 3 at canvas size, 15 MB each at 2868 × 1320 (documented, #443) | |

Measured in headless Chrome (`level_stats`' `gpu_owners`, `game-level`, `game-memory` and `render-level`; the targets 30 MiB at 1024 × 768 in the iOS Simulator, the rest as in Chrome) and natively (`level_memory.rs`, as stored). On the device BC is held as stored where Metal has BC (documented, #443). In the iOS Simulator a level of `l01_escape` takes its Metal host to 1.8 GB, nearly all textures held as RGBA8 there (measured, #443); ETC2 too is expanded there (measured), so the Simulator's footprint does not show the device's. What else WebKit and ANGLE hold: [GPU and JavaScript memory](#gpu-and-javascript-memory).

## Heap minus live

Headless Chrome, `game-level` and `game-memory`, three runs each (MiB, heap/most live; the range over the runs):

| | Before the first New Game | First New Game | Title menu after quitting | A New Game after relaunching the game, at 2868 × 1320 | Second New Game after quitting to the title menu | Third |
|---|---|---|---|---|---|---|
| Heap / live | 16–17/11 | 88–89/81 | 88–89/12 | 88–95/81 | 92–95/81 | 92–95/81 |

The level, once loaded, holds 67 MiB live. The load's most live comes at its end, under `st_loading_textures` and `st_client_synchronising` (58 → 81 MiB): the objects' CPU geometry with every detail window's indices is among it until the world uploads (the 8.0 MiB index list and the 4.0 MiB vertex list, #528, #653). The heap's last two growths come in that stage (9.6 MiB asked for at 73.6 MiB, then 4.0 MiB) (the 9.6 MiB block's owner: #666).

After quitting, 12 MiB stay live: the program's data and the menus' (40 MiB with the collision world's spare arrays, before #688). The second New Game's collision world fits the space the first's left (the heap stays at 89 MiB through `st_loading_cform`; `game-memory`, two runs); its heap grows once, under `st_loading_textures`, by 5.3 MiB (89 → 94 MiB; 4.0 MiB, 89 → 93, with the spares kept), and the third New Game's not at all.

Where the gap comes from:

- **Large blocks meeting split free space.** The heap grows by 26 MiB under the collision stage for 24 MiB more live: its arrays (up to 10.5 MiB each) find no hole that size in the free space the earlier stages left (measured, `heap_log`, `heap_record`'s growths; #373). It is the only stage that grows the heap by about what it keeps.
- **Loading transients among long-lived blocks.** `st_client_spawning` (2.52 million allocations, 6,743 kept) and `st_server_connecting` (537,041, 83,764 kept) leave their kept blocks scattered through the space their temporaries used, so the next large block needs new memory above them. At the load's most live, blocks under 64 KiB hold about 7 MiB, blocks of 1 MiB or more 53 MiB (measured, `heap_record`).
- **The preparation's buffers.** The texture read buffer and the band buffer arrive last, above the level's data.
- **A later New Game grows the heap by 4–6 MiB once** (in the relaunched game's or the second), the next none: an allocation of the texture stage finds no hole that size in the first game's layout (measured, `heap_record`'s growths).

The program's data leaves no split free space before a game (`heap_map`, #432).

## The original

Measured (#667): its New Game of `l01_escape` run unmodified under Wine three times, on the Wine image before #590's (that one loads no graphics driver, #671), its log only; every `phase cmem` was the same in all three, `stat_memory`'s within 0.1%. A caption's figure is what its stage keeps, not its peak (the log gives none). Two amounts of about 527 K move between captions from run to run in the spec's runs (console-diagnostics.md 2.4.1), so a difference of that size at one caption is not a finding. Ours today: `loading_memory.rs`, native, requested bytes (#667); the browser's per caption come with the migration's first step.

| *n* | Caption | The original keeps, K | Ours keeps, K | What must change |
|---|---|---|---|---|
| 1 | `st_prefetching_objects` | 85,047 | 0 | Load the 394 models of `prefetch_visuals_single` (`config\prefetch\prefetch_single.ltx`) and their textures here, kept while the game type stays (a later New Game of the type skips the stage, loading-screen.md 5); the objects spawned later take them from there. Its *m* (28,221 K of motions and skinned models shared) follows from sharing them. |
| 2 | `st_server_starting` | 205 | 0 | |
| 3 | `st_loading_alife_simulator` | 0 | 0 | |
| 4 | `st_creating_new_game` | 32,164 | 222 | The offline simulation's registry, the game graph and the character are made here, from `all.spawn` held whole and read in place ([File-backed data](#file-backed-data)). |
| 5 | `st_server_connecting` | 0 | 15,198 | Nothing made here (moved to 4); the navigation positions move to 14. |
| 6 | `st_client_connecting_to` | 8 | 0 | |
| 7 | `st_opening_stream` | 0 | 7,705 | The `level` file held whole and read in place (counted in *c*); what is made from its chunks is made under the caption whose stage reads them (the shader table under 9, the static draw list and spheres under 10 and 11). |
| 8 | `st_loading_cform` | 72,669 | 29,409 | |
| 9 | `st_loading_shaders` | 1,400 | 0 | The shader table (from 7). |
| 10 | `st_loading_geometry` | 353 | 366 | |
| 11 | `st_loading_spatial_db` | 4,297 | −1,819 | |
| 12 | `st_loading_details` | 10 | 5,423 | The compact slot grid (a leaner structure in place of the mapped `level.details`, rule 5) counted in the file cache's figure, not *h*. The cache's 0.2 MB stays (where the original holds its cache is unestablished). |
| 13 | `st_loading_sectors_portals` | 143 | 0 | |
| 14 | `st_loading_ai_objects` | 0 | 3 | The navigation positions (moved from 5; a leaner structure in place of the mapped `level.ai`, rule 5) and `level.gct` counted in the file cache's figure, not *h*. |
| 15 | `st_client_spawning` | 4,708 | 13,551 | The objects' models come from 1 (one not prefetched is loaded here); their CPU geometry, every detail window's indices among it (12.3 MB), freed once uploaded, as the original keeps it in Direct3D's buffers. |
| 16 | `st_loading_textures` | 1 | 338 | The texture file read into the file cache, not the heap. |
| 17 | `st_client_synchronising` | 0 | 0 | |
| | The load, its first caption to its end | 201,005 | 70,396 | |
| | *h* at the load's end | 227,120 | 75,602 | |
| | *t* at the load's end | 260,798 | 148,764 (the renderer's texture bytes, headless Chrome) | Counted by the original's rule; the prefetched models' textures added (the original's 865 base textures against 631 without prefetching, 2.4.1). |

Where ours keeps less than the original (8, 11) it stays (rule 5): the budget is a ceiling. Beside them at the load's end the original writes the committed address space 849,680–849,744 K (the whole 32-bit process, its program, libraries and driver; ours, the wasm memory, 101,760 K today), the process heap 400,268–400,269 K and the game scripts' pool 25,762–25,784 K (measured, #667).

## Design

### Rules

1. **One heap, the original's memory manager in front of it**: our global allocator serves small requests from fixed-size pools and large ones from Rust's `System` allocator (dlmalloc over the wasm memory), as the original's memory manager does over the C runtime's heap ([The allocator](#the-allocator)). No heap per lifetime: the experiment measured it to grow the heap ([Rejected](#rejected-heaps-by-lifetime-401)).
2. **What the original maps goes to the file cache; the rest streams.** A file the original maps and reads in place is held, whole or by parts, in the file cache, which has a budget and a figure of its own ([File-backed data](#file-backed-data)). Other files are read as streams into their final form, the smallest that serves their readers.
3. **An owner of many long-lived things holds them in a few large blocks** (a region): one text buffer and tables of ranges or indices into it, sized once from what is read and never grown afterwards. Plain `Vec`s on the one heap, reached by index, freed whole with their owner: what the pools can't serve well (sizes over 862 bytes, data read as one run) and what keeps the block count low.
4. **No owner keeps a spare across loads.** The collision world is freed with its level, as the original's; so is every other owner's data. (Kept between loads, the texture file buffer took the space the geometry's upload reused, so the first game's heap grew by more than the second saved: 76–82 against 74 MiB, #487.)
5. **A leaner structure is an approved kind of deviation.** A better algorithm or data structure than the original's, with the same observable behaviour, is taken wherever found: it stays within the original's budget for the same data (the caption's growth, or the file cache's share of the file it replaces) and its behaviour is checked against the specs. Taken so far: the collision world (29.4 MB kept against the original's 72,669 K; each triangle's material and sector in 2 bytes, #490), the detail objects' compact slot grid, the navigation positions (6 of each vertex's 21 bytes).
6. **The same data under the same caption.** Each stage loads and keeps what the original's does under that caption ([The original](#the-original)), so each caption's growth compares with the original's.
7. **No allocation in a running frame.** Per-frame data goes into buffers its owner keeps and clears (`Vec::clear` keeps capacity), taken from the renderer's pool, reported as *r* ([The allocator](#the-allocator)).
8. **Nothing of a level outlives it.** A level's heap data and GPU resources are released when the level ends, before the next load starts. The prefetched models' are the game type's, not the level's.

### The allocator

The global allocator, natively and in wasm, follows the original's memory manager (console-diagnostics.md 2.1.1):

- A request of up to 862 bytes (alignment at most 16) goes to one of 54 pools of fixed block sizes 16, 32, … 864 bytes: *n* bytes to the pool of 16 × (⌊(*n* + 1) / 16⌋ + 1). A pool takes its memory from `System` in chunks of 16 KiB plus 22 bytes and never returns them; a freed block goes back to its pool. A reallocation within the same pool stays in place.
- A larger request (or one aligned over 16) is one `System` block of the request plus 23 bytes.
- `phase cmem` and *h* count what the original's heap walk counts: every pool's chunks whole (16,406 bytes each, used or free) and every large block with its 23 bytes; `System`'s own overhead, as the C runtime heap's, not in it. The test ops keep their requested-byte counts beside (`heap_log`, `heap_map`, `heap_record`, the frame's traffic), as do `loading_memory.rs` and `level_memory.rs` beside *h*.
- Two pools sit outside *h*, as the original's are committed apart from its heap (console-diagnostics.md 2.1, 2.1.1): the game scripts' (the script runtime's allocator, *g*) and the renderer's (the lists a frame's scene, passes and draw list grow and refill, *r*: the original's is 0 at the load's end and a few hundred K once frames are drawn). Each takes its memory from `System` in segments of its own and reports its bytes in use. *e* prints 0: no shader scripts run.
- The pools are locked (native tests run threads; the engine's worker is single-threaded, so the lock is never contended there).

What stays: the one heap, the regions and their owners (rule 3), the counting wrapper's place. What goes: the collision world's spares (#357). What changes: small blocks no longer reach dlmalloc one by one, so the stages' hundreds of thousands of small temporaries (`st_client_spawning`'s 2.52 million allocations) fill pool chunks rather than splitting dlmalloc's free space between large blocks (believed: fewer holes the large blocks can't use, as [Heap minus live](#heap-minus-live) describes); a pool's high-water mark is kept for the page's life and counted in *h*, as in the original.

### File-backed data

The original reads in place, from views of the mapped archives, the files it keeps open while their data is in use, and copies none into its heap; their views count in *c*, not *h* (console-diagnostics.md 2.1.1, 2.4.1). A mapping is a cache of the file's pages. As wasm can't map a file ([Limits](#limits)), the file cache stands for it: a file is held whole in memory where its budget affords it, otherwise as a least-recently-used cache of its parts, and read in place either way. It is memory the original's figures don't show, so it has a budget and a figure of its own; it is not in *h* and not left unnamed in *c*.

| File | Bytes (`l01_escape`) | Open in the original |
|---|---|---|
| `level` | 4,094,604 | from `st_opening_stream` to the level's end (believed: the spec says it is read in place, not when it is closed) |
| `all.spawn` | 4,474,073 | while `st_creating_new_game` reads it (how long the original keeps it open is unestablished) |
| `level.ai`, `level.gct` | 12,505,535; 3,573,054 | from `st_loading_ai_objects` to the level's end |
| `level.details` | 8,392,388 | from `st_loading_details` to the level's end |
| Any other file read from the archives (the texture files, the models', the sounds', the configuration's) | its size | while it is read |

- **Budget**: at any moment, the bytes of the files the original has open then (for `l01_escape` in a level 28.6 MB: `level`, `level.ai`, `level.gct`, `level.details`; during the load, the file being read beside them). A leaner structure that replaces a mapped file (rule 5: the detail slot grid, the navigation positions) counts here against that file's share, not in *h*. Files are held whole, which the budget affords by its definition; a file is held by parts only where a measured hard limit doesn't afford it whole, with a smaller budget of the cache's size (a deviation, [Deviations](#deviations)).
- **Reported**: the file cache's bytes, by file, and its most since the load began, in `level_stats` (`file_cache`) and by the game suites against the budget at each caption and at the load's end. The console's lines stay the original's: the figure is not added to `stat_memory`, and *c* remains the wasm memory.
- Natively the same cache is used (one path; the native tests count *h* and the cache the same way).

### Textures and the GPU

*t* is counted by the original's rule (console-diagnostics.md 2.1): each texture the renderer holds by name, lightmaps included, render targets not, adds its file's byte length reduced to a quarter for each level step the texture quality drops for it (texture-quality-filtering.md); not the bytes uploaded, which differ with the target's format (BC, or ETC2 on the iPhone). Where the spec leaves the rule open (a single-level texture's figure) the figure is the file's size until it is established. The GPU's resources are otherwise outside *h* as in the original, whose vertex and index buffers and textures are Direct3D's (2.1.1); ours keep no CPU copy ([GPU and JavaScript memory](#gpu-and-javascript-memory)), so the original's process-heap copies (*p*) have no counterpart and *p* prints 0.

### Lifetimes and their homes

Nesting: the program holds game types; a game type holds sessions; a session holds levels; a level holds its loading and its frames. Each lifetime's home is on the one heap, allocated in that order, so the longer-lived data sits lower and the shorter-lived above it.

| Lifetime | Home | Size, `l01_escape` (largest level) |
|---|---|---|
| Program | Regions made at launch, before anything else: the archive index (#300), the configuration (#299), the string tables and the effect library (#484; the library read once at launch rather than with each level: our choice, as `gamemtl.xr` is read with the level). | 5.3 MB in about 4,100 blocks |
| Game type | The prefetched models and their GPU resources, made under `st_prefetching_objects` and kept until the game type changes. | the original's 85,047 K |
| Session | The offline simulation's registry (a slot table, each record's strings its own); the game graph; the character; the script runtime's state. | 8.7 MB (16 with the navigation) |
| Level | Its files in the file cache ([File-backed data](#file-backed-data)); its owners' blocks: the collision world's arrays, the detail cache, the objects' sounds and effects and the models not prefetched; its frames' lists in the renderer's pool. All freed when it ends (rule 8). | about 54 MB (about 62, `l11_pripyat`; believed), and the file cache's 28.6 MB |
| Loading | Each reader's own stream buffer: 64 KiB for the graph and the geometry's read; 2 MiB pieces for the geometry's upload and the index runs; in the browser one band buffer per preparation. A file being read is in the file cache. Stage temporaries freed by the stage's end. | under 10 MB above what each stage keeps |
| Frame | Buffers kept by the level (its scene's lists among them), the sounds and the presenter (the draw list's), from the renderer's pool, cleared each frame. | held for the level |

There is no program scratch shared by the loaders: each owns its stream buffer for its read.

### Containers

- Containers are `std`'s: `Vec` for regions and tables, `HashMap` (std's is hashbrown) where a map is needed. No container takes a custom allocator, so `allocator-api2`, `bumpalo` and the unstable `allocator_api` are not used.
- Strings that many records repeat (section names, file paths, model names) are stored once in a region and referred to by index or range (`u32`), not as `String`s per record. The registry's records keep their own strings (#485).

### What stays on the general heap as it is

Small, short-lived or rare allocations: the UI's state, menus, the console, errors and diagnostics, a stage's temporaries that are small; the audio worker's own instance (2 MiB, its sounds in JavaScript). They are a few MiB and either short-lived or allocated where no large block follows them. The script runtime's blocks and the frame's lists are in their own pools, *g* and *r* ([The allocator](#the-allocator)).

### Budgets

Reported, never gated; no headroom:

| What | Budget | Reported by |
|---|---|---|
| Each caption's growth of `phase cmem` | the original's, K ([The original](#the-original)) | the game suites (browser); `loading_memory.rs` (native) |
| *h* at the load's end | 227,120 K | the game suites |
| *t* at the load's end | 260,798 K | the game suites |
| The file cache, at each caption and the load's end | the bytes of the files the original has open then ([File-backed data](#file-backed-data)) | the game suites; `loading_memory.rs` |

Reported beside them, not budgets: each stage's peak (the original's log gives none); the wasm memory; *g* and *r*; *h* at startup and in the title menu against the original's 6,009 and 26,449 K (console-diagnostics.md 2.1, measured examples of one run).

Targets no tool reports against: a frame's allocations in a running level 0; JavaScript 12 MiB (Chrome).

### Deviations

Matching the original is what is approved; each deviation is the user's decision, case by case, recorded here. A choice leaner than the original's for the iPhone or a lower-spec device (other than rule 5's) is weighed only against a measured hard limit: so far the reference iPhone's 2,528 MB per process (measured, #443); another device's limit is measured when it is targeted.

| Deviation | Decision |
|---|---|
| The file cache: what the original maps is copied into memory, whole or by parts. | Approved (wasm can't map a file): extra memory with its own budget, reported ([File-backed data](#file-backed-data)). |
| Leaner structures with the same observable behaviour (rule 5). | Approved as a kind: within the original's budget for the same data, behaviour checked against the specs. |
| *p* (the process heap) prints 0. | Approved: we have nothing equivalent (no CPU copy of GPU resources, no system libraries' heap). |
| The sounds: the audio worker's heap and the compressed sound files in JavaScript, outside *h*. | Pending: where the original holds its sound data is to be measured from a run with sound on and its volumes muted (the spec's runs used `-nosound`, console-diagnostics.md 2.1.1); matched once known. |

### Level change and the second New Game

A level change goes through a save and a load in the original (docs/behavior/offline-simulation.md 6.1); a New Game after a game is a game's end and a load. Both run in this order:

1. The level ends whole: its screen, its sounds (with the collision world they share, #451), the presenter's world (released with the first frame drawn without it) and any preparation in progress, every GPU resource of the level and its heap data, the collision world's arrays among them (rule 8).
2. A game's end also ends the session (the registry and graph go; the next session's blocks fit their holes, as they are sized from the same files).
3. The next load starts.

So the heap after any sequence of levels is what the program, the game type, the session and the largest level met need, with the holes their layouts leave (a second game's layout added 5 MiB once, measured above); the GPU holds one level at a time.

### GPU memory: what is owned and released when

The renderer's own WebGL 2 layer (docs/architecture/rendering.md) creates and deletes its buffers, textures and vertex arrays, and no staging copy lives in the heap. It counts every resource it makes by its owner (`gl::Owner`, given with `Gl::owning`; a resource made outside it is the level's): `level_stats` reports each owner's (`gpu_owners`), the level's as `gpu_level` and the rest as `gpu_kept` (#512, #482).

| Lifetime | Resources | Released |
|---|---|---|
| Game (the owners `renderer`, `particles`, `lights`, `images`, `video`; `gpu_kept`) | The programs; the UI's vertex buffer; the particles' and the light passes' buffers, grown to the largest need; the menus' textures, by path, while drawn; video textures while their video plays. | A menu texture when no sprite draws it (`Presenter`'s images); a video's when it ends; the rest at the game's normal exit. |
| Video mode (`targets`; `gpu_kept`) | The video mode's frame and the offscreen frame, the supersampled target with its distortion and depth textures, the world's depth. | Each when the size it is needed at changes, before the new one is made. |
| Level (`level`; `gpu_level`) | The world's geometry and draw records, its surfaces' and objects' textures, the detail models, the light passes' attenuation, glow and presentation noise textures. | When the level ends (a quit, a game's end, a level change), before the next load's preparation starts. |
| Loading | Textures and geometry prepared for the load in progress. | Given to the level when it is made; freed when the load is abandoned. |

At the game's normal exit (the menu's quit, `quit`, Alt+F4) the app has the platform release its renderer, presenter and samplers (`App::set_device_release`, `gpu::release`) before the shutdown's resource counts (docs/behavior/console-diagnostics.md 2.5), as the original destroys its graphics device first: the context stays, and the renderer is made again when the game is launched. The GL layer keeps every live texture and program by id with its name (`Gl::remaining`: a file's texture by its path, a program by its stages' module and entry points, another texture by its owner), which that shutdown lists after a count that is not 0.

A level's world, its effect and glow shaders and its detail models go with the first frame drawn without a world (`Presenter::draw_list`): the title menu after a level holds no GPU world (measured, `level_stats` in `game-memory`), and a New Game after relaunching the game holds of its level what the first did. The light passes' attenuation textures and the presentation noise go with it (#602): by owner, the title menu after a level holds what it held before one (measured, `gpu_owners` in `game-memory`: in headless Chrome; in the iOS Simulator, before #602, it held the same plus those 4 textures of 1 MiB). What the Simulator's Metal host keeps after a level, 200–300 MB above the title menu before one (#436), is therefore the browser's (WebKit, ANGLE or the Simulator's Metal), not ours.

What the platform prepares while a level loads belongs to that load (`App::load`, a number per New Game load, kept by the level it made): the texture task stops and reports nothing once its load is no longer the current one, and the prepared textures and geometry of a load quit or replaced are freed on the next frame (`platforms/web/src/textures.rs`, #262). `game-memory` quits a New Game about 100 textures into its preparation: the GPU holds 28 MiB of textures in the title menu after it against 30 before it (headless Chrome).

Data written each frame is never written into a buffer the GPU may still be reading: ANGLE's Metal backend copies such a buffer whole (documented, #443).

## The iPhone

The real limit is the process's: 2,528 MB on the reference iPhone (measured, Jetsam, #443), the wasm heaps, JavaScript, WebKit and the GPU's resources together. The death it recorded was ANGLE's index rewriting, with the wasm heap at about 104 MiB (fixed, #443), not the heap. The 110 MiB wasm heap figure (`WASM_HEAP_CAP`, tests/browser/game.js) was a guard from a measured heap, not a limit, and is replaced by the original's figures ([Budgets](#budgets)).

Expected after a New Game of `l01_escape` under this design (believed, from the figures above): the wasm memory about 280–300 MiB (*h* 222 MiB, the file cache's about 28 MiB with a file being read, *g* about 25 MiB as the original's, and dlmalloc's holes); the GPU about 370 MiB (textures by *t*'s files, BC as stored, about 255 MiB with the prefetched ones; the level's buffers 79 MiB; the targets 52 MiB at 2868 × 1320) and the drawing buffers up to 45 MB; JavaScript and WebKit about 120 MB (the Simulator's WebContent held 199–222 MB in a level with a heap of about 90 MiB, #443). About 0.9 GB in all: a margin of about 1.6 GB to the limit, measured on the deployed app once the migration lands (the real device's footprint is the user's report; the Simulator's doesn't show it, #443).

## Migration

Each step is its own issue, filed after this design is approved, in this order, measured before and after (`phase cmem` per caption, *h*, *t*, *c* and the wasm memory in `game-level` and `game-memory`; `loading_memory.rs`), with any figure past its budget filed.

1. **Report against the original's figures.** The game suites compare each caption's `phase cmem` growth and the end-of-load *h* and *t* with the original's (no headroom); `loading_memory.rs` the same natively; `WASM_HEAP_CAP` and the per-stage budgets go.
2. **The original's memory manager as the global allocator** (native and wasm): the 54 pools, the 16 KiB chunks, the 23 bytes; `phase cmem` and *h* counted as the original's.
3. **The collision world freed with its level**: its spares go (#357).
4. ***t* by the original's rule.**
5. **The file cache**: what the original maps held whole or by parts outside *h*, its budget and its figure reported (`level_stats`, the game suites); the texture files' read first.
6. **The `level` file in the file cache, read in place**, its chunks' data made under the caption that reads them (the shader table under `st_loading_shaders`).
7. **The detail slot grid counted in the file cache** against `level.details`.
8. **The navigation positions made under `st_loading_ai_objects`** and counted in the file cache against `level.ai`, with `level.gct`.
9. **The new game made under `st_creating_new_game`** from `all.spawn` in the file cache: the registry, the game graph, the character.
10. **Prefetching under `st_prefetching_objects`**: the 394 models of `prefetch_visuals_single` and their textures, kept while the game type stays; the objects spawned take them; the objects' CPU geometry freed once uploaded.
11. **The game scripts' pool**: the script runtime allocates from a pool of its own, outside *h*, reported as *g*.
12. **The renderer's pool**: the frame's lists from a pool of their own, outside *h*, reported as *r*.
13. **Leaner structures**: a survey of the owners against the original's figures for opportunities of rule 5's kind, each found filed as its own issue.

Pending, not a step: where the original holds its sound data (a run with sound on and volumes muted).

The open issues weighed before this design (#486 the objects' models as regions, #489 kept scratch buffers, #679 a frame's last allocations, #405 sorted `Vec`s for `BTreeMap`s) are taken where they match the original or are rule 5's kind; any other is the user's decision.

## GPU and JavaScript memory

JavaScript holds 8 MiB beside the wasm heaps in headless Chrome ([Program](#program-from-the-launch-to-closing-the-page)); WebKit has no `measureUserAgentSpecificMemory()` (documented, MDN's compatibility data: Chromium only), and the Simulator's WebContent process (wasm, JavaScript and WebKit together) held 199–222 MB in a level (measured, #443). What the browser holds for WebGL 2 beyond our resources (#443; WebKit's and ANGLE's source read with `gh api`, the iOS Simulator measured with `tests/browser/webgl-memory.html` and `footprint`):

- ANGLE keeps no CPU copy of buffers on Apple GPUs (its shadow buffers are for Intel), nor WebKit of buffers or textures (documented).
- A buffer made without data is zero-filled from a CPU scratch buffer of its size in the GPU process: a 64 MiB buffer took the Simulator's GPU process 128 MB at first, 69 MB later; a buffer written while the GPU uses it is given a whole new copy (`BufferMtl::setSubDataImpl`, documented): written again in 2 MiB pieces after a draw, the 64 MiB buffer held 62 MB more (measured). An 8 MiB buffer written 1 MiB a frame and drawn held no more over 600 frames, nor a 16-bit index buffer drawn from an odd first index (measured).
- With WebGL's last-vertex provoking convention, ANGLE's Metal backend rewrites the indices of every draw whose program has a flat varying into a buffer of its own, held until the command buffer completes (documented); a whole view's draws in one submission killed the iPhone (measured, #443). The context uses the first-vertex convention (`WEBGL_provoking_vertex`).
- The context is made without depth or stencil (`gpu::make_webgl_context`): WebKit gives the default framebuffer a canvas-size depth buffer otherwise (Depth32Float on Metal, 15 MB at 2868 × 1320; documented), which nothing uses.

## Rejected: heaps by lifetime (#401)

The design first approved (#399) gave the level, loading and the frame each a heap of its own beside the general one: an instance of dlmalloc over segments of 64 KiB pages taken from the general heap (branch `memory-heaps-experiment`, commit e6135e9, not merged). In headless Chrome the heap after the New Game grew in every variant: 119–122 MiB against 106 for the general heap alone, 130–132 against 110 after the second (one run each). Each heap needs its segments whole, so the level's blocks moved from the general heap's holes to new memory at its top, and no heap's free space served the others. The regions of rule 3 differ: an owner's data in a few blocks on the one heap, sized from its files, so they cut the block count instead of moving it.
