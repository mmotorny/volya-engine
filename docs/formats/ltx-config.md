# `.ltx` configuration file format

Functional specification of the text configuration files of S.T.A.L.K.E.R.: Shadow of Chernobyl
(`config\system.ltx`, `config\game.ltx`, the files they include, `textures\textures.ltx`,
`levels\<level>\level.ltx`, the font descriptors `textures\ui\*.ini`, and the configuration files
that game scripts open). It covers how such a file is turned into a set of named sections holding
key/value entries, how includes and section inheritance combine several files and sections, how
lookups behave, how values are conventionally interpreted, and how the game locates these files
through its path aliases (the path configuration file `fsgame.ltx`, whose syntax differs and is
described separately in section 10).

Terminology: a *configuration* is the result of loading one *root file* (with everything it
includes). It consists of *sections*; each section has a *name* and an ordered set of *entries*;
each entry has a *key* and a *value*. All text is handled as bytes. "Whitespace" is defined
precisely where it matters, because two different definitions are used.

- **Trim bytes**: every byte from `0x00` to `0x20` inclusive (all ASCII control characters and
  space). *Trimming* a piece of text removes trim bytes from both ends.
- **Value whitespace**: the six bytes `0x09`–`0x0D` and `0x20` (tab, line feed, vertical tab,
  form feed, carriage return, space).

Bytes `0x80`–`0xFF` are never whitespace of either kind and are never changed by case
conversion. "Lowercasing" means mapping only `A`–`Z` (`0x41`–`0x5A`) to `a`–`z`.

## 1. Encoding and lines

Files are single-byte text in Windows-1251 (Cyrillic appears in comments and in a few values; no
section name in this edition contains a byte above `0x7F`). There is no byte-order mark and no
header.

A file is a sequence of lines. A line ends at the first carriage return (`0x0D`) or line feed
(`0x0A`); any run of consecutive CR/LF bytes after it is skipped, so line terminators may be CRLF,
LF or CR and empty lines simply disappear. If a line contains a NUL byte (`0x00`), the line ends
just before it (the rest of that line is ignored; reading continues with the next line).

A line longer than 4,094 bytes is an error. There is no line continuation: every line stands
alone.

Each line is then processed in this order:

1. **Trim** the line.
2. **Remove a comment.** A comment starts at the first `;` in the line, or at a `//` pair — but
   only the *first* `/` of the line is examined: if it is immediately followed by another `/` and
   no `;` comes before it, the comment starts there. (So in `a/b // c` the `//` does not start a
   comment, and the whole text is kept.) The comment and everything after it is removed. Quotes do
   not protect `;` or `//`: comment detection happens before any quote handling.
3. **Classify** the remaining text (which may now end in trim bytes that preceded the comment):
   - empty → ignored;
   - first byte `#` and the text contains `#include` anywhere → an **include line** (section 4);
   - first byte `[` → a **section header** (section 2);
   - anything else → an **entry line** (section 3). This includes lines starting with `#` that do
     not contain `#include`.

## 2. Section headers

A section header has the form

```
[name]
[name]:parent1,parent2,...
```

- The header must contain a `]`; a header line without one is an error.
- The **name** is every byte after the opening `[` up to (not including) the first `]`,
  lowercased. It is *not* trimmed (`[ a ]` would name a section ` a `; this edition has no such
  header).
- If the text contains the two-byte sequence `]:` (no space allowed between `]` and `:`), the
  text after the first `]:` is a **parent list** (section 5). If there is no `]:`, everything after
  the first `]` is ignored. The data uses this: `config\creatures\m_dog.ltx` has the header
  `[m_dog_step_params]` followed on the same line by column labels (`fwdleftleg fwdrightleg …`),
  which are ignored.
- The header starts a new **current section**. Entry lines that follow belong to it until the next
  header in the same file or the end of the file.

A section is *registered* in the configuration when it is closed — that is, when the next section
header *of the same file* is reached or that file ends. At registration, if a section with the
same (lowercased) name is already registered anywhere in the configuration, loading fails
("duplicate section"). A section with no entries is registered like any other.

Text before the first section header of a file (other than include lines and comments) is
ignored.

## 3. Entry lines

An entry line inside a current section is split at its **first** `=`:

