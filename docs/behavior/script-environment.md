# Script environment: which Lua

Which public Lua implementation the shipped S.T.A.L.K.E.R.: Shadow of Chernobyl binaries (Russian edition)
embed, and which extensions that runtime gives scripts. Nothing else is covered.

Conventions:

- Files are in the game's `bin\` directory, identified by size and MD5:

  | File | Bytes | MD5 |
  |---|---|---|
  | `xrLUA.dll` | 290,816 | `a65ba53867962afc92e0f17409e9cd1f` |
  | `xrGame.dll` | 6,246,400 | `19938b4b9fa11e3b4b6462ce7e7a12ac` |
  | `XR_3DA.exe` | 1,636,424 | `ba3ad0127bff6619ed991a7c82d8b946` |
  | `dedicated\XR_3DA.exe` | 1,599,560 | `2e668e1acb298b403c6fd2d67575fb91` |

- "At offset X" means the file holds exactly those bytes from file offset X (hexadecimal), followed by a NUL
  byte unless stated otherwise; a hex viewer confirms it. Export and import names are read from the PE export
  and import tables (any PE viewer lists them).
- Each fact is **measured** (read from file bytes or tables), **static** (from static analysis of the
  binaries, not observed at run time) or **unestablished**. No binary was run: no Windows execution
  environment is set up.

## 1. The implementation and version

The Lua runtime is **LuaJIT 1.0.3**, which identifies itself as built on **Lua 5.1 (alpha)**. It is in
`xrLUA.dll`, and only there.

### 1.1 Self-identification strings (measured, `xrLUA.dll`)

| Offset | Bytes |
|---|---|
| `0x37940` | `$Lua: Lua 5.1 (alpha) + LuaJIT 1.0.3 Copyright (C) 1994-2005 Lua.org, PUC-Rio $`, line feed (0A), `$Authors: R. Ierusalimschy, L. H. de Figueiredo & W. Celes $`, line feed, `$URL: www.lua.org $`, line feed, then NUL |
| `[position removed]` | `Lua 5.1 (alpha) + LuaJIT 1.0.3`; the next string, at `0x37ed8`, is `_VERSION` |
| `0x386e8` | `[LuaJIT frontend failed: %s]`, line feed, then NUL |
| `0x38790` | `JIT engine init failed (%08x)` |
| `0x38774` | `jit.util` |

These are the only occurrences of `LuaJIT` (at `[position removed]`, `[position removed]`, `0x386e9`) and of `Lua 5.1` (at
`[position removed]`, `[position removed]`) in `xrLUA.dll`. The string at `[position removed]` is the value of the global `_VERSION` (static).

### 1.2 Library signatures (measured, `xrLUA.dll` exports)

422 named exports: 132 undecorated C names and 290 Microsoft-decorated C++ names, all of the latter in the
namespace `luabind` (the public C++ binding library; its version is **unestablished**, as no luabind version
string is present). The C names are:

- 81 beginning with `lua_` and 37 with `luaL_`: the public Lua C API naming, including names that the public
  Lua 5.1 API has (for example `lua_pushinteger`, `lua_setfield`, `luaL_checkinteger`);
- 10 library openers: `luaopen_base`, `luaopen_coco`, `luaopen_debug`, `luaopen_io`, `luaopen_jit`,
  `luaopen_math`, `luaopen_os`, `luaopen_package`, `luaopen_string`, `luaopen_table`;
- `luaJIT_compile`, `luaJIT_setmode`, `luaS_newlstr`, `luaV_strcmp`.

`luaopen_jit`, `luaopen_coco` and the two `luaJIT_` names are LuaJIT 1.x additions to the Lua API.

### 1.3 Which file contains it (measured)

