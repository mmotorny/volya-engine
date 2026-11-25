# Hits on the player character

Functional specification of what a hit of a given type and power does to the player character of
S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) in a single-player game: the immunities chosen by the
game difficulty, the worn outfit's protection and its loss of condition, the changes of health, power,
radiation and psy health, the wounds a hit opens, bleeding and the healing of wounds, and what sets and
clears the "can be harmed" flag. What makes hits (weapons, monsters, falls, anomalies, radiation zones,
scripts) is not specified here: a hit arrives with the values of section 2.

It settles, of `docs/behavior/player-state.md` (cited as `state`): the unestablished item "wound records,
bleeding amounts and wound healing" of section 8; the out-of-scope items "hits and wounds" and "immunities
and outfit protections" of section 9, as far as applying a hit goes; what clears the "can be harmed" flag of
section 4.2; and the wound healing of eating (section 5 item 5).

It builds on:

- `state`: the condition values and their ranges (section 2.3), the condition update (section 4.2), eating
  (section 5), what a save holds (section 6), and the flag of section 4.1 that disables the condition update;
- `docs/formats/sav.md` (cited as `sav`): the client data condition block (section 8), `q8` (section 7.4.1),
  the Item block and outfit update (section 7.4.2);
- `docs/behavior/player-movement.md` (cited as `movement`): the movement request and state (section 3), the
  movement time `dt_m` (section 3.2), sprint (section 4.3), the desired acceleration (section 5);
- `docs/behavior/inventory-screen.md` (cited as `inventory`): the outfit protection lines (section 4.8);
- `docs/behavior/options-screens.md` (cited as `options`): the variable `g_game_difficulty` (section 7) and
  its effect during a game (section 14);
- `docs/formats/ltx-config.md`: configuration files, sections, inheritance and values.

Conventions:

- "Configuration" is the `system.ltx` tree (winning copies of the combined archives).
- The **worn outfit** is the item in inventory slot 6 (`state` section 3.3); its **condition** `c` is its
  item condition (0 … 1).
- `clamp(x, a, b)` limits `x` to `[a, b]`.
- Statements are **static** (read from the program and the data; the game was not run). Where the order of
  events at run time was inferred rather than read, the statement is marked **believed**; what could not be
  established is marked **unestablished**.
- Bracketed ids are the checks that reproduce the statement next to them: `D…` in `data.py` (configuration),
  `B…` in `binary.py` (the program), `S…` in `scripts.py` (shipped scripts), `M…` in `model.py` (numbers
  computed from this text). All are in the dirty workspace's `checks/player-hits/`; `run.py` runs them.

## 1. Configuration

| File (winning copy) | Sections and keys used |
|---|---|
| `config\creatures\actor.ltx` | `[actor]`: `immunities_sect`, `damage`, `hit_probability_gd_novice` … `_gd_master`; the four sections `[actor_immunities_gd_novice]` … `[actor_immunities_gd_master]`; `[actor_condition]`: `health_hit_part`, `power_hit_part`, `min_wound_size`, `bleeding_v`, `wound_incarnation_v`; `[actor_damage]` |
| `config\misc\outfit.ltx` | the outfit sections (`slot = 6`) and their immunity sections |
| `config\creatures\damages.ltx` | the bone protection sections named by the outfits' `bones_koeff_protection` |
| eatable sections (`state` section 5) | `wounds_heal_perc` |

Values [D1, D3]:

| Key | Value | Used in |
|---|---|---|
| `[actor]` `immunities_sect` | `actor_immunities_gd_master` | 3 |
| `[actor]` `damage` | `actor_damage` | 4.3 |
| `[actor]` `hit_probability_gd_novice`, `_gd_stalker`, `_gd_veteran`, `_gd_master` | 0.20, 0.30, 0.40, 0.50 | 3 (read only) |
| `[actor_condition]` `health_hit_part` (`H`) | 1.0 | 4.5 |
| `[actor_condition]` `power_hit_part` (`Q`) | 0.1 | 4.5 |
| `[actor_condition]` `min_wound_size` | 0.0256 | 5.3 |
| `[actor_condition]` `bleeding_v` | 0.0005 | 5.2 |
| `[actor_condition]` `wound_incarnation_v` | 0.003 | 5.3 |

