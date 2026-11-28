# Script API F: configuration and files

What the configuration and file members of the game scripts' interface do in S.T.A.L.K.E.R.: Shadow of
Chernobyl (Russian edition): the configuration object class `ini_file` with its constructor and its 13 readers,
the global functions `system_ini`, `game_ini` and `create_ini_file`, the token classes `token`, `token_list` and
`rtoken_list`, the file system reached through `getFS()` (the class `FS` with its 24 member overloads and 10
constants), the file-list classes `FS_file_list` and `FS_file_list_ex`, the list entry class `FS_item` (with the
unlisted `ModifDigitOnly`), the file-entry class `fs_file`, and the global functions `command_line` and
`user_name`. This is spec F of the approved scoping specification `docs/behavior/script-api-scope.md` (section 6,
order 6), which assigns these names to it; its scope rule applies: this spec owns every name of these classes
and functions as the binary registers them, listed or not. Section 9 works through the reads of one logic file,
that of the trader's door (`docs/behavior/physics-shells.md` 10), as an example.

Builds on (not repeated here):

- `docs/formats/ltx-config.md` (cited as **LTX**): how a configuration file becomes sections and entries (its
  sections 1–5), the stored (normalized) value of an entry and "no value" (section 3), lookups, case and the
  sorted order of entries (section 6), value interpretations (section 7), loading a missing root file
  (section 8), path aliases and `fsgame.ltx` (section 10), fatal errors (section 11).
- `docs/formats/db-archive.md` (cited as **DB**): the combined tree of archive entries and loose files and its
  precedence (section 7); read failures (section 9). Its combined index of paths is called **the index** here.
