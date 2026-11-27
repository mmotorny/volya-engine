# Miscellaneous settings commands

What the original does with eight console variables whose effect no other specification covers:
`g_autopickup`, `g_corpsenum`, `g_eventdelay`, `cl_cod_pickup_mode`, `cam_slide_inert`,
`snd_acceleration`, the action `check_for_updates`, and `r2em`. Their kind, range, built-in value, status,
help text and saving rule are those of `docs/behavior/console-commands.md` (CC) section 8 and are not
repeated; this specification states only what changing them does, with the single-player game first.
What could not be established is marked **unestablished**; a statement inferred without a direct check is
marked **believed**, with why.

Conventions:

- "The engine" is `bin\XR_3DA.exe`, "the game library" `bin\xrGame.dll`, "the sound library"
  `bin\xrSound.dll`, "the renderer" the loaded renderer library (`xrRender_R1.dll`, or `xrRender_R2.dll`),
  all of the shipped Russian edition (GOG build).
- The **actor** is the player's character; the **controlled object** is the object the level currently
  takes the player's input for (normally the actor).
- The **game type** is the kind of game the level runs; type 1 is the single-player game (the command
  `g_game_difficulty` acts only in type 1 and prints `For this game type difficulty level is disabled.`
  otherwise) [MS-1d]. Every other type is a multiplayer game.
- An **action** is a named input of the key bindings (`config\default_controls.ltx`, OS section 11);
  each has a number: `cam_1` 16, `cam_2` 17, `cam_3` 18, `use` 42 [MS-4l, MS-5e].
- Distances are in metres (level units). "Prints" means "writes as one log line" (CC conventions).
- Specifications referred to: CC `console-commands.md`, CS `console-scope.md`, OS `options-screens.md`,
  TM `title-menu.md`, PM `player-movement.md` (all `docs/behavior/`).

**Checks.** Every claim carries a tag [MS-*n*]. The checks are one script in the dirty workspace,
`misc-settings/checks/run.py`; it reads the shipped binaries (data values, strings) and read-only static
analysis of the same binaries, the game data extracted from the archives, the shipped `user.ltx`, and
the decompiled command registrations of the renderers kept by the checks of CS (`console/out/`). Tags
MS-8a … MS-8c are checked once per renderer library. All checks pass.

## 1. Summary

| Command | Effect in the single-player game | Section |
|---|---|---|
| `g_autopickup` | none: it acts only in multiplayer games | 2 |
| `g_corpsenum` | none: it acts only on a multiplayer server | 3 |
| `g_eventdelay` | none: the delay it adds is stored but never read | 4 |
| `cl_cod_pickup_mode` | 1: the item looked at becomes the pick-up target, shown on the HUD and taken with `use`; 0: no pick-up target | 5 |
| `cam_slide_inert` | smoothing of the distance of the third-person (look) cameras | 6 |
| `snd_acceleration` | only with `-dsound` on the command line: how sound buffers are created | 7 |
| `check_for_updates` | asks an online service for a newer patch; reports the answer in the log and the main menu | 8 |
| `r2em` | none on drawing: `on`/`off` switch a message, a number prints it while switched on | 9 |

## 2. `g_autopickup`

- The setting is read in one place only: a step of the actor's per-frame update [MS-1a, MS-1b, MS-1e].
- That step does nothing unless the setting is on **and** the game type is not 1 (single player); so in
  the single-player game the setting has no effect [MS-1c, MS-1d].
