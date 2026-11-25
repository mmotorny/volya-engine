# Torches and torch night vision

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) with the game's first
(static-lighting) renderer, of two specifications that share one device, the torch:

- **Part A, Torches** (P20 of the approved `docs/behavior/particles-and-lights-scope.md`, cited as
  `scope`): the actor's torch (the data of its lights, `torch_definition` and how the model reaches it,
  switching with the `torch` command, its state at the start) and characters' torches (switched by the
  shipped scripts; `ai_use_torch_dynamic_lights`); the lights, the glow and the visible light cone a torch
  shows, where they are placed each frame, and how a torch is attached to its owner.
- **Part B, Torch night vision** (S22 of the approved `docs/behavior/level-sound-scope.md`): the four
  night-vision sounds and the night-vision mode they belong to, as far as the sounds need it: when the
  mode switches on and off, which sound plays then and how. The picture and the screen effect of night
  vision are `docs/behavior/hud-scope.md` spec 21 and are not specified here; section 12 states the on/off
  behaviour they share with the sounds.

Builds on (not repeated): `docs/behavior/dynamic-lights.md` (cited as `DL`: light records, their fields and
updates, which lights are drawn and how), `docs/behavior/glows.md` (cited as `GL`: glows), 
`docs/formats/lanims-xr.md` (colour animations), `docs/behavior/player-state.md` (cited as `PS`: the
starting inventory, slots, the torch's state at creation), `docs/formats/sav.md` (the torch byte of the
update packet, section 7.4.2), `docs/behavior/level-objects.md` (cited as `LO`: the object transform
(section 3.4), model files, bones and the pose at rest (section 4)), `docs/behavior/player-movement.md`
(cited as `PM`: bindings (section 2.1), the camera's yaw and pitch (section 8)),
`docs/behavior/sound-positional.md` (cited as `SPos`: sounds, play requests, 2D sounds, volume
categories), `docs/formats/ltx-config.md` (includes), `docs/behavior/options-screens.md` (the setting
`ai_use_torch_dynamic_lights`), `docs/behavior/script-environment.md` and
`docs/behavior/script-loading.md` (which scripts run), `docs/behavior/sky.md` (game time).

Conventions:

- Vectors are rows; a matrix maps a point p by (p, 1)·M (row-vector convention, as in `DL`). "Row k" of a
  4 × 4 transform is its k-th row (0-based): rows 0–2 are the rotation, row 3 the translation. *E* is the
  camera position. "Frame" is one drawn image; *dt* its frame time in seconds.
- **The player kind**: objects of class id `O_ACTOR` (the player in single-player games). **Characters**:
  the non-player owners of torches, stalkers (class ids `AI_STL`, `AI_STL_S`).
- A torch is **on** or **off**; its night vision is **requested** or not (section 15).
- Values of configuration keys are after LTX inheritance and comment removal.
- Statements come from static analysis of the shipped binaries and from the game data; the game was not
  run. **Believed** marks a statement whose program path was not followed completely or whose meaning is
  inferred; what could not be established is marked **unestablished**.
- Bracketed ids name the checks that reproduce each statement, scripts in `checks/torch/` of the dirty
  workspace: `T-B…` in `binary.py` (the binaries), `T-D…` in `data.py` (game data), `T-M…` in `model.py`
  (the torch model), `T-N…` in `numeric.py` (numbers computed from this text); `run_all.py` runs them all.

# Part A — Torches

## 1. The data

### 1.1 The torch item

The torch is the section `device_torch` (`config\misc\items.ltx`). The keys this specification uses
[T-D1]:

| Key | Value | Use |
|---|---|---|
| `class` | `TORCH_S` | the torch kind (`sav` 7.4.2) |
| `visual` | `equipments\light_night.ogf` | the torch model (section 1.2) |
| `slot` | 9 | the torch slot (`PS` section 3.3) |
| `light_trace_bone` | `cone_bone` | the bone shown only while the torch is on (section 4) |
| `attach_bone_name` | `bip01_head` | the owner's bone the torch is attached to (section 6) |
| `attach_angle_offset` | 0, −3.14, 1.6 | rotation of the attachment (section 6) |
| `attach_position_offset` | 0.115, 0.00, 0.07 | translation of the attachment (section 6) |
| `night_vision` | on | the torch has night vision (Part B) |
| `snd_night_vision_on`, `_off`, `_idle`, `_broken` | `weapons\nv_start`, `weapons\nv_off`, `weapons\nv_loop`, `weapons\nv_off` | Part B |
| `disabled_maps` | `ai_test,ai_test2, andy_test_` | Part B |

The section also has `auto_attach = false`, `night_vision_texture = wpn\wpn_night`,
`night_vision_effector = postprocess_night`, `night_vision_recharge_time = 6`,
`night_vision_recharge_time_min = 2` and `night_vision_discharge_time = 20`. None of these six key names
(nor `postprocess_night`) occurs in any of the 34 program files under `bin\`, so the program does not read
them and they change nothing [T-D3]. The keys the program does read all occur in `bin\xrGame.dll` [T-D3].

The only other torch section, `mp_device_torch`, has the same class and model [T-D2]. No section has the
class id `D_TORCH` [T-D2].

### 1.2 The model and `torch_definition`

`meshes\equipments\light_night.ogf` (73,740 bytes) is a skeleton model (`LO` section 4.3; type 10;
chunks 1, 9, 13, 16, 17, 18) [T-M1]. Its four bones, in index order, are `main_bone` (the root),
`bone_nigth`, `lights_bone` and `cone_bone`, each the child of the one before [T-M2]. Its five child
meshes, all 1-link progressive meshes [T-M3]:

| Child | Texture | Shader | Vertices | Triangles | Bones used |
|---|---|---|---|---|---|
| 0 | `wpn\wpn_upgrade` | `models\model` | 116 | 315 | `main_bone`, `bone_nigth` |
| 1 | `wpn\wpn_upgrade` | `models\selflight` | 18 | 16 | `bone_nigth` |
| 2 | `lights\lights_cone_handing_lamp` | `models\lightplanes` | 16 | 8 | `cone_bone` |
| 3 | `glow\glow_Yellow` | `models\lightplanes` | 50 | 30 | `cone_bone` |
| 4 | `wpn\wpn_upgrade` | `models\weapons` | 534 | 4,088 | `bone_nigth` |

**How the torch reaches `torch_definition`.** A model's user data (chunk 17) is read as LTX text whose
base directory is the game configuration directory (`config\`) [T-B46]. The torch model's user data is the
single include line `#include "models\objects\light_night.ltx"` [T-M4], which therefore names
`config\models\objects\light_night.ltx` (`ltx-config.md` section 4; no such file exists beside the model)
[T-M5]. That file holds the one section `torch_definition` (`scope` 1.3) with [T-D4]:

| Key | Value | | Key | Value |
|---|---|---|---|---|
| `range` | 13 | | `range_r2` | 13 |
| `color` | 1.0, 1.0, 1.0, 0.2 | | `color_r2` | 1.2, 1.2, 1.2, 0.2 |
| `omni_range` | 1.5 | | `omni_range_r2` | 0.75 |
| `omni_color` | 1.0, 1.0, 1.0, 0.1 | | `omni_color_r2` | 1.0, 1.0, 1.0, 0.1 |
| `color_animator` | `empty` | | `spot_texture` | `internal\internal_light_torch_r2` |
| `spot_angle` | 60 | | `glow_texture` | `glow\glow_torch_r2` |
| `glow_radius` | 0.3 | | `guide_bone` | `lights_bone` |

A torch whose model has no user data is a fatal error ("Empty Torch user data!") [T-B3].

## 2. A torch's parts

Each torch has (`DL` section 3.2) [T-B4, T-B8]:

- a **main light**: a spot light record, shadow setting on;
- a **second light**: a point light record, shadow setting off;
- a **glow** (`GL` section 2.2);
- the **cone**: the model's bone `light_trace_bone` (`cone_bone`) and its descendants.

Its state: on/off (initially off), the night-vision request (initially not requested), the
**brightness** (initially 1), the smoothed **heading** and **pitch** of the player's torch and their
**heading offset** (all initially 0) (section 7.3) [T-B5, T-B19].

## 3. Spawn

When a torch object is spawned (`sav` section 7; also when a save is loaded) [T-B3 … T-B5]:

1. `torch_definition` is read from the model's user data (section 1.2). The renderer in use selects the
   keys: with the first renderer the plain keys (`range`, `color`, `omni_range`, `omni_color`), with the
   second renderer the `_r2` keys.
2. The colour animator is the `lanims.xr` entry named by `color_animator` (`lanims-xr.md` section 4).
   The shipped value `empty` names no entry, so the torch has no colour animator [T-D5].
3. The guide bone is the model bone named by `guide_bone` (`lights_bone`).
4. **Brightness** b = (r + g + b) / 3 of `color` (r, g, b its first three values): 1.0 for the shipped
   data.
5. Main light: colour = `color` (all four values), range = `range`; cone angle = `spot_angle` × π/180;
   the projected texture name `spot_texture` is passed and has no effect in the first renderer (`DL`
   section 2.5). Shipped: colour (1, 1, 1, 0.2), range 13, cone 1.0471976 rad (60°, the record default),
   bounding radius 8.6667 (`DL` 2.3) [T-N5].
6. Second light: colour = `omni_color`, range = `omni_range`. Shipped: (1, 1, 1, 0.1), 1.5.
7. Glow: texture `glow_texture`, colour = `color` (no visible effect, `GL` section 1), radius
   `glow_radius`. Shipped: `glow\glow_torch_r2`, 0.3.
8. The torch is switched (section 4) to bit 0 of its stored torch byte (`sav` 7.4.2; section 9).
9. The night-vision request is cleared (Part B, section 15.3).
10. **Heading offset** = π/2 − atan((0.5 × r) / 0.2), r the main light's range: 0.030759 rad (1.762°) for
    the shipped range 13 [T-B5, T-N1].

The light records' updates follow `DL` section 2.2 (small changes of position, range and cone are
ignored) throughout.

