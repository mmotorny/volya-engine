# Script API M: game object basics

What the game-object basics of the game scripts' interface do in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition): the class `game_object` (the scripts' handle to an object in the level) with the members the scoping
specification gives to this spec (identity, place, condition, the kind-specific members of creatures, items,
zones and restrictors, its 22 constants and 5 properties), the class `hit`, the class `CGameObject` with the
overrides the engine reaches, the class `explosive`, and the engine client classes the scripts can name, with
`CPhysicObject`, which the listing lacks. This is spec M of the approved scoping specification
`docs/behavior/script-api-scope.md` (section 6, order 18); its scope rule, its split of `game_object`'s
declared members by function and its review gaps apply: this spec owns every name of these classes as the
binary registers them, listed or not, and the `CGameObject` overrides of its section 4.5.

Builds on (not repeated here):

- `docs/behavior/script-environment.md` (the runtime, one number type, the 64-bit double),
  `docs/behavior/script-loading.md` (namespaces, the 250 loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md`: script classes with
  engine bases, how an engine call reaches an override, base-class calls, how arguments are matched and what a
  mismatch or a call of a missing member does. Where a statement needs that ground it says **depends on A**.
- Spec B, `docs/behavior/script-api-engine-services.md` (the render device and its times, the classes
  `DLL_Pure` and `IRender_Visual`; the routine behind `log`, which returns at once).
- Spec C, `docs/behavior/script-api-class-ids-and-the-object-factory.md` (class ids and their numbering, the
  object factory, the shipped registrations and their client classes).
- Spec E, `docs/behavior/script-api-value-types-and-math.md` (`vector`, `matrix`; a vector result is a new
  vector object).
- Spec F, `docs/behavior/script-api-configuration-and-files.md`: the class `ini_file`. What an
  `ini_file` object is and how it reads its text is F's; statements that need it say **depends on F**.
- `docs/behavior/offline-simulation.md` (spawn records, simulation objects, which objects come online, 6.5,
  level and game vertices, `level.ai` level vertices of 21 bytes), `docs/behavior/player-hits.md` (hit types,
  wounds, what a hit does to the player), `docs/behavior/physics-shells.md` (the trader's door's record, 10: the
  worked example of section 12).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`), "the engine"
  `bin\XR_3DA.exe` (1,636,424 bytes, MD5 `ba3ad0127bff6619ed991a7c82d8b946`) [GO-1].
- **The listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is
  established from the game library's registration code.
- An **engine object** is an object of the level as the engine keeps it (created from its spawn data when it
  comes online, offline-simulation.md 6.5); its **handle** is the `game_object` value scripts hold for it. A call is
  written as scripts write it: `obj:position()`, `h.power = 2`. A **number** is the runtime's double; a
  **text** is a Lua string; **nil** is Lua's nil.
- Class names are given as the game library registers them for scripts (`CAI_Stalker`, `CPhysicObject`, …);
  engine classes that scripts cannot name are described, not named ("the player's object").
