# Creatures' sounds

How the stalkers, the monsters and the trader of S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition) play their
own sounds: the creature's **sound player** over time (when a queued sound starts, how it is delayed, where it is
placed and how it follows its bone, when it counts as active, when it ends), which collections the engine gives
each creature and with which voice, the monsters' sound delays, the requests the engine itself makes, and what the
scripts' theme system (`xr_sound`, `sound_theme`, `config\misc\script_sound.ltx`) asks of the player. This is spec
CH-25 (`creature-sounds`) of the approved scoping specification `docs/behavior/characters-scope.md` (its section
5 row CH-25 and its "Gaps found in review" apply).

Builds on (not repeated here):

- `docs/behavior/script-api-sound.md` (cited as **T**): the sound player, its **collections**, the **queued
  sounds** and their fields (priority, mask, bone, sound, start time, stop time), and what `add_sound`,
  `play_sound`, `remove_sound`, `set_sound_mask`, `active_sound_count` and `sound_prefix` do at the moment of the
  call (its 6.2 – 6.5). This spec states what happens afterwards and what the engine does with the same player.
- `docs/behavior/sound-positional.md` (cited as **S2**): sounds, emitters, **play at position** (3.2), the
  emitter's position control (3.3), **stop at once** (4.4), the duration *D* (2), the sound system's own update.
- `docs/formats/sound-ogg-params.md` (cited as **S1**): how a sound name finds its file; the AI sound type names
  (its 7.2 table, used below for the type values).
