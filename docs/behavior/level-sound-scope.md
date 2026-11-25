# Sound of the first level (scoping specification)

Scoping specification for what the player hears on the first level, `levels\l01_escape\`, during
normal play after New Game. It proposes a split into small specifications, one format or one behaviour
each, each reviewable and implementable on its own. It states only which data each one covers and what
it depends on; the rules themselves are for the proposed specifications. What could not be established
here is marked **unestablished**.

Conventions: paths are stored paths of the combined game-data tree (`docs/formats/db-archive.md`, the
later archive wins); a sound name *n* names the file `sounds\<n>.ogg` (`docs/behavior/title-menu.md`
section 6); "exists" means that file is in the archives. Text files are Windows-1251, LTX per
`docs/formats/ltx-config.md` (including `#include` and section inheritance). "Records" are records of
`spawns\all.spawn` on `l01_escape` (`docs/behavior/level-objects.md` section 2.1 and table 7.1).

**Checks.** Every factual claim carries a tag [D*n*]. The checks are scripts in the dirty workspace,
`checks/level-sound-scope/`; `run_all.py` extracts the needed files from the archives (`extract.py`) and
runs them all: `check_data.py` (D1 – D9, D11 – D13), `check_objects.py` (D10), `check_keys.py` (D14),
`check_ogg.py` (D15), `check_breaking.py` (D16 – D20), `check_complete.py` (D21, D22, D24, D25),
`check_levelfiles.py` (D23), `check_more.py` (D26 – D28), `check_global.py` (D29 – D31).

## 1. Already specified elsewhere

Not part of this split:

- The title menu's music, also played while the menu is open during play: `title-menu.md` section 6
  (`config\ui\ui_mm_main.xml`).
- The intro movie after New Game: `docs/behavior/level-loading.md`, `docs/behavior/startup.md`
  (`config\ui\ui_movies.xml`).
- The inventory screen's sounds: `docs/behavior/inventory-screen.md` section 7
  (`config\ui\inventory_new.xml`, `inventory_new_16.xml`).
- The volume settings `snd_volume_eff`, `snd_volume_music` and the setting `snd_efx`:
  `docs/behavior/options-screens.md`.

## 2. Completeness

The split was checked against every place the data in play names a sound:

- **LTX, reached from the data in play.** Starting from the sections of all 870 records, the player's
  starting inventory (`player-state.md` section 3.1: `detector_simple`, `device_torch`,
  `novice_outfit`, `wpn_binoc`, `device_pda`), the sections `[actor]`, `[sound]`, `[maingame_ui]` and
  the weather cycle `[sect_default_weather]`, and following every key or value that names another
  section, every key whose value names an existing sound file or whose name contains `snd` or `sound`
  is assigned to exactly one specification of section 3 [D21]. Counted as (starting section, section,
  key) triples these are 707, as distinct (section, key) pairs 414, in 65 sections [D21, D29]. The music
  files are listed with S5.
- **LTX, everything else.** Of all sections of the `config\system.ltx` include tree, 404 name a sound
  or have such a key; the 339 not reached from the data in play each have an owner [D29]:
  - `[bullet_manager]` (named by no record or item; its name occurs in the program files): S20 [D29];
  - the other weather sections, not used by the `default` weather (`ambient_env_indoor`,
    `ambient_env_stancia1`, `ambient_env_stancia2`, `ambient_env_x18`, `ae0_effect_9`): S4;
    `sect_thunderbolt_stancia_00` … `_03`: S8 [D29];
  - multiplayer sections (`mp_*`, `*_snd_messages`, `*_messages_menu`; 49): out of this area [D29];
  - sections of other objects, items and characters, by class [D29]: weapons (classes `WP_*`, `W_*`;
    63) S19; grenades and rockets (`G_*`) and explosives (`II_EXPLO`) (10) S18; anomalies (`Z_*`,
    `ZS_*`; 49) S15; detectors (`D_SIMDET`; 2) S21; characters and monsters (`AI_*`, `SM_*`; 156) S24.
    None of them is reached from the data in play; they matter on `l01_escape` only if such an object
    is created during play (by trade, loot or scripts, not followed here), and then belong to the
    specification of their kind.