## 4. Switching on and off

Switching a torch to a state s (on or off) [T-B8 … T-B11]:

1. The state becomes s.
2. **Main light**: made active (s = on) or inactive (s = off), unless the owner withholds torch lights
   (below), in which case neither light is touched.
3. **Second light**: made active or inactive likewise, unless the owner withholds torch lights or the
   owner is of the player kind. So the player's torch never lights its second light. **Disputed in review:**
   unbacked for the switch at spawn (section 3, step 8): whether the owner is already known then is not
   established; if it is not, a torch whose stored bit 0 is set (a save with the torch on) makes its second
   light active at spawn, and nothing places it (section 7.4) or makes it inactive until the torch is next
   switched off.
4. **Glow**: on or off with s, always (also when the lights are withheld; `GL` 2.2).
5. **Cone**: the bone `light_trace_bone` and all its descendants are made visible (s = on) or hidden (off).
   A child mesh none of whose bones is visible is not drawn [T-B47], so children 2 and 3 of the model (the
   light cone `lights\lights_cone_handing_lamp` and `glow\glow_Yellow`, which use only `cone_bone`) are
   drawn only while the torch is on [T-M3].

**Owners that withhold torch lights** [T-B9 … T-B11]: a character withholds them while the setting
`ai_use_torch_dynamic_lights` is off (on in the shipped `user.ltx` and presets; built-in initial value on;
the advanced video options' check box, `options-screens.md`) [T-B9, T-D7]. The player and every other kind
of owner, and a torch without an owner, never withhold them [T-B10, T-B11]. The setting is consulted when the torch is
switched and each frame for placement (section 7); changing it does not switch lights that are already
active or inactive.

A torch is switched [T-B5, T-B25, T-B30, T-B33, T-B41, T-B42, T-B48]:

- at spawn, to the stored on bit (section 3);
- for the player, by the `torch` command (section 8);
- for a character, when it is attached to or detached from the character (section 10);
- **off**, whenever it is detached from its owner (section 6);
- **off**, when it leaves its owner (dropped, given away or otherwise removed from the owner's inventory);
  this happens while the owner is still known; the night-vision request is then switched off with the
  owner still known (section 15.2) and its night-vision on, off and idle sounds are stopped
  at once (the broken sound is not) [T-B41];
- **off**, when it is destroyed (also with the night-vision request cleared) [T-B42];
- **off**, in an update without owner (section 7.1).

Entering a vehicle or a mounted weapon switches off only an attached item of class id `D_TORCH`; the
shipped torches are `TORCH_S`, so they are not switched (believed: the test compares class ids exactly)
[T-B31, T-D2].

## 5. The owner permission each frame

Sections 7.2 and 7.3 place the lights only while the owner does not withhold torch lights (section 4);
the glow is placed in every case [T-B15, T-B17].

## 6. Attachment

An item is **attached** to its owner while it hangs on one of the owner's bones. Every item is created
**enabled** for attachment [T-B21]; only scripts change that (section 10.2). The owner can attach an item
when all hold [T-B22]:

- the item is enabled;
- the item allows it: a torch whose owner is of the player kind allows it only while it is the item in its
  own slot (`slot` 9); a torch of any other owner always [T-B23];
- the item's section is in the owner's `attachable_items` list (`actor`: `device_torch`,
  `attachable_item`, `hand_radio`; the stalker sections list `device_torch` too) [T-D6];
- no attached item of the same section is attached yet;
- for a character, a further condition of the character holds (**unestablished**) [T-B30].

The owner tries to attach an item when the item is taken into its inventory and when it is put into a
slot; it detaches it when the item is moved to the backpack or dropped [T-B26]. On attaching, the item's
bone is looked up by `attach_bone_name` and its model is shown; on detaching it is told so (a torch then
switches off, section 4) and, when the owner has no attached items left, the item's model is hidden
[T-B24, T-B25].

**Transform of an attached item.** Each time the owner's bones are computed, the attached item's transform
becomes [T-B28, T-B29]

  X_item = O · B · X_owner

with B the current model-space matrix of the owner's bone `attach_bone_name`, X_owner the owner's
transform (`LO` 3.4), and O the offset: rows 0–2 the rotation below of `attach_angle_offset` (o₀, o₁, o₂),
row 3 = (`attach_position_offset`, 1). With sx = sin o₀, cx = cos o₀, sy = sin o₁, cy = cos o₁,
sz = sin o₂, cz = cos o₂:

    O[0] = ( cz·cx − sz·sx·sy,   −sz·cy,   sz·cx·sy + cz·sx )
    O[1] = ( cz·sx·sy + sz·cx,    cy·cz,   sz·sx − cz·cx·sy )
    O[2] = ( −cy·sx,               sy,     cy·cx            )

This is the rotation R of `LO` section 3.4 evaluated at the angles (o₁, o₀, o₂) [T-N2]. For
`device_torch` (0, −3.14, 1.6): O[0] = (−0.0292, 0.9996, −0.0016), O[1] = (0.9996, 0.0292, 0),
O[2] = (0, −0.0016, −1), translation (0.115, 0, 0.07) [T-N2].

Whether the player's own attached torch model is drawn in the first-person view belongs to the drawing of
the player's model and is **unestablished** here.

## 7. Placement each frame

Each frame a torch that is on is updated; one that is off is not [T-B12]. **G** is the model-space matrix
of the guide bone (`lights_bone`) of the torch's own model at rest; from the model's bind data (`LO` 4.4)
its rows are (−1, 0, 0), (0, 1, 0), (0, 0, −1) and its translation (0.0563, 0.0162, −0.1480) [T-N3].

### 7.1 Without owner

If the torch's model is visible and a second condition of the torch holds (**believed**: that it has a
physics body, i.e. lies in the world), the torch is switched off: on/off becomes off, both lights
inactive, the glow off [T-B12]. (The cone is not hidden by this path.) Otherwise nothing changes.

### 7.2 The placement frame M (any owner)

If the owner is of the player kind, its bones are marked for recomputation first [T-B13].

- **Near form** (the owner is less than 100 m from *E*, squared distance < 10,000, or the game type is not
  single player (**believed**: the game's type value 1 is single player [T-B14])): the owner's bones are
  computed (which places the attached torch, section 6) and M = G · X_torch, X_torch the torch's transform
  [T-B13].
- **Far form** (otherwise): M = the owner's transform with its translation replaced by the owner's centre
  raised along y by 2/3 of the owner's radius (**believed**: the centre and radius of the owner's bounding
  sphere) [T-B13].

### 7.3 Characters' and other owners' torches

If the owner is not of the player kind [T-B15]:

- unless the owner withholds torch lights: both lights take position row 3 of M, direction row 2 of M and,
  as their second ("right") vector, row 0 of M (`DL` 2.1);
- the glow: position row 3, direction row 2 of M (`GL` 2.2).

### 7.4 The player's torch

If the owner is of the player kind [T-B16 … T-B19]:

1. **Targets**: the heading target is −yaw and the pitch target is −pitch of the player's active camera
   (believed: the yaw and pitch of `PM` section 8).
2. **Smoothing**: the heading h and the pitch p each move toward their target T, with v_min = 0.5,
   v_max = 7.5 (rad/s), R = π/6 and the frame time *dt*:
   - h and T are first brought into [−π, π] (an angle outside it is reduced modulo 2π into [0, 2π) and
     then, if above π, has 2π subtracted); d = T − h, brought into [−π, π] by adding or subtracting 2π
     once;
   - speed s = v_min + |(v_max − v_min) / R × |d||;
   - if |d| ≥ 10⁻⁷, h moves by min(|d|, s × *dt*) in the direction of d's sign; a result below 0 has 2π
     added, one above 2π has 2π subtracted;
   - finally h is brought into [−π, π] again and e = h − T, brought into [−π, π] as d: if e < −R,
     h = h − (e + R); if e > R, h = h − (e − R) — so h stays within R of the target. The result is the
     new h.
   Examples, target stepping from 0 to 1 rad at *dt* = 0.02: 0.4764, 0.6264, 0.7363, 0.8168, 0.8758; to
   0.1 rad: 0.03674, 0.06365, 0.08337, 0.09782, 0.1 [T-N4].
3. **Direction** **g** = (−sin h′ · cos p, sin p, cos h′ · cos p) with h′ = h + the heading offset
   (section 3) [T-B18].
4. **Main light** (unless withheld; the player never withholds): position
   row 3 − 0.2 · row 0 + 0.1 · row 1 − 0.3 · row 2 of M, direction **g**, and as second vector one
   perpendicular to **g** (not used by the passes, `DL` 2.1) [T-B17].
5. **Glow**: position row 3 of M, direction **g** [T-B17].
6. The second light is not placed (it is never on, section 4) [T-B17]. **Disputed in review:** "never on"
   holds only if the owner is known when the torch is switched at spawn (see section 4).

### 7.5 Colour

While the torch is on and has a colour animator, each frame the animator's colour at the game clock in
seconds (`lanims-xr.md` section 6.2) gives the light colour (R, G, B, 1) = (red, green, blue channels) ×
b / 255, b the brightness; it is set on both lights (unless withheld) and the glow [T-B20]. The shipped
torch has no animator (section 3), so its lights keep the colours of section 3.

## 8. The player's torch command

`torch` is command 22 and `night_vision` command 23 of the command table [T-B32]; the shipped bindings
are L and N (`config\default_controls.ltx` and the default `user.ltx`; `PM` 2.1) [T-D7]. When the player
presses `torch` [T-B33 … T-B35]:

- nothing happens unless the player's health is above 0, the player is not in a vehicle or at a mounted
  weapon, and the item in the player's hands does not take the command (believed: no shipped item takes
  `torch` or `night_vision`), besides the conditions that gate all of the player's commands (not
  specified here);
