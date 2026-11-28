# The player's use action

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the player's **use
action** as one system: how the key reaches it, the object under the crosshair and how it is picked, the object's
use callback, inventory boxes, characters (the search window of the dead, talk with the living), taking items,
dragging bodies with left Shift, the use while in a vehicle or other holder, and the flag "usable not only
through its script", which this specification owns in full.

Builds on (not repeated):

- `docs/behavior/console-key-bindings.md` (cited as `KB`): how a press reaches the player character (5).
- `docs/behavior/player-movement.md` (`PM`): actions and bindings (2).
- `docs/behavior/physics-contacts.md`, `docs/behavior/physics-world.md` (`world`: the step `h`, the world spring
  `S` and damping `D`, gravity 19.62), `docs/behavior/physics-shells.md` (`shells`: elements, their speed limits
  2.4), `docs/behavior/physics-scope.md` (`scope`: `[ph_capture_visuals]`, 1.4).
- `docs/behavior/script-api-game-object-basics.md` (`M`): the script handle, `set_nonscript_usable` (6.3).
- `docs/behavior/script-api-object-event-callbacks.md` (`O`), `docs/behavior/script-api-engine-services.md` (`B`),
  `docs/behavior/script-api-scope.md` (`SA`): the script API members a use callback calls (4.1).
- `docs/behavior/inventory-screen.md` (`IS`): in-game windows, the body-search window's files (5.9).
- `docs/behavior/console-misc-settings.md` (`MS`): `cl_cod_pickup_mode` and the pick-up target (5).
- `docs/behavior/view-projection.md` (`VP`): the environment's far value `Z` (2.1).
- `docs/behavior/dialogue-scope.md` (`DG`): what a talk is once started (DG-3).
- `docs/behavior/hud-scope.md`: the hint and pick-up line drawn from the crosshair record (13).

Conventions:

- The **player** is the player's character. **Single play** is game type 1 (`MS` conventions).
- The **use flag** is the player's own flag "use held" (section 1.3); it is not `M` 6.3's "use flag", which this
  specification calls the **usable flag**. The **usable flag** is the per-object flag
  "usable not only through its script" (section 10).
- A **character** is an object that has an inventory of its own: the player, stalkers, traders and every monster
  class [U5]. An **item** is an inventory item. A **holder** is an object the player can occupy: a car, a mounted
  gun (configuration class `W_MOUNTD`), a stationary gun (`W_STMGUN`) [U5b, U13].
- An **in-game window** is one of `IS` 1.1 (inventory, PDA, talk, trade, body search, …).
- Statements are static (read from the shipped binaries and data; the game was not run). Inferred statements are
  marked **believed**, with why; what could not be established is marked **unestablished**.
- Bracketed ids are checks: `U*` in `checks/use-action/checks.py` (`U17*` in `checks/use-action/door_callback.py`), `SR-*` (folded in with the usable flag, see
  Provenance) in `checks/use-action-usable-flag/shapeless_root.py`; `python3 checks/use-action/run_all.py` runs both.

## 1. Input

### 1.1 Binding

The action is `use`, number 42 in the key-action table, bound by default to `kF`
(`config\default_controls.ltx`: `bind use kF`) [U1, U1b, SR-D5]. A press reaches the player by `KB` 5 (rule 6);
while any in-game window is open the press is taken by the interface and never reaches the player (`KB` 5 rule 3).

### 1.2 The press

The player handles a press of `use` only when all of these hold, in this order [U2, U2b, U2c]:

1. the player object is the one this computer controls (**believed**: a per-object network flag; always so in
   single play);
2. the player is not talking (its talk flag, set when a talk starts and cleared when it ends, section 6.2, is clear);
3. no monster has taken control of the player: an attack that takes over the player's input installs a filter
   that passes only two actions, neither of them `use`, so `use` is ignored for its duration;
4. the player's health is above 0.

Then the item in the player's hands is offered the action first; no shipped item takes `use` (**believed**: no
item kind's own action handling names it), and the use action of section 3 runs. While the player is in a holder
the other actions go to the holder, but `use` still goes to the use action [U2].