- **Program files.** The shipped program files themselves contain 13 strings that name existing sound
  files [D30]: `ambient\rain` (S8); `affects\tinnitus3a` (S17; also the `explosion` entry of
  `[actor_hit_snds]` [D7]); `zone_mosquito_bald` (S15, S21); `car\test_car_start`, `car\test_car_stop`
  and eight names under `monsters\controller\` (no section or record in play names them, and no
  `l01_escape` record has the controller's class `SM_CONTR` [D30]): S24, to be scoped with the
  characters, monsters and scripts.
- **UI files.** Of `config\**\*.xml`, 11 files name an existing sound file [D22]: `inventory_new.xml`,
  `inventory_new_16.xml` (section 1), `ui_mm_main.xml` (section 1), `maingame.xml`, `maingame_16.xml`
  (S23), `game_tutorials.xml` (S24); `inventorymp.xml`, `mp_buy_menu_buttons.xml` (multiplayer; out of
  this area); `ui_credits.xml` (the credits; out of this area); `ui_custom_msgs.xml` and
  `ui_videoplayer.xml`, where `car\trabantnoise` is a texture and a video file attribute, not a sound.
- **Level files.** Of the 30 files directly in `levels\l01_escape\`, only `level.snd_static` (S3) and
  `level.spawn` contain a string naming a sound file [D23]; `level.spawn` names `alexmx\helicopter`,
  `ambient\dogs_1` and `characters_voice\scenario\escape\heli_radio_part_1`, the same three names that
  records carry in their own data (next item). `level.som` (S6) and `level.cform` (S6, S11, S13) hold no
  sound names but are used for sound.
- **Records' own data.** Three records name sounds in their state: `esc_heli` (`C_HLCP_S`,
  `alexmx\helicopter`), and the restrictors `esc_heli_attack_zone` and `esc_dogs_sound` (`SPC_RS_S`, in
  custom data with a `[logic]` section) [D25]: all S24.
- **Models.** Of the 115 model files named by the records, only
  `meshes\physics\decor\bottle_bar_stakan.ogf` and `meshes\weapons\vodka\vodka.ogf` contain sound keys
  [D24] (S14). The 14 model files (world and hand models) of the starting inventory and of the level's
  weapons contain none [D24].
- **Game materials.** `gamemtl.xr` chunk 0x1003 (S9, used by S11 – S14, S20).
- **Scripts.** 22 of the 439 shipped `scripts\*.script` files use the script identifier
  `sound_object` [D13] (S24).

**Where the program reads the data.** Each of these key or file names occurs in the shipped program
files and in none of the 439 scripts [D14]: `level.snd_static`, `level.som`, `level.snd_env`,
`occlusion_scale`, `music_tracks`, `env_ambient`, `rain_density`, `thunderbolt`, `step_params`,
`hit_sounds`, `heavy_breath_snd`, `heavy_blood_snd`, `idle_sound`, `blowout_sound`, `entrance_sound`,
`snd_collide_min_volume`, `snd_new_contact`, `zone_sound_`, `snd_shoot`, `break_sound`,
`snd_night_vision`, `snd_explode`, `snd_checkout`, `artefact_activation_seq`, `hit_break_threthhold`;
the program files also name `senvironment.xr` [D14]. So these are played by the program without scripts
(believed: the name occurring is evidence that the program reads it, not proof of how).

## 3. Proposed specifications

Each entry gives: what it specifies (data and observable behaviour); files; dependencies; parts that
need scripts or AI.

### S1. Sound parameters in `.ogg` files

- **Specifies:** the format of the per-file sound parameters a `.ogg` file carries in its Vorbis
  comment header (each of the five files of S3 has exactly one comment, 24 bytes, not text) [D15].
- **Files:** `sounds\…\*.ogg` (the files named by S3 – S23).
- **Depends on:** `title-menu.md` section 6 (Ogg Vorbis files).
- **Scripts or AI:** none.

### S2. Positional sound playback

- **Specifies:** how a sound is heard: placed at a world position (distance attenuation from the S1
  parameters, the listener) or listener-relative ("2D", as the menu music of `title-menu.md` section 6),
  the volume category, looping and stopping. Every specification below plays its sounds through it;
  which of its sounds are listener-relative is for each of them to establish.
- **Files:** none of its own.
- **Depends on:** S1; `title-menu.md` section 6 (volume categories); `options-screens.md` (volumes);
  `player-movement.md` sections 8, 9 (the camera).
- **Scripts or AI:** none.

### S3. Static sound sources (`level.snd_static`)

- **Specifies:** the format of `level.snd_static` and the rule by which its sources play.
- **Files:** `levels\l01_escape\level.snd_static` (435 bytes, 5 sources: `nature\new_flies1`,
  `ambient\sparks1`, `scripts\magnitofon\magnitofon_2`, `script_replics\radio\message\radio_noise_1`,
  `ambient\indoors\small_fan`; all five files exist) [D1, D2].
- **Depends on:** S2; `docs/behavior/sky.md` (game time).
- **Scripts or AI:** none (the two sources under `scripts\` and `script_replics\` are named by this
  file, not by a script) [D2, D14].

### S4. Ambient sounds of the weather

- **Specifies:** which sounds of the weather's ambient sets and of their effects play, when and where.
- **Files:** `config\game_maps_single.ltx` (`[l01_escape]` `weathers = default`) [D3];
  `config\weathers\environment.ltx` (`[weathers]` `default = sect_default_weather`, 24 hourly sections)
  [D4], which includes `config\weathers\weather_default.ltx`, the file defining the hourly sections
  [D19]; key `env_ambient` of those sections, naming 7 distinct sets, each a section of
  `config\weathers\env_ambient.ltx` (which has 21 sections: 11 sets and 10 effect sections) [D4, D19];
  their `sounds` lists (80 distinct names) and their `effects` (9 distinct effect sections, whose
  `sound` keys name `ambient\rnd_outdoor\rnd_wind_1`, `_2`, `_3`); all these files exist [D4].
- **Depends on:** S2; `sky.md` (game time, the hourly weather sections and their blend).
- **Scripts or AI:** none. The effects' particles and wind values are not sound: they belong to a
  particle specification (none approved).

### S5. Level music

- **Specifies:** which music tracks play during play on the level and when.
- **Files:** `config\game_maps_single.ltx` (`[l01_escape]` `music_tracks = l01_escape_musics`);
  `config\game_levels_music.ltx` section `[l01_escape_musics]`: two lines, keys `music\amb01` and
  `music\amb20`, each with five numbers; `sounds\music\amb01_l.ogg`, `amb01_r.ogg`, `amb20_l.ogg`,
  `amb20_r.ogg` exist [D3]. The meaning of the five numbers is **unestablished** here.
- **Depends on:** S2; `title-menu.md` section 6 (the `_l` / `_r` pair, the music category); `sky.md`
  (game time).
- **Scripts or AI:** none known; whether scripts also start or stop the level music is **unestablished**.

### S6. Sound occlusion

- **Specifies:** how walls and the level's occlusion mesh lower a sound's volume between it and the
  listener; the format of `level.som`.
- **Files:** `levels\l01_escape\level.som` (1,384 bytes, a chunked file starting with chunk 0 of 4
  bytes) [D1, D2]; `levels\l01_escape\level.cform`; `config\system.ltx` `[sound]` `occlusion_scale = 0.4`
  [D6].
- **Depends on:** S2; `docs/formats/level-cform.md`; `docs/formats/chunks.md`.
- **Scripts or AI:** none.

### S7. Reverb

- **Specifies:** whether and how environmental reverb is applied to the level's sounds.
- **Files:** `senvironment.xr` [D1]; `level.snd_env` exists for 5 levels and **not** for `l01_escape`
  [D1]; setting `snd_efx` (`options-screens.md`). What is heard on `l01_escape` is **unestablished**.
- **Depends on:** S2.
- **Scripts or AI:** none.

### S8. Rain and thunder

- **Specifies:** the rain sound and the thunder sounds of the weather.
- **Files:** `config\weathers\weather_default.ltx`: exactly four of the 24 hourly sections
  (`default_weather_01`, `_12`, `_16`, `_17`) have a non-zero `rain_density`, and exactly they have a
  `thunderbolt` key, `thunderbolt_collection_default` [D5]; that section of
  `config\weathers\thunderbolt.ltx` lists four sections whose `sound` keys are `nature\thunder-0` …
  `nature\thunder-3`; those files and `sounds\ambient\rain.ogg` exist [D5]. Which file or files the rain
  sound plays is **unestablished** here; the program files contain the name `ambient\rain` [D30].
- **Depends on:** S2, S6; `sky.md` (hourly sections, rain).
- **Scripts or AI:** none. The lightning's picture is not sound (a sky specification).

### S9. Material pairs (`gamemtl.xr` chunk 0x1003)

- **Specifies:** the format of the material-pair list: the pair header and the sound strings of each
  pair, and whether a pair linked to another pair takes values from it.
- **Files:** `gamemtl.xr` chunk 0x1003: 1,247 pairs, each with sub-chunks 0x1000, 0x1002, 0x1003, 0x1005
  [D8]. Sub-chunk 0x1000 is 20 bytes in every pair: five `u32`, the first two material ids, the third the
  pair's id; the fourth is 0xFFFFFFFF in 372 pairs and the id of another existing pair in the other 875;
  the fifth is not examined here [D8, D26]. Of the 57 pairs with `creatures\actor`, 9 have such a link
  and 4 have an empty sub-chunk 0x1003 (`objects\bullet`, `materials\fake_ladders`, `creatures\phantom`,
  `materials\earth_death`) [D26]. Whether a linked pair inherits the strings of the pair it names is
  **unestablished**; every count of pair strings in this document (S11, S13, S14, S20) is of stored
  values and may differ from what plays.
- **Depends on:** `level-cform.md` section 3.1 (the file and the materials).
- **Scripts or AI:** none.

### S10. Animation cycle lengths of the player's model (`.omf`)

- **Specifies:** the part of the motion file format needed to know each animation's name and duration
  (no bone data).
- **Files:** `meshes\actors\stalker_animation.omf`, named by the player's model
  `meshes\actors\hero\stalker_novice.ogf` (the visual of the player's record; the model file contains
  the string `actors\stalker_animation`) [D9]; it contains the animation names `norm_walk_fwd_0`,
  `norm_run_fwd_0`, `norm_escape_00`, `cr_walk_fwd_0` and `norm_jump_end` [D9].
- **Depends on:** `docs/formats/ogf.md`; `docs/formats/sav.md` (the player's record and its visual).
- **Scripts or AI:** none.

### S11. Footsteps and landings of the player

- **Specifies:** when the player's footsteps play (step times within the leg animation chosen from the
  movement state, including the landing animation) and which step sound plays on which ground material.
  It uses only sub-chunk 0x1003 of the pairs; any collision sound of the player's body, including the
  impact of a landing, is S13's.
- **Files:** `config\creatures\actor.ltx` `[actor]`: `material = creatures\actor`,
  `step_params = stalker_step_manager` [D7]; section `[stalker_step_manager]` of
  `config\creatures\m_stalker.ltx`: 37 lines keyed by animation name, five numbers each [D7, D9]; the 57
  pairs of `creatures\actor` (one with each of the 57 materials; material id 90), whose stored
  sub-chunks 0x1003 name 40 distinct sounds, all present [D8].
- **Depends on:** S2, S9, S10; `level-cform.md` (ground material, airborne); `player-movement.md`
  (movement state, landing).
- **Scripts or AI:** none.

### S12. Climbing

- **Specifies:** whether climbing a ladder makes a sound, and which.
- **Files:** the 14 `O_CLMBL` records (ladders) [D28]; the pair `creatures\actor` /
  `materials\fake_ladders` has an empty stored sub-chunk 0x1003 [D26]. Whether climbing makes any sound
  is **unestablished**.
- **Depends on:** S2, S9, S11; a specification of climbing (`player-movement.md` section 10 leaves it
  out; none approved).
- **Scripts or AI:** none.

### S13. Contact sounds

- **Specifies:** sounds when bodies hit each other or the level (the player's body, landing impacts
  included, and the level's physics objects), by material pair and impact.
- **Files:** sub-chunk 0x1005 of the pairs (format: S9): its first string names 111 distinct sounds,
  all present [D8]. 50 pairs name sounds under `material\bullet\collide\`: 47 of the 57 pairs with
  `objects\bullet`, and the pairs `materials\water` / `objects\knife`, `objects\glass` / `objects\glass`,
  `objects\glass` / `objects\small_weapon` [D17]. Contacts with `objects\bullet` are S20's; all other
  pairs, these three included, are S13's. `config\system.ltx` `[sound]`
  `snd_collide_min_volume = 0.1`, `snd_collide_max_volume = 200.` [D6].
- **Depends on:** S2, S6, S9; `level-cform.md`; a specification of physics objects' motion (none
  approved). Which contacts of the player's own body play a sound is **unestablished** here.
- **Scripts or AI:** none; contacts caused by characters need AI.

### S14. Breaking sounds

- **Specifies:** the sound played when an object or item breaks.
- **Files:**
  - sub-chunk 0x1002 of the pairs (format: S9): a string, empty in 1,245 pairs and
    `material\bottle\collide\bottle_break_1` (an existing file) in two, `objects\bottle` /
    `objects\bottle` and `objects\glass` / `materials\asphalt` [D16];
  - item sections `[vodka]` (4 records) and `[energy_drink]` (13 records), class `II_BOTTL`:
    `break_sound = material\bottle\collide\bottle_break_1` and a `break_particles` key [D27];
  - embedded configuration text with a `[sound]` section and
    `break_sound = Material\Bottle\collide\bottle_break_1` in two models: `meshes\weapons\vodka\vodka.ogf`
    (the 4 `vodka` records) and `meshes\physics\decor\bottle_bar_stakan.ogf` (12 `P_DSTRBL` records); no
    other model of the 196 physics objects (`P_DSTRBL` 127, `O_PHYS_S` 69; 39 model files) has sound
    keys [D18, D24];
  - the 49 breakable objects (`O_BRKBL`, section `[breakable_object]` with `hit_break_threthhold`,
    `collision_break_threthhold`, `remove_time` and no sound key; models
    `levels\l01_escape\meshes\brkbl#0.ogf` … `brkbl#48.ogf`, none containing the text `sound`) [D27].
    Whether breaking them makes a sound (e.g. through S9 or S13) is **unestablished**.
