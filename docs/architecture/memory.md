# Memory

How the engine's memory is laid out: the heaps, what lives in each, and what they measure. Numbers are for a New Game of `l01_escape` on the Russian edition and are marked measured (with where), documented (with the source) or believed. The design that the migration works towards, and what its first implementation measured, is in its own section, [Design: heaps by lifetime](#design-heaps-by-lifetime-399); everything before it describes the code as it is.

## Heaps and allocators

There are no arenas: every allocation goes to one general heap, the target's system allocator.

| Target | Allocator | Notes |
|---|---|---|
| wasm32 (the engine's worker, the audio worker) | Rust's `System`: dlmalloc over the wasm linear memory, grown with `memory.grow` | The memory never shrinks (documented: WebAssembly has no instruction to release memory), so a load's high-water mark stays the page's. One instance each, single-threaded (built without atomics). |
| Native (tests, tools) | Rust's `System`: the platform's `malloc` | Gives memory back; the memory tests count requested bytes instead of reading a footprint. |

Two reuse fixes from #357 stand in for arenas:

- The collision world's five arrays are kept when it is dropped (a thread-local spare in `volya-engine-core::collision`) and reused by the next world when large enough, so a second New Game's collision world finds its 30 MB without new memory (it needed up to 13 MiB more before, measured in headless Chrome).
- Prepared textures share one read buffer and one transcoding buffer, and texture writes go in bands of 2 MiB.

## Measurements by lifetime (#399)

Measured natively (release, M-series Mac) with counting allocators: `volya-engine/tests/loading_memory.rs` per loading stage (budgets reported as "OVER BUDGET" notes, never failures), `volya-engine/tests/frame_memory.rs` per frame of the running level. Bytes are as requested, so the allocator's overhead is not in them.

### Held across levels

| What | Live | Blocks |
|---|---|---|
| The archive tree (`DbVfs`: every file's entry and every directory's names, `HashMap<String, _>`, `BTreeSet<String>`) | 7.7 MB | 87,747 |
| The game's data before any game (`GameData`): the system configuration 5.6 MB, string tables 1.8 MB, UI 0.2 MB | 7.6 MB | 118,008 |
| The collision world's spare arrays after a level (#357) | 30.1 MB (42.0 before #371) | 5 |

### Held for the level

What each loading stage keeps, and the blocks it leaves live:

| Stage | Keeps | Blocks kept | Allocations made | Peak above the stage's start |
|---|---|---|---|---|
| `st_creating_new_game` | 4.7 MB | 66 | 452 | 12.8 MB |
| `st_server_connecting` (the offline simulation: registry 14.8, navigation 7.1, graph and paths 1.8) | 19.8 MB | 108,551 | 603,156 | 19.8 MB |
| `st_opening_stream` (the `level` file's parts) | 7.8 MB | 6,887 | 41,503 | 12.6 MB |
| `st_loading_cform` (collision: vertices 8.4, triangles 11.0, materials 1.4, grid 9.3; before #371 triangles 16.5 and grid 15.8) | 30.7 MB (42.7 before) | 9,654 | 14,924 | 35.1 MB (45.0 before) |
| `st_loading_geometry` | 0.4 MB | 121 | 205 | 0.7 MB |
| `st_loading_spatial_db` | −6.0 MB | −6,486 | 117 | 5.4 MB |
| `st_loading_ai_objects` (the particles' effect library) | 1.9 MB | 8,417 | 18,083 | 3.3 MB |
| `st_client_spawning` (objects' models, weather, level sounds; the objects' geometry 4.9 until uploaded) | 6.2 MB | 6,929 | 2,459,422 | 11.0 MB |
| `st_loading_textures` | 0.3 MB | 1,639 | 103,158 | 3.1 MB |

The level keeps 66.0 MB at the end of loading in 136,065 blocks (77.9 MB before #371), 61 MB once its geometry is uploaded; the load's peak is 70.3 MB above its start (82.3 before). The game data a level needs resident is about 48 MB (`level.cform` as stored 30.4, the navigation positions 7.1, `all.spawn` 4.5, the graph's tables 1.8, the configuration's text 1.4, the strings 1.8, the effect library 1.9); the rest lives on the GPU as stored.

### Loading temporaries

Each stage's peak above what it keeps: about 24 MB in all, the largest 8.1 MB (`st_creating_new_game`: the spawn file and the game graph read whole), 4.8 (`st_client_spawning`), 4.8 (`st_opening_stream`), 5.4 (`st_loading_spatial_db`), 2.8 (`st_loading_textures`), 2.3 (`st_loading_cform`). By count they are the many: `st_client_spawning` makes 2.46 million allocations to keep 6,929 blocks, `st_server_connecting` 603,156 to keep 108,551.

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

| Path | After the first New Game | After the second |
|---|---|---|
| WebGPU | 98 heap, 87 live at most | 98 heap, 87 live at most |
| WebGL 2 (BC textures as stored) | 106 heap, 95 live at most | 110 heap, 97 live at most |

The audio worker's instance adds 3–4 MiB (#354). Before the New Game the heap already stands at 59 MiB with 11 MiB live: the import and the menus reached 35 MiB live, and what they left split the free space, so the collision world's arrays (up to 16 MB each) did not fit and the heap grew from 59 to 87 MiB under `st_loading_cform` with 39 MiB more live. On WebGL 2 the level holds about 6 MiB more live than on WebGPU, and its first world frame peaks about 10 MiB above the frame before: wgpu records the world's render bundles in our heap there (#372).

### What fragments the heap

A source is substantial when it leaves many small long-lived blocks between large ones, or makes large temporaries where long-lived blocks then settle:

- the offline simulation's registry: 108,551 blocks for 8,641 objects (#370), made after the previous level's blocks were freed and before the collision world's are allocated;
- the loading temporaries: 2.46 million allocations under `st_client_spawning` alone, interleaved with what the stage keeps;
- the level's other small long-lived blocks: the `level` file's parts (6,887), the effect library (8,417), the objects (6,929);
- the collision world's large arrays, which need contiguous space (#371);
- the frame's temporaries, 285 KB a frame in about 300 blocks, made and freed between the level's.

## GPU and JavaScript memory

Measured in headless Chrome (`menu` suite, `level_stats`) for the first level: textures 133 MB (the level's and the objects' 24 MB, as stored), the level's geometry 62.7 MB on WebGPU and 50.6 MB on WebGL 2, the objects' geometry 5.0 and 6.3 MB. They live in the browser's GPU process, outside the wasm heap; on WebGL 2 wgpu also keeps staging and recorded commands in the wasm heap (#372). What the page and the workers hold in JavaScript is not measured (#374).

## Design: heaps by lifetime (#399)

User-approved (2026-11-25): memory is organised by lifetime, and every substantial fragmentation source above moves into the heap of its lifetime. No other allocator algorithm is tried (#400, not planned).

### Heaps

Four heaps, one per lifetime:

| Heap | Holds | Freed |
|---|---|---|
| General | The archive tree, the game's data, menus, the renderer and wgpu, anything not placed elsewhere | Block by block |
| Level | What a level's loading makes and keeps, and what the running level adds to it | Block by block; empty once the level is dropped |
| Loading | A stage's temporaries: files read whole, parse intermediates | Empty at each stage's end |
| Frame | A frame's temporaries: the scene, the draw list, sound buffers | Empty at each frame's end |

Each of the level, loading and frame heaps is its own instance of dlmalloc (the `dlmalloc` crate, MIT/Apache-2.0: the allocator the wasm target's `System` already is, documented in the crate) over segments taken from the general heap in multiples of 64 KiB pages, at least 256 KiB at a time, aligned to a page so each page belongs to one heap. Segments are never given back: what a heap frees stays its own for the next level, load or frame, and the general heap's long-lived blocks never settle between them. Taking segments from the general heap, not from `memory.grow`, lets them fill the free space the general heap already has; a split of the heap's address space in two (#357) made the heap larger because neither half could use the other's free space (measured, one run).

They nest in time: the general heap lives for the page; a level heap's blocks for a level, a second New Game's level overlapping the first's only while the first is dropped; loading's within a stage; a frame's within a frame, which runs under the loading screen as well as in the level. In space they do not nest: each heap's segments are blocks of the general heap.

### Placement by scope

An allocation goes to the heap of the scope its thread runs in (`memory::enter(Lifetime::Level)`, restored when the guard is dropped): loading's stages and the level's update run in the level scope, the stages' known temporaries in the loading scope within it, the scene and draw list in the frame scope. A block stays in the heap it was made in: freeing or growing it finds its heap from its address through a page table (one byte per 64 KiB page), whatever scope runs then. A wrong scope therefore costs only placement, never correctness: a block a level scope makes and something global keeps simply stays in the level heap.

Why scopes rather than containers with an allocator parameter: the level's holders are deep trees of the standard containers (an object record holds its strings, vectors and boxed kind; the configuration its sections and entries), shared with the format crate and the save code. Scopes place every nested allocation from one guard at the stage's boundary, without retyping them. This is the implicit context allocator of Odin (`context.allocator`, documented in the language overview) and the heap stacks game engines push per level (believed: commonly described, not checked against a specific engine).

Containers that take an allocator remain possible over the same heaps on stable Rust, for a holder that must grow in its heap from outside its scope: `allocator-api2` (MIT/Apache-2.0) is a stable copy of the `Allocator` trait; `hashbrown` implements its maps over it (its `allocator-api2` feature, on by default in 0.17, documented in its manifest), `bumpalo` has an optional `allocator-api2` feature (documented), and `allocator-api2` has its own `Vec` and `Box`. Std's `BTreeMap` takes no allocator on stable (its parameter is behind the unstable `allocator_api`, documented). None is used until a holder needs it; scoped placement needs no replacement of the level's `BTreeMap`s (#405).

Why heaps and not bump arenas released whole: a bump arena reuses nothing until it is reset, and can only be reset once all its blocks are gone. Loading interleaves temporaries with what it keeps, and the running level frees and regrows its data; a heap per lifetime reuses its own freed space and needs no proof of emptiness. Bump allocation stays a choice for the frame, whose blocks are all gone at its end.

### The first implementation, measured (#401)

Built on the branch `memory-heaps-experiment` (commit e6135e9), not merged: the heaps above, with the level scope around every stage, the level's update and its making; the loading scope around files of 64 KiB or more read whole, the parsed `level` file, the static draw list's making and the objects' model parsing; the loading heap given back whole once empty and the level heap's empty segments at each stage's end. Natively it placed what it should (`loading_memory.rs`, measured): the loading heap emptied after each stage and the level heap reserved 83.6 MB for 77.7 MB live at the load's end (88.6 MB before the loading scope), the waste in the offline simulation's stage (5.4 MB) and the objects' (2.9 MB). The frame scope was measured natively only: the first draw list's lasting state (the world's upload) kept 4.1 MB in the frame heap and 13.1 MB reserved.

In headless Chrome (`menu` suite, MiB, heap after the New Game; one run each):

| Variant | WebGPU, first | WebGPU, second | WebGL 2, first | WebGL 2, second |
|---|---|---|---|---|
| Before (the general heap only) | 98 | 98 | 106 | 110 |
| Level heap, segments kept for good (256 KiB) | 108 | 111 | 119 | 132 |
| Level and loading heaps, empty segments given back (256 KiB) | 107 | 111 | 120 | 131 |
| The same, 64 KiB segments | 113 | 120 | 122 | 130 |

The heap grew in every variant: under `st_loading_cform` by 37 MiB for 39 MiB more live (28 before). Heaps apart from the general one need their segments whole: the general heap's free space before the game (48 MiB of 59, split by what the import and the menus left) serves its own small blocks but not the level heap's segments, so the level's blocks moved from the general heap's holes to new memory at its top; and each heap's free space (the level heap's 11 MiB above live in the browser) is no longer the others'. Separate heaps cost the sharing of free space that one heap has, and here the sharing is worth more than the separation.

### Open

What the measurements leave, for the user to decide (#401): the level's large blocks are what needs new memory (the collision world's arrays, up to 11 MB each since #371 (16.5 before), the game graph and the spawn file read whole, 8.1 and 4.5 MB, the navigation 7.1 MB), not its small ones. Ways that keep one heap's sharing:

- make the large blocks smaller: the collision grid and triangles packed (#371), the registry compacted (#370), the game graph and spawn file streamed (#403);
- split the large blocks into fixed pieces (1 MiB, say) that any free space of that size takes, and that the next level's pieces reuse;
- a level arena of such pieces released whole, with the level's temporaries kept out of it (an experiment, not yet measured).

### Migration

Each step is measured natively (`loading_memory.rs`: each stage's peak, kept bytes, allocations and blocks kept; `frame_memory.rs`) and in the browser (`heap_log` after the first and second New Game, both paths).

Done when heap minus live after the first and the second New Game is near zero on both paths (#399).
