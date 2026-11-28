# Player character state: New Game and condition over time

Functional specification of the state of the player character of S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition) as far as a first implementation needs it: the values the character has at a New
Game (health and the other conditions, money, profile, community, inventory with the placement and
state of each item, the active slot), the configuration they come from, how the conditions change with
time while the character stands, walks, runs, sprints and jumps, and how using food and medicines
changes them. Changes caused by combat, anomalies, radiation zones, artefacts, sleeping, trading,
dialogs and scripts are out of scope (section 9).

It builds on:

- `docs/formats/sav.md` (cited as `sav`): the player's record (section 7.4.2), the object, item, weapon,
  PDA and torch layouts (7.4.1, 7.4.2), the client data (8), game time (5, 12);
- `docs/behavior/player-movement.md` (cited as `movement`): the movement request and state flags
  (section 3), the "runs" test (4.1), sprint (4.3), jump (4.4), the unable-to-move check (5, item 4), and
  its out-of-scope list (10);
- `sav` section 10.1: the player's record is object index 63 of `spawns\all.spawn`; it is the only record of
  class id `O_ACTOR` (home unestablished: `sav` 10.1 states it is the only record with section `actor`, and
  `docs/behavior/offline-simulation.md` 9.1 that it is the only `O_ACTOR` record of `l01_escape`);
- `docs/formats/ltx-config.md`: configuration files, sections, inheritance and value syntax.

Conventions:

- "Configuration" is the `system.ltx` tree (winning copies of the combined archives). Values are given
  as read with the LTX rules; a key "absent" is not present in the section or its parents.
- The **simulation** is the offline object registry that saves hold (`sav` sections 7, 14 item 2); the
  **online** character is the interactive counterpart whose state is saved as client data (`sav`
  section 8).
- Game time is `sav` section 12's; game time runs `time_factor` = 10 times faster than real time
  (`config\alife.ltx`, `sav` section 15). `dt` below is in **game seconds**.
- `clamp(x, a, b)` limits `x` to `[a, b]`.
- Statements are **static** (read from the program and the data; the game was not run). Where the order
  of events at run time was inferred rather than read, the statement is marked **believed**; what could
  not be established is marked **unestablished**.

## 1. Configuration