- No other file under `bin\` (recursively) contains the bytes `LuaJIT` or `Lua 5.`.
- Exactly three binaries import from `xrLUA.dll`: `xrGame.dll` (108 names), `XR_3DA.exe` (76) and
  `dedicated\XR_3DA.exe` (76). No binary under `bin\` other than these and `xrLUA.dll` itself contains the
  bytes `xrLUA`.

### 1.4 Not established

- Whether `xrLUA.dll` differs from the public LuaJIT 1.0.3 release: no public release was obtained or
  compared.

## 2. What the runtime gives scripts

The game creates two separate Lua states: one in `xrGame.dll` (game scripts) and one in `XR_3DA.exe` (shader
scripts) (static).

### 2.1 Standard libraries opened

Openers imported (measured, import tables):

- `xrGame.dll`: `luaopen_base`, `luaopen_table`, `luaopen_string`, `luaopen_math`, `luaopen_jit`,
  `luaopen_coco`.
- `XR_3DA.exe` and `dedicated\XR_3DA.exe`: `luaopen_base`, `luaopen_table`, `luaopen_string`,
  `luaopen_math`, `luaopen_jit`, and `luaJIT_setmode`.
- No binary imports `luaopen_io`, `luaopen_os`, `luaopen_debug` or `luaopen_package`.

Libraries opened (static):

| State | Always | Unless the command line contains `-nojit` |
|---|---|---|
| game (`xrGame.dll`) | `base`, `table`, `string`, `math` | `jit`, `coco` |
| shader (`XR_3DA.exe`) | `base`, `table`, `string`, `math`, `jit` | (no such check) |

`xrGame.dll` contains `-nojit` at `[position removed]` (its only occurrence); the test is a substring search of the
command line, which is lowercased first, so it is case-insensitive (static). So `io`, `os`, `debug`,
`package`, `require` and `module` never exist.

Name tables of these libraries in `xrLUA.dll` (measured: arrays of name and function pairs ending in a pair
of zeros, found by scanning; static: which opener installs which):

| Table | Names |
|---|---|
| globals (`base`) | `assert` `collectgarbage` `dofile` `error` `gcinfo` `getfenv` `getmetatable` `loadfile` `load` `loadstring` `next` `pcall` `print` `rawequal` `rawget` `rawset` `select` `setfenv` `setmetatable` `tonumber` `tostring` `type` `unpack` `xpcall` |
| `coroutine` (by `base`) | `running` `status` |
| `coroutine` (by `coco`) | `resume` `yield` `create` `wrap` `cstacksize`, and the field `coco` = `true` |
| `table` | `concat` `foreach` `foreachi` `getn` `insert` `remove` `setn` `sort` |
| `string` | `byte` `char` `dump` `find` `format` `gfind` `gmatch` `gsub` `len` `lower` `match` `rep` `reverse` `sub` `upper` |
| `math` | `abs` `acos` `asin` `atan2` `atan` `ceil` `cosh` `cos` `deg` `exp` `floor` `fmod` `frexp` `ldexp` `log10` `log` `max` `min` `modf` `pow` `rad` `random` `randomseed` `sinh` `sin` `sqrt` `tanh` `tan` |
| `jit` | `on` `off` `debug` `compile` `compilesub` |
| `jit.util` | `stats` `bytecode` `const` `upvalue` `closure` `closurenup` `mcode` `mcodeaddr` `jsubmcode` |

So without `coco` (game state with `-nojit`, and the shader state always) there is no `coroutine.create`,
`resume`, `yield` or `wrap` (static). Fields that these libraries set outside their name tables are not
listed here.

### 2.2 JIT control

- The `jit` library (2.1) is opened in the shader state always and in the game state unless `-nojit`.
- Opening it switches the state's JIT compiler on; if that fails, opening raises the error
  `JIT engine init failed (%08x)` (static).
- The shader state switches the compiler on again after loading its scripts, through the imported
  `luaJIT_setmode` (static).
- Whether code is actually compiled to machine code at run time is **unestablished** (not run).

### 2.3 Bit operations

None provided by the runtime (measured): `xrLUA.dll` contains neither the bytes `bit` nor `band` anywhere,
and has no such opener or name table. Whether the engine itself provides bit operations is outside this
spec.

### 2.4 ffi

None (measured): `xrLUA.dll` exports no `luaopen_ffi`, and its only occurrence of the bytes `ffi` is inside
the export name `luaL_buffinit` (at `[position removed]`).

### 2.5 Number type

- One number type: the IEEE-754 64-bit double (static). Numbers are stored as 8-byte doubles with one type
  tag; integers passed in from C are converted to double under the same tag, and conversions to integers
  round the double. There is no separate integer representation.
- `xrLUA.dll` contains the format `%.14g` at `[position removed]` (measured), the number-to-string format (static).

## Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the four binaries above, plus
  `xrCore.dll` for the command-line lowercasing.
- Analysis: file bytes, sizes and MD5s read directly; PE export and import tables read by a throwaway parser;
  name tables found by a throwaway scanner of `xrLUA.dll`'s data sections; static analysis with Ghidra of the
  library openers and number handling in `xrLUA.dll`, the state creation in `xrGame.dll` and `XR_3DA.exe`,
  and the command-line copy in `xrCore.dll`. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used. Library
  names and versions come only from the strings and exports above.
