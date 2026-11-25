# Script loading: which files, and what text the runtime compiles

How the shipped S.T.A.L.K.E.R.: Shadow of Chernobyl binaries (Russian edition) find and load the game's
`*.script` files, which text they hand to the Lua compiler, and the comment syntax that compiler accepts
beyond the public LuaJIT 1.0.3 release. Builds on the script environment specification
(`docs/behavior/script-environment.md`: the runtime, its libraries, the game state and the shader state),
the archive specification (`docs/formats/db-archive.md`: file tree and precedence) and the LTX
specification (`docs/formats/ltx-config.md`: syntax of `script.ltx`, path aliases in section 10).

Conventions:

- Binaries are those of the script environment specification, identified by size and MD5:
  `xrLUA.dll` (290,816 bytes, `a65ba53867962afc92e0f17409e9cd1f`) and `xrGame.dll` (6,246,400 bytes,
  `19938b4b9fa11e3b4b6462ce7e7a12ac`). "At offset X" means the file holds exactly those bytes from file
  offset X (hexadecimal), followed by a NUL byte.
- Each fact is **measured** (read from file bytes or game data, or by compiling game data with the public
  LuaJIT 1.0.3 release), **static** (from static analysis of the binaries, not observed at run time) or
  **unestablished**. No binary was run.
- "The public release" is the public LuaJIT 1.0.3 source distribution (a third-party library).
- Byte values are hexadecimal: LF is 0A, CR is 0D. "Line break" means what Lua 5.1 counts as one: LF, CR,
  or a CR LF or LF CR pair.
- The game data examined is the file tree of the root-directory archives `gamedata.db0` … `gamedata.dbd`
  with the archive specification's precedence (the `mods\` archive holds no `*.script` file). In it, 442
  paths end in `.script`: 440 directly in `scripts\` and 2 under `config\` (`config\misc\gulag_radar_u.script`,
  `config\scripts\qa.script`). `scripts\` has no subdirectories.

## 1. Comment syntax of the embedded runtime

### 1.1 What differs from the public release (static)

The embedded runtime's lexical analyzer accepts two comment forms that the public release does not: a
**line comment** starting with `//` and a **block comment** starting with `/*`. Nothing else in the lexical
analyzer differs: its handling of white space, line breaks, names and reserved words, numbers, quoted
strings and their escapes, long brackets (including the error for `[[` nested inside `[[ … ]]`), `--`
comments, the operators and the end of input matches the public release, and so do all its error messages.

The reserved-word and token-name table is identical to the public release's (measured, `xrLUA.dll`): `and`
`break` `do` `else` `elseif` `end` `false` `for` `function` `if` `in` `local` `nil` `not` `or` `repeat`
`return` `then` `true` `until` `while` `..` `...` `==` `>=` `<=` `~=` `<number>` `<name>` `<string>` `<eof>`, in
this order. So there are no new keywords or token kinds; whether the parser (grammar and code generation)
differs anywhere is **unestablished** (it was not compared), but every game script it must accept is
accepted by the public parser once the rules below are applied (section 4).

The extension is in the runtime itself, so it applies to every chunk the runtime compiles: script files,
chunks compiled by scripts (`loadstring`, `load`, `dofile`), and the shader state's scripts alike (static).

### 1.2 Where a comment can start (static)

`//` and `/*` start a comment only where a token can start: not inside a quoted string, not inside a long
string (`[[ … ]]`, `[=[ … ]=]`, …), and not inside a `--` comment (short or long). Inside a `//` or `/*`
comment, `--`, quotes and brackets have no meaning.

A `/` that is not followed by `/` or `*` is the division operator, as in the public release.

In Lua 5.1, `/` can never be directly followed by `/` or `*` outside strings and comments in a valid chunk
(neither `/` nor `*` can start an expression), so every chunk the public release accepts is lexed by the
embedded runtime into exactly the same tokens: the extension only makes otherwise invalid text valid.

### 1.3 Line comment `//` (static)

The comment runs from `//` up to, **not including**, the next LF byte (0A), or to the end of the chunk if
there is none. Only LF ends it: a CR (0D) is part of the comment. The LF that ends it is then an ordinary
line break (counted for line numbers, and a CR LF or LF CR pair is one break as usual). So in files with CR
LF or LF line breaks, a `//` comment ends at the end of its line, exactly like a `--` comment; in a file
whose line breaks were bare CRs it would run on to the next LF.

### 1.4 Block comment `/* … */` (static)

- The comment starts at `/*`. From the byte after the `*`, the runtime scans bytes as follows: a byte that
  is not `*` is skipped; a `*` is consumed **together with the byte after it**, and the comment ends if that
  following byte is `/`; otherwise both are skipped and scanning continues after them.