- Each fact is **static** (from static analysis of the binaries, not observed at run time), **measured** (read
  from game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was
  run.

**Checks.** Every claim carries a tag [GO-*n*]. The checks are one script in the dirty workspace,
`checks/game-object-basics/run_all.py` (`python3 -I run_all.py`): it reads the binaries (bytes, run-time type
information), the evidence of read-only headless static analysis of them (produced by `gh.py` beside it), the
listing, the scoping spec's own scan and the shipped scripts and data. All checks pass (`ALL PASS 15`).

## 1. What this spec owns

### 1.1 Items (measured, static)

The scoping spec's split gives this spec 162 listed items [GO-2]:

| Owner | Items |
|---|---|
| `game_object` | the class; 46 functions (sections 3–7); the 5 properties `health`, `psy_health`, `power`, `radiation`, `morale` (5.1); 22 constants (section 8) |
| `hit` | the class; 6 members: `bone`, `direction`, `draftsman`, `impulse`, `power`, `type`; 10 constants (section 9) |
| `CGameObject` | the class; 7 functions `Visual`, `getEnabled`, `getVisible`, `net_Export`, `net_Import`, `net_Spawn`, `use` (section 10) |
| `explosive` | the class; `explode` (10.4) |
| client classes | 59 further classes (10.3), and the listed classes `ce_script_zone` and `ce_smart_zone` |

and, unlisted: the class `CPhysicObject` (10.3), the method `actor_sleep` and the base calls
`CGameObject.Load`, `CGameObject.shedule_Update`, `CGameObject.net_Destroy`; and the names `setEnabled`,
`setVisible`, `is_actor_sleeping`, which the scripts use and which occur nowhere in the game library (section
11) [GO-2]. The scoping spec assigns no candidate strings to this spec.

### 1.2 `game_object` as registered (static)

`game_object` is registered with no base class and **no constructor**: scripts cannot create a handle, they
only receive one from the engine [GO-3]. Against the listing: every listed function name but two is
registered, among them all 46 functions and the 5 properties above; the listed functions
`debug_planner` and `set_character_reputation` are not registered at all; and one registered name is not
listed, `invulnerable` (two forms, 5.5), which by function belongs to this spec [GO-3]. The class has exactly
these 5 properties: the registration defines no other readable or writable attribute, so there is no engine
property of `game_object` that the listing lacks (the scoping spec's gap 2) [GO-3].

## 2. The handle

### 2.1 What a handle is (static)

A handle refers to one engine object [GO-5]. Each engine object has at most one handle: it is made the first
time anything needs the object's handle and kept by the object; every later request returns the same one
[GO-5]. Members called on a handle act on its engine object at the time of the call; nothing is copied into the
handle [GO-4, GO-5].

How scripts obtain handles (from binders, callbacks, the level, the simulation registry and other members)
belongs to the specs owning those members (L, O, N, K and others). Members of this spec that return another
object (`parent`, `get_corpse`, `hit`'s `draftsman`) return that object's handle, or nil when there is none
[GO-7, GO-9].

When the engine object is destroyed (it goes offline or is removed), its handle is freed [GO-5]. A script that
still holds the handle then holds a value whose object no longer exists: what calling a member on it does is
**unestablished** (**believed** undefined: the freed handle is not marked for the runtime, which still refers
to it) [GO-5].

### 2.2 Kinds of object (static, measured)

Many members apply only to objects of a certain **kind**. The engine object answers a fixed set of kind
queries, each "none" for a plain object and the object itself (or a part of it) for an object of that kind; a
member asks one query and uses the answer [GO-4, GO-6]. The kinds, by the classes whose objects have them
(the registered client classes of section 10.3, and the player's object, whose class scripts cannot name)
[GO-6]:

| Kind | Classes whose objects have it |
|---|---|
| **entity** | the 19 creature classes (`CAI_Bloodsucker`, `CAI_Boar`, `CAI_Dog`, `CAI_Flesh`, `CAI_PseudoDog`, `CAI_Stalker`, `CAI_Trader`, `CBurer`, `CCat`, `CChimera`, `CController`, `CFracture`, `CPoltergeist`, `CPseudoGigant`, `CPsyDog`, `CPsyDogPhantom`, `CSnork`, `CTushkano`, `CZombie`); the player; also `CCar` and `CHelicopter` |
| **living** | the 19 creature classes and the player (not the car or the helicopter) |
| **inventory owner** | the 19 creature classes and the player |
| **fighter** | the creature classes except `CAI_Trader` (18) |
| **stalker** | `CAI_Stalker` only |
| **player** | the player's object only |
| **item** | the 36 item classes: the weapons `CWeaponAK74`, `CWeaponBM16`, `CWeaponBinoculars`, `CWeaponFN2000`, `CWeaponFORT`, `CWeaponGroza`, `CWeaponHPSA`, `CWeaponKnife`, `CWeaponLR300`, `CWeaponPM`, `CWeaponRG6`, `CWeaponRPG7`, `CWeaponSVD`, `CWeaponSVU`, `CWeaponShotgun`, `CWeaponUSP45`, `CWeaponVal`, `CWeaponVintorez`, `CWeaponWalther`; the grenades `CF1`, `CRGD5`; `CScope`, `CStalkerOutfit`, `CTorch`; the artefacts `CBastArtefact`, `CBlackDrops`, `CBlackGraviArtefact`, `CDummyArtefact`, `CElectricBall`, `CFadedBall`, `CGalantineArtefact`, `CGraviArtefact`, `CMercuryBall`, `CRustyHairArtefact`, `CThornArtefact`, `CZudaArtefact` |
| **physical** (has a physics part) | the creature classes, the item classes, the player, `CPhysicObject`; also `CCar`, `CHelicopter` and the hanging lamps of spec Y |
| **anomalous zone** | `CHairsZone`, `CMincer`, `CMosquitoBald` |
| **restrictor** | `CSpaceRestrictor`, the three anomalous-zone classes, `ce_script_zone`, `ce_smart_zone` |

A `CPhysicObject` object has exactly one of these kinds, physical [GO-6]. The player's object has the kinds
entity, living, inventory owner, player and physical [GO-6]. Two members test for one further class instead
of a kind: `active_zone_contact` (`ce_script_zone`, 6.4) and `set_invisible`, `set_manual_invisibility`
(`CAI_Bloodsucker`, 6.5) [GO-10].

### 2.3 A member on an object of the wrong kind (static)

When the kind (or class) a member needs is missing, the member does nothing to the object and returns the
fallback result its section gives (a number −1 or 0, `false`, nil, or (0, 0, 0)); the diagnostic text the code
passes on is handed to a routine that returns at once, so nothing is written anywhere [GO-5]. Exceptions, which
do not test the kind at all and so reach through a missing part (**believed**: the program crashes): the
anomaly members of 6.1 [GO-10].

## 3. Identity (static)

All are set when the engine object spawns from its spawn data (the simulation object or spawn record it is
created from, offline-simulation.md) [GO-7]:

| Member | Result |
|---|---|
| `id()` | the object's 16-bit id (0 … 65535): the id of the spawn data it was created from [GO-7] |
| `name()` | text: the object's name, which is the spawn data's object name when that is not empty, otherwise its section name [GO-7] |
| `section()` | text: the name of the object's configuration section (the spawn data's section) [GO-7] |
| `clsid()` | the number of the object's class id in spec C's numbering (spec C 5, 7), looked up from its section when it spawns [GO-7]; it compares with `clsid` constants |
| `story_id()` | the story id of the object's simulation part; −1 when it has none [GO-7] |
| `spawn_ini()` | a configuration object (spec F's `ini_file` class) loaded from the text the spawn data's simulation part holds (**believed** its custom data: the scripts read their `[logic]` and similar sections through it), as a root file whose includes resolve against the directory of `$game_config$` (**depends on F** for the loading and the readers); nil when that text is empty [GO-7]. The object belongs to the engine object, not to the script side: the same object is returned on every call while the engine object exists, and it is deleted when the engine object is destroyed [GO-7] |
| `parent()` | the handle of the object this one is attached to (for example the owner holding an item), or nil [GO-7] |

`name()` and `section()` return nil if the engine object has no such text (**believed** never: spawning always
sets both) [GO-7]. None of these members needs a kind.

## 4. Place (static)

| Member | Result |
|---|---|
| `position()` | a new vector: the object's position (the origin of its placement matrix, spec E's `c`) [GO-8] |
| `direction()` | a new vector: the object's forward axis, the `k` row of its placement matrix [GO-8] |
| `center()` | a new vector: the centre of the bounding sphere of the object's visual, carried by its placement matrix (`c + x·i + y·j + z·k` for the visual's centre (x, y, z)) [GO-8]; for an object without a visual, **believed** a crash (the visual is read without a test) |
| `bone_position(name)` | a new vector: the current position of the visual's bone `name` (its transform's origin) carried by the object's placement matrix; `name` empty: the visual's root bone [GO-8]. A name the visual lacks: **unestablished** (the bone lookup's "no bone" index is used unchecked; **believed** an invalid read) |
| `level_vertex_id()` | number: the object's level vertex (a vertex of the level's `level.ai`) as the engine keeps it; at spawn, the simulation object's level vertex when it is in range [GO-8]. How it changes as the object moves is **unestablished** here |
| `game_vertex_id()` | number: the object's game vertex (offline-simulation.md), kept and set at spawn the same way [GO-8] |
| `level_vertex_light(vertex)` | number: the light of level vertex `vertex` (the argument; the object is not used): the upper 4 bits of byte 11 of that vertex's 21-byte record, divided by 15, so 0 … 1 [GO-8] (**believed**: the records read are the `level.ai` records of offline-simulation.md section 3; byte 11 is among the bytes it leaves unestablished). A vertex beyond the level's: **unestablished** (read unchecked) |
| `set_actor_position(position)` | player only: the player's placement with its origin replaced by `position` is applied through the engine's forced-placement operation; nothing for any other object [GO-8] |
| `set_actor_direction(yaw)` | player only: the player's active camera is given the three angles (`yaw`, 0, 0) (**believed**: heading, pitch, roll, in radians, by the camera's angle setter); nothing for any other object [GO-8] |

