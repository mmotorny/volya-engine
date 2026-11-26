# Memory

Where the engine's memory goes, by lifetime and owner; the design it follows (one heap, owners that hold few large blocks, no level's resources outliving it); the order of the work. Numbers are for a New Game of `l01_escape` on the Russian edition unless a level is named, and are marked measured (with where), documented (with the source) or believed. MB is 10^6 bytes (native counts), MiB 2^20 (the browser's heap).

## Limits

- The product runs on an iPhone, where iOS kills the page's process at its per-process limit: 2,528 MB on the reference iPhone (measured: its Jetsam report, #443). Every ANGLE Metal resource and drawing buffer is attributed to that process (documented, #443), so the limit covers the wasm heaps, JavaScript, WebKit and the GPU's resources together.
- A wasm memory never shrinks (documented: WebAssembly has no instruction to release memory): the heap's high-water mark is the page's for its life. Free space in it is only worth something to later allocations that fit it.
- GPU resources are released to the browser when deleted; the GPU process frees them some time after (documented for WebGL; when exactly is the implementation's).
- Budgets are measured and reported ("OVER BUDGET" notes in `loading_memory.rs` and the `menu` suite), never release gates.

## Measuring

| Tool | Where | What |
|---|---|---|
| `crates/volya-engine/tests/loading_memory.rs` | native, counting allocator | Each loading stage's peak above its start, what it keeps, allocations made and blocks kept; the program's data; what outlives the level. |
| `crates/volya-engine/tests/level_memory.rs` | native, counting allocator | Every shipped level's own files: the collision world's and the detail objects' heap, the `level` file's parse peak, and as stored the geometry, the navigation file and the textures its surfaces name. |
| `render-level` suite's "a level frame's heap traffic" (`volya_engine::selftest::frame_memory`) | browser, the test build's counting allocator | Allocations and bytes a frame of the running level makes on the wasm heap, by part. |
| `heap_log` op (test builds) | browser | The wasm heap and the most live under each loading caption. |
| `heap_map` op (test builds) | browser | Live KiB per MiB of the heap and every live block of 256 KiB or more, by address. |
| `heap_record` op (test builds) | browser | At the load's most live: live bytes by block size and the largest blocks; every allocation that grew the wasm memory since the reset, by size and address. |
| `level_stats` op | browser | The wasm heaps (engine, audio worker) and the GPU world's buffers and textures. |
| `menu` suite's JS memory | Chrome | `performance.measureUserAgentSpecificMemory()` after the New Game (cross-origin isolated pages only, documented, MDN): each context's JavaScript, the wasm memories not among it (measured: 8 MiB in all beside a 87 MiB heap). |
| `footprint` of the Simulator's processes | iOS Simulator | Per-process footprints; ETC2 textures there are held as RGBA8, so they don't show the device's (#443). |

`cargo xtask e2e chrome menu` reports the browser's numbers; the native tests run with `VOLYA_GOG_INSTALLER` (release build for meaningful counts).

## Inventory

### Program: from the launch to closing the page

| Owner | Live | Blocks | Notes |
|---|---|---|---|
| The archive tree (`DbVfs`: a region of every name once and a node table, each directory's children a sorted run) | 1.3 MB | 18 | 7.7 MB in 87,312 blocks as maps of paths before #300; building it peaks 7.0 MB above its start, one archive's table at a time. |
| The system configuration (`Config`: one text buffer of every name, key and value; tables of sections, entries and each section's run of entry indices) | 1.2 MB | 4 | 5.6 MB in about 100,000 blocks, a heap string per key and value, before #299. |
| The string tables (`StringTable`: one buffer of every key and text, the entries sorted by key) | 1.2 MB | 2 | 1.8 MB in 15,344 blocks, a map of heap strings, before #484. |
| The UI's data | 0.2 MB | | |
| The particles' effect library (`particles.xr`, `Library`: one buffer of every name, a table of every effect's actions and one of every group's members) | 1.4 MB | 1,095 | Read once at launch (`GameData`); with each level under `st_loading_ai_objects` before #484, 1.9 MB in 8,411 blocks. Its blocks are the boxed `source` actions. |
| The renderer and its programs; the menus' textures and video frames | about 6 MiB | | The title menu's heap is 18 MiB with 11 live against 5 after the launch (measured, the `menu` suite in headless Chrome). |
| The audio worker's wasm instance | 2–4 MiB | | Its own heap (`level_stats`). |
| JavaScript: the audio worker 3.1, the engine worker 2.0, the page 1.1–1.6, shared 0.9 MiB | 8 MiB | | Measured in headless Chrome after the New Game (two runs). The audio worker keeps the compressed sound files there (`JsFile`), not in a wasm heap. |

The program's data holds about 4,100 of the heap's blocks before a game, 1,095 of them the effect library's (measured: 453 with the archive tree, 4,088 with the game's data).

### Session: a game, from its New Game to its end

The offline simulation's registry (6.3 MB, 54,735 blocks), the game graph and paths (1.8 MB) and the player character, made under `st_server_connecting`: 15.6 MB with the current level's navigation (below), 83,763 blocks, from 537,042 allocations (measured, native). Of the registry, the slot table of 8,646 objects of 392 bytes is about 3.4 MB in one block (448 bytes before #498: the rare large kinds, among them the crows, the visual anomalies and the trader, and every skeleton's saved bone state, which no New Game record has, are now boxed); the records' own strings and lists (every kind's names, section, custom data, restrictions, character fields) 2.4 MB in 51,204 blocks, the section and the name alone 17,282 of them; travel 0.2 MB in 2,698. The switch predicates' inputs kept per object held a copy of its class id, section and name until #485 (25,923 blocks, 1.0 MB). The script runtime's state joins this lifetime when the game's scripts run (`lua_alloc` is Rust's allocator, docs/architecture/scripts.md).

### Level: from its loading to leaving it

| Owner | `l01_escape` | The largest level |
|---|---|---|
| The collision world (vertices, triangles, each triangle's material and sector in 2 bytes, tree) | 29.4 MB in 4 blocks (largest 10.5 MiB) | 44.5 MB, `l11_pripyat`; 39.4 `l10_radar`, 34.4 `l07_military` |
| The navigation positions (6 of each vertex's 21 bytes) | 7.1 MB, one block (`level.ai` 12.5 MB) | `l01_escape`'s `level.ai` is the largest |
| What the `level` file keeps after `st_loading_spatial_db` (the shader table; the static draw list and spheres) | 1.8 MB | |
| The detail objects (slot grid, compact, and cache) | 5.5 MB | 11.9 MB, `l12_stancia` and `l12_stancia_2` |
| The objects' models, weather, the level's sounds | 6.3 MB, 6,800 blocks | |
| The objects' CPU geometry until uploaded | 4.9 MB | |
| The level in all at the end of loading | 63.5 MB, 128,169 blocks above the program's data | `l11_pripyat` about 72 MB (believed: `l01_escape`'s with its own collision, details and navigation file) |

Measured natively (`loading_memory.rs`, `level_memory.rs`; requested bytes, the allocator's overhead not in them). After the level is dropped, 29.4 MB and 4 blocks outlive it: the collision world's arrays, kept as spares for the next world (#357).

### Loading transients

Each stage's peak above what it keeps, measured natively: `st_opening_stream` 4.8 MB (the `level` file read whole, 12.6 MB peak; 12.9 for `l11_pripyat`), `st_loading_spatial_db` 5.4, `st_client_spawning` 4.7, `st_loading_textures` 2.8, `st_loading_cform` 0.2, `st_server_connecting` 0.0 (the spawn file and the graph streamed, #403). The load peaks 67.6 MB above its start against 63.5 kept. By count the transients are the many: `st_client_spawning` makes 2.52 million allocations to keep 6,800 blocks, `st_server_connecting` 536,685 to keep 83,406.

In the browser the level's preparation adds the texture read buffer (one file, up to 4.5 MB) and the band being transcoded (without BC); WebGL copies what is written at each call (docs/architecture/rendering.md).

### Frame

A frame of the running level, on the wasm heap (the `render-level` suite, 600 frames after 60): 324 allocations and 433 KB (at most 904 allocations): the update 28.5 and 8.4 KB, the sounds 5.0 and 0.1 KB, the scene 237.8 and 356 KB, the draw list 52.8 and 69 KB. Live memory grew 0.81 MB over the 600 frames. In the browser the heap stays flat while the level runs (87 MiB before and while the intro plays), so the frame's blocks reuse their own holes.

### The GPU

| Owner | `l01_escape` | The largest level |
|---|---|---|
| The level's textures, as stored (BC) | 127 MiB (133.3 MB; the objects' 23.6 MB of it; 458 textures) | Its surfaces' alone: 91.2 MB `l07_military`, 83.3 for `l01_escape` |
| The level's buffers (geometry, the objects', draw records) | 55.3 MB (the objects' 6.3) | `level.geom` 109.7 MB `l12_stancia_2` against 57.0 |
| Render targets: the supersampled target and its Depth32Float depth, the video mode's frame, the surface | about 90 MB at 2868 × 1320 with `r__supersample` 2 (believed: computed from their sizes) | |
| The canvas's drawing buffers | up to 3 at canvas size, 15 MB each at 2868 × 1320 (documented, #443) | |

Measured in headless Chrome (`level_stats`) and natively (`level_memory.rs`, as stored); the render targets are not reported by any tool. On the device BC is held as stored where Metal has BC (documented, #443). In the iOS Simulator a level of `l01_escape` takes its Metal host to 1.8 GB, nearly all textures held as RGBA8 there (measured, #443); ETC2 too is expanded there (measured), so the Simulator's footprint does not show the device's. What else WebKit and ANGLE hold: [GPU and JavaScript memory](#gpu-and-javascript-memory).

## Heap minus live

Headless Chrome, `menu` suite, two runs (MiB, heap/most live):

| | Launched | Title menu | Before the New Game | First New Game | Title menu after quitting | Second | Third |
|---|---|---|---|---|---|---|---|
| Heap / live | 18/11 | 25/16–18 | 35/17 | 87/79 | 87/45 | 89/80 | 93/80 |

The 10 MiB between the title menu and the New Game is the suite's own capture of the title menu, free at the heap's top. After quitting, 45 MiB stay live: 26 MiB of them are the collision world's spare arrays (three blocks of 8.0, 10.5 and 7.0 MiB), the rest the program's data and the menus'.

Where the gap comes from:

- **Large blocks meeting split free space.** The heap grows by 26 MiB under the collision stage for 23 MiB more live: its arrays (up to 10.5 MiB each) find no hole that size in the free space the earlier stages left (measured, `heap_log`; #373). It is the only stage that grows the heap by more than it keeps.
- **Loading transients among long-lived blocks.** `st_client_spawning` (2.52 million allocations, 6,800 kept) and `st_server_connecting` (536,685, 83,406 kept) leave their kept blocks scattered through the space their temporaries used, so the next large block needs new memory above them. At the load's most live, blocks under 64 KiB hold about 14 MiB, blocks of 1 MiB or more 58 MiB (measured, `heap_record`).
- **The preparation's buffers.** The texture read buffer and the band buffer arrive last, above the level's data.
- **A second New Game grows the heap by 4 MiB once**, the third none (the `menu` suite's path of a phone's log: the first game played, quit to the title menu, a New Game 5 s later; 74 → 78 MiB in one run, 74 → 82 in another; the third game no further). Two allocations of the texture stage grow it, 2.2 and 4.0 MiB (measured, `heap_record`'s growths), as the first game's layout leaves no hole that size there. Keeping the texture file buffer as a spare (alone, or shared with the geometry's upload piece) does not pay off: live, it takes the space the geometry's upload reused, so the first game's heap grows by more (76–82 against 74, the second 79–82 against 78; one run each, #487). The navigation's and the details' stages grow nothing in the second game, so their spares would not help either.

The program's data leaves no split free space before a game (`heap_map`, #432).

## Design

### Rules

1. **One heap**: Rust's `System` allocator (dlmalloc over the wasm memory). No second allocator or other allocator algorithm (#400); no heap per lifetime: the experiment measured it to grow the heap ([Rejected](#rejected-heaps-by-lifetime-401)).
2. **Shrink and stream first.** Files are read as streams into their final form; nothing large is read whole to be parsed. A file's form in memory is the smallest that serves its readers.
3. **An owner of many small long-lived things holds them in a few large blocks** (a region): one text buffer and tables of ranges or indices into it, sized once from what is read and never grown afterwards. These are the "appropriate arenas" of the user's rule: plain `Vec`s on the one heap, reached by index, freed whole with their owner. They move the fragmentation sources (hundreds of thousands of small blocks) out of the general heap's free space without a second heap.
4. **A lifetime's large blocks are kept for the next of their kind.** Freeing a large wasm block returns nothing to the device; keeping it as the owner's spare means the next level finds it whole instead of looking for a hole. Each spare grows to the largest level met and no further.
5. **No allocation in a running frame.** Per-frame data goes into buffers its owner keeps and clears (`Vec::clear` keeps capacity); a bump arena is not added, as the heap already stays flat while the level runs (#402).
6. **Nothing of a level outlives it on the GPU.** A level's GPU resources are released when the level ends, before the next load starts.

### Lifetimes and their homes

Nesting: the program holds sessions; a session holds levels; a level holds its loading and its frames. Each lifetime's home is on the one heap, allocated in that order, so the longer-lived data sits lower and the shorter-lived above it.

| Lifetime | Home | Size, `l01_escape` (largest level) |
|---|---|---|
| Program | Regions made at launch, before anything else: the archive index (one sorted path buffer with child ranges, #300), the configuration (one text buffer per file, sections, keys and values as ranges, #299), the string tables (one buffer and a sorted index, #484), the effect library (one name buffer, tables of actions and members; read once at launch rather than with each level: our choice, as `gamemtl.xr` is read with the level, #484). The persistent scratch (below). | about 9 MB (17.2 measured) in a few hundred blocks |
| Program, scratch | Kept buffers, made once at launch: the stream buffer (64 KiB), the texture file buffer and the band buffer (sized to the largest texture file and band, 4.5 and 2 MiB), the upload staging of our GL layer if it needs one (2 MiB). | about 9 MiB |
| Session | The offline simulation's registry with its names and sections as indices into the configuration's region and one string region of its own; the game graph; the character; the script runtime's state. | about 7 MB (9.1 measured: 16.2 with the navigation) |
| Level | Its owners' large blocks, each kept as a spare after the level (rule 4): the collision world's arrays (#357), the navigation positions, the detail grid and cache. The level's small data (the objects' models, sounds, effects) as regions of the level's owners. | 62.7 MB (about 72, `l11_pripyat`) |
| Loading | Streams through the program's scratch; stage temporaries freed by the stage's end. | under 5 MB above what the level keeps |
| Frame | Buffers kept by the scene, the draw list, the update and the sounds, cleared each frame. | about 0.5 MB, held for the level |

So there is no arena besides the regions and spares above: none with an allocator of its own.

### Allocators and containers

- The allocator is `std`'s `System` on every target; the test builds' counting wrapper only counts.
- Containers are `std`'s: `Vec` for regions and tables, `HashMap` (std's is hashbrown) where a map is needed. Collections the level or the program holds that have many small keys become a sorted `Vec` or an index into a region (`BTreeMap`/`BTreeSet` of `String` keys: 17 and 11 uses in the engine's crates). No crate is needed: no container takes a custom allocator, so `allocator-api2`, `bumpalo` and the unstable `allocator_api` are not used.
- Strings that many records repeat (section names, file paths, model names) are stored once in a region and referred to by index or range (`u32`), not as `String`s per record.

### What stays on the general heap as it is

Small, short-lived or rare allocations: the UI's state, menus, the console, errors and diagnostics, a stage's temporaries that are small; the audio worker's own instance (2–4 MiB, its sounds in JavaScript); the script runtime's blocks (Lua allocates per object; its state is the session's). They are a few MiB and either short-lived or allocated where no large block follows them.

### Budgets

Reported against these, never gated (the `menu` suite also notes both wasm heaps over 110 MiB):

| What | Budget |
|---|---|
| The program's data (native) | 12 MB |
| The session (native) | 10 MB |
| The level at the end of loading (native, `l01_escape`; the largest level) | 64 MB; 80 MB |
| The load's peak above its start (native) | 70 MB |
| A frame's allocations in a running level (native) | 0 |
| The engine's wasm heap after the first, second and third New Game (browser) | heap minus live under 4 MiB; the second and third no larger than the first |
| The level's GPU memory, as stored (largest level) | 250 MB |
| JavaScript (Chrome) | 12 MiB |

### Level change and the second New Game

A level change goes through a save and a load in the original (docs/behavior/offline-simulation.md 6.1); a New Game after a game is a game's end and a load. Both run in this order:

1. The level ends whole: its screen, its sounds (with the collision world they share, #451), the presenter's world (released with the first frame drawn without it) and any preparation in progress, every GPU resource of the level (rule 6). The level's large arrays go to their owners' spares.
2. A game's end also ends the session (the registry and graph go; their regions leave holes the next session's fit exactly, as they are sized from the same files).
3. The next load starts. Its large blocks come from the spares; its regions fit the holes the previous level's left, as both are sized from the level's files, and a larger level grows them at the top once.

So the heap after any sequence of levels is what the program, the session, the largest level met and its spares need; the GPU holds one level at a time.

### GPU memory: what is owned and released when

The renderer's own WebGL 2 layer (docs/architecture/rendering.md) creates and deletes its buffers, textures and vertex arrays, and no staging copy lives in the heap. What must be owned, and released when:

| Lifetime | Resources | Released |
|---|---|---|
| Program | The UI's programs and vertex arrays; the menus' textures, by path, while drawn; video textures while their video plays. | A menu texture when no sprite draws it (`Presenter`'s images); a video's when it ends. |
| Video mode | The supersampled target and its depth, the video mode's frame, the presentation targets. | When the video mode or `r__supersample` changes, before the new ones are made. |
| Level | The world's geometry and draw records, its surfaces' and objects' textures, the detail models, the particles' buffers, the light and glow textures. | When the level ends (a quit, a game's end, a level change), before the next load's preparation starts. |
| Loading | Textures and geometry prepared for the load in progress. | Given to the level when it is made; freed when the load is abandoned. |
| Frame | Streaming buffers (sprites, UI quads), made once and reused. | With the program. |

A level's world, its effect and glow shaders and its detail models go with the first frame drawn without a world (`Presenter::draw_list`): the title menu after a level holds no GPU world (measured, `level_stats` in the `menu` suite). What the iOS Simulator's Metal host keeps after a level, 200–300 MB above the title menu before one (#436), is therefore not the world; no tool attributes it to our resources or the browser's, so every GPU resource we hold is reported by owner (migration step 1). The code departs from the table in one place:

- **An abandoned load's preparation outlives it** (#262): textures prepared for a load that is quit stay in `Renderer::prepared` until the next `clear_prepared`, and the texture task (`platforms/web/src/textures.rs`) does not check that its load is still the current one, so a stale task can mark a later load's textures prepared. Since #449 every load prepares its textures this way. Preparation is tied to its load (an id given with the load), an abandoned load's prepared resources are freed at once, and a test quits during preparation.

Data written each frame is never written into a buffer the GPU may still be reading: ANGLE's Metal backend copies such a buffer whole (documented, #443).

### The original

The specs in docs/ state nothing about how the original manages its memory (texture-quality-filtering.md leaves open which textures stay referenced across a level change); no comparison is made until they do or its working set is measured (#375, blocked by #341).

## Migration

Ordered; each step is its own issue, measured before and after with the tools above (native and `cargo xtask e2e chrome menu`), its gain checked against the expectation here.

1. **Every GPU resource reported by owner.** `level_stats` reports the bytes of each lifetime's resources in the table above (the render targets, the menus' textures, the particles' buffers, the world), at the title menu as in a level. Expected: about 90 MB of render targets at 2868 × 1320 with `r__supersample` 2 measured rather than computed, and #436's 200–300 MB found ours or the browser's.
2. **Preparation tied to its load** (#262): an id given with the load; the texture task checks it before it reports; an abandoned load's prepared textures and geometry freed at once; a test quits during preparation. Expected: a load quit during its preparation leaves no prepared resource (up to 127 MiB of textures for `l01_escape`), measured by step 1's report.
3. **Stream the `level` file** into its parts, never holding it whole. Expected: `st_opening_stream`'s peak from 12.6 to about 8 MB native (l11: 12.9), its transient blocks gone.
4. **The archive index as one region** (#300). Expected: 7.7 → about 3 MB, 87,747 → a few blocks.
5. **The configuration as one region per file** (#299). Expected: 5.6 → about 2 MB, ~100,000 → a few hundred blocks.
6. **The string tables and the effect library as program regions**, the library read once at launch instead of with each level. Expected: about 1 MB and 8,000 blocks less, the effect library's 3.3 MB peak out of every level's load.
7. **The registry's names and sections as indices** into regions (the session's string region, the configuration's sections). Expected: 109,329 blocks to a few thousand; 7.9 MB by an estimated 2–3 MB (believed).
8. **The objects' models, sounds and effects as regions of their owners** (`st_client_spawning`: 2.52 million allocations, 6,800 kept). Expected: the blocks kept to hundreds; its 4.7 MB transient peak lower; with steps 4–7, heap minus live after the first New Game under 4 MiB.
9. **Spares for the level's other large blocks** (the navigation positions, the detail grid and cache), as the collision world's are. Expected: the third New Game no larger than the second (93 against 89 MiB, measured above), a larger level growing the heap once.
10. **No allocation in a running frame**: the scene's and the draw list's buffers kept and cleared. Expected: 324 allocations and 452 KB a frame to 0; the 0.81 MB growth over 600 frames located and gone. Also CPU time (believed small).
11. **Sorted `Vec`s for the `BTreeMap`/`BTreeSet`s of `String`s the program or the level holds** (#405), where a step above did not already replace them. Expected: their blocks; measured per container.

Pools for particles, emitters and voices (#404) are not a step: the update and the sounds make 34 allocations a frame in all, which step 10 removes.

## GPU and JavaScript memory

JavaScript holds 8 MiB beside the wasm heaps in headless Chrome ([Program](#program-from-the-launch-to-closing-the-page)); WebKit has no `measureUserAgentSpecificMemory()` (documented, MDN's compatibility data: Chromium only), and the Simulator's WebContent process (wasm, JavaScript and WebKit together) held 199–222 MB in a level (measured, #443). What the browser holds for WebGL 2 beyond our resources (#443; WebKit's and ANGLE's source read with `gh api`, the iOS Simulator measured with `tests/browser/webgl-memory.html` and `footprint`):

- ANGLE keeps no CPU copy of buffers on Apple GPUs (its shadow buffers are for Intel), nor WebKit of buffers or textures (documented).
- A buffer made without data is zero-filled from a CPU scratch buffer of its size in the GPU process: a 64 MiB buffer took the Simulator's GPU process 128 MB at first, 69 MB later; a buffer written while the GPU uses it is given a whole new copy (`BufferMtl::setSubDataImpl`, documented): written again in 2 MiB pieces after a draw, the 64 MiB buffer held 62 MB more (measured). An 8 MiB buffer written 1 MiB a frame and drawn held no more over 600 frames, nor a 16-bit index buffer drawn from an odd first index (measured).
- With WebGL's last-vertex provoking convention, ANGLE's Metal backend rewrites the indices of every draw whose program has a flat varying into a buffer of its own, held until the command buffer completes (documented); a whole view's draws in one submission killed the iPhone (measured, #443). The context uses the first-vertex convention (`WEBGL_provoking_vertex`).
- The context is made without depth or stencil (`gpu::make_webgl_context`): WebKit gives the default framebuffer a canvas-size depth buffer otherwise (Depth32Float on Metal, 15 MB at 2868 × 1320; documented), which nothing uses.

## Rejected: heaps by lifetime (#401)

The design first approved (#399) gave the level, loading and the frame each a heap of its own beside the general one: an instance of dlmalloc (the `dlmalloc` crate, MIT/Apache-2.0) over segments of 64 KiB pages taken from the general heap, each allocation placed by the scope its code ran in, a page table finding a block's heap when it is freed. Built on the branch `memory-heaps-experiment` (commit e6135e9), not merged. Natively it placed what it should: the loading heap emptied after each stage, the level heap reserved 83.6 MB for 77.7 MB live at the load's end. In headless Chrome the heap after the New Game grew in every variant (MiB, first and second New Game, one run each; before #370, #371 and #432):

| Variant | First | Second |
|---|---|---|
| The general heap only | 106 | 110 |
| Level heap, segments kept for good (256 KiB) | 119 | 132 |
| Level and loading heaps, empty segments given back (256 KiB) | 120 | 131 |
| The same, 64 KiB segments | 122 | 130 |

Heaps apart from the general one need their segments whole: the general heap's free space before the game served its own small blocks but not the level heap's segments, so the level's blocks moved from the general heap's holes to new memory at its top, and each heap's free space was no longer the others'. Separate heaps cost the sharing of free space one heap has. The regions of this design differ in what they hold: an owner's data in a few blocks sized once from its files, on the one heap, so they fit the holes their predecessors left and cut the block count instead of moving it.