- Consequence: the `*` of a `*/` is recognized only if it is not itself the byte consumed after a previous
  `*`. `/**/` and `/* x */` end where C would end them. `/***/` does not end: after `/*`, the bytes `**`
  are consumed as a pair and the `/` is then an ordinary skipped byte. Likewise `/* a **/` does not end at
  its `**/`; `/* a **/ b */` ends only at the final `*/`.
- No nesting: a `/*` inside a block comment has no meaning.
- An unterminated block comment is **not an error**: it runs to the end of the chunk, which then ends
  normally.
- **Line breaks inside a block comment are not counted.** Line numbers after the comment (in error
  messages and debug information) are lower than the physical line numbers by the number of line breaks
  the comment contains. Each CR, LF or CR LF / LF CR pair inside counts as what would have been one break.
- After the comment, lexing resumes with the byte following the closing `/`; the comment separates tokens.

### 1.5 An equivalent text transformation (for implementers using the public release)

Applying the following to a chunk's bytes, then compiling the result with the public release, yields the
same tokens and the same line numbers as the embedded runtime (static, from 1.2–1.4):

1. Scan the bytes, tracking quoted strings (with `\` escapes; a `\` followed by a line break continues the
   string), long strings and `--` comments as the Lua 5.1 lexer does, and copy them unchanged.
2. Outside them, delete each `//` comment: the bytes from `//` up to but not including the next LF (or to
   the end).
3. Outside them, replace each `/*` comment — from `/*` through its end as defined in 1.4, or through the
   end of the chunk if unterminated — by a single space (20). This also deletes its line breaks, matching
   the runtime's line numbering.
4. Copy all other bytes unchanged.

The verification facts in section 4 use exactly this transformation.

## 2. Script files and namespaces (game state, `xrGame.dll`)

### 2.1 Which file a name means (static)