The vector members return (0, 0, 0) and the vertex members their engine value only through a handle without an
object, which does not occur (2.1) [GO-8].

## 5. Condition (static)

### 5.1 The properties `health`, `psy_health`, `power`, `radiation`, `morale`

Living objects only [GO-4, GO-9]:

- Reading `obj.health` (and the others) returns the living object's current value of that condition (for the
  player, the conditions of player-state.md, **believed**); −1 for any other object [GO-9].
- **Assigning adds**: `obj.power = v` changes the value by `v`; it does not set it to `v` [GO-9]. The
  conditions are not clamped by the assignment itself; whether a later update clamps them is **unestablished**
  here. Assigning to an object that is not living changes nothing [GO-9]. **Disputed in review:** the
  assignment does not change the value that reading returns: for each of the five, the amount is added to a
  second value of the living object's conditions, separate from the one the read returns (static); so a read
  right after an assignment returns the old value. When and how that second value changes the condition is
  **unestablished** (**believed**: it is a pending change applied by the conditions' later update).
- `health` alone: a positive `v` is always added; a zero or negative `v` is added only while the object may be
  harmed, which the engine decides from a per-object flag together with a test of the game's network state
  (which states pass is **unestablished**); otherwise nothing changes [GO-9].

### 5.2 Entities

| Member | Result |
|---|---|
| `alive()` | `true` when the entity's health is above 0 (**believed** the same health as 5.1 for living objects); `false` for any other object [GO-9] |
| `max_health()` | number: the entity's maximum health; −1 for any other object [GO-9] |
| `death_time()` | number: the time the entity died, in the render device's milliseconds (spec B 3.3, **believed** its global time); 0 while it has not died, and for any other object [GO-9] |
| `kill(who)` | see below |

`kill(who)` (`who` a handle or nil) [GO-9]: on an entity whose death time is 0, the entity records as its killer
the id of `who`, or its own id when `who` is nil, sets its death time to the device's current time and records
the game time; unless the object is marked for destruction (the engine's "destroy" state), it then emits its death event carrying the killer's id
(**believed**: sent only where this side is the server; what a death event does is the death handling's,
**unestablished** here). If the killer is the object itself and a killer was already recorded, nothing happens.
On an entity already dead (death time not 0), and on any other object, nothing happens [GO-9].

### 5.3 Living objects

| Member | Result |
|---|---|
| `get_bleeding()` | number: the mean, over the wounds its conditions hold (**believed** the wounds of player-hits.md), of each wound's total, the sum of its 11 per-hit-type amounts; 0 with no wound; −1 for an object that is not living [GO-9] |
| `who_hit_name()` | text: the name (as `name()`) of the object the living object's conditions record as having hit it last (**believed** from the member's name), nil if none is recorded; nil for an object that is not living [GO-9] |
| `who_hit_section_name()` | text: that object's section name, likewise [GO-9] |

### 5.4 Items

| Member | Result |
|---|---|
| `condition()` | number: the item's condition (0 … 1); 0 for any other object [GO-9] |
| `set_condition(c)` | sets the item's condition to `c`, then clamps it to 0 … 1; nothing for any other object [GO-9] |
| `cost()` | number: the item's cost; 0 for any other object [GO-9] |
| `mass()` | number: the mass of the object's physics part (physical objects, 2.2; not only items); −1 for any other object [GO-9] |

### 5.5 Fighters and stalkers

| Member | Result |
|---|---|
| `invulnerable()` (unlisted) | `true` while the fighter's invulnerability flag is set; `false` otherwise and for any other object [GO-9] |
| `invulnerable(on)` (unlisted) | sets that flag; nothing for any other object [GO-9]. What the flag changes is the hit handling's (**unestablished** here) |
| `critically_wounded()` | `true` while the fighter has a critical wound recorded (its critical-wound state is not "none"); `false` otherwise [GO-9] |
| `get_corpse()` | the handle of the corpse the fighter's memory has selected, nil if none or if that corpse is marked for destruction; nil for any other object [GO-9] |
| `is_body_turning()` | `true` while the fighter's body heading differs from its target heading by at least 0.00001 (absolute difference); for a stalker also while a second heading of it does (**believed** its head's); `false` otherwise and for any other object [GO-9] |
| `wounded()` | stalker: `true` while its "wounded" flag is set; `false` for any other object [GO-9] |
| `wounded(on)` | stalker: sets that flag; when it changes, the stalker's behaviour is updated (the details are the stalker's AI, **unestablished** here) [GO-9] |