### 1.3 The use flag and the release

- The use action sets the use flag first thing (section 3, step 1) [U6, U3].
- A release of `use` that reaches the player clears it; the release is handled under conditions 1, 3 and 4 above
  (not 2) [U3].
- A release while an in-game window is open is taken by the interface and never reaches the player [U3c]: so after
  a press that opens a window (sections 5, 6), the use flag stays set until a later release of `use` reaches the
  player, or until something below clears it.
- Entering a holder clears it (section 8) [U13b]; so does a take of the pick-up target by `MS` 5.1 step 6.
- The flag is read only by the pickup step (section 9.1) and the pick-up target of `MS` 5.1 [U3b].

## 2. What is under the crosshair

### 2.1 The crosshair's pick

Once per frame (**believed**: the HUD's frame routine runs it [U16d]) the HUD casts a ray [U4, U4b]:

- from the camera position along the camera's view direction, of length 0.99 × the environment's far value `Z`
  (`VP` 2.1);
- against both objects and the level mesh, ignoring the controlled object (the player);
- going through the hits in order: a level-mesh hit whose material has the game-material flag bit 0x80 set is
  passed through <!-- spec-lint: constant -->; the first other hit ends the search and is the **pick**: the
  object hit (none for the level mesh), its distance and, for an object, the element (bone) hit;
- if nothing is hit, the distance is the ray's length and there is no object; a hit closer than 0.5 m is
  recorded at 0.5 m.

Which material that flag marks, and the order of the HUD's pick against the player's update in a frame, are
**unestablished**.

### 2.2 The crosshair record

The player's update keeps a record of what is under the crosshair [U4c, SR-B15c]. It is cleared (all parts none)
when a monster controls the player (1.2, item 3), when the pick has no object, or when the pick's distance is at
least `[inventory] take_dist` = 2 (`config\system.ltx`) [U15b]. Otherwise it holds, for the picked object:

- the object;
- its **usable part**: every game object has one; it carries the usable flag and the hint text (section 10);
- the object as an **inventory box**, if it is one;
- the object as a **character**, if it is one;
- the object as an **item**, if it is one: this becomes the inventory's **target item** (section 9.1);
- the object as a **holder**, if it is one (used for the hint only).

The hint drawn from the record is the HUD hint's (`hud-scope` 13, another specification's); it reads the usable part's hint text only, never the usable
flag [SR-B15c]. In this specification "the object under the crosshair" is the record's object: it exists only
within 2 m. The pick of 2.1 itself has no distance limit below the ray's length; sections 7 and 8 use it directly.

## 3. The use action

Each run [U6, U6b, U6c, U6d, U6e, SR-B15, SR-B15b]:

1. Set the use flag (1.3).
2. **In a holder:** send the event "use the holder" (event 42) for the player, naming the holder; nothing else
   happens (section 8). Stop.
3. **While dragging** (section 7): release the drag, then go on.
4. **Use callback:** if an object is under the crosshair, run its use callback (section 4), whatever its usable flag.
5. **Inventory box:** if the object under the crosshair is an inventory box and its usable flag is set: in single
   play with the single-player interface, open the box (section 5). Stop either way.
6. **Gate:** go on only if no object is under the crosshair or its usable flag is set. Otherwise stop.
7. **Character:** only in single play, if a character is under the crosshair:
   - health above 0: try to talk to it (section 6.2);
   - health 0 or less, left Shift not held, and the single-player interface present: open its search window
     (section 6.1);
   - then go on (both cases).
8. **Pick** (2.1): if the pick has an object of a kind that can have a physics shell (characters, physics objects,
   cars; not inventory boxes; items count too, through their own part) [U5b]:
   - left Shift held: if the object's model name is listed in `[ph_capture_visuals]` and no drag is under way,
     start dragging the picked element (section 7);
   - left Shift not held: if the object is a holder, send the request to enter it (event 41, section 8).

