# Memory

Where the engine's memory goes, by lifetime and owner; the design it follows (one heap, owners that hold few large blocks, no level's resources outliving it); the open steps. Numbers are for a New Game of `l01_escape` on the Russian edition unless a level is named, and are marked measured (with where), documented (with the source) or believed. MB is 10^6 bytes (native counts), MiB 2^20 (the browser's heap).

## Limits

- The product runs on an iPhone, where iOS kills the page's process at its per-process limit: 2,528 MB on the reference iPhone (measured: its Jetsam report, #443). Every ANGLE Metal resource and drawing buffer is attributed to that process (documented, #443), so the limit covers the wasm heaps, JavaScript, WebKit and the GPU's resources together.
- A wasm memory never shrinks (documented: WebAssembly has no instruction to release memory): the heap's high-water mark is the page's for its life. Free space in it is only worth something to later allocations that fit it.
- GPU resources are released to the browser when deleted; the GPU process frees them some time after (documented for WebGL; when exactly is the implementation's).
- Budgets are measured and reported ("OVER BUDGET" notes in `loading_memory.rs` and the game suites), never release gates.

## Measuring

| Tool | Where | What |
|---|---|---|
| `crates/volya-engine/tests/loading_memory.rs` | native, counting allocator | Each loading stage's peak above its start, what it keeps, allocations made and blocks kept; the program's data; what outlives the level. |
| `crates/volya-engine/tests/level_memory.rs` | native, counting allocator | Every shipped level's own files: the collision world's and the detail objects' heap, the `level` file's read peak, and as stored the geometry, the navigation file and the textures its surfaces name. |
| `render-level` suite's "a level frame's heap traffic" (`volya_engine::selftest::frame_memory`) | browser, the test build's counting allocator | Allocations and bytes a frame of the running level makes on the wasm heap, by part. |
| `heap_log` op (test builds) | browser | The wasm heap and the most live under each loading caption. |
| `heap_map` op (test builds) | browser | Live KiB per MiB of the heap and every live block of 256 KiB or more, by address. |
| `heap_record` op (test builds) | browser | At the load's most live: live bytes by block size and the largest blocks; every allocation that grew the wasm memory since the reset, by size and address. |
| The console's `stat_memory`, `dump_resources` and `* phase cmem` (docs/behavior/console-diagnostics.md; in the game's log) | browser, every build | The wasm heap's live bytes (requested, as `loading_memory.rs` counts them; its stage lines give the same figure natively) as the memory manager's and the C runtime heap's; the wasm memory as the address space's committed part, the rest of 4 GiB free; the renderer's textures no framebuffer draws into, their bytes, the colour targets, the programs (as vertex and as fragment shaders) and the vertex arrays (as vertex declarations). The original's other figures have no counterpart and print 0. |
| `level_stats` op | browser | The wasm heaps (engine, audio worker); the GPU world's buffers and textures; everything the context holds (`gpu_live`), split into the level's (`gpu_level`) and what the renderer keeps across games (`gpu_kept`), and by owner (`gpu_owners`: the level, the renderer's own, the targets, the particles', the lights', the images, the videos). |
| `game-level` and `game-memory` suites' JS memory | Chrome | `performance.measureUserAgentSpecificMemory()` after the New Game (cross-origin isolated pages only, documented, MDN): each context's JavaScript, the wasm memories not among it. |
| `footprint` of the Simulator's processes | iOS Simulator | Per-process footprints; ETC2 textures there are held as RGBA8, so they don't show the device's (#443). |

`bazel test //tests/browser:game_level_chrome //tests/browser:game_memory_chrome //tests/browser:render_level_chrome` reports the browser's numbers; the native tests with game data (`--config=measure` for meaningful counts).

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

Measured natively. After the level is dropped, 29.4 MB in 7 blocks outlive it: the collision world's arrays, kept as spares for the next world (#357).

### Loading transients

Each stage's peak above its start and what it keeps, measured natively (MB): `st_server_connecting` 15.6 and 15.6 (the spawn file and the graph streamed, #403); `st_opening_stream` 8.3 and 7.9 (the `level` file streamed, #483: 12.6 read whole; `l11_pripyat` 8.2, 12.9 whole); `st_loading_cform` 30.3 and 30.1; `st_loading_spatial_db` 6.2, freeing 1.9 (it keeps the progressive visuals' detail windows, 0.7); `st_loading_details` 5.6 and 5.6; `st_client_spawning` 19.7 and 13.9; `st_loading_textures` 2.4 and 0.3. The load peaks 77.5 MB above its start against 71.7 kept. By count the transients are the many: `st_client_spawning` makes 2.52 million allocations to keep 6,744 blocks, `st_server_connecting` 537,041 to keep 83,764.

In the browser the level's preparation adds the texture read buffer (one file, up to 4.5 MB) and the band being transcoded (without BC); WebGL copies what is written at each call (docs/architecture/rendering.md).

### Frame

A frame of the running level, on the wasm heap (the `render-level` suite, 600 frames after 60): 39 allocations and 63 KB (at most 161): the update 20.2 and 11.8 KB, the sounds 0.0, the scene 10.0 and 26.5 KB, the draw list 9.1 and 24.8 KB; live memory grew 0.48 MB over the 600 frames. The passes' lists (the camera pass's dynamic items among them), the lights', glows' and simulation's, the effects drawn and the particle sprites' scratch are kept and cleared (#488). In the browser the heap stays flat while the level runs (88–89 MiB before the intro, while it plays and after 20 s of walking), so the frame's blocks reuse their own holes.

On an iPhone the heap grew while a level ran (74.9 → 78 MiB about 110 s into the first game, 82 → 86.6 about 180 s into the second; its diagnostic log): at the level's first music track (`music\amb01`, whose files no longer pass through the heap, #491; in the iOS Simulator the most live went 62.2 → 69.8 MiB at its start before and after that change) and under the in-game menu, which holds 5.7 MB more while it shows (measured there; believed its videos' read-ahead windows and decoders, made again at each opening, #517).

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
| Heap / live | 16–17/11 | 88–89/81 | 88–89/40 | 88–95/81 | 92–95/81 | 92–95/81 |

The level, once loaded, holds 67 MiB live. The load's most live comes at its end, under `st_loading_textures` and `st_client_synchronising` (58 → 81 MiB): the objects' CPU geometry with every detail window's indices is among it until the world uploads (the 8.0 MiB index list and the 4.0 MiB vertex list, #528, #653). The heap's last two growths come in that stage (9.6 MiB asked for at 73.6 MiB, then 4.0 MiB) (the 9.6 MiB block's owner: #666).

After quitting, 40 MiB stay live: 25 MiB of them are the collision world's spare arrays (three blocks of 8.0, 10.5 and 7.0 MiB), the rest the program's data and the menus'.

Where the gap comes from:

- **Large blocks meeting split free space.** The heap grows by 26 MiB under the collision stage for 24 MiB more live: its arrays (up to 10.5 MiB each) find no hole that size in the free space the earlier stages left (measured, `heap_log`, `heap_record`'s growths; #373). It is the only stage that grows the heap by about what it keeps.
- **Loading transients among long-lived blocks.** `st_client_spawning` (2.52 million allocations, 6,743 kept) and `st_server_connecting` (537,041, 83,764 kept) leave their kept blocks scattered through the space their temporaries used, so the next large block needs new memory above them. At the load's most live, blocks under 64 KiB hold about 7 MiB, blocks of 1 MiB or more 53 MiB (measured, `heap_record`).
- **The preparation's buffers.** The texture read buffer and the band buffer arrive last, above the level's data.
- **A later New Game grows the heap by 4–6 MiB once** (in the relaunched game's or the second), the next none: an allocation of the texture stage finds no hole that size in the first game's layout (measured, `heap_record`'s growths).

The program's data leaves no split free space before a game (`heap_map`, #432).

## Design

### Rules

1. **One heap**: Rust's `System` allocator (dlmalloc over the wasm memory). No second allocator or other allocator algorithm (#400); no heap per lifetime: the experiment measured it to grow the heap ([Rejected](#rejected-heaps-by-lifetime-401)).
2. **Shrink and stream first.** Files are read as streams into their final form; nothing large is read whole to be parsed. A file's form in memory is the smallest that serves its readers.
3. **An owner of many small long-lived things holds them in a few large blocks** (a region): one text buffer and tables of ranges or indices into it, sized once from what is read and never grown afterwards. Plain `Vec`s on the one heap, reached by index, freed whole with their owner, they move the fragmentation sources (hundreds of thousands of small blocks) out of the general heap's free space without a second heap.
4. **Only the collision world keeps its large blocks as spares.** Freeing a large wasm block returns nothing to the device, and the next load's earlier stages split its space before the collision world is read; kept, its arrays are reused whole when large enough, and each grows to the largest level met. No other owner keeps a spare: kept between loads, the texture file buffer took the space the geometry's upload reused, so the first game's heap grew by more than the second saved (76–82 against 74 MiB; the navigation's and the details' stages grow nothing in a second game; #487).
5. **No allocation in a running frame.** Per-frame data goes into buffers its owner keeps and clears (`Vec::clear` keeps capacity); a bump arena is not added, as the heap already stays flat while the level runs (#402).
6. **Nothing of a level outlives it on the GPU.** A level's GPU resources are released when the level ends, before the next load starts.

### Lifetimes and their homes

Nesting: the program holds sessions; a session holds levels; a level holds its loading and its frames. Each lifetime's home is on the one heap, allocated in that order, so the longer-lived data sits lower and the shorter-lived above it.

| Lifetime | Home | Size, `l01_escape` (largest level) |
|---|---|---|
| Program | Regions made at launch, before anything else: the archive index (#300), the configuration (#299), the string tables and the effect library (#484; the library read once at launch rather than with each level: our choice, as `gamemtl.xr` is read with the level). | 5.3 MB in about 4,100 blocks |
| Session | The offline simulation's registry (a slot table, each record's strings its own); the game graph; the character; the script runtime's state. | 8.7 MB (16 with the navigation) |
| Level | Its owners' blocks: the collision world's arrays (kept as spares after the level, rule 4), the navigation positions, the detail grid and cache, the objects' models, sounds and effects. | about 54 MB (about 62, `l11_pripyat`; believed) |
| Loading | Each reader's own stream buffer: 64 KiB for the spawn file and the graph, the `level` file, the geometry's read and the detail grid; 2 MiB pieces for the geometry's upload and the index runs; in the browser one texture file buffer and one band buffer per preparation. Stage temporaries freed by the stage's end. | under 10 MB above what each stage keeps |
| Frame | Buffers kept by the scene, the update and the sounds, cleared each frame. | held for the level |

There is no program scratch shared by the loaders: each owns its stream buffer for its read. Sizing kept scratch buffers from the measured loading peaks so they never raise the heap's peak is #489.

### Allocators and containers

- The allocator is `std`'s `System` on every target, under a wrapper that only counts: in every build the live bytes (the console's `* phase cmem`), in test builds also the ops' figures.
- Containers are `std`'s: `Vec` for regions and tables, `HashMap` (std's is hashbrown) where a map is needed. No container takes a custom allocator, so `allocator-api2`, `bumpalo` and the unstable `allocator_api` are not used.
- Strings that many records repeat (section names, file paths, model names) are stored once in a region and referred to by index or range (`u32`), not as `String`s per record. The registry's records keep their own strings: moving their sections and names to indices would cut a third of their blocks and about 0.3 MB, not worth changing the object records and the save writer (#485).

### What stays on the general heap as it is

Small, short-lived or rare allocations: the UI's state, menus, the console, errors and diagnostics, a stage's temporaries that are small; the audio worker's own instance (2 MiB, its sounds in JavaScript); the script runtime's blocks (Lua allocates per object; its state is the session's). They are a few MiB and either short-lived or allocated where no large block follows them.

### Budgets

Reported, never gated:

| What | Budget | Reported by |
|---|---|---|
| The program's data (native): the archive index, the configuration, the string tables, the effect library | 2.0, 1.5, 1.5, 1.8 MB | `loading_memory.rs` |
| Each loading stage's peak above its start (native) | `STAGE_BUDGETS` | `loading_memory.rs` |
| The load's peak above its start (native) | 79 MB | `loading_memory.rs` |
| Each wasm heap after a New Game (browser) | 110 MiB | the game suites |

Targets no tool reports against: a frame's allocations in a running level 0; heap minus live after the first New Game under 4 MiB, and the second and third no larger than the first; the level's GPU memory as stored 250 MB for the largest level; JavaScript 12 MiB (Chrome).

### Level change and the second New Game

A level change goes through a save and a load in the original (docs/behavior/offline-simulation.md 6.1); a New Game after a game is a game's end and a load. Both run in this order:

1. The level ends whole: its screen, its sounds (with the collision world they share, #451), the presenter's world (released with the first frame drawn without it) and any preparation in progress, every GPU resource of the level (rule 6). The collision world's arrays go to its spares.
2. A game's end also ends the session (the registry and graph go; the next session's blocks fit their holes, as they are sized from the same files).
3. The next load starts. The collision world's arrays come from the spares, and a larger level grows them at the top once.

So the heap after any sequence of levels is what the program, the session, the largest level met and the collision spares need, plus what a second game's layout adds once (4 MiB, measured above); the GPU holds one level at a time.

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

### The original

The specs in docs/ state nothing about how the original manages its memory (texture-quality-filtering.md leaves open which textures stay referenced across a level change); no comparison is made until they do or its working set is measured (#375, blocked by #341).

## Open steps

Each is its own issue, measured before and after with the tools above (native and those three browser suites), its gain checked against the expectation here, and stopped if it doesn't pay off.

- **The objects' models, sounds and effects as regions of their owners** (#486; `st_client_spawning`: 2.52 million allocations, 6,743 kept). Expected: the blocks kept to hundreds, its 4.6 MB transient lower, heap minus live after the first New Game under 4 MiB.
- **Kept scratch buffers sized from measured loading peaks** (#489), so they never raise the heap's peak. Expected: the second New Game's 4 MiB growth gone without growing the first's (which a kept texture file buffer did, rule 4).
- **The frame's remaining allocations** (#516): each particle draw's own vertex list, the details' frame, the draw list, which need the scene and the renderer to share kept buffers. Expected: 39 allocations and 63 KB a frame to 0, and the 0.48 MB growth over 600 frames gone.
- **The in-game menu's videos made once** (#517): their players or the decoder's frame buffers kept for the program. Expected: the 5.7 MB taken at each opening, and the iPhone's growth under the menu, gone.
- **Sorted `Vec`s for the `BTreeMap`/`BTreeSet`s of `String`s the program or the level holds** (#405): the archive tree's, the console's, the storage's. Expected: their blocks; measured per container.

Pools for particles, emitters and voices (#404) are not a step: the update and the sounds make about 20 allocations a frame in all, which #516 removes.

## GPU and JavaScript memory

JavaScript holds 8 MiB beside the wasm heaps in headless Chrome ([Program](#program-from-the-launch-to-closing-the-page)); WebKit has no `measureUserAgentSpecificMemory()` (documented, MDN's compatibility data: Chromium only), and the Simulator's WebContent process (wasm, JavaScript and WebKit together) held 199–222 MB in a level (measured, #443). What the browser holds for WebGL 2 beyond our resources (#443; WebKit's and ANGLE's source read with `gh api`, the iOS Simulator measured with `tests/browser/webgl-memory.html` and `footprint`):

- ANGLE keeps no CPU copy of buffers on Apple GPUs (its shadow buffers are for Intel), nor WebKit of buffers or textures (documented).
- A buffer made without data is zero-filled from a CPU scratch buffer of its size in the GPU process: a 64 MiB buffer took the Simulator's GPU process 128 MB at first, 69 MB later; a buffer written while the GPU uses it is given a whole new copy (`BufferMtl::setSubDataImpl`, documented): written again in 2 MiB pieces after a draw, the 64 MiB buffer held 62 MB more (measured). An 8 MiB buffer written 1 MiB a frame and drawn held no more over 600 frames, nor a 16-bit index buffer drawn from an odd first index (measured).
- With WebGL's last-vertex provoking convention, ANGLE's Metal backend rewrites the indices of every draw whose program has a flat varying into a buffer of its own, held until the command buffer completes (documented); a whole view's draws in one submission killed the iPhone (measured, #443). The context uses the first-vertex convention (`WEBGL_provoking_vertex`).
- The context is made without depth or stencil (`gpu::make_webgl_context`): WebKit gives the default framebuffer a canvas-size depth buffer otherwise (Depth32Float on Metal, 15 MB at 2868 × 1320; documented), which nothing uses.

## Rejected: heaps by lifetime (#401)

The design first approved (#399) gave the level, loading and the frame each a heap of its own beside the general one: an instance of dlmalloc over segments of 64 KiB pages taken from the general heap (branch `memory-heaps-experiment`, commit e6135e9, not merged). In headless Chrome the heap after the New Game grew in every variant: 119–122 MiB against 106 for the general heap alone, 130–132 against 110 after the second (one run each). Each heap needs its segments whole, so the level's blocks moved from the general heap's holes to new memory at its top, and no heap's free space served the others. The regions of rule 3 differ: an owner's data in a few blocks on the one heap, sized from its files, so they cut the block count instead of moving it.