- `docs/behavior/script-api-object-binders.md` (cited as **OB**): the order of spawning (its 5.2: the object's
  engine reload of its section, the binder's `reload`, the engine reinitialisation, the binder's `reinit`, …, the
  binder's `net_spawn`).
- `docs/behavior/script-api-scripted-actions.md` (cited as **V**): a script action's monster sound (its 7.3).
- `docs/behavior/script-api-game-object-basics.md` (cited as **M**): the kinds of object.
- `docs/formats/character-descriptions.md`: the specific character's `snd_config`.

Conventions:

- **Now** is the device's game time in milliseconds (T's "now", `time_global()`).
- A **creature** is a stalker or a monster (the scoping spec's sense); a creature's **section** is its
  `config\system.ltx` section. The **player** of an object is its sound player (T 6).
- A queued sound is **due** when now ≥ its start time, **started** once the player has asked for it to be
  played (a flag of the queued sound, initially clear), **sounding** while its sound has an emitter (S2 3.1),
  and **finished** when it has no emitter and now ≥ its stop time.
- Masks and AI sound types are given in hexadecimal; the type names are S1's.
- "The game library" is `bin\xrGame.dll`, "the sound library" `bin\xrSound.dll` (both as identified by T), "the
  executable" `bin\XR_3DA.exe` (1,636,424 bytes, MD5 `ba3ad0127bff6619ed991a7c82d8b946`) [C-1].
- Each fact is **static** (from static analysis of the binaries), **measured** (from the game data), **believed**
  (inferred; the reason is given) or **unestablished**. No binary was run.

**Checks.** Every claim carries a tag [C-*n*]. The checks are one script in the dirty workspace,
`checks/creature-sounds/run_all.py` (`python3 -I run_all.py`), reading the binaries' bytes and run-time type
information, the evidence of read-only headless static analysis it produces (helpers beside it), T's evidence it
cites, the extracted scripts and configuration, the records of `spawns\all.spawn` and the sound files. All checks
pass (`ALL PASS 17`).

## 1. The player's life (static)

### 1.1 Which objects have one

Every stalker, every monster and the trader gets a player when its object is made [C-2]. A new player has no
collections and no queued sounds, an empty sound prefix, and the object as the owner of what it plays; its mask is
not set at that point [C-2]. Each player keeps its own **draw state** for the delays of T 6.3 step 6, started from
a high-resolution time counter when the player is made, so the delays are not reproducible from run to run
[C-2, C-14].

The trader's player never receives a collection: the engine registers collections only for stalkers and monsters
(sections 4 and 5 list every registration) and the script members of T 6 do nothing on it (it is not a fighter,
T 6.1) [C-2]. It is
updated like the others (2.1) and so stays empty; what the trader says is `scripted-motion-channels.md`'s.

### 1.2 Spawning

In OB's order:

1. **The engine reload of the section** (OB step 2) clears the player of every creature: every collection is
   removed, every queued sound is stopped at once (S2 4.4) and removed, the mask becomes 0 and the sound prefix
   becomes empty [C-3]. So the player's mask is 0 from spawning on (T 10 left this open). For a monster the same
   reload then registers the monster's collections (section 5) [C-10].
2. **The engine reinitialisation** (OB step 4) of a stalker sets the sound prefix to the text of the stalker's
   specific character's `snd_config` (an empty text when the description has none), then registers the stalker's
   collections (section 4) with that prefix in force [C-8].
3. **The binder's `net_spawn`** (OB step 7): for stalkers, `xr_motivator.script` calls `sound_theme.load_sound`,
   which registers the script themes (section 7) [C-15]. **Disputed in review:** the binder reaches
   `sound_theme.load_sound` through `xr_sound.load_sound`, and only for a stalker alive at `net_spawn`: for a dead
   one the binder's `net_spawn` returns before it, so a dead stalker gets no theme collections.

### 1.3 Destruction

When a creature's object is destroyed (it goes offline or is removed), every queued sound is removed and stopped at
once whatever the mask (the player's mask is set to all bits for this removal and restored after), and a player
update still waiting for the frame (2.1) is cancelled [C-7]. Queued sounds therefore never survive the object; a
creature that comes online again starts with a fresh player (1.1, 1.2).

## 2. Over time (static)

### 2.1 When the player is updated

The player is updated once each time its object's own per-frame update runs (stalkers, monsters and the trader
alike); which frames that is belongs to the objects' update, not to this spec [C-4]. The game library has a fixed
option word whose value is 0x1ff, and no instruction of the game library names it except to read it; with its
bit of value 8 set (so always) the update is not
run in place but queued as a job for the current frame, which the engine's second thread runs once, after which
the frame's job list is emptied; with the bit clear it would run in place [C-4]. **Believed:** running the update
right after the object's own per-frame update, in the same frame and with the same now, reproduces it (the job
reads only the player, the object's transform and its bones).

### 2.2 The update

Each update does, in this order [C-4, C-5, C-6]:

1. **Removal.** Every queued sound whose mask shares a bit with the player's mask is removed, and so is every
   finished one. Removing a queued sound stops its emitter at once if it has one (S2 4.4; never with fade) and
   discards its sound.
2. **The pass**, over the remaining queued sounds in their order:
   - a **sounding** one: its emitter's position (S2 3.3) is set to the current world position of its bone (2.3);
     nothing else of the emitter is changed;
   - one that is **due** and **not started**: it is played with S2's play at position, flags 0 (3D, not looped)
     and delay 0, at its bone's world position, and marked started;
   - any other: nothing.

A queued sound is asked to play only once. It is marked started even when the request makes no emitter (for
example with `-nosound`, S2 2); it then waits, silent, until it is finished and removed. A sound whose emitter
ends before its stop time (the stop time includes the pause *d*₂ of T 6.3 step 6) stays queued, silent, until its
stop time has come [C-5, C-6].

So a queued sound passes through four phases: **delayed** (not due), **sounding**, **pause** (emitter ended, stop
time not yet come) and **removed**. A sound queued with no start delay is played by `play_sound` itself (T 6.3
step 7) and is sounding from the call; a delayed one is played by the first update at or after its start time, so
its actual start lies up to one frame after the start time. Its emitter then starts at the sound system's next
update (S2 4.1).

### 2.3 Where it sounds

The bone of a queued sound is looked up by name on the object's model when `play_sound` (or the engine's own
request, sections 4 – 6) queues it, and kept with it [C-6]. Its **world position** is the object's world transform
applied to the translation of the bone's current transform in the model [C-6]; **believed:** that transform is
the pose the model's animation last computed. The position is taken when the sound is played (2.2 step 2 and T 6.3
step 7) and again at every update while it is sounding, so the sound follows the bone (a stalker's voice moves
with its head; a corpse's death cry follows the falling body; **believed**: the object's per-frame update goes on after death).
What happens when the bone name is not on the model is **unestablished** (as in T 10).