- In a multiplayer game, while it is on and the actor is alive (health > 0) and is this computer's player
  (**believed**: the actor is compared with the level's own player), every frame the step finds the objects in an axis-aligned box around the actor and, for each one that is
  an item the actor may take, sends the request to take it (the same request as taking an item by
  hand) [MS-1c]. The box is given by two keys that exist only in `config\mp\mp_actor.ltx`:
  `AutoPickUp_AABB = 0.2, 0.1, 0.2` (half sizes) and `AutoPickUp_AABB_offs = 0.0, 0.1, 0.0` (offset of its
  centre from the actor's position) [MS-1f]. The reading of these keys into the box is **believed** (the
  box is built from two vectors of the actor read by the one function that reads the keys). Which items
  qualify (team and game-mode rules) is **unestablished**; it belongs with multiplayer.

## 3. `g_corpsenum`

- The value is read in one place only: the per-frame update of the multiplayer server's game, which is
  reached only by multiplayer game types (it is not part of the single-player game) [MS-2a, MS-2c].
- There, while the server's list of dead bodies is longer than `g_corpsenum`, it removes bodies from the
  front of the list (the oldest, **believed**: the list is a queue) [MS-2b]: a body whose object no longer
  exists is dropped with the log line `corpse [<id>] not found [<frame>]`; a body with no objects attached to it
  (no items) is destroyed with `corpse [<id>] send destroy [<frame>]`; a body that still has attached objects is kept, logged
  as `corpse [<id>] childern not empty [<frame>]` (sic), and the next one is examined [MS-2b].
  `<frame>` is the engine's frame number (**believed**).
- In the single-player game it has no effect.

## 4. `g_eventdelay`

- The value is read in one place only: when the client side of a level reads a received game-event
  message into its event queue, the value (in milliseconds) is added to the event's time stamp [MS-3a,
  MS-3b, MS-3c]. This happens in every game type, the single-player game included (its messages pass the
  same path, **believed**: the receive step is the level's only one).
- The queue is emptied completely each time it is processed, every event handled in order, and the time
  stamp of a queued event is never read [MS-3d, MS-3e]. The setting therefore has no observable effect.

## 5. `cl_cod_pickup_mode`

### 5.1 The pick-up target (mode 1)

Every frame the actor's update runs first the use step of 5.2 (only while the actor is alive), then the
following [MS-4n]. It is the only reader of the setting [MS-4a], and it does nothing unless the actor is the
controlled object and `cl_cod_pickup_mode` is not 0 [MS-4b]. The built-in and the shipped value is 1 (CC section 8).

1. If the actor's health is 0 or less, or its active camera is not the first-person camera (camera 0,
   section 6.2), the pick-up target is set to none and the step ends [MS-4c, MS-5g].
2. The candidates are the objects in the view frustum (the current view and projection) [MS-4d]. A
   candidate is kept only if it is an inventory item lying in the world (carried by no one), passes the
   item's own test of whether it may be taken (**unestablished** which), and its configuration class is
   neither `G_RPG7` nor `G_FAKE` (the rockets: `[wpn_rpg7_missile]`, `[helicopter_missile]`,
   `[wpn_fake_missile]`, `[wpn_fake_missile1]`, `[wpn_fake_missile2]`) [MS-4e, MS-4f]. Grenades and the
   other thrown items are kept only if a further state test passes (**unestablished** which state)
   [MS-4g]. An item with one particular object flag set is skipped (**unestablished** which flag)
   [MS-4i].
3. A kept item qualifies if the squared distance from the actor's position to the item's centre is at
   most 4 (2 m), and the squared distance from the item's centre to the line through the active camera's
   position along its view direction is at most 1 (1 m). Among the qualifying items the one nearest to
   that line wins (ties: the first examined) [MS-4h].
4. The winner is dropped unless it is visible: its centre more than 0.25 m from the viewpoint, its
   bounding sphere in the frustum, and no obstacle on the straight line from the viewpoint to its centre
   (a ray query; which objects count as obstacles is **unestablished**) [MS-4j]. The viewpoint is
   **believed** to be the camera position.
5. The winner (or none) becomes the **pick-up target** [MS-4k].
6. If there is a target and the use flag of 5.2 is set, the request to take the target is sent and the use
   flag is cleared, so one press of `use` takes at most one item this way [MS-4k].

The pick-up target is set to none when the HUD starts [MS-4q].

### 5.2 The use flag

- Pressing the key of the action `use` (42) sets the actor's **use flag**; releasing it clears it [MS-4l,
  MS-4m]. Pressing `use` also does the actor's other `use` effects (talking, searching bodies, vehicles),
  which are outside this specification.
- In the single-player game, while the actor is alive and the use flag is set, a separate step (which runs
  before 5.1, whatever the mode) takes the inventory's current target item if there is one, and handles
  the items the actor touches; it does not clear the flag [MS-4o]. How that target and those items are
  chosen is **unestablished** here (the pick-up hint of `docs/behavior/hud-scope.md`).

### 5.3 The HUD icon

- While there is a pick-up target and the controlled object is of class `O_ACTOR` (the actor), the HUD
  element `pick_up_item` of `config\ui\maingame.xml` shows the target's inventory icon; otherwise it is
  hidden [MS-4p, MS-4q]. The icon is the rectangle of the item's section keys `inv_grid_x`, `inv_grid_y`,
  `inv_grid_width`, `inv_grid_height`, each multiplied by 50 (pixels of the inventory icon texture), drawn
  in colour 0xC0FFFFFF (white, alpha 192), scaled by min(1, *w* / *iw*, *h* / *ih*) where *iw* × *ih* is
  the icon's size and *w* × *h* that of the element [MS-4p, MS-4r]. Its exact position inside the element
  is **unestablished**.