- **Key**: the text before the `=`, trimmed. Case and inner spaces are preserved
  (`ignore_ distance_sound` in `config\misc\gulag_escape.ltx` is a key with a space in it).
- **Value**: the text after the `=`, normalized as follows. Scan it byte by byte, keeping a
  "quoted" flag that starts off; each `"` toggles the flag and is itself **kept**; every value
  whitespace byte outside quotes is **removed** (not collapsed — removed); every other byte, and
  every whitespace byte inside quotes, is kept. An unbalanced `"` keeps whitespace up to the end
  of the line. Further `=` characters are ordinary value bytes.
- An entry line with no `=` has the whole (trimmed) text as its key and an empty value.
- If the key is empty (e.g. a line `= x`), the line is ignored.
- An empty value is stored as "no value". Readers that ask for the string get nothing (rather
  than an empty string); a consumer may treat it as the empty string.

When the current section already has an entry with exactly the same key (byte-for-byte; keys are
case-sensitive), the new value **replaces** the old one; the entry keeps its place in the section's
ordering (section 6). This applies equally to entries inherited from parents (section 5) and to a
key repeated within one section's own lines: the last line wins.

Examples (all from the data; `→` shows the stored value):

| Line (inside its section) | Key | Value |
|---|---|---|
| `max_belt		= 5 //16` | `max_belt` | `5` (`//16` is a comment) |
| `$spawn			= "weapons\ammo\ammo_9x18_fmj" ; option for Level Editor` | `$spawn` | `"weapons\ammo\ammo_9x18_fmj"` (quotes kept) |
| `inv_name		= "9x18 PBP"` | `inv_name` | `"9x18 PBP"` |
| `$spawn			= "" ; option for Level Editor` | `$spawn` | `""` (two quote bytes; not empty) |
| `discovery_dependency = ` | `discovery_dependency` | no value |
| `script_func = ;trader_job.artefacts_task_list_text` | `script_func` | no value |
| `ph_box0_center 						= 0.0,  0.9,  0.0` | `ph_box0_center` | `0.0,0.9,0.0` |
| `on_info = {=hit_by_actor} %+agr_krot_skirmish_start%` | `on_info` | `{=hit_by_actor}%+agr_krot_skirmish_start%` |
| `caption = "l04u_labx18_test` | `caption` | `"l04u_labx18_test` |
| `physics\Dead_Body\skelet_crash` (no `=`) | `physics\Dead_Body\skelet_crash` | no value |

## 4. Includes

An include line has the form

```
#include "relative\path.ltx"
```

The included path is the text between the first and the second `"` of the line (or to the end of
the line if there is no second `"`), trimmed. Text outside the quotes is ignored.

**Resolution.** The path is appended to the directory of the file that contains the include line
(everything up to and including the last path separator of that file's full path). Nested includes
therefore resolve relative to the directory of the file that contains them, not relative to the root
file. The combined path is looked up case-insensitively in the game's file tree (all paths are
lowercased; see the archive specification). Every include path in this edition uses `\` as the
separator and contains no `..`; other forms were not examined.

Example: `config\system.ltx` contains `#include "creatures\monsters.ltx"` →
`gamedata\config\creatures\monsters.ltx`, which contains `#include "M_flesh.ltx"` →
`gamedata\config\creatures\m_flesh.ltx` (the file is stored in lower case).

A missing include file is an error.

**Semantics.** The included file is processed at the point of the include line, as if its sections
were written there, with two differences that follow from section scoping:

- The included file starts with *no* current section: its lines before its own first header are
  ignored, and its last section is closed and registered when it ends.
- A section that is open in the including file when the include line is met stays open: entry
  lines after the include line still go into it, and it is registered only when the including file
  reaches its next header or ends — so it is registered *after* every section of the included file.
  (In this edition every include line comes before the first section header of its file.)

Included files may include further files. There is no guard against including a file twice; doing
so registers its sections twice and fails as a duplicate. No configuration of this edition loads
any file twice.

## 5. Section inheritance (parents)

The parent list after `]:` is a comma-separated list of section names. Each name is trimmed (so
`[x]:a, b` and `[x]:a,b` are the same) and looked up case-insensitively (section 6). A single
comma at the very end of the list is ignored; any other empty name (e.g. between two commas) is
looked up as a section named by the empty string and is therefore an error. `[x]:` with nothing
after the colon has no parents.

