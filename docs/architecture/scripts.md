# Scripts

Scripts run on the runtime the original embeds (docs/behavior/script-environment.md): the public LuaJIT 1.0.3 (Lua 5.1 alpha), interpreter only, decided in #91. Crate `crates/volya-engine-script`.

## Source and licence

- `vendor/luajit-1.0.3/` holds only the interpreter's files (the Lua core; the base, table, string, math, jit and coco libraries; their headers; `COPYRIGHT`), plus `lcoco.c`, not built, for its licence notice. Vendored files are never modified: the build patches a copy.
- Licence MIT: `COPYRIGHT` for Lua, the notices in `luajit.h` and `lcoco.c` for Mike Pall's parts. `csrc/volya_jit.c` and `csrc/volya_coco.c` derive from `ljit_core.c` and `lcoco.c` and carry MIT too; the crate is `BSD-3-Clause AND MIT`.
- Provenance: `LuaJIT-1.0.3.tar.gz` from `http://luajit.org/download/LuaJIT-1.0.3.tar.gz`, fetched from the Internet Archive (`https://web.archive.org/web/20160318172846id_/http://luajit.org/download/LuaJIT-1.0.3.tar.gz`; its captures from 2007 to 2016 share one SHA-1), 308,336 bytes, SHA-256 `e39204aad8d2a3f9ef74a4a515fedf3cce3f55ff247af8ae50f2a8cb01f257c3`. `LUA_VERSION` is `Lua 5.1 (alpha) + LuaJIT 1.0.3`, the spec's string.

## Build

- `build.rs` with the `cc` crate: natively the system C compiler; for wasm32 the clang of Bazel's pinned LLVM (wasi-sdk's, docs/architecture/build.md; Apple's has no wasm target), which the target names in `VOLYA_WASM_CLANG`.
- Vendored files compile from a copy in `OUT_DIR`, patched by `build.rs`; each replacement must match exactly once. `csrc/volya_rand.h` is force-included before every vendored file.
- `llex.c`: the original's C-style comments (docs/behavior/script-loading.md 1), in every chunk: a case for `/` where a token can start; `//` to the next LF; `/* */` without nesting, needing no end and counting no line breaks, a `*` taken with the byte after it.
- `luaconf.h`: numbers convert to integers with the header's x86 method on every target (add 2^52 + 2^51, take the double's low 32 bits): round to nearest, ties to even, as the spec's x86 build does, with out-of-range values wrapping, where our targets' casts would truncate. String-to-number is our decimal-only `volya_strtod` (`src/libc.rs`) on every target: C99 libraries also read hexadecimal, `inf` and `nan`; Lua 5.1 alpha is written for ANSI C's. The release's other defaults stand (`string.gfind` is `gmatch`, `math.mod` is `fmod`, `table.setn` raises "obsolete"); the spec doesn't establish the original's.

## Runtime

