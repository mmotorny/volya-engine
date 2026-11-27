# The PDA: scope and split

Scoping specification for the PDA of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition): the window the
player opens over the level, with its tabs (tasks, map, diary, contacts, ranking, statistics,
encyclopedia), and the game data and records those tabs show (tasks, encyclopedia and diary articles,
news, contacts, ranks and relations, statistics, level maps). It lists every part, the data each part
reads, and proposes how to split the area into small specifications, each covering one format or one
behaviour, that can be investigated and reviewed on their own; it states for each which approved
specifications it builds on and which systems not yet specified it waits on. It specifies no behaviour
itself.

It builds on the approved specifications `ui-layout`, `text-and-fonts`, `ltx-config`, `colour-animations`,
`sav`, `db-archive` (`docs/formats/`), `hud-scope`, `inventory-screen`, `options-screens`,
`save-load-screens`, `console-key-bindings`, `console-window`, `player-state`, `player-movement`,
`sound-notifications`, `level-loading`,
`level-objects`, `script-loading`, `script-environment`, `script-api-scope`, `narrowed-specs-scope`
(`docs/behavior/`), all cited by name.

Conventions:

- **Named in `xrGame.dll`** means the exact text occurs in the shipped `bin\xrGame.dll` as a printable
  string of 4 or more characters, or as one `:`-separated part of such a string (layout paths such as
  `task_root_item:caption`). It shows that the program refers to that file, element or section; what it
  does with it is for the proposed spec to establish.
- The functional descriptions (which tab shows what, what a record means) are readings of the names in
  the data (element names, string ids and their texts, tab ids) and are **believed** until the proposed
  spec establishes them; they fix what each spec must cover, not how the part behaves. Elsewhere
  **believed** marks such a reading with its reason; **unestablished** marks what this scoping work could
  not settle and leaves to the proposed spec.