Scripts are addressed by a **script name** N. The file for N is the path alias `$game_scripts$` followed by
N followed by `.script`, resolved by the path aliases (LTX specification, section 10; in this edition
`$game_scripts$` is `gamedata\scripts\`), so the lookup is case-insensitive and the archive precedence
applies. The extension is always `.script`, and the directory is always `$game_scripts$`: the two
`config\…\*.script` files are never loaded by the script engine.

The name `_G` is special: its file is `$game_scripts$_G.script`, i.e. `scripts\_g.script`, and it is loaded
into the global environment (2.3).

`.script` string at offset `[position removed]` and `$game_scripts$` at offset `[position removed]` of `xrGame.dll` (measured).

### 2.2 When a file is loaded (static)

A file is loaded into the **namespace** N, a table reachable from the globals as `_G.N` (for a dotted N
such as `a.b`, as `_G.a.b`). A namespace counts as **already loaded** when following N's dot-separated
components from the globals table by raw access ends at a table. If it ends at a value that is neither nil
nor a table, the game stops with a fatal error (a message containing "the namespace name is already being
used by the non-table object"). A namespace that is already loaded is not loaded again by the triggers
below. N is loaded on these occasions:

1. **Startup.** After creating the game state and registering the engine's script classes, the engine
   loads `_G`, i.e. `scripts\_g.script`, into the global environment. This load does not test whether `_G`
   is already loaded (the globals table always is).
2. **Class registrators and the common list.** `$game_config$script.ltx`, section `[common]`: the key
   `class_registrators` is a comma-separated list of function names, and the key `script` a comma-separated
   list of script names (LTX lists). Each class registrator `X.f` loads namespace `X` (as in occasion 4) and
   is then called; each name in `script` is loaded. In this edition, `class_registrators =
   class_registrator.register, game_registrator.register` and `script = _G` (already loaded by occasion 1).
3. **Automatic loading on global access.** The globals table has a metatable whose `__index` handler, when
   a key that is a string (or a number, converted to a string) is not present in the globals table, tries
   to load the script of that name and then returns the globals table's raw value for the key (nil if
   nothing was loaded). So reading an undefined global `xr_logic` — directly, or from a namespaced script
   whose environment falls back to the globals (2.3) — loads `scripts\xr_logic.script` into `_G.xr_logic`
   and yields that table. If the file does not exist, nothing is loaded and the value is nil; the game
   remembers the most recently missing name and does not look for that file again while that name is the
   one remembered. The empty string loads nothing.
4. **Function names called by the engine.** Where the engine calls a script function named in its data as
   `A.f` (the part after the last `.` is the function; the part before it the namespace), it loads namespace
   `A` first, whether or not its file was found earlier; a name without a `.` is looked up in the globals
   and loads nothing. This is established for `class_registrators`; other engine-called names (in
   `script.ltx` the keys `game_type_clsid_factory` and `ui_type_clsid_factory`, and the `namespace.function`
   names in other configuration) are **believed** to take the same path but were not each traced.
5. **`prefetch(N)`.** The global function `prefetch` (string at offset `[position removed]`), given a script name,
   loads it as in occasion 4.

The game-type sections of `script.ltx` (`[single]`, `[deathmatch]`, …) also have a `script` key listing
scripts; every one of them is empty in this edition (in `[single]`, the value starts with `;`, a comment),
and how such a list is processed is not covered here. The empty `levels` key of `[common]` is not covered
either.

### 2.3 The text handed to the compiler (static; strings measured)

The file's bytes are read whole and **not changed in any way**: no character-set conversion (the files are
Windows-1251 and their bytes reach the lexer as they are), no byte-order-mark removal, no line-ending
conversion. For N other than `_G`, a one-line **prefix** is put directly before them, with no separator:
the chunk is the prefix's bytes followed by the file's bytes. For `_G` the chunk is the file's bytes alone.

The prefix is one of two `xrGame.dll` strings, with its three `%s` replaced in order by: N; the
**opening** text; the **closing** text.

- Default (at offset `[position removed]`, 126 bytes):
  `local function script_name() return "%s" end local this = {} %s this %s setmetatable(this, {__index = _G}) setfenv(1, this) `
  followed by two tab bytes (09 09). (The text shown ends with one space, 20, before the tabs.)
- If the lowercased command line contains `-_g` as a substring (string at offset `[position removed]`; the same
  lowercasing and substring test as for `-nojit` in the script environment specification), this one is
  used instead (at offset `[position removed]`, 104 bytes):
  `local function script_name() return "%s" end local this = {} this._G = _G %s this %s setfenv(1, this) `
  followed by 09 09.

For N split at each `.` into components c1, c2, …, ck (all non-empty; otherwise the load fails, without any
message):

- opening = `c1=`, then for each later component `{` `ci` `=`. For `xr_logic`: `xr_logic=`; for `a.b.c`:
  `a={b={c=`.
- closing = one `}` per component after the first: empty for `xr_logic`, `}}` for `a.b.c`.

So for N = `xr_logic` the default prefix is
`local function script_name() return "xr_logic" end local this = {} xr_logic= this  setmetatable(this, {__index = _G}) setfenv(1, this) `
plus 09 09. Its effect when the chunk runs: the global `N` is set to a new table `this` (for a dotted name,
the first component is set to a new table nesting it, replacing any previous value); the rest of the
chunk runs with `this` as its environment, so the script's global assignments (`function f() … end`,
`x = 1`) create fields of its namespace; by default, reads of names the namespace lacks fall back to the
globals (and so can trigger automatic loading, 2.2.3); with `-_g` there is no fallback and the namespace
has a field `_G` holding the globals. `script_name()` and `this` are locals of the chunk, visible in the
whole file.

The prefix contains **no line break**, so the file's first line is the chunk's line 1 and line numbers are
the file's own (apart from block comments, 1.4). The chunk name is `@` followed by the file's full path
as resolved by the path aliases, so messages show `<path>:<line>:` (Lua shortens long names).

Consequences:

- A name that is not a sequence of Lua names separated by `.` (for example one containing a space, such
  as `copy of a1`) produces an invalid prefix, so that file never compiles, whatever it contains.
- `scripts\.script` (empty name) is never loaded: no trigger reaches the loader with an empty name.

### 2.4 Failures (static)

The chunk is compiled and then run immediately. If compiling fails, or running raises an error, the game
logs nothing, discards the error and continues. After a compile failure the namespace does not exist, so
later accesses to the name try to load the file again; after a run failure of a prefixed chunk the
namespace exists (the prefix creates it first), holding what the run defined, and counts as loaded. The same holds when an existing file
cannot be opened, and when a class registrator cannot be found: no message is shown or logged in
either case.

## 3. Which files the game can load (measured, conservative)

A file in `scripts\` can be loaded only through its name (2.2). The set of files the game can be led to
load from its own data was estimated as follows (an over-approximation: a name being mentioned does not
mean the code mentioning it runs):

- Roots: `_g` (startup), and every word made of letters, digits and `_` (not starting with a digit),
  compared case-insensitively with the script names, found anywhere in: the 893 `config\*.ltx` and
  `config\*.xml` files; the 195 `*.ltx`, `*.ini`, `*.spawn` and `*.game` files outside `config\` and
  `scripts\` (including `spawns\all.spawn`, whose object settings are stored as plain text); and the bytes
  of `xrGame.dll` and `XR_3DA.exe`.
- Closure: every such word in a reached script's text, after removing its `--` comments and its C-style
  comments (section 1; string contents are kept), reaches that script.

Result: **250** of the 440 files in `scripts\` are reachable (132 directly from roots). The other 190
include all files whose names are not Lua names (`.script`, `copy of _test.script`, `copy of a1.script`),
and many test, debug and unused files.

## 4. Verification

Compiling means `luaL_loadbuffer` of the public release on the given bytes (nothing is run).

### 4.1 Counts (measured)

| Text compiled | Files | Compile | Fail |
|---|---|---|---|
| raw file bytes, all `*.script` paths | 442 | 409 | 33 |
| raw bytes after the transformation of 1.5 | 442 | 436 | 6 |
| chunk as the game builds it (2.3 default prefix, then 1.5), the 437 files in `scripts\` with loadable names (all but `.script`, `copy of _test.script`, `copy of a1.script`) | 437 | 433 | 4 |
| the same, the 250 reachable files (section 3) | 250 | **250** | 0 |
| the same with the `-_g` prefix, the 250 reachable files | 250 | 250 | 0 |

So every script the game can load from its data compiles under the public parser with the rules of this
specification. The 33 raw failures:

- 27 stop at a `/` that starts a C-style comment: `.script`, `_g`, `arbug`, `bind_stalker`, `ce_switcher`,
  `game_last_dialog`, `game_last_sv`, `h`, `level_psy_antenna`, `monster_test`, `ph_test`, `postprocess`,
  `se_new_attachable_item`, `se_switcher`, `sr_psy_antenna`, `state_mgr_animation`, `test_invisible`,
  `test_zombie`, `ui_main_base_tab`, `ui_mp_main`, `ui_save_dialog`, `x3`, `xr_actions_id`,
  `xr_evaluators_id`, `xr_rest`, `xr_sounds`, `zone_no_weapon` (all `scripts\<name>.script`). All 27
  compile after the transformation. 16 of them are reachable (section 3): `_g`, `bind_stalker`,
  `ce_switcher`, `game_last_dialog`, `game_last_sv`, `h`, `postprocess`, `se_switcher`, `sr_psy_antenna`,
  `state_mgr_animation`, `ui_mp_main`, `ui_save_dialog`, `x3`, `xr_actions_id`, `xr_evaluators_id`,
  `xr_rest`. (`xr_sounds` is mentioned only by `xr_sos.script`, which nothing mentions; `h` and `x3` are
  reachable only through common words.)
- 6 fail for other reasons and also fail after the transformation; none can be loaded by the game:
  - `config\misc\gulag_radar_u.script`: LTX-style text (`;` comments) outside `$game_scripts$`, never
    loaded (2.1). The global `gulag_radar_u` loads `scripts\gulag_radar_u.script` instead, which compiles.
  - `scripts\copy of _test.script`: not Lua (starts with Cyrillic text); its name is not loadable.
  - `scripts\lua_help.script`: a listing of exported classes in C++-like notation, not Lua; not reachable.
  - `scripts\gulag_selo.script`: line 13 has a string whose closing quote is the Windows-1251 byte 94
    (a right double quotation mark) instead of `"`, so the string is unfinished; not reachable (absent from
    the gulag list in `xr_gulag.script`).
  - `scripts\mob_alife_control.script`: line 44, a statement that is an expression; not reachable.
  - `scripts\test_ini.script`: line 13 starts with `.def(`, C++ text pasted in; not reachable.