### 5.6 `hit(h)`

`obj:hit(h)` (`h` a `hit` object, section 9) delivers a hit to the object as the engine's **hit event**, sent
through the game's message path as any hit is [GO-13]. The event carries: the server time; the object's id as
its target; `h`'s power; `h`'s direction; `h.draftsman`'s id as the hitter and 0 as the weapon's id; the bone
named by `h:bone(…)` in the object's visual (bone 0 when no bone was named); a hit position (0, 0, 0) in the
bone's space; `h`'s impulse; `h`'s type; for type 8 (`fire_wound`) an armour-piercing value of 0; and a flag
"aimed bullet" false [GO-13]. What the hit then does is the hit handling's (player-hits.md for the player).
`h.draftsman` must be a handle: with nil (the default, section 9) the hitter's id is read through nothing
(**believed**: the program crashes) [GO-13]. No kind is required of the object.

## 6. Zones, restrictors, use and visibility (static)

### 6.1 Anomalous zones

| Member | Effect |
|---|---|
| `enable_anomaly()` | switches the zone to its state 0 (**believed** active) [GO-10] |
| `disable_anomaly()` | switches the zone to its state 4 (**believed** disabled) [GO-10] |
| `get_anomaly_power()` | number: the zone's power value [GO-10] |
| `set_anomaly_power(p)` | sets the zone's power value to `p` [GO-10] |