### 5.4 Mode 0

With `cl_cod_pickup_mode 0` the steps of 5.1 do not run: the pick-up target keeps the value it had (none,
unless it was set before the mode changed), the icon of 5.3 is not shown, and `use` takes items only
through the step of 5.2 [MS-4b, MS-4o].

## 6. `cam_slide_inert`

### 6.1 What it changes

- The value *s* is read in one place only: the update of the **look camera**, the camera that orbits a
  pivot point at a distance (third-person view) [MS-5a, MS-5b]. The engine itself only registers it.
- Every update the look camera computes its view direction **f**, then casts a ray from the pivot point
  backwards (along −**f**) of length *z* + 1.2, where *z* is the camera's zoom distance, ignoring the
  object that owns the camera (what else the ray tests is **unestablished**) [MS-5c]. With *r* the distance of the nearest
  hit, the camera's current distance *d* becomes
  *d* ← (1 − *s*) · (*r* − 1.2) + *s* · *d*,
  and the camera is placed at pivot − **f** · (*d* + 0.2) [MS-5c].
- So *s* = 0 puts the camera at the hit distance at once; values toward 1 make it slide there slowly; with
  *s* = 1 it never moves from its first distance. The step is per update, not per second, so the sliding
  speed depends on the frame rate. The shipped and built-in value is 0.25 [MS-5f].
- The distance *r* when the ray hits nothing is **unestablished** (**believed** the ray's length, so the
  camera rests at the zoom distance *z*).

### 6.2 Which cameras

- Look cameras are made, each with its own configuration section of `config\system.ltx`, for the actor
  (`[actor_look_cam]`, except when the command line contains `-psp`, which gives the actor a different
  camera, `[actor_look_cam_psp]`; and `[actor_free_cam]`), for vehicles (`[car_look_cam]`,
  `[car_free_cam]`) and for the multiplayer spectator [MS-5d].
- The actor's cameras are chosen by the actions `cam_1` (camera 0, first person, `[actor_firsteye_cam]`),
  `cam_2` (camera 1, `[actor_look_cam]`) and `cam_3` (camera 2, `[actor_free_cam]`); cameras 1 and 2 are
  look cameras [MS-5d, MS-5e, MS-5g]. None of these actions is bound in the shipped
  `default_controls.ltx` or `user.ltx`, so the actor's look camera is reached only after binding `cam_2`
  [MS-5f]. Whether other rules prevent the switch in the single-player game is **unestablished**; the
  third-person cameras are outside PM (PM 10). The mapping of `cam_1` … `cam_3` to cameras 0 … 2 is
  **believed** (the three choice steps are in the order of the actions).
- How a vehicle's look camera is selected is **unestablished**.

## 7. `snd_acceleration`

- The sound library has two output paths: OpenAL, used by default, and DirectSound, used only when the
  command line contains `-dsound` [MS-6c].
- The setting (the mask 0x2 of the sound flags; on by default) is read in one place only: when a DirectSound
  output channel is created [MS-6a, MS-6b, MS-6f]. So without `-dsound` it has no effect.
- With `-dsound`, each output channel's sound buffer is created with the buffer flags 0x100B0, plus 0x8
  when `snd_acceleration` is off [MS-6b]. Its 3D processing algorithm is chosen by the sound library's
  sound model, which is always 0 (nothing sets it), giving the identifier
  00000000-0000-0000-0000-000000000000; when `snd_acceleration` is on the identifier
  C2413340-1C1B-11D2-94F5-00C04FC28ACA is used instead [MS-6b, MS-6d, MS-6e] (both as given to the
  DirectSound buffer description; their meaning is defined by DirectSound).
- A change takes effect for channels created afterwards; when channels are created again (at `snd_restart`,
  **believed**) is not established here.
- The web build has neither path; the setting has no counterpart there (**believed**: browsers expose no
  such choice).

## 8. `check_for_updates`

- It does something only while the main menu exists (the main menu object registers itself when it is
  created; when that happens and whether it exists during a level are **unestablished**) [MS-7a, MS-7b].
- Argument: none (or empty) means **report** on; otherwise the argument is read as a decimal integer (1 if
  it does not start with one) and report is on unless it is 0 [MS-7a].
- It then sends a query for a newer patch to the online patch service of the online-services library
  (`xrGameSpy.dll`) [MS-7h]. If the query cannot be sent it prints `! Unable to send query for patch!`
  [MS-7a]. The query's contents and the service are **unestablished** (outside this specification).