| File (winning copy) | Sections used |
|---|---|
| `config\creatures\actor.ltx` | `[actor]` (the section of the player's record), `[actor_condition]` (named by `[actor]` `condition_sect`) |
| `config\system.ltx` | `[inventory]` |
| `config\gameplay\npc_profile.xml`, `config\gameplay\character_desc_general.xml` | the profile `actor` and the specific character `actor` (files listed in `system.ltx` `[profiles]`) |
| `config\creatures\game_relations.ltx` | `[game_relations]` `communities` |
| `config\misc\items.ltx`, `config\misc\outfit.ltx`, `config\weapons\w_binoc.ltx`, `config\weapons\w_bolt.ltx` | the item sections of section 3 |

All condition keys are read from `[actor_condition]`:

| Key | Value | Used in |
|---|---|---|
| `stand_power` | −0.005 | 4.2 (power while not moving) |
| `walk_power` | 0.00001 | 4.2 |
| `walk_weight_power` | 0.00005 | 4.2 |
| `overweight_walk_k` | 5 | 4.2 |
| `accel_k` | 3 | 4.2 (running) |
| `sprint_k` | 75 | 4.2 (sprinting) |
| `jump_power` | 0.01 | 4.4 |
| `jump_weight_power` | 0.05 | 4.4 |
| `overweight_jump_k` | 5 | 4.4 |
| `max_power_leak_speed` | 0.0 | 4.2 |
| `satiety_v` | 0.000015 | 4.2 |
| `satiety_power_v` | 0.000055 | 4.2 |
| `satiety_health_v` | 0.0001 | 4.2 |
| `radiation_v` | 0.0001 | 4.2 |
| `radiation_health_v` | 0.004 | 4.2 |
| `psy_health_v` | 0.001 | 4.2 |
| `morale_v` | 0.0001 | 4.2 |
| `alcohol_v` | −0.0003 | 4.2 |
| `bleeding_v` | 0.0005 | 4.2 (wounds, out of scope) |
| `wound_incarnation_v` | 0.003 | wound healing (out of scope) |
| `health_restore_v` | absent → 0 | 4.2 |
| `max_walk_weight` | 60 | 4.3 |
| `limping_health_begin`, `limping_health_end` | 0.1, 0.2 | 4.3 |
| `limping_power_begin`, `limping_power_end` | 0.1, 0.2 | 4.3 |
| `cant_walk_power_begin`, `cant_walk_power_end` | 0.01, 0.10 | 4.3 |
| `cant_sprint_power_begin`, `cant_sprint_power_end` | 0.20, 0.30 | 4.3 |

The program checks at load that each `*_begin` ≤ its `*_end` (it stops with an assertion otherwise).
`satiety_critical` (0.0) is present but not read. The `*_sleep` keys apply while sleeping (out of scope).
`health_hit_part`, `power_hit_part`, `min_wound_size` and the immunity sections concern hits (out of scope).

`[inventory]`: `max_weight` = 50, `max_belt` = 5, `slot_persistent_1` … `slot_persistent_10`,
`take_dist` = 2. The inventory always has 11 slots (0 … 10); the key `slots` = 11 is not read, nor is
`max_ruck` (56) (static).

## 2. State at New Game

### 2.1 The player's record

At a New Game the simulation creates every object from its `all.spawn` record (`sav` section 10). The
player's record (object index 63, section `actor`) supplies, unchanged:

| Value | From the record | `l01_escape` value |
|---|---|---|
| health | Creature block | 1.0 |
| money | Trader-like block | 40 |
| character profile | Trader-like block | `actor` |
| model | Visual block | `actors\hero\stalker_novice` |
| position, angles, game vertex, level node | spawn packet, Object block | `sav` 10.2 |

The record's custom data is the LTX text (CR LF line ends, 99 bytes without the terminating zero):

    [dont_spawn_character_supplies]

    [spawn]
    wpn_binoc
    detector_simple
    novice_outfit
    device_torch

At a New Game the object the simulation creates for the player gets **object id 0** (static; the program
tests a further state of the simulation for this, believed always to hold at a New Game); every other
object gets an id from the simulation's id pool.

### 2.2 Profile, community, rank, reputation, name

The record's specific character is empty, community −1 and rank and reputation `0x80000001`
(−2,147,483,647): these values mean "not set". When the simulation supplies the player (section 3.1) it
resolves the specific character from the profile and fills the unset values from it:

1. The profile `actor` of `npc_profile.xml` names the specific character `actor`
   (`<specific_character>actor</specific_character>`); that becomes the specific character.
2. From the specific character `actor` of `character_desc_general.xml`: community `actor`, rank `00`,
   reputation `0`, name `actor_name`, visual `actors\hero\stalker_novice` (the same model).
3. Community: −1 is replaced by the index of `actor` in the communities of `[game_relations]`
   (`communities = actor, 0, actor_dolg, 1, …`): **0**. (Whether the index is the position in the list or
   the number after the name is **unestablished**; both are 0 for `actor`.)
4. Rank and reputation: each `0x80000001` is replaced by the specific character's value: **0** and **0**.
5. Character name: the string-table translation of `actor_name` (Russian edition:
   `text\rus\stable_bio_name.xml`, «Меченый», included through `string_table_includes.xml`).

So after a New Game the player's record holds money 40, specific character `actor`, trader flags 0,
profile `actor`, community 0, rank 0, reputation 0 and the translated name. (`[actor]` `money = 40` is
the value an object of this section has before its record is read; the record's value replaces it.)

### 2.3 Conditions at New Game

The online character starts with these condition values (its client data is empty at a New Game, `sav`
section 8; health comes from the record):

| Condition | Initial | Range | Notes |
|---|---|---|---|
| health | 1.0 (record) | −0.01 … max health | 0 or less is dead |
| max health | 1.0 | | |
| power (stamina) | 1.0 | 0 … max power | |
| max power | 1.0 | 0.1 … 1.0 | |
| satiety | 1.0 | 0 … 1 | |
| radiation | 0 | 0 … 1 | |
| psy health | 1.0 | 0 … 1 | |
| morale | 1.0 | 0 … 1 | no effect described here |
| alcohol | 0 | 0 … 1 | |
| wounds (bleeding) | none | | |
| limping, cannot walk, cannot sprint | off | | section 4.3 |

## 3. Starting inventory

### 3.1 Items created by the simulation

When the simulation creates the player at a New Game it supplies the inventory, in this order:

1. **Custom-data items.** The custom data is read as an LTX configuration (`docs/formats/ltx-config.md`).
   If it has a section `[spawn]`, each of its entries creates items, the entries taken by index, that is
   in the sorted order of `ltx-config.md` section 6 (byte-wise by key), not in file order; a key that
   appears on several lines is one entry (the last value wins). An entry is a key `<section>` with no
   value or with a `<value>`:
   - count: the integer value of the first comma-separated item of the value (0 or not a number → 1;
     no value → 1; no shipped line has a negative count);
   - `prob=<p>` anywhere in the value: probability (default 1; a value below 1e−7 in magnitude counts as
     1). Each of the count items is created when `p` is greater than a draw `r = rand15 × (1/32767)`
     computed in single precision, where `rand15` is the next value 0 … 32767 of a random generator held
     by the object being supplied (here the player's simulation object): a 32-bit state `x`, stepped as
     `x ← x · 0x343FD + 0x269EC3` (mod 2³²), giving `rand15 = (x >> 16) & 0x7FFF`; for `p` = 1 the item
     is skipped only when `rand15` = 32767 (then `r` rounds to 1.0);
   - `cond=<c>`: the item's condition (default 1.0);
   - the substrings `scope`, `silencer`, `launcher` anywhere in the value mark the corresponding addon
     installed (bits below), for a weapon whose addon status for it is 2 (attachable); otherwise not
     installed.
2. **PDA.** One `device_pda` item, always.
3. **Character supplies.** Unless the custom data has a section `[dont_spawn_character_supplies]`, the
   `<supplies>` text of the specific character (here `[spawn] \n device_torch \n`) is processed as
   in item 1. The player's custom data has that section, so the character supplies are **not** created.

For the player this gives, all with count 1 and condition 1.0, and no addons, in this order of
creation (items 1–4 are the four `[spawn]` keys in sorted order, item 5 the PDA):

| # | Section | Class id | `slot` | `inv_weight` |
|---|---|---|---|---|
| 1 | `detector_simple` | `D_SIMDET` | 8 | 0 |
| 2 | `device_torch` | `TORCH_S` | 9 | 0 |
| 3 | `novice_outfit` | `E_STLK` | 6 | 3.0 |
| 4 | `wpn_binoc` | `WP_BINOC` | 4 | 1.0 |
| 5 | `device_pda` | `D_PDA` | 7 | 0.3 |

Each item is created (its id taken from the id pool), registered with the simulation and attached to the
player before the next one is created; its condition and addons (below) are set right after its creation.

(The draw of step 1 could skip a custom-data item with a chance of 1 in 32,768 each; a reimplementation
may treat probability 1 as certain. The generator's state is seeded, when the object is constructed, from
a high-resolution timer, so the draws are not reproducible; any 15-bit generator serves.)

Each created item is a simulation object whose parent is the player (parent id 0). Its record (`sav`
sections 7.2–7.4) at creation:

Spawn packet:

| Field | Value |
|---|---|
| section | as above |
| name | the section followed by the object id written in decimal, zero-padded to at least 4 digits (e.g. `device_pda0005`) |
| the two `u8` after the name (game type and the following byte) | 1, 255 |
| position | the player's position |
| angles | (0, 0, 0) |
| respawn time | 0 |
| id | from the simulation's id pool (which ids is **unestablished**; any unused ids in 1 … 65534 serve) |
| parent id | 0 (the player) |
| phantom id | 0xFFFF |
| flags | 0 at creation; a save adds 0x20 and 0x01 (`sav` section 7.2) |
| version | 118 |
| spawn id | 0xFFFF (not created from an all.spawn record) |

Object block: game vertex and level node the player's; distance 0; direct control 1; state flags
`0xFFFFFFBF` (all bits but 0x40; bit 0x200 is set, so the items are saved, `sav` section 7.1); custom data
empty (a section's `custom_data` key would name a file whose text becomes the custom data; none of these
sections has one); story id −1; spawn story id −1.

Visual block: the section's `visual` value with everything from its last `.` removed and lowercased
(`detector_simple` `equipments\item_datchik1`, `device_torch` `equipments\light_night`, `novice_outfit`
`equipments\novice_suit`, `wpn_binoc` `weapons\binoculars\wpn_binoculars`, `device_pda`
`equipments\item_pda`); visual flags 0.

Item block: condition 1.0 (the `cond=` value; a section's `condition` key would set the default; none of
these has one).

Kind-specific state at creation:

- `wpn_binoc` (Weapon state): ammo in reserve 90 (a fixed default for every weapon), ammo in magazine =
  the section's `ammo_mag_size` (absent → 0; 0 for the binoculars, which use no ammunition), weapon state
  0, installed addons 0, ammo type 0. `scope_status = 1` is a built-in scope, not an addon. Fire mode 0.
- `device_torch`: torch byte 0 (off, night vision off, bit 2 clear).
- `device_pda`: original owner id 0 (the player), specific character `actor`, info portion empty.
- `detector_simple`, `novice_outfit`: no further state.

Installed-addon bits (the Weapon state's addon byte) set by the words of a supply line (step 1):
`scope` 0x01, `launcher` 0x02, `silencer` 0x04, each only for a weapon whose status for that addon
(`scope_status`, `grenade_launcher_status`, `silencer_status`) is 2.

Update packets: the Item update's first byte `f` (physics elements) of a freshly created item is
**unestablished** (the value is not set when the item is created; it is the online item's physics state
when a save collects it, believed 0 for items in an inventory). With `f` = 0: the outfit's update adds
`q8(condition)` = 255; the torch's update adds the torch byte 0; the binoculars' Weapon update has
`q8(condition)` = 255, weapon flags 0, ammo in magazine 0, addons 0, ammo type 0, weapon state 0, zoom
**unestablished** (not set at creation), then fire mode 0.

Attaching an item to the player appends its id to the end of the player's list of children (static). So
these five items are the player's children in the order of creation above: `detector_simple`,
`device_torch`, `novice_outfit`, `wpn_binoc`, `device_pda`. A save written before any item is added
to or removed from the inventory (the bolt of section 3.2 aside: it comes after them and is never saved)
writes them right after the player's record in this order (`sav` section 7.1, children in the order the
parent lists them).

### 3.2 The bolt

The online player character also receives one **bolt** (section `bolt`, class id `II_BOLT`, `slot` 5,
`inv_weight` 0) in single-player games: each time the player character comes online (New Game and after
loading a save), the game requests a `bolt` item with the player as parent. Bolts are never saved (`sav`
section 7.1). (Custom-data items and the PDA are created by the online character only when there is no
simulation; in single-player they come from section 3.1.)

### 3.3 Placement: slots, belt, backpack

The inventory has three places: the 11 **slots** (each holds one item), the **belt** (at most
`max_belt` = 5 items) and the **backpack** (unlimited count).

An item taken into the player's inventory goes, unless a stored place applies (section 6):

1. if its section has a `slot` key, the slot with that number exists (0 … 10) and is empty: to **that
   slot**;
2. else, if its section has `belt = true`, its `default_to_ruck` is false and the belt is not full: to the
   **belt** (the program makes one further test before putting an item on the belt, **unestablished**;
   no starting item reaches this step);
3. else to the **backpack**.

`default_to_ruck` is true when absent. Food and drink (class ids `II_FOOD`, `II_BOTTL`) always go to the
backpack when the player takes them, even though their sections have `slot = 4`; medkits, bandages and
antiradiation drugs have no `slot` key.

Slot numbers, with the `[inventory]` comments that name them and their `slot_persistent_<n+1>` values:

| Slot | Name (comment) | Persistent | Can be active |
|---|---|---|---|
| 0 | knife | true | yes |
| 1 | pistol | false | yes |
| 2 | automatic | false | yes |
| 3 | grenades | true | yes |
| 4 | binocular | true | yes |
| 5 | bolt | true | yes |
| 6 | outfit | false | no |
| 7 | pda | true | no |
| 8 | detector | true | no |
| 9 | torch | true | no |
| 10 | — | false (no key) | yes |

"Can be active": slots 6, 7, 8 and 9 can never be the active slot; the others can. (The meaning of
"persistent" is not used here.)

So at a New Game every starting item is in a slot: binoculars 4, bolt 5, outfit 6 (worn), PDA 7, detector
8, torch 9. The belt and backpack are empty.

**Active slot.** The inventory has an active slot (none at first). When an item is put into a slot that
can be active while there is no active slot and none is pending, that slot becomes the pending active
slot: the item is then brought into the hands. At a New Game the binoculars are the first such item:
the items created before them (detector, torch, outfit) go to slots 8, 9 and 6, which cannot be active,
and the PDA (slot 7) and the bolt come after them (static, given that the online character takes the
simulation's children in the order of section 3.1 and before the bolt, which is **believed**), so slot 4
becomes the active slot. Level scripts may hide the item in hand during the opening scene (out of
scope).

### 3.4 Carried weight

The carried weight `M` is the sum of `inv_weight` over every item in the inventory, in any place,
including the worn outfit (a weapon also counts its installed addons and the rounds in its magazine;
none here). At a New Game `M` = 1.0 + 0 + 3.0 + 0 + 0.3 + 0 = **4.3**.

Two limits use it:

- the carry limit `W` = `[inventory]` `max_weight` (50) plus the worn outfit's
  `additional_inventory_weight2`, used by the max-power leak (section 4.2 item 2) and the jump cost
  (section 4.4);
- the walking limit = `max_walk_weight` (60) plus the worn outfit's `additional_inventory_weight`
  (section 4.3).

`novice_outfit` has both additional weights 0 and no `power_loss` key (factor 1.0).

## 4. Conditions over time

### 4.1 When the conditions update

The conditions update repeatedly while the player character is alive (health > 0) and online. Each
update uses `dt` = the game time elapsed since the previous update, in seconds (game milliseconds ×
0.001); the first update after the character comes online has `dt` = 0. The rates below are per game
second, so the result does not depend on how often the update runs (apart from where a clamp or
threshold is crossed within one update; the update interval is **unestablished**; updating every frame
is suitable). With `time_factor` 10, one real second is 10 game seconds.

A flag among the player settings that also hold `g_always_run` (`movement` section 1.2) disables the
update in single-player; no console command of this edition sets that flag, so it is always clear here.
The same flag disables the weight test of section 4.3 and the jump cost of section 4.4.

### 4.2 One update

With `dt` as above, `w = M / 50` (carried weight over `max_weight`, without outfit bonus), the movement
state of `movement` section 3 as left by the latest movement update, and the worn outfit's `power_loss`
`L` (1.0 when absent, when no outfit is worn, or when `power_loss` < 1 and the outfit's condition is 0 or
less), each update does, in this order:

1. **Power from movement.** If the state has none of forward, back, left, right:
   `power ← power − stand_power·dt` (i.e. +0.005·dt). Otherwise

       power ← power − dt · K · (walk_power + walk_weight_power · O · w) · L
       O = overweight_walk_k if w > 1, else 1
       K = 1 if the state does not run; accel_k if it runs and does not sprint; sprint_k if it runs and sprints

   where "runs" is `movement` section 4.1's test without the ability part and "sprints" is the state's
   sprint flag.
2. **Max power.** `max power ← clamp(max power − (min(M, W)/W + max(0, (M − W)·0.1) + 1) ·
   max_power_leak_speed · dt, 0.1, 1)` with `W` = carry limit. With `max_power_leak_speed` = 0 this only
   keeps max power in 0.1 … 1.
3. **Alcohol.** `alcohol ← clamp(alcohol + alcohol_v·dt, 0, 1)` (falls with time; drinks raise it,
   section 5).
4. **Psy health at zero kills.** If psy health is 0 (within 1e−7), health is set to 0.
5. **Satiety.** If satiety > 0: `satiety ← clamp(satiety − satiety_v·dt, 0, 1)`.
6. **Health from satiety.** If the character was not bleeding at the previous update (always, without
   wounds) and can be harmed (below): health change `+ satiety_health_v·dt` if satiety > 0, else
   `− satiety_health_v·dt`.
7. **Power from satiety.** power change `+ satiety_power_v·dt` (always, whatever the satiety).
8. Items 8–13 run only if health is above 0 at this point (after item 4 has set it to 0 nothing further
   happens in this update, the clamps included). If health with the changes so far is 0 or less, the
   character dies (death is out of scope).
9. **Bleeding.** health change `− (total bleeding of the wounds) · bleeding_v · dt` and
   `+ health_restore_v · dt` (0); the character counts as bleeding when the first term is at least 1e−7.
   Without wounds this does nothing.
10. **Radiation.** If radiation > 0: radiation change `− radiation_v·dt`, and health change
    `− radiation_health_v · dt · radiation`.
11. **Psy health.** If psy health > 0: psy health change `+ psy_health_v·dt`.
12. **Morale.** If morale < 1 (its maximum): morale change `+ morale_v·dt`.
13. **Apply and clamp.** All changes are added, then: health to −0.01 … max health, power to 0 … max power,
    radiation to 0 … 1, morale to 0 … 1, psy health to 0 … 1.

Items 2, 4, 5, 6 and 7 run only in the single-player game type (like the weight test of section 4.3);
in other game types they are skipped.

Items 6, 9 (its first term) and 10 (its health term) change health only while the character can be
harmed; in a single-player game it can (static: a flag set when the character is created; what clears it
is out of scope).

Satiety and alcohol change immediately (items 3, 5); the other changes accumulate and are applied in
item 13, together with changes made by eating since the previous update (section 5).

Consequences with the New Game values (`M` = 4.3, `w` = 0.086, `L` = 1), per game second:

| Activity | Power change |
|---|---|
| standing still | +0.005 + 0.000055 = +0.005055 |
| walking | −0.0000143 + 0.000055 = +0.0000407 |
| running | −0.0000429 + 0.000055 = +0.0000121 |
| sprinting | −0.0010725 + 0.000055 = −0.0010175 |

Satiety falls from 1 to 0 in 66,666.7 game seconds (18.5 game hours, 1.85 real hours); after that health
falls by 0.0001 per game second (from 1 to 0 in 10,000 game seconds, 16.7 real minutes), and while
satiety is above 0 health rises by the same rate up to max health.

### 4.3 Thresholds that limit movement

These states change with hysteresis; each is re-evaluated when the movement asks for it:

| State | Becomes on when | Becomes off when | Effect |
|---|---|---|---|
| limping | power < 0.1 or health < 0.1 | power > 0.2 and health > 0.2 | the character cannot run (`movement` section 4.1, ability to run) |
| cannot walk | power < 0.01 | power > 0.10 | unable to move (`movement` section 5 item 4, first condition) |
| cannot sprint | power < 0.20 | power > 0.30 | cannot sprint (`movement` section 4.3, "the character's condition allows sprinting") |

Between the two thresholds a state keeps its previous value. ("On when" and "off when" use strict
comparisons: power exactly 0.2 does not end limping.) `[actor_condition]` `use_limping_state` (on) is read but
belongs to the limping test of other characters; the player's limping test above does not use it.

The second unable-to-move condition of `movement` section 5 item 4, "carried weight too high", holds
when `M` > `max_walk_weight` + the worn outfit's `additional_inventory_weight` (60 with the novice outfit);
it is tested only in single-player. The third condition holds while the character is
in a conversation (talking to another character).

Sprinting from full power (New Game values) lasts 0.8 / 0.0010175 = 786.2 game seconds (78.6 real
seconds) until power falls below 0.20; standing still then restores power above 0.30 in 19.8 game seconds.

The bobbing table's "limping" row of `movement` section 9.1 is believed to use the limping state above.

### 4.4 Jumping

At the start of each jump (`movement` section 4.4):

    power ← power − (jump_power + jump_weight_power · O · w_j) · L
    w_j = M / (max_weight + the worn outfit's additional_inventory_weight2)
    O = overweight_jump_k if w_j > 1, else 1

With the New Game values: 0.01 + 0.05 × 0.086 = 0.0143 per jump. (This change is direct and is clamped at
the next update.)

## 5. Eating and medicines

Food, drink, medkits, bandages and antiradiation drugs are **eatable** items: their sections have the
keys below (all read with no default except `eat_max_power`, absent → 0; `eat_alcohol` is read only for
drinks, absent → 0; "—" in the table means absent).

When the player uses an eatable item:

1. health change `+ eat_health` (a negative value applies only while the character can be harmed);
2. power change `+ eat_power`;
3. satiety `← clamp(satiety + eat_satiety, 0, 1)` immediately;
4. radiation change `+ eat_radiation`;
5. wounds are healed by `wounds_heal_perc` (clamped to 0 … 1 at load; no wounds without combat);
6. max power `← clamp(max power + eat_max_power, 0.1, 1)` immediately;
7. for a drink (class id `II_BOTTL`), after the above: alcohol `+ eat_alcohol` (absent → 0) immediately;
   the clamp to 0 … 1 happens at the next update (section 4.2 item 3);
8. the item's remaining portions (set from `eat_portions_num` when the online item is set up) decrease by 1
   if at least 1, else become 0; when 0 remain the item is removed from the inventory and destroyed.

The health, power and radiation changes are applied at the next condition update (section 4.2 item 13),
with its clamps. Every shipped eatable has `eat_portions_num` 1 or −1, so each is used up at once.

Eatables of the configuration (single-player sections; values as read):

| Section | Class id | `eat_health` | `eat_power` | `eat_satiety` | `eat_radiation` | `wounds_heal_perc` | `eat_portions_num` | `eat_max_power` | `eat_alcohol` |
|---|---|---|---|---|---|---|---|---|---|
| `bread` | `II_FOOD` | 0.05 | 0 | 0.2 | 0 | 0 | −1 | — | — |
| `kolbasa` | `II_FOOD` | 0.1 | 0 | 0.3 | 0 | 0 | −1 | — | — |
| `conserva` | `II_FOOD` | 0.15 | 0 | 0.4 | 0 | 0 | −1 | — | — |
| `vodka` | `II_BOTTL` | 0.01 | 0 | 0.01 | −0.3 | 0 | −1 | — | 0.1 |
| `energy_drink` | `II_BOTTL` | 0 | 1.0 | 0 | 0 | 0 | 1 | 0.2 | 0 |
| `medkit` | `II_MEDKI` | 0.6 | 0.0 | 0 | −0.1 | 0.3 | 1 | 0.1 | — |
| `medkit_army` | `II_MEDKI` | 1.0 | 0.0 | 0 | −0.1 | 1.0 | 1 | 0.1 | — |
| `medkit_scientic` | `II_MEDKI` | 1.0 | 0.0 | 0 | −1.0 | 0.3 | 1 | 0.1 | — |
| `bandage` | `II_BANDG` | 0.1 | 0.0 | 0 | 0.0 | 1.0 | 1 | — | — |
| `antirad` | `II_ANTIR` | 0 | 0 | 0 | −1.0 | 0 | 1 | — | — |

(Also eatable: `mp_medkit`, `mp_antirad` (multiplayer) and the rat section `m_rat_e`.) How the player
chooses to use an item (inventory menu, quick-use) is interface, out of scope.

## 6. What a save holds

Where each value of this specification is stored in a save (`sav` sections 7.4.2 and 8):

| Value | Save field |
|---|---|
| health | player record: Creature block health and Creature update health |
| money, specific character, profile, community, rank, reputation, name | player record: Trader-like block |
| radiation | client data condition block (below); the player update block's radiation field is believed to carry the same value |
| power, morale, psy health | client data condition block |
| alcohol, satiety | client data player condition block |
| max power | not saved: the online character starts with 1.0 whenever it comes online, after loading a save too |
| wounds | client data condition block (wound records, out of scope) |
| inventory items | the children records after the player's record; condition in each Item block, weapon and torch state in their blocks |
| item place | client data item block of each item |
| active slot | client data inventory-owner block (`sav` section 8: believed its `u8`, 0xFF none) |

The client data blocks (`sav` section 8) are, for the player:

- condition block: `bool8` alive (health > 0); if alive: `float` power, `float` radiation, `float`
  morale, `float` psy health (in this order), `u8` wound count, wound records;
- player condition block: the condition block, then `float` alcohol, `u16` flags (bits of tutorial hints
  already shown and of the over-weight state; out of scope; 0 at a New Game), `float` satiety;
- item block: its first byte is the item's place, 1 slot, 2 belt, 3 backpack (0 none); when the online
  player takes an item whose place is set, it goes to that place instead of following section 3.3 (food
  and drink are set to 3 first, section 3.3). If the stored place is a slot that is not available (no
  `slot` key, or occupied), the item is in the inventory but in no place (it is not sent to the
  backpack).

At a New Game the client data is empty and the online character starts from section 2.3; health is
always taken from the record.

## 7. Verification facts

From the game data (`spawns\all.spawn`, `config\`):

- `all.spawn` has exactly one record of class id `O_ACTOR`: object index 63, section `actor`, name
  `level_prefix_actor_0001`, parent 0xFFFF. No all.spawn record has a parent other than 0xFFFF, so the
  starting inventory comes only from section 3.
- The player's record: health 1.0; money 40; specific character empty; trader flags 0; profile `actor`;
  community −1; rank and reputation −2,147,483,647 (`01 00 00 80`); character name empty; update health
  1.0, radiation 0.0. Custom data 99 bytes, sections `dont_spawn_character_supplies` (no keys) and
  `spawn` with four keys and no values, in file order `wpn_binoc`, `detector_simple`, `novice_outfit`,
  `device_torch`, enumerated by index (sorted) as `detector_simple`, `device_torch`, `novice_outfit`,
  `wpn_binoc`.
- Specific character `actor`: `<supplies>` `[spawn] \n device_torch \n`, `<community>actor</community>`,
  `<rank>00</rank>`, `<reputation>0</reputation>`, `<visual>actors\hero\stalker_novice</visual>`;
  profile `actor`: `<specific_character>actor</specific_character>`.
- Starting inventory (section 3), in order of creation and as the player's children:
  `detector_simple` ×1 (slot 8), `device_torch` ×1 (slot 9), `novice_outfit` ×1 (slot 6), `wpn_binoc` ×1
  (slot 4), `device_pda` ×1 (slot 7), all condition 1.0; plus `bolt` ×1 (slot 5, online only). Six distinct slots; belt and backpack empty; carried weight 4.3;
  active slot 4.
- Item keys: `detector_simple` `default_to_ruck = false`; the other five have no `default_to_ruck`
  (true) and none has `belt`. `novice_outfit`: `additional_inventory_weight 0`,
  `additional_inventory_weight2 0`, no `power_loss`.
- `wpn_binoc`: `ammo_mag_size 0`, `scope_status 1`, `silencer_status 0`,
  `grenade_launcher_status 0`; all 34 weapon records of all.spawn have ammo in reserve 90 and weapon state
  0, magazine 0 (32) or 1 (2, `wpn_rpg7`).
- `[actor_condition]` and `[inventory]` values as in section 1; `[game_relations]` `communities` starts
  `actor, 0`.
- Derived (section 4): power per game second +0.005055 standing, +0.0000407 walking, +0.0000121 running,
  −0.0010175 sprinting; jump cost 0.0143; sprint endurance 786.2 game s from power 1 to below 0.2;
  satiety 1 → 0 in 66,666.7 game s.
- `vodka` used at New Game values: alcohol 0.1 at once, 0.1 − 0.0003 × `dt` after the next update.
- Medkit used at New Game values, then one update with `dt` = 0: health 1.0 (clamped), radiation 0
  (clamped), max power 1.0, satiety 1.0; the medkit is removed.

Invariants:

- Each `*_begin` ≤ `*_end` in `[actor_condition]`.
- Every section named in the player's custom data `[spawn]` exists and has a `slot` key; their slots and
  `device_pda`'s are distinct, so all go to slots.
- Every eatable section of the table has `eat_portions_num` ≤ 1.
- After each update health is within −0.01 … 1, power within 0 … max power, max power within 0.1 … 1,
  satiety, radiation, psy health and alcohol within 0 … 1.
- Standing still never lowers power; with the shipped values, walking and running at New Game weight do
  not lower it either (net positive); only sprinting and jumping do.

## 8. Unestablished

- Which object ids the created items get (section 3.1).
- The interval of the condition update (section 4.1).
- The further simulation state tested before the player gets id 0 (section 2.1; believed always to hold
  at a New Game).
- That the online player takes the simulation's children in their order and before the bolt (section
  3.3, active slot: believed).
- Whether a community index is the list position or the listed number (section 2.2).
- That the player update block's radiation field equals the condition's radiation (section 6: believed).
- That the bobbing's limping row uses the limping state of section 4.3 (believed).
- Wound records, bleeding amounts and wound healing (combat).
- The Item update's physics byte `f` and the weapon update's zoom byte of a freshly created item (section
  3.1).
- The further test before an item is put on the belt (section 3.3).

## 9. Out of scope

Hits and wounds (weapons, monsters, falls, anomalies, radiation zones, burns), immunities and outfit
protections, death, artefacts on the belt (they change the conditions over time), sleeping and the
`*_sleep` rates, psy effects (`psy_health` < 1 shows a screen
effect), the tutorial hint callbacks to scripts when conditions cross the
`[tutorial_conditions_thresholds]`, trading and dialogs that give or take items and money, rank changes,
scripted invulnerability, and the interface showing these values.

## 10. Provenance

- Date: 2026-11-24 (revised the same day: created item records, drinks' alcohol, addon bits, weapon
  magazine at creation, the random generator, belt and stored-place details; then the order of the
  custom-data items and of the player's children, the single-player-only steps, max power on load).
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); archives `gamedata.db0` …
  `gamedata.dbd`; binary `bin\xrGame.dll`.
- Data examined: `spawns\all.spawn` (the player's record and every weapon record, parsed with the
  layouts of `sav`), `config\creatures\actor.ltx`, `config\system.ltx` (`[inventory]`), the item, outfit
  and weapon sections of the `system.ltx` tree, `config\creatures\game_relations.ltx`,
  `config\gameplay\npc_profile.xml`, `config\gameplay\character_desc_general.xml`,
  `config\text\rus\stable_bio_name.xml`, `config\text\rus\string_table_includes.xml`,
  `config\localization.ltx`, `scripts\bind_stalker.script`, `scripts\xr_effects.script`.
- Analysis: static analysis of `xrGame.dll` (disassembly and decompilation with Ghidra) of the creation of
  objects at a New Game and of items for an inventory, the supply of an inventory from custom data and
  from the specific character, the resolution of the specific character, the online character's own
  supply, the reading of the condition, inventory, item, eatable and outfit configuration, the condition
  update and its time base, the limping and unable-to-walk and unable-to-sprint tests, the jump cost, the
  inventory's placement and activation rules, the carried weight, the use of eatable items and drinks,
  the construction and the state and update writing of created simulation objects, items, weapons and
  torches, their attachment to the parent's list of children, the per-object random generator, and the client-data condition and item blocks; configuration values were read from a fresh extraction of the winning
  copies with an independent LTX reader, which also recomputed the derived numbers of section 7 and checked
  every fact of section 7. The program was not executed.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- The uniqueness of the `O_ACTOR` record over all of `spawns\all.spawn` (builds-on list) has no home in an approved
  specification; its citation is marked home unestablished.