The bone is the collection's: a script collection's is the `bone` argument of `add_sound` (`bip01_head` when
omitted, T 6.2); a stalker's engine collections take the section's `bone_head` key (4); a monster's take the text
`bip01_head` (5).

### 2.4 Counted as active

T 6.4's count applied to the phases [C-6, C-5]:

| Phase | `active_sound_count()` | `active_sound_count(true)` |
|---|---|---|
| delayed | not counted | not counted |
| sounding | counted | counted |
| pause | counted | not counted |
| removed | not counted | not counted |

A finished sound is counted until the next removal takes it away: the next update, `play_sound` or
`set_sound_mask` (T 6.3 step 3, 6.4). Since the update runs every frame, the count drops to 0 within one frame of
the later of the emitter's end and the stop time.

### 2.5 How a queued sound ends

A queued sound is removed, and its emitter stopped at once, by: the removal of an update (finished, or sharing a
bit with the player's mask); `play_sound` of a collection whose mask shares a bit with it (T 6.3 step 3, which runs
only when the request may play, step 1); `set_sound_mask` (T 6.4); the engine reload (1.2) and the destruction
(1.3) [C-3, C-5, C-7]. `remove_sound` leaves it (T 6.4).

## 3. Random draws (static)

- **Which sound of a collection** (T 6.3 step 4): each collection keeps its own draw state, started from a
  high-resolution time counter when the collection is made, and its **last** index (none at first) [C-14]. A
  draw advances the state *s* to *s* · 134775813 + 1 (32 bits) and takes the high 32 bits of the 64-bit product of
  the new *s* and the number of sounds; with 3 or more sounds it draws again while the result equals the last
  index [C-14]. The chosen index, drawn or given explicitly, becomes the last index [C-14]. Collections shared by
  several objects (T 6.2) share this state and last index (**believed**, as T 6.2's sharing).
- **The delays** (T 6.3 step 6) use the player's draw state (1.1), the start delay drawn before the pause; the
  state advances only for a delay whose maximum is greater than its minimum [C-14].

## 4. The stalker's collections (static; data measured)

At the engine reinitialisation (1.2) a stalker registers 23 collections from its section, each with 100 items,
the bone named by the section's `bone_head` key (`bip01_head` in all 95 sections of the stalker classes) and the
sound prefix `snd_config` (1.2) [C-8, C-9]. The ids are those the listing's `stalker_ids` gives (`sound_die` 0,
`sound_die_in_anomaly` 1, `sound_injuring` 2, `sound_humming` 3, …, `sound_injuring_by_friend` 12, …), except that
the listing gives `sound_enemy_killed_or_wounded` the value −805302272 while the collection's id is 22 [C-9].