When the header is read — after the previous section of the same file has been registered — the
new section is filled, before any of its own lines, with a **copy** of every entry of each parent,
parents taken from left to right. A key that occurs in an earlier parent and again in a later one
gets the later parent's value. The section's own entry lines then add entries or replace inherited
values (section 3).

Consequences:

- A parent must already be **registered** when the header is read: it must be defined earlier in
  the load order (earlier in the same file, or in a file processed earlier), and it cannot be the
  section that is still open in an including file. A missing parent is an error.
- Inheritance is resolved once, at load time, by copying. Chains (`a` inherits `b` which inherits
  `c`) need no further work, because `b` already contains `c`'s entries when `a` copies it.
- There is no way to delete an inherited entry; it can only be given a new value (possibly none).

No multi-parent section of this edition has two parents that disagree on a key, so the
left-to-right rule is established from the program only.

## 6. Lookups, case and ordering

- **Section names** are stored lowercased. Looking a section up by name lowercases the requested
  name first, so `[PH_Skeleton_Object]` and `[ph_skeleton_object]` are the same section. One
  exception exists in the original program: its plain *existence test* for a section compares the
  requested name as given, without lowercasing, so asking whether `"PH_Skeleton_Object"` exists
  answers *no* while reading from it succeeds. Callers in the game pass lowercase names to that
  test.
- **Keys** are compared byte-for-byte; they are case-sensitive (`GroupControlSection` is not
  `groupcontrolsection`). The data has 1,876 key lines with upper-case letters.
- Reading a key that does not exist, or a section that does not exist, is an error in the original;
  consumers test for existence first when an entry is optional.
- **Ordering.** Sections of a configuration, and entries of a section, are kept sorted by
  byte-wise comparison of the (lowercased) section name or the key, treating bytes as unsigned.
  Anything that enumerates sections, or enumerates the entries of a section by index, sees this
  sorted order, not file order. The entry count of a section is the number of distinct keys.
  Examples: the first section of `system.ltx` is `[??tg_event]` (`?` = `0x3F` sorts before
  letters and `_`); the entries of `[mp_ammo_9x18_fmj]` are enumerated as `$prefetch`, `$spawn`,
  `GroupControlSection`, `box_size`, `buck_shot`, … (`$` < upper case < lower case).

## 7. Interpreting values

The reader stores every value as a byte string (after the normalization of section 3); the
consumer decides how to interpret it. The game consistently uses the interpretations below.
Because whitespace has already been removed outside quotes, the separators in these forms are bare
commas.

- **String**: the value as stored, quotes included.
- **Unquoted string**: one leading `"` is removed if the value starts with one, and one trailing
  `"` is removed if it ends with one (independently). `"weapons\ammo\ammo_9x18_fmj"` →
  `weapons\ammo\ammo_9x18_fmj`; `""` → empty; values up to 511 bytes.
- **Integer** (8-, 16- or 32-bit, signed or unsigned): the longest prefix that forms an optionally
  signed decimal integer, as by the C library's `atoi`; no digits gives 0. The result is truncated
  to the target width.
- **Float**: the longest prefix forming a decimal floating-point number as by the C library's
  `atof` in the "C" locale (`.` decimal point, optional exponent), converted to 32-bit float.
  Trailing garbage is ignored, which the data relies on: `0.25f` → 0.25, `3.f` → 3.0,
  `200.` → 200.0, `.2` → 0.2. No digits gives 0.
- **Boolean**: true if the value, lowercased, is exactly `on`, `yes`, `true` or `1`; anything else
  (including `off`, `false`, `no`, `0`) is false. In `system.ltx` the values seen are `on` (827),
  `true` (538), `false` (471), `off` (413) and `TRUE` (1, true).
- **Vector of 2, 3 or 4 floats** (positions, sizes, 4-component float colours): components
  separated by commas, each parsed like a float; components that are missing or fail to parse,
  and all after them, are 0. Example: `0.0,  0.9,  0.0` → (0, 0.9, 0).
- **Vector of 2, 3 or 4 integers**: the same with integer components.
- **Packed colour**: up to four integers `r,g,b,a` (missing `r`,`g`,`b` = 0, missing `a` = 255)
  packed into a 32-bit value as `a·2²⁴ + r·2¹⁶ + g·2⁸ + b`, each component taken modulo 256.