- `docs/behavior/script-environment.md` (the runtime, LuaJIT 1.0.3; numbers are 64-bit doubles),
  `docs/behavior/script-loading.md` (the 250 **loadable** scripts of its section 3),
  `docs/behavior/script-api-scope.md` (the listing `scripts\lua_help.script`, the counting rules, the split).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md`: how a call's arguments are
  matched to a registered signature and what happens when none matches. Statements that rest on it say
  **depends on A**.
- Spec B, `docs/behavior/script-api-engine-services.md` (the fatal-error wording of its conventions; the
  run-time library's handler for a failed bounded string operation, its section 2.3).
- Spec C, `docs/behavior/script-api-class-ids-and-the-object-factory.md`: class ids and their numbering (its
  sections 1 and 2.4; its section 7 names `ini_file:r_clsid` as reaching scripts).
- Spec E, `docs/behavior/script-api-value-types-and-math.md`: the class `vector`, the f32 conventions, and the
  run-time library's float-to-integer conversion of its section 8 (called **the E8 conversion** here).
- `docs/behavior/console-log.md` section 3.8: the fatal-error report.

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the core
  library" `bin\xrCore.dll` (856,064 bytes, MD5 `eab71f90f01575eca3500e623873e57c`), "the engine"
  `bin\XR_3DA.exe` (1,636,424 bytes, MD5 `ba3ad0127bff6619ed991a7c82d8b946`) [CF-1].
- **The listing** is `scripts\lua_help.script` (scoping spec 2.1); it is a guide only, every member below is
  established from the game library's registration code [CF-3].
- A call is written as scripts write it: `ini:r_string(section, key)`, `getFS():exist(alias, name)`. A
  **text** is a Lua string; a **number** the runtime's double; a **boolean** `true` or `false`.
- **Fatal error**: the program ends through the fatal-error report of `console-log.md` 3.8. Where a message is
  given, it is the report's description (`<…>` stands for the argument named).
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (from
  the game data or file bytes), **documented** (the published behaviour of a standard C run-time function),
  **believed** (inferred; the reason is given) or **unestablished**. No binary was run.
- **Checks.** Every claim carries a tag [CF-*n*]. The checks are one script in the dirty workspace,
  `checks/script-api-config/run_all.py` (`python3 -I run_all.py`): it reads the binaries, the evidence of
  read-only headless static analysis of them (produced by `ghidra.py` beside it into `bin/`, reused when
  present), the listing, the shipped scripts, `config\scripts\esc_trader_door.ltx` and the scoping spec's
  scan results. All checks pass (`ALL PASS 24`).

## 1. What this spec owns

### 1.1 Items (measured and static, [CF-2], [CF-3])

The listing has 84 items for this spec: 9 classes, 69 declared members and 6 global functions. The game
library registers exactly these, plus one member the listing lacks (`FS_item:ModifDigitOnly`), all in the
global namespace; no other name is registered for these classes, and no class of this spec has a property
the listing lacks (the scoping spec's gap 2). The scoping spec assigns no candidate strings to this spec
[CF-2].

| Name | Kind | Members | Section |
|---|---|---|---|
| `system_ini()`, `game_ini()` | global functions | → a configuration object | 2.1 |
| `ini_file(name)` | class with one constructor | `section_exist`, `line_exist`, `line_count`, `r_string`, `r_string_wq`, `r_s32`, `r_u32`, `r_float`, `r_bool`, `r_vector`, `r_clsid`, `r_token`, `r_line` | 2, 3 |
| `create_ini_file(text)` | global function | → a configuration object | 2.3 |
| `token()` | class with one constructor | properties `name`, `id` | 4.1 |
| `token_list()` | class with one constructor | `add`, `remove`, `clear`, `id`, `name` | 4.2 |
| `rtoken_list()` | class with one constructor | `add`, `remove`, `clear`, `count`, `get` | 4.3 |
| `getFS()` | global function | → the file system object | 5.1 |
| `FS` | class, no constructor | 10 constants; `path_exist`, `update_path`, `get_path`, `append_path`, `exist` (2 forms), `file_length`, `get_file_age`, `get_file_age_str`, `file_delete` (2), `file_rename`, `file_copy`, `dir_delete` (2), `r_open` (2), `r_close`, `w_open` (2), `w_close`, `file_list_open` (2), `file_list_open_ex` | 5 |
| `fs_file` | class, no constructor | read-only fields `name`, `vfs`, `ptr`, `size_real`, `size_compressed`, `modif` | 5.3 |
| `FS_file_list` | class, no constructor | `Size`, `GetAt`, `Free` | 5.6 |
| `FS_file_list_ex` | class, no constructor | `Size`, `GetAt`, `Sort` | 5.7 |
| `FS_item` | class, no constructor | `NameFull`, `NameShort`, `Size`, `Modif`, `ModifDigitOnly` (unlisted) | 5.8 |
| `command_line()`, `user_name()` | global functions | → text | 6 |

How a call whose arguments match no registered signature is rejected, and whether a number is accepted where a
text is expected, **depends on A**. The listing writes some members with an explicit first parameter
(`r_line(ini_file*, …)`, `file_list_open(FS*, …)`); scripts call them as methods.

### 1.2 Results

A member that the table of section 3 or 5 gives as returning a boolean gives a Lua boolean; a text result that
is missing gives nil (believed: the binding's text conversion, as for the console getters of spec B 2.5); a
32-bit unsigned result is pushed as a number in [0, 2³²): a value whose top bit is set has 2³² added [CF-9];
signed integers are numbers in [−2³¹, 2³¹), an f32 is the f32 value as a number (believed: the binding's
plain number conversion).

## 2. Configuration objects

A **configuration object** is a configuration in the sense of LTX: sections of entries, loaded once. Every
reader of section 3 works on any configuration object, however it was made.

### 2.1 `system_ini()` and `game_ini()` (static, [CF-4])

`system_ini()` returns the engine's configuration of `config\system.ltx` and `game_ini()` that of
`config\game.ltx` (LTX section 8), the same object on every call; the engine loaded them at start-up. Scripts
do not own them: nothing a script does frees them. No loadable script calls `game_ini` (section 8).

### 2.2 `ini_file(name)` (static, [CF-5])

Makes a new configuration object from a file:

1. The file's path is the directory of the alias `$game_config$` (LTX section 10) followed by `name`, the
   whole path lowercased. `name` is used as given: no separator is inserted, and `..` or a leading `\` are not
   treated specially. The path is built in a 520-byte buffer; a path of 520 bytes or more fails the run-time
   library's bounded copy, which ends the program as in spec B 2.3 (believed: the same handler).
2. The file is read and loaded as a root file (LTX sections 1–5); its includes resolve against the file's own
   directory (LTX section 4). A file that is not in the index gives a configuration with no sections, with no
   error and no message (LTX section 8).
3. The object is read-only: it is never written back.

The object belongs to the script side; it is destroyed when the runtime collects it (static: the class's
destroy function is registered; when collection happens is the runtime's). With the configuration so made, a
missing file is noticed only by its readers: `section_exist` answers false for every name (3.1).

Example: `ini_file("scripts\\esc_trader_door.ltx")` reads `<game data>\config\scripts\esc_trader_door.ltx`,
which the archives provide as `config\scripts\esc_trader_door.ltx` (section 9).

### 2.3 `create_ini_file(text)` (static, [CF-6])

Makes a new configuration object whose file contents are `text`: the bytes of `text` up to its first zero
byte (or all of it) are loaded as a root file is (LTX sections 1–5), and its include lines resolve against the
directory of `$game_config$`. The object is read-only and belongs to the script side (the binding's adopt
rule: the runtime destroys it when it collects the result). The one loadable caller is
`gulag_tasks.script`, for configurations it builds as text (section 8).

### 2.4 Reading errors

Every reader that needs a section or a key that the configuration does not have ends the program (LTX section
11); the messages are [CF-7], [CF-8]:

| What is missing | Fatal-error description |
|---|---|
| a section, for `line_count`, `r_line` and every `r_` reader | `Can't open section '<name>'` (the name as the script gave it) |
| a key in an existing section, for every `r_` reader except `r_line` | `Can't find variable <key> in [<section>]` |

