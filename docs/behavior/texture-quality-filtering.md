# Texture quality and texture filtering settings (first renderer)

Behaviour specification, in two parts, of what two groups of video settings do in the first
(static-lighting) renderer, `renderer_r1`, of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition):

- **Part 1, `texture-quality`:** `texture_lod` — how many top mip levels a texture loses when it is
  loaded, the list `[reduce_lod_texture_list]` and how a texture matches it, files with few levels, which
  loads pick up a change.
- **Part 2, `texture-filtering-settings`:** `r__tf_aniso` and `r1_tf_mipbias` — when they reach the
  graphics device (on set, on status read, at device creation), the engine's own values after a device
  reset, and how both interact with the passes of shaders. Section 9 is the correction of
  `docs/behavior/static-shading.md` 4.4 asked for by section 5 of the scoping specification.

These are items 5 and 6 of the scoping specification `docs/behavior/r1-settings-scope.md` (here RS),
written as one specification. Builds on: `docs/behavior/static-shading.md` (SS: texture lookup 4.2,
sampler states 4.4, renderer settings 11), `docs/formats/save-picture.md` (SPIC: one-level pictures and
textures shared by name, 6), `docs/behavior/options-screens.md` (OS: control refresh 4.5, restart step
5.4, the controls 6.2, presets 8, the user configuration file 12), `docs/formats/ltx-config.md` (LTX: key
case 6, path aliases 10), `docs/formats/db-archive.md` (archive precedence). What could not be
established is marked **unestablished**; what is inferred without proof is marked **believed**, with why.

Conventions:

- "The engine" is `XR_3DA.exe`, "the renderer" `xrRender_R1.dll`, "the game library" `xrGame.dll`, all
  of `bin\` of the shipped edition. "Variable", "setting a variable", "status" and "video restart" are as
  in RS (conventions) and OS 7.
- A **device reset** is the reset of the existing graphics device (the Direct3D 9 device's own reset):
  the device is kept, its states return to their defaults, resources in the managed pool survive it
  (**documented** Direct3D 9 behaviour). `vid_restart` is one of several causes (section 8.1).
- A texture's **levels** are its mip levels, level 0 the largest. A **2D texture** is a texture that is
  not a cube map. Direct3D 9 sampler states are named by their public names (`D3DSAMP_MAXANISOTROPY`,
  `D3DSAMP_MIPMAPLODBIAS`, …) so that statements can be checked against the public documentation.
- A **stage** is one of the device's texture sampler stages 0, 1, …; S is the number of stages the two
  settings are applied to (section 6.2).
- Every claim carries a tag [TQ-n]; the checks are one script in the dirty workspace,
  `texq/checks/run.py` (helpers `lib.py`, `pe_imports.py`, `pe_exports.py`, `dds.py` in the same folder).
  It reads the shipped binaries (imports, exports, strings, constants), static analysis of the same
  binaries, the DDS headers of every texture file in the archives (read in place), the extracted
  configuration files and, for the statements this document corrects, the approved specifications. All 71
  checks pass.

# Part 1 — Texture quality (`texture_lod`)

## 1. The variable and the rule

### 1.1 Variable

`texture_lod` is registered by the engine: integer 0 … 4, built-in value 0 [TQ-1]. Its only reader is
the renderer's texture loading, which consults it each time a texture file is loaded; the game library
does not read it [TQ-1]. The shipped `user.ltx` sets `texture_lod 0` [TQ-15]; the presets set the
values of OS 8.

### 1.2 Which loads are reduced

Reduced (rule of 1.3–1.5) is every **2D texture file** read through the renderer's texture loading:

- a texture name resolved to a `.dds` file as in SS 4.2 (`$level$`, `$game_saves$`, `$game_textures$`),
  including the light maps, the level's own textures and the pictures of saved games (SPIC 6);
- every frame of a `.seq` sequence (each frame name is loaded like a plain name);
- the substitute of a missing bump texture: a name containing `_bump` for which no
  `$game_textures$<name>.dds` exists loads `textures\ed\ed_dummy_bump#.dds` if the name contains
  `_bump#`, otherwise `textures\ed\ed_dummy_bump.dds`, and logs `! auto-generated bump map: <name>`
  [TQ-6]. Both substitutes are 64 × 64 with 7 levels [TQ-6].

Not reduced [TQ-6]:

- **cube-map files**: a DDS file whose resource type is a cube texture is loaded with all its levels;
  the archives hold 168 cube-map files (e.g. the sky cubes of `textures\sky\`) and no volume texture;
- **video textures**: a name with a `$game_textures$<name>.ogm` file (Theora) or `.avi` file plays as
  video and never goes through the rule (the archives hold 38 `.ogm` files and 11 `.seq` sequences under
  `textures\`, no `.avi`);
- names equal (ignoring case) to `$null` and names containing `$user$` load no file.

The order of the tests for a name is `.ogm`, `.avi`, `.seq`, then the plain `.dds` lookup [TQ-6].

### 1.3 Levels to drop

With v = `texture_lod` at the moment of the load, the number n of top levels to drop is [TQ-2]:

| v | 0 | 1 | 2 | 3 | 4 |
|---|---|---|---|---|---|
| texture is **listed** (1.4) | 0 | 1 | 1 | 2 | 2 |
| other textures | 0 | 0 | 1 | 1 | 2 |

So a listed texture is treated as if the setting were one step higher, at most 2.

### 1.4 The list and the match

- The list is the section `[reduce_lod_texture_list]` of the game's main configuration
  (`$game_config$system.ltx`, which includes `external.ltx`, where the section is defined) [TQ-3]. Its
  entries are the section's **keys** (LTX: keys keep their case, LTX 6) [TQ-3].
- A texture is **listed** when at least one key occurs as a contiguous substring of the
  **lowercased full path of the file actually read** [TQ-3]. The full path is the alias directory
  followed by the name and `.dds` (LTX 10), e.g.
  `<root>\gamedata\levels\l01_escape\terrain\terrain_escape.dds` or
  `<root>\gamedata\textures\act\act_antigas.dds`; for a bump substitute it is the substitute's path.
- Consequences:
  - A key with an upper-case letter never matches. The shipped list has 635 distinct keys without values,
    each of the form `<folder>\<name>`; seven of them contain upper-case letters and therefore have no
    effect: `act\act_Controller`, `act\act_Lenin01`, `act\act_Lenin02`, `act\act_Rat`,
    `act\act_Soldier`, `wpn\wpn_FN2000`, `wpn\wpn_PM` [TQ-4].
  - A key matches every path that contains it, not only the file of that name: `act\act_antigas` also
    matches `act\act_antigas_bump.dds`, `terrain\terrain_escape` also matches
    `terrain\terrain_escape_lm.dds` [TQ-4].
  - The directory part takes part in the match, so a key matches a texture in any alias directory whose
    path contains it, and in principle through the installation or documents folder names. No shipped key
    occurs in the fixed parts `\gamedata\textures\`, `\gamedata\levels\`, `\savedgames\`,
    `stalker-shoc` or in `.dds` [TQ-4].
- Shipped list by folder: `act` 259, `pfx` 143, `wpn` 114, `terrain` 65, `detail` 54 keys [TQ-4]. Of the
  archives' 5,306 `.dds` files (4,941 under `textures\`, 365 under `levels\`), 808 non-cube files are
  listed (106 of them with one level); no cube file is listed [TQ-4]. The listed files include the
  level-folder terrain textures and their `_lm` companions of the 23 levels that have one (46 files) —
  for the first level `levels\l01_escape\terrain\terrain_escape.dds` and `…_lm.dds`, through the key
  `terrain\terrain_escape` [TQ-4].

### 1.5 Dropping levels

Let the file have L levels and top size W × H, and n be the count of 1.3 [TQ-5]:

- If L ≤ 1 nothing is dropped (a one-level file is always loaded as stored; SPIC 6 relies on this).
- Otherwise d = min(n, L − 1) levels are dropped: the texture gets L − d levels and top size
  max(1, ⌊W / 2^d⌋) × max(1, ⌊H / 2^d⌋), i.e. the size of the file's level d.
- Level i of the texture is level i + d of the file, copied unchanged: same pixel format (block-compressed
  formats stay compressed), no filtering or re-encoding [TQ-5].

In the shipped data no non-cube file has exactly two levels: 1,157 have one level and the others three or
more, so for every multi-level file d = n [TQ-5].

## 2. When a change takes effect

- The value is read only when a texture file is loaded [TQ-1]. A texture exists once per name; asking for
  a name that already exists returns the existing texture without loading anything (SPIC 6: textures are
  shared by name and released when nothing refers to them) [TQ-7].
- A texture is loaded when it is created by name, if the graphics device exists and loading is not being
  deferred; a deferred texture is loaded during the prefetching at the start of a game (all textures not
  yet loaded) or at its first use, whichever is first [TQ-7]. Loading does nothing for a texture that
  already holds its image [TQ-7].
- **A video restart reloads no texture.** `vid_restart` is a device reset (8.1); the reset handling of the
  resources rebuilds render targets, geometry streams and state blocks and does not touch textures, which
  live in the managed pool and survive the reset (**documented** Direct3D 9 behaviour) [TQ-7]. So textures
  keep the levels they were loaded with until they are released and created again.
- Hence a new value applies to the textures created after the set:
  - set in the main menu before a game is started or loaded: the level's textures are loaded afterwards and
    use it; textures already in use by the menu keep theirs;
  - set during a game (options screen opened from the game): textures of the running level keep their
    levels; only textures created later use the new value.
  - **Believed:** a level's own textures are released with the level when another level is started or a
    game is loaded (the old level is always destroyed before a new one is created, 8.1), and are then
    reloaded with the current value; textures that stay referenced across levels (menus, objects kept by
    the game) keep their levels. Which textures stay referenced is **unestablished**.

## 3. Verification facts

- With the shipped `texture_lod 0` nothing is dropped from any texture (SS 4.2) [TQ-2, TQ-15].
- First level at `texture_lod 1`: `terrain\terrain_escape` and `terrain\terrain_escape_lm` (both from the
  level folder) and every texture whose path contains one of the `act\`, `pfx\`, `wpn\`, `detail\`,
  `terrain\` keys lose their top level if they have more than one level; all others are unchanged [TQ-2, TQ-4].
- At `texture_lod 4` every multi-level 2D texture loses two top levels; one-level files (light maps, save
  pictures) and cube maps are unchanged [TQ-2, TQ-5, TQ-6].

# Part 2 — Texture filtering settings (`r__tf_aniso`, `r1_tf_mipbias`)

## 4. The variables

| Variable | Registered by | Kind, range | Built-in | Shipped `user.ltx` |
|---|---|---|---|---|
| `r__tf_aniso` | the renderer | integer 1 … 16 | 4 | 16 |
| `r1_tf_mipbias` | the renderer | real −0.5 … 0.5 (RS 1) | 0 | 0. |

[TQ-8, TQ-15]. Both have their own handling on set and on status read (section 6) [TQ-8]. The options
screen has a control for `r__tf_aniso` only (`track_aniso`, OS 6.2 row 4); `r1_tf_mipbias` is set only by
the console, script files such as `user.ltx` and the preset files (OS 8) [TQ-10, TQ-15]. In the shipped
binaries only the two renderer libraries contain either name, and in the game data only the six preset
files and the options screen layout [TQ-15].

## 5. What reaches the device

Two per-stage sampler states of the graphics device are concerned:

- `D3DSAMP_MAXANISOTROPY` (maximum anisotropy);
- `D3DSAMP_MIPMAPLODBIAS` (mip level-of-detail bias, a floating-point value).

The values in force on a stage are those last written to it by one of two writers:

- **the settings** (section 6): maximum anisotropy = clamp(`r__tf_aniso`, 1, 16), bias = `r1_tf_mipbias`
  (the stored real value, unchanged) [TQ-9];
- **the engine's defaults** (section 7): maximum anisotropy 4, bias −0.5 [TQ-11].

Nothing else in the game writes these two states (section 8.3) [TQ-14].

## 6. Applying the settings

### 6.1 When

The current value of a variable is applied:

- after **every execution of the variable with an argument** (console line, script line, preset file,
  options control), whether or not the argument was accepted: a rejected value (out of range) leaves the
  variable unchanged and its current value is applied again [TQ-9];
- after **every read of its status** [TQ-9]. The status is read when the bare name is executed without an
  argument (the console then logs `- <name> <status>`) [TQ-10], and when the user configuration file is
  written (`cfg_save`, OS 12.3; the game writes it at normal shutdown);
- by the renderer itself, at device creation and in its part of every device reset, by executing the bare
  name of each variable (a status read) [TQ-12].

The options screen's anisotropy bar sets `r__tf_aniso` whenever it is refreshed (OS 4.5), which happens
on every opening of the screen (OS 5.2) and after a preset is chosen (OS 6.1); each refresh therefore
applies `r__tf_aniso` [TQ-10].

Nothing is applied while no graphics device exists: values set before the device is created (e.g. by
`user.ltx`, which runs before device creation, RS 2.10) reach the device at its creation (7.2) [TQ-9,
TQ-12].

### 6.2 On which stages

Both are written to stages 0 … S − 1, where S is the device's reported number of simultaneously usable
textures, taken modulo 16 [TQ-9, TQ-11]. Direct3D 9 devices report at most 8 here (**believed**, from the
public documentation of that capability), so the modulo has no effect in practice; stages S and above keep
the device default (maximum anisotropy 1, bias 0; **documented**).

## 7. The engine's defaults and the order of writers

### 7.1 Engine defaults

When the device is created and after every device reset, the engine sets on stages 0 … S − 1: maximum
anisotropy 4, bias −0.5, and minification, magnification and mip filter linear [TQ-11]. (Passes set the
filters of the samplers they declare from their own recorded states, 8.3; no pass sets anisotropy or
bias.)

### 7.2 Device creation

Order: engine defaults, then the renderer applies both variables [TQ-12]. **After creation the settings
are in force.**

### 7.3 Device reset

Order: the resources are reset, ending with the renderer's part, which applies both variables; then the
engine defaults [TQ-12]. **After every device reset the engine's 4 and −0.5 are in force**, on all stages
0 … S − 1, until a variable is next applied (6.1), each variable independently.

## 8. Consequences

### 8.1 When the device is reset

A device reset happens [TQ-13]:

1. on `vid_restart` (only when the device exists), i.e. at the end of every Apply and Cancel of the
   options screen (OS 5.4) and from the console;
2. at the **start of every level** (new game, loaded game, level change): the main menu is switched off,
   the device is reset, then the level is created and loaded [TQ-13]. A level is only started when no level
   exists, so a running level is always destroyed before the next one starts [TQ-13];
3. at the start of a frame when the device reports that it was lost and can be restored (e.g. after
   switching away from a full-screen game) [TQ-13];
4. twice in the level-map capture of the developer demo recorder (not reachable in normal play) [TQ-13].

### 8.2 Values in force with the shipped settings

With the shipped `user.ltx` (`r__tf_aniso 16`, `r1_tf_mipbias 0.`, `_preset Extreme`) [TQ-15] and no
console use:

| Situation | Maximum anisotropy | Bias |
|---|---|---|
| After start-up (menus) | 16 | 0 |
| In a level, after it was started or loaded (8.1 item 2) | 4 | −0.5 |
| Options screen opened (the anisotropy bar is refreshed) | 16 | unchanged (−0.5 in a level) |
| Options screen closed with Apply or Cancel (`vid_restart`) | 4 | −0.5 |
| Options screen closed with Esc (no restart) | 16 | unchanged |
| A preset chosen on the options screen (its file sets both) | the preset's | the preset's, until the next Apply/Cancel |
| After a lost device was restored | 4 | −0.5 |

**Believed:** nothing during level loading applies either variable. Evidence: no binary other than the two
renderer libraries contains the names, the renderer names them only at registration and in its creation and
reset handling, and the game data names them only in the preset files and the options layout [TQ-12,
TQ-15]. A status read through a generic listing of all variables other than `cfg_save` is not excluded:
**unestablished** whether one occurs during play.

### 8.3 Interaction with the passes of shaders

- No pass of any shader writes maximum anisotropy or bias: the sampler states a pass records are only
  addressing (U, V, W) and filters (magnification, minification, mip) [TQ-14]. This holds for library
  definitions and scripted definitions alike (both record through the engine), and the renderer records
  sampler states only through the engine [TQ-14]. Replaying a pass's recorded states changes only the
  states it recorded (**documented** behaviour of Direct3D 9 state blocks).
- Directly, sampler states are written only by the engine defaults (7.1), the replay of passes' recorded
  states, the renderer's handling of the two variables, and one debug entry of the engine that nothing calls
  [TQ-14].
- Therefore every pass samples with the values in force on its stage (section 5): a pass's sampler bound
  to stage k uses stage k's maximum anisotropy and bias.
- Maximum anisotropy has an effect only where the sampler's minification or magnification filter is
  anisotropic (**documented** Direct3D 9 behaviour); in the first renderer these are the samplers named in
  SS 4.4 (`s_base`, `s_detail` and their rule). **Disputed in review:** unbacked and too narrow as a
  statement about the whole first renderer: a pass's filters are recorded per sampler, and besides the
  library rule there are recording paths that set anisotropic minification (with or without anisotropic
  magnification) and paths that record filter values given by the shader definition (scripted
  definitions included, e.g. SS 8.1); which samplers of all shaders end up anisotropic is not established
  here. The bias shifts the mip level chosen by every mipmapped
  sampler (**documented**): positive values select smaller levels (blurrier), negative values larger ones
  (sharper). Samplers without a mip filter (e.g. `s_hemi`, SS 4.4) and one-level textures are unaffected
  by the bias (**documented**: without a mip filter level 0 is sampled).
- How the device default anisotropy 4 versus 16 shows in the pixels depends on the graphics hardware's
  anisotropic filter (**unestablished**: no execution).

## 9. Correction of `docs/behavior/static-shading.md` 4.4

SS 4.4 states the values as "On every sampler the maximum anisotropy is clamp(`r__tf_aniso`, 1, 16)
(default 16) and the mip LOD bias is `r1_tf_mipbias` (default 0)" [TQ-16]. Corrected statement:

> On every sampler the maximum anisotropy and the mip LOD bias are the values in force on the sampler's
> device stage (texture-quality-filtering 5–7): clamp(`r__tf_aniso`, 1, 16) and `r1_tf_mipbias` after the
> variables were last applied, but 4 and −0.5 after a device reset until then. A device reset happens at the
> start of every level, so a level drawn with the shipped settings and without a later change of either
> variable uses maximum anisotropy 4 and bias −0.5 (texture-quality-filtering 8.2). The maximum anisotropy
> matters only on samplers with anisotropic filtering.

SS 11's list "`r__tf_aniso 16`, `r1_tf_mipbias 0`" remains correct as the shipped variable values; the
values on the samplers are those above.

## 10. Statements of approved specifications this contradicts

For the orchestrator; not changes this document makes:

- **OS 5.4** ("`vid_restart` … textures are reloaded") and **OS 6.2 row 3** (`texture_lod`: "textures
  reload at the restart"): a video restart reloads no texture (section 2) [TQ-7].
- **RS section 1**, row `texture_lod` ("video restart reloads (OS 5.4)") and RS 2.5 ("the options screen
  marks a video restart for it (OS 5.4), which reloads the textures"): same [TQ-7, TQ-16].
- **SS 4.4**: section 9 [TQ-16].
- RS 2.6 states the restart override for a video restart only; it applies to every device reset, including
  the start of every level (8.1).

## 11. Not established

- Which textures stay referenced across a level change or a load, and so keep their levels (2).
- Whether a status read of either variable occurs during play other than by `cfg_save` at shutdown (8.2).
- The visible difference between the anisotropy values on real hardware (8.3).
- The level count requested for a bump substitute's file (1.2): the rule of 1.5 applies to what is loaded.
- Nothing was executed; all timing statements come from the order of operations in the binaries.

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; archives `gamedata.db0` … `gamedata.dbd` with archive precedence; the shipped
  `__support\userdocs\user.ltx`.
- Data analysis: the DDS headers (size, level count, cube and volume flags) of all 5,306 `.dds` entries of
  the archives, read in place (all entries are stored uncompressed); the archive listings for `.seq`,
  `.ogm`, `.avi`; `config\external.ltx`, `config\system.ltx`, `config\rspec_*.ltx`,
  `config\ui\ui_mm_opt.xml`, the shipped `user.ltx`; byte searches of all 33 shipped binaries.
- Binary analysis: static analysis with Ghidra (headless, read-only) of `XR_3DA.exe` (variable
  registration, console command execution and status reads, texture creation and loading, deferred loading,
  device creation and reset, device-state setup, device capabilities, application events, pass sampler-state
  recording) and `xrRender_R1.dll` (variable registration and handlers, texture file loading and level
  dropping, device creation and reset handling); import and export tables read directly from the files. No
  execution or debugging.
- Public Direct3D 9 documentation for the meaning of sampler states, state blocks, device reset and the
  managed pool.
- Approved specifications of the repository read for the statements built on and corrected.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The pixel format requested for a bump substitute's file (1.2) is not established either, like its level count (section 11); the "same pixel format" of 1.5 is established only for the plain `.dds` lookup.
- `vid_restart` has a second caller in the game library besides the options screen's restart step: when the main menu is switched off and a pending-restart mark of the menu is set, `vid_restart` is executed; whether and when that mark is ever set is not established, so 8.1 item 1 and the table of 8.2 may miss a device reset.