- **No machine-code compiler.** DynASM and the x86 backend (`ljit_core.c`, `ljit_mem.c`, `ljit_backend.c`, `ljit_dasm.c`) aren't built; `csrc/volya_jit.c` implements the engine interface without them. The `jit` library keeps every function: `jit.on`, `jit.off`, `jit.debug` and per-function modes change only flags; `jit.compile` and `jit.compilesub` return `jit.util.status.ENGINE_OFF`, every function's status is `ENGINE_OFF`. `jit.util`'s bytecode introspection works; `mcode`, `mcodeaddr` and `jsubmcode` are replaced in `csrc/volya_glue.c` and raise "jit.util.X is unavailable".
- **Coco without C stacks.** Stack switching can't be done in wasm: `csrc/volya_coco.c` provides Coco's C API with plain Lua threads, and Coco's coroutine library is kept (`create`, `resume`, `yield`, `wrap`, `cstacksize`, `coroutine.coco`). A yield across a C call (metamethod, `pcall`, iterator, Rust function) raises `attempt to yield across metamethod/C-call boundary` (#100).
- **`math.random`**: one generator on every target, the C standard's example `rand` (`RAND_MAX` 32767, seed 1; `csrc/volya_rand.c`), until the original's is established (#106).
- **Libraries**: what the original's game state opens by default (spec 2.1): base, table, string, math, jit, coco; never io, os, debug or package (`open_libs`, `csrc/volya_glue.c`).
- **Errors**: Lua's setjmp/longjmp. Natively the C library's; in wasm32 LLVM's lowering to WebAssembly exception handling (`-mexception-handling -mllvm -wasm-enable-sjlj`, legacy encoding) with our runtime (`csrc/wasm/libc.c`, the `__c_longjmp` tag in `csrc/wasm/c_longjmp.S`). Legacy exception handling is in Chrome 95+, Safari and iOS Safari 15.2+, Firefox 100+ (documented, MDN browser-compat-data; measured with the `platform` suite in Chrome and the iOS Simulator's Safari); the newer `exnref` encoding needs Chrome 137+ and Safari 18.4+ (documented), so it isn't used.
- **Memory**: Lua's allocator is Rust's on every target (`lua_alloc`, `src/libc.rs`) and aborts when memory runs out, so Lua never raises a memory error; a recoverable per-state budget is #107.
- **C library for wasm32** (`csrc/wasm`): headers and the functions the interpreter calls. `malloc` and friends are Rust's (`volya-engine-core`'s `c_heap`, shared with the physics solver's C++); `printf`-family formatting is ours on every target (`csrc/volya_printf.c`, floating-point digits from Rust's formatting, checked against the system's natively); `frexp` and `modf` are ours, other math comes from Rust's compiler-builtins (the native build uses the system's; results may differ in the last bit); ctype is the "C" locale; no files (`fopen` fails, so `dofile` and `loadfile` fail to open), stdin is empty, `print` output is discarded.

## Rust API

- `Lua`: create a state, `exec` or `compile` a named chunk, `call` a global function, `register` Rust functions. Values: nil, booleans, numbers, strings (bytes), tables, and `Other` (functions, userdata, threads by type name; read only).
- Lua's longjmp never crosses Rust frames: every Lua call that can raise runs in C inside `lua_cpcall` (`csrc/volya_glue.c`); Rust only reads values from stack slots C reserved beforehand (tables up to 64 levels deep, `MAX_DEPTH`; self-containing tables refused), which allocates nothing in Lua and raises nothing, and gets errors as `Result`s. A Rust function's results or error are handed back to C, which pushes or raises them; a panic in one becomes a Lua error natively (wasm aborts on panic).

## Loading scripts

`loader`, docs/behavior/script-loading.md 2 (#109).

- `Scripts` is a game script state over the game's file trees, lent per read through `Trees` (an `Rc<dyn Trees>`, as the loader is called back from Lua), like the web platform's `with_trees`. Scripts are read from the archive tree. Drop it before the platform unmounts the files.
- Script name N is `$game_scripts$N.script`, compiled as the game builds the chunk: the file's bytes unchanged after the one-line namespace prefix (default or `-_g`); none for `_G`.
- The loader's Lua side (`src/loader.lua`) runs before any script; its entry function is kept in the registry, and scripts reach it only through C functions (the globals' `__index` handler, `prefetch`), so no Lua function of the loader, and none of the Rust functions it holds, is reachable (`jit.util` reads only Lua functions' upvalues; there is no debug library).
- `load_globals` is startup's `_G`; `load_namespace`, `exec` and `call` may load more. Failures (including an unmakeable path and a failing `__tostring`) are discarded in Lua as the original does and kept for the engine (`take_failures`). `Common` reads `script.ltx`'s `[common]` lists.
- Our choices where the spec is silent or Lua can't follow it: the fatal error can't unwind past a script's `pcall`, so the call it happens in returns `ErrorKind::Fatal` whatever scripts did, every later call does too, and every later load raises it again; its message's `namespace N: ` prefix; a non-table before the last component is fatal too; `prefetch` converts numbers and raises for other non-strings; `_G` is compared case-sensitively; an unreadable file isn't remembered as missing; chunks are named `@` plus our lowercase path relative to the game root.
- `tests/gog.rs` (`every_loadable_script_compiles`, with game data) holds the shipped scripts to this: all 437 of the Russian edition's scripts with loadable names compile except 4 that aren't Lua, none reachable (spec 4.1); the 2 under `config\` are never loaded.