- In addition, `scripts\copy of a1.script` (identical to `a1.script`) compiles raw but not as the game
  would build it, because its name is not loadable (2.3).

### 4.2 Comment usage in the data (measured, 440 files in `scripts\`)

| Fact | All 440 | 250 reachable |
|---|---|---|
| files with `//` comments | 23 (228 comments) | 12 (55) |
| files with `/*` comments | 5 (6 comments) | the same 5 |
| `/*` comments spanning line breaks | all 6, 124 line breaks in total | same |
| unterminated `/*` comments | 1 (`bind_stalker.script`, from line 432 to the end) | same |
| `**` pairs inside a `/*` comment (1.4) | 1 (`bind_stalker.script`, the final `/**/` on line 449) | same |
| quoted or long strings containing `//` or `/*` | 0 | 0 |
| `--` comments containing `//` or `/*` | 29 in 12 files | 4 in 4 files |
| byte-order marks, NUL bytes, bare CR line breaks | 0 | 0 |
| files with CR LF line breaks (the other 3 have no line break) | 437 | 250 |
| files with bytes 80–FF | 191 | 147 |

Bytes 80–FF occur outside strings and comments only in `copy of _test.script` and `gulag_selo.script`
(both unloadable), so whether the runtime would treat such bytes as letters does not arise.