- **Depends on:** S2, S6, S9; the models' embedded configuration (not covered by `ogf.md`); a
  specification of object and item destruction (none approved).
- **Scripts or AI:** none when the player breaks an object; breaking by other characters needs AI.

### S15. Anomaly sounds

- **Specifies:** the sounds of the level's anomalies: idle, and when something enters them, is hit or
  they discharge.
- **Files:** the 178 anomaly records (class ids `Z_MBALD`, `ZS_MBALD`, `ZS_MINCE`, `Z_RADIO`) have 8
  sections, each defined in one of the files `config\misc\zone_*.ltx` [D10]:

  | Class id | Section | Records | `sound` | Other sound keys |
  |---|---|---|---|---|
  | `ZS_MBALD` | `zone_mosquito_bald_weak` | 85 | `zone_mosquito_bald` | idle, blowout, hit, entrance, artefact born |
  | `ZS_MBALD` | `zone_mosquito_bald_weak_noart` | 2 | `zone_mosquito_bald` | same |
  | `ZS_MBALD` | `zone_witches_galantine` | 9 | `zone_mosquito_bald` | idle, blowout, hit, entrance |
  | `ZS_MINCE` | `zone_mincer_weak_noart` | 4 | `zone_mosquito_bald` | idle, awake, accum, blowout, hit, entrance, body tearing, artefact born |
  | `Z_MBALD` | `zone_flame_small` | 11 | empty | idle, blowout, hit, entrance |
  | `Z_MBALD` | `zone_mine_field` | 49 | empty | blowout, hit, entrance |
  | `Z_RADIO` | `zone_radioactive_average`, `zone_radioactive_killing` | 1, 17 | `zone_mosquito_bald` | none |

  (Other keys: `idle_sound`, `blowout_sound`, `hit_sound`, `entrance_sound`, `artefact_born_sound`,
  `awake_sound`, `accum_sound`, `body_tearing_sound`; the files named by the first four exist.) [D10, D20]
  `sound` names an existing file, `sounds\zone_mosquito_bald.ogg` [D20]; whether and how the program
  plays it for an anomaly is **unestablished** (the key name is too generic to search the program for).
  The further `zone_*` sections reached from these by reference (section 2) are also S15's data [D21].