- **Token**: compared case-insensitively against a list of names defined by the consumer; the value
  associated with the matching name, or 0 when nothing matches.
- **Class identifier** (the `class` key, e.g. `AMMO`, `P_SKELET`): the first 8 bytes of the value,
  padded on the right with spaces to 8 bytes, read as a big-endian 64-bit number (first character
  in the most significant byte). The data uses ASCII only.
- **List**: comma-separated items, interpreted by the consumer (e.g. `[texture_desc] files`,
  `[ambient_env_0] sounds`). With whitespace removed, an item never has surrounding spaces unless
  quoted.
- **Section reference**: many values name another section of the same configuration (e.g.
  `immunities_sect = sect_identity_immunities`); such names are looked up like any section name.

Value shapes in `system.ltx` (57,094 resolved entries, inherited copies counted once per section):
15,803 floats, 13,928 integers, 3,004 three-component, 839 two-component and 178 four-component
numeric vectors, 2,250 booleans, 599 quoted strings, 2,639 other comma lists, 14,950 single words
and 2,904 empty values.

## 8. Root files and how the game addresses them

The engine loads configurations by alias-relative path (section 10); the main ones are:

| Root file | Alias directory | Content |
|---|---|---|
| `system.ltx` | `$game_config$` | global settings of objects, weapons, creatures, UI, etc.; includes 139 other files |
| `game.ltx` | `$game_config$` | level list, music, maps; includes 7 other files |
| `textures.ltx` | `$game_textures$` | sections `association`, `specification`, `types` |
| `level.ltx` | `$level$` (the directory of the current level) | per-level settings |

Loading a root file that does not exist yields an empty configuration (no sections); the engine
treats an empty `system.ltx` or `game.ltx` as fatal.

