# The trader's motions and the crows (scoping specification)

Scoping specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the area of issues
#551 and #559: the motions of the trader `escape_trader` in the bunker at the start of a New Game on the
first level (`levels\l01_escape\`), and the crows (records of class id `AI_CROW`) of every level, with
their flight, motions, sounds and death. Section 1 states what produces each in the original; section 2
what the approved specifications already cover; section 3 proposes the split into small specs, one
behaviour each, with their order and size; section 4 gives the smallest correct step for each issue. It
states no behaviour in full detail: the specs of section 3 do. What could not be established is marked
**unestablished**; what is inferred rather than read from the data or the program is marked **believed**.

Builds on (not repeated): `docs/formats/sav.md` (record layouts), `docs/behavior/level-objects.md`
(presence at the start, transform, models, skinning at rest), `docs/formats/skeletal-motions.md` (motion
files, motion names, partitions, the pose at a time *t*), `docs/behavior/offline-simulation.md`,
`docs/behavior/object-visibility.md`, `docs/behavior/script-loading.md`,
`docs/behavior/script-environment.md`, `docs/behavior/script-api-scope.md`,
`docs/behavior/level-sound-scope.md`, `docs/behavior/narrowed-specs-scope.md`.

"Scripts" are the shipped Lua scripts (`scripts\*.script`); "logic file" is an LTX file that a record's
custom data names with `cfg =`; both are data. Motion lengths are *N* / 30 s (skeletal-motions 5.1). Every
fact carries the name of its check, a function of `checks.py` in `checks/trader-and-crows-scope/` of the
dirty workspace (`run_all.py` runs them all).

## 1. What produces each in the original

### 1.1 The trader

**The record.** `escape_trader` is object 84 of `spawns\all.spawn` on `l01_escape`, section `m_trader`,
class id `AI_TRD_S`; its custom data has `[logic]` with `cfg = scripts\esc_trader.ltx` [T1, T1b]. The
section names `script_binding = bind_monster.bind` and the visual `actors\trader\trader` [T1c], whose
motion file `actors\trader_animation` has 65 motions and two partitions: `torso` (45 bones, including
`bip01_head`) and `head` (10 bones: `bip01_ponytail1`, `bip01_ponytail2`, `bone01`, `bone02`, `bone04`,
`bone06`, `bone08`, `bone10`, `bone12`, `bone14`) [T1d, T1e]. All 65 motions have flags 2; 34 play on all
partitions, 22 on `torso`, 9 on `head` [T5d].

**No AI, no smart cover, no fixed motion.** The trader has no movement or decision-making of its own: a
script scheme chooses each motion by name, and the program plays it. The words "smart cover" occur in no
file of the game data and not in `bin\xrGame.dll` [T4f]. The trader's spawn starts no motion [T8c]; until
a script sets one he is believed to stay at the bind pose (level-objects 4.8), which is what our engine
shows.

**The program's side** (static analysis of `bin\xrGame.dll`). The trader's script object has
`set_trader_global_anim`, `set_trader_head_anim`, `set_trader_sound`, `external_sound_start` and
`external_sound_stop` [T7]. The trader has two motion channels: a **global** and a **head** channel; each
plays the cycle of the given motion name and is told when it ends [T7b]. `set_trader_sound` stops the
sound playing, starts the given head motion and plays the given sound [T7c]. In the trader's
per-frame update (skipped under a condition that is **unestablished**) the program raises the object
events 32 (`trader_sound_end`, the sound has ended), 30 (`trader_global_anim_request`, the global motion
has ended) and 31 (`trader_head_anim_request`, the head motion has ended while a sound plays) [T7c, T7d,
T7e]. At spawn the program also installs a callback on the bone `bip01_head` that turns it horizontally
toward an object of the level, believed to be the player [T8, T8b].

**The scheme.** `scripts\bind_monster.script` sets up the record's logic when the trader is spawned and
passes every update to the active scheme [T2]; `scripts\modules.script` loads the schemes `mob_trader`
and `mob_trade` [T2b]. `scripts\mob_trader.script` [T3, T3b, T3c]:

- holds 21 named sets of global motions and 4 of head motions, naming 39 of the 65 motions (all exist);
  a section's `anim_global` and `anim_head` name a set, and a motion is picked from the set uniformly at
  random;
- on entering a section it starts a global motion of the section's set and, if the section has
  `sound_phrase`, the sound (picked from a sound set of `scripts\mob_sound.script`) with a head motion;
- when the global motion ends (event 30) it sets the signal `animation_end` and, unless the section's
  `on_signal` waits for `animation_end`, starts another motion of the same set; when the head motion ends
  (event 31) it starts another head motion; when the sound ends (event 32) it sets the signal
  `sound_phrase_end`.

Section changes follow `scripts\xr_logic.script`: `on_timer` compares the game's `time_global`
(milliseconds) with the time the section was entered, `on_signal` waits for a signal of the scheme, the
`on_actor_dist_…_nvis` keys compare the distance to the player [T6].

**The logic file** `config\scripts\esc_trader.ltx` has 33 scheme sections using only the keys
`anim_global`, `anim_head`, `sound_phrase`, `on_signal`, `on_timer`, `on_info` (… `on_info3`),
`on_actor_dist_ge_nvis`, `on_actor_dist_ge_nvis2`, `on_actor_dist_le_nvis`, `can_talk`, `tip_text`,
`tip` [T4c]. Its first section is `mob_trader@new_idle1` if the player has the information portion
`esc_trader_hello_played`, else `mob_trader@new_start_wait` [T4]; that portion is defined in
`config\gameplay\info_l01escape.xml` and given only by this file and a dialog of
`config\gameplay\dialogs_escape.xml` [T4d], so a New Game starts with the **start chain** [T4b, T5, T3c,
T3e]:

| Section | Global set (motions, length) | Head, sound | Leaves on |
|---|---|---|---|
| `new_start_wait` | `chair_hack_idle` (`idle_2` 8.000 s, `idle_2_def_0` 3.333 s) | — | 5,000 ms |
| `new_start_intro1` | `chair_hack_to_desk` (`spinka_idle` 1.667 s) | — | end of the motion; gives `esc_trader_hello_played` |
| `new_start_intro2` | `first_phrase` (`vstuplenie` 23.367 s, partition `torso`) | `vstuplenie_head` (23.367 s, partition `head`); `trader_monolog1` = `characters_voice\scenario\trader\trader_monolog1` | end of the motion |
| `new_start_intro3` | `desk_idle` (`idle` 3.333, `idle_1` 10.000, `palci` 1.667, `zatylok` 3.333 s) | — | 5,000 ms, or talking |
| `new_start_intro4` | `desk_talk` (`idle`, `idle_1`) | head set `normal`; `trader_come_here2` = `…\trader1b` | end of the sound, or distance |

after which the trader moves between five idle states (the file's comments: 1 "at the desk", 2
"diagonal", 3 "leaning back", 4 "at the computer", 5 "reading") by random transitions at the end of each
motion, and to talking, trading, greeting and farewell sections by dialog, trade and distance [T4, T4e].
The start chain uses motions on all partitions except `vstuplenie` and `vstuplenie_head`, which play
together on the two partitions [T5b].

**At the fidelity view `bunker-start`** (30 s after play starts, the player not moving): the chain
reaches the end of `vstuplenie` 5 + 1.667 + 23.367 = 30.033 s after the logic starts [T5c], so the
trader is believed to be at the end of his first phrase or at the start of `desk_idle`, both at the desk
(the state the file calls "at the desk"), as the original shows. When the logic starts relative to the
view's "play starts" is **unestablished**; the motions picked at random are not reproducible frame for
frame (section 5).

### 1.2 The crows

**The records.** 197 records of class id `AI_CROW` on 5 levels: `l04_darkvalley` 61, `l01_escape` 55,
`l07_military` 37, `l02_garbage` 27, `l05_bar` 17; all of section `m_crow`, with no custom data and the
visual `monsters\crow\crow` [C1, C1b]. On `l01_escape` all have state flags 0xFFFFFFBA and health 1, at
heights −17.43 … 18.34 m [C1c]; the nearest is 59.0 m from the eye of the fidelity view `outdoor`
(−180, 5, −120) and 36.7 m from the player's record [C1d]. All 55 are online at the start
(level-objects 2.6) and never move offline (offline-simulation 5.1).

**The configuration** `config\creatures\m_crow.ltx` `[m_crow]`: `speed = 6.0`, `angular_speed = 0.1`,
`goal_change_delta = 5.0`, `min_height = 35`, `goal_variability = 10.0, 15.0, 200.0`,
`idle_sound_delta = 400.f`, and no `script_binding` [C2]; the program reads these six keys [C4, C4b].
Every script line that names the crow's class id is a kind test (`otype == clsid.crow`) [C2b, C2c]: crows
are driven by the program alone, with no scripts and no AI beyond what follows.

**The model** `meshes\monsters\crow\crow.ogf`: 16 bones, one partition, 12 embedded motions [C3]: `norm_idle`
(2 keys), `idle` (1), `norm_death_0` … `_2` (0.700 s each), `norm_death_dead_0` … `_2` (0.433 s),
`norm_death_idle` (0.200 s), `norm_fly_fwd` (0.800 s), `hit` (an fx motion, 0.233 s), `norm_idle_0`
(4.667 s) [C3b, C3c].

**Motion groups.** At spawn the program collects five groups by name: for each of `norm_idle`,
`norm_fly_fwd`, `norm_death`, `norm_death_idle`, `norm_death_dead`, the motion of that name and those named
*name*`_0` … *name*`_7` that exist, at most 8; an empty group is fatal [C5, C5b]. In the data: `norm_idle`
and `norm_idle_0`; `norm_fly_fwd`; `norm_death_0` … `_2`; `norm_death_idle`; `norm_death_dead_0` … `_2`
[C5c]. The motions `idle` and `hit` belong to no group.

**Spawn and states.** A spawned crow is visible and enabled, with its per-frame update switched off [C5d];
it starts with no state and the target state **glide** [C7d]. The scheduled update starts the new state's
group whenever the state changes, playing a random motion of the group [C7, C7b]: **glide** plays the
`norm_idle` group, **flap** the `norm_fly_fwd` group, **death** the `norm_death` group (with an end
notification, after which the `norm_death_idle` group plays), **dead** the `norm_death_dead` group [C7,
C7b, C10]. Glide becomes flap when the crow's height has risen since its previous flight step, and flap
becomes glide when it has not [C7c].

**Flight.** The crow flies toward a **goal**. When its goal timer runs out, the timer gains
`goal_change_delta` × (0.5 … 1.5) s and the goal becomes the camera's eye position plus, per axis, the
corresponding `goal_variability` component × (−0.5 … 0.5), with `min_height` added on the vertical [C8,
C8b]; the eye position is the same program value around which the weather's random sounds are placed
(sound-weather-ambient.md, the camera's eye *E*) [C8c]. Each flight step turns the crow's pitch and heading (believed reading of the two angles) toward the goal at
rates scaled by `angular_speed` (pitch limited to ±0.8, raised when the goal is more than 1 m above, lowered
when more than 1 m below, else decaying by 0.95 per step) and moves it by `speed` × the step's time along
its heading [C9, C9b]. Flight steps happen at most once per frame: when the crow is drawn, or in its
scheduled update when it was not drawn in the previous frame [C9c]. So every crow of the level heads for
the camera's surroundings; with the shipped values the goals lie within ±5 m, 35 ± 7.5 m above and ±100 m
of the camera's position along the three world axes (believed reading of the axes).

**Sounds.** The idle sounds are `monsters\crow\idle` and `idle_0` … `idle_7` where they exist (in the data
`sounds\monsters\crow\idle_0.ogg` … `idle_2.ogg`) [C6, C6b]; while flying, a timer of
`idle_sound_delta` × (0.5 … 1.5) s plays a random one at the crow's position [C8].

**Death.** A dead crow gets a physics shell [C10b]; it falls in the death state and enters the dead state
once its vertical speed exceeds −0.001 [C10]. What kills a crow (hits) is outside this area.

**At the fidelity view `outdoor`** (210 s after play starts, a camera track's eye): the two crows the
original shows are believed to be crows that flew from their records toward goals around the track's eye.
Which crows, and where, depends on random numbers whose source and seeding are **unestablished**; a frame
cannot be matched crow for crow.

## 2. What the approved specs cover

| Need | Approved coverage | Missing |
|---|---|---|
| Presence at the start | level-objects 2: the trader and 55 crows online [A1] | — |
| Drawing at rest | level-objects 3 – 5 (transform, skinned models, shading); light-shadows-projection: crows cast no shadow [A1h] | — |
| Record layouts | sav.md 7.4: crow (Creature block), trader (Object, Visual, Trader-like) [A1d] | — |
| A motion's pose at time *t* | skeletal-motions 1 – 5, with the trader's `idle` and the crow's `norm_fly_fwd` as worked examples [A1b] | playback: how *t* advances, speed, a motion's end and the flags, blending (accrue, falloff, power), several motions on partitions, fx motions, end notifications (its section 7) |
| Offline behaviour | offline-simulation 5.1: crows never move offline [A1c] | — |
| Whether a crow is drawn this frame | object-visibility (the camera pass's dynamic items) [A1h] | — |
| Scripts | script-loading, script-environment [A1h]; script-api-scope proposals R (`set_trader_*`), O (the callbacks), T (`external_sound_*`), L (binders) [T9, A1f] | the trader's channels; the logic schemes (`xr_logic`, `mob_trader`) |
| Crow sounds | level-sound-scope S24 lists `[m_crow]` and `monsters\crow\idle` as its own [A1e] | their timing (1.2) |
| First-frame motion of objects without AI | narrowed-specs-scope LO-2 (lamps, Skeleton block), not characters or crows [A1e] | — |

No approved spec covers motion playback, the trader, crows or the logic schemes [A1g].

## 3. Proposed specs

| # | Title (file) | Covers (data and observable behaviour) | Depends on | Needs scripts | Size |
|---|---|---|---|---|---|
| TC1 | Motion playback (`motion-playback.md`) | playing a cycle on all partitions or one; advancing *t* with the motion's speed; what flags bits 1 – 3 do at a motion's end; end notifications; blending a new cycle in and the old out (accrue, falloff, power); cycles of several channels on one model; fx motions; verified on the trader's and the crow's motions | skeletal-motions | no | L; splits, if one review round cannot check it, into cycles (time, end, flags, partitions, notifications) and blending with fx motions |
| TC2 | The trader's motion channels (`trader-motion-channels.md`) | the global and head channels, events 30 – 32 and when each is raised, `set_trader_sound` and the external sound, the update's skip condition, the head turning toward the player, what shows before a motion is set | TC1; sound-positional.md; script-api R, O, T for the names | no (the channels are the program's) | S |
| TC3 | The trader's logic without dialog (`trader-logic.md`) | the `mob_trader` scheme and every section of `esc_trader.ltx` reachable without dialog or trade: the start chain, the five idle states and their transitions, waiting, greeting and farewell by distance; the condition kinds, condition lists (information portions, random `{~n}`), effects (`%+…%`), random motion choice, the sound sets of `mob_sound.script` it names | TC2; script-environment; `xr_logic.script`, `mob_trader.script`, `mob_sound.script` | yes | M |
| TC4 | The trader's logic in dialog and trade (`trader-talk-trade.md`) | the sections entered by talking and trading (`on_info` with `talking`, `mob_trade@new` and its three results), `can_talk`, `tip_text`, `tip` | TC3; specs of dialogs and trade (none approved; script-api R) | yes | S |
| TC5 | Crow flight (`crow-flight.md`) | 1.2 in full except death and sounds: spawn, states glide and flap, motion groups, goal choice, steering, speed, the cadence of flight steps and the scheduled update's interval, the random numbers | TC1 (cycles); level-objects; object-visibility; view-projection.md (the camera position) | no | M |
| TC6 | Crow sounds (`crow-sounds.md`) | the idle sound timer and choice, placement at the crow | TC5; sound-positional.md; ownership to be agreed with level-sound-scope S24 | no | S |
| TC7 | Crow death (`crow-death.md`) | what kills a crow, the death, death-idle and dead states with their motions, the physics shell and fall, the body afterwards, the `hit` fx motion | TC5; a physics spec (none approved or proposed); player-hits.md and the weapons | no | M |

Order: TC1; then TC2 and TC5 (independent of each other); then TC3 and TC6; then TC4 and TC7, which wait
for dialogs and trade and for physics.

## 4. Smallest correct step

- **#551, the trader:** TC1 (at least its cycle part, with blending if the two parts split), TC2's global
  and head channels and TC3's start chain. The trader needs no AI: his motions are the scheme's choices
  played on the program's channels. Playing one fixed motion would not be correct: the start chain begins
  with the "leaning back" set and reaches the desk only after `spinka_idle` and the 23.367 s first phrase
  (1.1). TC3 is defined by scripts; until the script runtime runs `mob_trader.script` and `xr_logic.script`
  themselves, an implementation may play the chain from TC3 natively as a temporary stand-in.
- **#559, the crows:** TC1's cycles and TC5. Crows need no scripts and no AI; flight needs only the
  camera's position, random numbers, the frame and the scheduled update. Death (TC7) cannot happen before
  hits exist, and sounds (TC6) are independent of what the frame shows.

## 5. Unestablished

- The condition under which the trader's update skips his channels (TC2); which object the head callback
  turns toward (believed the player, TC2).
- When the trader's logic starts relative to the fidelity views' "play starts" (TC3).
- The source and seeding of the random numbers the scheme and the crows use (TC3, TC5).
- The interval of the crows' scheduled update, and which world axis each `goal_variability` component
  applies to (believed x, y, z, TC5).

## 6. Provenance

- Date: 2026-11-26.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG), archives `gamedata.db0` …
  `gamedata.dbd` (later archives win); binary `bin\xrGame.dll`.
- Data examined: `spawns\all.spawn` (all records), `config\creatures\m_crow.ltx`, `m_person.ltx`,
  `config\scripts\esc_trader.ltx`, `config\gameplay\info_l01escape.xml`, `dialogs_escape.xml`, every file of
  the merged archives other than media for name searches, `scripts\bind_monster.script`, `modules.script`,
  `mob_trader.script`, `mob_sound.script`, `xr_logic.script`, `lua_help.script`, the scripts naming the
  crow's class id, `meshes\actors\trader\trader.ogf`, `meshes\actors\trader_animation.omf`,
  `meshes\monsters\crow\crow.ogf`, the archive listing; the approved specifications listed above.
- Analysis: static analysis of `bin\xrGame.dll` (headless disassembly and decompilation with Ghidra) for the
  crow's configuration reading, spawn, motion groups, states, goal, flight step and its cadence, sounds and
  death, and for the trader's spawn, update, motion channels, sound and script methods; constants read from
  the binary; reading of the scripts and logic files as data; motion data decoded by a throwaway decoder
  written from skeletal-motions.md. No execution or debugging. The checks in
  `checks/trader-and-crows-scope/` reproduce every fact.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Section 4 leaves out of the smallest step for #551 the turning of the trader's head bone toward an
  object of the level (1.1), although it changes the drawn pose; it belongs to TC2.
- `anim_head` is read only from a section that has `sound_phrase`; in a section without one the head set
  becomes `normal` once the section has started (1.1 says only that a section's `anim_head` names a set);
  TC3 covers it.
- When the global motion ends, the program can also end the head motion, under a condition that is
  unestablished; TC2 covers it with the events 30 – 32.