`section_exist` and `line_exist` never fail (3.1).

## 3. `ini_file` readers

In all readers, `section` is looked up by its lowercased name (LTX section 6) and `key` exactly, byte for byte
(case-sensitive), except where 3.1 says otherwise.

### 3.1 Lookups (static, [CF-7])

| Call | Result |
|---|---|
| `section_exist(section)` | boolean: whether a section of exactly that name exists. The name is **not** lowercased (the exception of LTX section 6): since stored names are lowercase, a name with an upper-case letter is never found |
| `line_exist(section, key)` | boolean: false when `section_exist(section)` is false; otherwise whether the section has an entry with key `key` |
| `line_count(section)` | number: the number of entries of the section (distinct keys, inherited ones included, LTX sections 5 and 6); fatal for a missing section |

### 3.2 Texts (static, [CF-8])

- `r_string(section, key)` returns the entry's stored value (LTX section 3) as a text, quotes included; for an
  entry with no value it returns nil.
- `r_string_wq(section, key)` returns the stored value with one leading `"` removed if the value starts with one
  and one trailing `"` removed if it ends with one (LTX section 7 "unquoted string"); for no value it returns
  nil. The value is copied into a 512-byte buffer first: a value of 512 bytes or more fails the bounded copy and
  ends the program as in spec B 2.3 (believed: the same handler). No shipped script calls it.

### 3.3 Numbers (static and documented, [CF-9])

| Call | Result |
|---|---|
| `r_s32(section, key)` | the value read by the C library's `atoi` (the longest prefix forming an optionally signed decimal integer, after leading white space; no digits give 0; documented), as a signed 32-bit number |
| `r_u32(section, key)` | the same 32 bits as `r_s32` (one reader serves both), given as unsigned: `-1` reads as 4294967295 |
| `r_float(section, key)` | the value read by the C library's `atof` (LTX section 7 "float"), rounded to f32 |

For an entry with no value these readers pass no text to `atoi` or `atof`; the run-time library then calls its
handler for an invalid argument, which ends the program as in spec B 2.3 (documented for the run-time library;
that the handler is the one of B is believed).

### 3.4 `r_bool(section, key)` (static, [CF-10])

The first 7 bytes of the stored value are copied and lowercased; the result is `true` exactly when they read
`on`, `yes`, `true` or `1` followed by the end of the value, otherwise `false` (LTX section 7 "boolean"). A
value of 7 bytes or more is therefore always false. For an entry with no value the program faults (believed:
the copy reads through no text).

### 3.5 `r_vector(section, key)` (static, [CF-11])

Returns a new `vector` (spec E section 3) whose three f32 components are read from the value as three
comma-separated floats by the C library's `sscanf` with the format `%f,%f,%f`: reading stops at the first
component that does not parse, and that component and all after it are 0 (LTX section 7 "vector"). For no value,
believed fatal as in 3.3. No loadable script calls it.

### 3.6 `r_clsid` and `r_token` (static, [CF-12])

- `r_clsid(section, key)` converts the stored value to a class id (spec C section 1) and returns its
  **number** (spec C section 2.4: for an id that is not registered, the number of the next larger registered
  id). No shipped script calls it.
- `r_token(section, key, list)` compares the stored value, ignoring the case of ASCII letters, with the names of
  the `token_list` `list` (section 4.2) in order, and returns the id of the first that matches, or 0 when none
  does (an empty list gives 0 without reading the value). For no value and a non-empty list, believed fatal as in
  3.3. No shipped script calls it.

### 3.7 `r_line(section, index, name, value)` (static, [CF-13])

Reads the entry at position `index` of the section, positions counted from 0 in the section's sorted order (LTX
section 6). `index` is converted by the E8 conversion. It returns three values:

- for 0 ≤ `index` < `line_count(section)`: `true`, the entry's key, and its stored value (`""` for no value);
- otherwise: `false`, `""`, `""`.