| Id | Key | Type | Priority | Mask | `[stalker]` value |
|---|---|---|---|---|---|
| 0 | `sound_death` | 0x20020000 `monster_die` | 0 | 0xffffffff | `fight\death\death_` |
| 1 | `sound_anomaly_death` | 0x20020000 | 0 | 0xffffffff | `fight\death\anomaly_` |
| 2 | `sound_hit` | 0x20010000 `monster_injure` | 1 | 0xffffffff | `fight\hit\hit_` |
| 12 | `sound_friendly_fire` | 0x20010000 | 1 | 0xffffffff | `fight\friendly_fire\friendly_fire_` |
| 13 | `sound_panic_human` | 0x20004000 `monster_talk` | 2 | 0xd0000000 | `states\panic_human\panic_human_` |
| 14 | `sound_panic_monster` | 0x20004000 | 2 | 0xd0000000 | `states\panic_monster\panic_monster_` |
| 16 | `sound_grenade_alarm` | 0x20004000 | 3 | 0xd0000000 | `fight\grenade\grenade_` |
| 17 | `sound_friendly_grenade_alarm` | 0x20004000 | 3 | 0xd0000000 | `fight\friendly_grenade\friendly_grenade_` |
| 15 | `sound_tolls` | 0x20004000 | 4 | 0xd0000000 | `fight\tolls\tolls_` |
| 4 | `sound_alarm` | 0x20004000 | 5 | 0xd0000001 | `fight\enemy\enemy_` |
| 5 | `sound_attack_no_allies` | 0x20004000 | 5 | 0xd0000002 | `fight\fire\fire_,fight\threat\threat_distant_` |
| 6 | `sound_attack_allies_single_enemy` | 0x20004000 | 5 | 0xd0000004 | `fight\attack\attack_,fight\attack\attack_one_,fight\fire\fire_` |
| 7 | `sound_attack_allies_several_enemies` | 0x20004000 | 5 | 0xd0000008 | `fight\attack\attack_,fight\attack\attack_many_,fight\fire\fire_` |
| 8 | `sound_backup` | 0x20004000 | 5 | 0xd0000010 | `fight\backup\backup_,fight\threat\threat_close_` |
| 9 | `sound_detour` | 0x20004000 | 5 | 0xd0000020 | `fight\detour\detour_,fight\fire\fire_,fight\threat\threat_close_` |
| 11 | `sound_search1_no_allies` | 0x20004000 | 5 | 0xd0000040 | `fight\threat\threat_distant_` |
| 10 | `sound_search1_with_allies` | 0x20004000 | 5 | 0xd0000080 | `fight\searching_enemy\search_,fight\searching_enemy\enemy_lost_` |
| 3 | `sound_humming` | 0x20004000 | 6 | 0xe0000001 | `states\idle\idle_` |
| 18 | `sound_need_backup` | 0x20004000 | 4 | 0xd0000100 | `fight\cover_fire\cover_fire_` |
| 19 | `sound_running_in_danger` | 0x20004000 | 6 | 0xd0000200 | `states\breath\breath_1` |
| 20 | `sound_kill_wounded` | 0x20004000 | 5 | 0xd0000400 | `help\wounded\kill_wounded_` |
| 21 | `sound_enemy_critically_wounded` | 0x20004000 | 4 | 0xd0000800 | `fight\enemy_hit\enemy_hit_` |
| 22 | `sound_enemy_killed_or_wounded` | 0x20004000 | 4 | 0xd0001000 | `fight\enemy_down\enemy_down_` |