- otherwise the first attached item that is a torch is toggled: switched to off if on, to on if off
  (section 4). With no attached torch (e.g. the torch moved to the backpack) nothing happens;
- the toggle happens only where this machine runs the game's server, which a single-player game does
  (believed), and not in a demo-playback state (believed) [T-B34].

## 9. Saving and loading

The torch's update data ends with the torch byte (`sav` 7.4.2) [T-B6, T-B7]: bit 0 = on, bit 1 = night
vision requested (section 15), bit 2 = the owner is of the player kind and the torch is attached to it
(settles the bit `sav` leaves **unestablished**). A torch record created without a byte has all three 0.
On loading, the torch is switched to bit 0 (section 3); bit 1 is not restored — the request is cleared at
spawn [T-B5]; bit 2 is not read back by the torch.

## 10. Characters' torches

### 10.1 The engine side

A character's torch is switched **on** when it is attached to the character and **off** when it is
detached, while the character's health is above 0 [T-B30]. The character attaches the torch when it is
taken into the inventory (section 6), so a character's torch is on from that moment until a script
disables it.

Scripts enable and disable attachment with the item method `enable_attachable_item(value)` [T-B27]:

- value false, item enabled, with an owner: the item becomes not enabled, is detached (a torch switches
  off) and its model is hidden;