The third and fourth arguments must be texts (believed: they only make the call match the registered signature;
their contents are not used). A missing section is fatal (2.4). The scripts call it as
`result, id, value = ini:r_line(section, i, "", "")` in a loop `for i=0,n-1` with `n = ini:line_count(section)`
(measured, for example `xr_box.script`).

## 4. Token classes

### 4.1 `token` (static, [CF-14])

`token()` makes an object with the properties `name` (text) and `id` (number); both can be read and assigned. A
new token has `name` nil and `id` 0. `id` holds a signed 32-bit integer (believed: assigned through the E8
conversion). Assigning `name` stores a reference to the Lua text itself, not a copy: after the runtime collects
that text, reading `name` is unestablished (believed: it reads freed memory). No member in the listing takes or
returns a `token`; no shipped script makes one.

### 4.2 `token_list` (static, [CF-14])

`token_list()` makes an empty list of (name, id) entries, ended by an internal empty entry that is not one of
them. This is the list `ini_file:r_token` searches.

| Call | Effect and result |
|---|---|
| `add(name, id)` | appends the entry (a copy of `name`, and `id` by the E8 conversion); duplicates are allowed |
| `remove(name)` | removes the first entry whose name equals `name` exactly (case-sensitive), moving later entries up; when no entry has that name, the move runs past the list's end (believed: memory corruption or a crash) |
| `clear()` | removes every entry |
| `id(name)` | the id of the first entry whose name equals `name` exactly; when none does, the value is read past the list's end and is **unestablished** |
| `name(id)` | the name of the first entry whose id equals `id`; when none does, the result is read past the list's end and is **unestablished** |

`r_token` compares ignoring case, `remove` and `id` exactly. No shipped script uses `token_list`.

### 4.3 `rtoken_list` (static, [CF-15])

`rtoken_list()` makes an empty list of texts.

| Call | Effect and result |
|---|---|
| `add(text)` | appends `text` |
| `remove(i)` | removes the entry at position `i` (from 0) when `i` is below the count, moving later entries up; otherwise nothing |
| `clear()` | removes every entry |
| `count()` | the number of entries |
| `get(i)` | the text at position `i` when `i` is below the count, otherwise nil |

`i` is converted to an unsigned 32-bit integer (believed: as the E8 conversion, then read as unsigned), so a
negative index is never below the count. No shipped script uses `rtoken_list`.

## 5. The file system

### 5.1 `getFS()` and the constants (static, [CF-16], [CF-2])

`getFS()` returns the engine's single file system object (the owner of the index and the aliases of LTX section
10), the same object on every call. Scripts cannot make another (`FS` has no constructor).

`FS` has these constants:

| Constant | Value | Use |
|---|---|---|
| `FS_ListFiles` | 1 | file-list flag: list files |
| `FS_ListFolders` | 2 | file-list flag: list folders |
| `FS_ClampExt` | 4 | file-list flag: drop the extension of file names |
| `FS_RootOnly` | 8 | file-list flag: only the folder itself, not its subfolders |
| `FS_sort_by_name_up`, `FS_sort_by_name_down` | 0, 1 | `Sort` modes (5.7) |
| `FS_sort_by_size_up`, `FS_sort_by_size_down` | 2, 3 | |
| `FS_sort_by_modif_up`, `FS_sort_by_modif_down` | 4, 5 | |

Flags are combined with `bit_or` (spec E section 8) by the scripts.

**Paths.** Members that take `(alias, name)` build a **full path**: the alias's directory followed by `name`,
lowercased (LTX section 10), in a 520-byte buffer (a longer path ends the program as in 2.2, believed). An alias
that is not defined is a failed check whose description is the alias: fatal (static). Members that take one
`path` use it as a full path exactly as given, not lowercased: since the index holds lowercased full paths, a
path with an upper-case letter is not found there (believed; [CF-18]).

### 5.2 Paths and aliases (static, [CF-17])

| Call | Result |
|---|---|
| `path_exist(alias)` | boolean: whether `alias` is a defined alias |
| `update_path(alias, name)` | text: the full path of `(alias, name)` |
| `get_path(alias)` | the alias's path object; `append_path(alias, root, add, recurse)` defines a new alias whose directory is `root` followed by `add`, indexes the loose files under it (subfolders included when `recurse` is not 0, believed), and returns its path object |

The path object is of a class the game library does not register with the runtime; what a script receives is
**unestablished** (depends on the binding: see section 10). No shipped script calls these four members.

### 5.3 File entries: `exist` and `fs_file` (static, [CF-18])