- Every fact carries its check, a script in `checks/pda-scope/` of the dirty workspace: `c1` =
  `c1_layout.py`, `c2` = `c2_binary.py`, `c3` = `c3_config.py`, `c4` = `c4_strings.py`, `c5` =
  `c5_scripts.py`, `c6` = `c6_coverage.py`, followed by the label of the assertion (e.g. c1 L2). File
  paths are archive paths of the winning copies (the later archive wins); layout files are in
  `config\ui\`, task, article and information-portion files in `config\gameplay\`, string tables in
  `config\text\rus\`.

## 1. Data of the PDA

### 1.1 Layout files

The window and its tabs are built from these files (c1 L1, L4). "Named" is named in `xrGame.dll`.

| File | Named | Part (believed from names) | Notes |
|---|---|---|---|
| `pda.xml` | yes | the window: background, tab bar, clock, new-content marks | 1.2 |
| `ui_pda.xml` | (texture description) | the background picture's id `ui_PDA` | listed by `[texture_desc]` (ui-layout 6.1) |
| `pda_events.xml` | yes | tasks tab: task list with filter, task description, a map pane | 1.3 |
| `job_item.xml` | yes | the task list's items (task, objective) | 1.3 |
| `pda_map.xml` | yes | map tab | 1.3 |
| `events_new.xml` | yes | diary tab: journal and news history | 1.3 |
| `news.xml` | yes | the news items (`news_item`, `talk_item`) | 1.3 |
| `pda_contacts_new.xml` | yes | contacts tab | 1.3 |
| `pda_character.xml` | yes | a contact's details | 1.3 |
| `stalkers_ranking.xml` | yes | ranking tab | 1.3 |
| `stalkers_ranking_character.xml` | yes | a ranked character's details (`_16` variant exists) | 1.3 |
| `actor_statistic.xml` | yes | statistics tab | 1.3 |
| `encyclopedia.xml` | yes | encyclopedia tab | 1.3 |
| `encyclopedia_item.xml` | yes | article items (`events_wnd`, `encyclopedia_wnd`) | 1.3 |
| `pda_dialog_character.xml` | yes | a character panel with rank, reputation, community (`_16` variant exists) | 3 |

Of these only `pda_dialog_character.xml` and `stalkers_ranking_character.xml` have a `_16` variant
(ui-layout 1.1) (c1 L1). None of the 13 window files (the table less `ui_pda.xml` and
`pda_dialog_character.xml`) names a sound (c1 L1).

Older or other files of the same family that no binary, script or configuration file names (as
`<name>.xml` or a `ui\<name>` path), believed unused (`B-unused`): `pda_new.xml` (a four-button tab bar
Events, Contacts, Map, Encyclopedia; c1 L5), `map.xml`, `pda_contacts.xml`, `pda_character_new.xml`,
`actor_diary.xml`, `global_map.xml` (names `ui\ui_global_map_small`, which no archive holds; c1 L5),
`news_templates.xml` (4 `news_template` texts; c1 L5), `pda_dialog.xml` (0 bytes), `map_character.xml`,
`jobs.xml`, `jobs_new.xml`, `contracts.xml`, `contracts_character.xml` (c1 L1). `hud-scope` spec 9
lists `news_templates.xml` as data of the game log "as far as the log uses it"; nothing here suggests
the PDA uses it.

### 1.2 The window (`pda.xml`)

Top elements in order: `off_button`, `main`, `background_static`, `mbbackground_frame_line`,
`timer_frame_line`, `tab`, `updated_section_static`, `old_section_static` (c1 L2).

- `background_static` at (3, 20), 891 × 643, texture id `ui_PDA`: `ui_pda.xml` defines that one id as
  (0, 0) 1020 × 702 of `ui\ui_PDA` (c1 L2). `textures\ui\ui_pda.dds` (1024 × 1024 DXT5) is byte-identical
  to `textures\ui\eng_pda\ui_pda.dds` and differs from `textures\ui\rus_pda\ui_pda.dds`; the names
  `rus_pda` and `eng_pda` occur in no binary, script or configuration file (c1 L3). Whether the Russian
  edition ever shows the `rus_pda` picture is **unestablished** (believed not: nothing names it).
- `tab` at (129, 573), 550 × 30, with 7 buttons in this order and these string texts (c1 L2, c4 S2):
  `pda_btn_quests` «Задачи», `pda_btn_map` «План», `pda_btn_diary` «Журнал», `pda_btn_contacts`
  «Контакты», `pda_btn_ranking` «Ранги», `pda_btn_statistics` «Данные», `pda_btn_encyclopedia`
  «Справка», each with the hint `<id>_hint`.
- `timer_frame_line` at (710, 573), 161 × 32, with a `title` (believed the game clock).
- `updated_section_static` (texture id `ui_pda_m_noupdate`) and `old_section_static` (`ui_pda_m_update`),
  9 × 11, both ids of `ui_common.xml` (c1 L2): believed the marks of tabs with new or seen content.
  `events_new.xml` has the same two elements (c1 L4).
- `off_button`, 1 × 1, texture `ui\ui_pda_button`, which no archive holds (c1 L2); its hint string
  `pda_btn_off_hint` «Выключить» exists in `ui_st_pda` (c4) but `pda.xml` does not reference it.

The tab windows' `main_wnd` all sit at (110, 90), 761 wide: 475 high in `pda_events.xml`,
`events_new.xml`, `pda_contacts_new.xml`, `pda_map.xml`; 455 in `encyclopedia.xml`,
`stalkers_ranking.xml`, `actor_statistic.xml` (c1 L4).

The tutorials' item element `pda_section` takes the values `pda_quests` (6 items), `pda_map` (3),
`pda_closed` (2), `pda_contacts`, `pda_diary`, `pda_ranking`, `pda_statistics`, `pda_encyclopedia` (1
each); all but `pda_closed` are named in `xrGame.dll`, `pda_closed` by neither binary (c2 B3). The seven
named values match the seven tabs (believed by name).

### 1.3 The tabs

- **Tasks** (`pda_events.xml`, items `job_item.xml`). Left: a `list` with a `filter_tab` of buttons
  `pda_btn_active`, `pda_btn_accomplished`, `pda_btn_failed` and a `notused_button` `pda_btn_own`; a check
  button `primary_or_all` (`check_mode` 1, text `show_only_general_tasks`). Right: `task_descr_view` (a
  `scroll_view` of `objective_item`s) and a `map_wnd` with its own tool bar (c1 L4). Items:
  `task_root_item` (`image`, `caption`, `caption_time`, `rem_time`, `switch_description_btn`,
  `location_btn`), `task_sub_item` (`state_image`, `description` with `text_colors` `accomplished`,
  `failed`, `active`, `active_objecttive_image`, `show_pointer_btn`, `show_descr_btn`),
  `notused_task_user_item`, `notused_edit_user_item` (c1 L4). The paths `main_wnd:right_frame:map_wnd`,
  `main_wnd:right_frame:task_descr_view`, `task_root_item:switch_description_btn`,
  `task_sub_item:show_descr_btn` and `task_sub_item:description:text_colors:accomplished` are named; the
  `notused_*` elements, `location_btn`, `show_pointer_btn` and `primary_or_all` are not (c2 B5). The
  values `show_only_general_tasks` and `pda_show_general_or_all_tasks` have no string (c4 S1).
- **Map** (`pda_map.xml`). `map_wnd:main_wnd`, a header line with `title` and a `tool_bar` of
  `global_map_btn`, `actor_btn`, `notused_add_spot_btn`, `zoom_in_btn`, `zoom_out_btn`; a `main_map_frame`
  with `level_frame` (8, 41) 725 × 406; a `text_hint` (c1 L4). The paths `:global_map_btn` and
  `:main_wnd:main_map_frame:level_frame` are named (c2 B5). Hints: `global_map_btn_hint` «На карту зоны»,
  `actor_btn_hint` «Центр на Меченом», `zoom_in_btn_hint`, `zoom_out_btn_hint` (c4).
- **Diary** (`events_new.xml`, items `news.xml`). A `filter_tab` of `pda_btn_journal` «Журнал»,
  `pda_btn_news` «История» and an `_button` `pda_btn_info`; a tree list `src_list`; on the right a
  `scroll_view` (c1 L4, c4). `news.xml` holds `list`, `news_item` (text, date, image) and `talk_item`
  (c1 L4); `news_item` and `talk_item` are named (c2 B5).
- **Contacts** (`pda_contacts_new.xml`, details `pda_character.xml`). Frames, a `list`, a `detail_list`
  (c1 L4); the details: `name_static`, `icon_static`, `community_*`, `reputation_*`, `relation_*`,
  `rank_*` captions and values (captions `ui_st_community`, `ui_st_reputation`, `ui_st_relation`,
  `ui_st_rank`), and `pda_char_auto_statics` with the picture `ui\ui_pda_contacts_online` (c1 L4).
- **Ranking** (`stalkers_ranking.xml`, details `stalkers_ranking_character.xml`). Title `st_pda_top20`
  «Лучшая 20-ка сталкеров» (c4 S2); `stalkers_list` with item templates `item_human`, `item_actor`,
  `item_ellipsis`, `no_items`; details `icon_static`, rank, community, relation and a `_biography_list` (the binary names
  `biography_list`, not `_biography_list`: believed switched off by the leading `_`; c1 L4, c2 B8, c4).
- **Statistics** (`actor_statistic.xml`). `actor_stats_wnd` with `master_part`s of ids `foo`,
  `stalkerkills`, `monsterkills`, `quests`, `total` (texts `st_pda_statistic_score`,
  `st_pda_stalker_kills`, `st_pda_monster_kills`, `st_pda_quest_complete`, `st_pda_total`),
  `detail_part_def`, `detail_part_5`, and an `_relation_communities_list` of 9 communities (c1 L4).
  `xrGame.dll` names `stalkerkills`, `monsterkills`, `quests`, `artefacts`, `total`, `detail_part_%s`,
  `st_detail_list_for_%s`, `relation_communities_list` (without the leading `_`) and
  `unknown_community`; the string table has five `st_detail_list_for_*` texts (artefacts,
  community_relations, monsterkills, quests, stalkerkills) (c2 B8).
- **Encyclopedia** (`encyclopedia.xml`, items `encyclopedia_item.xml`). An index tree `idx_list` (fonts
  `tree_root_font`, `tree_item_font`), an `info_list`, `article_header_static` (c1 L4).

The tab files share the controls of section 4, PD-2: frame windows (`ui\ui_pda_frame_sub` with
`_lt` … `_rb` files), frame lines with a title (`ui_string_01` … `_03` with `_b`, `_back`, `_e` files),
buttons whose looks are ids with `_e`, `_h`, `_t`, `_d` suffixes (`ui_icons_newPDA_*` in `ui_common.xml`),
the corner animations `ui\ui_pda_anims_01` … `_05` and the lines `ui\ui_pda_*_line*` (c1 L6).

### 1.4 Configuration and content files

- **Level maps** (`config\game_maps_single.ltx`; c3 C1). `[level_maps_single]` lists 17 levels (first
  `l01_escape`). `[global_map]`: `texture = ui\ui_global_map` (1024 wide × 2048 high, DXT1),
  `bound_rect = 0.0,0.0, 1024, 2634.0`, `max_zoom = 6.0`. Each listed level has a section with
  `global_rect` and, except `l12u_control_monolith` and `l12u_sarcofag`, a `texture` (all present) and a
  `bound_rect`; e.g. `l01_escape`: `map\map_escape` (512 × 1024, DXT5), `bound_rect = -317.164,
  -633.718, 404.880, 811.627`, `global_rect = 355.0,2011.0, 576.0,2453.000000`. Five further sections
  are not in the list (`l10u_bunker`, `l06u_labx16`, `l09_deadcity` with the absent `map\map_deadcity`,
  `andy_test`, `ai_test`). The keys `level_maps_single`, `global_map`, `bound_rect`, `global_rect`,
  `max_zoom` are named.
- **`level.ltx` `[level_map]`** (c3 C1). Of the 18 single-player levels' `level.ltx` files only
  `l01_escape` (`texture = ui\ui_map_escape`, absent, with `x1`, `x2`, `z1`, `z2`) and `l08_yantar`
  (`ui\ui_map_mp_factory`) have the section; `xrGame.dll` names `level_map` only as a layout-path suffix.
  Believed not read for the map (`level-loading` lists the file as required to exist).
- **Tasks** (c3 C4). `[game_tasks] files = game_tasks`; `game_tasks.xml` includes 12 files (11
  `tasks_<level>.xml` and `game_tasks_by_vendor.xml`, 84 tasks) and defines 164 `game_task`s (attributes
  `id`, `prio` on 61; one `title`) with 340 `objective`s, whose children are `text` 313, `icon` 268,
  `infoportion_complete` 214, `map_location_type` 162, `object_story_id` 162, `function_fail` 113,
  `function_complete` 103, `article` 51, `infoportion_fail` 50, `map_location_hidden` 26,
  `infoportion_set_complete` 19, `infoportion_set_fail` 12, `function_call_complete` 4,
  `function_call_fail` 1; all these names are named.
- **Encyclopedia and diary articles** (c3 C2). `[encyclopedia]` lists 8 files with 429 `article`s
  (attributes `id`, `name`, `group`, `article_type`; children `text` on all, `texture`, `ltx`,
  `texture_`); `storyline_info_to_diary.xml` brings 254 of them through 14 includes. `article_type`
  values: none 166, `task` 177, `journal` 37, `Journal` 11, `encyclopedia` 21, `info` 11, `1` 6; groups
  may have `/`-separated levels. `xrGame.dll` names `article_type`, `journal`, `encyclopedia`, `task`,
  `info` and the message "incorrect article type definition for [%s]" (c2 B7).
- **Information portions** (c3 C3). `[info_portions]` lists 32 files with 1,707 `info_portion`s, whose
  children are `article` 297, `task` 79, `disable` 61, `action` 2, `dialog` 1. Of the article references
  3 name no article (`agr_nii_fab_mil_task`, `agr_nii_security_shedule`, `tr_escape_broken_car`); of the
  task references 6 (5 ids) name no task (c3 C3, C4).
- **UI information portions** (c2 B4). `info_portions.xml` declares, as "special info portion that sends
  to actor when it use in-game UI", 21 `ui_pda*` portions; `ui_pda` and `ui_pda_hide` disable each other.
  Named: `ui_pda`, `ui_pda_hide`, `ui_pda_contacts`, `ui_pda_map_local`, `ui_pda_news_hide`,
  `ui_pda_encyclopedia`, `ui_pda_actor_info`, `ui_pda_ranking`, `ui_pda_events`; the other 12 are not.
  The scripts read them: `xr_conditions.can_send_tutorial` is false while the player has `ui_inventory`
  or `ui_pda` (c5 P5).
- **Relations and ranks** (`config\creatures\game_relations.ltx`; c3 C5). `[game_relations]`
  `rating_names = novice, 300, experienced, 600, veteran, 900, master`, `reputation_names = terrible,
  -1000, very_bad, -150, bad, -50, neutral, 50, good, 150, very_good, 1000, excellent`, `goodwill_names =
  enemy, -400, indifferent, 500, friendly`, and 15 communities with indices.
- **The PDA item** (c3 C6). `device_pda` (`config\misc\items.ltx`): class `D_PDA`, `radius` 50, `slot` 7,
  the only `D_PDA` section; the quest PDAs `bar_tiran_pda`, `bar_lucky_pda`, `mil_svoboda_leader_pda` are
  class `II_ATTCH`. Its contact list is specified by `sound-notifications` 3.
- **Map location types** (`map_spots.xml` with its includes; c3 C7). 213 entries, of which 98 are
  location types (entries with a `level_map` or `mini_map` child): 33 with only `level_map`, 6 with only
  `mini_map`, 59 with both. `level_map_spot_border` and `mini_map_spot_border` are named.
- **Keys** (c2 B1, B2). The binary's action-name table holds `contacts`, `map`, `active_jobs` (and
  `inventory`, `scores`) and no action named `pda`; default bindings `active_jobs` P, `map` M, `contacts`
  H; the key-binding screen's `kb_active_jobs`, `kb_map`, `kb_contacts` (`options-screens` 11). How the
  three actions open the PDA is `console-key-bindings` 5 rule 3; which tab each opens is believed from
  the names (tasks, map, contacts).
- **News show time** (c2 B7). `[maingame_ui] pda_msgs_max_show_time = 20000` (owned by `hud-scope` spec 9).
- **Strings** (c4). `ui_st_pda` defines 68 ids (tab texts, hints, captions); the window files use 65
  distinct text and hint values, of which 6 have no string (`##`, `...`, `...empty...`,
  `pda_btn_info_hint`, `pda_show_general_or_all_tasks`, `show_only_general_tasks`). Task texts are in
  `stable_task_manager` and the 11 `string_table_tasks_*` tables, map-spot hints in 10
  `string_table_mapspots_*`, statistics captions in `stable_statistic_caption`, all through
  `string_table_includes` (c4 S3).