- value true, item not enabled, with an owner: the item becomes enabled, the owner tries to attach it
  (section 6; a character's torch switches on) and its model is shown;
- without an owner, only the enabled flag is stored; otherwise nothing changes.

Scripted object actions `turn_on` and `turn_off` also switch a character's torch on and off [T-B48]; only
`monster_test.script` uses them, and nothing names that script, so it is never loaded (`script-loading.md`
2.2) [T-D17].

### 10.2 The scripts (`sr_light.script`)

Only `sr_light.script` enables or disables torches [T-D16]: `xr_light.script` also does but is named by
no other script, so it is never loaded; `enable.script` and `xr_comm_ptr.script` act only on `hand_radio`,
and `state_mgr_animation.script` only on the items of its attach list, which has no `device_torch`
(settles the gap of `scope`'s review).

`sr_light.check_light(character)` is called by `xr_motivator.script` alone: in the character's update
when more than 1000 ms of `time_global()` have passed since its previous call (initially 0, so believed
at the character's first update), and in the character's death callback [T-D14]. If the character has an
item `device_torch`, it decides `light` and calls `enable_attachable_item(light)` on it [T-D15]:

1. A dead character: off, decided.
2. Otherwise, for each **light zone** (below) in turn: if the zone is active and contains the character's
   position, `light` = the zone's `light_on` and the decision is made (the first such zone wins); a zone
   that does not contain the character sets `light` to off without deciding.
3. Not yet decided: on if the game-time hour (`level.get_time_hours()`, the hour of the game date
   [T-B49]; `sky.md`) is at most 4 or at least 22; else on if the level is one of `l03u_agr_underground`,
   `l03u_agr_underground_e3`, `l03u_agr_underground_hw`, `l04u_labx18`, `l08u_brainlab`, `l10u_bunker`,
   `l12u_sarcofag`, `l12u_control_monolith`, `chugai_test`; else off.
4. Not decided and on: off if the character's active logic scheme is `kamp` or `camper`.

**Light zones** are space restrictors with the restrictor scheme `sr_light` (registered in
`modules.script`), with the key `light_on` (default false); a zone is active while its scheme section
runs and stops being one when its logic switches section. The data has five, on `l05_bar`
(`bar_angar_light_restrictor`, true), `l08_yantar` (`yantar_bunker_space_restrictor`, false),
`l08u_brainlab` (`yan_attack zombies_space_restrictor`, true), `l07_military` (`mil_khutor_light_zone`,
true) and `l11_pripyat` (`pri_wave7_out_restr`, true); none on `l01_escape` [T-D12].

Which characters carry a torch follows from their specific character's `<supplies>`: 450 of the 906
specific characters of `config\gameplay\*.xml` list `device_torch` [T-D13]. Which specific character each
character of a level receives is not established here (**unestablished**).

## 11. Hand-over to the glows and lights

The glow of a torch is drawn by `GL` with the values of sections 3, 4 and 7; its lights by `DL` with the
records of sections 2–7 (main light spot, shadow on; second light point, shadow off; the shadow setting and
projected texture have no effect, `DL` 2.5).

## 12. At the first level's start

- The player's torch is the starting `device_torch` in slot 9 (`PS` 3.1, 3.3), torch byte 0: off, night
  vision not requested [T-B5, T-B6].
- It is attached to the player (enabled, in its slot, listed in `actor`'s `attachable_items`) [T-B21 …
  T-B23, T-D6].
- So its two lights are inactive, its glow is off and the cone is hidden. Pressing L switches it on: the
  main light (range 13, cone 60°, colour (1, 1, 1, 0.2)) and the glow (`glow\glow_torch_r2`, 0.3) appear,
  placed by section 7.4; the second light stays off.
- Game time is 05:30 (hour 5) [T-D18]; `l01_escape` is not an indoor level of section 10.2 and has no
  light zone [T-D12], so every check of section 10.2 on that level gives off at the start: a character's
  torch is switched off by its first check, having been on since it was attached (believed, given the
  unestablished attach condition of section 6).

## 13. Statements of earlier specifications settled here

- `scope` section 4, `GL` section 8: the torch reaches `torch_definition` through its model's user data,
  an include resolved against `config\` (section 1.2).
- `GL` 2.2 and 8: the owner-less condition (section 7.1, partly believed); the level-wide condition of
  the far form is the game type (believed single player, section 7.2); the player's heading offset
  (section 3) and the aim smoothing law (section 7.4); the owner test is of the player kind (`O_ACTOR`),
  no longer believed.
- `DL` section 3.3 and 12: the player's torch is off at the first level's start (section 12).
- `sav` 7.4.2: bit 2 of the torch byte (section 9).
- `scope`'s review gap on `enable.script`, `state_mgr_animation.script` and `xr_comm_ptr.script`: they do
  not switch torches (section 10.2).

# Part B — Torch night vision

## 14. The sounds

A torch whose section has `night_vision` on (the shipped `device_torch`) loads four sounds when its
section is loaded; a torch without it has no night vision and no sounds [T-B1, T-B2]:

| Sound | Key | Value | File |
|---|---|---|---|
| on | `snd_night_vision_on` | `weapons\nv_start` | `sounds\weapons\nv_start.ogg` |
| off | `snd_night_vision_off` | `weapons\nv_off` | `sounds\weapons\nv_off.ogg` |
| idle | `snd_night_vision_idle` | `weapons\nv_loop` | `sounds\weapons\nv_loop.ogg` |
| broken | `snd_night_vision_broken` | `weapons\nv_off` | `sounds\weapons\nv_off.ogg` |

All three files exist [T-D11].

**Loading a sound key** k [T-B43]: the lines k, k1, k2, … (the key followed by 1, 2, … without a gap) of
the section each give one **variant**; at least one is required (a fatal error otherwise). A line's value
is `name[, volume[, delay]]`: the sound name (created per `SPos` 3.1 in the **effects** category
[T-B45]), the play volume (default 1) and the delay in seconds (default 0). The shipped keys have no
numbered variants and no volume or delay [T-D1], so each sound has one variant, volume 1, delay 0. The
sounds also carry the value 0x40400000 for the reports to the AI (`SPos` 5.5; its meaning is S24's,
believed).

**Playing a sound** of the torch, looped or not [T-B44]: the sound's current emitter, if any, is stopped
at once (`SPos` 4.4); one variant is chosen at random; it is played at the owner's position (`SPos` 3.2,
"play at position"), with the variant's delay and play volume, and **2D** at position (0, 0, 0) when the
owner is the level's controlled entity (the player). **Stopping** a sound stops its emitter at once.

## 15. The night-vision mode

### 15.1 What it is

Night vision is a request of the torch (on or off, section 15.3) and, while active, a **night-vision
effect** on the player's camera: the effect of the section named by the worn outfit's `nightvision_sect`
(below); a new one is started only while none is active. What the effect shows is hud-scope spec 21's; this
section states when it starts and stops, which is also when the sounds play.

**The outfit's section.** The outfit is the item in slot 6 if it is an outfit (`PS` 3.3); its
`nightvision_sect` is the effect section (none if the key is absent) [T-B38]. In the data, 20 outfit
sections have one: 14 `effector_nightvision_bad`, 6 `effector_nightvision_good`; the starting
`novice_outfit` (an outfit, class id `E_STLK`) has none [T-D8, T-D20]. Both sections (`config\misc\postprocess.ltx`) have
`pp_eff_name = nightvision_bad.ppe` / `nightvision_good.ppe` and `pp_eff_cyclic = 1` and no camera effect
(`cam_eff_name`) [T-D9, T-B39]. The torch's own `night_vision_texture` and `night_vision_effector` are not
read (section 1.1).

### 15.2 Switching night vision to a request q

For a torch with `night_vision` on [T-B36, T-B37]:

1. The request becomes q.
2. Unless the torch's owner is of the player kind, nothing else happens.
3. The level is **disabled** if its name equals, ignoring case, one of the comma-separated items of
   `disabled_maps` (spaces around items removed). If the outfit has an effect section and the level is
   disabled: the **broken** sound plays once, and nothing else happens (for q on and off alike).
4. q = on: if no night-vision effect is active and the outfit has an effect section, the effect is started
   from that section, the **on** sound plays once and the **idle** sound starts looped. Otherwise nothing.
5. q = off: if a night-vision effect is active, it is told to stop (its ending is hud-scope spec 21's),
   the **off** sound plays once and the idle sound is stopped. Otherwise nothing.

A torch without `night_vision` ignores the request entirely (it is not even stored).

`disabled_maps` names no shipped level (`ai_test` and `ai_test2` are in `game.graph`'s level list
without level files) [T-D10], so the broken sound never plays in the shipped game.

### 15.3 What changes the request

- The player's `night_vision` command (N), under the conditions of the `torch` command (section 8: health,
  vehicle, the item in hand, server) on the first attached torch: the request is toggled (on if it was
  off, off if it was on) and section 15.2 runs [T-B33, T-B34].
- At spawn (also on loading): switched to off (section 3); so the saved bit 1 has no effect [T-B5].
- When the torch leaves its owner: switched to off with the owner still known (section 4), then the on,
  off and idle sounds are stopped at once — so the off sound started by step 5 does not continue
  (believed: it is stopped before any sound update) [T-B41].
- When the torch is destroyed: switched to off [T-B42].

Not ending it: switching the torch's light on or off, detaching the torch (moving it to the backpack) and
taking off the outfit do not change the request, the effect or the sounds; with the torch detached the
command cannot reach it (section 8) [T-B25, T-B33]. No other code starts or stops the night-vision effect;
the advanced detectors (`detector_advances`, `detector_elite`, `mp_detector_advances`, key
`night_vision_particle`) show their particles while it is active, which belongs to another specification
[T-B40, T-D19].

## 16. At the first level's start

The request is off. The worn `novice_outfit` has no `nightvision_sect` [T-D8], and `l01_escape` is not
disabled [T-D10], so pressing N toggles the request but starts no effect and plays no sound; pressing it
again finds no effect and plays nothing.

## 17. Verification facts

- `device_torch`: the values of section 1.1 [T-D1]; `torch_definition`: section 1.2 [T-D4]; the model:
  4 bones, 5 children, user data one include line [T-M1 … T-M4].
- Derived values: brightness 1.0; heading offset 0.030759 rad; cone 1.0471976 rad; main-light bounding
  radius 8.6667; offset rotation and guide-bone matrix of sections 6 and 7 [T-N1 … T-N5].
- `lanims.xr` has no entry `empty` [T-D5].
- 20 outfit sections with `nightvision_sect`, none of them `novice_outfit` [T-D8]; five light zones, none
  on `l01_escape` [T-D12]; 450 of 906 specific characters supply `device_torch` [T-D13].
- No program file reads the torch keys listed unread in section 1.1 [T-D3].

## 18. Unestablished

- The second condition of the owner-less switch-off (section 7.1; believed a physics body).
- That the game type value 1 means single player (section 7.2; believed); the owner's centre and radius of
  the far form (believed its bounding sphere).
- That the player's torch targets are the active camera's −yaw and −pitch of `PM` 8 (section 7.4;
  believed).
- Whether any item in the hands takes the `torch` or `night_vision` command; the network and demo
  conditions of the toggles (section 8; believed always met in single-player play).
- The further condition a character applies before attaching (section 6).
- Whether the player's attached torch model is drawn in first person (section 6).
- Which specific character, and so whether a torch, each character of a level receives (section 10.2).
- That the off sound of section 15.3 is never heard when a torch leaves its owner (believed).
- The use of the sounds' AI value 0x40400000 (section 14; S24's).

## 19. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`, `bin\xrSound.dll` (static analysis); every
  program file under `bin\` (string search); the default `__support\userdocs\user.ltx`.
- Data examined: the merged view of the archives' non-media files: `config\misc\items.ltx`,
  `config\misc\outfit.ltx`, `config\misc\unique_items.ltx`, `config\misc\postprocess.ltx`,
  `config\models\objects\light_night.ltx`, the `system.ltx` tree, `config\default_controls.ltx`,
  `config\rspec_*.ltx`, `config\alife.ltx`, `config\gameplay\*.xml`; `meshes\equipments\light_night.ogf`;
  `lanims.xr`; `game.graph`; `spawns\all.spawn` (custom data of the restrictors); the archive listings for
  the sound files; the shipped scripts.
- Analysis: headless disassembly and decompilation of the torch object (spawn, switching, per-frame
  update, saved state), the attachment of items to owners, the player's command handling, the night-vision
  switching and its sounds, the loading of model user data, bone visibility and the sound system's sound
  creation; reading of the shipped scripts as data; parsing of the data with throwaway tools written from
  the approved specifications and this text. The checks in `checks/torch/` reproduce every fact.

## Gaps found in review

- Whether a torch's owner is known when the torch is switched at spawn, including when a save is loaded;
  this decides whether the owner's withholding (section 4) applies then and whether the player's torch
  loaded on lights its second light.
- Whether anything outside the torch removes the night-vision effect (the player's death, a level change,
  loading a save, a removal of all camera effects) while the request stays on, and so whether the request
  and the effect can disagree beyond the cases of section 15.3.