- `exist(path)` and `exist(alias, name)` return the index's entry of that full path as an `fs_file`, or nil when
  the index has no such path. The entry is the index's own: it is not a copy (believed: after the file is deleted
  from the index, reading it is unestablished).
- `fs_file` fields, all read-only:

| Field | Meaning |
|---|---|
| `name` | text: the entry's full path, lowercased |
| `vfs` | number: for a loose file 4294967295; for an archive member which archive holds it (believed; its numbering is unestablished) |
| `ptr` | number: for a loose file 0; for an archive member its data offset (DB section 3; believed) |
| `size_real` | number: the file's size in bytes (for an archive member its original size, DB section 3; believed) |
| `size_compressed` | number: for a loose file its size; for an archive member its stored size (believed) |
| `modif` | number: for a loose file its last-write time in seconds since 1970-01-01 UTC, as the directory scan reports it (the low 32 bits); for an archive member **unestablished** |

Numbers are unsigned 32-bit (1.2).

- `file_length(path)`: the entry's `size_real`, or −1 when there is no entry.
- `get_file_age(path)`: the entry's `modif`, or 4294967295 when there is no entry.
- `get_file_age_str(path)`: `get_file_age(path)` converted to local time and formatted by the C library's
  `asctime`: 24 characters and a line feed, for example `Wed Jan 02 02:03:55 1980` (documented). For a missing
  entry, the time 4294967295 is formatted (believed: a date in 2106).

The loadable script `ui_save_dialog.script` calls `exist("$game_saves$", name .. ".sav")` to test whether a save
of that name exists (measured, [CF-23]).

### 5.4 Changing files (static, [CF-19])