- **Depends on:** S2, S6; `level-objects.md` (which objects are present); a specification of anomaly
  behaviour (when an anomaly is entered, hits or discharges; none approved).
- **Scripts or AI:** none for the player and the anomalies themselves; entrances by other characters or
  monsters need AI.

### S16. Artefact activation sounds

- **Specifies:** the sounds of an artefact's activation phases.
- **Files:** the artefact records name activation sections by `artefact_activation_seq`: `af_blood` →
  `af_activation_mincer`, `af_medusa` and `af_cristall_flower` → `af_activation_bold` (`ARTEFACT`),
  `af_electra_sparkler` and `af_electra_flash` → `af_activation_electra` (`SCRPTART`) [D28]. The level's
  anomalies name, by key `artefacts`, the artefacts they produce: `af_cristall_flower`, `af_medusa`,
  `af_night_star` (both `zone_mosquito_bald_weak` sections), `af_electra_flash`, `af_electra_moonlight`,
  `af_electra_sparkler` (`zone_witches_galantine`), `af_blood`, `af_mincer_meat`, `af_soul`
  (`zone_mincer_weak_noart`), which use the same three activation sections [D31]. The fourth reached
  section, `af_activation_gravi`, is used by `af_vyvert`, `af_gravi` and `af_gold_fish`, artefacts of
  `zone_gravi_zone`, which is reached only by reference (e.g. `detector_simple` `zone_class_4`, and
  characters' sections); no `l01_escape` record is of `zone_gravi_zone`'s class `ZS_GALAN`
  [D31]. Each of the four sections has lines naming existing sound files (keys among `starting`,
  `flying`, `idle_before_spawning`, `spawning`) [D21, D28]. Whether and when an artefact is activated in normal
  play is **unestablished**.
- **Depends on:** S2; `level-objects.md`; a specification of artefact activation (none approved).
- **Scripts or AI:** **unestablished**.

### S17. The player's body sounds

- **Specifies:** when the sounds named by the keys below play (by their names: the player's pain sounds
  by hit type, heavy breathing and heartbeat).
- **Files:** `config\creatures\actor.ltx` `[actor]`: `hit_sounds = actor_hit_snds`,
  `heavy_breath_snd = actor\breath_1`, `heavy_blood_snd = affects\heartbeat`; section `[actor_hit_snds]`
  with 11 hit types naming 13 distinct files; all exist [D7]. The program files also name
  `affects\tinnitus3a`, the `explosion` entry [D7, D30]; S17 owns it.
- **Depends on:** S2; `docs/behavior/player-state.md` (conditions); a specification of damage to the
  player (none approved), which S15, S18 and S20 hits feed.
- **Scripts or AI:** none for the sounds; hurt by other characters or monsters needs AI.

### S18. Explosions and grenades

- **Specifies:** the sounds of explosive objects exploding and of grenades (pin pulled, explosion).
- **Files:** `[explosive_barrel]` (4 records) and `[explosive_mobiltank]` (2 records), class `II_EXPLO`,
  `snd_explode = weapons\f1_explode` [D28]; grenade sections `[grenade_f1]` (`w_f1.ltx`) and
  `[grenade_rgd5]` (`w_rgd5.ltx`) with `snd_explode` and `snd_checkout` and no `snd_shoot` [D28];
  `[grenade_f1]` is reached in the data in play only through the trader's section [D21].
- **Depends on:** S2, S6; specifications of explosions and of throwing grenades (none approved).
- **Scripts or AI:** none for the player's own; explosions caused by characters need AI.

### S19. Weapons in hand

- **Specifies:** the sounds of a weapon the player holds: drawing, holstering, firing, empty, reloading,
  switching, zooming.
- **Files:** the `snd_*` keys of the weapon sections in play: `[wpn_pm]`, `[wpn_walther]`, `[wpn_bm16]`,
  `[wpn_ak74u]` (for the level's `WP_PM`, `WP_WALTH`, `WP_BM16`, `WP_LR300` records) and `[wpn_binoc]`
  (the binoculars of the starting inventory, including `found_snd`) [D21]; other weapon sections of
  `config\weapons\*.ltx` (35 files set `snd_shoot`, e.g. `w_pm.ltx`) [D12] for weapons acquired during
  play.
- **Depends on:** S2; `player-state.md` (inventory); a specification of weapons in hand (none approved).
- **Scripts or AI:** none for the player's weapons.

### S20. Bullet impacts

- **Specifies:** the sounds of bullets hitting surfaces and bodies, and of bullets passing by.
- **Files:** the 57 pairs with `objects\bullet` (their sub-chunk 0x1005, format per S9) [D17];
  `config\weapons\weapons.ltx` `[bullet_manager]` `whine_sounds = weapons\whine_1, weapons\whine_2,
  weapons\whine_3, weapons\whine_4, weapons\whine01, weapons\whine02` (all six exist; `whine_sounds`
  occurs in the program files and in no script) [D14, D29]. Which shots make a whine heard by the
  player is **unestablished**.
- **Depends on:** S2, S6, S9; `level-cform.md` (the materials hit); S19 (firing); a specification of
  bullet flight (none approved).
- **Scripts or AI:** none for the player's own shots; shots by other characters need AI.

### S21. Detector

- **Specifies:** the detector's sounds near anomalies.
- **Files:** `config\misc\items.ltx` `[detector_simple]` (in the starting inventory; class `D_SIMDET`):
  15 `zone_sound_…` keys naming existing files; `noise` and `buzzer` both `zone_mosquito_bald`, the file
  of S15's `sound` key [D11, D20].
- **Depends on:** S2, S15; `player-state.md` (inventory).
- **Scripts or AI:** none.

### S22. Torch night vision

- **Specifies:** the night-vision sounds of the torch.
- **Files:** `[device_torch]` (in the starting inventory): `snd_night_vision_on = weapons\nv_start`,
  `snd_night_vision_off = weapons\nv_off`, `snd_night_vision_idle = weapons\nv_loop`,
  `snd_night_vision_broken = weapons\nv_off` [D28].
- **Depends on:** S2; `player-state.md`; a specification of the torch (none approved).
- **Scripts or AI:** none.

### S23. HUD and PDA notifications

- **Specifies:** the sound of a new-contact notification.
- **Files:** `config\system.ltx` `[maingame_ui]`: `snd_new_contact = detectors\contact_1`,
  `snd_new_contact1 = detectors\contact_8` [D11, D28]; `config\ui\maingame.xml` and `maingame_16.xml`:
  `<new_contact_snd>detectors\contact_1</new_contact_snd>` [D22]. `snd_new_contact1` and
  `new_contact_snd` do not occur as such in the program files or scripts [D14]; whether they are used is
  **unestablished**.
- **Depends on:** S2; a specification of the HUD (none approved).
- **Scripts or AI:** messages sent by scripts need scripts.

### S24. Characters', monsters' and scripted sounds (not implementable yet)

- **Covers:** the characters' and monsters' sections in play (`[stalker]`, `[m_crow]`, `[boar_normal]`,
  `[boar_weak]`, `[dog_normal]`, `[dog_strong]`, `[dog_weak]`, `[flesh_weak]`, `[m_flesh_e]`,
  `[pseudodog_weak]`, `[helicopter]`, `[helicopter_missile]`, and `[kruglov_flash]`, reached only through
  a character's section) [D21]; speech and dialogs (files under `sounds\characters_voice\`, 14,140
  present) [D13]; the tutorial's PDA voice (`config\ui\game_tutorials.xml`: 20 `<sound>` entries, all
  under `characters_voice\`) [D13]; the records' own sound names (section 2) [D25]; every sound played
  by scripts [D13].
- **Depends on:** the scripts and the AI; to be scoped again once those exist.

## 4. Suggested order

1. S1, S2 (everything plays through them). 2. S3. 3. S4. 4. S5. 5. S6. 6. S7. 7. S8. 8. S9.
9. S10, then S11. 10. S12. 11. S15 (idle sounds first). 12. S21. 13. S22. 14. S17. 15. S13. 16. S14.
17. S19. 18. S20. 19. S18. 20. S16. 21. S23. 22. S24 last, after scripts and AI.

## 5. Unestablished

- Whether scripts also control the level music (S5); the meaning of its five numbers.
- What reverberates on `l01_escape` (S7).
- Which file the rain sound uses (S8).
- Whether linked pairs inherit strings (S9).
- Whether climbing makes a sound (S12).
- Which contacts of the player's body sound (S13); whether breakable objects sound when broken (S14).
- Whether and how the program plays an anomaly's `sound` key (S15).
- Whether artefacts are activated in normal play (S16).
- Whether `snd_new_contact1` and `new_contact_snd` are used (S23).

## Provenance

- Date: 2026-11-24. Game: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition, archives
  `gamedata.db0` … `gamedata.dbd`, and the shipped program files of `bin\`.
- Analysis: reading the game data (archive listing, configuration and UI files, `gamemtl.xr`, the
  level directory, `spawns\all.spawn`, `game.graph`, model and motion files) and searching the shipped
  program files and scripts for key strings. No code was read for this scoping specification.

## Gaps found in review

- The program files also name two sound-name prefixes not listed in section 2: `monsters\crow\idle`
  (files `idle_0` … `idle_2`) and `characters_voice\dialogs\`; both are S24's.
- LTX files outside the `config\system.ltx` include tree are not examined: the logic files that records'
  custom data names by `cfg = scripts\….ltx` (on `l01_escape`: the doors' `door_*.ltx` with `snd_open_start`,
  `snd_close_start`, `snd_close_stop` keys, and `esc_trader_door.ltx`, which also names
  `device\door_servomotor`), `config\misc\script_sound.ltx`, and model and vehicle configurations under
  `config\models\`. Door sounds are not named by any specification of section 3 (believed S24's, as the
  logic is read by scripts).
- Scripts play sounds through more script identifiers than `sound_object` (e.g. `add_sound`,
  `play_sound`), so the 22 scripts of section 2 are not all the scripts that play sounds.