Game scripts and some configuration values name further configuration files by a path **relative
to `$game_config$`** (e.g. scripts open `misc\dream.ltx`, `game.ltx`, `creatures\helicopter.ltx`;
the value `trade = misc\trade_freedom.ltx` in `config\misc\gulag_mil_freedom1.ltx`;
`custom_data = stalker_custom_data.ltx` in `config\creatures\m_stalker.ltx`). All 12 distinct
`.ltx` paths that appear as values in the data exist under `gamedata\config\`. Each such file is
loaded as a root of its own configuration.

Every root file's own directory is the base for its includes (section 4).

## 9. Files with `.ltx` names that are not in this format

Some `.ltx` files are console command scripts — one command per line, no sections — and are not
read through this format: `config\default_controls.ltx` (`bind quit kESCAPE`, …),
`config\rspec_minimum.ltx`, `rspec_low.ltx`, `rspec_default.ltx`, `rspec_high.ltx`,
`rspec_extreme.ltx`, `rspec_maximum.ltx`, and the user settings file `user.ltx` written by the
installer to the user's documents folder. Read as configuration files they produce no sections.
`fsgame.ltx` is also not in this format (section 10).

## 10. Path aliases: `fsgame.ltx`

The game names directories by **aliases** of the form `$name$`. A file is addressed by an alias
plus a relative path; its full path is the alias's directory followed by the relative path,
lowercased, and that path is looked up case-insensitively in the combined file tree (archives plus
loose files; see the archive specification).

Aliases are defined by the path configuration file `fsgame.ltx` in the game root directory (the
parent of the directory containing the executables). This edition's installer writes it with the
following content (`{userdocs}` is replaced by the user's documents folder):

```
$app_data_root$ = true | false| {userdocs}\Stalker-SHOC
$game_data$ =     false| true | $fs_root$      | gamedata\
$game_ai$ =       true | false| $game_data$    | ai\
$game_spawn$ =    true | false| $game_data$    | spawns\
$game_anims$ =    true | true | $game_data$    | anims\
$game_levels$ =   true | false| $game_data$    | levels\
$game_meshes$ =   true | true | $game_data$    | meshes\
$game_dm$ =       true | true | $game_data$    | meshes\
$game_shaders$ =  true | true | $game_data$    | shaders\
$game_sounds$ =   true | true | $game_data$    | sounds\
$game_textures$ = true | true | $game_data$    | textures\
$game_scripts$ =  true | false| $game_data$    | scripts\
$game_config$ =   true | false| $game_data$    | config\
$level$ =         false| false| $game_levels$
$game_saves$ =    true | false| $app_data_root$| savedgames\
$logs$ =          true | false| $app_data_root$| logs\
$screenshots$ =   true | false| $app_data_root$| screenshots\
```

Syntax and meaning:

- Lines are split as in section 1. A line whose **first byte** is `;` is a comment (no trimming
  happens before this test). Other lines are definitions; there are no sections, includes or
  trailing comments.
- A definition is `alias = fields`. The alias is the text before the first `=`, trimmed and
  lowercased. The fields are the text between the first `=` and the next `=` (or the end of the
  line), split at `|`; each field is trimmed. At least three fields are required.
  1. **recurse** (boolean): whether loose files under the directory are indexed including
     subdirectories. Booleans here are exactly `true`, `on`, `yes` or `1`, case-sensitive.
  2. a second boolean flag; it does not affect path resolution.
  3. **root**, lowercased: if it is the name of an alias defined on an earlier line (or a
     predefined one), the root is that alias's directory; otherwise the field itself is the root
     (e.g. an absolute path).
  4. optional **subdirectory**, lowercased, appended to the root.
  5. optional default file extension, and 6. optional caption — not needed for resolution.
- The alias's directory is root + subdirectory, lowercased, with a `\` appended if it is non-empty
  and does not already end in one.
- Defining the same alias twice is an error ("fsgame.ltx is corrupted").
- Predefined before the file is read: `$fs_root$`, the game root directory (with a trailing `\`).
  The engine changes `$level$` at run time to the directory of the level being loaded.
- Asking for an undefined alias is an error.

Resolution with the installer's file: `$game_data$` = `<root>\gamedata\`, `$game_config$` =
`<root>\gamedata\config\`, so `system.ltx` is `<root>\gamedata\config\system.ltx`, which the
archive tree provides as stored path `config\system.ltx`. `$level$` before any level is loaded is
`<root>\gamedata\levels\` (no subdirectory field). `$game_saves$` = `<documents>\stalker-shoc\savedgames\`.

## 11. Errors

The original treats each of these as fatal: a line longer than 4,094 bytes; a section header
without `]`; a duplicate section name in one configuration; a missing parent section; a missing
include file; reading a missing section or key; in `fsgame.ltx`, a definition with fewer than three
fields, a duplicate alias, or an undefined alias. No root configuration of this edition triggers
any of them.

## 12. Evidence from the Russian edition

Reproducible with any reader that implements this specification, applied to the combined file tree
of the archives (`gamedata.db0` … `gamedata.dbd`, then `mods\mp_military_2.xdb0`, later entries
replacing earlier ones):

- The archives hold 1,420 entries with extension `.ltx` (1,318) or `.ini` (102), giving 625
  distinct paths in the combined tree (523 `.ltx`, 102 `.ini`; the `.ini` files are the font
  descriptors under `textures\ui\`). Together they have 4,025,893 bytes, 90,182 entry lines and
  6,895 section headers, 637 of them with a parent list.
- Every file with more than one line uses CRLF line ends; no file has a bare CR or LF. 16 files
  end with a single NUL byte. 185 files contain bytes above `0x7F`. Besides tab, CR, LF and those
  final NULs, 4 files contain control bytes (`0x07` in `config\creatures\m_flesh.ltx` and
  `m_cat.ltx`, `0x1C` in both copies of `niva_engine.ltx` under `config\models\`), all inside
  comments. The longest line has 1,345 bytes
  (`config\weathers\env_ambient.ltx`). `config\misc\gulag_general.ltx` is empty.
- 177 include lines, all resolving; 177 distinct files are included by some other file. Of the 448
  files that nothing includes, 445 load without error on their own; the other three
  (`config\creatures\m_fraction.ltx`, `config\prefetch\old_variant_prefetch_single.ltx`,
  `config\weapons\w_mac10.ltx`) are unused fragments whose parents are defined only in files they
  do not include.
- `system.ltx` loads 140 files and yields 1,245 sections with 57,094 entries; 574 parent references,
  18 sections with more than one parent; no duplicate section. `game.ltx` loads 8 files and yields
  152 sections with 1,288 entries. `textures\textures.ltx` yields 3 sections with 10,877 entries.
- 6,369 entry lines have no `=`; 791 values contain a further `=`; 402 values are empty; 185 keys
  repeat within their own section's lines (e.g. `[helicopter]` sets `light_color` twice); 41
  section headers contain upper-case letters; no header has a space between `]` and `:`; no
  `;` occurs inside quotes; no `//` is preceded by a single `/` on the same line; no include line
  follows a section header in its file.

