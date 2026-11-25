# In-level HUD: scope and split

Scoping specification for the heads-up display of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition):
everything the game draws over the level while the player plays, with no screen open. It lists every
part, the data each part uses, and proposes how to split the area into small specifications, each
covering one behaviour or one format, that can be investigated and reviewed on their own.

It builds on the approved specifications `ui-layout` (`docs/formats/ui-layout.md`), `text-and-fonts`
(`docs/formats/text-and-fonts.md`), `title-menu`, `player-state`, `player-movement` and
`inventory-screen` (`docs/behavior/`), and names as dependencies the approved `startup`, `options-screens`,
`save-load-screens`, `loading-screen`, `level-loading`, `level-objects`, `script-loading`,
`script-environment` (`docs/behavior/`), `level-cform`, `sav`, `ogf` and `db-archive` (`docs/formats/`),
all cited by name.

Conventions:

- **Named in `xrGame.dll`** (or `XR_3DA.exe`) means the exact text occurs in the shipped binary as a
  printable string of 4 or more characters. It shows that the program refers to that file, element or
  section; what it does with it is for the proposed spec to establish.
- The functional descriptions of section 3 are readings of what the data names (element names,
  texture ids, string texts, key names) and are **believed** until the proposed spec establishes the
  behaviour; they fix what each spec must cover, not how the part behaves. Elsewhere **believed** marks
  such a reading; **unestablished** marks what this scoping work could not settle and leaves to the
  proposed spec.
- Every fact carries its check, a script in `checks/hud-scope/` of the dirty workspace: `c1` =
  `c1_layout.py`, `c2` = `c2_names.py`, `c3` = `c3_config.py`, `c4` = `c4_assets.py`, `c5` =
  `c5_scripts.py`, `c6` = `c6_more.py`, `c7` = `c7_coverage.py`. File paths are archive paths (winning copies); layout files are in `config\ui\`.

## 1. Data of the HUD

### 1.1 Layout files

| File | `_16` variant | Root children | Named in `xrGame.dll` |
|---|---|---|---|
| `maingame.xml` | yes | 24 (1.2) | yes |
| `motion_icon.xml` | yes | 10: `background`, `state_normal`, `state_crouch`, `state_creep`, `state_climb`, `state_run`, `state_sprint`, `power_progress`, `luminosity_progress`, `noise_progress` | yes |
| `zone_map.xml` | yes | 1: `minimap` (children `level_frame`, `background` with `dist_text`, `compass`, `center`) | yes, and the paths `minimap:level_frame`, `minimap:background`, `minimap:background:dist_text`, `minimap:compass`, `minimap:center` |
| `car_panel.xml` | yes | 2: `car_health_static`, `car_health_progress_bar` | yes |
| `messages_window.xml` | no | 7: `reinforcement`, `chat_prefix`, `chat_edit_box`, `chat_log_list`, `mp_log_list`, `sp_log_list`, `sp_log_list2` | yes; `sp_log_list` too, `sp_log_list2` not as a whole string |
| `maingame_pda_msg.xml` | no | 2: `icon_static`, `text_static` | yes |
| `ui_custom_msgs.xml` | no | 21 (1.3) | yes |
| `grenade.xml` | no | 1: `progress`, a radial progress (`sector_count` 30) | yes |
| `game_tutorials.xml` | no | 21 sequences (1.4); includes `ui\ui_credits.xml` and `ui\ui_movies.xml` | yes |
| `map_spots.xml` | no | map spot types; includes 10 files `map_spots_escape`, `_garbage`, `_agroprom`, `_darkvalley`, `_bar`, `_military`, `_radar`, `_mp`, `_relations`, `_lchangers` (`.xml`) | yes |

The `_16` variants have the same root children as their normal files (c1). `ui_hud.xml`, the texture
descriptions of the HUD pictures, is not named in `xrGame.dll` (c1); it is one of the texture
description files of `[texture_desc]` (`ui-layout` 6.1; c3).

Layout files whose names suggest the HUD but that no binary in `bin\`, no script and no other layout
file names, so believed never loaded and not covered: `ingame_msglog_sp.xml`, `ingame_msglog_mp.xml`,
`maingame_character.xml`, `maingame_new.xml`, `ui_videoplayer.xml` (c6).

### 1.2 `maingame.xml`

Root children in file order (c1): `speech_menu`, `static_health`, `progress_bar_health`, `static_armor`,
`progress_bar_armor`, `static_weapon`, `static_ammo`, `static_wpn_icon`, `car_panel`, `static_pda_online`,
`quick_info`, `icons_scroll_view`, `weapon_jammed_static`, `radiation_static`, `wound_static`,
`starvation_static`, `psy_health_static`, `invincible_static`, `can_sleep_static`, `artefact_static`,
`flashing_icons` (one `flashing_icon`), `pick_up_item`, `artefact_panel`, `new_contact_snd`.

All are named in `xrGame.dll` except `can_sleep_static` and `new_contact_snd` (c2);
`flashing_icon` is named too (c2). `can_sleep_static` is named by no script and no level logic either
(`.ltx` files, `config\misc\*.script`, the custom data in `spawns\all.spawn`; c2): believed unused. `new_contact_snd` (value `detectors\contact_1`) is named by no script (c2).

### 1.3 `ui_custom_msgs.xml`

21 statics (c1). By who names them (c2):

- named in `xrGame.dll`: `controller_fx`, `controller_fx2`, `monster_claws`, `main_task`, `game_saved`,
  `autosave`, `item_used`, `gun_jammed`, `cant_walk`, `cant_walk_weight`, `not_enough_money_mine`,
  `not_enough_money_other`; `max_ping_warn_1` … `_4` through the pattern `max_ping_warn_%d`;
- added by scripts by literal name: `cs_psy_danger`, `cs_radiation_danger`, `hud_timer_text`,
  `main_task`; `sr_timer.script` also adds a static whose name it reads from level logic
  (`timer_id`, default `hud_timer`);
- named by neither the binary, nor a script, nor level logic (as in 1.2): `sleep_001` (believed unused).

### 1.4 `game_tutorials.xml`

Sequences, in file order (c1): `part_1_pda`, `part_2_binocular`, `test`, `part_3_jump`, `part_4_crouch`,
`part_5_crouch_down`, `part_6_radiation`, `part_7_psy`, `part_8_wound`, `part_9_weapon`, `part_10_sprint`,
`part_11_weakness`, `part_12_relations`, `part_14_overload`, `part_15_hide_weapon`, `part_pda_diary`,
`pripyat_tutorial`, `agroprom_tutorial`, `bar_tutorial`, `yantar_tutorial`, `game_over`; the included
files add more (e.g. `credits_seq`, `mov_desire_1`, and in `ui_movies.xml` the logo sequence
`intro_logo` that the approved `startup` §6 specifies; c6). Who starts them (c2, c6):

- `xrGame.dll` names `game_over` (and the common word `test`) only;
- scripts start 18 by literal name through `game.start_tutorial`, among them `part_1_pda`, `part_9_weapon`,
  `credits_seq`, `mov_desire_1` … `_5`, `x18_dream`;
- `xr_effects.script` starts one named by level logic (`run_tutorial`, and `run_tutorial_if_newbie` in
  `gulag_escape.ltx`); level-logic files name `part_2_binocular`, `part_3_jump`, `part_4_crouch`,
  `part_10_sprint`, `part_12_relations`, `part_pda_diary`;
- a dialog starts one: `dialogs_escape.xml` action `escape_dialog.run_tutorial_pda`;
- `xr_conditions.script` tests `has_active_tutorial`.

Sequence items call scripts when they start or stop (`function_on_start`, `function_on_stop`, both named
in `xrGame.dll`; c6): `mob_trader.on_start_tutorial_item_1` (on start, ×2),
`mob_trader.on_stop_tutorial_item` (×2), and on stop `xr_effects.game_credits` (×7), `after_credits`,
`end_radar_dream`, `end_x18_dream`, `end_yantar_dream`, `presentation_task` (×4), `on_tutor_gameover_stop`,
`esc_init_dialod`. 37 items carry a `pda_section` (36 of them in `game_tutorials.xml`; named in
`xrGame.dll`; c6).

### 1.5 Configuration

All in `config\system.ltx` unless stated; every section below is named in `xrGame.dll` (c3).

| Section | Keys |
|---|---|
| `[hud_cursor]` | `cross_length`, `min_radius`, `max_radius`, `radius_lerp_speed`, `cross_color`, `disp_scale` |
| `[hud_hitmark]` | `hit_mark_texture = ui\ui_hud_hit_mark` |
| `[maingame_ui]` | `pda_msgs_max_show_time`, `info_msgs_max_show_time`, `snd_new_contact`, `snd_new_contact1` |
| `[main_ingame_indicators_thresholds]` | `radiation`, `wounds`, `jammed`, `starvation`, `fatigue` |
| `[tutorial_conditions_thresholds]` | `max_power`, `power`, `bleeding`, `satiety`, `radiation`, `weapon_jammed`, `psy_health` |
| `[level_maps_single]` (`config\game_maps_single.ltx`) | 17 levels, first `l01_escape`; each has its own section, all but `l12u_control_monolith` and `l12u_sarcofag` with a `texture` (`l01_escape`: `map\map_escape`) |

Also (c3):

- the `.ltx` files of `config\`: 17 `scope_texture` lines with 5 distinct values (`wpn\wpn_crosshair`, `_bino`, `_g36`,
  `_l85`, `_rpg`); `scope_texture` and `zoom_hide_crosshair` are named in `xrGame.dll`; 49 lines
  `hud = <section>` name sections believed to describe first-person models;
- `[device_torch]` (`config\misc\items.ltx`): `night_vision = on`, `night_vision_texture = wpn\wpn_night`,
  `night_vision_effector = postprocess_night`;
- `config\misc\postprocess.ltx`: 39 sections.

Settings (c3): the default `user.ltx` has `cl_dynamiccrosshair on`, `hud_crosshair on`,
`hud_crosshair_dist off`, `hud_info on`, `hud_weapon on`, `rs_stats off`. `hud_crosshair`,
`hud_crosshair_dist`, `hud_info`, `hud_weapon` and `cl_dynamiccrosshair` are named in `xrGame.dll`; `XR_3DA.exe` names `hud_draw 1`, `hud_weapon 1` and
`rs_stats`. The options screen edits five of them (`ui_mm_opt.xml`: `check_crosshair`,
`check_dyn_crosshair`, `check_dist`, `check_tips`, `check_show_weapon` → `hud_crosshair`,
`cl_dynamiccrosshair`, `hud_crosshair_dist`, `hud_info`, `hud_weapon`). The default `user.ltx` also has
`cl_cod_pickup_mode 1` and `g_autopickup on`, both named in `xrGame.dll`; from their names they choose
which item is picked up, and so plausibly what the pick-up icon shows (spec 13, to establish). Every
other setting has an owner in section 5 (c7).

Tasks (`config\gameplay\`, c7): 12 task files (`game_tasks.xml`, `tasks_aes.xml`, `tasks_agroprom.xml`,
`tasks_bar.xml`, `tasks_darkvalley.xml`, `tasks_deadcity.xml`, `tasks_escape.xml`, `tasks_garbage.xml`,
`tasks_military.xml`, `tasks_radar.xml`, `tasks_sarcofag.xml`, `tasks_yantar.xml`) give objectives a
`map_location_type` (162 in all: `green_location` 128, `blue_location` 33, `crlc_mdl` 1, each a spot type
of `map_spots*.xml`) and 26 of them, in 7 files, a `map_location_hidden`; both names are in `xrGame.dll`.

Bindings (`config\default_controls.ltx`, c3): among others `scores` Tab, `inventory` I, `active_jobs` P,
`map` M, `contacts` H, `use_medkit` `[`, `use_bandage` `]`, `torch` L, `night_vision` N, `speech_menu_0` C,
`speech_menu_1` Z, `wpn_zoom` right mouse button, `console` `` ` ``, `pause` Pause. The actions `scores`,
`use_medkit`, `use_bandage`, `torch`, `night_vision` are named in `xrGame.dll` (c6). Every bound action has
one owner in section 5 (c7).

