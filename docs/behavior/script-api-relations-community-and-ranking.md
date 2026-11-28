# Script API Q: relations, community and ranking

What the relations, community and ranking part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of
Chernobyl (Russian edition), as scripts see it: the members of `game_object` that read and change an object's
team, squad and group, its relation to another object, personal goodwill, community, rank, reputation and relation
flags; the namespace `relation_registry` (community goodwill); and the namespace `actor_stats` (the stalkers'
ranking and the player's points). This is spec Q of the approved scoping specification
`docs/behavior/script-api-scope.md` (section 6, order 23); its scope rule and its review gaps apply: this spec owns
every name of these classes and namespaces as the game library registers them, listed or not. It has no entry
points (the scoping spec's 4.5 has no row Q).

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type,
  namespaces, the 250 loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched, what a
  call of a member a class lacks does); spec E, `docs/behavior/script-api-value-types-and-math.md` (the class
  `flags32`); spec M, `docs/behavior/script-api-game-object-basics.md` (the handle, the **kinds** of object of its 2.2,
  what a member does on an object of the wrong kind, 2.3, the routine that returns at once, and its section 8, the
  constants `friend`, `neutral`, `enemy`, `dummy`, `relation_kill`, `relation_attack`, `relation_fight_help_human`,
  `relation_fight_help_monster`).
- `docs/behavior/character-identity.md` (cited as `identity`: the values of a character's simulation object, 1.1;
  the community list and a community's **position** and **number**, 1.3; the Trader-like block's community, rank
  and reputation, and what the online character copies, 5; the monster sections' `rank`, 6),
  `docs/behavior/characters-scope.md` (cited as `CH`: the parts CH-3 relations and CH-4 relation changes, 1.4 the
  sections of `game_relations.ltx`), `docs/formats/sav.md` (cited as `sav`: 7.4 the Creature and Trader-like
  blocks, chunk 9 parts 2 relations and 9 statistics, 9.10), `docs/behavior/offline-simulation.md` (simulation
  objects and their ids), `docs/formats/ltx-config.md`, `docs/behavior/console-log.md` (3.8, a failed check).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [Q-1]. **The
  listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from the
  game library's registration code.
- A call is written as scripts write it: `npc:relation(db.actor)`. A **number** is the runtime's double, a **text** a
  Lua string, **nil** Lua's nil. Integer results are converted to numbers as signed 32-bit integers [Q-6].
- Kinds are M 2.2's: **entity**, **living**, **inventory owner**, **fighter**, **stalker**; and one this spec adds,
  **monster**: the 17 creature classes other than `CAI_Stalker` and `CAI_Trader` (not the player) [Q-5]. A
  **character** is an inventory owner that is not a monster: a stalker, a trader or the player. The **player** is the
  player's object; the program keeps one reference to it, set when the player's object spawns and cleared when it
  is destroyed [Q-20].
- A member applied to an object without the kind it needs does nothing and returns the fallback given (M 2.3).
- `game_relations.ltx` is `config\creatures\game_relations.ltx` (`CH` 1.4); its values below are measured.
  **The system configuration** is the configuration of `ltx-config` the program reads these sections from.
- **Fatal** means a failed check reported as `console-log` 3.8 describes, after which the program ends (believed, as
  in the other specifications: no binary was run).
- Each fact is **static** (from static analysis of the game library, not observed at run time), **measured** (from
  game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**.

**Checks.** Every claim carries a tag [Q-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-relations/run_all.py` (`python3 -I run_all.py`): it reads the game library's bytes and run-time
type information, the evidence of read-only headless static analysis (produced by `gh.py` beside it), the
evidence of spec M's checks it cites, the listing, the scoping spec's scan and the shipped scripts and data. All
checks pass (`ALL PASS 22`).

## 1. What this spec owns

### 1.1 Items (measured, static)

The scoping spec's split gives this spec 27 listed items and no candidate strings [Q-2]:

| Owner | Items |
|---|---|
| `game_object` | 20 functions: `team`, `squad`, `group`, `change_team` (section 3); `relation`, `set_relation`, `goodwill`, `set_goodwill`, `change_goodwill`, `general_goodwill` (section 4); `rank`, `character_rank`, `set_character_rank`, `character_reputation`, `change_character_reputation`, `set_character_reputation` (section 5); `character_community`, `set_character_community` (5.3); `get_actor_relation_flags`, `set_actor_relation_flags` (section 6) |
| `relation_registry` | `community_goodwill`, `set_community_goodwill`, `change_community_goodwill` (section 7) |
| `actor_stats` | `add_to_ranking`, `remove_from_ranking`, `get_points`, `add_points` (section 8) |

Unlisted names: the scripts' methods `community_goodwill` and `set_community_goodwill` on game objects (1.2), and two
functions registered in `actor_stats` that the listing lacks, `get_actor_ranking` and `add_points_str` (section 8)
[Q-2, Q-3].

### 1.2 As registered (static)

- **`game_object`**: 19 of the 20 listed functions are registered, each once [Q-4]. `set_character_reputation` is
  not registered, and its name does not occur in the game library at all [Q-3] (as M 1.2 states); a call of it is a
  call of a member the class lacks (A).
- **`relation_registry`** is registered with exactly `change_community_goodwill`, `set_community_goodwill`,
  `community_goodwill` [Q-3].
- **`actor_stats`** is registered with exactly `get_actor_ranking`, `remove_from_ranking`, `add_to_ranking`,
  `get_points`, `add_points_str`, `add_points` [Q-3].
- **The methods `community_goodwill` and `set_community_goodwill` on game objects are not registered**: every
  occurrence of these names in the game library belongs to the registration of the `relation_registry` functions
  [Q-3]. The shipped calls `v:set_community_goodwill (-5000, db.actor)`, `v:community_goodwill(db.actor)`
  (`enemy.script`) and `obj:set_community_goodwill( goodwill, target_obj )` (`xr_gulag.script`, in `gulag:set_goodwill`)
  are calls of members `game_object` lacks (A) [Q-3, Q-22].

## 2. The values involved

### 2.1 Per object (static)

| Value | Held by | Read and changed by |
|---|---|---|
| team, squad, group | every entity (M 2.2); bytes | section 3 |
| monster community | every living object: the position of its section's `species` in `[monster_communities] communities` of `game_relations.ltx` (−1 when not listed), set when the object is loaded from its section [Q-8] | `relation` (4.1) |
| community, rank, reputation | every inventory owner (`identity` 5: an online character copies them from its simulation object's Trader-like block) | section 5 |
| monster rank | every monster: its section's `rank`, read when it comes online (`identity` 6) [Q-13] | `character_rank` (5.1) |
| relation flags | every stalker; 32 bits | section 6 |

**Team, squad and group** are set when the entity spawns: from its simulation object's team, squad and group
(`sav` 7.4, the Creature block, each as a byte); for a monster the team is then replaced by the **number** of its
species' entry in `[monster_communities] communities` when that number is not 255; for an entity whose simulation
object has no Creature block (the car and the helicopter of spec Y), all three are 0 [Q-6]. In this edition the
numbers are `actor` 255, `human` 255, `arena_monstr` 39, `boar` 40, … `tushkano` 56 (20 entries), and the creature
sections' `species` are 20 distinct names: `human` 4 sections, `arena_monstr` 4, `zombie` 3, the other 17 one each
[Q-21].

### 2.2 The relations registry (static)

The simulation keeps a **relations registry**: per object id, a **personal** table (another object's id → goodwill)
and a **community** table (a community's **position** → goodwill) [Q-9, Q-17]. It is part 2 of `sav` chunk 9,
saved and loaded with the game (`sav` 9). An entry is created when a value is first stored for the object; a value
never stored reads as 0 [Q-9, Q-17].

- The **personal goodwill of A towards B** is A's personal table at B's id. Every store clamps the value to
  `[action_points] personal_goodwill_limits` (minimum, maximum) of the system configuration, read once, on first
  use [Q-9]. In this edition −1000, 1000 [Q-21].
- The **community goodwill of community C towards B** is B's community table at C's position. Every store clamps
  it to `[action_points] community_goodwill_limits`, read the same way [Q-17]. In this edition −3000, 1000 [Q-21].

### 2.3 Lists and tables (static; data measured)

The program reads these from `game_relations.ltx`, each once, when first needed [Q-12]:

- **Lists** `[game_relations] rating`, `[game_relations] reputation` and the community lists (`identity` 1.3,
  `[monster_communities] communities`): items split at commas, taken as pairs (name, number); a name without a
  number after it gets 0 [Q-12]. A value's **category** in a list is the first entry whose number is greater than the
  value, else the last entry [Q-12]. So in this edition the rank categories are `novice` (below 300), `experienced`
  (300 … 599), `veteran` (600 … 899), `master` (900 and above); the reputation categories `terrible` (below
  −1000), `very_bad`, `bad`, `neutral` (−50 … 49), `good`, `very_good` (150 … 999), `excellent` (1000 and above)
  [Q-21].
- **Tables** `[communities_relations]` (communities × communities), `[rank_relations]` (rank categories ×
  rank categories), `[reputation_relations]` (reputation categories × reputation categories), `[monster_relations]`
  (monster communities × monster communities): each line is a row, found by its key in the row list (a key the
  list lacks is fatal, `wrong community %s in section [%s]`); the number of lines must equal the list's length (a
  failed check, `wrong size for table in section`); the row's items are split at commas and each converted as a
  decimal integer prefix (leading blanks skipped, 0 when it has no digits; a missing item 0) [Q-12]. *T*[*a*][*b*] is
  row *a*, column *b*.
- In this edition `[rank_relations]` and `[reputation_relations]` are all 0 [Q-21]. The `human` row of
  `[monster_relations]` lacks the comma after its first value (`0 		 1,`), so as read it is shifted: `human` →
  `actor` 0 and `human` → `human` 0, each following column takes the next written value, and the last column (`tushkano`)
  is 0 [Q-21].

## 3. Team, squad and group (static)

| Member | Effect |
|---|---|
| `obj:team()`, `obj:squad()`, `obj:group()` | entities: the number (0 … 255) of 2.1; −1 for any other object [Q-6] |
| `obj:change_team(team, squad, group)` | fighters (M 2.2: creatures other than the trader; not the player): see below; nothing for any other object [Q-7] |

`change_team` takes three numbers, each converted to a byte (believed: the low 8 bits of the integer). When all three
equal the current ones it does nothing [Q-7]. Otherwise the creature leaves the level's entry for its old (team,
squad, group), its three values are set to the new ones, and it joins the entry for the new triple, which is
created if needed (believed: the bookkeeping by which the level groups creatures; its uses are CH's) [Q-7]. A monster
also leaves and rejoins the monsters' own grouping for the old and new triple (believed; CH's) [Q-7].

The team is not the community: `set_character_community` (5.3) changes both.

## 4. Relations and goodwill (static)

### 4.1 `obj:relation(other)`

Both `obj` and `other` must be living (M 2.2); otherwise the result is −1 (`dummy`) [Q-8]. The result is a relation
value: `friend` 0, `neutral` 1, `enemy` 2, the unnamed value 3, or −1 (`dummy`) (the constants of M 8) [Q-8]:

1. When `obj` is a stalker or the trader and `other` is a character (an inventory owner that is not a monster): from
   the **attitude** of `obj` towards `other` (4.4) [Q-8]: below `[game_relations] attitude_neutal_threshold`
   `enemy`; otherwise below `attitude_friend_threshold` `neutral`; otherwise `friend` (an attitude of exactly
   −2,147,483,647 gives `neutral`) [Q-8]. The two thresholds are read once, on first use; in this edition −400 and
   500 [Q-21].
2. Otherwise (`obj` a monster or the player, or `other` a monster): from *T*[community of `obj`][community of
   `other`] of `[monster_relations]`, the monster communities of 2.1: 1 gives `friend`, 0 `neutral`, −1 `enemy`,
   −2 the value 3, any other value −1 [Q-8].

So the player's relation to anything, and anything's relation to a monster, comes from `[monster_relations]` (the
player's species is `actor`, a stalker's `human`) [Q-8, Q-21]. A monster community of −1 (a species the list lacks)
reads outside the table (believed undefined; no creature section of this edition has one) [Q-8, Q-21].

### 4.2 Personal goodwill

| Member | Effect |
|---|---|
| `obj:goodwill(other)` | inventory owners: the personal goodwill of `obj` towards `other` (2.2), 0 when none was stored; `other` may be any object (only its id is used); 0 for any other `obj` [Q-9] |
| `obj:set_goodwill(value, other)` | inventory owners: stores `value` (made an integer) as the personal goodwill of `obj` towards `other`, clamped (2.2); nothing for any other `obj` [Q-9] |
| `obj:change_goodwill(delta, other)` | inventory owners: stores the current personal goodwill of `obj` towards `other` plus `delta`, clamped [Q-9] |
| `obj:set_relation(relation, other)` | both inventory owners: stores, as the personal goodwill of `obj` towards `other` (clamped), `[game_relations] goodwill_friend` for `relation` 0 (`friend`), `goodwill_neutal` for 1 (`neutral`) and `goodwill_enemy` for any other value (`enemy`, `dummy`, …) [Q-10]; nothing when either is not an inventory owner. The three values are read once, on first use; in this edition 1000, 0, −1000 [Q-21] |

So `set_relation` sets one goodwill; the relation that results is `relation`'s (4.1) only as far as the other terms
of the attitude allow.

### 4.3 `obj:general_goodwill(other)`

The attitude of `obj` towards `other` (4.4) [Q-11]. Neither handle's kind is tested: both must be inventory owners;
for any other object the program reaches through no object (believed: an access fault) [Q-11].

### 4.4 The attitude

The attitude of a character A towards a character B is the sum of five terms [Q-11]:

1. the personal goodwill of A towards B (2.2);
2. `[reputation_relations]` *T*[reputation category of A][reputation category of B] (2.3; the reputations of 5.2);
3. `[rank_relations]` *T*[rank category of A][rank category of B] (the inventory owners' ranks of 5.1);
4. the community goodwill of A's community towards B (2.2);
5. `[communities_relations]` *T*[A's community][B's community] (positions, `identity` 1.3).

A community of −1 (none) reads outside the tables (believed undefined). Which other parts of the program use the
attitude (and its terms' changes by attacks, kills and help in fights) is CH-3's and CH-4's.

## 5. Rank, reputation and community (static)

### 5.1 Rank

| Member | Effect |
|---|---|
| `obj:rank()` | stalkers: the character's rank (the inventory owner's, as `character_rank`); 0 for any other object [Q-13] |
| `obj:character_rank()` | monsters: the monster rank of 2.1; other inventory owners: the inventory owner's rank; 0 for any other object [Q-13] |
| `obj:set_character_rank(rank)` | inventory owners: sets the rank (made an integer), under the condition of 5.4; nothing for any other object [Q-13] |

### 5.2 Reputation

| Member | Effect |
|---|---|
| `obj:character_reputation()` | inventory owners: the reputation; 0 for any other object [Q-14] |
| `obj:change_character_reputation(delta)` | inventory owners: sets the reputation to the current one plus `delta`, under the condition of 5.4 [Q-14] |

The shipped scripts call `change_character_reputation(character_reputation() + reward)` (`task_manager.script`,
twice), so the reputation becomes twice the old one plus the reward [Q-14, Q-22]. (`set_character_rank` is called
with `character_rank() + reward` in the same places, which sets it to the old rank plus the reward [Q-22].)

### 5.3 Community

| Member | Effect |
|---|---|
| `obj:character_community()` | inventory owners: a text, the name of the community at the inventory owner's community position (`identity` 1.3) [Q-15]. A position outside the list (−1, none) is fatal: `item by index not found in section %s, line %s` [Q-15]. nil for any other object [Q-15] |
| `obj:set_character_community(name, squad, group)` | objects that are inventory owners and entities: see below; nothing for any other object [Q-15] |

`set_character_community`: `name` is looked up in the community list; a name the list lacks fails a check
(`item not found, id`; believed fatal) [Q-15]. Then:

1. the community position is set under the condition of 5.4; when it is set, the object's team also becomes that
   community's **number** (its squad and group kept), as by `change_team` (section 3), and the simulation object's
   community is set too [Q-15];
2. in every case, the object's `change_team(number, squad, group)` follows, with the community's number and the
   arguments `squad`, `group` [Q-15].

So when the condition of 5.4 fails, the team, squad and group still change but the community does not [Q-15].

### 5.4 The simulation-object condition

`set_character_rank`, `change_character_reputation` and `set_character_community` (step 1) change the value only when
the simulation has an object with the object's id whose state has a Trader-like block (`sav` 7.4; a stalker, the
trader, the player; not a monster, `identity` 6) [Q-13, Q-14, Q-15]. Then they set the value both on the online
object and on that simulation object (whose Trader-like block saves it) [Q-13, Q-14, Q-15]; otherwise nothing
changes. A monster's rank, reputation and community therefore cannot be set (believed: its simulation object has no
Trader-like block, `identity` 6). What a monster's inventory-owner community, rank and reputation hold is
**unestablished** (they are not copied from a Trader-like block).

## 6. Relation flags (static)

| Member | Effect |
|---|---|
| `obj:get_actor_relation_flags()` | a `flags32` (spec E) with the stalker's relation flags [Q-16] |
| `obj:set_actor_relation_flags(flags)` | sets the stalker's relation flags to the `flags32` `flags` [Q-16] |

Neither tests the kind: on an object that is not a stalker the program reaches through no stalker (believed: an
access fault) [Q-16]. The flags are cleared to 0 when the stalker is initialised (believed: at spawn), and the
program's relation-change routine (CH-4) sets in a stalker's flags the bits of the relation event done to it, the
values of M 8's `relation_attack` 1, `relation_fight_help_human` 2, `relation_fight_help_monster` 4
(`relation_kill`, 0, sets none) [Q-16]. What the flags then decide is CH-4's; no shipped script calls these members
[Q-2].

## 7. `relation_registry` (static)

Each function looks up `community` (a text) in the community list as 5.3 does (an unknown name fails the same check)
and takes `id` as an object id [Q-17]:

| Function | Effect |
|---|---|
| `relation_registry.community_goodwill(community, id)` | the community goodwill of `community` towards object `id` (2.2), 0 when none was stored [Q-17] |
| `relation_registry.set_community_goodwill(community, id, value)` | stores `value` as that goodwill, clamped to the community limits (2.2) [Q-17] |
| `relation_registry.change_community_goodwill(community, id, delta)` | stores the current value plus `delta`, clamped [Q-17] |

The object need not exist: the registry is keyed by id only [Q-17] (believed: an entry for an id no object has is
kept and saved like any other). This goodwill is the attitude's term 4 for every member of `community` towards
object `id` (4.4).

## 8. `actor_stats` (static)

### 8.1 The ranking

The program keeps one **ranking list** of entries (object id, that object's simulation Trader-like part) [Q-18]:

| Function | Effect |
|---|---|
| `actor_stats.add_to_ranking(id)` | removes the entry for `id` if any, then appends a new entry for `id` with its simulation object's Trader-like part (none when the simulation has no such object) [Q-18] |
| `actor_stats.remove_from_ranking(id)` | removes the entry for `id`, if any [Q-18] |
| `actor_stats.get_actor_ranking()` | sorts the list by the entries' ranks (the simulation objects' Trader-like rank), greatest first (believed: the order of equal ranks is not defined), then returns the 0-based position of the player's entry; 1 when the player has none [Q-18] |

An entry without a Trader-like part is read through no object when the list is sorted (believed: an access fault)
[Q-18]. The list is also sorted and read, by the same function, by the PDA's stalkers' ranking window
(`config\ui\stalkers_ranking.xml`), which shows the player's position plus 1 (its layout is the PDA's) [Q-18]. The
shipped scripts add the player when its binder spawns and remove it when it is destroyed (`bind_stalker.script`), and
add stalkers (`se_stalker.script`) and remove them (`se_stalker.script`, `xr_motivator.script`), each call guarded by
a test that the function it calls is not nil (`if(actor_stats.add_to_ranking~=nil)`) [Q-2, Q-22].

### 8.2 The player's points

The player's **statistics** (part 9 of `sav` chunk 9, keyed by the player's id) are **sections** (each a key) of
**details** (each a key, a **count**, **points** and a **text**; the two numbers and the text of `sav` 9.10) [Q-19]:

| Function | Effect |
|---|---|
| `actor_stats.add_points(section, detail, count, points)` | finds the section and its detail, creating each (count and points 0, empty text) if missing; adds `count` to the count and `count` × `points` to the points [Q-19] |
| `actor_stats.add_points_str(section, detail, text)` | finds or creates them the same way and sets the detail's text to `text` [Q-19] |
| `actor_stats.get_points(section)` | for a section other than `total`: finds or creates it; the sum over its details of count × points, or −1 when any detail has a non-empty text [Q-19]. For `total`: the sum of that value over every section whose value is not −1; −1 when there is none [Q-19] |

All three act on the player; with no player the program reaches through no object (believed) [Q-19, Q-20]. So a
detail given points twice holds count 2 and points 2·*p*, and `get_points` counts it as 4·*p* [Q-19]. The shipped
scripts call `add_points("quests", name, 1, reward_rank)` and, for kills, `add_points(props, section, 1, points)`
(`task_manager.script`, `xr_statistic.script`) [Q-22]; no shipped script calls `get_points`, `add_points_str` or
`get_actor_ranking` [Q-2]. What reads these statistics (the PDA's statistics) is the PDA's.

## 9. Worked example: a storyline task's reward

`task_manager.script`'s storyline reward (`CRandomTask:task_reward_storyline`) does, for a task with a reputation
reward *r*, relation rewards and a rank reward *k* [Q-22]:

1. `db.actor:change_character_reputation(db.actor:character_reputation() + r)`: with the player's reputation *p*,
   the reputation becomes 2*p* + *r* (5.2), set on the player and on its simulation object (5.4).
2. For each (community *c*, value *v*): `relation_registry.change_community_goodwill(c, db.actor:id(), v)`: the
   community goodwill of *c* towards the player becomes its old value plus *v*, clamped to −3000 … 1000 (section 7).
   Every member of *c* then has that much more attitude towards the player (4.4, term 4), so with the thresholds −400
   and 500 its relation to the player may change (4.1).
3. `actor_stats.add_points("quests", name, 1, k)` adds 1 to the count and *k* to the points of the detail `name` of
   section `quests` (8.2), and `db.actor:set_character_rank(db.actor:character_rank() + k)` raises the player's rank by
   *k* (5.1), which may change its rank category (2.3; `veteran` from 600).

Similarly `dialogs_military.script` calls `relation_registry.set_community_goodwill ("freedom", db.actor:id (), -5000)`,
which stores −3000 (the community minimum) [Q-17, Q-21, Q-22].

## 10. Use by the shipped scripts (measured)

By the scoping spec's scan, the 24 names of this spec are used by 31 loadable scripts as the scoping spec counts them
(32 counting names that several classes declare), 38 of all scripts [Q-2]. Most used: `character_community` (14
loadable scripts), `relation` (11), `character_rank`, `set_relation` (6 each) [Q-2]. Not used by any script: `team`,
`general_goodwill`, `change_goodwill`, `get_actor_relation_flags`, `set_actor_relation_flags`,
`set_character_reputation`, `actor_stats.get_points` [Q-2].

## 11. Unestablished

- What the level's grouping of creatures by (team, squad, group) and the monsters' own grouping are used for
  (section 3; CH's).
- The values of a monster's inventory-owner community, rank and reputation (5.4), and so what `character_community`,
  `character_reputation`, `goodwill` and the attitude give for a monster.
- The order of equal ranks in the ranking (8.1).
- What the relation flags decide (section 6, CH-4), and where the attitude is used besides `relation` (CH-3).
- What a monster community or character community of −1 reads in the tables (4.1, 4.4).

## 12. Verification facts

All reproduced by `run_all.py` (22 PASS lines, no failure):

- Scope: 27 listed items, 24 used names, 32 loadable scripts as upper bound [Q-2]; registrations of the two
  namespaces (9 functions, 2 unlisted) and of 19 `game_object` members; `set_character_reputation` absent [Q-3,
  Q-4].
- Kinds per member; the monster kind [Q-5]; team, squad, group and their spawn values [Q-6]; `change_team` [Q-7].
- `relation` by attitude and by `[monster_relations]` [Q-8]; the registry, its limits, `set_relation` [Q-9, Q-10,
  Q-17]; the five terms of the attitude [Q-11]; lists, categories and tables [Q-12].
- Rank, reputation, community and the simulation-object condition [Q-13 – Q-15]; relation flags [Q-16]; ranking and
  points [Q-18, Q-19]; the player reference [Q-20].
- `game_relations.ltx`: limits −1000, 1000 and −3000, 1000; goodwill constants 1000, 0, −1000; thresholds −400, 500;
  rank and reputation tables all 0 [Q-21].

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `bin\xrGame.dll` as identified above;
  the scripts and configuration of the root archives as extracted for the scoping spec.
- Analysis: static analysis of `xrGame.dll` with Ghidra in headless read-only mode (decompilation and disassembly of
  the registration code of the two namespaces and of the members of this spec, of the functions they call, of the
  readers of the relation lists and tables, and of the users of the ranking list); run-time type information read
  from the file bytes; the scoping spec's scan of the scripts; a reader of `game_relations.ltx` emulating the
  program's item splitting. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.