The four keys of `[actor_condition]` named in this section are read without a default; `health_restore_v`
is absent (0, `state` section 4.2) [B20].

## 2. The hit

A hit carries [B1, B13]:

- a **type** `t`, a number 0 … 10 (table below);
- a **power** `P` (a real number in the units of the conditions: health 1.0 is full health);
- a **bone** `b` of the player's model, or none;
- an **armour piercing** value `ap`, used only by `fire_wound` hits (section 4.4);
- its **source** (the object that hit), used only by effects outside this specification (section 7).

Hit types, with the names the configuration and the scripts use for them [B1]:

| `t` | Name | Player immunity key | Outfit protection key | Outfit immunity key |
|---|---|---|---|---|
| 0 | `burn` | `burn_immunity` | `burn_protection` | `burn_immunity` |
| 1 | `shock` | `shock_immunity` | `shock_protection` | `shock_immunity` |
| 2 | `strike` | `strike_immunity` | `strike_protection` | `strike_immunity` |
| 3 | `wound` | `wound_immunity` | `wound_protection` | `wound_immunity` |
| 4 | `radiation` | `radiation_immunity` | `radiation_protection` | `radiation_immunity` |
| 5 | `telepatic` | `telepatic_immunity` | `telepatic_protection` | `telepatic_immunity` |
| 6 | `chemical_burn` | `chemical_burn_immunity` | `chemical_burn_protection` | `chemical_burn_immunity` |
| 7 | `explosion` | `explosion_immunity` | `explosion_protection` | `explosion_immunity` |
| 8 | `fire_wound` | `fire_wound_immunity` | `fire_wound_protection` | `fire_wound_immunity` |
| 9 | `wound_2` | — | — | — |
| 10 | `physic_strike` | `physic_strike_wound_immunity` (absent → 1.0) | `physic_strike_protection` (absent → 0) | `physic_strike_wound_immunity` (absent → 1.0) |

Every key of the table except the two marked "absent →" is read without a default (a missing key or
section is an error) [B3, B10]. A hit of type 9 is treated as type 3 from section 4.2 on, so `wound_2` needs
no keys [B6]. A type outside 0 … 10 is reported as an error ("Unknown/unregistered hit type") [B5].

## 3. The player's immunities and the difficulty

The player character has one **immunity coefficient** `I_t` per type: a factor on the hits of that type
(section 4.5). All eleven start at 1.0 [B3]. They are then read, with the keys of section 2, from a section
of `actor.ltx` [B2, B4]:

1. when the online character is set up from its configuration section `actor`: first from the section named
   by `immunities_sect` (`actor_immunities_gd_master`); then, in a single-player game, from
   `actor_immunities_<d>`, where `<d>` is the current value of `g_game_difficulty` (`gd_novice`,
   `gd_stalker`, `gd_veteran` or `gd_master`). So in single-player the difficulty always decides. (That the
   set-up happens each time the character comes online, at a New Game and after loading a save, is
   **believed**.)
2. whenever `g_game_difficulty` is executed while a single-player game is running (`options` section 14):
   at once, from `actor_immunities_<d>` of the new value. In another game type it prints "For this game type
   difficulty level is disabled." and the immunities stay.
3. whenever a script sets the difficulty (the script function `set_game_difficulty`): the same as item 2.

`I_9` (`wound_2`) is never read and stays 1.0; it is never used (section 2) [B3]. `I_10` is 1.0 because no
`actor_immunities_*` section has `physic_strike_wound_immunity` [D2]. At the same moments the value
`hit_probability_<d>` of `[actor]` is read; it does not take part in applying a hit (it belongs to what
makes hits, out of scope) [B2].

The shipped script that saves the player's script data writes the difficulty into each save and, when the
save is loaded, executes `g_game_difficulty` with the saved value, so the immunities of the saved difficulty
apply again after loading (`scripts\bind_stalker.script`, the player's script binder: it writes the
difficulty number plus 128 as a `u8`, and on load maps 0 … 3 to `gd_novice` … `gd_master`) [S1].