Left Shift is the key state of DirectInput code 0x2A (public documentation) <!-- spec-lint: constant --> on the
level's input [U6f]. With no object under the crosshair, step 6 passes and step 8 still acts on the pick, at any
distance. Note: the event of step 8 without Shift is not a take event; it is the
request to enter a holder [U5b, U5c]. This specification also corrects `M` 6.3: the usable flag has three readers
(section 10.2), and with left Shift the engine's handling is the drag of section 7, not a handling for items.

Taking items is not a step of the action: it is done by the pickup step (section 9) while the use flag is set.

## 4. The use callback

The use callback is script callback 15 (`callback.use_object` in `scripts\lua_help.script`), called with the
object and the player [U7] (its registration: script API, `M`). It runs at step 4 for every object under the
crosshair, whatever the usable flag, and is the only thing the use action does to an object whose usable flag is
cleared: steps 5 to 8 do nothing for it (a drag under way is still released at step 3).

What a use callback does is the shipped scripts' own behaviour, not the program's: the engine calls the function
a script stored for id 15 with its bound value (`O` 3.2, 4) and nothing more; the scripts run as shipped, so an
implementation needs only the script API members they call.

### 4.1 The physics-object binder and the door scheme

- The physics-object binder (`bind_physic_object.script`) stores, on its updates (when: `O` 10), its own
  use handler for `callback.use_object` with the binder as the bound value (`O` 10). The
  handler, while a section is active, passes the use to the active scheme as its event `use_callback` with the
  object and the user; the event calls that method of each active action of the scheme (`xr_logic.issue_event`),
  nothing when the scheme has none [U17, U17b].
- The door scheme (`ph_door.script`), on that event: if its section is `locked` and names a `snd_open_start`
  sound set, it plays that set at the door; then, only if the section has an `on_use` condition list, it switches
  to the section that list picks for the player and the door (none, empty or the active section: no switch); it
  does nothing else [U17c, U17c2, U17f]. A switch ends the active scheme (its event `deactivate`) and starts the
  picked section's scheme (`xr_logic`); the list's conditions, effects and info portions given or disabled on the
  player are the scripts' too [U17f, U17f2].
- Playing a set: the scheme stops the sound it played last, takes a sound object for the set from
  `xr_sound.get_sound_object` (an entry of the set from `sound_theme.script`, at random among two or more, else the
  only one; one sound object per file path, made once) and plays it at the door object's position [U17d, U17d2].
  **Disputed in review:** the pick is incomplete: for a set of two or more entries, once an entry of that set has
  been picked, each later pick is at random among the entries other than the one picked last for that set (only
  the first pick is among all of them) (`xr_sound.get_sound_object`).
- Script API members of this path: `set_callback` with `callback.use_object` (`O` 3, 4); `name()` (evaluated as
  an argument of the scheme's own `printf`, which does nothing) and `position()` (`M` 3, 4); `log` (`B` 4.3,
  through the global `printf` of `xr_sound`) [U17c2, U17d3]; the sound class `sound_object(path)` and its
  `stop()` and `play_at_pos(object, position)`, which belong to the sound part of the script API (`SA` row T,
  with no approved specification); on a switch, the members of the condition list's functions and of the
  picked section's scheme start (`ph_door`'s: `M`, `O`, the physics members of
  `docs/behavior/script-api-physics-vehicles-helicopter-and-lamps.md` and `set_tip_text`, which `M` 12 assigns to `SA` row P).
  What playing a sound object does is the sound part's to specify, not this specification's.

## 5. Inventory boxes

Opening a box: when no in-game window is open, the body-search window (`carbody_new.xml`, `IS` 5.9) is filled
with the player's inventory and the box's contents, the box is marked as being looked into (**believed**, a flag
set on the box), and the window is shown [U8d, U8e]. When an in-game window is open nothing happens (but the press
cannot reach the player then, 1.1). What the window shows and does is that window's specification
(**unestablished** here). A box whose usable flag is cleared is not opened: only its use callback runs (steps 4–6).

## 6. Characters

### 6.1 The search window of the dead