The 5 files with block comments: `bind_stalker` (432–449, unterminated), `ce_switcher` (101–137),
`game_last_dialog` (14–29 and 157–173), `h` (20–33), `se_switcher` (54–78) (physical line numbers of `/*`
and of the line holding `*/` or `/**/`).

Line numbering (static, derived from 1.4): in `game_last_dialog.script`, code on physical line 30 is line
15 for the runtime, and code after line 173 is reported 31 lines lower than its physical line; in
`h.script`, lines after 33 are reported 13 lower.

### 4.3 Hashes (measured)

MD5 of the file, of the result of the transformation of 1.5, and of the whole chunk as the game builds it
(default prefix, 2.3):

| File (name) | File bytes, MD5 | Transformed bytes, MD5 | Chunk bytes, MD5 |
|---|---|---|---|
| `_g.script` (`_G`) | 39,542, `c7562102faf72dc95222be7f913533df` | 39,119, `221c84fe1cb40e731d670ef7a667d65b` | same as transformed (no prefix) |
| `bind_stalker.script` | 14,778, `37b31764e67a824a3f4b9ac2dbb0cefe` | 14,241, `97e424a7c82bf7bcc5dc2e5906b8f036` | 14,386, `335f362cd0ffd97eacb442851b29ed42` |
| `game_last_dialog.script` | 7,187, `0231f02d02f17814d4e8bb50974b63e2` | 6,103, `e50c332f8074f3d05c8e2f82ddccbfd9` | 6,256, `f99bb3df2fab50b027342f418d95d693` |
| `h.script` | 916, `461455cc0530097d41970083681446a8` | 609, `ca1d11225dfc73b2f474884e02739349` | 732, `3bfc1e226db2378d4cd8a0009d747f59` |
| `xr_rest.script` | 19,198, `f3f590676f82151a8f1d14e4944b3165` | 18,978, `688ead187275cf845762fd3547dd9b9b` | 19,113, `f74ae5557038b3527eb81f23168906dc` |
| `a1.script` (no comments) | 249, `d3d9442930bdd0c4ee07ec6e0f547e47` | unchanged | 374, `702bcf7a65bb75dde2913e5332a078ef` |

Line-feed counts before and after the transformation: `bind_stalker` 451 → 431, `game_last_dialog`
228 → 197, `h` 39 → 26, `_g` 1,282 → 1,282.

### 4.4 Shader scripts (measured; their loading is not covered)

The 69 shader-state scripts (`shaders\r1\*.s`, `shaders\r2\*.s`): 56 compile raw and all 69 after the
transformation of 1.5 (no prefix applied). How the shader state names and prefixes them is not covered.

## 5. Not established

- Whether the embedded parser and code generator differ from the public release beyond the lexical
  analyzer (not compared; the game's loadable scripts do not need any difference).
- Whether the runtime's tests for letters and digits treat bytes 80–FF as letters (depends on the C
  library's locale at run time; does not arise in the loadable scripts).
- Whether each engine-called `namespace.function` name other than `class_registrators` loads its namespace
  in the same way (believed, 2.2.4); what the engine does with the game-type `script` lists and the
  `levels` key of `script.ltx` (empty in this edition).
- Which of the reachable scripts are actually loaded in a given play session (section 3 is a mention-based
  over-approximation; loading happens on first access at run time).
- Limits on name length (the prefix is built in a fixed-size buffer whose overflow behavior was not
  examined; no shipped name comes close).

## Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrLUA.dll` and `xrGame.dll`
  as identified above; the archive file tree of `gamedata.db0` … `gamedata.dbd`, extracted with the
  project's archive tool.
- Analysis: static analysis with Ghidra of the embedded runtime's lexical analyzer and buffer loading in
  `xrLUA.dll`, compared with the public LuaJIT 1.0.3 source; static analysis of the script loading, name
  resolution, namespace and prefix construction, automatic loading, startup order and `script.ltx` reading
  in `xrGame.dll`; strings and tables read from file bytes by a throwaway parser. Verification by compiling
  the game data with the public LuaJIT 1.0.3 release (built from source, compile only) after a throwaway
  implementation of sections 1.5 and 2.3, and a throwaway mention scanner for section 3. No binary was
  executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.