Binoculars (`config\weapons\w_binoc.ltx`, c7): `vision_present`, `vis_frame_speed`, `vis_frame_color`,
`found_snd = detectors\DA-2_beep1`, all four named in `xrGame.dll`.

### 1.6 Pictures, effects and animations

- Textures present (c4): `ui\ui_hud`, `ui\cursor`, `ui\ui_disk_io`, `ui\ui_hud_map_arrow`, `ui\ui_nomap2`,
  `ui\ui_icon_equipment`, `wpn\wpn_night`, `map\map_escape`, the video `car\trabantnoise` (`.ogm`), and 20
  files `textures\ui\ui_hud_hit_mark*.dds`.
- Named in `xrGame.dll` (c4): `ui\cursor`, `ui\ui_disk_io`, `ui\ui_nomap2`, `ui\ui_icon_equipment`, and the
  shaders `hud\crosshair`, `hud\cursor`, `hud\movie`; also `ui\ui_enemy_frame` (`textures\ui\ui_enemy_frame.dds`
  exists; id `ui_enemy_frame` in `ui_old_textures.xml`; c7). Section 5 lists every texture or shader path
  that `xrGame.dll` names, with its owner.
- 32 `.ppe` files, all in `anims\` (c4).
- `lanims.xr` (named in `XR_3DA.exe`) holds every colour and transform animation that the files of 1.1,
  their `_16` variants and their includes name with `light_anim` or `xform_anim`: `asus_logo_01`,
  `credits_head`, `map_spot_new_xform`, `map_spot_rel_xform`, `monster_claws`, `tutor_rad_sign_clr`,
  `tutor_rad_sign_xform`, `ui_btn_hint`, `ui_mp_minimap_af_enemy`, `ui_mp_minimap_af_friend`,
  `ui_mp_minimap_af_neutral`, `ui_pda_contacts`, `ui_slow_blinking_alpha` (c4).
  The file is shared beyond the HUD: the approved `startup` §6.4 establishes the entry `asus_logo_01`
  (frame rate, keys, ARGB values, whole-frame advance, linear interpolation, `la_alpha`/`la_texture`),
  `options-screens` uses `ui_map_area_anim`, and `level-objects` names its entries as light colour
  animators. `ui-layout` 11 leaves these animations in general unestablished.
- Shaders: `hud\hitmarker` is a `shaders.xr` entry (`ui-layout` 8) that neither `xrGame.dll` nor
  `XR_3DA.exe` names (c4); `hud\movie` draws video pictures (`ui-layout` 8).

### 1.7 Scripts that change what the HUD shows

Callers among the shipped scripts (`lua_help.script`, a listing, excluded; c5):

| Script functions | Called by |
|---|---|
| `level.show_indicators` | `avi_start`, `bind_stalker`, `xr_effects` |
| `level.hide_indicators` | `avi_start`, `xr_effects` |
| `AddCustomStatic`, `GetCustomStatic`, `RemoveCustomStatic` (of the HUD object) | `bind_stalker`, `news_manager`, `sr_psy_antenna`, `sr_timer` |
| `give_game_news` (16 calls) | `_test`, `mob_remark`, `news_manager`, `test_heli_2` |
| `level.map_add_object_spot`, `_ser` (40 calls) | `bar_dialogs`, `gulag_general`, `level_tasks`, `se_monster`, `se_stalker`, `sim_statistic`, `smart_terrain`, `sr_mapspot`, `test_heli_2`, `treasure_manager` |
| `level.map_remove_object_spot` | the same less `bar_dialogs`, `sr_mapspot`, `test_heli_2` |
| `level.map_change_spot_hint` | `smart_terrain` |
| `game.start_tutorial` | `_g`, `_test`, `dialogs_aes`, `escape_dialog`, `test`, `ui_main_menu`, `xr_effects`, `xr_meet` |
| `level.add_pp_effector` (20 calls), `remove_pp_effector`, `set_pp_effector_factor` | `_test_aura`, `_test_aura_2`, `level_weathers`, `sr_cutscene`, `sr_psy_antenna`, `sr_teleport`, `test`, `x1`, `x11`, `xr_detector`, `xr_effects`, `xr_patrol` |
| `hide_weapon`, `restore_weapon` | `bind_stalker`, `sr_no_weapon`, `xr_effects`, `zone_no_weapon` |
| `AddDialogToRender`, `RemoveDialogToRender` | `game_last_cl`, `game_last_ui` |
| `AddCustomMessage` | `game_last_ui` |
| `level.add_dialog_to_render`, `level.remove_dialog_to_render` | `avi_start` |
| `give_talk_message` (believed news shown in the talk screen, `B-screen`) | `_test`, `news_manager`, `task_manager` |
| objective `set_map_hint`, `set_map_location` | `task_manager` |
| `level.map_has_object_spot` | `level_tasks`, `sr_mapspot` |

More callers (c6):

| Script functions | Called by |
|---|---|
| `set_tip_text` (hint text of usable objects; named in `xrGame.dll`) | `_test`, `bind_monster`, `explode_zone`, `mob_trader`, `ph_button`, `ph_car`, `ph_code`, `ph_door`, `ph_gate`, `ph_idle`, `xr_motivator` |
| `set_tip_text_default` (named in `xrGame.dll`) | `_test`, `bind_monster`, `explode_zone`, `xr_motivator` |
| `has_active_tutorial` | `xr_conditions` |
| classes derived from the script class `effector` (screen effects defined by scripts) | `_g`, `level_psy_antenna`, `postprocess`, `sr_aes_deadzone`, `sr_postprocess`, `xr_radar` |
| `effector_params` (filling such effects) | `level_psy_antenna`, `postprocess`, `sr_aes_deadzone`, `sr_postprocess` |
| `level.add_complex_effector` | `_test` only |
| `level.add_cam_effector`, `add_cam_effector2`, `remove_cam_effector` | `sr_cutscene`, `xr_effects` |

All functions of both tables (the script class `effector` included), and the task names of 1.5, are
named in `xrGame.dll` (c5 for all of them; c6 also for `set_tip_text` and `set_tip_text_default`). No
other script, no configuration file and no spawn custom data names `avi_start` (c5): it is believed
never run, so its calls (`show_indicators`, `hide_indicators`, its own window through
`add_dialog_to_render`) need no spec. `game_last_cl`, `game_last_ui` and their `AddCustomMessage` are
believed multiplayer (2).

## 2. Boundary

**Screens that open over the level** (`B-screen`) are not HUD; each is its own area: the inventory (approved,
`inventory-screen`), the PDA (`pda.xml`), talk (`talk.xml`), trade (`trade.xml`), body search
(`carbody_new.xml`), all named in `xrGame.dll` (c4), the code panel that `ph_code.script` opens
(`ui_numpad.script`, built from `ui_numpad_wnd.xml`; c6), the title menu (approved, `title-menu`) and the
console (binding `console`, c3). This list holds the screens found; spec 2 establishes any other screen
that opens over the level. `ui_game_over.script` (`ui_game_over.xml`) is named by no other script (c6):
believed unused. The HUD specs cover only how opening a screen changes the HUD (spec 2).

**Camera** (`B-camera` in section 5): camera effects (`level.add_cam_effector`, `_2`,
`remove_cam_effector`, 1.7) and the actions `cam_zoom_in`, `cam_zoom_out` are believed to move the
camera rather than draw over the level; they belong with camera and cut-scene behaviour.
**The torch light** (`B-torch`): the action `torch` is believed to switch the torch's light in the
level; it belongs with level lights and the torch device (night vision is spec 21).
**Gameplay actions** (`B-gameplay`): weapon selection (including `next_slot`, `prev_slot`), fire, reload, fire modes, the weapon function
key, `use` and `drop` act on the world, not on the HUD; what they change on the HUD reaches it through
the part specs (e.g. the weapon panel, 5).

**Console and developer tools** (`B-console`): the console (`console`), screenshots (`screenshot`) and
the engine statistics (`rs_stats`, off by default, c3), believed a developer overlay: not covered.

**Settings outside the HUD** (`B-settings`): settings of the default `user.ltx` that the binaries
name and that neither a HUD spec nor the approved `options-screens` takes (camera inertia, always-run,
corpse count, physics rate, sound and similar; listed in section 5).

**Unused** (`B-unused`): `sleep_001` (1.3).

**Coincident words** (`B-coincident`): string ids `actor`, `trader`, `artefact`, `psy_dog`, `wpn_hpsa`,
`global_map` that `xrGame.dll` names, believed because the same words are also section, object or
class names there; their texts belong to the screens that show them, not to the HUD.

**Multiplayer parts** (`B-mp`; believed, from their names): `messages_window.xml` `reinforcement`, `chat_prefix`,
`chat_edit_box`, `chat_log_list`, `mp_log_list`; the statics `max_ping_warn_1` … `_4`; the minimap
animations `ui_mp_minimap_*`; `map_spots_mp.xml`; and the `speech_menu` of `maingame.xml`, whose strings
(`mp_st_speechmenu.xml`, all `speech_*`, e.g. `speech_covermeartifact`) are team commands, with the
actions `speech_menu_0` and `speech_menu_1` (c2). The scripts `game_last_cl` and `game_last_ui` (1.7) are
believed multiplayer too. Spec 2 must establish that a single-player level never shows them; anything it
finds shown in single-player joins the spec of the matching part. Section 5 lists every name of this
boundary and of the others.

## 3. Proposed specs

Each entry: functional description; layout files and data; dependencies (approved specs, and systems
not yet specified); script functions involved; order. The order puts first the format every animated
part needs, then the frame the other parts sit in, then the parts that depend most on approved specs,
then the parts that wait on systems not yet specified. Section 5 lists every name each spec owns.

Wherever scripts drive a part, the spec builds on the approved `script-loading` (which script files run,
and when) and `script-environment` (the Lua runtime); the game functions the scripts call (1.7) are not
specified yet and are named as "the script API".

1. **Colour and transform animations** (`colour-animations`, format). The animation library that
   UI elements name with `light_anim` and `xform_anim`, and how an animation changes a picture's or a
   text's colour, alpha and transform over time.
   The whole file, since level lights and the menus use it too (1.6), consistent with `startup` §6.4.
   Data: `lanims.xr`; the `light_anim`, `la_*`, `xform_anim*` attributes (1.6). Depends on:
   `ui-layout`, `startup` §6.4. Scripts: none. Order 1.

2. **HUD composition and visibility** (`hud-visibility`). Which HUD parts exist in a level, the
   order they are drawn in, the `_16` file choice, and when each part is shown or hidden: the
   indicator and crosshair switches, screens opening over the level, zoom, death, pause (the text
   `st_game_paused`, action `pause`; how the inventory screen treats pause is `inventory-screen` 1.4),
   the settings of 1.5 (`hud_draw`, `hud_crosshair`, `hud_weapon`, `hud_info`) and their options check
   boxes. It also settles the action `scores` (Tab): the layout files it might open, `stats.xml`
   (believed multiplayer: it has a window `frag_wnd_tdm`, c7), `actor_statistic.xml` and
   `statisticwnd.xml` (believed PDA), are screens, so whether Tab shows anything over a single-player
   level is a visibility question; and it establishes whether the names of section 5 it owns for that
   purpose (`hud\fog_of_war`, `ui\ui_icons_task`, `ui\ui_pda_over_list`, `ui\ui_pop_up_active_back`,
   `st_detail_list_for_*`, the animation `ui_slow_blinking`, and the string ids `Clear`, `Name`,
   `ui_inv_bleeding`, `ui_inv_health`, `ui_inv_power`, `ui_inv_radiation`, `ui_inv_satiety`, `ui_st_days`,
   `ui_st_hours`, `ui_st_mins`, `ui_st_months`, `ui_st_secs`, `ui_st_opponent_items`,
   `ui_st_preset_stored_to`) are ever drawn over the level, handing each
   to the spec of the part it finds or to the boundary.
   Data: the root windows of the files of 1.1; default `user.ltx`; `ui_mm_opt.xml`. Depends on:
   `ui-layout`, `options-screens`, `player-movement` (bindings), `inventory-screen` (opening, closing,
   pause), `script-loading`, `script-environment`; the other screens (2) and the script API (not
   specified). Scripts: `level.show_indicators`, `level.hide_indicators` (1.7). Ownership: spec 2 owns
   every switch (`hud_draw`, `hud_crosshair`, `hud_info`, `hud_weapon`, the indicator and crosshair
   switches, what zooming hides, `zoom_hide_crosshair`) and whether a part is shown; each part spec owns
   what that part draws while shown (crosshair 6, target information 14, zoom picture 15, first-person
   model 26). Order 2.

3. **Health and armour panel** (`hud-condition-panel`). The health bar and the outfit bar with their
   frames and icons.
   Data: `maingame.xml` / `_16` `static_health`, `progress_bar_health`, `static_armor`,
   `progress_bar_armor`; `ui_hud.xml`. Depends on: `ui-layout`, `player-state` (health, outfit slot,
   item condition), `inventory-screen` 4.7 (bar drawing). Scripts: none. Order 3.

4. **Motion icon** (`hud-motion-icon`). The posture picture, the power bar, and the noise and
   luminosity bars.
   Data: `motion_icon.xml` / `_16`. Depends on: `player-movement` (state), `player-state` (power),
   `inventory-screen` 4.7; the player's noise and light level (not specified; the spec establishes
   what drives these two bars). Scripts: none. Order 4.

5. **Weapon panel** (`hud-weapon-panel`). The active item's name, its ammunition line and its icon.
   Data: `maingame.xml` / `_16` `static_weapon`, `static_ammo`, `static_wpn_icon`; item sections
   (`inv_name_short`, `inv_grid_*`); `ui\ui_icon_equipment`. Depends on: `player-state` (active slot),
   `inventory-screen` 4.2 (icons), `text-and-fonts`; weapons: magazine, ammunition types, fire modes,
   grenade launcher (not specified). Scripts: none. Order 5.

6. **Crosshair** (`hud-crosshair`). The cross and the aim-point picture: geometry, radius from the
   weapon's spread, and which item shows which.
   Data: `[hud_cursor]`; `cl_dynamiccrosshair` (the switch `hud_crosshair` is spec 2's). To establish:
   whether the crosshair is drawn with `ui\cursor` and the shaders `hud\crosshair` and `hud\cursor`
   (all three named in `xrGame.dll`, c4, but not yet linked to the crosshair; `title-menu` §7 has
   `hud\cursor` as the shader of the menu's mouse cursor, `ui\ui_ani_cursor`). Depends on:
   `player-movement`, `player-state`, `level-cform` (what the aim ray meets), `title-menu` §7; weapon
   dispersion (not specified). Scripts: none. Order 6.

7. **Warning icons** (`hud-warning-icons`). The column of condition icons (weapon wear, radiation,
   bleeding, hunger, psy health, invulnerability, artefact) with colour by threshold.
   Data: `maingame.xml` / `_16` `icons_scroll_view` and the statics `weapon_jammed_static` …
   `artefact_static` (`can_sleep_static` included to establish whether it is ever shown);
   `[main_ingame_indicators_thresholds]`. Depends on: `ui-layout` 9.1, `player-state` (radiation,
   satiety, psy health); wounds, weapon condition, invulnerability, artefacts (not specified). Scripts:
   none. Order 7.

8. **On-screen messages** (`hud-messages`). The centre-screen message statics: how one is added,
   timed out and removed, and every message the program raises (item used, gun jammed, can't walk,
   not enough money, task title and `st_no_active_task`, controller and claw effects), with the
   quick-use actions `use_medkit` and `use_bandage` (believed to raise `item_used`, whose text is
   `st_item_used`). The messages `game_saved` and `autosave` are established by the approved
   `save-load-screens` §8.3 and §10 (element, text, font, when and how long) and are not re-specified;
   spec 8 states only how they share the add, replace and time-out rules with the others.
   Data: `ui_custom_msgs.xml` (1.3); their string-table texts, among them the ids `gun_jammed`,
   `cant_walk`, `cant_walk_weight`, `st_item_used`, `st_no_active_task`, and `ui_st_time_remains`
   (believed the timer's text, to establish). Depends on: `ui-layout`,
   `text-and-fonts`, `colour-animations`, `player-state` (movement limits, eating and medicines),
   `save-load-screens`, `script-loading`, `script-environment`; weapons, tasks, monster attacks,
   trading, the script API (not specified). Scripts: `AddCustomStatic`, `GetCustomStatic`,
   `RemoveCustomStatic` (`news_manager` task titles, `bind_stalker` radiation danger, `sr_psy_antenna`
   psy danger, `sr_timer` timer). Order 8.

9. **Game log and news** (`hud-game-log`). The message log and the PDA news lines that scroll on
   screen and fade, with their icons and show times.
   Data: `messages_window.xml` `sp_log_list`, `sp_log_list2`; `maingame_pda_msg.xml`; of `[maingame_ui]`
   only `pda_msgs_max_show_time` and `info_msgs_max_show_time` (the new-contact sounds are spec 12's);
   `news.xml` (named in `xrGame.dll`) and `news_templates.xml` as far as the log uses them (c4); the
   animation `ui_main_msgs_short` (to establish). Depends on: `text-and-fonts`, `ui-layout`,
   `colour-animations`, `script-loading`, `script-environment`; news, information portions and the
   script API (not specified). Scripts: `give_game_news` (`news_manager`, `mob_remark`); its counterpart
   `give_talk_message` goes to the talk screen (`B-screen`, 1.7). Order 9.

10. **Minimap and compass** (`hud-minimap`). The round level map that follows the player, its frame,
    centre point, compass arrow and distance text.
    Data: `zone_map.xml` / `_16`; `[level_maps_single]` and the level sections (`texture`,
    `bound_rect`); the `textures\map\` pictures; `ui\ui_nomap2` (to establish). Depends on:
    `ui-layout` 5.4 (rotatable pictures), `player-movement` (position, heading), `level-loading` (which
    level). Scripts: none. Order 10.

11. **Map spots on the minimap** (`hud-map-spots`). The marks for characters, tasks and places drawn
    on the minimap, the pointers at its edge, and their colours by relation.
    Data: `map_spots.xml` and its includes (`map_spots_mp.xml` believed multiplayer, 2); its pictures
    of `ui\ui_hud` and `ui\ui_common` (c4). Depends on: `hud-minimap`, `colour-animations`,
    `level-objects`, `script-loading`, `script-environment`; relations, tasks, the PDA map, the script
    API (not specified). It also owns how a task objective's location becomes a minimap spot: the
    objectives' `map_location_type` and `map_location_hidden` (1.5), and the task scripts that set them.
    Scripts: `level.map_add_object_spot`, `_ser`, `map_remove_object_spot`, `map_change_spot_hint`,
    `level.map_has_object_spot`; objective `set_map_hint`, `set_map_location` (`task_manager`) (1.7).
    Order 11.

12. **PDA indicators** (`hud-pda-indicators`). The contact counter and the flashing PDA icon, with
    the new-contact sounds.
    Data: `maingame.xml` `static_pda_online`, `flashing_icons`, `new_contact_snd`; `[maingame_ui]`
    `snd_new_contact`, `snd_new_contact1`; the animation `ui_pda_contacts`. Depends on:
    `colour-animations`, `level-objects`; PDA contacts and messages (not specified). Scripts: none
    found. Order 12.

13. **Pick-up icon and hint line** (`hud-pickup-hint`). The icon of the item under the crosshair and
    the hint text line near the bottom of the screen.
    Data: `maingame.xml` `pick_up_item`, `quick_info`; the settings `cl_cod_pickup_mode` and
    `g_autopickup` (to establish what they change here); the hint texts `character_use`,
    `dead_character_use`, `dead_character_use_or_drag`, `inventory_box_use`, `inventory_item_use`,
    `car_character_use` (string ids named in `xrGame.dll`, c7; `bind_monster` and `xr_motivator` pass
    `character_use` to `set_tip_text`, `mob_trader` uses it as its default `tip_text`, c7). Depends on: `colour-animations`,
    `level-objects`, `inventory-screen` 4.2 (icons), `level-cform`, `script-loading`,
    `script-environment`; usable objects and the script API (not specified). Scripts: `set_tip_text`,
    `set_tip_text_default` (1.7). Order 13.

14. **Target information** (`hud-target-info`). What the crosshair shows about the character or
    object it points at: colour, name, and the distance when `hud_crosshair_dist` is on.
    Data: `hud_crosshair_dist`; character descriptions and relations; the string ids `enemy`,
    `friend`, `neutral`, `novice`, `stalker`, `veteran`, `master` (named in `xrGame.dll`, c7; to
    establish whether the crosshair shows them) (the switch `hud_info` is spec
    2's). Depends on: `hud-crosshair`, `level-objects`, `text-and-fonts`; characters and relations (not
    specified). Scripts: none. Order 14.

15. **Zoom picture** (`hud-zoom`). The scope or binoculars picture drawn while aiming.
    Data: `scope_texture` (1.5); `textures\wpn\wpn_crosshair*`; the action `wpn_zoom` (what zooming
    hides is spec 2's). Depends on: `hud-visibility`, `player-state`; weapons: zoom and scope addons
    (not specified). Scripts: none. Order 15.

16. **Binocular target frames** (`hud-binocular-frames`). The frames drawn over creatures seen through
    the zoomed binoculars and the sound when one is found (believed, from the key names and the comment
    of `vis_frame_speed`, "the speed at which the frame homes onto the target").
    Data: `config\weapons\w_binoc.ltx` `vision_present`, `vis_frame_speed`, `vis_frame_color`,
    `found_snd` (`detectors\DA-2_beep1`), all four named in `xrGame.dll`; the picture
    `ui\ui_enemy_frame` (named in `xrGame.dll`; `textures\ui\ui_enemy_frame.dds`; id
    `ui_enemy_frame` in `ui_old_textures.xml`) (c7). Depends on: `hud-zoom`, `level-objects`,
    `player-state`; creatures and their visibility (not specified). Scripts: none. Order 16.

17. **Hit marks** (`hud-hit-marks`). The marks drawn when the player is hit.
    Data: `[hud_hitmark]`; `textures\ui\ui_hud_hit_mark*`; the animation `hud_hit_mark` (in
    `lanims.xr`, named in `xrGame.dll`; c7); the shader `hud\hitmarker` (`ui-layout` 8; whether hit
    marks use it is for the spec to establish). Depends on: `colour-animations`; hits and damage (not
    specified). Scripts: none. Order 17.

18. **Tutorials and scripted sequences** (`hud-tutorials`). The sequences of pictures, texts, sounds and
    videos shown over the level: tutorials, dreams, endings, credits, game over; how they start, run,
    take input, pause and stop.
    The approved `startup` §6 already specifies the sequence format as `intro_logo` uses it
    (`global_wnd`, `play_each_item`, video and image items, `delay`, `pause_state`, `sound`,
    `video_wnd`, `length_sec`, `start_time`, input), and `level-loading` the sequence `intro_game`;
    spec 18 covers only what they leave open: among it the item elements that `intro_logo` does not use
    (`guard_key`, `grab_input`, `cursor_pos`, `disabled_key`, `pda_section`, `function_on_start`,
    `function_on_stop`; c6), sequences started during play, and `game_over`.
    Data: `game_tutorials.xml` with `ui_credits.xml` and `ui_movies.xml`; `[tutorial_conditions_thresholds]`;
    `.ogm` videos; the shader `hud\movie`; the tutorial string ids `crouch`, `jump`, `sprint`,
    `hide_weapon`, `radiation`, `relation`, `weapon`, `wound` (in `string_table_tutorial.xml`, named in
    `xrGame.dll`, c7; whether the program looks them up as text is to establish). Depends on: `startup` §6, `level-loading`, `ui-layout`,
    `text-and-fonts`, `colour-animations`, `title-menu` 3.2 (videos), `player-state`, `script-loading`,
    `script-environment`; the PDA (`pda_section`), information portions, dialogs and the script API (not
    specified). Scripts: `game.start_tutorial`, `has_active_tutorial`, the item hooks
    `function_on_start` / `function_on_stop` (1.4), level logic through `xr_effects` (`run_tutorial`,
    `run_tutorial_if_newbie`) and the dialog action `escape_dialog.run_tutorial_pda` (1.4, 1.7).
    Order 18.

19. **Post-process effect files** (`ppe-format`, format). The layout of the `.ppe` files and what
    each of their values means.
    Data: the 32 `anims\*.ppe` files (1.6). Depends on: `db-archive` (reading them). Scripts: none.
    Order 19.

20. **Screen effects** (`screen-effects`, behaviour). Full-screen colour, noise and blur effects laid
    over the level picture by anomalies, hits, psy fields, cut-scenes and scripts: how one is started,
    combined with others, weighted and stopped, and how it changes the picture.
    Data: every effect definition of `config\` listed under kind "effect" in section 5 (c7):
    `config\misc\postprocess.ltx`; the effect sections of `config\misc\effectors.ltx`
    (`monster_attack_effector`, `small_monster_attack_effector`, `sleep_effector`, `explode_effector`);
    the effect keys and sections of 16 creature files `config\creatures\m_*.ltx` (`attack_effector`,
    `aura_effector`, `appear_effector`, `threaten_effector`, `scan_effector_section`, … and the
    sections they name, e.g. `m_zombie_attack_effector`, `psy_dog_aura_effector`); the `postprocess`
    keys of 16 anomaly files `config\misc\zone_*.ltx` and of `config\scripts\bun_psy_zone.ltx`; the
    `fx_set_*` curves of `config\scripts\postprocess1.ltx`, `teleport.ltx`, `earthshake.ltx`,
    `deadcity_wake.ltx`; and effects loaded from `.ppe` files. Depends on: `ppe-format`,
    `script-loading`, `script-environment`; rendering of the level, hits, anomalies, the script API (not
    specified). Scripts: `level.add_pp_effector`, `remove_pp_effector`, `set_pp_effector_factor`;
    effects that scripts define as classes derived from `effector`, filled through `effector_params`
    (`_g`, `level_psy_antenna`, `postprocess`, `sr_aes_deadzone`, `sr_postprocess`, `xr_radar`);
    `level.add_complex_effector` (called only by `_test`) (1.7). Camera effects are outside (2).
    Order 20.

21. **Night vision** (`hud-night-vision`). The torch's night-vision mode: its key, picture, effect and
    sounds.
    Data: `[device_torch]` `night_vision`, `night_vision_texture`, `night_vision_effector`,
    `snd_night_vision_on`, `_off`, `_idle`, `_broken`, `night_vision_recharge_time`, `_min`,
    `night_vision_discharge_time` (c7); `wpn\wpn_night`; `postprocess_night`; the action
    `night_vision`. Depends on: `ppe-format`, `screen-effects`, `player-state` (the torch in the
    starting inventory); the torch (not specified). Scripts: none found. Order 21.

22. **Artefact panel** (`hud-artefact-panel`). The row of icons of the artefacts on the belt.
    Data: `maingame.xml` `artefact_panel`. Depends on: `player-state` (belt), `inventory-screen` 4.2
    (icons). Scripts: none. Order 22.

23. **Grenade indicator** (`hud-grenade-indicator`). The radial indicator of `grenade.xml`: what
    drives it and whether single-player shows it (unestablished).
    Data: `grenade.xml`; ids `ui_hud_grenadetarget_e`, `_d` of `ui_hud.xml` (c4). Depends on: grenades
    (not specified). Scripts: none. Order 23.

24. **Disk-activity icon** (`hud-disk-icon`). The picture shown while the game reads or writes a save.
    Data: `ui\ui_disk_io`. Depends on: `sav`, `save-load-screens`. Scripts: none. Order 24.

25. **Vehicle panel** (`hud-vehicle-panel`). The vehicle condition bar shown while driving.
    Data: `maingame.xml` `car_panel`; `car_panel.xml` / `_16`. Depends on: `inventory-screen` 4.7;
    vehicles (not specified). Scripts: none. Order 25.

26. **First-person model** (`hud-first-person-model`). The player's hands and the active item drawn
    in front of the camera (the switch `hud_weapon` is spec 2's).
    Data: the `hud = <section>` sections (1.5) and the models they name. Depends on: `ogf`,
    `player-movement` (camera), `script-loading`, `script-environment`; skeletal animation, weapons and
    the script API (not specified). Scripts: `hide_weapon`, `restore_weapon` (1.7). Order 26.

## 4. Unestablished

- What drives each part (left to its spec), including whether `can_sleep_static`, `sleep_001` and
  `sp_log_list2` are ever shown, and whether the multiplayer parts (2) can appear in single-player.
- Who starts the sequences `part_5_crouch_down`, `part_7_psy`, `pripyat_tutorial`, `agroprom_tutorial`,
  `bar_tutorial`, `yantar_tutorial`: no binary string, script, level logic (as in 1.2) or dialog file
  (`config\gameplay\*.xml`) names them (c2).
- Whether the action `scores` does anything in single-player (spec 2 settles it).
- What `ui\ui_nomap2` is used for.

## 5. Coverage

Completeness pass (c7). The universe of HUD-related names, built from the data and the binary:

- **layout**: every root child of `maingame.xml`, `motion_icon.xml`, `car_panel.xml`,
  `messages_window.xml`, `maingame_pda_msg.xml`, `ui_custom_msgs.xml`, `grenade.xml` and
  `game_tutorials.xml` (the `_16` variants have the same, c1), the `minimap` element of `zone_map.xml`
  with its children, every sequence of `ui_movies.xml` and `ui_credits.xml`, and the `map_spots*.xml`
  files as wholes. The pictures, texts, sounds and attributes inside an element go with its owner;
- **string**: every `xrGame.dll` string that is a texture or shader path (`ui\…`, `hud\…`, `wpn\…`), a
  string id (`st_…`) or a layout file name (`….xml`);
- **string-id**: every id of `config\text\rus\*.xml` that `xrGame.dll` names (219);
- **animation**: every `lanims.xr` name that `xrGame.dll` names;
- **config**: the HUD sections and keys of 1.5 (all `[device_torch]` night-vision keys), the task
  objective names of 1.5, and the effect files of 1.6;
- **effect**: in every `.ltx` file of `config\`, each section with three or more post-process values
  (`duality_h`, `duality_v`, `blur`, `gray`, `noise_intensity`, `color_base`, `color_gray`,
  `color_add`), each file with `fx_set_*` curves, each key `postprocess` or `…effector…`, and every
  section of `config\misc\effectors.ltx`;
- **setting**: every setting of the default `user.ltx` that `xrGame.dll` or `XR_3DA.exe` names, and every
  option of `ui_mm_opt.xml` (123 in all); `sv_…`, `net_…`, `mm_…` and `bemsg` go to `B-mp`, a setting the
  approved `options-screens` mentions to it, the rest to `B-settings`, except those a HUD spec takes;
- **action**: every action bound in `config\default_controls.ltx` and every action of the key-binding
  screen (`ui_keybinding.xml` `exe`), which adds `crouch_toggle`, `next_slot` and `prev_slot`.

Each name has exactly one owner: a proposed spec (section 3), an approved spec (c7 checks that the
approved spec's text mentions the name), or a boundary entry of section 2 (`B-screen`, `B-mp`,
`B-camera`, `B-torch`, `B-gameplay`, `B-console`, `B-settings`, `B-unused`, `B-coincident`). The check fails on a name without an
owner, on two owners, and on any difference between this table and the one it prints.

| Owner | Kind | Names |
|---|---|---|
| 1 `colour-animations` | config | `lanims.xr` |
| 2 `hud-visibility` | string | `hud\fog_of_war`, `maingame.xml`, `st_detail_list_for_%s`, `st_detail_list_for_community_relations`, `st_game_paused`, `ui\ui_icons_task`, `ui\ui_pda_over_list`, `ui\ui_pop_up_active_back` |
| 2 `hud-visibility` | string-id | `Clear`, `Name`, `st_detail_list_for_community_relations`, `st_game_paused`, `ui_inv_bleeding`, `ui_inv_health`, `ui_inv_power`, `ui_inv_radiation`, `ui_inv_satiety`, `ui_st_days`, `ui_st_hours`, `ui_st_mins`, `ui_st_months`, `ui_st_opponent_items`, `ui_st_preset_stored_to`, `ui_st_secs` |
| 2 `hud-visibility` | animation | `ui_slow_blinking` |
| 2 `hud-visibility` | config | `hud_draw`, `zoom_hide_crosshair` |
| 2 `hud-visibility` | setting | `hud_crosshair`, `hud_info`, `hud_weapon` |
| 2 `hud-visibility` | action | `pause`, `scores` |
| 3 `hud-condition-panel` | layout | `maingame:progress_bar_armor`, `maingame:progress_bar_health`, `maingame:static_armor`, `maingame:static_health` |
| 4 `hud-motion-icon` | layout | `motion_icon:background`, `motion_icon:luminosity_progress`, `motion_icon:noise_progress`, `motion_icon:power_progress`, `motion_icon:state_climb`, `motion_icon:state_creep`, `motion_icon:state_crouch`, `motion_icon:state_normal`, `motion_icon:state_run`, `motion_icon:state_sprint` |
| 4 `hud-motion-icon` | string | `motion_icon.xml` |
| 5 `hud-weapon-panel` | layout | `maingame:static_ammo`, `maingame:static_weapon`, `maingame:static_wpn_icon` |
| 5 `hud-weapon-panel` | string | `ui\ui_icon_equipment` |
| 6 `hud-crosshair` | string | `hud\crosshair`, `hud\cursor`, `ui\cursor` |
| 6 `hud-crosshair` | config | `[hud_cursor]` |
| 6 `hud-crosshair` | setting | `cl_dynamiccrosshair` |
| 7 `hud-warning-icons` | layout | `maingame:artefact_static`, `maingame:can_sleep_static`, `maingame:icons_scroll_view`, `maingame:invincible_static`, `maingame:psy_health_static`, `maingame:radiation_static`, `maingame:starvation_static`, `maingame:weapon_jammed_static`, `maingame:wound_static` |
| 7 `hud-warning-icons` | config | `[main_ingame_indicators_thresholds]` |
| 8 `hud-messages` | layout | `ui_custom_msgs:cant_walk`, `ui_custom_msgs:cant_walk_weight`, `ui_custom_msgs:controller_fx`, `ui_custom_msgs:controller_fx2`, `ui_custom_msgs:cs_psy_danger`, `ui_custom_msgs:cs_radiation_danger`, `ui_custom_msgs:gun_jammed`, `ui_custom_msgs:hud_timer`, `ui_custom_msgs:hud_timer_text`, `ui_custom_msgs:item_used`, `ui_custom_msgs:main_task`, `ui_custom_msgs:monster_claws`, `ui_custom_msgs:not_enough_money_mine`, `ui_custom_msgs:not_enough_money_other` |
| 8 `hud-messages` | string | `st_item_used`, `st_no_active_task`, `ui_custom_msgs.xml` |
| 8 `hud-messages` | string-id | `cant_walk`, `cant_walk_weight`, `gun_jammed`, `st_item_used`, `st_no_active_task`, `ui_st_time_remains` |
| 8 `hud-messages` | animation | `monster_claws` |
| 8 `hud-messages` | action | `use_bandage`, `use_medkit` |
| 9 `hud-game-log` | layout | `maingame_pda_msg:icon_static`, `maingame_pda_msg:text_static`, `messages_window:sp_log_list`, `messages_window:sp_log_list2` |
| 9 `hud-game-log` | string | `maingame_pda_msg.xml`, `messages_window.xml`, `news.xml` |
| 9 `hud-game-log` | animation | `ui_main_msgs_short` |
| 9 `hud-game-log` | config | `[maingame_ui] info_msgs_max_show_time`, `[maingame_ui] pda_msgs_max_show_time` |
| 10 `hud-minimap` | layout | `zone_map:minimap`, `zone_map:minimap:background`, `zone_map:minimap:background:dist_text`, `zone_map:minimap:center`, `zone_map:minimap:compass`, `zone_map:minimap:level_frame` |
| 10 `hud-minimap` | string | `ui\ui_nomap2`, `zone_map.xml` |
| 10 `hud-minimap` | config | `[level_maps_single]` |
| 11 `hud-map-spots` | layout | `map_spots.xml`, `map_spots_agroprom.xml`, `map_spots_bar.xml`, `map_spots_darkvalley.xml`, `map_spots_escape.xml`, `map_spots_garbage.xml`, `map_spots_lchangers.xml`, `map_spots_military.xml`, `map_spots_radar.xml`, `map_spots_relations.xml` |
| 11 `hud-map-spots` | string | `map_spots.xml` |
| 11 `hud-map-spots` | config | `task <map_location_hidden>`, `task <map_location_type>` |
| 12 `hud-pda-indicators` | layout | `maingame:flashing_icons`, `maingame:new_contact_snd`, `maingame:static_pda_online` |
| 12 `hud-pda-indicators` | animation | `ui_pda_contacts` |
| 12 `hud-pda-indicators` | config | `[maingame_ui] snd_new_contact`, `[maingame_ui] snd_new_contact1` |
| 13 `hud-pickup-hint` | layout | `maingame:pick_up_item`, `maingame:quick_info` |
| 13 `hud-pickup-hint` | string-id | `car_character_use`, `character_use`, `dead_character_use`, `dead_character_use_or_drag`, `inventory_box_use`, `inventory_item_use` |
| 13 `hud-pickup-hint` | setting | `cl_cod_pickup_mode`, `g_autopickup` |
| 14 `hud-target-info` | string-id | `enemy`, `friend`, `master`, `neutral`, `novice`, `stalker`, `veteran` |
| 14 `hud-target-info` | setting | `hud_crosshair_dist` |
| 15 `hud-zoom` | config | `scope_texture` |
| 15 `hud-zoom` | action | `wpn_zoom` |
| 16 `hud-binocular-frames` | string | `ui\ui_enemy_frame` |
| 16 `hud-binocular-frames` | config | `w_binoc found_snd`, `w_binoc vis_frame_color`, `w_binoc vis_frame_speed`, `w_binoc vision_present` |
| 17 `hud-hit-marks` | animation | `hud_hit_mark` |
| 17 `hud-hit-marks` | config | `[hud_hitmark]` |
| 18 `hud-tutorials` | layout | `game_tutorials:agroprom_tutorial`, `game_tutorials:bar_tutorial`, `game_tutorials:game_over`, `game_tutorials:part_10_sprint`, `game_tutorials:part_11_weakness`, `game_tutorials:part_12_relations`, `game_tutorials:part_14_overload`, `game_tutorials:part_15_hide_weapon`, `game_tutorials:part_1_pda`, `game_tutorials:part_2_binocular`, `game_tutorials:part_3_jump`, `game_tutorials:part_4_crouch`, `game_tutorials:part_5_crouch_down`, `game_tutorials:part_6_radiation`, `game_tutorials:part_7_psy`, `game_tutorials:part_8_wound`, `game_tutorials:part_9_weapon`, `game_tutorials:part_pda_diary`, `game_tutorials:pripyat_tutorial`, `game_tutorials:test`, `game_tutorials:yantar_tutorial`, `ui_credits:credits_seq`, `ui_movies:mov_desire_1`, `ui_movies:mov_desire_2`, `ui_movies:mov_desire_3`, `ui_movies:mov_desire_4`, `ui_movies:mov_desire_5`, `ui_movies:mov_join_osoznanie`, `ui_movies:mov_refuse_osoznanie`, `ui_movies:radar_dream`, `ui_movies:x18_dream`, `ui_movies:yantar_dream` |
| 18 `hud-tutorials` | string | `game_tutorials.xml`, `hud\movie` |
| 18 `hud-tutorials` | string-id | `crouch`, `hide_weapon`, `jump`, `radiation`, `relation`, `sprint`, `weapon`, `wound` |
| 18 `hud-tutorials` | config | `[tutorial_conditions_thresholds]` |
| 19 `ppe-format` | config | `anims\*.ppe` |
| 20 `screen-effects` | effect | `config\creatures\m_bloodsucker.ltx [m_bloodsucker_attack_effector]`, `config\creatures\m_bloodsucker.ltx [m_bloodsucker_vampire_effector]`, `config\creatures\m_bloodsucker.ltx attack_effector =`, `config\creatures\m_bloodsucker.ltx vampire_effector =`, `config\creatures\m_boar.ltx [m_boar_attack_effector]`, `config\creatures\m_boar.ltx attack_effector =`, `config\creatures\m_burer.ltx [m_burer_attack_effector]`, `config\creatures\m_burer.ltx [m_burer_scan_effector]`, `config\creatures\m_burer.ltx attack_effector =`, `config\creatures\m_burer.ltx scan_effector_section =`, `config\creatures\m_cat.ltx [m_cat_attack_effector]`, `config\creatures\m_cat.ltx attack_effector =`, `config\creatures\m_chimera.ltx [m_chimera_attack_effector]`, `config\creatures\m_chimera.ltx attack_effector =`, `config\creatures\m_controller.ltx [controller_aura_pp_effector_section]`, `config\creatures\m_controller.ltx [controller_control_effector]`, `config\creatures\m_controller.ltx [m_controller_attack_effector]`, `config\creatures\m_controller.ltx attack_effector =`, `config\creatures\m_controller.ltx aura_effector =`, `config\creatures\m_controller.ltx aura_effector_hit =`, `config\creatures\m_controller.ltx control_effector =`, `config\creatures\m_dog.ltx [m_dog_attack_effector]`, `config\creatures\m_dog.ltx attack_effector =`, `config\creatures\m_dog_.ltx attack_effector =`, `config\creatures\m_flesh.ltx [m_flesh_attack_effector]`, `config\creatures\m_flesh.ltx attack_effector =`, `config\creatures\m_fracture.ltx attack_effector =`, `config\creatures\m_giant.ltx [m_giant_attack_effector]`, `config\creatures\m_giant.ltx [pseudogigant_threaten_effector]`, `config\creatures\m_giant.ltx attack_effector =`, `config\creatures\m_giant.ltx threaten_effector =`, `config\creatures\m_poltergeist.ltx [m_poltergeist_attack_effector]`, `config\creatures\m_poltergeist.ltx [m_poltergeist_scan_effector]`, `config\creatures\m_poltergeist.ltx attack_effector =`, `config\creatures\m_poltergeist.ltx flame_scan_effector_section =`, `config\creatures\m_pseudodog.ltx [m_pseudodog_attack_effector]`, `config\creatures\m_pseudodog.ltx [psy_dog_aura_effector]`, `config\creatures\m_pseudodog.ltx [psy_dog_phantom_appear_effector]`, `config\creatures\m_pseudodog.ltx appear_effector =`, `config\creatures\m_pseudodog.ltx attack_effector =`, `config\creatures\m_pseudodog.ltx aura_effector =`, `config\creatures\m_snork.ltx [m_snork_attack_effector]`, `config\creatures\m_snork.ltx attack_effector =`, `config\creatures\m_tushkano.ltx attack_effector =`, `config\creatures\m_zombie.ltx [m_zombie_attack_effector]`, `config\creatures\m_zombie.ltx attack_effector =`, `config\misc\effectors.ltx [explode_effector]`, `config\misc\effectors.ltx [monster_attack_effector]`, `config\misc\effectors.ltx [sleep_effector]`, `config\misc\effectors.ltx [small_monster_attack_effector]`, `config\misc\postprocess.ltx [postprocess_anomaly]`, `config\misc\postprocess.ltx [postprocess_base]`, `config\misc\zone_ameba.ltx postprocess =`, `config\misc\zone_burningfuzz.ltx postprocess =`, `config\misc\zone_buzz.ltx postprocess =`, `config\misc\zone_gravi.ltx postprocess =`, `config\misc\zone_kampfire.ltx postprocess =`, `config\misc\zone_mincer.ltx postprocess =`, `config\misc\zone_minefield.ltx postprocess =`, `config\misc\zone_monolith.ltx postprocess =`, `config\misc\zone_mosquitobald.ltx postprocess =`, `config\misc\zone_nogravity.ltx postprocess =`, `config\misc\zone_radioactive.ltx postprocess =`, `config\misc\zone_rustyhair.ltx postprocess =`, `config\misc\zone_teleport.ltx postprocess =`, `config\misc\zone_torrid.ltx postprocess =`, `config\misc\zone_witchesgalantine.ltx postprocess =`, `config\misc\zone_zharka_static.ltx postprocess =`, `config\scripts\bun_psy_zone.ltx postprocess =`, `config\scripts\deadcity_wake.ltx fx_set_* curves`, `config\scripts\earthshake.ltx fx_set_* curves`, `config\scripts\postprocess.ltx fx_set_* curves`, `config\scripts\postprocess1.ltx fx_set_* curves`, `config\scripts\proba.ltx fx_set_* curves`, `config\scripts\sincos.ltx fx_set_* curves`, `config\scripts\teleport.ltx fx_set_* curves` |
| 21 `hud-night-vision` | config | `[device_torch] night_vision`, `[device_torch] night_vision_discharge_time`, `[device_torch] night_vision_effector`, `[device_torch] night_vision_recharge_time`, `[device_torch] night_vision_recharge_time_min`, `[device_torch] night_vision_texture`, `[device_torch] snd_night_vision_broken`, `[device_torch] snd_night_vision_idle`, `[device_torch] snd_night_vision_off`, `[device_torch] snd_night_vision_on` |
| 21 `hud-night-vision` | effect | `config\misc\items.ltx night_vision_effector =` |
| 21 `hud-night-vision` | action | `night_vision` |
| 22 `hud-artefact-panel` | layout | `maingame:artefact_panel` |
| 23 `hud-grenade-indicator` | layout | `grenade:progress` |
| 23 `hud-grenade-indicator` | string | `grenade.xml` |
| 24 `hud-disk-icon` | string | `ui\ui_disk_io` |
| 25 `hud-vehicle-panel` | layout | `car_panel:car_health_progress_bar`, `car_panel:car_health_static`, `maingame:car_panel` |
| 25 `hud-vehicle-panel` | string | `car_panel.xml` |
| 26 `hud-first-person-model` | config | `hud = <section>` |
| approved `ui-layout` | string | `_16.xml`, `color_defs.xml`, `hud\default`, `scroll_bar.xml` |
| approved `title-menu` | string | `message_box.xml`, `ui\ui_ani_cursor` |
| approved `title-menu` | string-id | `gd_master`, `gd_novice`, `gd_stalker`, `gd_veteran` |
| approved `title-menu` | action | `quit` |
| approved `inventory-screen` | string | `inventory_item.xml`, `inventory_new.xml`, `st_attach_gl_to_rifle`, `st_attach_scope_to_pistol`, `st_attach_scope_to_rifle`, `st_attach_silencer_to_pistol`, `st_attach_silencer_to_rifle`, `st_detach_gl`, `st_detach_scope`, `st_detach_silencer`, `st_dress_outfit`, `st_drink`, `st_drop`, `st_drop_all`, `st_eat`, `st_move_on_belt`, `st_move_to_bag`, `st_move_to_slot`, `st_undress_outfit`, `st_unload_magazine`, `st_use`, `ui\ui_grid` |
| approved `inventory-screen` | string-id | `st_attach_gl_to_rifle`, `st_attach_scope_to_pistol`, `st_attach_scope_to_rifle`, `st_attach_silencer_to_pistol`, `st_attach_silencer_to_rifle`, `st_detach_gl`, `st_detach_scope`, `st_detach_silencer`, `st_dress_outfit`, `st_drink`, `st_drop`, `st_drop_all`, `st_eat`, `st_move_on_belt`, `st_move_to_bag`, `st_move_to_slot`, `st_undress_outfit`, `st_unload_magazine`, `st_use`, `ui_inv_outfit_burn_protection`, `ui_inv_outfit_chemical_burn_protection`, `ui_inv_outfit_explosion_protection`, `ui_inv_outfit_fire_wound_protection`, `ui_inv_outfit_radiation_protection`, `ui_inv_outfit_shock_protection`, `ui_inv_outfit_strike_protection`, `ui_inv_outfit_telepatic_protection`, `ui_inv_outfit_wound_protection`, `ui_inv_weight` |
| approved `options-screens` | string | `ui_keybinding.xml` |
| approved `options-screens` | animation | `ui_map_area_anim` |
| approved `options-screens` | setting | `_preset`, `ai_use_torch_dynamic_lights`, `g_game_difficulty`, `mouse_invert`, `mouse_sens`, `ph_iterations`, `r2_ls_squality`, `r2_slight_fade`, `r2_sun`, `r2_sun_details`, `r__detail_density`, `r__geometry_lod`, `r__supersample`, `r__tf_aniso`, `renderer`, `rs_c_brightness`, `rs_c_contrast`, `rs_c_gamma`, `rs_fullscreen`, `rs_refresh_60hz`, `rs_v_sync`, `rs_vis_distance`, `snd_cache_size`, `snd_efx`, `snd_freq`, `snd_targets`, `snd_volume_eff`, `snd_volume_music`, `texture_lod`, `vid_mode` |
| approved `save-load-screens` | layout | `ui_custom_msgs:autosave`, `ui_custom_msgs:game_saved` |
| approved `save-load-screens` | string | `st_game_saved`, `ui\ui_noise` |
| approved `save-load-screens` | string-id | `st_game_saved` |
| approved `save-load-screens` | action | `quick_load`, `quick_save` |
| approved `loading-screen` | string | `st_client_connecting_to`, `st_client_spawning`, `st_client_synchronising`, `st_creating_new_game`, `st_loading_ai_objects`, `st_loading_alife_simulator`, `st_loading_saved_game`, `st_loading_textures`, `st_server_connecting`, `st_server_starting` |
| approved `loading-screen` | string-id | `st_client_connecting_to`, `st_client_spawning`, `st_client_synchronising`, `st_creating_new_game`, `st_loading_ai_objects`, `st_loading_alife_simulator`, `st_loading_saved_game`, `st_loading_textures`, `st_server_connecting`, `st_server_starting` |
| approved `level-loading` | layout | `ui_movies:intro_game` |
| approved `startup` | layout | `ui_movies:intro_logo` |
| approved `player-movement` | effect | `config\misc\effectors.ltx [bobbing_effector]` |
| approved `player-movement` | action | `accel`, `back`, `crouch`, `crouch_toggle`, `down`, `forward`, `jump`, `left`, `llookout`, `lstrafe`, `right`, `rlookout`, `rstrafe`, `sprint_toggle`, `up` |
| B-camera | effect | `config\creatures\m_giant.ltx step_effector_amplitude =`, `config\creatures\m_giant.ltx step_effector_period_number =`, `config\creatures\m_giant.ltx step_effector_time =`, `config\misc\effectors.ltx [zoom_inertion_effector]`, `config\scripts\bar_arena_combat_triger.ltx cam_effector =` |
| B-camera | action | `cam_zoom_in`, `cam_zoom_out` |
| B-coincident | string-id | `actor`, `artefact`, `global_map`, `psy_dog`, `trader`, `wpn_hpsa` |
| B-console | setting | `rs_stats` |
| B-console | action | `console`, `screenshot` |
| B-gameplay | action | `artefact`, `drop`, `next_slot`, `prev_slot`, `use`, `wpn_1`, `wpn_2`, `wpn_3`, `wpn_4`, `wpn_5`, `wpn_6`, `wpn_fire`, `wpn_firemode_next`, `wpn_firemode_prev`, `wpn_func`, `wpn_next`, `wpn_reload` |
| B-mp | layout | `maingame:speech_menu`, `map_spots_mp.xml`, `messages_window:chat_edit_box`, `messages_window:chat_log_list`, `messages_window:chat_prefix`, `messages_window:mp_log_list`, `messages_window:reinforcement`, `ui_custom_msgs:max_ping_warn_1`, `ui_custom_msgs:max_ping_warn_2`, `ui_custom_msgs:max_ping_warn_3`, `ui_custom_msgs:max_ping_warn_4` |
| B-mp | string | `buy_menu_item.xml`, `map_desc.xml`, `mp_buy_menu.xml`, `skin_selector.xml`, `spawn.xml`, `st_artefacthunt`, `st_deathmatch`, `st_mp_disabled_voting`, `st_mp_no_current_voting`, `st_mp_only_one_voting`, `st_mp_ready`, `st_mp_say_to_all`, `st_mp_say_to_team`, `st_mp_voting_failed`, `st_mp_voting_succeed`, `st_team_deathmatch`, `stats.xml`, `ui\ui_mp_frags_selection`, `ui\ui_mp_icon_kill`, `ui\ui_mp_icon_rank`, `ui_game_ahunt.xml`, `ui_game_dm.xml`, `ui_game_tdm.xml`, `ui_mapinfo.xml`, `voting_category.xml` |
| B-mp | string-id | `game type`, `icon`, `mm_mp_gamemode`, `mp_art_destroyed`, `mp_art_spowned`, `mp_artefacts`, `mp_artefacts_upcase`, `mp_ban`, `mp_by_anomaly`, `mp_change_map`, `mp_change_weather`, `mp_chat`, `mp_connected`, `mp_deaths`, `mp_description`, `mp_disconnected`, `mp_entered_game`, `mp_first_eye`, `mp_frags`, `mp_frags_upcase`, `mp_free_fly`, `mp_free_look`, `mp_from`, `mp_go`, `mp_has_drop_art`, `mp_has_tak_art`, `mp_is_now`, `mp_joined`, `mp_kick`, `mp_look_at`, `mp_match_started`, `mp_name`, `mp_ping`, `mp_player_wins`, `mp_players`, `mp_press_fire2spectator`, `mp_press_jump2pay_spaw`, `mp_press_jump2select_skin`, `mp_press_jump2select_team`, `mp_press_jump2spawn`, `mp_press_jump2start`, `mp_press_to_buy`, `mp_press_yes2pay`, `mp_ready`, `mp_restart`, `mp_restart_fast`, `mp_scores`, `mp_si_afbearer_cant_sprint`, `mp_si_afcount`, `mp_si_afrespawntime`, `mp_si_afstaytime`, `mp_si_anomalies`, `mp_si_anomalies_period`, `mp_si_any_time`, `mp_si_artefact_captured`, `mp_si_artefacts`, `mp_si_autoteam_balance`, `mp_si_autoteam_swap`, `mp_si_disabled`, `mp_si_enabled`, `mp_si_first_eye`, `mp_si_forcerespawn`, `mp_si_fraglimit`, `mp_si_free_fly`, `mp_si_free_look`, `mp_si_friendly_fire`, `mp_si_friendly_indicators`, `mp_si_friendly_names`, `mp_si_infinite`, `mp_si_invinsibility`, `mp_si_invinsibility_indicators`, `mp_si_invinsibility_time`, `mp_si_look_at`, `mp_si_maprotation`, `mp_si_min`, `mp_si_no`, `mp_si_players_respawn`, `mp_si_return_players`, `mp_si_sec`, `mp_si_servername`, `mp_si_shielded_bases`, `mp_si_spectatormodes`, `mp_si_team_only`, `mp_si_time_limit`, `mp_si_version`, `mp_si_voting`, `mp_si_warmuptime`, `mp_si_yes`, `mp_spectator`, `mp_spectators`, `mp_status`, `mp_switched_to`, `mp_team_wins`, `mp_time2respawn`, `mp_time2start`, `mp_timeleft`, `mp_voted`, `mp_voted_no`, `mp_voted_yes`, `mp_voting_broken`, `mp_voting_started`, `mp_voting_started_msg`, `mp_with_backstab`, `mp_with_headshot`, `mp_your_rank`, `ping`, `players`, `server name`, `st_artefacthunt`, `st_deathmatch`, `st_mp_disabled_voting`, `st_mp_no_current_voting`, `st_mp_ready`, `st_mp_say_to_all`, `st_mp_say_to_team`, `st_mp_voting_failed`, `st_mp_voting_succeed`, `st_team_deathmatch`, `ui_inv_cant_buy_item`, `ui_inv_count_restr`, `ui_inv_has`, `ui_inv_need`, `ui_inv_not_enought_money`, `ui_inv_rank_restr`, `ui_inv_you_already_have`, `version` |
| B-mp | animation | `ui_mm_mp_srvinfo`, `ui_mp_chat` |
| B-mp | setting | `bemsg`, `mm_mm_net_srv_dedicated`, `mm_net_con_publicserver`, `mm_net_con_spectator`, `mm_net_con_spectator_on`, `mm_net_filter_battleye`, `mm_net_filter_empty`, `mm_net_filter_full`, `mm_net_filter_listen`, `mm_net_filter_pass`, `mm_net_filter_wo_ff`, `mm_net_filter_wo_pass`, `mm_net_srv_gamemode`, `mm_net_srv_maxplayers`, `mm_net_srv_name`, `mm_net_srv_reinforcement_type`, `mm_net_use_battleye`, `mm_net_weather_rateofchange`, `net_cl_icurvesize`, `net_cl_icurvetype`, `net_cl_interpolation`, `net_cl_log_data`, `net_compressor_enabled`, `net_compressor_gather_stats`, `net_dbg_dump_export_obj`, `net_dbg_dump_import_obj`, `net_dbg_dump_update_read`, `net_dbg_dump_update_write`, `net_dedicated_sleep`, `net_sv_gpmode`, `net_sv_log_data`, `net_sv_pending_lim`, `net_sv_update_rate`, `sv_anomalies_enabled`, `sv_anomalies_length`, `sv_artefact_respawn_delta`, `sv_artefact_spawn_force`, `sv_artefact_stay_time`, `sv_artefacts_count`, `sv_auto_team_balance`, `sv_auto_team_swap`, `sv_bearercantsprint`, `sv_client_reconnect_time`, `sv_console_update_rate`, `sv_dedicated_server_update_rate`, `sv_dmgblockindicator`, `sv_dmgblocktime`, `sv_dump_online_statistics_period`, `sv_forcerespawn`, `sv_fraglimit`, `sv_friendly_indicators`, `sv_friendly_names`, `sv_friendlyfire`, `sv_hail_to_winner_time`, `sv_max_ping_limit`, `sv_pda_hunt`, `sv_reinforcement_time`, `sv_remove_corpse`, `sv_remove_weapon`, `sv_returnplayers`, `sv_rpoint_freeze_time`, `sv_shieldedbases`, `sv_spectr_firsteye`, `sv_spectr_freefly`, `sv_spectr_freelook`, `sv_spectr_lookat`, `sv_spectr_teamcamera`, `sv_statistic_collect`, `sv_teamkill_limit`, `sv_teamkill_punish`, `sv_timelimit`, `sv_vote_enabled`, `sv_vote_participants`, `sv_vote_quota`, `sv_vote_time`, `sv_warm_up` |
| B-mp | action | `buy_menu`, `chat`, `chat_team`, `skin_menu`, `speech_menu_0`, `speech_menu_1`, `team_menu`, `vote`, `vote_begin`, `vote_no`, `vote_yes` |
| B-screen | string | `actor_statistic.xml`, `carbody_item.xml`, `carbody_new.xml`, `encyclopedia.xml`, `encyclopedia_item.xml`, `events_new.xml`, `game_tasks.xml`, `hint_item.xml`, `job_item.xml`, `pda.xml`, `pda_character.xml`, `pda_contacts_new.xml`, `pda_dialog_character.xml`, `pda_events.xml`, `pda_map.xml`, `stalkers_ranking.xml`, `stalkers_ranking_character.xml`, `statisticwnd.xml`, `talk.xml`, `trade.xml`, `trade_character.xml`, `trade_item.xml` |
| B-screen | action | `active_jobs`, `contacts`, `inventory`, `map` |
| B-settings | setting | `cam_inert`, `cam_slide_inert`, `g_always_run`, `g_backrun`, `g_corpsenum`, `g_eventdelay`, `load_last_save`, `ph_frequency`, `snd_acceleration` |
| B-torch | action | `torch` |
| B-unused | layout | `ui_custom_msgs:sleep_001` |

## Provenance

- Date: 2026-11-24. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`.
- Data examined: winning copies of `config\` and `scripts\`, `spawns\all.spawn` (the objects'
  custom data, searched as text) and `lanims.xr`, extracted with the
  project's archive tool; the archive listing (texture and `.ppe` paths); the default
  `__support\userdocs\user.ltx`.
- Analysis: searches of the layout files, configuration (including `config\misc\*.script` and the
  dialog files), string tables, scripts and the spawn custom data; printable-string scans of the
  binaries in `bin\`. No disassembly or execution.
- Checks: `checks/hud-scope/c1_layout.py` … `c7_coverage.py` (run with `python3` or `python3 -I`), with `extract.py` (extraction), `lib.py`
  and `ltx.py` (helpers); all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Anomaly marks on the map from the detector: the detector item of `config\misc\items.ltx` gives each detected anomaly class a `zone_map_location_<n>` (value `anomaly_zone_location`, a spot type of `map_spots.xml`; the pattern `zone_map_location_%d` is named in `xrGame.dll`); no spec owns this source of map spots (spec 11 is the natural owner).
- Map marks of information portions: `config\gameplay\info_escape.xml` gives information portions a `<location>` (level, object, icon, text); whether the program reads it (the word `location` is not named in `xrGame.dll`) and whether it puts marks on the minimap is not addressed.
- The list of HUD-like layout files believed never loaded (1.1) omits `follower_panel.xml`, `sleep_dialog.xml`, `sleep_dialog_new.xml` and `map_spots_rostok.xml` (the last not included by `map_spots.xml`), which no binary of `bin\`, script or other configuration file names either.
- The `xrGame.dll` string `game_tasks.xml` is owned by `B-screen`, but no layout file of that name exists; the file is `config\gameplay\game_tasks.xml`, a task-definition file (also listed by `[game_tasks]` of `system.ltx`), whose owner should be the task system rather than a screen.
- The zoom-step actions `wpn_zoom_inc` and `wpn_zoom_dec` are named in `xrGame.dll` but neither bound in `default_controls.ltx` nor on the key-binding screen, so they fall outside the coverage universe; spec 15 should establish whether they change the zoom picture.