At step 7, for a character with health 0 or less, without left Shift, the same body-search window as 5 is filled
with the player's inventory and the character's and shown, when no in-game window is open [U6d, U8e]. Every
character class qualifies, monsters included (static: they all have an inventory part) [U5]; what the window shows
is that window's specification.

### 6.2 Talk with the living

At step 7, for a character with health above 0 [U8, U8b, U8c]:

1. If the player is talking, nothing happens.
2. The character is offered the talk. It accepts when its talk is enabled (the flag the script functions
   `enable_talk`, `disable_talk` and `is_talk_enabled` set, clear and read) and both it and the player have health
   above 0; on accepting it starts talking with the player (its talk flag set, its partner the player).
3. If it accepted: the player starts talking with the character (talk flag set, partner the character); then, with
   the single-player interface, an open in-game window, if any, is toggled off, and the talk window (`talk.xml`) is
   shown.

What the talk then does (which dialogs, their end) is `DG` DG-3's. Left Shift does not matter here.

## 7. Dragging with left Shift

### 7.1 Start

At step 8 with left Shift, a drag starts when all hold [U6e, U9]:

- the picked object's model name is a key of `[ph_capture_visuals]` (`config\system.ltx`, 147 names; `scope` 1.4) [U15, SR-D6]; whether the name is matched with its case and folders as the object's
  record gives it is **believed**;
- no drag is under way;
- the object has a physics shell and the shell is active (**believed**: so living characters, which move without
  an active shell, are not dragged);
- the object is not an item;
- the player's model has a `[capture]` section in its user data naming a `bone` (the **capture bone**), and the
  picked bone has a physics element.

Parameters from the player's model's `[capture]` section [U9d]:

| key | 15 player models | `actors\dolg\stalker_do_nauchniy.ogf` (`dolg_scientific_outfit`) |
|---|---|---|
| `bone` | `bip01_tail` | `bip01_tail` |
| `pull_distance` | 1.5 | 1.4 |
| `distance` | 0.3 | 0.3 |
| `capture_force` | 90000 | 6500 |
| `time_limit` | 3 (seconds) | 3 |
| `pull_force` | 3000 | 8000 |
| `velocity_scale` | 0.05 | 0.022 |

(The 15 are `[actor] visual` `actors\hero\stalker_novice.ogf` and every other outfit's `actor_visual`.)

Set-up [U9b, U9c]: `P` is the capture bone's position in the world (from the player's model and world transform)
and `E` the picked element's position. If `|P − E| > pull_distance` the drag fails at once (nothing else happens).
Otherwise:

- start time = now (milliseconds); limit `time_limit × 1000` ms;
- pull force `F = min(pull_force, m × g × 4)`, `m` the object's shell mass (sum of its elements' masses), `g` the
  world's gravity 19.62 (`world`);
- the element's two speed limits (**believed**: the two values 150 and 9.817477 of `shells` 2.4, which the element
  starts with) are multiplied by `velocity_scale` [U16];
- the player's inventory slots are all blocked (the event 40 "block slots" with every slot and "on"): the item in
  the hands is put away while dragging (**believed**, from the slot blocking of `IS`).

### 7.2 Each physics step

The drag is in one of three states: pulling (from the start), holding, released [U10, U10b, U10c, U10d, U10f].

**Pulling.** With `P`, `E` recomputed:

- release (7.3) if the element is not active, or more than the limit has passed since the start, or
  `|P − E| > pull_distance`;
