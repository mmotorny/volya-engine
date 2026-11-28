# Characters and monsters: scope and split

Scoping specification for the characters (stalkers and the trader) and the monsters of S.T.A.L.K.E.R.:
Shadow of Chernobyl (Russian edition) while they are online: their profiles and the state the engine
keeps for them, how they move and which motions they play, what they see, hear and remember, how they
fight, die and are hurt, how they take part in a talk, and what of all this a save holds. It lists the data
each part reads, proposes how to split the area into small specifications, each covering one format or one
behaviour, that can be investigated and reviewed on their own, and states for each its data, the approved
specifications it builds on, what not yet specified it waits on, the shipped scripts that drive it, and its
order. It points out the shortest path to the first thing the game needs from this area: the trader in the
bunker of the first level as a character the player can talk to, enough for his start dialog. It specifies
no behaviour itself.

It builds on the approved specifications `sav`, `ltx-config`, `ui-layout`, `text-and-fonts`, `db-archive`,
`skeletal-motions`, `level-cform`, `chunks`, `sound-ogg-params` (`docs/formats/`), `offline-simulation`,
`script-loading`, `script-environment`, `script-api-scope`, `dialogue-scope`,
`trader-and-crows-scope`, `physics-scope`, `hud-scope`, `pda-scope`, `level-sound-scope`,
`narrowed-specs-scope`, `sound-occlusion`, `sound-positional`, `object-visibility`, `player-hits`,
`environment-hits` (`docs/behavior/`), all cited by name.

Conventions:

- **Named in `xrGame.dll`** means the exact text, letter case included, occurs in the shipped
  `bin\xrGame.dll` (the file of `script-environment`) as a printable string of 4 or more characters, or
  as one `:`-separated part of such a string. It shows that the program refers to that key, element, file
  or message; what it does with it is for the proposed spec to establish. A key that is not named may
  still be read (for example through a name the program composes); that is for its spec to establish.
- **Character** means an object of the class ids `AI_STL_S` (stalker), `AI_STL` or `AI_TRD_S` (trader);
  **monster** an object of an `SM_` class id; **creature** either. Crows (`AI_CROW`) are
  `trader-and-crows-scope`'s and are not covered here. "Player character" is the character the player
  controls. Kinds, blocks and fields ("Trader-like block", "Monster-like block", "client data") are
  `sav`'s (7.4, 8).
- The functional descriptions (what a key or element is for) are readings of the names, of the
  comments in the data and of the shipped scripts, and are **believed** until the proposed spec
  establishes them; they fix what each spec must cover, not how the part behaves. **Unestablished** marks
  what this scoping work could not settle and leaves to the proposed spec.