The immunity sections [D2]:

| Section | `burn`, `strike`, `shock`, `wound`, `radiation`, `telepatic`, `chemical_burn`, `fire_wound` | `explosion` |
|---|---|---|
| `actor_immunities_gd_novice` | 0.5 | 0.1 |
| `actor_immunities_gd_stalker` | 0.65 | 0.3 |
| `actor_immunities_gd_veteran` | 0.8 | 0.6 |
| `actor_immunities_gd_master` | 1.0 | 1.0 |

A lower coefficient means less effect: at `gd_novice` an explosion does a tenth of what it does at
`gd_master`. `radiation_immunity` is read but radiation hits do not use it (section 4.5).

## 4. Applying a hit

In a single-player game a hit `(t, P, b, ap)` on the player character is applied in this order. Items 4.1
to 4.6 happen at the moment of the hit; the changes of the conditions they make take effect at the next
condition update (section 4.7). Multiplayer differences are not specified.

### 4.1 Movement effects

1. **Sprint.** If the movement state sprints (`movement` section 3) and `t` is neither 4 (`radiation`) nor
   5 (`telepatic`), the sprint request is cleared: sprinting ends and must be toggled on again (`movement`
   section 4.3) [B5]. (The program makes this test only for the character the player controls, which the
   player character is outside scripted control: **believed**.)
2. **Slowdown.** The player character keeps a **slowdown** value `s` (0 when it comes online, not saved)
   [B7]. A hit of type 2 (`strike`) or 3 (`wound`) sets `s = clamp(P, 0, 1)` with the power as it arrives;
   a hit of any other type, `wound_2` included, sets `s = 0` [B5]. On each movement update,
   `s ← max(0, s − dt_m)` and then the desired acceleration `a` (`movement` section 5) is multiplied by
   `(1 − s)` before the character physics uses it [B7]. So a strike or wound hit of power 0.5 slows the
   player for 0.5 s, from half the acceleration back to full.

### 4.2 Power before the conditions

1. **Belt.** `P1 = P × B_t`, where `B_t` is the factor of the artefacts on the belt for type `t` (`t` as the
   hit arrived, before item 3). With an empty belt `B_t = 1` [B8]. How artefacts give `B_t` is not specified
   here (artefacts on the belt).
2. **Disabling flag.** If the flag of `state` section 4.1 is set, `P1 = 0` and nothing of sections 4.3 to
   4.6 happens [B9]. No console command of this edition sets that flag [B15], so this never applies.
3. **Wound type.** A type 9 (`wound_2`) becomes type 3 (`wound`) for the rest of the hit [B6].

### 4.3 Bone factors

The player's damage table `[actor_damage]` (named by `[actor]` `damage`) gives each bone of the player's
model two factors: the **hit scale** `h` and the **wound scale** `w` [B11, D4]:

- an entry `<bone name> = <h>, <animation>, <w>` sets them for that bone; each item is read as a decimal
  number; the middle number does not take part in applying a hit (the file's comment calls it a hit
  animation index), and a fourth number, where an entry has one, is not used for the player;
- the entry `default = <h>, <animation>, <w>` sets them for every bone without its own entry; without a
  `default` entry both are 1.0;
- the hit's bone selects its factors; a hit with no bone uses the `default` values;
- the table is read whenever the player's model is set (the model changes with the worn outfit, section
  4.4) [B11]; a bone name the model lacks is an error, and the model's first bone must not have `h` or `w`
  0 (an error otherwise). Without a `damage` key both factors are 1.0 for every bone [B11].

`[actor_damage]` has 45 keys [D4]: `default = 1.0, -1, 1.0`; the trunk, neck, arm and leg bones
(`bip01_pelvis`, `bip01_spine`, `bip01_spine1`, `bip01_spine2`, `bip01_neck`, the clavicles, upper arms,
forearms, thighs, calves, feet and toes) `h` 1.0, `w` 1.5; `bip01_head` `h` 0.2, `w` 1.5; `bip01_l_hand`
`h` 1.0, `w` 0.5; `bip01_r_hand` `h` 1.0, `w` 0.0; the fingers, `eyelid_1`, `eye_left`, `eye_right` and
`jaw_1` `h` 0.0, `w` 0.0.

