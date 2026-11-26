# Memory

How the engine's memory is laid out: the heaps, what lives in each, and what they measure. Numbers are for a New Game of `l01_escape` on the Russian edition and are marked measured (with where), documented (with the source) or believed. The design the work follows, and the rejected one with its measurements, are in their own section, [Design](#design-one-heap-its-large-blocks-small-or-streamed-399); everything before it describes the code as it is.

## Heaps and allocators

There are no arenas: every allocation goes to one general heap, the target's system allocator.

| Target | Allocator | Notes |
|---|---|---|
| wasm32 (the engine's worker, the audio worker) | Rust's `System`: dlmalloc over the wasm linear memory, grown with `memory.grow` | The memory never shrinks (documented: WebAssembly has no instruction to release memory), so a load's high-water mark stays the page's. One instance each, single-threaded (built without atomics). |
| Native (tests, tools) | Rust's `System`: the platform's `malloc` | Gives memory back; the memory tests count requested bytes instead of reading a footprint. |

Two reuse fixes from #357 stand in for arenas:

- The collision world's large arrays are kept when it is dropped (a thread-local spare in `volya-engine-core::collision`) and reused by the next world when large enough, so a second New Game's collision world finds its 28 MB without new memory (it needed up to 13 MiB more before, measured in headless Chrome).
- Prepared textures share one read buffer, and are written in bands of at most 2 MiB, a transcoded one transcoded a band at a time into one reused buffer (`TextureUpload`, #440): a whole transcoding of the largest texture held 8 MiB beside its file.

## Measurements by lifetime (#399)

Measured natively (release, M-series Mac) with counting allocators: `volya-engine/tests/loading_memory.rs` per loading stage (budgets reported as "OVER BUDGET" notes, never failures), `volya-engine/tests/frame_memory.rs` per frame of the running level. Bytes are as requested, so the allocator's overhead is not in them.

### Held across levels

| What | Live | Blocks |
|---|---|---|
| The archive tree (`DbVfs`: every file's entry and every directory's names, `HashMap<String, _>`, `BTreeSet<String>`) | 7.7 MB | 87,747 |
| The game's data before any game (`GameData`): the system configuration 5.6 MB, string tables 1.8 MB, UI 0.2 MB | 7.6 MB | 118,008 |
| The collision world's spare arrays after a level (#357) | 28.1 MB (30.1 before #396, 42.0 before #371) | 5 |

### Held for the level

What each loading stage keeps, and the blocks it leaves live:

| Stage | Keeps | Blocks kept | Allocations made | Peak above the stage's start |
|---|---|---|---|---|
| `st_creating_new_game` (the game graph; `all.spawn` and `game.graph` read as streams since #403) | 0.2 MB (4.7 before: the spawn file kept) | 66 | 519 | 0.7 MB (12.8 before) |
| `st_server_connecting` (the offline simulation: registry 7.9, navigation 7.1, graph and paths 1.8; the registry 14.8 before #370) | 17.2 MB with the graph from the stage before (12.7 before #403, the spawn file freed here) | 109,329 | 612,580 | 17.2 MB (14.1 before #403, 19.8 before #370) |
| `st_opening_stream` (the `level` file's parts) | 7.8 MB | 6,887 | 41,503 | 12.6 MB |
| `st_loading_cform` (collision: vertices 8.4, triangles 11.0, materials 1.4, tree 7.3; before #396 a grid of 9.3, before #371 triangles 16.5 and grid 15.8) | 28.7 MB (30.7 before #396, 42.7 before #371) | 9,653 | 14,921 | 28.9 MB (35.1, 45.0) |
| `st_loading_geometry` | 0.4 MB | 121 | 205 | 0.7 MB |
| `st_loading_spatial_db` | −6.0 MB | −6,486 | 117 | 5.4 MB |
| `st_loading_details` (the detail objects' slot grid 4.3, compact, and cache, #411; 9.5 with the grid as stored) | 5.5 MB | 94 | 181 | 5.5 MB |
| `st_loading_ai_objects` (the particles' effect library) | 1.9 MB | 8,417 | 18,083 | 3.3 MB |
| `st_client_spawning` (objects' models, weather, level sounds; the objects' geometry 4.9 until uploaded) | 6.2 MB | 6,929 | 2,459,422 | 11.0 MB |
| `st_loading_textures` | 0.3 MB | 1,639 | 103,158 | 3.1 MB |

The registry is one list indexed by id with each object's record inline; the three kinds largest inline (hanging lamps, monster groups and helicopters, 256–416 bytes) are boxed, and the list is shrunk to its length once every object is registered, so it no longer keeps the half its growth left spare (#370). The level keeps 62.7 MB at the end of loading (66.8 before the compact detail grid, #411; 77.9 MB before #370 and #371, without the detail objects), 64 MB once its geometry is uploaded; the load's peak is 72.8 MB above its start (82.3 before). The game data a level needs resident is about 48 MB (`level.cform` as stored 30.4, the navigation positions 7.1, `all.spawn` 4.5, the graph's tables 1.8, the configuration's text 1.4, the strings 1.8, the effect library 1.9); the rest lives on the GPU as stored.

### Loading temporaries

Each stage's peak above what it keeps: about 16 MB in all (24 before `all.spawn` and `game.graph` were streamed, #403, whose 8.1 MB under `st_creating_new_game` is gone: the spawn file is read an object at a time through a 64 KiB buffer, the graph a record at a time), the largest 4.8 (`st_client_spawning`), 4.8 (`st_opening_stream`), 5.4 (`st_loading_spatial_db`), 2.8 (`st_loading_textures`), 2.3 (`st_loading_cform`). By count they are the many: `st_client_spawning` makes 2.46 million allocations to keep 6,929 blocks, `st_server_connecting` 603,156 to keep 108,551.

### Per-frame temporaries

A frame of the running level (update, sounds, scene, draw list; the camera turning at the start, 600 frames after 60 to warm up):

| Part | Allocations a frame | Bytes a frame |
|---|---|---|
| The level's update | 25.7 | 7.8 KB |
| Sounds | 5.0 | 0.1 KB |
| The scene | 211.7 | 200.6 KB |
| The draw list | 50.9 | 77.0 KB |
| All | 293 (838 at most) | 285.5 KB |

Live memory grew 0.49 MB over the 600 frames.

### In the browser

Headless Chrome, `menu` suite (MiB, the wasm heap and the most live meanwhile, from `heap_log` in test builds):

| Path | Before the New Game | After the first New Game | After the second | Before #440 | Before #432 (first, second) | Before #370 and #371, without the detail objects (#411) |
|---|---|---|---|---|---|---|
| WebGPU | 34 heap, 16 live | 82 heap, 74 live at most | 93 heap, 82 live at most (before #440) | 90/77 | 59/16 before it; 90/81, 90/81 | 98/87, 98/87 |
| WebGL 2 (BC textures as stored) | 25 heap, 20 live (a fresh engine) | 87 heap, 79 live at most | 91 heap, 80 live at most | 97/85, 99/86 | 104/93, 104/89 | 106/95, 110/97 |

One run each (the heap varies by a few MiB from run to run). The audio worker's instance adds 3–4 MiB (#354). Where the live bytes sit (the `heap_map` op of test builds, #432; WebGPU): after the launch the heap is 17 MiB with 10 live (the archive tree and the game's data), at the title menu 24 MiB with 16 live, packed from the heap's start (its largest blocks 0.4–1.4 MiB, believed the menu videos' frames and the UI textures). The game itself leaves no split free space before a New Game. What took the heap to 59 MiB before #432 was the `menu` suite's own capture of the title menu: a 1024 × 768 frame sent to the page as a JSON array of numbers, about 35 MiB at the heap's top, freed at once. Captures are now sent as base64 and decoded in the engine worker; the suite's capture still takes the heap from 24 to 34 MiB, all of it free at the top. The WebGL 2 row starts a fresh engine straight into the game. What stands between the heap and its live peak, 10–11 MiB on WebGPU and 12 MiB on WebGL 2, arises during the loading itself. With the level shown WebGL 2 holds about 3 MiB more live than WebGPU (79 against 76), believed wgpu-core's and naga's state, in blocks under 64 KiB, which the browser holds on WebGPU. At the load's peak the two paths hold the same large blocks; what WebGL 2 adds is staging, wgpu-core's copies of what was written, kept in our heap until WebGL reports the writes done (the `heap_record` op of test builds: live bytes by block size and the largest blocks at the load's most live, #440).

### What fragments the heap

A source is substantial when it leaves many small long-lived blocks between large ones, or makes large temporaries where long-lived blocks then settle:

- the offline simulation's registry: 109,329 blocks for 8,641 objects, made after the previous level's blocks were freed and before the collision world's are allocated;
- the loading temporaries: 2.46 million allocations under `st_client_spawning` alone, interleaved with what the stage keeps;
- the level's other small long-lived blocks: the `level` file's parts (6,887), the effect library (8,417), the objects (6,929);
- the collision world's large arrays, which need contiguous space (#371);
- the frame's temporaries, 285 KB a frame in about 300 blocks, made and freed between the level's.

## GPU and JavaScript memory

Measured in headless Chrome (`menu` suite, `level_stats`) for the first level: textures 133 MB (the level's and the objects' 24 MB, as stored), the level's geometry 50.6 MB, the objects' 6.3 MB. They live in the browser's GPU process, outside the wasm heap; wgpu also keeps staging and recorded commands in the wasm heap (#372). What the page and the workers hold in JavaScript is not measured (#374).

What the browser holds for WebGL 2 beyond our resources (#443; WebKit's and ANGLE's source read with `gh api`, the iOS Simulator measured with `tests/browser/webgl-memory.html` and `footprint`):

- In the iOS Simulator a level of `l01_escape` takes SimMetalHost to 1.8 GB (181 MB idle), nearly all textures: ETC2 is held there as RGBA8, 128 MiB of ETC2 RGBA8-EAC with mips taking 515 MB (RGBA8 itself 1.0×; measured). ANGLE maps ETC2 to Metal's ETC2 formats for the Simulator (documented), so the expansion is the Simulator's own (believed). The device uploads BC as stored and keeps it so where Metal has BC (ANGLE exposes the extension only then; documented), so the Simulator's footprint doesn't show the device's.
- ANGLE keeps no CPU copy of buffers on Apple GPUs (its shadow buffers are for Intel), nor WebKit of buffers or textures (documented).
- A buffer made without data is zero-filled from a CPU scratch buffer of its size in the GPU process: a 64 MiB buffer took the Simulator's GPU process 128 MB at first, 69 MB later; a buffer written while the GPU uses it is given a whole new copy (`BufferMtl::setSubDataImpl`, documented): written again in 2 MiB pieces after a draw, the 64 MiB buffer held 62 MB more (measured). An 8 MiB buffer written 1 MiB a frame and drawn held no more over 600 frames, nor a 16-bit index buffer drawn from an odd first index (measured).
- Every ANGLE Metal resource and drawing buffer is attributed to the page's process (documented).
- The context is made without depth or stencil (`gpu::make_webgl_context`): WebKit gives the default framebuffer a canvas-size depth buffer otherwise (Depth32Float on Metal, 15 MB at 2868 × 1320; documented), which wgpu never uses.

## Design: one heap, its large blocks small or streamed (#399)

User decision (2026-11-25), from the measurements below: the engine keeps one heap. What needs new memory is the level's large blocks, not its small ones, so each large block is made smaller or read as a stream into its final form (#403): the collision world (done, #371), the registry (done, #370), `game.graph` and `all.spawn` (streamed, #403), the navigation data, and anything else large. The one arena is a bump arena for a frame's temporaries (the scene, the draw list), all freed at the frame's end (#402); pools (#404) and replacing the level's `BTreeMap`s (#405) follow only if measurement shows they matter. Heap minus live is measured after the first and second New Game after each step; arenas are revisited only if it stays large. No other allocator algorithm is tried (#400, not planned).

Done when heap minus live after the first and the second New Game is near zero (#399). Each step is measured natively (`loading_memory.rs`: each stage's peak, kept bytes, allocations and blocks kept; `frame_memory.rs`) and in the browser (`heap_log`).

For the frame arena, containers that take an allocator exist on stable Rust: `allocator-api2` (MIT/Apache-2.0) is a stable copy of the `Allocator` trait; `hashbrown` implements its maps over it (its `allocator-api2` feature, on by default in 0.17), `bumpalo` has an optional `allocator-api2` feature, and `allocator-api2` has its own `Vec` and `Box` (each documented in the crate's manifest). Std's `BTreeMap` takes no allocator on stable (its parameter is behind the unstable `allocator_api`, documented).

### Rejected: heaps by lifetime (#401, not planned)

The design first approved (#399) gave the level, loading and the frame each a heap of its own beside the general one: an instance of dlmalloc (the `dlmalloc` crate, MIT/Apache-2.0, the allocator the wasm target's `System` already is) over segments of 64 KiB pages taken from the general heap, each allocation placed by the scope its code ran in (an implicit context allocator, as Odin's `context.allocator`, documented), a page table finding a block's heap when it is freed. Built on the branch `memory-heaps-experiment` (commit e6135e9), not merged: those heaps, with the level scope around every stage, the level's update and its making; the loading scope around files of 64 KiB or more read whole, the parsed `level` file, the static draw list's making and the objects' model parsing; the loading heap given back whole once empty and the level heap's empty segments at each stage's end. Natively it placed what it should (`loading_memory.rs`, measured): the loading heap emptied after each stage and the level heap reserved 83.6 MB for 77.7 MB live at the load's end (88.6 MB before the loading scope), the waste in the offline simulation's stage (5.4 MB) and the objects' (2.9 MB). The frame scope was measured natively only: the first draw list's lasting state (the world's upload) kept 4.1 MB in the frame heap and 13.1 MB reserved.

In headless Chrome (`menu` suite, MiB, heap after the New Game; one run each):

| Variant | First | Second |
|---|---|---|
| Before (the general heap only) | 106 | 110 |
| Level heap, segments kept for good (256 KiB) | 119 | 132 |
| Level and loading heaps, empty segments given back (256 KiB) | 120 | 131 |
| The same, 64 KiB segments | 122 | 130 |

The heap grew in every variant: under `st_loading_cform` by 37 MiB for 39 MiB more live (28 before). Heaps apart from the general one need their segments whole: the general heap's free space before the game (48 MiB of 59, split by what the import and the menus left) serves its own small blocks but not the level heap's segments, so the level's blocks moved from the general heap's holes to new memory at its top; and each heap's free space (the level heap's 11 MiB above live in the browser) is no longer the others'. Separate heaps cost the sharing of free space that one heap has, and here the sharing is worth more than the separation.