- **Saved state.** `sav` 9 already gives the save layout of every PDA record: known information
  portions (part 1), relations (2), talk contacts (3), encyclopedia articles (4: time, id, read flag,
  type), news (5: type, text, time, texture and rectangle), map locations (7), game tasks (8, record 9.8)
  with the active task and objective, and statistics (9, record 9.10).

### 1.5 Scripts that feed the PDA

Calls in the shipped scripts (`.` or `:` before the name; c5 P1), all names in `xrGame.dll`:

| Function | Calls | Scripts |
|---|---|---|
| `give_game_news` | 16 | `_test`, `mob_remark`, `news_manager`, `test_heli_2` |
| `give_talk_message` | 11 | `_test`, `news_manager`, `task_manager` |
| `give_task` | 5 | `_test`, `task_manager` |
| `set_task_state` | 22 | `dialogs_military`, `info_test`, `info_test1`, `level_tasks`, `mil_tasks`, `test_heli_2`, `xr_effects` |
| `map_add_object_spot` / `_ser` / `map_remove_object_spot` / `map_change_spot_hint` / `map_has_object_spot` | 37 / 3 / 15 / 4 / 4 | (owned by `hud-scope` spec 11) |
| `set_map_hint`, `set_map_location` (objective) | 5, 5 | `task_manager` |
| `set_article_id` (objective) | 3 | `_test`, `task_manager` |
| `change_character_reputation`, `set_character_rank` | 3, 7 | `dialogs`, `task_manager`; `_g`, `task_manager`, `xr_statistic` |
| `add_to_ranking`, `remove_from_ranking` | 2, 3 | `bind_stalker`, `se_stalker`; and `xr_motivator` |
| `give_info_portion`, `disable_info_portion` | 800, 17 | many |
| `set_relation` | 39 | 10 scripts |

`news_manager` defines `send_tip`, `send_tip_nosound`, `send_task`, `send_encyclopedy`, `send_treasure`,
`get_inv_name`, `relocate_item`, `relocate_money` (c5 P2). The player's binder (`bind_stalker`) sets the
callbacks `article_info` (number 12) and `task_state` (13); `map_location_added` (14) only in a comment
(c2 B6). Its article callback sends news `send_encyclopedy("diary", …)` for the group `Diary` and
`("encyclopedy", …)` otherwise; its task callback, for objective 0, sends news `fail`, `complete` (after
`task_manager.reward_by_task`) or `new` (c5 P3). The tutorial `part_1_pda` is started by `_test`,
`test` and `escape_dialog` (`run_tutorial_pda`) (c5 P4). Which of these scripts the game loads is
`script-loading`'s and SE-6's.

## 2. The PDA map and the minimap

Both maps draw the current level's picture from the same level section (`texture`, `bound_rect`) of
`game_maps_single.ltx` (1.4; `hud-scope` spec 10 lists them as the minimap's data), and both draw the
same map locations: a location's type (`map_spots.xml`) gives a `level_map` spot (the PDA map, believed
by name, and `level_map_spot_border`) and/or a `mini_map` spot (the minimap; `mini_map_spot_border`)
(c3 C7). The PDA map adds what the minimap does not use: the global map (`[global_map]`, `global_rect`
of each level, `max_zoom`) and the tool bar's zoom and centring. The list of locations (who adds and
removes them, their hints, which are saved, how a task objective's location becomes one, the detector's
anomaly marks) is owned by `hud-scope` spec 11 (`hud-map-spots`); this split does not re-own it. The
format of the level-map data is proposed here as its own spec (PD-3) that the minimap spec can build on.

## 3. Boundary

- **HUD** (`B-hud`): the minimap (`zone_map.xml`), the map locations and their script functions, the
  new-contact sound and the PDA indicators (`static_pda_online`, `snd_new_contact`), the HUD news lines
  (`maingame_pda_msg.xml`), the task title and `st_no_active_task` messages: `hud-scope` specs 8–12. The
  notification sounds are approved in `sound-notifications`.