### 4.4 The worn outfit

If the player character wears an outfit, it changes the power `P1` into `P2` and loses condition; without a
worn outfit `P2 = P1` [B10].

Each outfit section has the protections `p_t` (keys of section 2, `p_9` is 1.0 and unused), the section
`immunities_sect` naming its own immunity coefficients `m_t` (keys of section 2), and the section
`bones_koeff_protection` naming its **bone protection table** [B10]. The bone protection table gives:

- `hit_fraction` `F` (absent → 0.1);
- the default entry: `K` = 1.0, `A` = 0, unless the table has a key `default = <K>, <A>, …`;
- for each other key `<bone name> = <K>, <A>[, <n>]`: that bone's **protection factor** `K` and **armour**
  `A` (a third number `n` greater than 0.5 marks the bone as passing bullets, which is not used here). Each
  item is read as a decimal number (the third as an integer). Bone names are looked up in the model of the
  character wearing the outfit (a missing bone is an error).

A hit on a bone with an entry uses that entry; a hit on any other bone, or with no bone, uses the default
entry. The table is read again whenever the player puts the outfit on, after the player's model has
changed to the outfit's `actor_visual` (static; that this is the moment the outfit enters slot 6 is
**believed**) [B12].

With `c` the outfit's condition before this hit:

- for `t` = 8 (`fire_wound`): `P2 = max(P1 − (1 − ap) × c × A_b, F × P1)`; `fire_wound_protection` is not
  used;
- for every other type: `P2 = P1 × (1 − K_b × p_t × c)`;

where `K_b`, `A_b` are the bone's values (the default entry for a hit with no bone) [B10]. Then the outfit
loses condition: `c ← clamp(c − m_t × P1, 0, 1)` [B10].

(`inventory` section 4.8 shows `p_t × c` for the nine protections; for every type but `fire_wound`, a hit
with no bone or on a bone with `K` = 1 is reduced by exactly the shown fraction.)

### 4.5 Changes by type

With `I_t` the player's immunity (section 3), `H` = `health_hit_part`, `Q` = `power_hit_part`, `h` and `w`
the bone factors (section 4.3) and `x = I_t × P2` [B13]:

| `t` | Health change | Power change | Other | Wound size `z` |
|---|---|---|---|---|
| 0 `burn` | `− h × H × x` | `− Q × x` | | none |
| 1 `shock` | `− H × x` | `− Q × x` | | none |
| 2 `strike`, 7 `explosion`, 10 `physic_strike` | `− H × x` | `− Q × x` | | `w × x` |
| 3 `wound`, 8 `fire_wound` | `− h × H × x` | `− Q × x` | | `w × x` |
| 4 `radiation` | | | radiation `+ P2` (no immunity) | none |
| 5 `telepatic` | | | psy health `− I_5 × I_5 × P2` | none |
| 6 `chemical_burn` | | | | `w × x` |

The health change applies only while the character can be harmed (section 6); the power, radiation and psy
health changes always apply [B13]. So `chemical_burn` hurts only through bleeding, and `shock`, `strike`,
`explosion` and `physic_strike` ignore the bone's hit scale. A telepathic hit can kill through psy health
(`state` section 4.2 item 4).

### 4.6 The wound a hit opens

If the hit has a wound size `z` (table above) and the character's health is above 0 (its value before this
hit's changes take effect), the wound of bone `b` grows [B13, B14]:

- the character has at most one wound per bone (a hit with no bone has its own wound "no bone"); if bone `b`
  has none, a new wound with all eleven amounts 0 is added at the end of the wound list;
- the wound's amount for type `t` becomes `clamp(amount_t + (0.5 + r / 32767) × z, 0, 10)`, where `r` is
  the next value 0 … 32767 of a 15-bit random generator (the factor is uniform in 0.5 … 1.5; the program
  steps a generator of the same kind as that of `state` section 3.1, one shared by the whole game; any
  uniform generator serves).

### 4.7 When the changes take effect