(Registration order is the table's.) [C-9]. The key's value is read from the stalker's own section (the table shows
`[stalker]`'s, which its descendants inherit unless they override it); every key is read unconditionally (what a
missing one does is the configuration reader's, `ltx-config`) [C-9]. The 21 collections other than ids 1 and 3
carry a link to the speaking stalker in their sounds, which a creature that hears the sound can query while the
stalker exists and is not being destroyed (**believed**; what hearing does with it is CH-14's) [C-9].

What the masks mean together with T 6.3's rule: the scripts' talk collections (priority 2, mask 1, section 7) share
bit 0 with ids 0, 1, 2, 12 (all bits), 4 (alarm) and 3 (humming). So a death cry or a hit (priority 0 or 1)
removes a queued talk sound and cannot be removed by one; an alarm or humming (priority 5 or 6) cannot start while
talk is queued and is removed when talk is played; ids 5 – 11 and 13 – 22 share no bit with talk and play
alongside it, but each excludes the others whose masks overlap (the top bits 0xd0000000) unless its priority number
is lower.

## 5. The monsters' collections and delays (static; data measured)

### 5.1 Collections

At the engine reload (1.2), after clearing, every monster registers these collections, each only if its section
has the key, each with 16 items, the bone `bip01_head` (a fixed text, not the section's `bone_head` key) and an
empty sound prefix (so the keys carry full paths, e.g. `monsters\dog\bdog_idle_`) [C-10]:

| Id | Key | Type | Priority | Mask |
|---|---|---|---|---|
| 1 | `sound_idle` | 0x20004000 `monster_talk` | 32768 | 0x80 |
| 12 | `sound_distant_idle` | 0x20004000 | 32769 | 0x80 |
| 2 | `sound_eat` | 0x20004000 | 132 | 0x80 |
| 3 | `sound_aggressive` | 0x20002000 `monster_attack` | 131 | 0x80 |
| 4 | `sound_attack_hit` | 0x20002000 | 9 | 0xffffffff |
| 5 | `sound_take_damage` | 0x20010000 `monster_injure` | 8 | 0xffffffff |
| 6 | `sound_strike` | 0x20002000 | 128 | 0x8000 |
| 7 | `sound_die` | 0x20020000 `monster_die` | 1 | 0xffffffff |
| 8 | `sound_die_in_anomaly` | 0x20020000 | 1 | 0xffffffff |
| 9 | `sound_threaten` | 0x20002000 | 128 | 0x80 |
| 10 | `sound_steal` | 0x20008000 `monster_step` | 129 | 0x80 |
| 11 | `sound_panic` | 0x20008000 | 130 | 0x80 |

Single kinds add, after these, collections read unconditionally from their sections (16 items, bone `bip01_head`)
[C-10]:

| Kind | Id | Key | Type | Priority | Mask |
|---|---|---|---|---|---|
| bloodsucker (`SM_BLOOD`) | 16384 | `Sound_Vampire_Grasp` | 0x20002000 | 12 | 0x80 |
| | 16385 | `Sound_Vampire_Sucking` | 0x20002000 | 11 | 0x80 |
| | 16386 | `Sound_Vampire_Hit` | 0x20002000 | 10 | 0x80 |
| | 16387 | `Sound_Vampire_StartHunt` | 0x20002000 | 13 | 0x80 |
| | 16390 | `Sound_Invisibility_Change_State` | 0x20002000 | 128 | 0x10000 | <!-- spec-lint: constant -->
| | 16389 | `Sound_Growl` | 0x20002000 | 14 | 0x80 |
| | 16391 | `Sound_Alien` | 0x20002000 | 1 | 0xffffffff |
| burer (`SM_BURER`) | 16384 | `sound_gravi_attack` | 0x20002000 | 10 | 0x80 |
| | 16385 | `sound_tele_attack` | 0x20002000 | 11 | 0x80 |
| pseudo-dog (`SM_P_DOG`) and the kinds built on it | 16384 | `sound_psy_attack` | 0x20002000 | 11 | 0x80 |

The pseudo-dog's collection is registered only while a flag of the object is clear; which state the flag marks is
**unestablished** (the same flag also skips another part of the reload) [C-10]. That the psy dog (`SM_DOG_P`) and
its phantom reach this registration is **believed** (the phantom's reload leads to it; the psy dog's was not
traced) [C-10].

The other sound keys of single kinds (`PsyAura_SoundLeftPath`, `PsyAura_SoundRightPath`, `Sound_Idle`,
`flame_sound`, `flame_scan_sound`, `scan_sound`, `sound_control_hit`, `sound_control_start`, `sound_gravi_wave`,
`sound_tele_hold`, `sound_tele_throw`, `sound_threaten_hit`, `sound_threaten_start`) are not collections: the kind
makes a plain sound (S2 3.1) from each key and plays it itself [C-17]; when and where each plays belongs to the
specification of that ability (**unestablished** here).

### 5.2 Delays

The keys `idle_sound_delay`, `eat_sound_delay`, `attack_sound_delay`, `distant_idle_sound_delay` (integers, in
milliseconds) and `distant_idle_sound_range` (a distance in metres) are read with the monster's other base
parameters from its section (and, by the same reader, from a second configuration when that has the key) [C-11].

When the monster's current state asks for its state sound of kind *k* (which states ask for which kind, and when, is
the situations' of CH-17 … CH-22), the request is [C-11]:

1. If *k* is the aggressive sound (3) and the last state sound asked for was not 3: the attack-hit collection (4)
   is played instead, with no delays.
2. Otherwise, unless the request says "no delay" (then the pause bound *P* is 0), with *n* the number of the
   other living members of the monster's group within 20 m of it:
   - idle (1): if the distance from the player character to the monster is at most `distant_idle_sound_range`,
     collection 1 with *P* = round(`idle_sound_delay` · √(1 + *n*)); otherwise collection 12 (distant idle) with
     *P* = round(`distant_idle_sound_delay` · √(1 + *n*));
   - eat (2): *P* = round(`eat_sound_delay` · √(1 + *n*));
   - aggressive (3, when step 1 does not apply) and panic (11): *P* = round(`attack_sound_delay` · √(1 + *n*));
   - any other kind: *P* = 0.
3. The collection is played as `play_sound(id, 0, 0, P, 0)`: no start delay and a pause *d*₂ drawn from 0 …
   *P* − 1 (T 6.3 step 6). The collection played becomes the last one, except after step 1, where the last
   one becomes 3.

So the delay is silence **after** each sound, during which the pause phase (2.2) keeps the monster's next state
sound from interrupting it unless that sound's priority number is lower (T 6.3 step 1; mask 0x80). That the player
character is the object measured from is **believed** (the object read there is the one the game holds as the
player's). The rounding is **believed** round-to-nearest (the processor's default mode). A script action's monster
sound (V 7.3; **believed** to be the request path that reads the kind and delay of the action) plays the same
collections with its own delay, or with the delay of the kind (`idle_sound_delay` for
idle, `eat_sound_delay` for eat, `attack_sound_delay` for attack, threaten, steal and panic) when its delay is −1,
as `play_sound(id, 0, 0, delay, 0)` [C-11]. **Disputed in review:** only the kinds idle (1), eat (2), aggressive
(3), threaten (9), steal (10) and panic (11) use the action's delay (or the kind's when it is −1); attack-hit (4),
take-damage (5) and die (7) are played as `play_sound(id, 0, 0, 0, 0)` whatever the action's delay; any other
kind (among them strike, die in anomaly and distant idle) plays nothing.

For example `[dog_normal]` has `idle_sound_delay` 80000, `distant_idle_sound_delay` 80000 and
`distant_idle_sound_range` 30.0: a dog alone more than 30 m from the player plays `monsters\dog\bdog_distant_*`, and
its queued sound then lasts until up to 80 s after the sound has ended, during which another distant idle (equal
priority) is refused while an idle (priority 32768) still interrupts it; with three pack-mates within 20 m the bound is
round(80000 · 2) = 160000 ms [C-11].

## 6. The engine's own requests (static)

The engine asks a creature's player to play at 74 places [C-13]. Deaths [C-12]:

- A stalker's death: the mask is set to 0, finished queued sounds are removed, then collection 0 is played, or 1
  when the stalker died in an anomaly (the same test as a monster's), with no delays. Priority 0 and all mask bits:
  every other queued sound is removed (T 6.3 step 3).
- A monster's death: collection 7, or 8 when it died in an anomaly, with no delays.

The stalker's requests with fixed delays (the situations that make them are CH-17 … CH-22's) [C-13]:

| Collection | Start delay *d*₁ (ms) | Pause *d*₂ (ms) | Places |
|---|---|---|---|
| 3 `sound_humming` | 10,000 … 59,999 (minimum 10,000, maximum 60,000) | 0 | 2 |
| 18 `sound_need_backup` | 3,000 (minimum = maximum) | 10,000 (minimum = maximum) | 1 |
| 8 `sound_backup` | computed at the place (**unestablished**) | 4,000 … 5,999 (minimum 4,000, maximum 6,000) | 1 |
| 4 `sound_alarm`; 15, 17, 19, 21, 22 | 0 | 0 | 1 each |

The other places compute their collection or delays; their values and situations are **unestablished** here.

## 7. The scripts' themes (measured)

The scripts (run as shipped) drive the player through T's members only; this section states what they ask for, so
that the effect follows from sections 2 – 3 and T.

- **Registration** (`sound_theme.load_sound`, at `net_spawn`, 1.2): for each theme with a section `<theme>_rnd`,
  `<theme>_seq` or `<theme>_single` in `script_sound.ltx`, the value chosen for the character (by story id, profile,
  community, else the `stalker` line) is `<flag>,<path>`; the theme's collection is registered with
  `add_sound(path, 64, snd_type.talk, 2, 1, id)`, with the character's own prefix (`snd_config`) when the flag is 1,
  and with the prefix `characters_voice\` (restored afterwards) when it is 0 [C-15]. The ids are
  `stalker_ids.sound_script` + 10000 (the listing's 23: 10023) onwards, two per theme (`rnd_id`, `into_id`) in the
  order of `sound_theme.script`'s table [C-15].
- **Playing** (`xr_sound.play_sound`): a sequence theme (`into_id`) is played as `play_sound(id, 1, 0, 1, 0, i)`
  and a random theme (`rnd_id`) as `play_sound(id, t + 1, t, 1, 0, i)`, *t* the caller's timeout in ms (0 when
  none), *i* the index the script chose [C-15]. By T 6.3 step 6 the first form has no start delay and no pause;
  the second starts exactly *t* ms later (the maximum exceeds the minimum by 1, so the draw is 0) and has no
  pause. The theme's own idle interval is the scripts' timing, not the player's.
- **Interrupting** (`stop_play_sound`): `set_sound_mask(-1)` then `set_sound_mask(0)`: every queued sound is
  removed and stopped at once, and nothing stays blocked [C-15]. **Disputed in review:** `stop_play_sound`
  does this only while the object is alive; on a dead one it does nothing. `xr_sound.set_sound_play` does this before every
  play [C-15].
- **Waiting**: `xr_sound.update` (from the stalker binder's update) does nothing while `active_sound_count()` is
  not 0; members of one sound group (`soundgroup`) take turns [C-15].

### 7.1 Worked example: the beginners' village campfire on `l01_escape`

The campfire is the job `kamp1@esc_lager` (`center_point = camp_center`, `soundgroup = esc_lager`) of the
beginners' village; its stalkers are the records `esc_lager1` … `esc_lager7`, all of profile
`esc_stalker_novice` [C-15, C-16]. Take one whose specific character is `esc_stalker_novice_default1`: its
`snd_config` is `characters_voice\human_01\newbie\`, and its supplies may give it a guitar (`guitar_a`, probability
0.7) [C-16].

**Registration.** At `net_spawn` its prefix is `characters_voice\human_01\newbie\` (1.2). The five themes have only
`_rnd` sections, all with flag 1 [C-15]:

| Theme | `rnd_id` | Item | Sounds (files) | First file's *D* |
|---|---|---|---|---|
| `guitar` | 10115 | `music\guitar_` | 11 (`guitar_1` … `guitar_11`) | 29,088 ms |
| `intro_joke` | 10117 | `talk\intros\intro_joke_` | 5 | 1,341 ms |
| `joke` | 10119 | `talk\jokes\joke_` | 10 (`joke_1` … `joke_10`) | 22,019 ms |
| `reac_joke` | 10067 | `reactions\joke\reaction_joke_` | 6 | — |
| `story_reac_laughter` | 10173 | `reactions\story\laughter_` | 6 | 2,967 ms |

Each collection holds the files `<prefix><item><i>` for *i* = 0 … 63 that exist (T 6.2), so index 0 is
`guitar_1` (there is no `guitar_0`) and `add_sound` returns 11 for the guitar [C-16].

**The guitar.** When the campfire's animation reaches the guitar's playing part (`into_music = "guitar"`), the state
manager calls `stop_play_sound` and then `xr_sound.set_sound_play(npc, "guitar")` with no timeout: everything
queued is stopped, and `play_sound(10115, 1, 0, 1, 0, i)` plays guitar sound *i* at once (T 6.3 step 7) at the
stalker's `bip01_head` [C-15]. For *i* = 0 (`guitar_1`, *D* = 29,088 ms) the queued sound's stop time is the call's
now + 29,088. Every frame the update moves the emitter to the head's current position, so the music follows the
stalker's head as he sways (2.2, 2.3). `active_sound_count()` is 1 from the call until the first update at which
the emitter has ended and the stop time has come, then 0 [2.4].

**The jokes.** The storyteller first calls `xr_sound.set_sound_play(npc, "intro_joke", t)` with *t* =
`math.random(2000, 3000)`, later `set_sound_play(npc, "joke", t)` likewise; a listener answers with `reac_joke`
(the first one) or `story_reac_laughter` (`math.random(100, 300)`) [C-15]. For the joke with *t* = 2,500 and
*i* = 0: the call removes all queued sounds, then queues `joke_1` with start time now + 2,500 and stop time start
+ 22,019. For 2,500 ms the sound is delayed: `active_sound_count()` is 0, and the campfire script, whose joke and guitar
states wait for the sound to begin, holds its state until the count becomes non-zero [C-15]. The first
update at or after the start time plays it at the head; from then on the count is 1, and the campfire moves on
only once it is 0 again, about 22 s later [2.2, 2.4]. A hit on the storyteller meanwhile plays collection 2
(priority 1, all mask bits) and removes the joke at once (section 4).

## 8. Verification facts

- Player: made with the object; mask 0 after the engine reload; the update queued per frame (option word 0x1ff,
  bit of value 8 set) [C-2 – C-4].
- Update: removal (mask, finished), then sounding → position to the bone, due and not started → play at position
  (flags 0, delay 0) [C-5, C-6].
- Stalker: 23 collections, 100 items, prefix `snd_config`; monster: 12 optional collections, 16 items, empty prefix,
  bone `bip01_head`; bloodsucker 7, burer 2, pseudo-dog 1 more [C-9, C-10].
- Monster pause bound round(delay · √(1 + *n*)), *n* within 20 m; dog alone: 80,000; with three: 160,000 [C-11].
- Theme ids: `guitar` 10115, `intro_joke` 10117, `joke` 10119, `reac_joke` 10067, `story_reac_laughter` 10173
  [C-15]. Newbie collections 11, 5, 10, 6, 6 [C-16].
- Worked example: `joke` with *t* = 2,500, index 0: delayed 2,500 ms (count 0), then sounding 22,019 ms (count 1),
  removed within a frame after [C-15, C-16].

## 9. Unestablished

- What a bone name not on the model gives (2.3); the pose the bone transform reflects (2.3, believed the last
  computed).
- That running the update in the frame, right after the object's update, reproduces the second thread's job (2.1,
  believed).
- Whether queued sounds or the player's state are saved with a game (not examined; the saved state is CH-27's).
- The flag that gates the pseudo-dog's collection; whether the psy dog reaches it (5.1).
- The rounding mode of 5.2 (believed nearest); that the distance of 5.2 is the player character's (believed).
- When and where the single kinds' plain sounds play (5.1).
- Which situations make the engine's requests, and the computed collections and delays of the places not in 6's
  table (CH-17 … CH-22).
- What hearing does with a stalker collection's speaker link (4, CH-14).
- The 8 CH-25 keys not named in the game library (`PsyAura_HeadSound`, `sound_attack`, `sound_bkgnd`,
  `sound_growling`, `sound_landing`, `sound_search`, `sound_surrender`, `sound_walking_in_danger`): believed unread
  by the engine (they may still be read through a composed name) [C-17].

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll`, `xrSound.dll` and
  `XR_3DA.exe` as identified above; the scripts, configuration, records and sound files of the archives
  `gamedata.db0` … `gamedata.dbd` as extracted for the approved specs.
- Analysis: static analysis of the three binaries with Ghidra (headless, read-only: decompilation and disassembly
  of the sound player's routines and their callers, the creatures' spawning, update, death and registration
  routines, the engine's per-frame job runner; run-time type information for which objects own them); reading of
  the shipped scripts and configuration; Ogg page headers of the sound files. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

- Section 6's table omits a stalker request with constant arguments: collection 2 (`sound_hit`) played with no
  delays (start 0, pause 0) at a place that also plays collection 21 with no delays.
- Section 7's "Waiting" omits the other conditions of `xr_sound.update`: it runs only while the stalker is alive
  (the binder calls it only then), does nothing while no theme is set for the stalker, while another member of the
  sound group is the group's busy speaker, or when the stalker spoke last in a group of more than one, and plays
  the set themes only once the group's idle interval (drawn from the theme's `min_snd` … `max_snd` seconds) has
  passed since the group's last play.
- Section 7's registration does not state how the chosen value splits: only its first character is the flag (any
  character other than `0` keeps the character's prefix) and the path is the text from its third character on; a
  theme section with no line for the character (no story id, profile, community or `stalker` line) registers
  nothing.