| Call | Effect |
|---|---|
| `file_delete(path)`, `file_delete(alias, name)` | when the index has the full path: deletes that file from the disk and removes the entry from the index; otherwise nothing. An empty `alias` in the two-argument form means that `name` is used as a full path. Whether the disk deletion succeeded is not checked (an archive member's entry is removed from the index although no loose file is deleted, believed) |
| `file_rename(from, to, overwrite)` | only when the index has `from` (full paths, as given): when it also has `to` and `overwrite` is false, nothing; when it has `to` and `overwrite` is true, deletes `to` as `file_delete` does; then the entry of `from` is moved to `to` (lowercased) in the index, the folders of `to` are made sure to exist on the disk (believed) and the file is renamed on the disk |
| `file_copy(from, to)` | when the index has `from`: reads it whole and writes its bytes to the new file `to` (as `w_open(to)`, 5.5); otherwise nothing |
| `dir_delete(path, remove_files)`, `dir_delete(alias, name, remove_files)` | for the folder's full path: when `remove_files` is 0 and the folder has a file below it in the index, nothing is deleted and nothing changes (believed); otherwise every file below it is deleted from the disk and the index, and the folders below it are removed from the disk |

The loadable script `ui_load_dialog.script` deletes a save with `file_delete("$game_saves$", save_file)` and the
matching picture with the same call (measured, [CF-23]).

### 5.5 Readers and writers (static, [CF-16])

`r_open(alias, name)` and `r_open(path)` open a file of the index for reading and return a data-stream reader
(the class `reader` of spec I, believed from the listing's `r_close(reader*&)`), or nil when the path is not in
the index; read failures are those of DB section 9 (believed). `r_close(reader)` closes it.
`w_open(alias, name)` opens a new file for writing; the writer is of a class the game library does not register,
so what a script receives is **unestablished**. The one-argument `w_open(name)` is registered with a text
parameter but bound to the function that closes a writer: calling it treats the text as a writer and is believed
to crash. `w_close(writer)` takes a writer reference no script can make. No shipped script calls these members.

### 5.6 `file_list_open` and `FS_file_list` (static, [CF-20])

`file_list_open(alias, flags)` and `file_list_open(alias, name, flags)` list the index below a folder:

1. The folder: for the two-argument form, `alias` is looked up as an alias and its directory is used; when it is
   not an alias it is used as a full path as given. For the three-argument form, the full path of
   `(alias, name)`; an empty or missing `alias` there is a failed check (fatal).
2. When the index has no entry for the folder itself, the result is a list object holding no list: `Size` and
   `GetAt` on it fault (believed: they read through no list), and `Free` does nothing.
3. Otherwise every index entry whose full path starts with the folder's path, in the index's order (byte order of
   the full path, believed), gives one name: the rest of its path after the folder's path. A folder entry (its
   path ends in `\`) is listed, with its trailing `\`, only with `FS_ListFolders`, and with `FS_RootOnly` only when
   it is directly in the folder. A file is listed only with `FS_ListFiles`, and with `FS_RootOnly` only when its
   rest contains no `\`; with `FS_ClampExt` the name is cut at its last `.`.

`FS_file_list`:

- `Size()`: the number of names.
- `GetAt(i)`: the name at position `i` (from 0), `i` by the E8 conversion. For `i` outside [0, `Size()`) the
  run-time library raises its out-of-range exception with the text `invalid vector<T> subscript`; what the
  script then sees is **unestablished** (depends on the binding: section 10).
- `Free()`: releases the names; after it, every member of the object is unestablished (believed: they read freed
  memory). A list that is never freed is never released (believed: the object's own destruction frees only the
  object).

`ui_save_dialog.script` lists `$game_saves$` with `FS_ListFiles` and frees the list (measured, [CF-23]).

### 5.7 `file_list_open_ex` and `FS_file_list_ex` (static, [CF-21])

`file_list_open_ex(alias, flags, mask)` lists a folder with masks and returns an `FS_file_list_ex`:

1. The alias's folder is marked to be re-read from the disk, and the marked folders are re-read before listing,
   so loose files written since the game started (new saves) are in the index (believed: the re-read re-indexes
   the folder's loose files).
2. `mask` is split at commas; each piece is trimmed of bytes 00–20 at both ends and empty pieces are dropped. With
   no pieces every file is accepted; otherwise a file is accepted when its name (the rest of its path) matches a
   piece: `*` matches any run of bytes (none included), `?` any one byte, any other byte only itself
   (case-sensitive), and the whole name must match.
3. Entries are chosen as in 5.6 step 3 (folder from `alias` as in its two-argument form, except that `alias`
   must be a defined alias: step 1 looks it up, and an undefined one is fatal as in 5.1; masks apply to files
   only; `FS_ClampExt` drops the extension of the name), each giving an **item** (5.8) with its name, size
   (`size_real`) and modification time (`modif`). Items are in byte order of their names. When the index has no
   entry for the folder, the list is empty.

`FS_file_list_ex`:

- `Size()`: the number of items.
- `GetAt(i)`: a copy of the item at position `i` (from 0), as a new `FS_item`; `i` by the E8 conversion. The index
  is not checked: outside [0, `Size()`) the copy reads outside the list (believed: garbage or a crash).
- `Sort(mode)`: sorts the items in place by mode 0–5 (5.1): names by unsigned byte order, ascending (0) or
  descending (1); sizes ascending (2) or descending (3); modification times ascending (4) or descending (5), as
  unsigned numbers. Any other mode leaves the list unchanged. The sort is not stable: the order of items with
  equal keys is **unestablished**.

`ui_load_dialog.script` lists the saves with `file_list_open_ex("$game_saves$", bit_or(FS.FS_ListFiles,
FS.FS_RootOnly), "*.sav")` and sorts them with `FS_sort_by_modif_down`, newest first (measured, [CF-23]).

### 5.8 `FS_item` (static, [CF-21])

| Call | Result |
|---|---|
| `NameFull()` | text: the item's name (the rest of the path after the folder, 5.7) |
| `NameShort()` | the same text as `NameFull()` (both are bound to one function) |
| `Size()` | number: the size in bytes |
| `Modif()` | text: the modification time in local time formatted by `asctime` (as in 5.3, with its line feed) |
| `ModifDigitOnly()` | text: the modification time in local time as `DD:MM:YYYY hh:mm`: day and month two digits with a leading zero, colons between day, month and year, the year in four digits (right-aligned in four places), a space, hours and minutes two digits each with a colon between; for example `05:03:2008 14:07` |

`Modif()` and `ModifDigitOnly()` write their text into the item itself and return it; each call replaces the text
the previous one returned for that item (believed: the runtime copies a returned text, so earlier results in Lua
are unaffected). Local time is the system's time zone at the moment of the call (documented for the run-time
library's conversion). The loadable scripts `ui_load_dialog.script` and `ui_save_dialog.script` show
`"[" .. item:ModifDigitOnly() .. "]"` beside each save (measured, [CF-23]).

## 6. `command_line()` and `user_name()` (static, [CF-22])

- `command_line()`: the command line the process was started with, as the system gives it (program path
  included), lowercased (ASCII letters), as copied at start-up into a 512-byte buffer; a longer command line ends
  the program at start-up (believed: the bounded copy's handler, spec B 2.3).
- `user_name()`: the Windows user name of the account running the game, as read at start-up into a 64-byte
  buffer; when it does not fit, the text is **unestablished** (believed: empty).

No shipped script calls either.

## 7. Errors and limits at a glance

| Situation | Outcome |
|---|---|
| reader on a missing section or key | fatal (2.4) |
| `r_s32`, `r_u32`, `r_float`, `r_vector`, `r_token` on no value | fatal (believed, 3.3) |
| `r_bool` on no value | fault (believed, 3.4) |
| `r_string_wq` on a value of 512 bytes or more; a path of 520 bytes or more | fatal (believed, 2.2, 3.2) |
| undefined alias | fatal (5.1) |
| `token_list:remove` of an absent name | believed crash (4.2) |
| `FS_file_list` of an unindexed folder, then `Size`/`GetAt` | believed fault (5.6) |
| `FS_file_list_ex:GetAt` out of range | believed garbage or crash (5.7) |
| `w_open(name)` with one argument | believed crash (5.5) |

## 8. Use by the shipped scripts (measured, [CF-23])

With the scoping spec's scan and counting rules (its section 1: names are attributed to every class that declares
them), the loadable scripts use 37 distinct names of this spec, in 55 loadable scripts counting only names no
other spec has, and in 141 counting shared names (`id`, `name`, `count`, `add`, `get`, `clear`, `remove`, the
`r_` readers that the data-stream classes of spec I also declare); these match the scoping spec's table.
Loadable scripts per name (shared names are upper bounds):

| Name | Loadable scripts |
|---|---|
| `ini_file(...)` | 18 |
| `system_ini` | 11 |
| `create_ini_file` | 1 (`gulag_tasks.script`) |
| `section_exist` | 25 |
| `line_exist` | 18 |
| `r_string` | 17 |
| `r_bool` | 13 (shared) |
| `r_u32` | 12 (shared) |
| `r_float` | 8 (shared) |
| `r_s32` | 2 (shared) |
| `line_count`, `r_line` | 8 each |
| `getFS`, `file_list_open_ex`, `GetAt`, `Size`, `Sort`, `NameFull` | 3 each (`ui_load_dialog.script`, `ui_save_dialog.script`, `ui_spawn_dialog.script`) |
| `file_list_open`, `exist`, `Free` | 1 (`ui_save_dialog.script`) |
| `file_delete` | 1 (`ui_load_dialog.script`) |
| `Modif` | 1 (`ui_spawn_dialog.script`) |
| `ModifDigitOnly` | 2 (`ui_load_dialog.script`, `ui_save_dialog.script`) |

No loadable script uses `game_ini`, `command_line`, `user_name`, `token`, `token_list`, `rtoken_list`,
`r_string_wq`, `r_vector`, `r_clsid`, `r_token`, `NameShort`, the path members of 5.2, the members of 5.5,
`file_rename`, `file_copy`, `dir_delete`, `file_length`, `get_file_age` or `get_file_age_str`.

## 9. Worked example: the logic reads of the trader's door (measured and static, [CF-24])

The logic of a physics object is read by its binder and the logic scripts
(`bind_physic_object.script`, `xr_logic.script`, `utils.script`, `ph_door.script`) through these members only:
the constructor `ini_file(name)` (2.2), `section_exist` (3.1), `line_exist` (3.1), `r_string` (3.2) and `r_bool`
(3.4). An object's custom-data configuration itself comes from the game object member `spawn_ini()` (spec M,
not this spec); everything read from it goes through the same readers. For the door object of
`physics-shells.md` section 10:

1. The binder asks the custom data for the sections `animation`, `logic` and `drop_box` with `section_exist`.
2. The logic reads `[logic] cfg` from the custom data, guarded by `section_exist` and `line_exist`, with
   `r_string`: the custom data `[logic]` / `cfg = scripts\esc_trader_door.ltx` / `[collide]` / `ignore_static`
   (LTX: `cfg` has the value `scripts\esc_trader_door.ltx`, `ignore_static` no value) gives the text
   `scripts\esc_trader_door.ltx`.
3. The logic makes `ini_file("scripts\\esc_trader_door.ltx")` (2.2), the file `config\scripts\esc_trader_door.ltx`
   with the sections `logic`, `ph_door@locked`, `ph_door@closed` and `ph_door@open`, all lowercase, so
   `section_exist` finds them as the scripts spell them.
4. The further reads go through `utils.cfg_get_string` and `utils.cfg_get_bool`, which call
   `section_exist(section)` and `line_exist(section, key)`, and only when both are true `r_string` or `r_bool`;
   otherwise the scripts use their default; `xr_logic.script` tests `section_exist` before it activates a
   section. So, believed from the functions of this path, no read of the door reaches a fatal error of 2.4, and
   an absent key costs only the two lookups.
5. The values the scheme gets from `ph_door@locked` (stored values, LTX section 3): `[logic] active` →
   `r_string` `ph_door@locked`; `locked` → `r_bool` `true`; `snd_open_start` → `r_string` `trader_door_unlock`;
   `on_info` → `r_string` `{+esc_trader_can_leave}ph_door@closed%=play_snd(device\door_servomotor)%` (spaces
   removed outside quotes). Keys the scheme asks for and the section lacks (`closed`, `no_force`, `show_tips`,
   `tip_open`, `tip_close`, `snd_close_start`, `snd_close_stop`, `on_use`, `hit_on_bone`, and the switch and
   override keys of `xr_logic.script`) answer false to `line_exist`.

So these reads exercise the path rule and the missing-file rule of 2.2, the as-given (not lowercased) name of
`section_exist`, the exact key comparison of `line_exist`, `r_string`'s stored value and its nil for no value,
and `r_bool`'s four true words; the same members serve every object's logic.

## 10. Unestablished

- What a script receives for a result of a class the game library does not register (the path objects of 5.2,
  the writer of 5.5), and what it sees when a member raises a run-time library exception (`FS_file_list:GetAt` out
  of range, 5.6): both are the binding library's general behaviour, not stated by spec A.
- Whether a number is accepted where a text parameter is registered (depends on A).
- The values `token_list:id` and `token_list:name` give for an absent entry (4.2), and the outcome of
  `token_list:remove` of an absent name.
- `fs_file.vfs`, `ptr`, `size_compressed` and `modif` for archive members (5.3).
- The order of equal keys after `FS_file_list_ex:Sort` (5.7).
- Exactly which folders the re-read of `file_list_open_ex` re-scans and what it adds or removes (5.7 step 1).
- The `dir_delete` outcome in detail (5.4), and what `append_path` does with an alias name that is already
  defined.
- `user_name()` when the name does not fit 64 bytes (6).
- When the runtime collects configuration objects made by scripts (2.2, 2.3).

## 11. Verification facts

- Binaries identified by size and MD5 [CF-1]. Listing: 84 items (9 classes, 69 members, 6 globals) [CF-2].
- Registration: `ini_file` 13 methods and a constructor; `FS` 24 member overloads of 18 names and 10 constants;
  `fs_file` 6 fields; `FS_file_list` and `FS_file_list_ex` 3 methods each; `FS_item` 4 listed methods and
  `ModifDigitOnly`; no constructor for `FS`, `fs_file`, the two list classes and `FS_item` [CF-3], [CF-16].
- Script use: 37 names, 55 (141) loadable scripts [CF-23]. Door file sections and values [CF-24].

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the binaries identified above; the
  winning copies of `gamedata.db0` … `gamedata.dbd` as extracted with the project's archive tool
  (`scripts\lua_help.script`, the shipped scripts, `config\scripts\esc_trader_door.ltx`); the door's custom data
  from `spawns\all.spawn` as decoded with the record layouts of `sav.md` 7.4.
- Analysis: static analysis (Ghidra, headless, read-only) of the registration code of these classes and
  functions in `xrGame.dll`, of the functions it binds there, and of the configuration reader, the file index,
  the path aliases and start-up of `xrCore.dll`; reading of the shipped scripts; the scoping spec's scan of the
  scripts for the use counts. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used. The
  behaviour of standard C run-time functions (`atoi`, `atof`, `sscanf`, `asctime`, local-time conversion) is
  their published documentation.

## Gaps found in review

1. What `r_s32` and `r_u32` give for a value whose integer lies outside the signed 32-bit range is not stated (3.3).
2. The readers that look a section up by its lowercased name (`line_count`, `r_line`, every `r_` reader, and `line_exist` once `section_exist` is true) first copy the name into a 256-byte buffer: a name of 256 bytes or more fails the bounded copy (believed: the end of the program as in spec B 2.3) instead of giving the message of 2.4; not stated.
3. Members that take one `path` (`exist`, `file_length`, `get_file_age`, `file_delete`, `file_rename`, `dir_delete` and the others of 5.3–5.5) also copy it into a 520-byte buffer before the index lookup; 5.1 states the limit only for `(alias, name)`.
4. `dir_delete` of a folder the index has no entry for deletes nothing; and neither form gives the script a result. Not stated (5.4).
5. `dir_delete(alias, name, remove_files)` with an empty `alias` uses `name` as a full path, as the two-argument `file_delete` does; 5.4 says this only for `file_delete`.