- **Tutorials**: when a tutorial item's `pda_section` applies is `hud-scope` spec 18's; what each value
  does to the PDA is PD-1's.
- **Dialogue window** (scoped by issue #286): `give_talk_message`, the news it sends to the talk window
  (`news.xml` `talk_item`, believed), and `pda_dialog_character.xml`. The program names the constants
  `dialog_pda_msg`, `info_pda_msg`, `no_pda_msg`, `PDA_DIALOG_WND_BACK_BUTTON_CLICKED`,
  `PDA_DIALOG_WND_MESSAGE_BUTTON_CLICKED` (shipped API listing, c2 B6), believed a talk-through-the-PDA
  mode; PD-1 establishes whether the shipped PDA has such a mode at all and hands anything it finds to
  the dialogue window's specs.
- **Characters and relations** (`B-ai`, scoped by issue #279 and script API Q, #221): how goodwill,
  reputation and rank change (the `*_attack_*`, `*_fight_help_*`, `*_kill_*` keys, community goodwill),
  smart-terrain tasks (`*smart_terrain_task*`, `can_choose_alife_tasks`: characters' jobs, not the
  player's tasks).
- **Multiplayer** (`B-mp`; believed from names): `statisticwnd.xml` ("Weapon Efficiency", "JOIN"),
  `map_desc.xml`, `ui_mapinfo.xml`, `ui_map_description.xml`, `ui_statistics.xml`, the `mp_*`, `dm_rank*`,
  `team*_rank*`, `rank_*`, map rotation and `sv_*` names, `pda_taken`, `pdahunt`.
- **Unrelated words** (`B-other`): `contact_0` (a phantom's motions), `contact_hit`,
  `active_zone_contact`, the physics contact functions.
- **Unused** (`B-unused`): the files of 1.1's last paragraph.

## 4. Proposed specs

Each entry: functional description; data; builds on (approved); waits on (not yet specified); scripts;
order. Script functions are specified by `script-api-scope` P (#220, tasks, news, information portions,
map spots) and Q (#221, relations, community, ranking), named below as "API P" and "API Q". Section 6
lists every name each spec owns.

**PD-1 PDA window** (`pda-window`, behaviour). The window itself: when it can open (the three actions,
the conditions shared with the inventory screen, whether the PDA item in slot 7 is required), which tab
each action opens, switching tabs (buttons, hints, text colours), closing (the same action, Escape and
the menu key as SL-3 leaves it, death, another window opening), pause and the console while it is open,
the mouse cursor, the background (including whether `rus_pda` is ever chosen), the clock, the off
button, the new-content marks, what each tutorial `pda_section` value does (including `pda_closed`), and
the `ui_pda*` information portions the program gives the player on opening, closing and switching
(which of the 21, when). It also settles whether the shipped PDA has a talk mode (section 3).
Data: `pda.xml`, `ui_pda.xml`, the textures of 1.2; `ui_st_pda` tab strings; the 21 `ui_pda*` portions.
Builds on: `ui-layout`, `text-and-fonts`, `inventory-screen` 1, `console-key-bindings` 5,
`save-load-screens`, `player-state` 3.3, `console-window`, PD-2. Waits on: nothing for the window and
empty tabs; information portions (PD-8) for the `ui_pda*` portions; tutorials (`hud-scope` 18) for
`pda_section`. Scripts: none call the window; `xr_conditions` reads `ui_pda`. Order 2.

**PD-2 PDA controls** (`pda-controls`, format and behaviour). The window controls the tabs use that no
approved spec covers: the tab bar and filter tabs (buttons with text colours `t`, `d`, `e`, `h`), check
buttons (`check_mode`), hints (the `hint` attribute and `text_hint`), frame windows with alpha `a` and
`base_texture`, frame lines with a `title`, the tree list (`tree_root_font`, `tree_item_font`), scroll
views and their items, `auto_static` groups, the corner animations (`a_static`, `anim_static` with
`ui\ui_pda_anims_*`), `ui\ui_pda_over_list` (`hud-scope` spec 2 hands it on; believed a list
highlight). It builds on, and does not repeat, the frame and button looks of `save-load-screens` and
`inventory-screen` and the lists of `ui-layout` 9. Data: the generic elements of the 13 window files
(section 6). Builds on: `ui-layout`, `text-and-fonts`, `colour-animations`, `save-load-screens`,
`options-screens`. Waits on: nothing. Scripts: none (script windows' controls are API G2, #208). Order 1.

**PD-3 Level maps** (`level-maps`, format). `game_maps_single.ltx`: `[level_maps_single]`, `[global_map]`,
the level sections (`texture`, `bound_rect`, `global_rect`, `max_zoom`; the other keys `weathers`,
`music_tracks` belong to weather and music), the sections outside the list and the levels without a
picture; the `textures\map\` pictures and `ui\ui_global_map`; how a level position maps into a picture
and a level rectangle into the global map; whether `level.ltx` `[level_map]` is read. Data: 1.4.
Builds on: `ltx-config`, `ui-layout` 6.3, `level-loading`. Waits on: nothing. Scripts: none.
`hud-scope` spec 10 can build on it. Order 3.

**PD-4 PDA map** (`pda-map`, behaviour). The map tab and the tasks tab's map pane: the level map and the
global map, switching between them (`global_map_btn`, choosing a level on the global map), zoom in and
out up to `max_zoom`, centring on the player (`actor_btn`), moving the view, the player's mark and
heading, drawing each location's `level_map` spot and pointer with `level_map_spot_border`, hints over
spots (`text_hint`), `ui_map_area_anim`, the `notused_*` tool-bar buttons, `ui_icons_map.xml` (ids
`death`, `artefact`), and whether `visible_for_map` (a simulation-object property named by the binary)
limits what is drawn. Data: `pda_map.xml`, the map pane of `pda_events.xml`, PD-3's data, `map_spots.xml`
`level_map` halves. Builds on: PD-2, PD-3, `player-movement` (position, heading), `level-loading`.
Waits on: map locations (`hud-scope` spec 11, #178, itself waiting on API P) for anything but the
player's mark. Order 4.

**PD-5 Task definitions** (`game-tasks-format`, format). `game_tasks.xml` with its includes: the task
(`id`, `prio`, `title`) and objective elements (1.4) with their meanings and defaults, the icon forms
(texture id or file with rectangle), references to information portions, articles, story ids and
script functions, `user_task`, and what a reference to a missing id does. Data: the 12 task files and
their string tables. Builds on: `ui-layout` (XML, `#include`), `text-and-fonts`, `sav` 9.8 (the saved
fields). Waits on: nothing. Scripts: the task-file functions (API P). Order 5.

**PD-6 Tasks** (`game-tasks`, behaviour). The player's tasks: how a task is given (an information
portion's `task`, `give_task`), objective states (in progress, completed, failed) and how they are
decided (information portions, script functions, the order of objectives, objective 0 deciding the
task), what a state change does (`infoportion_set_*`, `function_call_*`, the `task_state` callback), task
priority, time limits (`caption_time`, `rem_time` show times; whether tasks expire), the active task and
objective (and `st_no_active_task`), objective map locations and hints (`set_map_hint`,
`set_map_location`; the location itself becomes a spot under `hud-scope` spec 11), and saving (`sav`
9.8). Data: PD-5. Builds on: PD-5, `sav`, `script-loading`, `script-environment`. Waits on: information
portions (PD-8), API P, story ids of level objects, saving and loading. Scripts: `give_task`,
`set_task_state`, objective setters (1.5); `task_manager`, `level_tasks`, `xr_effects`, the binder's task
callback. Order 9.

**PD-7 PDA tasks tab** (`pda-tasks`, behaviour). The tasks tab: the list of tasks and objectives with
their icons and state pictures (`ui_icons_PDA_subtask_*`, `ui\ui_icons_task`), the active/accomplished/
failed filter, `primary_or_all`, the description view with `objective_item`s and the task's article,
switching description and map (`switch_description_btn`, `ui_icons_newPDA_showmap`/`showtext`),
choosing the active objective, the time texts, and the elements the binary does not name
(`notused_*`, `location_btn`, `show_pointer_btn`, `primary_or_all`; c2 B5): whether any is shown. Data: `pda_events.xml`, `job_item.xml`. Builds on: PD-1, PD-2, PD-4,
PD-5. Waits on: PD-6. Order 10.

**PD-8 Information portions** (`info-portions`, format and behaviour). The 32 files of
`[info_portions]`: `info_portion` with `article`, `task`, `disable`, `dialog`, `action`; giving and
disabling (`give_info_portion`, `disable_info_portion`, dialogues), the known list and its times (`sav`
9 part 1), what a given portion's children cause (articles to PD-9, tasks to PD-6, disabled portions),
references to missing ids, and the `info_portion` of PDA and document items (`sav` 7). Data: 1.4.
Builds on: `sav`, `script-loading`. Waits on: API P (the giving calls), the dialogue window for
`dialog`. Scripts: `give_info_portion` (800 calls), `disable_info_portion`, `has_info`. Order 6.

**PD-9 Articles** (`encyclopedia-articles`, format and behaviour). The 8 files of `[encyclopedia]`:
`article` (`id`, `name`, `group` with its levels, `article_type`, `text`, `texture`, `ltx`), the type
values and the error message for a bad one, which tab each type goes to (encyclopedia, diary journal,
task description, `info`), the player's known articles (time, read flag, type; `sav` 9 part 4), how one
is received (information portions; `article_info` callback with its group and name) and marked read.
Data: 1.4. Builds on: `ui-layout`, `text-and-fonts`, `sav`. Waits on: PD-8. Scripts: the binder's
`article_callback` (`news_manager.send_encyclopedy`). Order 7.

**PD-10 PDA encyclopedia tab** (`pda-encyclopedia`, behaviour). The index tree by group, the article view
(header, text, picture), unread marks, and the `events_wnd` / `encyclopedia_wnd` item templates. Data:
`encyclopedia.xml`, `encyclopedia_item.xml`. Builds on: PD-1, PD-2, PD-9. Waits on: PD-9's receiving
(PD-8). Order 11.

**PD-11 News** (`game-news`, behaviour). The player's news list: what an item holds (type, text, time,
texture and rectangle; `sav` 9 part 5), how items are added (`give_game_news` with its show time and
delay, the scripts' tips, task and article news, `give_talk_message` as far as it adds to the list), its
length limit and order. The HUD lines (`hud-scope` spec 9) and the diary (PD-12) read it. Data: `news.xml`
`news_item`. Builds on: `sav`, `script-loading`. Waits on: API P. Scripts: `news_manager`, `mob_remark`,
`give_game_news`. Order 8.

**PD-12 PDA diary tab** (`pda-diary`, behaviour). The journal (the articles of the journal types, by
group in `src_list`) and the news history (PD-11 items with date and picture), the filter tab, the
new-content marks. Data: `events_new.xml`, `news.xml`. Builds on: PD-1, PD-2. Waits on: PD-9, PD-11.
Order 12.

**PD-13 PDA contacts tab** (`pda-contacts`, behaviour). The list of the PDA's contacts and the selected
contact's details (name, picture, community, reputation, relation to the player, rank), the online
mark, and how the list follows the contact list as contacts come and go. Data: `pda_contacts_new.xml`,
`pda_character.xml`. Builds on: PD-1, PD-2, `sound-notifications` 3 (the contact list),
`level-objects`. Waits on: PD-14; characters' profiles (names, pictures; AI scope #279). Order 13.

**PD-14 Character ratings shown** (`character-ratings`, format and behaviour). How a character's rank,
reputation, community and the relation to the player are turned into the texts and pictures the PDA
shows: `rating_names`, `reputation_names`, `goodwill_names`, the community names and indices, their
string ids, and the unknown cases. It does not cover how the values change (AI scope #279, API Q).
Data: `game_relations.ltx` (1.4). Builds on: `ltx-config`, `text-and-fonts`. Waits on: the values
(characters' profiles and relations, #279). Scripts: `character_rank`, `character_reputation` (API Q).
Order 14.

**PD-15 Ranking** (`pda-ranking`, behaviour). The ranked set (`add_to_ranking`, `remove_from_ranking`
from the binders), its order, the top 20, the player's own row, the ellipsis and empty rows, and the
selected character's details. Data: `stalkers_ranking.xml`, `stalkers_ranking_character.xml`. Builds on:
PD-1, PD-2, PD-14. Waits on: API Q, object binders (API L), characters (#279). Scripts: `bind_stalker`,
`se_stalker`, `xr_motivator`. Order 15.

**PD-16 Statistics** (`actor-statistics`, behaviour). What the program counts for the player (stalker
and monster kills, completed tasks, artefacts, relations to communities), the points of each detail and
section, `actor_stats` (`add_points`, `get_points`), and the saved record (`sav` 9.10). Data: the
section names of 1.3. Builds on: `sav`. Waits on: kills (characters and monsters, #279), PD-6,
artefacts, API Q. Order 16.

**PD-17 PDA statistics tab** (`pda-statistics`, behaviour). The master list and detail lists, the
captions of `stable_statistic_caption`, the communities list, `foo`, and the player's details panel.
Data: `actor_statistic.xml`. Builds on: PD-1, PD-2, PD-14. Waits on: PD-16. Order 17.

## 5. What can be built on approved specs alone

| Spec | Builds on approved specs alone | Waits on |
|---|---|---|
| PD-2 controls | yes | — |
| PD-1 window | yes (window, tabs empty); `ui_pda*` portions and `pda_section` wait | PD-8, `hud-scope` 18 |
| PD-3 level maps | yes | — |
| PD-4 map | the pictures, global map, zoom, the player's mark | locations: `hud-scope` 11 (#178), API P |
| PD-5 task definitions | yes (reading) | — |
| PD-8 information portions | reading the files | giving: API P, dialogues |
| PD-9 articles | reading the files | receiving: PD-8 |
| PD-14 ratings shown | the name tables | the values: #279, API Q |
| PD-13 contacts | the list (`sound-notifications` 3) | details: PD-14, #279 |
| PD-6, PD-7 tasks | — | PD-8, API P, saving |
| PD-10 encyclopedia, PD-12 diary | the views | PD-9, PD-11 |
| PD-11 news | — | API P |
| PD-15 ranking | — | API Q, API L, #279 |
| PD-16, PD-17 statistics | — | #279, PD-6, API Q |

Every PDA record is saved (`sav` 9); saving and loading them waits until the systems that hold them
exist (roadmap, saved games).

## 6. Unestablished

- Which tab each of `active_jobs`, `map`, `contacts` opens, and whether the PDA opens without the PDA
  item (PD-1).
- Whether the shipped PDA has a talk mode (section 3) and what `pda_dialog_character.xml` is used by.
- What sounds the window itself plays: none of its files names one (1.1); PD-1 establishes it.
- What the 12 `ui_pda*` portions the binary does not name are for (believed leftovers: 15 `task_<n>`
  scripts and `task_sim` name them; c5 P5).
- Whether `level.ltx` `[level_map]` and the `rus_pda` picture are ever read.

## 7. Coverage

Universe (c6 V2): 601 names: 280 strings of `xrGame.dll` with the PDA's words (pattern in `c6`), the 255
element names of the 13 window files (written `<element>@file`), 37 layout files (`file:`), the 21
`ui_pda*` information portions (`ip:`) and the 8 `pda_section` values (`sec:`). Each has exactly one
owner by the ordered rules of `c6` (V1); counts per owner (V2): PD-1 66, PD-2 146, PD-3 1, PD-4 24, PD-5 4,
PD-6 10, PD-7 63, PD-9 6, PD-10 12, PD-11 3, PD-12 8, PD-13 17, PD-14 24, PD-15 25, PD-16 3, PD-17 18,
B-ai 50, B-hud 20, B-mp 82, B-other 6, B-unused 13. PD-8 owns no name of this universe (its names,
`info_portion` and its children, are listed in 1.4). The full table is `checks/pda-scope/coverage_table.md`,
reproduced here:

| Owner | Count | Names |
|---|---|---|
| PD-1 | 66 | `<background_static>@pda`, `<main>@pda`, `<mbbackground_frame_line>@pda`, `<off_button>@pda`, `<old_section_static>@pda`, `<timer_frame_line>@pda`, `<updated_section_static>@pda`, `active_jobs`, `contacts`, `cse_alife_item_pda`, `device_pda`, `dialog_pda_msg`, `file:pda.xml`, `file:pda_dialog_character.xml`, `file:ui_pda.xml`, `info_pda_msg`, `inventory_pda`, `ip:ui_pda`, `ip:ui_pda_actor_info`, `ip:ui_pda_contacts`, `ip:ui_pda_contacts_hide`, `ip:ui_pda_contracts`, `ip:ui_pda_diary`, `ip:ui_pda_diary_hide`, `ip:ui_pda_encyclopedia`, `ip:ui_pda_encyclopedia_hide`, `ip:ui_pda_events`, `ip:ui_pda_hide`, `ip:ui_pda_jobs`, `ip:ui_pda_jobs_accomplished`, `ip:ui_pda_jobs_current`, `ip:ui_pda_jobs_failed`, `ip:ui_pda_jobs_hide`, `ip:ui_pda_map_global`, `ip:ui_pda_map_local`, `ip:ui_pda_news`, `ip:ui_pda_news_hide`, `ip:ui_pda_ranking`, `no_pda_msg`, `pda.xml`, `pda_contacts`, `pda_dialog_character.xml`, `pda_diary`, `pda_encyclopedia`, `pda_map`, `pda_quests`, `pda_ranking`, `pda_section`, `pda_statistics`, `sec:pda_closed`, `sec:pda_contacts`, `sec:pda_diary`, `sec:pda_encyclopedia`, `sec:pda_map`, `sec:pda_quests`, `sec:pda_ranking`, `sec:pda_statistics`, `ui_pda`, `ui_pda_actor_info`, `ui_pda_contacts`, `ui_pda_encyclopedia`, `ui_pda_events`, `ui_pda_hide`, `ui_pda_map_local`, `ui_pda_news_hide`, `ui_pda_ranking` |
| PD-2 | 146 | `<_button>@events_new`, `<a_static>@actor_statistic`, `<a_static>@encyclopedia`, `<a_static>@pda_contacts_new`, `<a_static>@stalkers_ranking`, `<anim_static>@events_new`, `<anim_static>@pda_events`, `<auto>@actor_statistic`, `<auto>@stalkers_ranking`, `<auto_static>@actor_statistic`, `<auto_static>@events_new`, `<auto_static>@pda_character`, `<auto_static>@pda_events`, `<auto_static>@stalkers_ranking`, `<button>@events_new`, `<button>@pda`, `<button>@pda_events`, `<chicon_frame_line>@actor_statistic`, `<chicon_frame_line>@stalkers_ranking`, `<chicon_frame_window>@actor_statistic`, `<chicon_frame_window>@stalkers_ranking`, `<d>@events_new`, `<d>@job_item`, `<d>@pda`, `<d>@pda_events`, `<e>@events_new`, `<e>@job_item`, `<e>@pda`, `<e>@pda_events`, `<filter_tab>@events_new`, `<filter_tab>@pda_events`, `<font>@actor_statistic`, `<font>@encyclopedia`, `<font>@events_new`, `<font>@news`, `<font>@stalkers_ranking_character`, `<h>@events_new`, `<h>@job_item`, `<h>@pda`, `<h>@pda_events`, `<header_frame_line>@pda_events`, `<image>@encyclopedia_item`, `<image>@job_item`, `<image>@news`, `<image>@pda_events`, `<info_frame_line>@actor_statistic`, `<info_frame_line>@stalkers_ranking`, `<info_frame_window>@actor_statistic`, `<info_frame_window>@stalkers_ranking`, `<left_auto>@stalkers_ranking`, `<left_auto_static>@actor_statistic`, `<left_auto_static>@encyclopedia`, `<left_auto_static>@pda_contacts_new`, `<left_frame>@events_new`, `<left_frame>@pda_events`, `<left_frame_header>@events_new`, `<left_frame_header>@pda_events`, `<left_frame_line>@encyclopedia`, `<left_frame_line>@pda_contacts_new`, `<left_frame_window>@encyclopedia`, `<left_frame_window>@pda_contacts_new`, `<list>@actor_statistic`, `<list>@news`, `<list>@pda_contacts_new`, `<list>@pda_events`, `<list>@stalkers_ranking`, `<main_frame>@pda_events`, `<main_map_frame>@pda_events`, `<main_map_frame>@pda_map`, `<main_wnd>@actor_statistic`, `<main_wnd>@encyclopedia`, `<main_wnd>@events_new`, `<main_wnd>@pda_contacts_new`, `<main_wnd>@pda_events`, `<main_wnd>@pda_map`, `<main_wnd>@stalkers_ranking`, `<map_header_frame_line>@pda_events`, `<map_header_frame_line>@pda_map`, `<mask_frame_window>@actor_statistic`, `<mask_frame_window>@encyclopedia`, `<right_auto>@stalkers_ranking`, `<right_auto_static>@actor_statistic`, `<right_auto_static>@encyclopedia`, `<right_auto_static>@pda_contacts_new`, `<right_frame>@events_new`, `<right_frame>@pda_events`, `<right_frame_header>@events_new`, `<right_frame_line>@encyclopedia`, `<right_frame_line>@pda_contacts_new`, `<right_frame_window>@encyclopedia`, `<right_frame_window>@pda_contacts_new`, `<scroll_view>@events_new`, `<scroll_view>@pda_events`, `<t>@events_new`, `<t>@job_item`, `<t>@pda`, `<t>@pda_events`, `<tab>@pda`, `<text>@actor_statistic`, `<text>@encyclopedia`, `<text>@encyclopedia_item`, `<text>@events_new`, `<text>@job_item`, `<text>@news`, `<text>@pda`, `<text>@pda_character`, `<text>@pda_contacts_new`, `<text>@pda_events`, `<text>@pda_map`, `<text>@stalkers_ranking`, `<text>@stalkers_ranking_character`, `<text_color>@events_new`, `<text_color>@job_item`, `<text_color>@pda`, `<text_color>@pda_events`, `<text_color_>@pda_events`, `<text_colors>@job_item`, `<text_cont>@encyclopedia_item`, `<text_cont>@news`, `<text_cont>@pda_events`, `<texture>@actor_statistic`, `<texture>@encyclopedia`, `<texture>@events_new`, `<texture>@job_item`, `<texture>@pda`, `<texture>@pda_character`, `<texture>@pda_contacts_new`, `<texture>@pda_events`, `<texture>@pda_map`, `<texture>@stalkers_ranking`, `<texture_>@job_item`, `<title>@actor_statistic`, `<title>@encyclopedia`, `<title>@pda`, `<title>@pda_contacts_new`, `<title>@pda_events`, `<title>@pda_map`, `<title>@stalkers_ranking`, `<tree_item_font>@encyclopedia`, `<tree_item_font>@events_new`, `<tree_root_font>@encyclopedia`, `<tree_root_font>@events_new`, `<window_name>@pda_events`, `<window_name>@pda_map`, `<work_area>@events_new`, `ui\ui_pda_over_list` |
| PD-3 | 1 | `level_maps_single` |
| PD-4 | 24 | `:global_map_btn`, `:level_map`, `:main_wnd:main_map_frame`, `:main_wnd:main_map_frame:level_frame`, `:main_wnd:map_header_frame_line`, `:main_wnd:map_header_frame_line:tool_bar`, `<actor_btn>@pda_map`, `<global_map_btn>@pda_map`, `<level_frame>@pda_map`, `<map_wnd>@pda_map`, `<notused_add_spot_btn>@pda_map`, `<text_hint>@pda_map`, `<tool_bar>@pda_map`, `<zoom_in_btn>@pda_map`, `<zoom_out_btn>@pda_map`, `file:pda_map.xml`, `file:ui_icons_map.xml`, `global_map`, `level_map_spot_border`, `main_wnd:right_frame:map_wnd`, `map_wnd`, `pda_map.xml`, `ui_map_area_anim`, `visible_for_map` |
| PD-5 | 4 | `game_task`, `game_tasks.xml`, `map_location_hidden`, `map_location_type` |
| PD-6 | 10 | `get_task_state`, `give_task`, `set_article_id`, `set_article_key`, `set_map_hint`, `set_map_location`, `set_task_state`, `task`, `task_dummy`, `task_state` |
| PD-7 | 63 | `<accomplished>@job_item`, `<active>@job_item`, `<active_objecttive_image>@job_item`, `<actor_btn>@pda_events`, `<background>@job_item`, `<cancel_btn>@job_item`, `<caption>@job_item`, `<caption_time>@job_item`, `<description>@job_item`, `<edit_caption>@job_item`, `<edit_description>@job_item`, `<edit_text_btn>@job_item`, `<failed>@job_item`, `<global_map_btn>@pda_events`, `<level_frame>@pda_events`, `<location_btn>@job_item`, `<map_wnd>@pda_events`, `<notused_add_spot_btn>@pda_events`, `<notused_button>@pda_events`, `<notused_edit_user_item>@job_item`, `<notused_highlight_spot_btn>@pda_events`, `<notused_remove_spot_btn>@pda_events`, `<notused_task_user_item>@job_item`, `<objective_item>@pda_events`, `<ok_btn>@job_item`, `<primary_or_all>@pda_events`, `<rem_time>@job_item`, `<remove_btn>@job_item`, `<show_descr_btn>@job_item`, `<show_pointer_btn>@job_item`, `<state_image>@job_item`, `<switch_description_btn>@job_item`, `<task_descr_view>@pda_events`, `<task_root_item>@job_item`, `<task_sub_item>@job_item`, `<text_hint>@pda_events`, `<tool_bar>@pda_events`, `<zoom_in_btn>@pda_events`, `<zoom_out_btn>@pda_events`, `file:job_item.xml`, `file:pda_events.xml`, `main_wnd:right_frame:task_descr_view`, `pda_events.xml`, `task_root_item`, `task_root_item:caption`, `task_root_item:caption_time`, `task_root_item:image`, `task_root_item:rem_time`, `task_root_item:switch_description_btn`, `task_sub_item`, `task_sub_item:active_objecttive_image`, `task_sub_item:description`, `task_sub_item:description:text_colors:accomplished`, `task_sub_item:description:text_colors:active`, `task_sub_item:description:text_colors:failed`, `task_sub_item:show_descr_btn`, `task_sub_item:state_image`, `ui\ui_icons_task`, `ui_icons_PDA_subtask_accomplished`, `ui_icons_PDA_subtask_active`, `ui_icons_PDA_subtask_failed`, `ui_icons_newPDA_showmap`, `ui_icons_newPDA_showtext` |
| PD-9 | 6 | `article`, `article_disable`, `article_info`, `article_type`, `encyclopedia`, `journal` |
| PD-10 | 12 | `<article_header_static>@encyclopedia`, `<encyclopedia_wnd>@encyclopedia_item`, `<events_wnd>@encyclopedia_item`, `<idx_list>@encyclopedia`, `<info_list>@encyclopedia`, `<objective_item>@encyclopedia_item`, `article_header_static`, `encyclopedia.xml`, `encyclopedia_item.xml`, `encyclopedia_wnd:objective_item`, `file:encyclopedia.xml`, `file:encyclopedia_item.xml` |
| PD-11 | 3 | `give_game_news`, `news.xml`, `news_item` |
| PD-12 | 8 | `<date_text_cont>@news`, `<news_item>@news`, `<old_section_static>@events_new`, `<src_list>@events_new`, `<talk_item>@news`, `<updated_section_static>@events_new`, `file:events_new.xml`, `file:news.xml` |
| PD-13 | 17 | `<community_caption>@pda_character`, `<community_static>@pda_character`, `<detail_list>@pda_contacts_new`, `<icon_static>@pda_character`, `<name_static>@pda_character`, `<pda_char_auto_statics>@pda_character`, `<rank_caption>@pda_character`, `<rank_static>@pda_character`, `<relation_caption>@pda_character`, `<relation_static>@pda_character`, `<reputation_caption>@pda_character`, `<reputation_static>@pda_character`, `file:pda_character.xml`, `file:pda_contacts_new.xml`, `pda_char_auto_statics`, `pda_character.xml`, `pda_contacts_new.xml` |
| PD-14 | 24 | `character_community`, `character_rank`, `character_reputation`, `communities_relations`, `community`, `community_caption`, `community_static`, `game_relations`, `goodwill`, `goodwill_names`, `monster_relations`, `rank`, `rank_caption`, `rank_name`, `rank_relations`, `rank_static`, `relation`, `relation_caption`, `relation_static`, `reputation`, `reputation_caption`, `reputation_names`, `reputation_relations`, `reputation_static` |
| PD-15 | 25 | `<_biography_list>@stalkers_ranking_character`, `<character_info>@stalkers_ranking`, `<community_caption>@stalkers_ranking_character`, `<community_static>@stalkers_ranking_character`, `<icon_static>@stalkers_ranking_character`, `<item_actor>@stalkers_ranking`, `<item_ellipsis>@stalkers_ranking`, `<item_human>@stalkers_ranking`, `<no_items>@stalkers_ranking`, `<rank_caption>@stalkers_ranking_character`, `<rank_static>@stalkers_ranking_character`, `<relation_caption>@stalkers_ranking_character`, `<relation_static>@stalkers_ranking_character`, `<stalkers_list>@stalkers_ranking`, `<text_1>@stalkers_ranking`, `<text_2>@stalkers_ranking`, `<text_3>@stalkers_ranking`, `add_to_ranking`, `file:stalkers_ranking.xml`, `file:stalkers_ranking_character.xml`, `get_actor_ranking`, `remove_from_ranking`, `stalkers_list`, `stalkers_ranking.xml`, `stalkers_ranking_character.xml` |
| PD-16 | 3 | `actor_stats`, `relation_communities_list`, `st_detail_list_for_community_relations` |
| PD-17 | 18 | `<_relation_communities_list>@actor_statistic`, `<actor_stats_wnd>@actor_statistic`, `<article_header_static>@actor_statistic`, `<character_info>@actor_statistic`, `<detail_list>@actor_statistic`, `<detail_part_5>@actor_statistic`, `<detail_part_def>@actor_statistic`, `<master_list>@actor_statistic`, `<master_part>@actor_statistic`, `<r>@actor_statistic`, `<text_0>@actor_statistic`, `<text_1>@actor_statistic`, `<text_2>@actor_statistic`, `<text_3>@actor_statistic`, `actor_statistic.xml`, `actor_stats_wnd`, `file:actor_statistic.xml`, `unknown_community` |
| B-ai | 50 | `action_accomplish_task`, `action_reach_task_location`, `action_smart_terrain_task`, `can_choose_alife_tasks`, `change_character_reputation`, `change_community_goodwill`, `change_goodwill`, `community_goodwill`, `community_goodwill_limits`, `community_member_kill_goodwill`, `enemy_attack_goodwill`, `enemy_attack_reputation`, `enemy_fight_help_goodwill`, `enemy_fight_help_reputation`, `enemy_kill_goodwill`, `enemy_kill_reputation`, `friend_attack_goodwill`, `friend_attack_reputation`, `friend_fight_help_goodwill`, `friend_fight_help_reputation`, `friend_kill_goodwill`, `friend_kill_reputation`, `general_goodwill`, `get_actor_relation_flags`, `get_rank`, `goodwill_enemy`, `goodwill_friend`, `goodwill_neutal`, `neutral_attack_goodwill`, `neutral_attack_reputation`, `neutral_fight_help_goodwill`, `neutral_fight_help_reputation`, `neutral_kill_goodwill`, `neutral_kill_reputation`, `personal_goodwill_limits`, `property_smart_terrain_task`, `relation_attack`, `relation_fight_help_human`, `relation_fight_help_monster`, `relation_kill`, `relation_registry`, `set_actor_relation_flags`, `set_character_community`, `set_character_rank`, `set_community_goodwill`, `set_goodwill`, `set_rank`, `set_relation`, `smart_terrain_task_activate`, `smart_terrain_task_deactivate` |
| B-hud | 20 | `:mini_map`, `file:maingame_pda_msg.xml`, `file:map_spots.xml`, `file:zone_map.xml`, `main_task`, `maingame_pda_msg.xml`, `map_add_object_spot`, `map_add_object_spot_ser`, `map_change_spot_hint`, `map_has_object_spot`, `map_location_added`, `map_remove_object_spot`, `map_spots.xml`, `mini_map_spot_border`, `snd_new_contact`, `sound_contact`, `st_no_active_task`, `static_pda_online`, `zone_map.xml`, `zone_map_location_%d` |
| B-mp | 82 | `change_map`, `change_map:background`, `change_map:btn_cancel`, `change_map:btn_ok`, `change_map:frame`, `change_map:header`, `change_map:list`, `change_map:list_back`, `change_map:map_frame`, `change_map:map_pic`, `current_map_name`, `current_map_name_internal`, `disabled_maps`, `dm_rank1`, `dm_rank2`, `dm_rank3`, `dm_rank4`, `file:map_desc.xml`, `file:statisticwnd.xml`, `file:ui_map_description.xml`, `file:ui_mapinfo.xml`, `file:ui_statistics.xml`, `intro\intro_map_pic_`, `item_color_restr_rank`, `level_maps_mp`, `map_desc`, `map_desc.xml`, `map_desc:background`, `map_desc:btn_next`, `map_desc:btn_spectator`, `map_desc:caption`, `map_desc:image`, `map_desc:image_frames_1`, `map_desc:image_frames_2`, `map_desc:image_frames_3`, `map_desc:map_info`, `map_desc:text_desc`, `map_info`, `map_name`, `map_rotation`, `map_usage`, `mp\map_list.ltx`, `mp_change_map`, `mp_si_maprotation`, `mp_your_rank`, `new_rank`, `pda_taken`, `pdahunt`, `progress_back_rank`, `progress_bar_rank`, `rank:pic`, `rank_`, `rank_%d`, `rank_aquire_money`, `rank_base`, `rank_diff_exp_bonus`, `rank_exp`, `rank_kill_points`, `rank_wnd`, `rank_wnd:background`, `rank_wnd:rank_%d`, `ranks_properties`, `static_item_rank`, `static_player_rank`, `statisticwnd.xml`, `sv_dump_online_statistics`, `sv_dump_online_statistics_period`, `sv_pda_hunt`, `sv_statistic_collect`, `sv_statistic_save`, `team1_rank1`, `team1_rank2`, `team1_rank3`, `team1_rank4`, `team2_rank1`, `team2_rank2`, `team2_rank3`, `team2_rank4`, `text\map_desc\`, `ui\ui_mp_icon_rank`, `ui_inv_rank_restr`, `ui_mapinfo.xml` |
| B-other | 6 | `active_zone_contact`, `contact_0`, `contact_hit`, `dJointCreateContact`, `dJointCreateContactSpecial`, `lua_newstate` |
| B-unused | 13 | `file:actor_diary.xml`, `file:contracts.xml`, `file:contracts_character.xml`, `file:global_map.xml`, `file:jobs.xml`, `file:jobs_new.xml`, `file:map.xml`, `file:map_character.xml`, `file:news_templates.xml`, `file:pda_character_new.xml`, `file:pda_contacts.xml`, `file:pda_dialog.xml`, `file:pda_new.xml` |

## Provenance

- Date: 2026-11-26. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives
  `gamedata.db0` … `gamedata.dbd`; binaries `bin\xrGame.dll`, `bin\XR_3DA.exe`.
- Data examined: winning copies of `config\`, `scripts\`, every level's `level.ltx`, `textures\map\` and
  the PDA's `textures\ui\` pictures, extracted with the project's archive tool (`checks/pda-scope/extract.py`);
  the archive listing; byte comparison of the three `ui_pda.dds` copies.
- Analysis: searches of the layout, configuration, task, article, information-portion and string-table
  files and the scripts; printable-string scans of the binaries, and of the binary's run of action names.
  No disassembly, debugging or execution.
- Checks: `checks/pda-scope/c1_layout.py` … `c6_coverage.py`, run from that directory with `python3 -I`
  (`run_all.py` runs them all), with `lib.py`, `tree.py`, `ltx.py`, `xmlx.py` (helpers); all pass.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- `xrGame.dll` has the message "Error. Path from actor to selected map spot does not contain level changer :(": the program routes from the player to a selected spot on another level through level changers (`map_spots_lchangers.xml` has their spots). No proposed spec covers how a spot on another level is shown or pointed to, or this route; PD-4 should, coordinated with `hud-scope` spec 11.
- The player's binder sets the callback `inventory_info` (number 11 in the shipped API listing; named in `xrGame.dll`), whose handler runs `level_tasks.proceed` and `level_tasks.process_info_portion` on every information portion received. Neither 1.5 nor PD-8 names it; PD-8 should cover when it is called and with what arguments.
- The callback `inventory_pda` (number 10; named in `xrGame.dll`; set by no shipped script) is in PD-1's coverage list, but no spec text says what it reports or when; PD-1 should establish it.
- The constant `PDA_CONTACTS_WND_CONTACT_SELECTED` (51 in the shipped API listing; named in `xrGame.dll`) is checked (c2 B6) but given to no spec; PD-13 should establish whether selecting a contact produces it.
- PD-8's scripts omit `dont_has_info` (named in `xrGame.dll`), and the test scripts' `_g.GiveInfoViaPda`, which calls `give_info_portion_via_pda`, a name `xrGame.dll` does not have; PD-8 should say what each does or that the second fails.
- `hud-scope` spec 2 leaves to visibility whether the action `scores` (Tab) shows `actor_statistic.xml` (believed PDA there); section 3 and PD-17 do not state this boundary, so it is unclear which spec decides whether the statistics view opens on `scores`.