The health, power, radiation and psy health changes of section 4.5 accumulate and are added, with the
clamps, at the next condition update (`state` section 4.2 item 13), together with the update's own changes;
a character whose health is then 0 or less dies (`state` section 4.2 item 8) [B13]. The outfit's condition
and the wounds change at once. A hit on a dead character changes nothing that is later applied (the
update does not run).

**Disputed in review:** "added at the next condition update" does not hold for the first condition update
after the character comes online or its condition block is loaded (the update with `dt` = 0 of `state`
section 4.1): before that update runs, the pending health, power and radiation changes are discarded, so a
hit arriving before it changes neither health, power nor radiation; its psy health change is kept and
applied (static).

## 5. Wounds, bleeding and healing

### 5.1 Wound records

A **wound** belongs to one bone of the player's model (or to "no bone") and has eleven amounts, one per hit
type 0 … 10, each 0 … 10 [B14]. Its **size** is the sum of its eleven amounts.

### 5.2 Bleeding

The **total bleeding** used by `state` section 4.2 item 9 is the mean size of the wounds: the sum of the
sizes of all wounds divided by their number (0 without wounds) [B16]. The health change per update is
`− total bleeding × bleeding_v × dt` while the character can be harmed, and the character counts as
bleeding when `total bleeding × bleeding_v × dt` is at least 1e−7 in magnitude (`state`).

### 5.3 Healing with time

Right after the bleeding term of `state` section 4.2 item 9 (so only in updates where item 9 runs), every
wound heals by `a = wound_incarnation_v × dt` [B16, B17]:

- if the wound's size is below 1e−7 in magnitude, all its amounts become 0;
- otherwise each amount becomes `amount − a`, and 0 if that is below `min_wound_size`.

A wound whose size is then exactly 0 is removed right after that condition update [B17]. An amount `z`
healed only by time thus becomes 0 after about `(z − 0.0256) / 0.003` game seconds (game time runs 10 times
faster than real time, `state` conventions).

### 5.4 Healing by eatables

Using an eatable item (`state` section 5 item 5) heals every wound as in section 5.3 with
`a = wounds_heal_perc` of the item instead of `wound_incarnation_v × dt` [B18]. For example a `bandage`
or `medkit_army` (1.0) clears every amount up to 1.0, a `medkit` or `medkit_scientic` (0.3) reduces each
amount by 0.3 [D7]. The healed wounds are removed at the next condition update (section 5.3) [B17].

### 5.5 What a save holds

In the client data condition block (`sav` section 8; `state` section 6), after the four condition values,
the `u8` wound count is the number of wounds (the low 8 bits), followed by the wounds in list order, each
[B19]:

| Field | Type |
|---|---|
| bone | `u8`: the low 8 bits of the bone's number in the player's model; "no bone" is written as 255 |
| amounts | 11 × `q8(amount, 0, 10)`, types 0 … 10 in order |

Loading the block replaces the wound list with the saved wounds, each wound's bone read back as the `u8`
value 0 … 255 (static). The bleeding state of `state` section 4.2 item 6 starts as "not bleeding" after a
load [B19]. When the client data is empty (New Game) the character has no wounds (`state` section 2.3).
The outfit's condition is saved as its item condition (`sav` section 7.4.2). The slowdown and the outfit's
bone protection table are not among the saved fields (`sav` section 8).

## 6. The "can be harmed" flag

The flag of `state` section 4.2 is set when the character's condition is created [B3]. In a single-player
game nothing changes it: the program writes it elsewhere only when handling a game event of the
multiplayer game kinds (deathmatch, team deathmatch, artefact hunt) [B21]. It is not saved.

The character can be harmed when the flag is set and the game runs its own local server with at least one
client connected, which a single-player game always does; during the playback of a recorded demo another
state of the level decides instead (not specified) [B21].

Other ways to protect a character do not reach the player: the script property `invulnerable` of a game
object reads and writes a value only monsters have (for the player it reports an error and does nothing)
[B22]; no shipped script uses it on the player [S2]. The disabling flag of section 4.2 item 2 is never set.

## 7. Effects of a hit not specified here

A hit on the player also does the following, which other specifications or later work cover; the
program was read here only far enough to name these effects, so each description is **believed**
[B5, B23, D8]:

- it plays a random sound from `[actor]` `hit_sounds` (`[actor_hit_snds]`, one list per type name) while
  the character is alive, except for `telepatic` hits;
- `explosion` hits play `affects\tinnitus3a` with a further effect, `fire_wound` and `wound_2` hits shake
  the camera, and `fire_wound` hits mark their direction on the HUD;
- blood and burn particles and blood drops on the ground (`[bloody_marks]`) follow the wounds;
- the physics impulse of the hit, the record of who hit whom for the characters' relations, and the
  report of the hit to scripts;
- the player's death when health reaches 0;
- `hit_probability_<d>` (section 3), read by what makes hits.

## 8. Verification facts

From the configuration [D1–D8]:

- `[actor]` `immunities_sect = actor_immunities_gd_master`, `damage = actor_damage`,
  `hit_sounds = actor_hit_snds`, `condition_sect = actor_condition`; the four
  `actor_immunities_gd_*` sections have the values of section 3, and none has
  `physic_strike_wound_immunity`.
- `[actor_damage]` has 45 keys: `default = 1.0, -1, 1.0`; 18 finger bones (9 per hand) and `eyelid_1`,
  `eye_left`, `eye_right`, `jaw_1` with `h` 0.0 and `w` 0.0; `bip01_head = 0.2, 0, 1.5`;
  `bip01_l_hand = 1.0, 4, 0.5`; `bip01_r_hand = 1.0, 2, 0.0`; 19 other bones with `h` 1.0 and `w` 1.5.
- 30 sections have `slot = 6`; every one has `immunities_sect` and `bones_koeff_protection`, none has
  `physic_strike_protection`. Their immunity sections have no `physic_strike_wound_immunity`, except the
  three `mp_*` sections (0.00).
- `novice_outfit` (New Game): protections `burn`, `shock`, `strike`, `wound`, `chemical_burn`,
  `explosion`, `fire_wound` 0.1, `radiation` and `telepatic` 0.0; immunities
  (`sect_novice_outfit_immunities`) `burn` 0.04, `strike` 0.01, `shock` 0.04, `wound` 0.025, `radiation`
  0.00, `telepatic` 0.00, `chemical_burn` 0.04, `explosion` 0.04, `fire_wound` 0.015; bone table
  `kurtka_mask_damage` (parents `body_kurtka_damage`, `head_mask_damage`): `hit_fraction` 0.1, no `default`
  key, 24 bones, all with `K` 1.0; the 18 body bones `A` 0.15, the 6 head bones `A` 0.05.
- Every bone table named by a single-player outfit (not `mp_*`) has 24 bones with two numbers each,
  `K` 1.0 for every bone, and no `default` key; `hit_fraction` 0.05 (`armor_helmet_damage`,
  `exo_helmet_damage`) or 0.1 (the others).

Computed from this specification with the New Game state (`novice_outfit` worn at condition 1, empty belt,
no wounds, health 1, power 1) [M1–M10]:

| Hit | Difficulty | `P2` | Health | Power | Wound amount (factor 0.5 … 1.5) | Outfit condition |
|---|---|---|---|---|---|---|
| `fire_wound` 0.5, `ap` 0, `bip01_spine` | `gd_master` | 0.35 | −0.35 | −0.035 | type 8: 0.2625 … 0.7875 | 0.9925 |
| the same | `gd_novice` | 0.35 | −0.175 | −0.0175 | type 8: 0.13125 … 0.39375 | 0.9925 |
| `fire_wound` 0.5, `ap` 0, `bip01_head` | `gd_master` | 0.45 | −0.09 | −0.045 | type 8: 0.3375 … 1.0125 | 0.9925 |
| `fire_wound` 0.1, `ap` 0, `bip01_spine` | `gd_master` | 0.01 (the `hit_fraction` floor) | −0.01 | −0.001 | type 8: 0.0075 … 0.0225 | 0.9985 |
| `fire_wound` 0.5, `ap` 1, `bip01_spine` | `gd_master` | 0.5 | −0.5 | −0.05 | type 8: 0.375 … 1.125 | 0.9925 |
| `strike` 0.2, no bone | `gd_master` | 0.18 | −0.18 | −0.018 | type 2: 0.09 … 0.27 | 0.998 |
| `burn` 0.2, no bone | `gd_master` | 0.18 | −0.18 | −0.018 | none | 0.992 |
| `chemical_burn` 0.2, no bone | `gd_stalker` | 0.18 | 0 | 0 | type 6: 0.0585 … 0.1755 | 0.992 |
| `radiation` 0.1, no bone | `gd_novice` | 0.1 | 0 (radiation +0.1) | 0 | none | 1.0 |
| `telepatic` 0.1, no bone | `gd_novice` | 0.1 | 0 (psy health −0.025) | 0 | none | 1.0 |