- When the answer arrives (while the main menu exists) [MS-7d]:
  - no newer patch: prints `No new patches are available.`; if report is on, the main menu shows the
    message box `msg_box_no_new_patch` (`config\ui\message_box.xml`, type `ok`, text
    `ui_st_conn_no_new_patch` "Нет новых патчей") [MS-7c, MS-7d, MS-7f];
  - a newer patch: prints `Found NewPatch: <version> - <address>` and, unless a patch download is already
    in progress, shows the message box `msg_box_new_patch` (type `yes_no`, text `ui_st_conn_new_patch`
    with the two values in its `%s` places) [MS-7d, MS-7e, MS-7f]. Accepting starts the download shown by
    the download strip (OS 3.5); the download is outside this specification.
- The shipped scripts execute `check_for_updates 0` when the title menu's multiplayer button opens the
  multiplayer screen (TM) and `check_for_updates 1` from the options' `btn_check_updates` button (OS 3)
  [MS-7g]. So the first never shows the "no new patches" box, the second does.
- The service the original queried is not available to the web build; what it does instead is a product
  decision, not part of the original.

## 9. `r2em`

Both renderer libraries handle `r2em` the same way [MS-8a, MS-8b, MS-8c]:

- `r2em on` and `r2em off` (exactly these words, compared after the line processing of CC section 2)
  switch a **message switch** on and off; nothing else changes [MS-8b, MS-8d].
- Any other argument is a number for the real-valued variable (CC section 3, range 0 … 4). After storing
  it, while the message switch is on, the renderer prints
  `* material set to [<a>]-[<b>], with lerp of [<f>]`, where *i* = the value truncated to an integer,
  *a* = name *i* mod 4, *b* = name (*i* + 1) mod 4 of the list `oren`, `blin`, `phong`, `metal`, and *f* =
  the value minus *i* printed with six decimals [MS-8b, MS-8c].
  **Disputed in review:** the message does not depend on the value being stored: after any argument other
  than `on` and `off`, accepted or rejected (CC section 3: a rejected value is left unchanged and the two
  rejection lines are printed), the message is printed while the switch is on, from the variable's value
  as it then is.
- Nothing else reads the value, the message switch, or the command by its name: it has no effect on
  drawing in either renderer [MS-8a, MS-8e, MS-8f, MS-8g].
- The message switch starts off and is not saved: no saved command covers it, and only `r2em` changes it
  [MS-8e, MS-8h].

## 10. Unestablished

- Which items `g_autopickup` takes in multiplayer, and the reading of its box keys (2).
- The order of the multiplayer server's list of dead bodies (3).
- The item tests of the pick-up target: the item's own test, the state test of thrown items, and the
  object flag (5.1); which obstacles block the visibility ray (5.1); the step of 5.2 that takes the
  inventory's target; the icon's position in `pick_up_item` (5.3).
- The look camera's distance when its ray hits nothing and what its ray tests; whether the single-player
  game allows the actor's look cameras once `cam_2` or `cam_3` is bound; how a vehicle's look camera is
  chosen (6).
- When DirectSound channels are created again after a change of `snd_acceleration` (7).
- Whether the main menu exists during a level, and the patch query itself (8).

## Gaps found in review

- Pressing and releasing `use` change the use flag (5.2) only while the actor is alive and the key reaches
  the actor's own input handling (not taken first by another receiver of the key); these conditions are not
  stated.
- `check_for_updates` keeps a single report setting, not one per query: an answer is shown according to the
  argument of the latest `check_for_updates` executed before it arrives (8).

## Provenance

- Date: 2026-11-26. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build) owned by the
  user; archives extracted with the project's archive tool (`config\mp\mp_actor.ltx`,
  `config\creatures\actor.ltx`, `config\weapons\*.ltx`, `config\creatures\helicopter.ltx`,
  `config\system.ltx`, `config\default_controls.ltx`, `config\ui\message_box.xml`,
  `config\text\rus\ui_st_others.xml`, `scripts\ui_main_menu.script`, `scripts\ui_mm_opt_main.script`);
  the shipped `__support\userdocs\user.ltx`.
- Static analysis: Ghidra 12.2, headless and read-only, of `xrGame.dll` (the readers of each setting, the
  actor's update and input, the camera update, the HUD, the main menu, the receive path), `XR_3DA.exe`
  (registration of `cam_slide_inert`), `xrSound.dll` (device choice, output channel creation) and both
  renderer libraries (`r2em`); data values and strings read directly from the files. No execution or
  debugging.
- No leaked source code, derived projects or third-party documentation of the engine were used.