The meaning of the zone's states and of its power value is the anomalous zones' (**unestablished** here). These
four do not test the kind (2.3): on an object that is not an anomalous zone, **believed** a crash [GO-10].

### 6.2 Restrictors: `inside(position[, radius])`

Restrictors only; `false` for any other object [GO-10]. `radius` defaults to 0.001. The result is `true` when
the sphere (`position`, `radius`) overlaps the restrictor's shape [GO-10]:

1. if the distance from `position` to the centre of the restrictor's bounding sphere is at least that sphere's
   radius plus `radius`: `false`;
2. otherwise `true` if, for any sphere of the restrictor's shape (in level space), the distance from its
   centre to `position` is less than its radius plus `radius`;
3. otherwise `true` if, for any box of the shape (held as 6 planes, each a unit normal *n* and an offset *d*),
   every plane gives *n* · `position` + *d* ≤ `radius`;
4. otherwise `false`.

The level-space shapes are derived from the restrictor's spawn data and placement when first needed (how they
are derived is **unestablished** here) [GO-10].

### 6.3 Use: `set_nonscript_usable(on)`

Every game object has a **use flag**, `true` when the object is created [GO-10]. `set_nonscript_usable(on)`
sets it [GO-10]. The flag is read in exactly one place, the player's use action (**believed**: the routine run
when the player uses what is under the crosshair) [GO-10]. **Disputed in review:** "exactly one place" is
not backed: the check counts only the reads of the flag on the further object below; reads of the targeted
object's flag elsewhere were not searched, so whether the use action is its only reader is **unestablished**.
That action first calls the targeted object's use
(its scripted use, section 10.2 and the use callback of spec O), then:

- if the targeted object's flag is `false`, it does nothing more: the engine's own handling of the use is
  skipped;
- otherwise it continues with the engine's own handling: actions on a character the player targets, and, for
  a physical object under the crosshair, either an event asking to take hold of it (sent when that object
  allows it) or, while the left Shift key (DirectInput code 0x2A, public documentation) is held, a different
  handling for items [GO-10]. What these actions do is outside this spec.

One further object the player keeps targeted (**believed**: an inventory box) is tested first: when its flag
is `true`, the use opens that object's interface instead of all of the above [GO-10]. **Disputed in
review:** that object is not tested first: the targeted object's scripted use is called before it, and an
open interface replaces only what follows (the flag test of the targeted object and the engine's own
handling), not the scripted use (static).

So `set_nonscript_usable(false)` makes using the object do only its scripted use (**believed**, from the
single reader).

### 6.4 `active_zone_contact(id)`

`ce_script_zone` objects only: `true` when an object with id `id` is among the zone's current contacts (the
objects it tracks as touching it, **believed**); `false` otherwise and for any other object [GO-10].

### 6.5 Invisibility

- `set_invisible(on)`, `CAI_Bloodsucker` only: sets the creature's invisibility flag to `on` and makes it
  hidden (`on` true) or shown (`on` false) by the engine's visibility switch; nothing for any other object
  [GO-10].
- `set_manual_invisibility(on)`: does nothing on any object (on a bloodsucker it returns at once) [GO-10].

## 7. Trade parameters and character names (static)

Inventory owners only; for any other object nothing happens, and the name members return nil [GO-11]:

| Member | Effect |
|---|---|
| `sell_condition(ini, section)`, `buy_condition(ini, section)` | reads section `section` of the `ini_file` `ini` (**depends on F**) as the owner's selling (buying) item factors: each line `item section = a, b` gives the two numbers `a`, `b` for that item section (read as decimal numbers), and a line without a value is handled separately (**unestablished** how). A missing section is a failed check with the message `cannot find section %s` (**believed** fatal, as failed checks are, console-log.md 3.8) [GO-11] |
| `sell_condition(f1, f2)`, `buy_condition(f1, f2)` | sets the owner's two default selling (buying) factors to `f1`, `f2` [GO-11] |
| `show_condition(ini, section)` | reads section `section` of `ini` as the owner's list of item sections to show (**believed**): each line without a value is taken, lines with a value are skipped [GO-11] |
| `profile_name()` | text: the owner's character profile id, nil if it has none [GO-11] |
| `character_name()` | text: the owner's character name [GO-11] |

How the trade uses these factors and lists is the trade's (spec R). The two numbers' roles (which applies to
which goodwill) are **unestablished** here.

## 8. The constants of `game_object` (static)

The class holds 22 integer constants, the same values as the listing [GO-12]:

| Group (used with the members of) | Constants |
|---|---|
| path types (spec X) | `game_path` 0, `level_path` 1, `patrol_path` 2, `no_path` 3 |
| action kinds (spec V) | `movement` 0, `watch` 1, `animation` 2, `sound` 3, `particle` 4, `object` 5, `action_type_count` 6 |
| relations (spec Q) | `friend` 0, `neutral` 1, `enemy` 2, `dummy` −1 |
| relation events (spec Q) | `relation_kill` 0, `relation_attack` 1, `relation_fight_help_human` 2, `relation_fight_help_monster` 4 |
| messages (spec P) | `dialog_pda_msg` 0, `info_pda_msg` 1, `no_pda_msg` 2 |

What each value selects belongs to the member that takes it.

## 9. The class `hit` (static)

A `hit` object describes a hit for `game_object:hit` (5.6) [GO-13]. It has two constructors:

- `hit()`: `power` 100, `direction` (1, 0, 0), no bone, `draftsman` nil, `impulse` 100, `type` 3 (`wound`);
- `hit(other)`: a copy of `other`, its bone included.