- `explosion` 0.5 with no bone and no outfit: health −0.05 at `gd_novice`, −0.5 at `gd_master` [M11].
- One wound of size 0.525 (one amount), no other wounds, updates of `dt` = 1 game second: the amount stays
  above 0.0256 for 166 updates and is 0 after the 167th; the bleeding takes 0.023046 health in total
  [M12]. Two wounds of sizes 0.4 and 0.2: total bleeding 0.3, health −0.00015 per game second [M13].
- A wound with amounts 0.525 (type 8) and 0.2 (type 2): after a `medkit` 0.225 and 0 (size 0.225, kept);
  after a `bandage` both 0, and the wound is removed at the next update [M14].
- A `strike` hit of power 0.5 gives slowdown 0.5: with `dt_m` = 0.01 s the desired acceleration is
  multiplied by 0.51 at the first movement update after the hit, and by exactly 1 from the 51st on when the
  slowdown is kept in single precision as the program keeps it (the 50th leaves a residue) [M15].

Invariants:

- For every single-player outfit (not `mp_*`) and every bone, `K × p_t ≤ 1`, so `P2 ≥ 0` for every type
  but `fire_wound`, and `P2 ≥ F × P1` for `fire_wound` [D5, D6].
- Every wound amount stays within 0 … 10; the outfit's condition within 0 … 1 [M16].
- With `gd_master` and no outfit, a hit of type 2, 3, 7, 8 or 10 on a bone with `h` 1 lowers health by
  exactly its power [M16].

## 9. Unestablished

- That the player's immunities are read from its configuration each time it comes online (section 3, item 1:
  believed).
- That the program's test for the controlled character (section 4.1 item 1) always holds for the player in
  single-player outside scripted control (believed).
- The moment the outfit's bone protection table is read (section 4.4: believed to be when it enters slot 6).
- The `hit_fraction` of an outfit whose section has no `bones_koeff_protection` (not set by the program in
  that case; every shipped outfit has the key).
- How artefacts on the belt give `B_t` (section 4.2).
- The harm test during demo playback (section 6).
- What uses the bone's "passes bullets" mark and the hit animation number (not part of applying a hit).

## 10. Provenance

- Date: 2026-11-24.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); archives `gamedata.db0` …
  `gamedata.dbd`; binary `bin\xrGame.dll`.
- Data examined: `config\creatures\actor.ltx`, `config\creatures\damages.ltx`, `config\misc\outfit.ltx` and
  every section with `slot = 6` in the `system.ltx` tree, the eatable sections, `scripts\bind_stalker.script`
  (difficulty saved and restored), `scripts\*.script` (uses of `invulnerable` and the difficulty).
- Analysis: static analysis of `xrGame.dll` (disassembly and decompilation with Ghidra) of the player's hit
  handling, the bone damage table and its loading, the worn outfit's protection, bone protection table and
  condition loss, the per-type condition changes, the immunity loading and the difficulty selection, the
  console command and script function that set the difficulty, the wound list (adding, bleeding, healing,
  removal, saving and loading), the eatable use, the slowdown after a hit and its use in the movement, the
  sprint cancel, the "can be harmed" flag and its test, and the script property `invulnerable`;
  configuration values were read from a fresh extraction of the winning copies with an independent LTX
  reader, and the numbers of section 8 were recomputed by a throwaway implementation written from this
  text. The program was not executed.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- None beyond the disputed statement of section 4.7.