- Every fact carries its check, an assertion of `checks/characters-scope/checks.py` or `coverage.py` in
  the dirty workspace, cited by label in brackets (e.g. [R1]). `run_all.py` runs both. File paths are
  archive paths of the winning copies (the later archive wins, `db-archive`). "Script" means a file of
  `scripts\`; calls are counted outside comments, and the shipped API listing `scripts\lua_help.script`
  (`script-api-scope` 2.1) is not counted as a script. A member "called by" a script is a use of
  `:name(` or `.name(` in it.

## 1. Data

### 1.1 The records

`spawns\all.spawn` holds 1,073 stalker records (`AI_STL_S`), one trader (`AI_TRD_S`), 276 monster
records of 11 class ids, 197 crows and the player character; no record has the class id `AI_STL` [R1]:

| Class id | Records | Class id | Records |
|---|---|---|---|
| `SM_DOG_S` | 90 | `SM_FLESH` | 11 |
| `SM_SNORK` | 68 | `SM_TUSHK` | 8 |
| `SM_BOARW` | 32 | `SM_CONTR` | 5 |
| `SM_P_DOG` | 28 | `SM_GIANT` | 5 |
| `SM_BLOOD` | 15 | `SM_DOG_P` | 2 |
| `SM_POLTR` | 12 | | |

[R1b]. On `l01_escape` there are 63 stalkers, 68 monsters (`SM_DOG_S` 48, `SM_BOARW` 10, `SM_FLESH` 8,
`SM_P_DOG` 2), the trader and 55 crows [R2, R2b] (the level of a record is the level of its game vertex,
`offline-simulation` 1).

In every stalker record and in the trader's, the Trader-like block names a character profile but leaves
the rest of the identity unset: the specific character and the character name are empty, the community
is −1 and the rank and reputation are −2,147,483,647 [R3]. The 1,074 records name 205 distinct profiles,
all defined in `config\gameplay\npc_profile.xml` (1.3) [R3b]. Money is 5,000 in 1,071 records and
100,000, 500,000 and 1,000,000 in one each; the trader flags are 1 in 1,071 records and 0 in 3 [R4]. So
the specific character, name, community, rank and reputation are believed filled in from the profile when
the game first needs them (CH-2).

Custom data: 428 stalkers and 86 monsters have a `[logic]` section (scripted behaviour, `xr_logic`);
1,001 stalkers have a `[smart_terrains]` section (jobs, CH-28) [R5].

### 1.2 Character and monster sections

The `config\system.ltx` tree has 164 sections whose class id is a character's or a monster's: 94
`AI_STL_S` (from `[stalker]` and its descendants), 1 `AI_STL` (`m_izgoy`), 1 `AI_TRD_S` (`m_trader`) and
68 of 17 `SM_` class ids [S1]. Six `SM_` class ids have sections but no record: `SM_BURER`, `SM_CAT_S`,
`SM_CHIMS`, `SM_DOG_F`, `SM_IZLOM`, `SM_ZOMBI` [S1b]; such creatures exist only if created in play
(`scripts\se_respawn.script` names the zombie sections, and `psy_dog_phantom`, the psy dog's phantom
section, is named in `xrGame.dll`) [S1c] (believed).

With parents merged (`ltx-config` 4), these sections have **475 distinct keys** [S2]; 363 are named in
`xrGame.dll`, 112 are not [S2b]. Section 8 gives every key one owner. Their families:

- `[stalker]`: `script_binding = bind_stalker.init`; vision `eye_fov` 110, `eye_range` 80, with
  `vision_free_section` and `vision_danger_section` naming `[stalker_vision_free]` and
  `[stalker_vision_danger]`; hearing `sound_threshold` 0.08 and `sound_perceive_section`; memory sizes
  `DynamicObjectsCount`, `DynamicSoundsCount`, `DynamicHitCount` (32 each); `max_ignore_distance` 15.0;
  `movement_speeds`; `character_profile = default` (commented as an option for the level editor) [S3].
  It also has the weapon queue and dispersion keys, the critical wound keys, a melee `hit_power`,
  `crouch_type` 0, `step_params = stalker_step_manager` and `damage = stalker_damage` [S3d].
- The two vision sections have the same nine keys; the free one has `visibility_threshold` 150 and
  view-distance factors 0.4 … 0.8, the danger one 120 and 0.7 … 1.0 [S3b].
- `[stalker_movement_speeds]` has 19 speeds by mental state, body state, movement type and direction:
  free walk forward 1.1, free run 3.0, danger run forward 3.6, panic run 7.0 [S3c].
- `[stalker_step_manager]` gives, per motion name (37 names), the cycle count and the time and power of
  the left and right step, e.g. `norm_walk_fwd_0 = 1, 0.2, 0.2, 0.65, 0.2` [M5].
- Monsters carry their vision keys directly in the section, and keys for movement speeds per motion
  (`Velocity_*`, `Accel_*`), attack, morale, eating, rest, jumping, sounds, and the abilities of single
  kinds (telekinesis, invisibility, psychic aura, phantoms, flame and others).
- `[m_trader]`: `AI_TRD_S`, `script_binding = bind_monster.bind`, money 1,000,000, species `human`; it
  has no `character_profile`, no vision and no movement keys [S4].
- `script_binding` is `bind_stalker.init` in the 94 `AI_STL_S` sections and `m_izgoy`, and
  `bind_monster.bind` in the other 67 monster sections and `m_trader`; `m_cat_e` has none [S5].
- `m_izgoy` (`AI_STL`, no record) has its own fire keys (`MaxMissDistance` and others) [S6].

### 1.3 Character descriptions

- `config\system.ltx` `[profiles]`: `files = npc_profile`, `specific_characters_files` lists 15 files
  (`character_desc_general`, `character_desc_simulation`, `character_desc_escape`, … ,
  `character_desc_sarcofag`) [P2].
- `npc_profile.xml` defines **474 character profiles**, each with a `class`; 18 also name a
  `specific_character` [P1].
- The 15 files define **743 specific characters** [P2]. Children (number of specific characters having
  each) and attributes [P2b]:

  | Element or attribute | Count | Element or attribute | Count |
  |---|---|---|---|
  | `name`, `icon`, `class`, `community`, `rank`, `reputation`, `visual`, `supplies` | 743 each | `actor_dialog` | 398 |
  | `bio` | 741 | `money` | 226 |
  | `snd_config` | 735 | `team` | 40 |
  | `map_icon` | 710 | `panic_threshold` | 20 |
  | `terrain_sect` | 652 | `panic_treshold` | 7 |
  | `start_dialog` | 603 | `hit_probability_factor` | 1 |
  | `critical_wound_weights` | 521 | `terrain_sect_` | 1 |
  | `crouch_type` | 513 | attribute `id` / `team_default` / `no_random` | 743 / 739 / 4 |

  472 distinct `class` values [P2c]. Communities: `stalker` 334, `monolith` 87, `bandit` 68, `military`
  57, `zombied` 53, `dolg` 49, `killer` 33, `freedom` 23, `arena_enemy` 18, `ecolog` 15, `actor` 2,
  `actor_dolg` 2, `trader` 2 [P2d].
- Included fragments: `character_criticals_1.xml` … `_6.xml` each hold one `critical_wound_weights`;
  `character_items.xml`, `character_food.xml`, `character_drugs.xml` hold supply lines with a `prob=`
  (believed a spawn probability); `character_dialogs.xml` is `dialogue-scope` 1.3's [P3].
- Named in `xrGame.dll`: `specific_character`, `character`, `class`, `community`, `rank`,
  `reputation`, `money`, `supplies`, `visual`, `icon`, `snd_config`, `crouch_type`, `terrain_sect`,
  `critical_wound_weights`, `panic_threshold`, `hit_probability_factor`, `team_default`, `no_random`,
  `start_dialog`, `actor_dialog`, `infinitive`, `character_profile`, `specific_characters_files`,
  `profiles`, `name`, `team`; not named: `bio`, `map_icon`, and the misspellings `panic_treshold` and
  `terrain_sect_` (believed ignored) [P4, P4b]. Messages named: `'community' field not fulfiled for
  specific character`, the same for `'rank'` and `'reputation'`, `no default specific character set for
  class`, and one beginning `cannot set 'no_random' and 'team_default' flags simultaneously` [P5].
- Every `terrain_sect` names one of the 8 sections `[stalker_terrain]`, `[stalker_dolg_terrain]`,
  `[stalker_freedom_terrain]`, `[stalker_ecolog_terrain]`, `[stalker_bandit_terrain]`,
  `[stalker_killer_terrain]`, `[stalker_military_terrain]`, `[stalker_zombied_terrain]` [N3, N3b]
  (believed: which kinds of level vertices the character may walk, CH-9).

### 1.4 Communities, ranks, reputation and relations

`config\creatures\game_relations.ltx` has the sections `[game_relations]`, `[communities_relations]`,
`[rank_relations]`, `[reputation_relations]`, `[communities_sympathy]`, `[rank_kill_points]`,
`[action_points]`, `[ranks_properties]`, `[smart_terrain_rank_change]`, `[monster_communities]`,
`[monster_relations]` [G2b]:

- `[game_relations]`: `rating = novice, 300, experienced, 600, veteran, 900, master`;
  `reputation = terrible, -1000, very_bad, -150, bad, -50, neutral, 50, good, 150, very_good, 1000,
  excellent`; `goodwill_names = enemy, -400, indifferent, 500, friendly`; `monster_rating = weak, 400,
  normal, 800, strong`; `attitude_neutal_threshold` −400, `attitude_friend_threshold` 500 [G1]; 15
  `communities` with indices `actor` 0, `actor_dolg` 1, `actor_freedom` 2, `stalker` 5, `monolith` 6, …,
  `trader` 15, `arena_enemy` 16 [G1b].
- `[communities_relations]` is a 15 × 15 table in the order of `communities`; the `trader` row is all 0,
  and the `actor` row is 0 in the `trader` column [G2].
- `[monster_relations]` has 20 rows of 20 values in the order of `[monster_communities]`, except the
  `human` row, which has 19: two values of its first line have no comma between them [G3] (what the
  program makes of that row is CH-3's).
- `[action_points]` has 29 keys: goodwill limits (`personal_goodwill_limits`,
  `community_goodwill_limits`), goodwill and reputation changes for attacking, killing and helping in a
  fight per relation, `min_attack_delta_time`, `fight_remember_time` [G4]. Of the 41 keys of
  `[game_relations]` and `[action_points]`, 28 are named in `xrGame.dll`; not named are `monster_rating`
  and the 12 keys `free_…_attack_…` and `danger_…_attack_…` [G4b]. The other nine section names are named
  [G4c].
- `[ranks_properties]`: factors for immunities, visibility and dispersion of `novice` and `experienced`
  characters (all 1 except `dispersion_experienced_k` 0.1) [G5].

### 1.5 Navigation, decision and evaluation data

- Each of the 18 levels has `level.ai`, `level.gct` and `level.game` [N1, N1b]. `offline-simulation` 2
  specifies `level.gct`, 3 the header of `level.ai` and the position of its level vertices, leaving
  "the other 16 bytes" of each vertex (among them links to neighbours) unestablished, and 4 the patrol
  paths of `all.spawn` chunk 3, whose point flags and links it does not use [D5]. `level.ai` and
  `$game_ai$` are named in `xrGame.dll` [N1b]. Whether the game reads `level.game` is unestablished.
- 24 evaluation-function files `ai\alife\*.efd` (12) and `ai\common\*.efd` (12), all named in
  `xrGame.dll` [N2, N2b]; the keys `ef_creature_type`, `ef_weapon_type`, `ef_detector_type` of the
  sections are believed their inputs (CH-16).
- The shipped API listing's `stalker_ids` has 119 constants: 58 actions and planners, 37 world
  properties and 24 sound kinds [A1]; they are `script-api-scope` W's names (the decision planner, CH-17
  … CH-19, owns what the engine does under them).

### 1.6 Motions and sounds

- 31 motion files: 22 under `meshes\actors\` (among them `stalker_animation.omf` and
  `critical_hit_grup_1.omf` … `_7.omf`) and 9 under `meshes\monsters\` [M1, M1b].
- `sounds\characters_voice\` holds `human_01` 4,168, `human_02` 4,347, `human_03` 4,244, `scenario`
  1,065, `multiplayer` 254 and `dialogs` 62 `.ogg` files [M2b]; specific characters choose a folder with
  `snd_config` (32 distinct values; 78 characters have `characters_voice\human_01\ military\`, with a
  space) [M2]; `[stalker]` sound keys are folder prefixes (`sound_death = fight\death\death_`,
  `sound_humming = states\idle\idle_`) [M4]. `sounds\monsters\` holds 516 `.ogg` files in 18 folders
  [M3].

### 1.7 Scripts

- `scripts\modules.script` loads 20 stalker schemes (19 scripts; `xr_meet` twice) and 13 monster schemes
  [K1, K1b]; with `xr_logic` they run the `[logic]` of 1.1. The stalker binder (`bind_stalker.init`)
  hands the stalker to `xr_motivator` [K7].
- Motion and movement requests: the 10 `state_mgr` scripts (with `state_lib`) set the movement type, the
  body state, the mental state, the sight and the item, and add script animations [K2, K2b]; `move_mgr`
  sets patrol paths and path types; 24 scripts set a desired position [K3].
- Decisions: 32 scripts reach the motivation action manager, `state_mgr_goap` builds an action planner
  [K4]; combat schemes `xr_combat`, `xr_combat_camper`, `xr_combat_monolith`, `xr_combat_zombied`,
  `xr_combat_ignore`, and `xr_danger`, `xr_wounded`, `mob_state_mgr`, `smart_terrain`, `xr_gulag` exist
  [K9]; `config\misc\` has 27 `gulag_*.ltx` files [K9b].
- Sight and memory: `see` is called by 14 scripts, `best_enemy` by 23, `memory_time` by `xr_camper` [K10].
- Talk: `enable_talk` is called by `mob_remark`, `mob_trader`, `xr_assistance`, `xr_companion`,
  `xr_meet`, `xr_sos`, `xr_wounded`; `disable_talk` by `dialogs`, `mob_remark`, `mob_trader`,
  `xr_assistance`, `xr_meet`, `xr_wounded` [K5]; `set_start_dialog` by `xr_meet`, `xr_sos`, `xr_wounded`;
  `is_talking` by 8 scripts [K11]. `mob_trader` reads a section's `can_talk` (default true) and enables
  talk when it is true, else disables it [K5b].
- Relations: `character_community` is called by 15 scripts, `set_relation` by 9,
  `set_character_community` by `dialogs` and `dialogs_military` [K12].
- Items: `death_manager` iterates inventories and reads `config\misc\death_generic.ltx`,
  `death_items_by_communities.ltx`, `death_items_by_levels.ltx`, `death_items_count.ltx` [K8].
- Saving: the stalker binder saves a boolean, then `xr_logic`'s, `dialog_manager`'s and
  `trade_manager`'s state; the monster binder saves `xr_logic`'s, and `trade_manager`'s for the trader
  [K7].

## 2. Parts

### 2.1 Profiles and state

- **Character descriptions** (CH-1): the format of 1.3: the two lists, profiles and their `class` and
  `specific_character`, specific characters and every child and attribute, string ids (`name`, `bio`),
  `money` with `min`, `max`, `infinitive`, the `supplies` text, the included fragments, the misspelt
  elements, and the program's messages for missing fields.
- **Identity** (CH-2): how a record's profile (1.1) becomes a specific character: an explicit
  `specific_character`, or a choice among the specific characters of the profile's `class` (with
  `no_random` and `team_default`; the rule and its random numbers), when it happens (at the first
  spawn, believed, since every record leaves it empty), and what it fills: the Trader-like block's
  specific character, name, community, rank, reputation and money, the visual (`visual`), and what the
  monster sections' `community`, `rank`, `spec_rank` give monsters.
- **Relations** (CH-3): the tables of 1.4, the names of rank, reputation and goodwill from their
  thresholds, the relation of a character to the player character and to another creature (community,
  personal goodwill, rank and reputation relations, sympathy, `Friend_Community_Overrides`, the monster
  table), what the talk panels and the crosshair show as values (`hud-scope` 14, `pda-scope` PD-14 for
  the texts), and the relations registry saved in `sav` chunk 9.
- **Relation changes** (CH-4): `[action_points]`, `[communities_sympathy]`, `[rank_kill_points]`: how
  attacks, kills and help in fights change goodwill, reputation and rank, with the limits and times; the
  unnamed `free_…`/`danger_…` keys (1.4); `pda-scope`'s B-ai.
- **Condition and life state** (CH-5): health and its maximum, power, satiety, radiation, psychic
  health, bleeding and wounds, the `condition_sect` keys, the `_hit_part` split, and the states others ask
  for: alive, dead, wounded (`ph_character_have_wounded_state` with PH-15, and the scripted wounded state
  of `xr_wounded`).
- **Items and money** (CH-6): the starting inventory (`supplies`, the record's `[spawn]`, the fragments'
  probabilities, `Spawn_Inventory_Item_*` for monsters), money and `infinitive`, the items characters pick
  up and select (`can_select_items`, `use_single_item_rule`, the equipment and main-weapon preferences of
  `sav` 7.4.2), `attachable_items` (items shown on the body), carrying limits, the trader's `cost`, and the
  items left on death (`death_manager`, 1.7).

### 2.2 Movement and motion choice

- **Level graph** (CH-8): `level.ai` in full (the 16 bytes per vertex left by `offline-simulation` 3:
  neighbour links, and believed cover, light and plane values), the patrol paths' point flags and links,
  and whether and how `level.game` is read.
- **Paths and restrictions** (CH-9): the path search over level vertices (the terrain masks of
  `terrain_sect`), space restrictions (the Monster-like block's out- and in-restrictions, dynamic
  restrictions, `SpaceRestrictionSection`), following a patrol path, and arrival.
- **Stalker movement** (CH-10): the detail path, speeds from `[stalker_movement_speeds]` by mental and
  body state and movement type, turning of body and head (`max_left_torso_angle`,
  `max_right_torso_angle`, `bone_*`), and the character body's moves (with PH-22).
- **Stalker motions** (CH-11): which legs, torso, head and whole-body motions a stalker plays for its
  movement, body, mental and weapon states, the motion files and their names, script animations added
  by `state_mgr` (1.7), `crouch_type`, and the step manager (`step_params`, `foot_bones`, `LegsCount`)
  for footstep times and powers.
- **Monster movement and motions** (CH-12): speeds per motion (`Velocity_*`, `Accel_*`, the movement
  factors), the motion chosen for each action and speed, the monster motion files, the giant's step
  effect (`step_effector_*`). Large; it splits by motion choice and movement when investigated.

### 2.3 Sight and memory

- **Vision** (CH-13): the field of view and range, the accumulation of visibility in the free and danger
  sections (1.2) and its monster form, the player character's luminosity and speed as seen, fog factors,
  `[ranks_properties]`' visibility factor, transparency of what lies between.
- **Hearing** (CH-14): `sound_threshold`, `SoundThreshold`, the perception factors per sound kind,
  `self_sound_factor`, `self_decrease_*`, `max_hear_dist`; the loudness reaching a hearer is
  `sound-occlusion`'s.
- **Memory and enemy choice** (CH-15): what is remembered of seen, heard and hitting objects and for how
  long (the three counts), the choice of the best enemy and danger (`ignore_monster_threshold`,
  `max_ignore_distance`, `species`, the relation of CH-3), anomaly detection by monsters.
- **Evaluation functions** (CH-16): the `.efd` format and how the engine evaluates them with the `ef_*`
  keys (believed weapon and enemy choice and the offline simulation's).

### 2.4 Combat

- **Stalker planner** (CH-17): the engine's decision planner of a stalker (the planners, actions and
  properties that `stalker_ids` names, 1.5: alive, dead, offline-task, items, anomalies, death), how
  `xr_motivator` and the schemes add their own actions (W's members), and the update that drives it.
- **Stalker combat** (CH-18): attack, cover (the cover data of CH-8, the `selector_*` keys), detour,
  search, retreat, hold position, killing wounded enemies, post-combat wait, panic (`panic_threshold`),
  and the scripted combat types (`xr_combat_*`, 1.7).
- **Stalker danger** (CH-19): reactions to grenades, unknown sounds, danger in a direction and anomalies.
- **Stalker weapons** (CH-20): firing queues per distance, dispersion by state (`disp_*`, the rank
  factor), aiming bones, reloading, changing weapons, throwing grenades (with PH-17), `m_izgoy`'s fire
  keys, `hit_probability_factor`.
- **Hits, wounds and death** (CH-21): damage per bone (`damage` sections), immunities, the melee hit
  (`hit_power`, `hit_type`, `power_fx_factor`), critical wounds (`critical_wound_*`, body parts, the
  `critical_hit_grup_*` motions), killers (`killer_clsids`), the Creature block's killer id and death
  time, and death (the body afterwards is PH-15's).
- **Monster behaviour** (CH-22): the common states of a monster: rest and the day (`DayTime_*`), eating
  (`eat_*`, satiety, corpses), hearing danger, attack (`attack_params`, attack distances, run attack,
  jumps), morale and panic (`Morale_*`), squads (`squad_attack_algorithm`), stealing, threatening,
  `weapon_usage`. Large; it splits by state family.
- **Monster abilities** (CH-23): the abilities of single kinds: telekinesis and gravity waves (burer,
  poltergeist), invisibility and the vampire attack (bloodsucker), control and the psychic aura
  (controller), phantoms (psy dog), flames (poltergeist), the huge kick (giant), fake death (zombie), and
  their particles. One spec per kind when investigated.
- **Script control** (CH-24): what the engine does when scripts take a creature over (`script`,
  the action queue of V, the monster scripts' captures) and gives it back.
- **Sounds** (CH-25): the creatures' own sounds: stalker voice kinds by situation (`sound_*`
  prefixes, `snd_config`, the 24 sound kinds of 1.5), monster sounds and their delays, the scripted
  sound themes (`xr_sound`, `sound_theme`, `config\misc\script_sound.ltx`); `level-sound-scope` S24's
  character and monster part.

### 2.5 Talk state

**Talk state** (CH-7): which objects can talk (stalkers and the trader: class ids `AI_STL_S`,
`AI_TRD_S`), the talk-enabled state per character with its default, how `enable_talk` and `disable_talk`
change it, being in a talk on the character's side, the character's current start dialog
(`set_start_dialog`, the stalker update's start dialog, `sav` 7.4.2), and the character's part while
talking (a stalker stops and faces the player character under `xr_meet`, believed; the trader's is
TC3 and TC4).

### 2.6 Online life and saving

- **Online life** (CH-26): what coming online creates for a creature (the model from `visual` and the
  specific character, `corpse_visual`, the body, inventory, binder), the update cadence (`scheduled`,
  believed), going offline, and creation during play (`config\misc\respawn.ltx`).
- **Saved state** (CH-27): the client data of creatures (`sav` 8: the condition block's wounds, AI
  memory, "further AI state" and the inventory-owner block, all unestablished there), the scripted
  parts of the binders (1.7), and what loading restores.
- **Smart-terrain jobs** (CH-28): how a character online takes and leaves a job (`smart_terrain`,
  `xr_gulag`, the `gulag_*.ltx` files, `[smart_terrain_rank_change]`), the engine's part in it.
- **Offline searches** (CH-29): `search_speed`, the item detection probabilities and the artefact
  search keys, which `offline-simulation` does not cover.

### 2.7 Boundaries

- **Crows and the trader's own logic** (`trader-and-crows-scope`): the trader "has no movement or
  decision-making of its own" [D2]; his motions and channels are TC1, TC2, his logic TC3, TC4. This scope
  gives the trader only what every character has: identity (CH-1, CH-2), relations (CH-3), life state
  (CH-5), items (CH-6), talk state (CH-7), online life and saving (CH-26, CH-27). Crows are TC5–TC7.
- **The script interface** (`script-api-scope`): J (simulation objects), K (simulation registry), M
  (game object basics), Q (relations, community and ranking), V (scripted actions), W (action planner)
  and X (movement, sight and memory), and R, S, L, O, T for dialogs, inventory, binders, callbacks and
  sound [D3]: their specs own the names and how scripts call them; the specs here own what the engine
  does behind them and name no member as their own.
- **Physics** (`physics-scope`): bodies of living creatures PH-22, ragdolls PH-15, throwing PH-17; the
  `ph_*` keys, `cform`, `material`, `destroyed_vis_name` [D10].
- **Offline simulation** (`offline-simulation`): offline travel, switching, `level.gct`,
  `going_speed`, `current_level_going_speed`, `smart_terrain_choose_interval`, `terrain`.
- **Screen effects** (`hud-scope` 20): the monsters' `*_effector` keys [D6].
- **Dialogue** (`dialogue-scope`): the dialogs, the window and `characters_voice\dialogs\`.
- **Other**: editor options (`$npc`, `$prefetch`, `$spawn`, `custom_data`, `stalker_custom_data.ltx`),
  `GroupControlSection` (spawn groups, `sav` 7.4.3), multiplayer voices.

## 3. Consistency with other scopes

`dialogue-scope` 2.11 names what the dialogue needs from characters as B-chars (1) … (6) [D1]; DG-3 waits
on B-chars 1, 2 and 4, DG-5 on 3, and its review gap gives DG-2 B-chars 6 [D1b]. They map to:

| B-chars | Dialogue's need | Spec here |
|---|---|---|
| 1 | which objects are characters that can talk, their talk-enabled state and its saving | CH-7 (saving with CH-27) |
| 2 | specific character and profile, and from it the `start_dialog` and `actor_dialog` lists | CH-1 (reading), CH-2 (choice); the current start dialog CH-7 |
| 3 | name, picture, community, rank, reputation, relation to the player character, as values | CH-2 (name, picture), CH-3 (community, rank, reputation, relation) |
| 4 | alive or wounded | CH-5 |
| 5 | behaviour while talking | CH-7 (the stalker's under `xr_meet`); motions CH-11; the trader TC3, TC4. **Disputed in review:** `dialogue-scope` B-chars 5 includes the character's sounds while talking; the row names no owner for them (believed CH-25 for stalkers) |
| 6 | items and money as dialog scripts ask for them (the speaker object) | CH-6; the object and its members `script-api-scope` M and S |

Other scopes waiting on this one: `hud-scope` 14 needs CH-2 and CH-3 (names, relations) and 16 needs
CH-5 and CH-13 (which creatures, alive, in view) [D6]; `pda-scope` B-ai is CH-4 and CH-28, PD-14 and
PD-15 need CH-2 and CH-3, PD-16 and PD-17 need CH-21 (kills) [D9]; `level-sound-scope` S24's character
and monster part is CH-25 [D7]; `narrowed-specs-scope` LO-2 leaves characters' and monsters' motions to
CH-11 and CH-12 [D8]; `sav` 14 items 6 and 7 are CH-2, CH-3, CH-9 (restrictions), CH-15 (memory), CH-27
and CH-28 [D4].

## 4. The first path: the trader as a talking character

This section is a worked example across the system specs of section 5, not a spec of its own: it names
what each of them must cover for the first conversation.

The trader is record 84 of `all.spawn`: section `m_trader`, class id `AI_TRD_S`, on `l01_escape`, story
id 3, money 1,000,000, profile `escape_trader`, custom data `[spawn]` with `wpn_pm` and `ammo_9x18_fmj`
and `[logic]` `cfg = scripts\esc_trader.ltx` [R6]. Like every character record it leaves the specific
character, name, community, rank and reputation unset [R3].

The profile `escape_trader` has the class `Trader` and names the specific character `escape_trader`
(`character_desc_escape.xml`, `no_random = 1`): name `escape_trader_name`, bio `escape_trader_bio` (both
string ids of the string table), icon `ui_npc_u_trader`, class `trader`, community `trader`, rank 330,
reputation 23, visual `actors\trader\trader`, start dialog `escape_trader_start_dialog`, no `snd_config`
[P6, P6c, P6d]; money `min` = `max` = 100,000 with `infinitive = 1`, supplies `[spawn]` `wpn_knife`
[P6b]. With the thresholds of 1.4 rank 330 is `experienced` and reputation 23 `neutral`, and the trader
community's relation to the player character's is 0 both ways, inside `indifferent` (−400 … 500), believed;
how the goodwill is combined is CH-3's [G1, G2].

His talk-enabled state is set by his logic: `mob_trader` enables talk when the active section's
`can_talk` is true (the default) and disables it otherwise (TC4 owns which sections set it) [K5b]. The
two functions of his start dialog that ask for an item, `have_darklab_documents` and
`hasnt_darklab_documents`, ask the first speaker [K6]; which speaker is first is DG-2's (believed the
player character, so the trader's own inventory is not needed).

**Path to the first conversation**: CH-1 (reading his profile and specific character), CH-2 (the
explicit specific character filling his identity; the panel's name and icon), CH-7 (that the trader can
talk, the talk-enabled state that `mob_trader` sets, being in a talk), CH-3 only for the panel's
community, rank, reputation and relation values (DG-5 can show the frames without them), and CH-5 only for
"alive" (his health is the section's 100, believed alive throughout). Outside this scope: the dialogue
(DG-1 … DG-6), the trader's logic (TC3, TC4), the script runtime and API (`script-api-scope` M, Q, R). No
movement, sight, memory or combat is needed: the trader has none [D2].

## 5. Proposed specs

Each entry: functional description (section 2); data; builds on (approved); waits on (not yet
specified); scripts; order. "First path" marks section 4's. Section 8 lists every name each owns.

**CH-1 Character descriptions** (`character-descriptions`, format). 2.1. Data: 1.3, the string tables.
Builds on: `ltx-config`, `ui-layout` (XML, `#include`), `text-and-fonts`, `db-archive`. Waits on: nothing.
Scripts: none. Order 1. First path.

**CH-2 Identity and profile choice** (`character-identity`, behaviour). 2.1. Data: 1.1's Trader-like
fields, 1.3, the monster sections' `community`, `rank`, `spec_rank`. Builds on: CH-1, `sav` 7.4,
`offline-simulation` 8 (creation order), 6.5 (which objects are present). Waits on: nothing for the explicit case; the
random generator for the choice. Scripts: none known. Order 2. First path.

**CH-3 Communities, ranks, reputation and relations** (`character-relations`, behaviour). 2.1. Data: 1.4.
Builds on: CH-2, `ltx-config`, `sav` chunk 9. Waits on: `script-api-scope` Q for the names. Scripts: the
callers of 1.7 (relations). Order 3. First path (panel values).

**CH-7 Talk state** (`character-talk-state`, behaviour). 2.5. Data: the class ids, the stalker update's
start dialog. Builds on: CH-2, `sav` 7.4.2. Waits on: `script-api-scope` R and M for the names;
`trader-and-crows-scope` TC4 for the trader's sections. Scripts: the callers of 1.7 (talk), `xr_meet`,
`mob_trader`. Order 4. First path.

**CH-5 Condition and life state** (`creature-condition`, behaviour). 2.1. Data: the condition keys of
section 8. Builds on: `sav` 7.4.1, 8, `player-hits`. Waits on: CH-21 for what lowers health; PH-15 for
the wounded body. Scripts: `xr_wounded`. Order 5. First path (alive only).

**CH-8 Level graph** (`level-graph`, format). 2.2. Data: `level.ai`, `level.game`, `all.spawn` chunk 3.
Builds on: `offline-simulation` 2–4, `chunks`. Waits on: nothing. Scripts: none. Order 6.

**CH-16 Evaluation functions** (`evaluation-functions`, format and behaviour). 2.3. Data: the 24 `.efd`
files, the `ef_*` keys. Builds on: `db-archive`. Waits on: nothing for the format. Scripts: none. Order 7.

**CH-26 Online life** (`creature-online`, behaviour). 2.6. Data: the CH-26 keys, `respawn.ltx`. Builds
on: CH-2, `offline-simulation` 6, `script-api-scope` L. Waits on: PH-22 for the body.
Scripts: `bind_stalker`, `bind_monster`, `se_respawn`. Order 8.

**CH-9 Paths and restrictions** (`character-paths`, behaviour). 2.2. Data: CH-8's, the terrain sections,
the restriction fields. Builds on: CH-8, `sav` 7.4.1. Waits on: `script-api-scope` X, N for the names.
Scripts: `move_mgr`, `xr_gulag` (restrictions), the walkers. Order 9.

**CH-10 Stalker movement** (`stalker-movement`, behaviour). 2.2. Data: `[stalker_movement_speeds]`, the
CH-10 keys. Builds on: CH-9. Waits on: PH-22 (body), `script-api-scope` X. Scripts: `state_mgr_movement`,
`state_mgr_bodystate`, `state_mgr_mental`, `state_mgr_direction`. Order 10.

**CH-11 Stalker motions** (`stalker-motions`, behaviour). 2.2. Data: the actor motion files, the step
manager. Builds on: `skeletal-motions`, motion playback (`trader-and-crows-scope` TC1), CH-10. Waits on:
TC1. Scripts: `state_mgr_animation`, `state_mgr_animation_list`, `state_mgr_animstate`, `state_lib`.
Order 11.

**CH-12 Monster movement and motions** (`monster-movement`, behaviour). 2.2. Data: the CH-12 keys, the
monster motion files. Builds on: CH-9, TC1. Waits on: TC1, PH-22. Scripts: `mob_state_mgr` and the
monster schemes. Order 12.

**CH-13 Vision** (`creature-vision`, behaviour). 2.3. Data: the CH-13 keys, `[ranks_properties]`.
Builds on: `level-cform` (materials), `object-visibility`. Waits on: the player character's luminosity
(lighting of dynamic objects). Scripts: callers of `see`. Order 13.

**CH-14 Hearing** (`creature-hearing`, behaviour). 2.3. Data: the CH-14 keys. Builds on:
`sound-occlusion`, `sound-positional`. Waits on: `script-api-scope` T (sound kinds). Scripts: none
known. Order 14.

**CH-15 Memory and enemy choice** (`creature-memory`, behaviour). 2.3. Data: the CH-15 keys. Builds on:
CH-3, CH-13, CH-14. Waits on: CH-16. Scripts: callers of `best_enemy`, `memory_time`. Order 15.

**CH-17 Stalker planner** (`stalker-planner`, behaviour). 2.4. Data: `need_osoznanie_mode`. Builds on:
CH-15. Waits on: `script-api-scope` W. Scripts: `xr_motivator`, the 19 stalker schemes, `state_mgr_goap`.
Order 16.

**CH-21 Hits, wounds and death** (`creature-hits`, behaviour). 2.4. Data: the CH-21 keys and damage
sections, the critical-hit motions. Builds on: `player-hits`, `environment-hits`, CH-5. Waits on:
weapons (not specified), PH-15. Scripts: `xr_hit`, `xr_death`, `mob_death`. Order 17.

**CH-18 Stalker combat** (`stalker-combat`, behaviour). 2.4. Data: the CH-18 keys. Builds on: CH-8,
CH-15, CH-17. Waits on: CH-20. Scripts: `xr_combat` and the combat types. Order 18.

**CH-19 Stalker danger** (`stalker-danger`, behaviour). 2.4. Data: none of its own. Builds on: CH-15,
CH-17. Waits on: explosives, anomalies' danger. Scripts: `xr_danger`. Order 19.

**CH-20 Stalker weapons** (`stalker-weapons`, behaviour). 2.4. Data: the CH-20 keys. Builds on: CH-6,
CH-17. Waits on: weapons (not specified), PH-17. Scripts: `state_mgr_weapon`. Order 20.

**CH-22 Monster behaviour** (`monster-behaviour`, behaviour). 2.4. Data: the CH-22 keys. Builds on:
CH-12, CH-15, CH-21. Waits on: hits by monsters on the player (`player-hits`). Scripts: the 13 monster
schemes, `mob_state_mgr`. Order 21.

**CH-23 Monster abilities** (`monster-abilities`, behaviour; one spec per kind). 2.4. Data: the CH-23
keys, the particles they name. Builds on: CH-22. Waits on: PH-9 (impulses), `hud-scope` 20. Scripts:
none known. Order 22.

**CH-6 Items and money** (`character-items`, behaviour). 2.1. Data: the CH-6 keys, `supplies`, the
fragments, `death_*.ltx`. Builds on: CH-1, CH-2, `sav` 7.4.2. Waits on: the inventory and trade (#284),
`script-api-scope` S. Scripts: `death_manager`, `trade_manager`. Order 23.

**CH-4 Relation changes** (`relation-changes`, behaviour). 2.1. Data: `[action_points]`,
`[communities_sympathy]`, `[rank_kill_points]`. Builds on: CH-3, CH-21. Waits on: hits by and on
characters. Scripts: callers of `set_relation`. Order 24.

**CH-25 Sounds** (`creature-sounds`, behaviour). 2.4. Data: the CH-25 keys, the voice and monster sound
folders, `script_sound.ltx`. Builds on: `sound-positional`, `sound-ogg-params`. Waits on: the situations
of CH-17 … CH-22. Scripts: `xr_sound`, `sound_theme`. Order 25.

**CH-24 Script control** (`script-control`, behaviour). 2.4. Data: none of its own. Builds on: CH-10,
CH-11, CH-12. Waits on: `script-api-scope` V. Scripts: the callers of the action queue. Order 26.

**CH-28 Smart-terrain jobs** (`smart-terrain-jobs`, behaviour). 2.6. Data: `gulag_*.ltx`,
`smart_terrain*.ltx`, `[smart_terrain_rank_change]`. Builds on: `offline-simulation` 7.2. Waits on:
`script-api-scope` K. Scripts: `smart_terrain`, `xr_gulag`, the `gulag_*` scripts. Order 27.

**CH-29 Offline searches** (`offline-searches`, behaviour). 2.6. Data: the CH-29 keys. Builds on:
`offline-simulation` 7. Waits on: nothing. Scripts: none known. Order 28.

**CH-27 Saved state** (`creature-saved-state`, format and behaviour). 2.6. Data: `sav` 8, the binders'
parts. Builds on: `sav`, CH-5, CH-7. Waits on: what it saves (CH-15 memory, CH-17 planner state, CH-6).
Scripts: `xr_motivator`, `bind_monster`, `xr_logic`, `dialog_manager`, `trade_manager`. Order 29 (the
talk-enabled state and the start dialog, for B-chars 1, can be established earlier with CH-7).

## 6. What can be built on approved specs alone

| Spec | Builds on approved specs alone | Waits on |
|---|---|---|
| CH-1 descriptions | yes | — |
| CH-2 identity | the explicit choice | the random choice's generator |
| CH-3 relations | the tables and names | API Q |
| CH-7 talk state | the state and its changes | API R, M; TC4 |
| CH-5 condition | the values and alive/dead | CH-21, PH-15 |
| CH-8 level graph, CH-16 evaluation | yes | — |
| CH-26 online life | creation and cadence | PH-22 |
| CH-9 … CH-12 movement and motions | path search, speeds | TC1, PH-22, API X |
| CH-13 … CH-15 senses and memory | the rules | lighting, API T, CH-16 |
| CH-17 … CH-23 decisions and combat | — | API W, weapons, PH-15, PH-17, PH-9 |
| CH-6, CH-4, CH-25, CH-24, CH-28, CH-29 | parts | #284, API S, V, K, the situations |
| CH-27 saved state | the binders' parts | all of the above |

## 7. Unestablished

- When and by what rule a profile becomes a specific character, and which random numbers it uses (CH-2).
- What the program makes of the `human` row of `[monster_relations]` with 19 values (CH-3).
- Whether the 112 unnamed keys of section 8 and the 13 unnamed relation keys are read (each spec).
- The 16 non-position bytes of a level vertex; the patrol points' flags and links; whether `level.game` is
  read (CH-8).
- The update cadence of creatures online (CH-26).
- What the client data of creatures holds beyond `sav` 8 (CH-27).
- What `need_osoznanie_mode`, `type`, `monster_type`, `icon` and `Human` of the sections do (their owners
  in section 8 are believed).
- Which speaker the item questions of the trader's start dialog ask (section 4; DG-2).
- Everything here is static: nothing was observed in the running game.

## 8. Coverage

Universe (`coverage.py` V1–V4): **535 names**: the 475 keys of the character and monster sections
(`key:`, as written), the 30 element and attribute names of the character descriptions (`xml:`), the 11
sections of `game_relations.ltx` (`relations:`), and 19 data files and folders (`file:`, `snd:`). Each has
exactly one owner (V5); counts (V6): CH-1 30, CH-2 4, CH-3 8, CH-4 3, CH-5 21, CH-6 11, CH-8 3, CH-9 1,
CH-10 8, CH-11 5, CH-12 32, CH-13 21, CH-14 7, CH-15 8, CH-16 4, CH-17 1, CH-18 10, CH-20 37, CH-21 19,
CH-22 50, CH-23 112, CH-25 74, CH-26 11, CH-28 3, CH-29 5, B-dialogue 1, B-editor 5, B-hud 9, B-offline 5,
B-other 2, B-phys 25. CH-7, CH-19, CH-24 and CH-27 own no name of this universe (their data are the
records, the scripts and the save). The table is `checks/characters-scope/coverage_table.md`, reproduced
here:

| Owner | Count | Names |
|---|---|---|
| CH-1 | 30 | `xml:@id`, `xml:@no_random`, `xml:@team_default`, `xml:actor_dialog`, `xml:bio`, `xml:character/class`, `xml:character/specific_character`, `xml:class`, `xml:community`, `xml:critical_wound_weights`, `xml:crouch_type`, `xml:hit_probability_factor`, `xml:icon`, `xml:map_icon`, `xml:money`, `xml:money@infinitive`, `xml:money@max`, `xml:money@min`, `xml:name`, `xml:panic_threshold`, `xml:panic_treshold`, `xml:rank`, `xml:reputation`, `xml:snd_config`, `xml:start_dialog`, `xml:supplies`, `xml:team`, `xml:terrain_sect`, `xml:terrain_sect_`, `xml:visual` |
| CH-2 | 4 | `key:character_profile`, `key:community`, `key:rank`, `key:spec_rank` |
| CH-3 | 8 | `key:Friend_Community_Overrides`, `relations:[communities_relations]`, `relations:[game_relations]`, `relations:[monster_communities]`, `relations:[monster_relations]`, `relations:[rank_relations]`, `relations:[ranks_properties]`, `relations:[reputation_relations]` |
| CH-4 | 3 | `relations:[action_points]`, `relations:[communities_sympathy]`, `relations:[rank_kill_points]` |
| CH-5 | 21 | `key:Health`, `key:MaxHealthValue`, `key:bleeding_v`, `key:condition_sect`, `key:health_hit_part`, `key:min_wound_size`, `key:morale_v`, `key:power_hit_part`, `key:psy_health_v`, `key:radiation_health_v`, `key:radiation_v`, `key:satiety_critical`, `key:satiety_health_v`, `key:satiety_power_v`, `key:satiety_v`, `key:sleep_health`, `key:sleep_power`, `key:sleep_psy_health`, `key:sleep_radiation`, `key:sleep_satiety`, `key:wound_incarnation_v` |
| CH-6 | 11 | `file:config\misc\death_*.ltx`, `key:Spawn_Inventory_Item_Probability`, `key:Spawn_Inventory_Item_Section`, `key:attachable_items`, `key:can_select_items`, `key:cost`, `key:inv_max_weight`, `key:max_item_mass`, `key:money`, `key:supplies`, `key:use_single_item_rule` |
| CH-8 | 3 | `file:levels\*\level.ai`, `file:levels\*\level.game`, `file:spawns\all.spawn chunk 3 (patrol paths)` |
| CH-9 | 1 | `key:SpaceRestrictionSection` |
| CH-10 | 8 | `key:bone_head`, `key:bone_model`, `key:bone_shoulder`, `key:bone_spin`, `key:bone_torso`, `key:max_left_torso_angle`, `key:max_right_torso_angle`, `key:movement_speeds` |
| CH-11 | 5 | `file:meshes\actors\*.omf (except critical_hit_grup_*)`, `key:LegsCount`, `key:crouch_type`, `key:foot_bones`, `key:step_params` |
| CH-12 | 32 | `file:meshes\monsters\*.omf`, `key:Accel_Aggressive`, `key:Accel_Calm`, `key:Accel_Generic`, `key:CrouchFactor`, `key:DamagedPanicFactor`, `key:DamagedRunFactor`, `key:DamagedRunFreeFactor`, `key:DamagedWalkFactor`, `key:DamagedWalkFreeFactor`, `key:MaxSpeed`, `key:MinSpeed`, `key:PanicFactor`, `key:RunBackFactor`, `key:RunFactor`, `key:RunFreeFactor`, `key:Velocity_Drag`, `key:Velocity_MoveBkwd`, `key:Velocity_MoveFwd`, `key:Velocity_RunFwdDamaged`, `key:Velocity_RunFwdNormal`, `key:Velocity_Stand`, `key:Velocity_Steal`, `key:Velocity_WalkFwdDamaged`, `key:Velocity_WalkFwdNormal`, `key:Velocity_Walk_Upper`, `key:WalkBackFactor`, `key:WalkFactor`, `key:WalkFreeFactor`, `key:step_effector_amplitude`, `key:step_effector_period_number`, `key:step_effector_time` |
| CH-13 | 21 | `key:always_visible_distance`, `key:always_visible_distance_danger`, `key:always_visible_distance_free`, `key:decrease_value`, `key:eye_fov`, `key:eye_range`, `key:far_plane_factor`, `key:fog_density_factor`, `key:luminocity_factor`, `key:max_view_distance`, `key:max_view_distance_danger`, `key:max_view_distance_free`, `key:min_view_distance`, `key:min_view_distance_danger`, `key:min_view_distance_free`, `key:time_quant`, `key:transparency_threshold`, `key:velocity_factor`, `key:visibility_threshold`, `key:vision_danger_section`, `key:vision_free_section` |
| CH-14 | 7 | `key:SoundThreshold`, `key:max_hear_dist`, `key:self_decrease_factor`, `key:self_decrease_quant`, `key:self_sound_factor`, `key:sound_perceive_section`, `key:sound_threshold` |
| CH-15 | 8 | `key:Anomaly_Detect_Radius`, `key:Anomaly_Detect_Time_Remember`, `key:DynamicHitCount`, `key:DynamicObjectsCount`, `key:DynamicSoundsCount`, `key:ignore_monster_threshold`, `key:max_ignore_distance`, `key:species` |
| CH-16 | 4 | `file:ai\*\*.efd`, `key:ef_creature_type`, `key:ef_detector_type`, `key:ef_weapon_type` |
| CH-17 | 1 | `key:need_osoznanie_mode` |
| CH-18 | 10 | `key:panic_threshold`, `key:selector_approach`, `key:selector_attack`, `key:selector_cover`, `key:selector_free_hunting`, `key:selector_getaway`, `key:selector_hear_sound`, `key:selector_reload`, `key:selector_retreat`, `key:selector_walk_around` |
| CH-20 | 37 | `key:FireRandomMax`, `key:FireRandomMin`, `key:MaxMissDistance`, `key:MaxMissFactor`, `key:MinMissDistance`, `key:MinMissFactor`, `key:NoFireTimeMax`, `key:NoFireTimeMin`, `key:bone_fire`, `key:bone_head_weapon`, `key:bone_torso_weapon`, `key:disp_crouch_factor`, `key:disp_jump_factor`, `key:disp_run_crouch`, `key:disp_run_stand`, `key:disp_stand_crouch`, `key:disp_stand_crouch_zoom`, `key:disp_stand_stand`, `key:disp_stand_stand_zoom`, `key:disp_vel_factor`, `key:disp_walk_crouch`, `key:disp_walk_stand`, `key:weapon_bone0`, `key:weapon_bone1`, `key:weapon_bone2`, `key:weapon_max_queue_interval_close`, `key:weapon_max_queue_interval_far`, `key:weapon_max_queue_interval_medium`, `key:weapon_max_queue_size_close`, `key:weapon_max_queue_size_far`, `key:weapon_max_queue_size_medium`, `key:weapon_min_queue_interval_close`, `key:weapon_min_queue_interval_far`, `key:weapon_min_queue_interval_medium`, `key:weapon_min_queue_size_close`, `key:weapon_min_queue_size_far`, `key:weapon_min_queue_size_medium` |
| CH-21 | 19 | `file:meshes\actors\critical_hit_grup_*.omf`, `key:body_parts_section_id`, `key:critical_wound_anim_head`, `key:critical_wound_anim_legs`, `key:critical_wound_anim_torso`, `key:critical_wound_bones_head`, `key:critical_wound_bones_legs`, `key:critical_wound_bones_torso`, `key:critical_wound_decrease_quant`, `key:critical_wound_threshold`, `key:critical_wound_weights`, `key:damage`, `key:explosion_immunity`, `key:fire_wound_immunity`, `key:hit_power`, `key:hit_type`, `key:immunities_sect`, `key:killer_clsids`, `key:power_fx_factor` |
| CH-22 | 50 | `key:DamagedThreshold`, `key:DayTime_Begin`, `key:DayTime_End`, `key:MaxAttackDist`, `key:Max_Satiety`, `key:Melee_Rotation_Factor`, `key:MinAttackDist`, `key:Min_Satiety`, `key:Morale_Attack_Success_Quant`, `key:Morale_Despondent_Speed`, `key:Morale_Despondent_Threashold`, `key:Morale_Hit_Quant`, `key:Morale_Stable_Speed`, `key:Morale_Take_Heart_Speed`, `key:RunAttack_PathDistance`, `key:RunAttack_StartDistance`, `key:Run_Attack_Delay`, `key:Run_Attack_Dist`, `key:Velocity_JumpGround`, `key:Velocity_JumpPrepare`, `key:Velocity_Jump_Stand`, `key:Velocity_RunAttack`, `key:Velocity_Run_Attack`, `key:actor_hit_effect`, `key:actor_restrictor`, `key:anger_hunger_threshold`, `key:anger_loud_threshold`, `key:as_min_dist`, `key:as_step`, `key:attack_params`, `key:berserk_always`, `key:distance_to_corpse`, `key:eat_freq`, `key:eat_slice`, `key:eat_slice_weight`, `key:jump_build_line_distance`, `key:jump_delay`, `key:jump_factor`, `key:jump_ground_trace_range`, `key:jump_hit_trace_range`, `key:jump_max_angle`, `key:jump_max_dist`, `key:jump_max_distance`, `key:jump_max_height`, `key:jump_min_dist`, `key:jump_min_distance`, `key:jump_trace_dist`, `key:satiety_threshold`, `key:squad_attack_algorithm`, `key:weapon_usage` |
| CH-23 | 112 | `key:Control_Hit`, `key:Delay_Flame_Aggressive`, `key:Delay_Flame_Min`, `key:Delay_Flame_Normal`, `key:Delay_Scare_Aggressive`, `key:Delay_Scare_Min`, `key:Delay_Scare_Normal`, `key:Delay_Tele_Aggressive`, `key:Delay_Tele_Min`, `key:Delay_Tele_Normal`, `key:EffectDistance`, `key:FakeDeathCount`, `key:Gravi_Hit_Power`, `key:Gravi_Impulse_To_Enemy`, `key:Gravi_Impulse_To_Objects`, `key:Gravi_Radius`, `key:Gravi_Speed`, `key:Gravi_Step`, `key:Gravi_Time_To_Hold`, `key:Hidden_Particles`, `key:HugeKick_Damage`, `key:HugeKick_MinMaxDelay`, `key:HugeKick_MinMaxDist`, `key:HugeKick_Particles`, `key:HugeKick_Time_SlowDown`, `key:ImpulseMax`, `key:ImpulseMin`, `key:Invisibility_BlinkMicroInterval`, `key:Invisibility_BlinkTime`, `key:Invisibility_EnergySpeed`, `key:Invisible_Energy_Activate_Value`, `key:Invisible_Energy_Aggressive_Restore_Velocity`, `key:Invisible_Energy_Critical_Value`, `key:Invisible_Energy_Decline_Velocity`, `key:Invisible_Energy_Restore_Velocity`, `key:Max_Controlled_Count`, `key:Max_Phantoms_Count`, `key:Min_Phantoms_Count`, `key:Particle_Gravi_Prepare`, `key:Particle_Gravi_Wave`, `key:Particle_Invisible`, `key:Particle_Shield`, `key:Particle_Tele_Object`, `key:Particles_Damage`, `key:Particles_Death`, `key:Particles_Hidden`, `key:Particles_Idle`, `key:Particles_Invisible_Tracks`, `key:Particles_Invisible_Tracks_Freq`, `key:Predator_Visual`, `key:PsyAura_Damage`, `key:PsyAura_Fake_Delay`, `key:PsyAura_Fake_Duration`, `key:PsyAura_Fake_MaxAddDist`, `key:PsyAura_Fake_MinAddDist`, `key:PsyAura_HitDelay`, `key:PsyAura_PPHitDelay`, `key:PsyAura_Radius`, `key:StartFakeDeathHealthThreshold`, `key:Tele_Delay_Between_Objects_Raise_Time`, `key:Tele_Delay_Between_Objects_Time`, `key:Tele_Distance`, `key:Tele_Find_Radius`, `key:Tele_Fly_Time`, `key:Tele_Fly_Velocity`, `key:Tele_Hold_Time`, `key:Tele_Max_Handled_Objects`, `key:Tele_Object_Count`, `key:Tele_Object_Height`, `key:Tele_Object_Max_Mass`, `key:Tele_Object_Min_Mass`, `key:Tele_Raise_Speed`, `key:Tele_Time_Object_Keep`, `key:Tele_Time_To_Hold`, `key:Tele_Wait_Time`, `key:Time_Phantom_Appear`, `key:Vampire_Delay`, `key:Vampire_Reach_Time`, `key:Vampire_Want_Speed`, `key:Vampire_Wound`, `key:Velocity_Invisible_Angular`, `key:Velocity_Invisible_Linear`, `key:control_fx_texture`, `key:control_fx_texture2`, `key:flame_aura_radius`, `key:flame_fire_time_delay`, `key:flame_fire_time_play`, `key:flame_hit_delay`, `key:flame_hit_value`, `key:flame_length`, `key:flame_max_dist`, `key:flame_max_height`, `key:flame_min_dist`, `key:flame_min_height`, `key:flame_particles_fire`, `key:flame_particles_prepare`, `key:flame_particles_stop`, `key:flame_scan_delay_min_max`, `key:flame_scan_radius`, `key:flames_count`, `key:flames_delay`, `key:particles_appear`, `key:particles_disappear`, `key:psy_hit_damage`, `key:scan_critical_value`, `key:scan_decrease_value`, `key:scan_radius`, `key:scan_trace_time_freq`, `key:scan_velocity_threshold`, `key:tube_at_once`, `key:tube_damage`, `key:tube_min_dist` |
| CH-25 | 74 | `file:config\misc\script_sound.ltx`, `key:PsyAura_HeadSound`, `key:PsyAura_SoundLeftPath`, `key:PsyAura_SoundRightPath`, `key:Sound_Alien`, `key:Sound_Growl`, `key:Sound_Idle`, `key:Sound_Invisibility_Change_State`, `key:Sound_Vampire_Grasp`, `key:Sound_Vampire_Hit`, `key:Sound_Vampire_StartHunt`, `key:Sound_Vampire_Sucking`, `key:attack_sound_delay`, `key:distant_idle_sound_delay`, `key:distant_idle_sound_range`, `key:eat_sound_delay`, `key:flame_scan_sound`, `key:flame_sound`, `key:idle_sound_delay`, `key:scan_sound`, `key:sound_aggressive`, `key:sound_alarm`, `key:sound_anomaly_death`, `key:sound_attack`, `key:sound_attack_allies_several_enemies`, `key:sound_attack_allies_single_enemy`, `key:sound_attack_hit`, `key:sound_attack_no_allies`, `key:sound_backup`, `key:sound_bkgnd`, `key:sound_control_hit`, `key:sound_control_start`, `key:sound_death`, `key:sound_detour`, `key:sound_die`, `key:sound_die_in_anomaly`, `key:sound_distant_idle`, `key:sound_eat`, `key:sound_enemy_critically_wounded`, `key:sound_enemy_killed_or_wounded`, `key:sound_friendly_fire`, `key:sound_friendly_grenade_alarm`, `key:sound_gravi_attack`, `key:sound_gravi_wave`, `key:sound_grenade_alarm`, `key:sound_growling`, `key:sound_hit`, `key:sound_humming`, `key:sound_idle`, `key:sound_kill_wounded`, `key:sound_landing`, `key:sound_need_backup`, `key:sound_panic`, `key:sound_panic_human`, `key:sound_panic_monster`, `key:sound_psy_attack`, `key:sound_running_in_danger`, `key:sound_search`, `key:sound_search1_no_allies`, `key:sound_search1_with_allies`, `key:sound_steal`, `key:sound_surrender`, `key:sound_take_damage`, `key:sound_tele_attack`, `key:sound_tele_hold`, `key:sound_tele_throw`, `key:sound_threaten`, `key:sound_threaten_hit`, `key:sound_threaten_start`, `key:sound_tolls`, `key:sound_walking_in_danger`, `snd:sounds\characters_voice\human_*\`, `snd:sounds\characters_voice\scenario\`, `snd:sounds\monsters\` |
| CH-26 | 11 | `file:config\misc\respawn.ltx`, `key:Human`, `key:Scheduled`, `key:class`, `key:corpse_visual`, `key:icon`, `key:monster_type`, `key:scheduled`, `key:script_binding`, `key:type`, `key:visual` |
| CH-28 | 3 | `file:config\misc\gulag_*.ltx`, `file:config\misc\smart_terrain*.ltx`, `relations:[smart_terrain_rank_change]` |
| CH-29 | 5 | `key:distance_to_search_for_artefacts`, `key:going_item_detect_probability`, `key:search_item_detect_probability`, `key:search_speed`, `key:time_to_search_for_artefacts` |
| B-dialogue | 1 | `snd:sounds\characters_voice\dialogs\` |
| B-editor | 5 | `file:config\stalker_custom_data.ltx`, `key:$npc`, `key:$prefetch`, `key:$spawn`, `key:custom_data` |
| B-hud | 9 | `key:appear_effector`, `key:attack_effector`, `key:aura_effector`, `key:aura_effector_hit`, `key:control_effector`, `key:flame_scan_effector_section`, `key:scan_effector_section`, `key:threaten_effector`, `key:vampire_effector` |
| B-offline | 5 | `file:levels\*\level.gct`, `key:current_level_going_speed`, `key:going_speed`, `key:smart_terrain_choose_interval`, `key:terrain` |
| B-other | 2 | `key:GroupControlSection`, `snd:sounds\characters_voice\multiplayer\` |
| B-phys | 25 | `key:cform`, `key:destroyed_vis_name`, `key:material`, `key:ph_after_death_velocity_factor`, `key:ph_box0_center`, `key:ph_box0_size`, `key:ph_box1_center`, `key:ph_box1_size`, `key:ph_character_have_wounded_state`, `key:ph_collision_damage_factor`, `key:ph_crash_speed_max`, `key:ph_crash_speed_min`, `key:ph_foot_size`, `key:ph_mass`, `key:ph_pelvis_factor_low_pose_detect`, `key:ph_skel_fatal_impulse_factor`, `key:ph_skel_shot_up_factor`, `key:ph_skeleton_airr_ang_factor`, `key:ph_skeleton_airr_lin_factor`, `key:ph_skeleton_ddelay`, `key:ph_skeleton_hinger_factor1`, `key:ph_skeleton_skin_ddelay`, `key:ph_skeleton_skin_ddelay_after_wound`, `key:ph_skeleton_skin_friction_end`, `key:ph_skeleton_skin_friction_start` |

## Provenance

- Date: 2026-11-27. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binary `bin\xrGame.dll`.
- Data examined: winning copies of `config\`, `scripts\` and `spawns\all.spawn` and the archive listing,
  as extracted for the approved `pda-scope` (`checks/pda-scope/extract.py`); `game.graph` as extracted for
  an earlier specification of the level's objects (its layout: `offline-simulation` 1); the shipped API listing `scripts\lua_help.script`; the approved specifications listed
  above.
- Analysis: reading of the spawn records with a reader written from `sav` 7.2–7.4 and
  `offline-simulation` 1; reading of the configuration with key case kept (`ltx-config` 6); searches of
  the character, relation and creature files and of the scripts; printable-string scans of
  `xrGame.dll`. No disassembly, debugging or execution; the game was not run.
- Checks: `checks/characters-scope/checks.py` and `coverage.py`, run with `python3 -I` (`run_all.py` runs
  both), with `lib.py`, `owners.py` and the `pda-scope` helpers `tree.py`, `ltx.py`; all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- No spec owns what reading a specific character does with a `start_dialog` or `actor_dialog` id that no
  dialog file defines (`dialogue-scope` 1.3's 8 undefined ids, part of B-chars 2); CH-1 does not list it.
- CH-26's waits-on omits `script-api-scope` J and K, through which creation during play (`se_respawn`,
  the simulation registry) reaches the engine.