Test vectors (configuration `system.ltx` unless stated; "none" = no value):

| Section | Key | Resolved value | Why |
|---|---|---|---|
| `script` | `current_server_entity_version` | `7` | value from the `gamedata.dbc` copy, which overrides the earlier archives |
| `inventory` | `max_belt` | `5` | `//` comment |
| `inventory` | `slot_persistent_1` | `true` | `;knife` comment removed |
| `artefacts_tasks` | `script_func` | none | whole value commented out |
| `sound` | `snd_collide_max_volume` | `200.` | |
| `helicopter` | `light_color` | `0.9,0.8,0.4` | second of two lines wins (first was `0.9,0.9,0.5`) |
| `helicopter` | `light_range` | `5` | second of two lines wins (first was `10.0`) |
| `ammo_9x18_fmj` | `$spawn` | `"weapons\ammo\ammo_9x18_fmj"` | quotes kept, trailing comment removed |
| `mp_ammo_9x18_fmj` | `$spawn` | `""` | own line overrides the value inherited from `ammo_9x18_fmj` |
| `mp_ammo_9x18_fmj` | `cost` | `70` | inherited from `ammo_9x18_fmj` |
| `mp_ammo_9x18_fmj` | `use_aim_bullet` | `false` | inherited through `ammo_9x18_fmj` → `ammo_base` → `identity_immunities` |
| `mp_ammo_9x18_fmj` | `discovery_dependency` | none | inherited empty value |
| `mp_ammo_9x18_fmj` | (entry count) | 35 | 32 of `ammo_9x18_fmj` plus 3 new keys |
| `MP_AMMO_9x18_FMJ` | `k_hit` | `1` | section lookup is case-insensitive |
| `kurtka_mask_damage` | (entry count) | 25 | parents `body_kurtka_damage` (18) and `head_mask_damage` (6) plus `hit_fraction` |
| `kurtka_mask_damage` | `jaw_1` | `1.0,0.05` | from the second parent, whitespace removed |
| `m_dog_step_params` | `stand_walk_fwd_0` | `1,0.2,0.75,0.4,0.7,0.6,0.75,0.8,0.75` | header trailer text ignored |
| `texture_desc` | `files` | `ui_mp_icon_rank,ui_icons_map,ui_common,…,ui_asus_intro` | spaces after commas removed |
| `level143` (`game_levels.ltx`) | `caption` | `"l04u_labx18_test` | unbalanced quote kept |
| `on_hit@agr_factory_hold_fake` (`misc\gulag_agroprom.ltx`) | `on_info` | `{=hit_by_actor}%+agr_krot_skirmish_start%` | first `=` splits; space removed |
| `symbol_coords` (`textures\ui\ui_font_letter_18.ini`) | `065` | `13,18,22,36` | |

## Provenance

- Date: 2026-11-23.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG installation): archives
  `gamedata.db0`–`gamedata.dbd` and `mods\mp_military_2.xdb0`, the installer's script that writes
  `fsgame.ltx`, and the shipped 32-bit Windows binaries `bin\xrCore.dll` and `bin\XR_3DA.exe`.
- Data analysis: all `.ltx` and `.ini` entries were extracted from every archive with the project's
  archive tool and combined by the precedence of the archive specification; a scratch reader
  written from this specification loaded every file, both as an include and as a root; all
  statistics and test vectors above come from it. Game script files were searched for the
  configuration paths they open.
- Binary analysis: static analysis (Ghidra 12.2 headless) of the configuration reader, the path
  alias setup and the file lookup in `xrCore.dll`, and of the code in `XR_3DA.exe` that opens the
  root configuration files. The binaries were not executed. Facts marked as "established from the
  program only" (multi-parent conflicts, behaviour of unbalanced or unusual syntax not present in
  the data, error conditions, the `fsgame.ltx` flag meanings and the location of `$fs_root$`) were
  not exercised by the data.
- No X-Ray source code, derivative engine, or third-party documentation of these formats was used.
