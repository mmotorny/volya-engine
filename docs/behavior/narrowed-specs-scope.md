# Scoping: restoring the agreed scope of five narrowed behaviour specifications

A scoping specification (docs/clean-room.md, "Scope comes from the dirty side"). Five approved behaviour
specifications cover less than the issue that agreed their scope. For each, this document says what is
missing against that issue and proposes how to cover it as small, independently reviewable specifications.
It states no game behaviour of its own beyond the data facts it cites as evidence that a part exists.

| Approved specification | Issue | Follow-up already open |
|---|---|---|
| `docs/behavior/save-load-screens.md` | #74 | #118 |
| `docs/behavior/level-objects.md` | #84 | |
| `docs/behavior/offline-simulation.md` | #89 | |
| `docs/behavior/player-state.md` | #90 | |
| `docs/behavior/script-environment.md` (with `docs/behavior/script-loading.md`, #104) | #92 | #106; the script API is scoped by #119; the runtime decision is #91 [S31] |

Conventions:

- Each proposal has an id (`SL-1`, …), a working file name, and these fields: **What** (one line), **Data**
  (files and records it covers), **Depends on** (approved specifications, other proposals, issues),
  **Scripts and unbuilt systems** (what it needs that has no approved specification or no runtime yet) and
  **Order** (a priority within its group, 1 = first; section 7 gives the order across all groups that the
  dependencies require).
- A proposal that settles an item an approved specification marks unestablished, believed or not covered
  names that specification's section; the approved text stays until the new specification is approved.
- Proposals are split by data and by observable behaviour.
- Each group starts with a coverage table: every scope item of the issue, quoted verbatim, with the approved
  sections that cover it and the proposals that cover what they leave open.
- Statements an approved specification marks **believed** stay as they are. A proposal whose data and
  behaviour include such a statement confirms or corrects it and names it under **Settles** (as SL-1 does
  for sav.md 13); every other believed statement of the five specifications is collected by X-1 for
  confirmation in the running original.
- Bracketed ids are the checks that back the statement next to them: `S…` in `scope.py` (issue texts and
  approved specifications, read with `gh issue view` and from the repository), `D…` in `data.py` (game
  data, after `extract.py` has extracted `config\` and `scripts\`). Both are in the dirty workspace's
  `checks/narrowed-specs-scope/`; `run.py` runs both. Counts of script files that call an operation come
  from `calls.py`, which ignores `--` comments, function definitions and `scripts\lua_help.script`.
- Paths are paths of the combined game-data tree (`docs/formats/db-archive.md`, winning copies).

## 1. Save and load screens (#74, #118)

### 1.1 Coverage of the issue

| Scope item of #74 and #118 (quoted) [S58, S5] | Approved coverage | Proposals |
|---|---|---|
| "opening the menu during a game (Escape and its in-game item list, per docs/behavior/title-menu.md §4.1)" | save-load-screens 2 | SL-3 (the key with an in-game window open) |
| "the save-game and load-game screens (layout, list of saves, entry fields, information shown, buttons, delete, confirmations and message boxes, input)" | save-load-screens 3–7, 12 | SL-1 (picture shown), SL-3 (section 14 details) |
| "save naming" | save-load-screens 4.4, 8.1, 8.2, 9.1 | SL-3 (spaces in a typed name) |
| "quick save/load keys" | save-load-screens 9 | SL-2 (when the keys are blocked) |
| "the last-save behaviour (title-menu §4.1)" | save-load-screens 11, sav.md 1.3 | none needed |
| #118: "save pictures, autosave timing" | save-load-screens 8.4, 10; sav.md 1.1, 13 | SL-1, SL-2 |
| #118: "details marked unestablished in the spec" | save-load-screens 14 | SL-1, SL-2, SL-3 |

### 1.2 Missing against the issue

Every element #74 lists [S1] has a section in the approved specification [S2]. Missing are the parts it
leaves open:

- the nine items of its section 14 [S3]: the consequence of the script runtime error on a stale selection,
  the order of rows with equal modification times, the rounding of a dragged thumb, the input-disable
  condition that blocks F6 and F7, whether a rewritten picture is shown fresh, whether leading or trailing
  spaces in a typed name are kept, whether the name field keeps focus when the screen is re-opened, whether
  the HUD and the save message appear in the save picture, and whether a push button's state is reset when
  its screen is re-opened;
- what an open in-game window does with the key that would open the menu [S4];
- the parts #118 names: save pictures and automatic-save timing [S5]. The specification relies on
  `docs/formats/sav.md` for both, which leaves open what suppresses a periodic automatic save, whether its
  clock is real time, and (believed) that a save without a picture leaves an older picture in place [S6].

### 1.3 Proposals

**SL-1 Save picture** (`save-picture.md`)

- What: which image a save's picture holds and how the load screen shows it.
- Data: `$game_saves$<name>.dds`; the load screen's picture element (save-load-screens section 5.2).
- Covers: whether the HUD and the save message are in the captured frame; the gamma applied; how the frame
  is reduced to 128 × 128; what happens to an older picture when a save writes none; whether a picture
  rewritten under a name already shown in the session is shown fresh (save-load-screens section 14,
  sav.md section 13).
- Depends on: `docs/formats/sav.md` (13), `docs/behavior/save-load-screens.md` (5.2, 8.4). From the HUD it
  needs only whether the HUD layer and the save message are drawn into the captured frame, which SL-1
  establishes itself; what the HUD shows is the HUD specification's (#101, not approved [S50]) and is not
  needed.
- Scripts and unbuilt systems: none to specify the capture. Measured answers (gamma, reduction filter)
  need the original running, which needs an execution environment the user has not approved; without one
  they are static or unestablished.
- Order: 1.

**SL-2 Automatic-save timing and input suppression** (`autosave-timing.md`)

- What: when a periodic automatic save happens, what postpones it, and when F6 and F7 are blocked.
- Data: `config\alife.ltx` `[alife]` (`autosave_interval`, `delay_autosave_interval`); the shipped scripts
  that mention `disable_input`: `scripts\_g.script`, `avi_start.script`, `bind_stalker.script`,
  `lua_help.script`, `xr_effects.script` [D2].
- Covers: the clock the interval is measured on, what sets and clears the suppression of sav.md 1.1, the
  retry, the effect of the menu and of pauses, and the input-disable condition of save-load-screens 9.2
  and 14.
- Depends on: `docs/formats/sav.md` (1.1), `docs/behavior/save-load-screens.md` (9.2, 10).
- Scripts and unbuilt systems: the conditions are set from scripts, through the engine API (#119); this
  specification states the engine-side rule and which script-callable entry sets it.
- Order: 1.

**SL-3 Save and load screens: remaining interaction details** (`save-load-screens-details.md`)

- What: the rest of save-load-screens section 14, and the menu key while an in-game window is open.
- Data: `config\ui\ui_mm_save_dlg.xml`, `config\ui\ui_mm_load_dlg.xml`, `scripts\ui_save_dialog.script`,
  `scripts\ui_load_dialog.script`, `scripts\ui_base_dialog.script` [S7]; the saved-games folder.
- Covers: the stale-selection error's consequence, equal-time row order, thumb-drag rounding, spaces in a
  typed name, focus and push-button state on re-opening, and the menu key while an in-game window is open.
  Of those windows (inventory, PDA, trade, dialogue, … [S46]) only the inventory screen has a specification:
  SL-3 covers the rule that decides whether the window or the menu gets the key, and the inventory screen's
  case; what each other window does with the key waits for that window's specification and is listed there
  as part of its scope.
- Depends on: `docs/behavior/save-load-screens.md`, `docs/formats/ui-layout.md`, SE-4 (what a runtime
  error in an engine-called script does), `docs/behavior/inventory-screen.md` (an in-game window).
- Scripts and unbuilt systems: the screens are built and driven by the three scripts above [S7], so the
  behaviour runs on the script runtime (#91) and the interface part of the engine API (#119).
- Order: 2 (after SE-4).

## 2. Spawned objects of the first level (#84)

### 2.1 Coverage of the issue

| Scope item of #84 (quoted) [S59] | Approved coverage | Proposals |
|---|---|---|
| "which all.spawn objects of l01_escape are present in the level on New Game" | level-objects 2 | OS-5 (smart-terrain jobs, script state at the first evaluation) |
| "online/offline switching distance as far as it decides presence at start" | level-objects 2.2–2.4; offline-simulation 6.2 | LO-0 (correction of 2.4) |
| "how each kind's visual is drawn at rest" | level-objects 3–6 | LO-3, LO-4, LO-6, LO-7 |
| "visual paths in the records and configuration" | level-objects 3.1, 3.2 | LO-3 (visual flags byte) |
| "the OGF variants they use beyond docs/formats/ogf.md" ("dynamic", …) | level-objects 4.1–4.7 | LO-3 (detail window) |
| "skinned/skeletal at bind pose or the first animation frame" | bind pose: level-objects 4.8 | first animation frame: LO-1, LO-2 |
| "transforms from position/angles" | level-objects 3.4 | none needed |
| "shading reuse from static-shading.md" | level-objects 5 | LO-4, LO-6, LO-7 |
| "hanging lamps' light only structurally" | level-objects 6 | LO-5 |
| "Characters' animation and AI are out of scope" | — | kept out: characters' motion choice (LO-2) |

### 2.2 Missing against the issue

- #84 asks for skinned and skeletal visuals "at bind pose or the first animation frame", with characters'
  animation and AI out of scope [S8]. The approved specification draws every object at bind pose and states
  that the running game shows something else at the first frame: characters, monsters and crows play
  motions from the first frame, hanging lamps play `idle` when their model has it, and objects with a
  Skeleton block may play a startup animation [S9]. Motions are data: type-3 models embed them or list
  motion files [S11], and the archive tree holds 31 distinct `.omf` paths [D1]. Nothing specifies them.
- The unestablished items of its section 8 [S10]: whether smart-terrain jobs bring characters online before
  the first frame; the effect of the visual flags byte; the detail value of skinned progressive meshes and
  the small-object test; the projected light texture and the receiver order; light-track values at the
  first frame and which dynamic lights affect which object; the remaining hanging-lamp light-flag bits and
  glow drawing details.
- Also in its section 8 [S32]: how `process_time` and `objects_per_update` limit the switch pass, and the
  exact script state at the first switch evaluation. The first is settled by offline-simulation, whose
  introduction corrects level-objects 2.4 (`process_time` limits one switch pass to about 900 s;
  `objects_per_update` limits scheduled updates, not the switch pass) [S33]; level-objects 2.4 and 8 still
  carry the uncorrected text; LO-0 corrects it. The second goes to OS-5 with the smart-terrain jobs.
- The appearance of the scripted model shaders `models\xwindows`, `models\pautina`, `models\xdistort`,
  `models\xdistortcolor`, `models\xdistortcolorl`, `models\xanomaly` and `models\xmonolith`, which
  level-objects 5.2 puts outside its scope [S34]: LO-6.
- Anomalies' particle effects (`idle_particles`, …), out of scope in level-objects 3.3 [S35]: these belong
  to the particle work, #99 (the effects present in `l01_escape` on New Game) and #116 (effects elsewhere)
  [S42], not to a proposal here.

### 2.3 Proposals

**LO-0 Correction of level-objects 2.4 and 8** (a revision of `docs/behavior/level-objects.md`)

- What: replace level-objects' statements on `process_time` and `objects_per_update` (2.4, and the item of
  section 8) with offline-simulation's [S32, S33], citing offline-simulation 6.2 and 7.1.
- Data: `config\alife.ltx` `[alife]`.
- Depends on: `docs/behavior/offline-simulation.md` (6.2, 7.1).
- Scripts and unbuilt systems: none. Reviewed like any specification; the corrected file replaces the
  approved one.
- Order: 1.

**LO-1 Skeletal motions and the pose at a moment** (`skeletal-motions.md`)

- What: decode a model's motions and compute every bone's current matrix at a given time of a motion.
- Data: motion chunks 14 and 15 and the motion-file list chunk 19 of type-3 `.ogf` models
  (level-objects 4.3); the `.omf` files [D1]; bone data chunks 13 and 16.
- Covers: motion and key formats, bone matching between motion and model, the evaluation at time *t*
  (*t* = 0 is the first frame), which feeds the render matrices of level-objects 4.8. Playback over time,
  blending and motion choice are not part of it.
- Depends on: `docs/behavior/level-objects.md` (4.3, 4.4, 4.8), `docs/formats/ogf.md`,
  `docs/formats/chunks.md`.
- Scripts and unbuilt systems: none.
- Order: 1.

**LO-2 First-frame motion of objects without AI** (`object-start-motion.md`)

- What: which motion, at which time, hanging lamps and objects with a Skeleton block show in the first frame.
- Data: the Skeleton and hanging-lamp blocks of `spawns\all.spawn` records (sav.md 7.4); the models'
  motion lists.
- Covers: lamps' `idle`, the Skeleton block's startup animation, the time each is at in the first frame.
  Characters, monsters and crows choose motions through AI and scripts, outside #84 [S8]; physics settling
  belongs to physics.
- Depends on: LO-1, `docs/behavior/level-objects.md` (4.8, 6).
- Scripts and unbuilt systems: none expected; anything found scripted is reported as such.
- Order: 2.

**LO-3 Which object meshes are drawn, and at what detail** (`object-detail.md`)

- What: the small-object test, the detail window of skinned progressive meshes, and the effect of the visual
  flags byte.
- Data: the Visual block's flags byte in `spawns\all.spawn`; chunk 6 of type-4 child meshes.
- Settles: level-objects 3.2 (visual flags), 4.9, 5.1.
- Depends on: `docs/behavior/level-objects.md`, `docs/formats/level.md` (9.4), `docs/formats/ogf.md` (6).
- Scripts and unbuilt systems: none.
- Order: 1.

**LO-4 Per-object lighting at the first frame** (`object-lighting.md`)

- What: the light inputs of each object model in the first frame.
- Data: the level's lights, the hanging lamps' lights (level-objects 6), the weather inputs.
- Settles: level-objects 5.3 (receiver order), 5.4 (light-track values at the first frame), 5.6 (which
  dynamic lights affect which object).
- Depends on: `docs/behavior/level-objects.md` (5), `docs/behavior/static-shading.md`; #116, whose scope
  includes general lamp and dynamic lighting [S30]: which dynamic lights reach which object is specified
  once, in whichever of the two is investigated first, and cited by the other.
- Scripts and unbuilt systems: none.
- Order: 3.

**LO-7 Projected light texture** (`projected-light-texture.md`)

- What: how the texture that level-objects 5.5 projects onto object models is filled, and when.
- Data: the objects and lights that contribute to it; the shaders and render states of the pass that fills
  it.
- Settles: level-objects 5.5 (content of the projected light texture) [S10].
- Depends on: `docs/behavior/level-objects.md` (5.3, 5.5), `docs/behavior/static-shading.md`; LO-4 for the
  receiver order.
- Scripts and unbuilt systems: none.
- Order: 4 (after LO-4).

**LO-5 Hanging-lamp flags and glows** (`hanging-lamp-details.md`)

- What: the remaining bits of a hanging lamp's light flags and how a lamp's glow is drawn.
- Data: the hanging-lamp block of `spawns\all.spawn` (sav.md 7.4.2); glow textures.
- Settles: level-objects 6 (flag bits with unestablished meaning, glow drawing).
- Depends on: `docs/behavior/level-objects.md` (6), `docs/formats/level.md` (8, level glows).
- Scripts and unbuilt systems: none. The colour animators (`lanims.xr`) animate over time and stay with
  the particle and lighting work (#116).
- Order: 2.

**LO-6 Scripted model shaders with distortion and blending** (`model-effect-shaders.md`)

- What: how object meshes using the seven scripted model shaders of level-objects 5.2 [S34] are drawn.
- Data: the passes of `shaders\r1\models_*.s` for those names; the textures and render states they use; the
  object models whose child meshes name them.
- Settles: level-objects 5.2 ("their appearance is outside this specification").
- Depends on: `docs/behavior/level-objects.md` (5), `docs/behavior/static-shading.md` (3.4, render-state
  vocabulary, dynamic light passes).
- Scripts and unbuilt systems: the shader scripts only (static-shading 3.4 already loads them); distortion
  needs the frame behind the object, which the renderer has to provide.
- Order: 2.

Whether smart-terrain jobs bring characters online before the first frame, and the script state at the first
switch evaluation [S32], are part of OS-5.

## 3. Offline simulation (#89)

### 3.1 Coverage of the issue

| Scope item of #89 (quoted) [S60] | Approved coverage | Proposals |
|---|---|---|
| "`game.graph` in full" | offline-simulation 1 | OS-1 |
| "graph vertices/edges/levels" | offline-simulation 1.2–1.6 | OS-1 (level points) |
| "each object's offline location and path progress" | offline-simulation 5; path progress is not saved [S63] | OS-2 (open cases, `level.ai` vertex bytes), OS-4 (group kinds) |
| "online/offline switching rules and distances" | offline-simulation 6.2–6.4; level-objects 2.2–2.5 | OS-5 (job-online values) |
| "time-driven offline movement" | offline-simulation 7 | OS-2, OS-4, OS-5 |
| "registration order" | offline-simulation 8 | OS-3 (supplies, random generator, ids) |
| "current level selection on load" | offline-simulation 6.1 [S16] | none needed |
| "the offline simulation as far as a save holds and needs it" ("docs/formats/sav.md §14 item 2") | the rows above | X-1 (measured confirmation) |

### 3.2 Missing against the issue

#89 asks for `game.graph` in full, each object's offline location and path progress, time-driven offline
movement, registration order and the current level on load [S12]. The current level is specified [S16].
Missing [S13]:

- `game.graph` in full: its trailing 116 bytes and the choice of a vertex's level points;
- offline location: the choice of the level vertex among stacked vertices and the fallback when none lies
  at the position; the 16 non-position bytes of a `level.ai` vertex, among them links to neighbouring
  vertices [S36], which the fallback may need; the location update of the two group kinds, not covered
  because neither occurs in `spawns\all.spawn` [S37];
- time-driven movement: a stalker or monster without a smart terrain does nothing more in its scheduled
  update [S14], and the smart-terrain choice, task and jobs are left to the scripts; tasks naming unusable
  paths are unestablished [S15]; the path search's open cases;
- registration order at a New Game: the supply rules and the random generator, hence most New Game ids;
- the scheduled updates of every other kind;
- measured confirmation: everything in the specification is static.

### 3.3 Proposals

**OS-1 Rest of `game.graph`** (`game-graph-rest.md`)

- What: the trailing bytes of `game.graph` and how a vertex's level points are chosen.
- Data: `game.graph`.
- Settles: offline-simulation 1.5, 1.7.
- Depends on: `docs/behavior/offline-simulation.md` (1).
- Scripts and unbuilt systems: none.
- Order: 1.

**OS-2 Offline location and path search: open cases** (`offline-path-cases.md`)

- What: the open cases of locating an offline object on the level graph and of its path search.
- Data: `levels\<level>\level.ai`, `game.graph`, `levels\<level>\level.gct`, the patrol paths of
  `spawns\all.spawn`.
- Settles: offline-simulation 3 (the 16 non-position bytes of a level vertex, as far as locating and the
  path search use them [S36]), 5.2 (stacked vertices, off-grid fallback), 7.3 (tasks naming a missing path
  or a point with an invalid game vertex), 7.4 (terrain masks when the start vertex matches none,
  accessibility between games, ties).
- Depends on: `docs/behavior/offline-simulation.md` (3–5, 7).
- Scripts and unbuilt systems: the task comes from a smart terrain (OS-5); the rule for an unusable task is
  on the engine side.
- Order: 2.

**OS-3 New Game creation: supplies, random generator and ids** (`new-game-supplies.md`)

- What: which items the simulation creates for characters at a New Game, from which random draws, and the
  object ids everything receives.
- Data: the custom data of `spawns\all.spawn` records; `config\gameplay\character_desc_*.xml` and the
  profile files; the item sections they name.
- Settles: offline-simulation 8.1 (supply rules, random generator, New Game ids) and player-state 3.1 (ids
  of created items) [S20].
- Depends on: `docs/behavior/offline-simulation.md` (8), `docs/behavior/player-state.md` (3.1),
  `docs/formats/sav.md` (7). #106 covers the scripts' `math.random` [S28]; this specification says whether
  the simulation draws from the same generator.
- Scripts and unbuilt systems: objects that scripts create at the start go through the engine API (#119).
- Order: 2.

**OS-4 Offline updates of the other kinds** (`offline-updates-other.md`)

- What: which kinds other than stalkers and monsters have scheduled offline updates, and what those change
  as far as a save holds it.
- Data: `spawns\all.spawn` records of those kinds; their configuration, e.g. the 66 sections with an
  `artefacts` key, 61 of them with a zone class id (`Z…`) [D9]; `docs/formats/sav.md` 7.4.
- Settles: offline-simulation 7.1 ("not covered here") and 5.1 (the location update of the two group
  kinds [S37]; whether any shipped data or script creates them is stated with a check, and their rule is
  specified either way).
- Depends on: `docs/behavior/offline-simulation.md` (7.1), `docs/formats/sav.md` (7.4, 7.5).
- Scripts and unbuilt systems: scripted kinds (`scripts\se_*.script`) define part of their updates; those
  parts run on the script runtime (#91) and engine API (#119).
- Order: 3.

**OS-5 Smart terrains in the offline simulation** (`smart-terrains.md`)

- What: how a character comes to belong to a smart terrain, the task it receives, and the job-online value
  that decides whether it is online.
- Data: smart-terrain records of `spawns\all.spawn` and their custom data; `scripts\smart_terrain.script`,
  `scripts\xr_gulag.script` and the 18 `scripts\gulag_*.script` files [D12].
- Settles: offline-simulation 7.2 (acceptance, suitability, joining), level-objects 2.5 (whether jobs bring
  characters online before the first frame) [S10].
- Depends on: OS-2, `docs/behavior/script-loading.md`, #119.
- Scripts and unbuilt systems: the logic is defined by the shipped scripts. If they run on the chosen
  runtime (#91), this specification covers the engine side they call (handed to #119's split) and
  verification facts: which characters of `l01_escape` join which smart terrain at a New Game, and which are
  online in the first frame.
- Order: 4.

Measured confirmation of offline-simulation (everything in it is static [S13]) goes to X-1.

## 4. Player character state (#90)

### 4.1 Coverage of the issue

| Scope item of #90 (quoted) [S61] | Approved coverage | Proposals |
|---|---|---|
| "the player character's state on New Game and how it changes in play, as far as saves hold it" | New Game: player-state 2, 3 | in play: the rows below; changes made by scripts: PS-17 |
| "health and conditions (bleeding, radiation, satiety, stamina/power)" | player-state 2.3, 4, 5 | PS-1 (hits, wounds, bleeding, psy health), PS-2 (falls, zones), PS-3 (belt), PS-4 (death), PS-5 (sleep), PS-11 (psy screen effect, threshold callbacks, saved condition flags), PS-10 |
| "money" | player-state 2.2 | PS-9 (trading), PS-17 (scripts) |
| "profile and community" | player-state 2.2 | PS-12 |
| "inventory contents and placement" | player-state 3; inventory-screen | PS-8 (taking from and dropping into the level), PS-9, PS-15 (addons, unloading), PS-17 |
| "active slot and weapon" | player-state 3.3, 6 | PS-6 (slot changes, quick use), PS-7, PS-14, PS-15 |
| "items' condition" | player-state 3.1 | PS-1 (outfit), PS-7 (weapons), PS-16 (torch and night vision) |
| "docs/formats/sav.md §14 items 5 and 8" [S55]: "movement and physics state" | player-movement; the player update's direction and physics fields are unestablished [S57] | PS-13 |
| sav.md §14 item 8: "condition, ammunition, addons, inventory placement" | player-state 3 | PS-7, PS-15 |

### 4.2 Missing against the issue

#90 asks for the state at a New Game and "how it changes in play" for health and conditions including
bleeding and radiation, money, inventory contents and placement, active slot and weapon, and items'
condition [S17]. The approved specification limits itself to what "a first implementation needs" [S18]
and puts out of scope [S19]: hits and wounds, immunities and outfit protections, death, artefacts on the
belt, sleeping and the `*_sleep` rates, psy effects, tutorial hint callbacks, trading and dialogs that give
or take items and money, rank changes, scripted invulnerability, and the interface showing these values
[S41]. The interface belongs to the HUD (#101, #115) and to `docs/behavior/inventory-screen.md`, not to a
proposal here. Also missing:

- changes of the active slot and of the weapon in play (magazine, ammunition type, fire mode, grenade
  launcher, zoom, addons), and items' condition other than eating; the torch and night vision, whose state is
  saved (torch byte, sav.md 7.4.2 [S52]);
- the `u16` flags of the player condition block (tutorial hints already shown, over-weight state), saved
  state that player-state 6 leaves out of scope [S51];
- the changes caused by scripts, out of scope in its introduction [S56]: the shipped scripts call
  operations that move items (`transfer_item`), drop items (`drop_item`), move money, create items in the
  player's inventory, hit or kill the player character, and read its health [D20, D21];
- the direction and physics fields of the player update (sav.md 14 item 5, "movement and physics state";
  unestablished in sav.md 16 [S55, S57]);
- items entering the inventory from the level; dropped items' placement in the world is dependent in the
  inventory screen [S21];
- the unestablished items of its section 8 [S20].

### 4.3 Proposals

**PS-1 Hits on the player character** (`player-hits.md`)

- What: what a hit of a given type and power does to the player character: immunities, the worn outfit's
  protection and condition loss, health, power, radiation and psy-health changes, wounds, bleeding and their
  healing, and what sets and clears the "can be harmed" flag of player-state 4.2 [S49].
- Data: `config\creatures\actor.ltx` (`[actor]` `immunities_sect` = `actor_immunities_gd_master` [D4]) and
  the immunity sections; outfit sections; the wound records of the client data (sav.md 8).
- Settles: player-state 8 (wound records, bleeding amounts, healing) and the "hits and wounds",
  "immunities and outfit protections" items of its section 9.
- Depends on: `docs/behavior/player-state.md` (4, 5), `docs/formats/sav.md` (8),
  `docs/behavior/inventory-screen.md` (4.8).
- Scripts and unbuilt systems: none for applying a hit; hit sources are PS-2, PS-17 (hits made by
  scripts) and later combat work.
- Order: 1.

**PS-2 Hits without AI: falls, anomalies and radiation zones** (`environment-hits.md`)

- What: when falls, anomaly zones and radiation zones hit the player character, and how hard.
- Data: zone records of `spawns\all.spawn` (shapes) and zone sections, among them the 6 of class id
  `Z_RADIO` [D5]; landing data of the movement.
- Depends on: PS-1, `docs/behavior/player-movement.md`, `docs/behavior/level-objects.md` (3.3).
- Scripts and unbuilt systems: anomaly visuals and particles are other work (#116); any zone behaviour found
  scripted is reported as such.
- Order: 2.

**PS-3 Artefacts on the belt** (`belt-artefacts.md`)

- What: how artefacts on the belt change the conditions over time and the hits taken.
- Data: artefact sections, e.g. the 31 sections with `health_restore_speed`, 28 of them `af_*` [D6].
- Depends on: PS-1, `docs/behavior/player-state.md` (3.3, 4.2), `docs/behavior/inventory-screen.md`.
- Scripts and unbuilt systems: none expected.
- Order: 2.

**PS-4 Player death** (`player-death.md`)

- What: what happens when the player character's health reaches 0: view, input, HUD, the menu offered.
- Data: `[actor]` configuration; the in-game menu lists (`docs/behavior/title-menu.md` 4.1).
- Depends on: PS-1, `docs/behavior/title-menu.md`, `docs/behavior/save-load-screens.md` (2).
- Scripts and unbuilt systems: script reactions to the death go through #119.
- Order: 3.

**PS-5 Sleeping** (`sleep.md`)

- What: how sleep starts and ends, how game time passes, and the `*_sleep` rates that replace the waking
  rates.
- Data: the 11 `*_sleep` keys of `[actor_condition]` and `[actor]` `max_sleep_hours` = 12 [D7];
  `scripts\dream.script`, which has a section of sleep callbacks [D11].
- Settles: the "sleeping and the `*_sleep` rates" item of player-state 9.
- Depends on: `docs/behavior/player-state.md` (4.2).
- Scripts and unbuilt systems: the scripts take part (#91, #119); whether the shipped game offers sleep to
  the player is to be established.
- Order: 4.

**PS-6 The active slot, the item in the hands and quick use** (`active-slot.md`)

- What: how the active slot changes in play, what the player holds, and the quick-use keys that use a
  medkit or bandage without the inventory screen (left out of player-state 5 as interface [S44]).
- Data: the actions `wpn_1` … `wpn_6` ("Оружие 1" … "Оружие 6", default keys 1–6), `next_slot` and
  `prev_slot` ("Оружие из следующего слота", "… предыдущего слота"; listed on the key-binding screen, unbound
  by default), `use_medkit` and `use_bandage` ("Быстрое использ. аптечки", "… бинта"; `[` and `]`)
  [D10, D18, D19]: meanings from `config\ui\ui_keybinding.xml` and `config\text\rus\ui_st_keybinding.xml`,
  keys from `config\default_controls.ltx`; slot keys of item sections.
- Depends on: `docs/behavior/player-state.md` (3.3, 6), `docs/behavior/inventory-screen.md` (5.4),
  `docs/formats/sav.md` (8, the active slot).
- Scripts and unbuilt systems: the first-person view of the held item (models, animations) is other work;
  this specification covers the state.
- Order: 1.

**PS-7 Weapon ammunition and firing state** (`weapon-state.md`)

- What: magazine and ammunition, reloading from the inventory, changing the ammunition type, fire modes,
  and the condition a weapon loses per shot, as far as a save holds it.
- Data: weapon sections, among them the 100 with `condition_shot_dec` [D8]; ammunition sections; the
  actions `wpn_reload` ("Перезарядить", R), `wpn_next` ("Сменить тип патронов", Y), `wpn_firemode_prev`
  and `wpn_firemode_next` ("Режим стрельбы пред./след.", 9 and 0) [D10, D18]; the weapon update fields
  (ammunition in the magazine, ammunition type, weapon state; player-state 3.1 [S53], sav.md 7.4).
- Depends on: PS-6, `docs/formats/sav.md` (7.4).
- Scripts and unbuilt systems: what a shot does in the world (bullets, hits on others) is combat, not part
  of it.
- Order: 2.

**PS-14 Weapon modes: grenade launcher and zoom** (`weapon-modes.md`)

- What: switching a weapon's grenade launcher on and off and zooming, as far as the weapon's saved state
  (weapon flags, zoom byte) changes.
- Data: the actions `wpn_func` ("Подствольник", V) and `wpn_zoom` ("Приблизить", right mouse button)
  [D18, D19]; weapon sections with a grenade launcher; the weapon update's zoom byte [S53].
- Settles: player-state 8 (the zoom byte of a freshly created item) [S20].
- Depends on: PS-7.
- Scripts and unbuilt systems: the zoomed view itself (field of view, scope picture) is HUD and camera work.
- Order: 3.

**PS-15 Weapon addons and unloading** (`weapon-addons.md`)

- What: which weapons take which addons, attaching and detaching them, unloading a magazine, and the addon
  and ammunition items these create.
- Data: weapon sections (addon keys) and addon item sections; the inventory-screen actions attach, detach
  and unload (inventory-screen 5.6 entries 5–10″, 5.8); the weapon flags and addons fields of the weapon
  update [S53].
- Settles: inventory-screen 5.8 and 10 (addons, magazines and ammunition, **dependent** (weapons) [S54]).
- Depends on: PS-7, `docs/behavior/inventory-screen.md` (5.6, 5.8).
- Scripts and unbuilt systems: none expected.
- Order: 3.

**PS-16 Torch and night vision** (`torch.md`)

- What: switching the torch and night vision on and off, and what each does to the saved torch byte.
- Data: the actions `torch` ("Фонарик", L) and `night_vision` ("Прибор ночного видения", N) [D18, D19]; the
  torch item `device_torch` and its section; the torch byte (bit 0 on, bit 1 night vision on, bit 2
  unestablished) [S52].
- Settles: sav.md 16 (torch bit 2).
- Depends on: `docs/behavior/player-state.md` (3.1), `docs/formats/sav.md` (7.4.2).
- Scripts and unbuilt systems: the torch's light and the night-vision picture are lighting and
  post-process work; this specification covers the state and the keys.
- Order: 2.

**PS-8 Taking items from the level** (`item-pickup.md`)

- What: which level objects the player can take, how (the `use` key, F [D10]), and where they go; where a
  dropped item ends up.
- Data: item records of `spawns\all.spawn`; item sections.
- Settles: the dependent part of inventory-screen 5.7 [S21].
- Depends on: `docs/behavior/player-state.md` (3.3), `docs/behavior/level-objects.md`,
  `docs/behavior/inventory-screen.md` (5.7).
- Scripts and unbuilt systems: a dropped item's fall needs physics, which has no specification.
- Order: 2.

**PS-9 Money and items from trading** (`trade-transfers.md`)

- What: how a trade changes the player's money and inventory: prices and what can be bought and sold.
- Data: the 6 `config\misc\trade_*.ltx` files [D13]; item sections (cost).
- Depends on: PS-6, `docs/behavior/inventory-screen.md` (5.9), and the trade screen, which has no
  specification, issue or proposal yet (listed in "Gaps found in review").
- Scripts and unbuilt systems: the trade screen is its own interface work.
- Order: 5.

**PS-17 Changes to the player's state made by scripts** (`player-script-changes.md`)

- What: the effect on the player's saved state of the changes the shipped scripts make (dialog actions,
  scripted scenes, tasks): items moved to or from the player, items dropped, items created in the player's
  inventory, money moved, hits on the player character, and reads of the condition the scripts act on.
- Data: the 17 `config\gameplay\dialogs*.xml` files [D13]; the script files that call these operations
  (calls only: comments, function definitions and the text listing `scripts\lua_help.script` excluded):
  item moves in 3 files (`agroprom_dialog.script`, `dialogs.script`, `xr_effects.script`), drops in 1
  (`xr_effects.script`), money moves directly in 2 (`agroprom_dialog.script`, `dialogs.script`) and through
  `relocate_money` in 9, item creation with the player as parent in 24 [D20]; hits on the player character
  in 6, kills in 2 (`sr_psy_antenna.script`, `xr_effects.script`), reads of its health in 3 [D21].
- Settles: the "scripts" and "dialogs" parts of player-state's out-of-scope statement [S56].
- Depends on: PS-1 (hits), PS-6, `docs/behavior/player-state.md` (3.3, 6), OS-3 (ids of created items).
- Scripts and unbuilt systems: every change starts in a script (#91); the entry points the scripts call are
  handed to #119's split; PS-17 states only what each does to the player's state.
- Order: 4.

**PS-13 Player update: direction and physics fields** (`player-update-physics.md`)

- What: the compressed direction and the physics fields of the player character's update record and how
  they follow the movement in play.
- Data: the player update of `docs/formats/sav.md` (7.4.2); the movement state of player-movement.
- Settles: sav.md 16 (compressed-direction encoding, physics fields of the player update) [S57], the
  "movement and physics state" of sav.md 14 item 5 [S55].
- Depends on: `docs/behavior/player-movement.md`, `docs/formats/sav.md` (7.4.2).
- Scripts and unbuilt systems: physics has no specification; fields that need it are marked as such.
- Order: 2.

**PS-10 Player state: remaining New Game details** (`player-state-details.md`)

- What: the remaining unestablished items of player-state 8 [S20], other than ids (OS-3) and wounds (PS-1).
- Data: as player-state.
- Covers: the update interval, the simulation state tested before the player gets id 0, the order of the
  player's children and the bolt, the meaning of a community index, the radiation field of the player
  update, the physics byte of created items (their zoom byte is PS-14's), the further belt test, and whether the bobbing
  table's limping row (player-movement 9.1) uses the limping state of player-state 4.3 [S20].
- Depends on: `docs/behavior/player-state.md`.
- Scripts and unbuilt systems: none.
- Order: 1.

**PS-11 Psy-health screen effect and tutorial threshold callbacks** (`player-condition-signals.md`)

- What: the screen effect shown while psy health is below 1, the callbacks the engine makes to scripts
  when conditions cross `[tutorial_conditions_thresholds]` [S48], and the saved `u16` flags of the player
  condition block (tutorial hints already shown, over-weight state) [S51].
- Data: the configuration section `[tutorial_conditions_thresholds]` [S48]; the configuration the psy
  screen effect reads (to be identified by the investigation).
- Settles: the "psy effects" and "tutorial hint callbacks" items of player-state 9 [S48]; the flags of
  player-state 6 [S51].
- Depends on: `docs/behavior/player-state.md` (2.3, 4.2), PS-1 (psy hits).
- Scripts and unbuilt systems: the callbacks reach the shipped scripts (#91); the callback interface itself
  is engine API and is handed to #119's split.
- Order: 3.

**PS-12 Rank, reputation and community changes** (`player-standing-changes.md`)

- What: how the player character's rank, reputation and community change in play, and what such a change
  alters, as far as a save holds it.
- Data: the Trader-like block of the player's record (player-state 6); the shipped scripts that call the
  setters (calls only, as in PS-17): rank in `_g.script`, `task_manager.script`, `xr_statistic.script`
  [D15]; community in `dialogs.script`, `dialogs_military.script` [D16]; reputation in `dialogs.script`,
  `task_manager.script` [D17].
- Settles: the "rank changes" item of player-state 9 [S19], and the "profile and community" part of #90
  in play [S17].
- Depends on: `docs/behavior/player-state.md` (2.2, 6), `docs/formats/sav.md` (7.4.2).
- Scripts and unbuilt systems: the changes the scripts make go through engine API entries (#119); PS-12
  states their effect on the player's state and whether the engine changes any of these on its own.
- Order: 3.

## 5. Script environment (#92, #104)

### 5.1 Coverage of the issue

| Scope item of #92 and #104 (quoted) [S62, S40] | Approved coverage | Proposals |
|---|---|---|
| "the Lua implementation and version the original embeds" | script-environment 1 | SE-3, SE-7, SE-8, SE-10, SE-11, SE-13 (differences from the public release) |
| "evidenced from the shipped binaries (version strings, library identification)" | script-environment 1.1–1.3 | SE-9 (binding library version) |
| "the libraries and extensions available to scripts (standard libraries, bit operations, JIT-specific facilities)" | script-environment 2.1–2.4 | SE-1 (fields), SE-7 (behaviour), SE-12 (observable effect of the JIT); bit operations of the engine: #119 |
| "number type" | script-environment 2.5 | SE-10 (conversions) |
| "how script files are found, loaded and namespaced" | script-loading 2; static-shading 3.4 | SE-5 |
| "which language features and libraries the shipped scripts actually use" | — | SE-2 |
| "Decides the runtime for fidelity" | decided on #91 [S39] | SE-2, SE-3, SE-7, SE-8, SE-10 – SE-13 confirm it |
| #104: "which scripts the game loads" | script-loading 3 (mention-based set) | SE-6 |

### 5.2 Missing against the issue

#92 asks for the runtime, its libraries and extensions, the number type, how script files are found, loaded
and namespaced, and "which language features and libraries the shipped scripts actually use", to decide the
runtime for fidelity [S22]. `script-environment.md` covers the runtime, libraries, JIT, bit operations, ffi
and number type and states "Nothing else is covered." [S23]; `script-loading.md` covers the game state's
loading and namespaces. Missing:

- which language features and libraries the shipped scripts use; for example `coroutine.` occurs in
  `scripts\_g.script` and in `scripts\.script` [D3], a file the game never loads [S43];
- the fields the libraries set outside their name tables [S23];
- the version of the C++ binding library the runtime exports (script-environment 1.2, unestablished
  [S38]), which decides how the binding's class system behaves for scripts: SE-9;
- whether code is compiled to machine code at run time (script-environment 2.2, unestablished [S38]); the
  user's runtime decision on #91 is the public release in interpreter mode [S39]: SE-12 states whether
  compilation changes any observable result;
- whether the embedded runtime differs from the public release anywhere else (script-environment 1.4
  [S47]): the libraries (SE-7) and the runtime core, by component (SE-8, SE-10, SE-11, SE-13);
- whether the embedded compiler differs from the public release beyond the lexical analyzer [S24]: SE-3;
- the open items of script-loading 5 [S24]: bytes 80–FF in names, the game-type `script` lists and the
  `levels` key, engine-called names other than `class_registrators`, name-length limits, and which scripts a
  session loads; and how the shader state names and prefixes its scripts [S25], beyond the module naming
  that static-shading 3.4 gives [S26];
- the random generator behind `math.random`: #106 [S28]; bit operations the engine itself provides: the
  engine API, #119 [S23, S29].

script-loading 2.4 covers errors when a file is loaded and run [S27], not errors in script functions the
engine calls later; save-load-screens 14 needs the latter [S3].

### 5.3 Proposals

**SE-1 Library fields outside the name tables** (`lua-library-fields.md`)

- What: the values the opened libraries set besides their name tables (constants, metatables, fields of the
  globals).
- Data: `bin\xrLUA.dll`; the public LuaJIT 1.0.3 source release as comparison material [S45] (a
  third-party library; script-environment 1.4 notes none had been compared [S47]).
- Settles: script-environment 2.1 ("not listed here") [S23].
- Depends on: `docs/behavior/script-environment.md`.
- Scripts and unbuilt systems: none.
- Order: 1.

**SE-2 What the shipped scripts use** (`script-usage.md`)

- What: the language features, standard-library functions and globals the loadable scripts use, and the
  globals no script defines (the list handed to #119 as the engine's script interface).
- Data: `scripts\*.script`, the loadable set of script-loading 3.
- Also: whether the shipped scripts yield across calls into the engine (the coroutine facility the `coco`
  library adds), which #91 leaves pending for the runtime work [S39].
- Depends on: `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md`.
- Scripts and unbuilt systems: measured from the script files; nothing run.
- Order: 1.

**SE-3 The embedded compiler beyond the lexical analyzer** (`lua-compiler-diff.md`)

- What: whether the embedded parser and code generator accept or compile anything differently from the
  public release.
- Data: `bin\xrLUA.dll`; the public LuaJIT 1.0.3 source release as comparison material [S45]; the
  loadable scripts as test input.
- Settles: script-loading 5, first item.
- Depends on: `docs/behavior/script-loading.md` (1).
- Scripts and unbuilt systems: none.
- Order: 2.

**SE-4 Runtime errors in engine-called script functions** (`script-runtime-errors.md`)

- What: what the player sees and what the game does when a script function called by the engine (interface
  handlers, object callbacks) raises an error.
- Data: `bin\xrGame.dll`; the script functions named in configuration and layout.
- Needed by: SL-3. #92 does not list it; it is proposed here because SL-3 cannot be completed without it,
  and needs the user's agreement as an addition.
- Depends on: `docs/behavior/script-loading.md` (2.4).
- Scripts and unbuilt systems: none to specify; observing it needs the original running (not approved).
- Order: 2.

**SE-5 Script loading: remaining details** (`script-loading-details.md`)

- What: the open items of script-loading 5 and how the shader state builds the chunk text of its scripts.
- Data: `config\script.ltx` (game-type `script` lists, `levels`), `bin\xrGame.dll`, `bin\XR_3DA.exe`,
  `shaders\r1\*.s`.
- Settles: script-loading 4.4 and 5 (except the item SE-3 takes and "which scripts a session loads",
  SE-6).
- Depends on: `docs/behavior/script-loading.md`, `docs/behavior/static-shading.md` (3.4).
- Scripts and unbuilt systems: none.
- Order: 2.

**SE-6 Which scripts the game loads** (`scripts-loaded.md`)

- What: which script files the game loads, which #104 asks for [S40]: statically, the files reached on the
  path from startup through a New Game on `l01_escape` to the first frame (startup, class registrators, the
  object kinds and binders of the level's records, the interface opened), narrower than script-loading 3's
  mention-based set; beyond that bound, unestablished until X-1 measures it.
- Data: `config\script.ltx`, `spawns\all.spawn` records of `l01_escape` (their sections and custom data),
  the configuration and layout files naming script functions, `scripts\*.script`.
- Settles: script-loading 5 ("which of the reachable scripts are actually loaded in a given play session")
  as far as static reading allows.
- Depends on: `docs/behavior/script-loading.md` (2, 3), `docs/behavior/level-objects.md` (2).
- Scripts and unbuilt systems: none to specify; the measured set needs X-1.
- Order: 2.

The random generator is #106 and needs no new proposal.

**SE-7 Library functions against the public release** (`lua-library-diff.md`)

- What: whether any function of the opened libraries behaves differently from the same function of the
  public release.
- Data: `bin\xrLUA.dll`; the public LuaJIT 1.0.3 source release [S45].
- Settles: script-environment 1.4 for the libraries [S47].
- Depends on: `docs/behavior/script-environment.md` (2.1), SE-1.
- Scripts and unbuilt systems: none.
- Order: 2.

Script-environment 1.4 [S47] is settled as a whole when script-loading 1 (lexer), SE-3 (parser and code
generator), SE-7 (libraries), SE-8 (interpreter and metamethods), SE-10 (conversions), SE-11 (garbage
collection) and SE-13 (exported C API) are approved; each compares one component of `bin\xrLUA.dll` with
the public LuaJIT 1.0.3 source release [S45]. SE-12 covers the JIT compiler.

**SE-8 Interpreter and metamethods against the public release** (`lua-vm-diff.md`)

- What: whether the embedded virtual machine executes the operations and calls the metamethods as the
  public release does.
- Data: the interpreter and metamethod dispatch of `bin\xrLUA.dll`; the public release [S45].
- Depends on: SE-3. Scripts and unbuilt systems: none. Order: 3.

**SE-10 Number and string conversions against the public release** (`lua-conversions-diff.md`)

- What: whether conversions between numbers and strings in the runtime core (coercion in arithmetic and
  concatenation, number formatting, parsing of numerals) match the public release.
- Data: `bin\xrLUA.dll` (including the format string of script-environment 2.5); the public release [S45].
- Depends on: `docs/behavior/script-environment.md` (2.5). Scripts and unbuilt systems: none. Order: 2.

**SE-11 Garbage collection against the public release** (`lua-gc-diff.md`)

- What: whether the collector's observable behaviour (when finalizers and weak tables act, the effect of
  `collectgarbage` and `gcinfo` arguments) matches the public release.
- Data: `bin\xrLUA.dll`; the public release [S45].
- Depends on: SE-1. Scripts and unbuilt systems: none. Order: 2.

**SE-12 Observable effect of the JIT compiler** (`lua-jit-effect.md`)

- What: whether machine-code compilation changes any result a script can observe, given the user's
  decision on #91 to run the public release in interpreter mode [S39].
- Data: the JIT parts of `bin\xrLUA.dll`; the public release [S45].
- Settles: script-environment 2.2 (whether code is compiled) [S38].
- Depends on: `docs/behavior/script-environment.md` (2.2). Scripts and unbuilt systems: none. Order: 3.

**SE-13 The exported C API against the public release** (`lua-c-api-diff.md`)

- What: whether the C API functions the engine imports behave as the public release's, and what the
  non-public exports do.
- Data: the export table of `bin\xrLUA.dll` and the import tables of `bin\xrGame.dll` and `bin\XR_3DA.exe`
  (script-environment 1.2, 1.3); the public release [S45].
- Depends on: `docs/behavior/script-environment.md` (1.2, 1.3). Scripts and unbuilt systems: none.
  Order: 2.

**SE-9 The binding library's class system as scripts see it** (`script-classes.md`)

- What: how scripts declare classes, inherit (from script classes and from engine classes), construct
  objects and use properties through the C++ binding library the runtime exports, and the library's version
  if one can be identified (script-environment 1.2, unestablished [S38]).
- Data: `bin\xrLUA.dll` (its binding-library exports); the 191 script files other than `scripts\.script` (never
  loaded [S43]) with a class declaration line (`class "…"`) outside comments [D14].
- Settles: script-environment 1.2 (binding library version or, failing it, its script-visible behaviour).
- Depends on: `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md`.
- Scripts and unbuilt systems: which engine classes are exposed and what their members do is #119's scope
  [S29]; SE-9 covers only the mechanism.
- Order: 1.

## 6. Measured confirmation in the running original

**X-1 Measurements in the running original** (`measured-confirmation.md`)

- What: the facts that only running the original can establish, collected so none is lost: the static
  statements of offline-simulation [S13], the gamma and reduction filter of the save picture (SL-1), the
  consequence of runtime script errors (SE-4), the scripts a session loads (SE-6), every **believed**
  statement of the five specifications that no proposal settles (conventions), and any item another
  proposal leaves unestablished for this reason.
- Data: the original game run on the user's copy.
- Depends on: an execution environment (Wine or a Windows virtual machine, docs/clean-room.md "Tooling"),
  which needs the user's approval; until then this proposal is held and tracked, not investigated.
- Order: after approval; it does not block any other proposal.

## 7. Order across all groups

Each step builds on approved specifications, on earlier steps, and on the issues named in its "Why here"
column (#91, #116, #119, which are not specifications). Proposals in one step are independent of each other.

| Step | Proposals | Why here |
|---|---|---|
| 1 | SL-1, SL-2, LO-0, LO-1, LO-3, OS-1, PS-1, PS-6, PS-10, SE-1, SE-2, SE-9 | build only on approved specifications |
| 2 | LO-2, LO-5, LO-6, SE-6, OS-2, OS-3, OS-4, PS-2, PS-3, PS-4, PS-7, PS-8, PS-13, PS-16, SE-3, SE-4, SE-5, SE-7, SE-10, SE-11, SE-13 | need step 1 (LO-2 needs LO-1; PS-2, PS-3, PS-4 need PS-1; PS-7 needs PS-6; SE-7 and SE-11 need SE-1) or are independent but lower in priority |
| 3 | SL-3, LO-4, PS-11, PS-12, PS-14, PS-15, SE-8, SE-12 | SL-3 needs SE-4; PS-11 needs PS-1; PS-14, PS-15 need PS-7; SE-8 needs SE-3; LO-4 is coordinated with #116; PS-12 and SE-12 are lower in priority |
| 4 | LO-7, OS-5, PS-5, PS-17 | LO-7 needs LO-4; OS-5, PS-5 and PS-17 also depend on the script runtime (#91) and the engine API (#119) |
| 5 | PS-9 | also needs the trade screen (no specification yet) |
| held | X-1 | needs an execution environment the user has not approved |

## Gaps found in review

Recorded from the fourth review, not yet worked into the proposals above; each needs a proposal or an
owner before this scoping is complete.

- Throwing grenades (sections of class `G_F1`, `G_RGD5`, slot 3) and the bolt in slot 5, as far as they
  change the saved inventory and the active slot (#90), are in no proposal.
- Taking items from corpses (`config\ui\carbody.xml`, `carbody_new.xml`) and from inventory boxes
  (`O_INVBOX`) is in no proposal.
- PS-17 omits script calls that change the active slot or hide the item in hand, and that move or turn
  the player (`activate_slot` in 3 files, `hide_weapon` in 4, `set_actor_position` and
  `set_actor_direction` in 4 [D22]).
- SE-2 covers only `scripts\*.script`; the shader scripts `shaders\r1\*.s`, `shaders\r2\*.s` (another
  state, another library set) and the engine globals they expect are not covered.
- SE-6 is limited to startup and the first frame of `l01_escape`; #104 needs the whole game's loaded set,
  statically, possibly split by phase, with measurement only as confirmation.
- The #89 row for sav.md §14 item 2 does not map game time and its speed (sav.md 5, 12), the spawn
  binding (sav.md 6) and the switching flags (7.4.1), nor what changes the time factor in play and the
  inverse game-time conversion (sav.md 16).
- Section 5.1 quotes only one scope item of #104; its comment questions (whether and where the original
  accepts C-style comments, inside strings, nesting, `/*` without `*/`) are not mapped to script-loading 1.
- X-1 is not one format or behaviour; it should be split per approved specification or folded into the
  proposals that own each item, held until an execution environment is approved.
- LO-4 names only #116; #99 also scopes how dynamic lights light objects; which of LO-4, #99 and #116
  specifies it is not said.
- SL-3 leaves the menu key in the PDA, trade and dialogue windows to specifications that do not exist and
  are not tracked.
- The trade screen that PS-9 needs has no specification, issue or proposal.
- Switching changed by scripts in play (calls of `set_switch_online`, `set_switch_offline`,
  `switch_online`, `switch_offline` in shipped scripts, part of #89's switching rules) is in no proposal;
  OS-5 covers only job-online values.
- Physics has no specification, issue or proposal, yet LO-2 (settling at the first frame), PS-8 (a
  dropped item's fall) and PS-13 (physics fields of the player update) defer to it.
- PS-17's file counts find only calls on a receiver named after the player; calls on an object passed in
  as an argument that may be the player (e.g. scripted hit effects) are not examined.

## Provenance

- Date: 2026-11-24 (revised three times the same day after review).
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`.
- Material examined: issues #74, #84, #89, #90, #91 (with comments), #92, #99, #104, #106, #115, #116, #118, #119 (read-only); the
  approved specifications named above and those they build on; the `config\` and `scripts\` trees and the
  archive listing.
- Analysis: comparison of each issue's text with the approved specification's coverage, unestablished and
  out-of-scope sections; data counts read by a throwaway LTX reader and text search over extracted files
  (checks `scope.py` and `data.py`). No binary was analysed or run for this document.
- No leaked source code, derived projects or third-party documentation of the engine were used.