| Member | Kind | Meaning |
|---|---|---|
| `power` | readable and writable number | the hit's power |
| `impulse` | readable and writable number | the impulse the hit gives |
| `direction` | readable and writable vector | the hit's direction; reading gives a new vector (spec E) |
| `draftsman` | readable and writable handle (**believed**: read as a game object) | the object the hit comes from |
| `type` | readable and writable number | the hit type (player-hits.md) |
| `bone(name)` | method | names the bone hit (by name; resolved in the target's visual when the hit is delivered) |

[GO-13]. Its constants, as the game library registers them: `burn` 0, `shock` 1, `strike` 2, `wound` 3,
`radiation` 4, `telepatic` 5, `chemical_burn` 6, `explosion` 7, `fire_wound` 8, and **`dummy` 11**: the
listing gives `dummy` 10, but the game library registers 11 [GO-13]. Every other value matches the listing.
No loadable script uses `hit.dummy` [GO-15].

## 10. `CGameObject`, the client classes and `explosive` (static)

### 10.1 `CGameObject`

`CGameObject` is registered with the bases the listing gives (spec B 5.1), a constructor taking no arguments,
and 8 methods: `_construct` (spec B 5.1), `Visual`, `getEnabled`, `getVisible`, `net_Spawn`, `net_Export`,
`net_Import`, `use` [GO-14]:

- `Visual()`: the object's visual, as spec B's `IRender_Visual`, or nil when it has none (**believed**: the
  engine's own accessor of the visual is registered as is) [GO-14].
- `getEnabled()`, `getVisible()`: the object's enabled and visible states, as the engine keeps them, numbers 1
  or 0 (**believed** returned as numbers: the engine's accessors return integers) [GO-14].
- `net_Spawn(data)`: spawns the object from the simulation object `data` (the engine's spawning of section 3);
  returns `true` when spawning succeeded [GO-14].
- `net_Export(packet)`, `net_Import(packet)`: do nothing in `CGameObject` itself [GO-14].
- `use(who)`: the object's scripted use: it calls the object's callback of id 15, `callback.use_object`
  (spec O), with the object's and the user's handles, and returns `true` [GO-14].

`Load`, `shedule_Update` and `net_Destroy` are **not** registered on `CGameObject` [GO-14].

### 10.2 The overrides the engine calls (the scoping spec's 4.5 row M)

For an object of a script class deriving from `CGameObject` (spec A), the engine looks up a script override for
exactly these engine calls, by spec A 5.2 (**depends on A** for the lookup and its errors) [GO-14]:

| Engine call | Override | When the engine makes the call |
|---|---|---|
| construction | `_construct` | spec B 5.1 / spec C |
| spawning | `net_Spawn(data)`; its result is taken as success | when the object spawns (section 3) |
| export, import | `net_Export(packet)`, `net_Import(packet)` | **unestablished** (**believed** the network update of the object; spec I owns packets) |
| use | `use(who)` | the player's use action (6.3) |

Every other engine call goes to the engine's own implementation; in particular overrides named `Load`,
`shedule_Update` and `net_Destroy` are never called by the engine [GO-14], and the base calls
`CGameObject.Load(self, …)`, `CGameObject.shedule_Update(self, …)`, `CGameObject.net_Destroy(self)` would find
no such member (spec A 4.4; a call of nil fails, **depends on A**).

The only shipped script class deriving from `CGameObject` is `ce_switcher.ce_switcher` (spec A 1.3), registered
for the class id `O_SWITCH` (spec C 3.3). The only section with that class id is `trigger` of
`config\misc\trigger.ltx`, and no spawn record of `spawns\all.spawn` has that section, so the class is
**believed** never instantiated in the shipped game [GO-15]. (Its `net_Spawn` calls the absent `setVisible` and
`setEnabled`, section 11.)

### 10.3 The client classes

Each of the 59 further listed client classes (the creature, item, zone and restrictor classes of 2.2 and
`CSpaceRestrictor`) and the unlisted `CPhysicObject` is registered under its name with `CGameObject` as its
only base, one constructor taking no arguments and no members of its own [GO-14]; its objects have the members
of `CGameObject` (spec A 1.3). `ce_script_zone` and `ce_smart_zone` are registered with `DLL_Pure` as their
base and one constructor taking no arguments [GO-10]. The class registrators (spec C 3.3) name 44 of the 59 listed client
classes and `CPhysicObject`; the 15 they do not name are the artefact classes other than `CElectricBall` (11),
`CF1`, `CRGD5`, `CWeaponFN2000` and `CWeaponFORT` [GO-14].

Calling a client class with no arguments (as the object factory does for the client side of a script-made
entry, spec C 6.2) constructs a **new engine object** of that class, not yet spawned, and returns it [GO-14] (static for
`CPhysicObject` and `CTorch`; **believed** for the others, whose registrations have the same form):
this answers spec C's review gap 1 for these classes. That object then spawns through the factory's normal
path (spec C 6.1).

### 10.4 `explosive`

`explosive` is registered with no base and no constructor, and one method, `explode()`, which starts the
explosion of the object's explosive part (**believed**; what an explosion does is outside this spec) [GO-14].
No member of this spec returns an `explosive` value; scripts call `explode` on handles (`game_object:explode`
belongs to spec Y).

## 11. Names the scripts use that are not registered (static, measured)

| Name | Script (loadable) | Registered? |
|---|---|---|
| `db.actor:actor_sleep(24, 0)` | `xr_effects.script` (`force_actor_sleep`) | no: the text `actor_sleep` occurs in the game library only as a callback constant (spec O) [GO-15] |
| `db.actor:is_actor_sleeping()` | `xr_conditions.script` | no; the name does not occur in the game library [GO-15] |
| `self:setVisible(1)`, `self:setEnabled(1)` | `ce_switcher.script` | no; neither occurs in the game library [GO-15] |
| `CGameObject.Load`, `CGameObject.shedule_Update`, `CGameObject.net_Destroy` | `ce_switcher.script` | no (10.2) |

A call of any of these finds nil and fails (**depends on A**, spec A 4.3). **Disputed in review:** spec A
4.1 and 4.3 cover script objects only (here `setVisible`, `setEnabled` and the base calls of
`ce_switcher`); `actor_sleep` and `is_actor_sleeping` are called on a game-object handle, which is not a
script object, and what reading an unregistered name on a handle gives is established neither by A nor here
(**unestablished**).

## 12. Worked example: the door scheme

The trader's door (`docs/behavior/physics-shells.md` 10) is a spawn record of section `physic_object`, class id
`O_PHYS_S`, whose client class is `CPhysicObject` (spec C 3.3) [GO-15]: its handle's object has the kind
physical only (2.2). The physics-object binder and the door scheme call, of this spec's members [GO-15]:

- `bind_physic_object.script`: `obj:name()`, `obj:spawn_ini()` (the door's custom data, an `ini_file`; its
  `section_exist` is spec F's), `obj:clsid()` (compared with `clsid.projector`, spec C), `obj:id()` (the key of
  the script's storage), and later `game_vertex_id()` and `position()` (section 3, 4);
- `ph_door.script`: `name()` (only as arguments of the script's own `printf`, which the script redefines to do
  nothing; the arguments are still evaluated), `position()` (where a sound is played at the door), and
  `set_nonscript_usable(false)` when its section starts (6.3: the player's use then does only the scripted use,
  so the engine's taking hold of the door is skipped).

None of these needs a kind, so all work on a `CPhysicObject` handle. The scheme's other calls belong to other
specs: `get_physics_shell` and the joint members (Y), `set_fastcall` (O), `set_tip_text` (P).

## 13. Use by the shipped scripts (measured)

By the scoping spec's scan (names attributed to every class declaring them, so the counts are upper bounds):
this spec's names are used by 139 loadable scripts (212 of all scripts) [GO-15]. The most used are `id` (79
loadable scripts), `name` (78), `position` (53), `alive` (31), `type` as a property (20), `spawn_ini` (18),
`level_vertex_id` (17) and `inside` (15) [GO-15].

## 14. Verification facts

All reproduced by `run_all.py` (15 PASS lines, no failure):

- Scope: 162 listed items (46 + 5 + 22 `game_object` members, 65 classes, 16 `hit` members, 7 `CGameObject`
  methods, `explode`), unlisted `CPhysicObject`, `actor_sleep`, 3 base calls, 3 absent names [GO-2].
- `game_object`: no base, no constructor, exactly 5 properties; 2 listed function names unregistered, 1
  registered name unlisted [GO-3]; 47 members paired with their implementations [GO-4].
- Kinds: 10 kind queries; 19 creature, 36 item, 3 anomalous-zone and 4 restrictor classes among the client
  classes [GO-6].
- Constants: 22 of `game_object` as listed [GO-12]; 10 of `hit`, `dummy` 11 against the listing's 10 [GO-13].
- `CGameObject`: 8 methods; the engine dispatches 4 named overrides (and construction) [GO-14]; 60 client
  classes with base and constructor [GO-14].

## 15. Unestablished

- Members called on a handle whose object was destroyed (2.1).
- The crash of the anomaly members on other objects, of `center` without a visual and of `hit` without a
  draftsman (believed); `bone_position` with an unknown bone; `level_vertex_light` beyond the level (4, 5.6, 6.1).
- Which network states let a negative `health` assignment through (5.1); later clamping of the conditions.
- What a death event, a hit event, the invulnerability flag, the zone states and power, and the stalker's
  wounded flag do (5, 6.1); how a stalker's `wounded(on)` updates its behaviour.
- How the level vertex and game vertex follow a moving object (4); how restrictor shapes are derived (6.2).
- The roles of the two trade factors (7); when the engine calls `net_Export` and `net_Import` (10.2).
- `Visual`, `getEnabled`, `getVisible` result conversions (believed, 10.1).
- Everything that depends on A (argument matching, calls of missing members, override lookup) and on F
  (`ini_file`).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the binaries identified above; the
  archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier approved specs
  (`scripts\*.script`, `config\misc\trigger.ltx`, `spawns\all.spawn`).
- Analysis: static analysis with Ghidra (headless, read-only) of the script registration of `game_object`,
  `hit`, `CGameObject`, `explosive` and the client classes in the game library, of the member implementations
  and the engine routines they reach (spawning and destruction of game objects, the handle's creation, the kind
  queries, restrictor shapes, the hit event, the player's use action, the override dispatch of script classes
  deriving from `CGameObject`), and of the engine's object accessors; run-time type information and data
  values read from the file bytes by a throwaway reader; the scoping spec's script scan reused; the listing and
  the scripts read as text. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

## Gaps found in review

1. The player's use action while the player is in a vehicle (or another holder) sends an event for the holder and returns at once: the targeted object's scripted use, the use flags and the engine's own handling are all skipped (static); 6.3 does not say so.
2. Whether the script binding of `spawn_ini()` leaves the returned configuration object to the engine object (so the runtime never deletes it) is not shown; 3 states only that the engine deletes it when the engine object is destroyed.
3. `obj:hit(h)` with a bone named in `h` on an object that has no visual: the bone is looked up in no visual (believed: a crash); not stated in 5.6.