- if `|P − E| ≥ distance`: add [U16] the force `F × (P − E)/|P − E|` to the element's body;
- otherwise **grab**: create an anchor body at `P` (a sphere mass of density 1, adjusted, gravity off; its radius
  and adjusted mass are **unestablished** here), a ball joint between the anchor and the element anchored at `P`,
  and an angular motor (library mode 1, Euler; 3 axes) between them; restore the element's speed limits; state
  holding. Motor (library parameter numbers, public ODE 0.5.0): axis 0 = `u = (P − E)/|P − E|` relative to the
  anchor; axis 2 relative to the element, chosen from the components of `u` (`ε` = 0.00001):

  | `u.x > ε` | `u.y > ε` | `u.z > ε` | axis 2 direction |
  |---|---|---|---|
  | yes | yes | any | (−u.y, u.x, 0) |
  | yes | no | yes | (−u.z, 0, u.x) |
  | yes | no | no | (1, 0, 0) |
  | no | yes | yes | (0, −u.z, u.y) |
  | no | yes | no | (0, 1, 0) |
  | no | no | any | (0, 0, 1) |

  (the library normalises the axis; in rows 3 and 5 axis 2 is parallel to axis 0). On all three axes: velocity 0,
  maximum force `0.2 × capture_force`, stop ERP `e` and stop CFM `c` and CFM `c`, with `a = S × 0.1 × h`,
  `e = a / (a + 10 × D)`, `c = 1 / (a + 10 × D)` (`S`, `D`, `h` of `world`; at their values with `h` = 0.01:
  `a` = 24000.0, `e` = 0.0059642, `c` = 2.4851 × 10⁻⁷) [U16b].

**Holding.** Using the ball joint's feedback (the force on the anchor `f₁`, on the element `f₂`):

- release (7.3) if the element is not active or `|f₂| > capture_force`;
- otherwise, if `|f₁| > 0.4545454 × capture_force`, a force (**believed**) `0.0666667 × capture_force` along `f₁/|f₁|` is
  applied to the player's movement body; then the anchor body is moved to the current `P`.

How the player's movement body takes that force is `PM` 6's (**unestablished** which entry point).

### 7.3 Release

A drag is released by the next use action (step 3), by the conditions of 7.2, and when its object goes away
(**unestablished**: other releases). Releasing [U10e]: the joints and the anchor body are removed; if it was still
pulling and the object still has an active shell, the element's speed limits are restored; the player's slots are
unblocked (event 40 with "off"); state released. A released drag is removed in the player's update a step or two
later (**believed**, from its failure flag); until then no new drag can start, so the press that releases a drag
never starts another (**believed**).

## 8. Holders

- **Entering** [U5c, U5d, U13]: the request of step 8 (event 41, naming the holder) goes to the server, which
  sends it back to the clients; the player's handling finds the holder and, when it has no user, enters it if the
  holder accepts (a car by its own test from the camera position, view direction and the player's position; a
  mounted or stationary gun by its own test): the use flag is cleared [U13b] and a torch the player carries is
  switched off (**believed**: its switch is called with "off"). The holders' own tests and what being in a holder does are the vehicles' specification
  (**unestablished** here). Cars running the shipped `ph_car` scheme clear their usable flag (section 10.3), so
  step 6 stops the action before step 8 for them while that scheme sets it.
- **Using while in a holder** [U6, U5c]: step 2 sends event 42 naming the holder; it comes back through the server;
  the player, while in a holder: a car is left when its own use test passes (else nothing); a mounted or stationary
  gun is left at once. The use callback, the usable flags and steps 3–8 are skipped.
- Whether the round trip through the server takes a frame is **unestablished**.

## 9. Taking items

### 9.1 The pickup step

Each frame the player's update, while the player is alive, runs this step (before the pick-up target of `MS`
5.1); it acts only in single play while the use flag is set [U11, U11b, U11c, U11e]:

1. If the inventory's target item (2.2) exists, passes its **usefulness test**, and the usable flag of the object
   under the crosshair is set, the request to take it is sent (event 1, naming the item) [U11]. The flag stays set,
   so holding `use` takes one target item after another as the crosshair finds them.
2. The player's touch set is updated: the objects within `[actor] pickup_info_radius` = 5 m of the player
   (`config\creatures\actor.ltx`) that are useful items lying in the world (no owner), or other characters.
