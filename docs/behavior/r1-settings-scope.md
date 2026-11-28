# Video settings of the first renderer (scoping specification)

Scoping specification for what the video and graphics settings change in the picture when the first
(static-lighting) renderer, `renderer_r1`, is running. The options screen that edits these settings,
their stored values and when the screen sets them are specified in `docs/behavior/options-screens.md`
(here "OS"); this document covers what happens after a variable is set. For every variable it states who
registers it, whether the first renderer reads it, when a new value takes effect, which approved
specification already describes its effect, and what is still unspecified. It then proposes a split of the
unspecified parts into small specifications. Rules belonging to those specifications are stated here only
as far as they decide scope. What could not be established is marked **unestablished**.

Conventions:

- A **variable** is a console variable as in OS section 7: `<name> <value>` sets it, its status is the
  text read back. "Setting" a variable means executing that command, from the console, from a script
  file such as `user.ltx` or a preset file, or from the options screen.
- "The engine" is `XR_3DA.exe`, "the renderer" is `xrRender_R1.dll`, "the game library" is
  `xrGame.dll`, all of the shipped Russian edition (`bin\`).
- A **video restart** is the console command `vid_restart` (OS section 5.4): the graphics device is reset
  while the game keeps running. **Device creation** happens once, at startup.
- "Takes effect **at once**" means: the picture of the next frame drawn after the set shows the new value.
- Abbreviations of approved specifications: SS `docs/behavior/static-shading.md`, VP
  `docs/behavior/view-projection.md`, SPIC `docs/formats/save-picture.md`, LV `docs/formats/level.md`,
  OGF `docs/formats/ogf.md`, MS `docs/behavior/model-shading.md`, ODL `docs/behavior/object-detail-level.md`,
  DL `docs/behavior/dynamic-lights.md`,
  GL `docs/behavior/glows.md`, TO `docs/behavior/torch.md`, SKY `docs/behavior/sky.md`, CF
  `docs/formats/level-cform.md`, LL `docs/behavior/level-loading.md`, CH `docs/formats/chunks.md`.

**Checks.** Every factual claim carries a tag [RS-*n*]. The checks are one script in the dirty
workspace, `r1set/checks/run.py` (helpers `lib.py`, `pe_imports.py`); it reads the shipped binaries
directly (strings, imports, constants, code references), uses read-only static analysis of the same
binaries for the behaviour of the code, the game data extracted from the archives (compared with the
archive tool's output for the preset files), the shipped `user.ltx`, and, for RS-12, the approved
specifications in the repository. All 150 checks pass.

## 1. Summary table

"Registered by": R = the renderer, E = the engine, G = the game library, — = nobody. "Read by the first
renderer": whether the value influences what the first renderer draws (directly, or through the engine
when the engine computes something the renderer draws with). "Effect" is when a new value takes effect.
"Specified" names the approved specification that already describes the effect; "Open" what is not yet
specified (section 3 proposes where it goes).

| Variable | Reg. | Kind, range | Read by the first renderer | Effect | Specified | Open |
|---|---|---|---|---|---|---|
| `rs_c_gamma`, `rs_c_contrast`, `rs_c_brightness` | E | real 0.5–1.5 [RS-1] | not for drawing: a display gamma ramp set by the engine [RS-5]; the renderer uses the table only for the screen capture (SPIC 4.1) [RS-11] | at once [RS-5] | the ramp's values: SPIC 4.1 | what the ramp applies to and when it is re-applied (2.1) |
| `rs_vis_distance` | E | real 0.4–1.5 [RS-1] | through the far plane [RS-10, RS-11] | at once, smoothed (VP 1, 2.2) | VP 2.1–2.2, SS 9.2, SKY 8, GL [RS-12] | nothing |
| `r__geometry_lod` | R | real 0.1–1.2 [RS-1] | yes [RS-10, RS-11] | at once (per frame) [RS-10] | static geometry: SS 10.2, LV 9.4, OGF 8 [RS-12] | objects (MS 3; ODL 3, 4) (2.3) |
| `r__detail_density` | R | real 0.2–0.6 [RS-1] | yes [RS-10, RS-11] | when a detail slot is built (2.4) | — [RS-12] | all of detail objects (2.4) |
| `texture_lod` | E | integer 0–4 [RS-1] | yes, when textures load [RS-8] | at the next texture load; video restart reloads (OS 5.4) | value 0 only: SS 4.2; one-level files: SPIC 6 [RS-12] | levels dropped per value and list (2.5) |
| `r__tf_aniso` | R | integer 1–16 [RS-1] | yes [RS-7] | at once [RS-7] | value used: SS 4.4 [RS-12] | timing, restart override (2.6) |
| `r__supersample` | E | integer 1–4 [RS-1] | yes [RS-9, RS-11] | target size at device creation and video restart; use per frame [RS-9] | target size: SS 10.2 [RS-12] | presentation of the target (2.7) |
| `rs_v_sync` | E | on/off [RS-1] | no [RS-6] | none [RS-6] | OS 6.2 (says "used at the next video restart") [RS-12] | correction only (section 5) |
| `rs_refresh_60hz` | E | on/off [RS-1] | no (device setting) [RS-6] | at device creation and video restart [RS-6] | OS 7.2 [RS-12] | nothing for the picture (2.8) |
| `rs_fullscreen` | E | on/off [RS-1] | only for the screen capture [RS-6] | at device creation and video restart [RS-6] | OS 6.1, SPIC 2 [RS-12] | window style (2.8) |
| `ai_use_torch_dynamic_lights` | G | on/off [RS-1] | indirectly (which torch lights exist) | when a torch is switched (TO) | TO [RS-12] | nothing |
| `_preset` | R | list of 5 (OS 7.2) [RS-1] | no (runs a preset file) [RS-3, RS-11] | at once: the file's commands run [RS-3] | OS 8 [RS-12] | startup order (2.10) |
| `r1_tf_mipbias` | R | real −0.5–0.5 [RS-1] | yes [RS-7] | at once [RS-7] | value used: SS 4.4 | timing, restart override (2.6) |
| `r1_ssa_lod_a`, `r1_ssa_lod_b` | R | real 16–96, 16–64 [RS-1] | yes [RS-10, RS-11] | at once (per frame) [RS-10] | SS 10.2, LV 9.4, OGF 8 | nothing for static geometry |
| `r__dtex_range`, `r__ssa_glod_start`, `r__ssa_glod_end`, `rs_detail`, `r__wallmark_ttl`, `rs_skeleton_update`, `vid_bpp` | — | not variables [RS-2] | no | none: unknown command [RS-2] | — | nothing (2.10) |
| `ph_iterations` | G | integer 15–50 [RS-1] | no (physics solver) | at once [RS-1] | CF 6.1 [RS-12] | nothing for the picture |
| `r1_dlights`, `r1_dlights_clip` | R | [RS-1] | yes [RS-11] | DL | DL [RS-12] | nothing |
| `r1_glows_per_frame` | R | integer 2–32 (GL 5) [RS-12] | yes (GL) | GL | GL [RS-12] | nothing |
| `r1_lmodel_lerp` | R | real 0–0.333 [RS-1] | yes [RS-11] | MS 7 | MS 7 [RS-12] | nothing |
| `r1_pps_u`, `r1_pps_v` | R | real −1–1 [RS-1] | yes, presentation quad [RS-9, RS-11] | per frame [RS-9] | — | (2.7) |
| `r2_sun_lumscale` | R | DL 8 | yes [RS-10, RS-11] | DL | DL 3.1, 8 [RS-12] | nothing |
| `rs_stats` | E | on/off [RS-1] | no: engine statistics overlay [RS-11] | at once (per frame) [RS-11] | — | (2.11) |
| `vid_mode` | E | resolution [RS-1] | yes: screen size | at device creation and video restart (OS 9) | OS 9; screen size used by SS 10.2, LV 9.4 | nothing new |

The renderer also registers the second renderer's variables (`r2_*`, including `r2_sun`,
`r2_sun_details`, `r2_slight_fade`, `r2_ls_squality`, `r2_tf_mipbias`, `r2_ssa_lod_a`, `r2_ssa_lod_b`)
[RS-1]; apart from `r2_sun_lumscale` none of their values is read by the first renderer: the on/off ones
are touched only by another second-renderer command, the others only where they are registered [RS-11].
They therefore have no effect on the picture of the first renderer.

## 2. Findings per setting

### 2.1 Gamma, contrast, brightness

- Setting any of the three rebuilds the colour table of SPIC 4.1 from the three current values and hands
  it to the graphics device as its gamma ramp, at once [RS-5].
- The ramp is also set when the device is created and when the loading screen's frame count runs out
  at the end of loading [RS-5]. A video restart does not itself set it [RS-5]; whether
  the device keeps the ramp across the restart is **unestablished** (no execution).
- These are the only three places in the engine that build the table for the device [RS-5]. The
  renderer uses the table only in the screen capture of SPIC [RS-11], not for drawing: the ramp is applied by the display hardware to every presented frame, menus included
  (**documented** behaviour of a device gamma ramp; not measured here). Whether the ramp has any effect
  in a windowed display is **unestablished**.

### 2.2 View distance

`rs_vis_distance` is read in one place, where the environment is blended to give the far plane, every
frame [RS-10]; the renderer does not import it [RS-11]. Its whole effect (far plane = blended far plane ×
the value, then smoothed, and everything that depends on the far plane: clipping, glows, sky, rain
sounds) is in VP 2.1–2.2, SS 9.2, SKY 8, GL and `docs/behavior/sound-rain-thunder.md` [RS-12]. Nothing remains open.

### 2.3 Geometry detail level

`r__geometry_lod`, `r1_ssa_lod_a` and `r1_ssa_lod_b` are read by the renderer's per-frame visibility
pass [RS-10]; a new value takes effect at once. Their use for static geometry (visibility threshold,
distant-billboard cross-fade, progressive detail) is in SS 10.2, LV 9.4 and OGF 8 [RS-12]. Open: the
detail level and small-object test of **objects** (the earlier object specification's 4.9 and 5.1 marked them unestablished; now MS 3, ODL 3 – 4) [RS-12].

### 2.4 Grass density

`r__detail_density` is read in one place: when a slot of the detail-object grid (grass and similar,
file `level.details`) is built, the number of placement positions is derived from it (2 divided by the
value) [RS-10, RS-11]. A new value therefore applies to slots built after the set; slots already built
keep theirs (**believed**: slots are rebuilt as the camera moves; the rebuild rule is not established
here). No approved specification describes `level.details` beyond its chunk container (CH) or the
drawing of detail objects [RS-12]; the density's exact use belongs to that specification.

### 2.5 Texture quality

`texture_lod` is registered by the engine and read by the renderer's texture loader only [RS-8,
RS-11]: when a texture is loaded, a number of its top mip levels is dropped. The count depends on the
value and on whether the texture name contains an entry of the section `[reduce_lod_texture_list]`
(`config\external.ltx`, 635 entries, the first `act\act_antigas`) [RS-8]:

| `texture_lod` | 0 | 1 | 2 | 3 | 4 |
|---|---|---|---|---|---|
| name matches a list entry | 0 | 1 | 1 | 2 | 2 |
| other names | 0 | 0 | 1 | 1 | 2 |

[RS-8]. A change applies to textures loaded afterwards; the options screen marks a video restart for it
(OS 5.4), which reloads the textures. SS 4.2 covers only the value 0, SPIC 6 the files with one level
[RS-12]. Open for its specification: "contains" as the matching rule (substring of which form of the
name), how levels are dropped when a file has fewer levels, and which loads besides the restart pick
up a new value.

### 2.6 Texture filtering and mip bias

- After every set of `r__tf_aniso` and every read of its status, the renderer sets the maximum
  anisotropy of every texture stage of the device to clamp(value, 1, 16); after every set or status read
  of `r1_tf_mipbias` it sets the mip level-of-detail bias of every stage to the value [RS-7]. Both take
  effect at once. (The options screen reads the status when it refreshes its controls, OS 4.5.)
- The renderer applies both again in its device-creation and device-reset handling [RS-7].
- The engine's own device state setup sets maximum anisotropy 4 and mip bias −0.5 on every stage
  [RS-7]. At device creation this runs **before** the renderer applies the variables, so the variables
  win; at a video restart it runs **after** the renderer's reset handling, so the engine's 4 and −0.5 are
  in force after a restart until either variable is next set or its status read [RS-7]. How this shows in
  the picture (whether shader passes set their own values) is **unestablished**.
- SS 4.4 gives the values used on the samplers [RS-12]; the timing and the restart override are open.

### 2.7 Anti-aliasing (supersampling) and presentation

- `r__supersample` is registered by the engine and imported by the renderer [RS-1, RS-11]. The
  renderer's off-screen target is sized from it (SS 10.2) when the target is built, which happens at
  device creation and at every video restart [RS-9]; the options screen marks a restart for it (OS 5.4).
- Each frame the scene is drawn into that target when `r__supersample` is above 1 or a post-process
  effect is active; otherwise directly to the screen [RS-9]. The target is then drawn to the screen with
  the shader `postprocess` as a screen quad whose position is offset by `r1_pps_u`, `r1_pps_v`, with a
  constant named `c_brightness` [RS-9].
- SS 10.2 states the target size and marks the presentation **unestablished** [RS-12]. Open: the
  presentation pass (filtering of the downscale, the offset, the constants), when a post-process effect
  counts as active (game-driven effects are outside this area and need their own scope).

### 2.8 Vertical sync, refresh rate, full screen, resolution

- `rs_v_sync` has **no effect**: the only code that reads it (a choice of presentation interval) is never
  called, and both device creation and video restart always present with an interval of one display
  refresh [RS-6]. OS 6.2 row 11 says otherwise (section 5). `rs_no_v_sync` is not a variable at all: it is
  only a name in the options screen's restart list [RS-11].
- `rs_refresh_60hz` and `rs_fullscreen` are read when the device is created or reset [RS-6]; their
  meaning is in OS 7.2 and OS 6.1. `rs_fullscreen` also chooses the window style [RS-6], and the renderer
  tests it only for the screen capture (SPIC 2) [RS-6]. Neither changes how a frame is drawn.
- The colour depth of the display is always 32 bits: the engine's built-in depth is 32 and nothing
  changes it [RS-2]; `vid_bpp` is not a variable (2.10). OS 9 conditions the format on `vid_bpp`
  (section 5).
- `vid_mode` sets the screen size used at device creation and video restart (OS 9); the size enters the
  thresholds of SS 10.2 and LV 9.4.

### 2.9 Characters' torch lights

`ai_use_torch_dynamic_lights` is registered by the game library [RS-1]; its effect (characters withhold
their torch lights while it is off, consulted when a torch is switched and for placement) is fully in TO
[RS-12]. Nothing remains open.

### 2.10 Presets, the stored file, and names no binary knows

- Setting `_preset` stores the name and then executes `cfg_load` of `$game_config$rspec_<name>.ltx`
  [RS-3]; what each file sets is OS 8; the six files `rspec_minimum`, `low`, `default`, `high`, `extreme`, `maximum` are in `gamedata.dbb` [RS-3].
- The shipped `user.ltx` begins with `_preset Extreme` [RS-4]. It is executed once at startup, after the
  renderer and the game library are loaded (so `_preset` exists) and before the device is created [RS-4].
  Its first line therefore runs `rspec_extreme.ltx`, and its later lines then set the stored values over
  the preset's [RS-4]; with the shipped file the values in force at startup are the stored ones (e.g.
  `r__supersample 2`, `r__tf_aniso 16`, `texture_lod 0`, `ph_iterations 38`) [RS-4].
- The presets also set seven names that no shipped binary contains: `r__dtex_range`,
  `r__ssa_glod_start`, `r__ssa_glod_end`, `rs_detail`, `r__wallmark_ttl`, `rs_skeleton_update`,
  `vid_bpp` [RS-2, RS-3]. `minimum` and `extreme` set all seven, the other four files only
  `r__wallmark_ttl` and `rs_skeleton_update` [RS-3]. Executing such a line only logs
  `! Unknown command: <name>` [RS-2]; they have no effect, and `user.ltx` stores none of them [RS-4].
- `ph_iterations` (all presets 18, `extreme` 38, shipped `user.ltx` 38) [RS-3, RS-4] passes its value to
  the physics solver at once [RS-1]; CF 6.1. It does not affect drawing.

### 2.11 Statistics overlay

`rs_stats` on makes the engine draw its statistics overlay every frame [RS-11]; off in the shipped
`user.ltx` [RS-4]. Its content is **unestablished**.

## 3. Proposed split

Ordered by what a player of the first level, `levels\l01_escape\`, with the shipped settings would
notice first. Each item names what it covers and what it builds on.

| # | Proposed specification | Covers | Builds on | Notes |
|---|---|---|---|---|
| 1 | `display-gamma` (behaviour) | the gamma ramp of SPIC 4.1 applied to the presented image; when it is set (each change, device creation, end of loading); behaviour across a video restart and in a window | SPIC 4.1, OS 6.1 | affects every frame; the shipped values give the identity (SPIC 4.1) |
| 2 | `level-details` (format) | the content of `level.details` (models, placement grid, slots) | CH, LL | grass covers much of the first level |
| 3 | `detail-objects` (behaviour) | drawing detail objects: slot building and rebuilding, placement, `r__detail_density`, distance, shaders | 2, SS | |
| 4 | `frame-presentation` (behaviour) | the off-screen target with `r__supersample`, its presentation quad (`postprocess`, `c_brightness`, `r1_pps_u`, `r1_pps_v`), when the target is used | SS 10.2 | the shipped `user.ltx` has `r__supersample 2`, so the default picture goes through it; the game's post-process effects need their own scope |
| 5 | `texture-quality` (behaviour) | `texture_lod`: levels dropped (table of 2.5), the list match, files with few levels, when loads pick up a change | SS 4.2, SPIC 6, OS 5.4 | with the shipped `texture_lod 0` nothing is dropped |
| 6 | `texture-filtering-settings` (behaviour) | `r__tf_aniso`, `r1_tf_mipbias`: application on set and status, at creation, the engine defaults after a video restart, interaction with shader passes | SS 4.4 | may be merged into 5 |
| 7 | `object-detail-level` (behaviour) | `r__geometry_lod` and the size thresholds for objects (MS 3; ODL 3, 4) | LV 9.4, OGF 8, MS | |
| 8 | `display-mode` (behaviour) | `rs_fullscreen` (window style), `rs_refresh_60hz`, `rs_v_sync` (no effect), 32-bit colour, `vid_mode` at creation and restart | OS 7.2, OS 9 | changes no pixel of a frame |
| 9 | `statistics-overlay` (behaviour) | `rs_stats` | | developer overlay, off by default |

Nothing else needs a specification: `rs_vis_distance`, `ai_use_torch_dynamic_lights`, `r1_ssa_lod_a`,
`r1_ssa_lod_b` (static geometry), `r1_dlights`, `r1_dlights_clip`, `r1_glows_per_frame`,
`r1_lmodel_lerp`, `r2_sun_lumscale` and `ph_iterations` are covered by the specifications named in
section 1; the seven unknown names and the other second-renderer variables have no effect (2.10, 1).
The startup order of 2.10 could be added to OS 8 or 12 instead of a new specification.

## 4. Not established

- Whether the device keeps its gamma ramp across a video restart; whether the ramp acts in a window (2.1).
- The rebuild rule of detail slots (2.4).
- The visible effect of the engine's anisotropy and bias defaults after a video restart (2.6).
- The presentation pass and when a post-process effect counts as active (2.7).
- The content of the statistics overlay (2.11).
- Nothing was executed: all timing statements come from the order of operations in the binaries.

## 5. Statements of approved specifications this contradicts

For the orchestrator; not changes this document makes:

- OS 6.2 row 11: `rs_v_sync` is "used at the next video restart". It is read by nothing that
  runs; the display always waits one refresh per frame [RS-6, RS-12].
- OS 9: the full-screen back-buffer format depends on "`vid_bpp` is 32". No variable `vid_bpp` exists; the
  depth is always 32 [RS-2, RS-12].
- SS 4.4 states the sampler values as clamp(`r__tf_aniso`, 1, 16) and `r1_tf_mipbias` without the restart
  override of 2.6 [RS-7, RS-12].

## Provenance

- Date: 2026-11-25. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; archives `gamedata.db0` … `gamedata.dbd` resolved by archive precedence with the project's archive
  tool; the shipped `__support\userdocs\user.ltx`.
- Data analysis: `config\rspec_*.ltx`, `config\external.ltx`, `user.ltx`; byte searches of all 33 shipped
  binaries for variable names.
- Binary analysis: static analysis with Ghidra 12.2 (headless, read-only) of `XR_3DA.exe`
  (variable registration, console command execution, gamma ramp, device creation and reset, startup
  order, environment blending), `xrRender_R1.dll` (variable registration, readers of each variable's
  value, sampler-state handlers, texture loading, off-screen target and presentation, detail slots) and
  `xrGame.dll` (variable registration, the options restart list); import tables and code references read
  directly from the files. No execution or debugging.
- Approved specifications of the repository read for coverage (section 1, RS-12).
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The off-screen target's presentation has a second shader, `postprocess_d` (in `shaders\r1\`), created only under a renderer condition not established here; item 4 of section 3 should cover which of the two shaders presents the frame and when.
- The game-driven post-process effects that send the frame through the off-screen target even with `r__supersample 1` (2.7) have no proposed specification or scope in section 3.