3. For each touched item that is visible (its centre farther than 0.25 m from the viewpoint, its bounding sphere
   in the view frustum, and no obstacle on the ray to its centre; **believed**: the viewpoint is the camera), its
   name (the string-table text of its section's `inv_name`) is written at its screen position, centred, colour
   0xFFDDDDDD <!-- spec-lint: constant --> [U11d]. The font and the exact position are **unestablished**.

The usefulness test [U12, U12b, U12c]: by default the item's **take flag**, its section's `can_take` (true when
absent; false only in `[explosive_fuelcan]`, `[explosive_tank]`, `[explosive_barrel_low]`, `[explosive_barrel]`,
`[explosive_mobiltank]`, `[explosive_dinamit]` of `config\weapons\weapons.ltx`); a bolt never passes; food,
ammunition, grenades, rockets and some artefacts add their own conditions (**unestablished** here: the item
specifications').

The pick-up target (`cl_cod_pickup_mode`) is `MS` 5.1's; it takes at most one item per press and clears the use
flag.

### 9.2 The take request

The request goes to the server [U12e], which writes the log line
`sv ownership id_parent <name> id_entity <name> [<number>]` (**believed**: the taker's and the item's names and the
frame number) and grants it only when the item has no owner (else it writes
`sv !ownership (entity already has parent) new_parent <name> id_parent <name> id_entity <name> [<number>]` and
nothing more happens). A granted request comes back to the player [U12d]: the player accepts the item unless it is
marked for removal or its take flag is false (the player has no weight limit here); an accepted item becomes the
player's and goes into the inventory (where it goes: inventory specification, **unestablished** here); a refused
one is handed back (event 3, the item dropped back into the world).

## 10. The usable flag

### 10.1 Value and setter

Every game object has the flag, set (true) when the object is made [SR-B14]; `set_nonscript_usable(b)` stores `b`
(`M` 6.3) [SR-B14]. For a handle with no object nothing is stored and nothing is printed [U7b]. The hint text is kept beside the flag [SR-B14b]. Nothing else writes the flag [SR-B15d]; so it is not
restored from a saved game (**believed**: a loaded object starts with it set until a script sets it again).

### 10.2 Readers

The flag is read in three places, all through the crosshair record (2.2), always the flag of the object under the
crosshair [U14, SR-B15d, U6c, U11]:

1. the use action's inventory-box test (step 5);
2. the use action's gate (step 6);
3. the pickup step's take of the target item (9.1 step 1): an item whose flag is cleared is not taken through the
   crosshair (the pick-up target of `MS` 5.1 does not read the flag).

The hint (2.2) does not read it. The search: no instruction of the game library other than these reads the flag
byte directly, and no function other than the setter both forms an object's usable part and reads its flag; the
crosshair record's usable part is read by these readers and by the hint (its hint text only). Reads through other
paths that these searches cannot see are **unestablished** (**believed**: none).

### 10.3 What it does

So clearing the flag of an object makes the use action on it run only its use callback: no box opening, no search
or talk, no drag or holder entry from that press (steps 5–8), and no take of it as the target item.

Shipped scripts that set it (data) [U14b]: `ph_door`, `ph_gate`, `ph_code` and `ph_car` clear it when their scheme
starts (`ph_car` sets it again in one case); `ph_idle` sets it from the logic key `nonscript_usable` (default
false); `xr_logic` sets it to true when an item's scheme is reset; `explode_zone` clears it on a fuel can [U16c].

## 11. Worked examples

### 11.1 The trader's door

A physics object is no character, no holder and no item; the pick accepts it [SR-B16]; `[ph_capture_visuals]`
lists no door and no model under `physics\` [SR-D6]. The `ph_door` scheme clears the door's flag (10.3). Using the
door within 2 m: step 4 runs the use callback (4.1): the door starts in its section `ph_door@locked`, which is
locked, has `snd_open_start = trader_door_unlock` (one entry, `device\door_servomotor`) and no `on_use`, so that
sound plays at the door and nothing switches [U17e]; step 6 stops. With the flag set, steps 7 and 8
would do nothing for it. So `set_nonscript_usable(false)` changes nothing the player can see.

### 11.2 The trader

The trader (`escape_trader`, model `actors\trader\trader`, `config\gameplay\character_desc_escape.xml`) is a
character. Within 2 m, alive, with its flag set (no shipped script clears it; **believed**): step 4 runs its use
callback if a script set one; step 7 tries the talk: it is accepted only while the trader's talk is enabled (which its logic controls: the trader's logic specification's),
and then the talk window opens with
`escape_trader_start_dialog` first (`DG`). Step 8: the trader is no holder, and its model is not in
`[ph_capture_visuals]`, so nothing more, with or without left Shift [U15]. The window then takes the release of
the key (1.3).

## 12. Unestablished and believed

- That the player-object condition of 1.2 is the local-player flag (believed); that no item takes `use` (believed).
- The material flag passed through by the pick, and the order of the pick and the player's update (2.1).
- The box window's "looked into" mark (believed) and what the body-search and talk windows do (5, 6).
- The model-name matching of `[ph_capture_visuals]` (believed); living characters having no active shell
  (believed); the anchor body's radius and mass; how the player's body takes the holding push; other drag releases;
  the timing of a released drag's removal (believed) (7).
- The holders' own tests and in-holder behaviour; the timing of the server round trip (8).
- The touched items' names' font and position; the item kinds' own usefulness conditions; where a taken item goes;
  the names and number in the server's log lines (believed) (9).
- That the flag is not restored from a save (believed); reads of the flag through paths the searches cannot see
  (believed none) (10).
- The trader's flag being set (believed) (11.2).

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` … `gamedata.dbd`
  (merged view); binary `bin\xrGame.dll`.
- Data examined: `config\default_controls.ltx`, `config\system.ltx` (`[inventory]`, `[ph_capture_visuals]`),
  `config\creatures\actor.ltx`, `config\misc\outfit.ltx`, `config\weapons\weapons.ltx`,
  `config\gameplay\character_desc_escape.xml`, the player models' user data (`meshes\actors\hero\*.ogf`,
  `meshes\actors\dolg\stalker_do_nauchniy.ogf`, with `config\models\capture\stalker_hero_captures.ltx`), the
  shipped scripts (`lua_help`, `ph_door`, `ph_gate`, `ph_code`, `ph_car`, `ph_idle`, `xr_logic`, `explode_zone`,
  `bind_physic_object`, `xr_sound`, `sound_theme`, `_g`), `config\scripts\esc_trader_door.ltx`;
  the public ODE 0.5.0 headers for the joint parameter and motor mode numbers.
- Analysis: static analysis of `bin\xrGame.dll` (headless decompilation and instruction listings with Ghidra;
  vtables and class layouts from its run-time type information): the player's key press and release handling and
  the monster-control filter, the HUD's crosshair pick, the player's crosshair record and per-frame update, the use
  action, the use callback, the talk offer and start, the single-player interface's windows, the drag (start,
  set-up, per-step states, release), the holder events and their server forwarding, the pickup step, touch set and
  name drawing, the item usefulness and take tests, the server's and the player's handling of the take request,
  the setter and every reader of the usable flag; constants read from the binary. No execution or debugging; the
  game was not run.
- The usable flag's facts of the earlier draft `specs/use-action-usable-flag.md` are folded into this
  specification, which owns them in full, with their checks (`SR-*`, `checks/use-action-usable-flag/`); this specification corrects their "take
  event" to the request to enter a holder (section 3) and answers that draft's review gaps (sections 3, 7, 10.2).
- The checks in `checks/use-action/` (`run_all.py`) reproduce every fact.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Which objects get the physics-object binder at all (`bind_physic_object.init` decides it from the object's
  custom data and class) is not stated, nor that the trader's door's
  custom data in `levels\l01_escape\level.spawn` names `scripts\esc_trader_door.ltx` as its logic (11.1).
- What happens when a door's sound set is not among the sound themes, or no sound object can be made for it (the
  scheme then calls the script function `abort`), is not stated.
- A sound theme entry may itself be a pair of paths (sound objects for the path with `_r` and with `_l` appended);
  which of the two a door plays then is not stated.
