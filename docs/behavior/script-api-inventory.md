# Script API S: inventory

What the inventory part of the game scripts' interface does in S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian
edition), as scripts see it: the 29 functions of `game_object` that the scoping specification gives to this spec,
for items (counting, finding and iterating them), slots (the active slot, the item in a slot, the worn outfit),
money, moving items between owners and the world (dropping, transferring, eating, restocking), and ammunition;
with the entry points `iterate_inventory` and `inventory_for_each` (their arguments, order and failures). This is
spec S of the approved scoping specification `docs/behavior/script-api-scope.md` (section 6, order 25); its scope
rule, its section 4.5 row S and its review gaps apply: this spec owns every name of these members as the game
library registers them, listed or not.

Builds on (not repeated here):

- `docs/behavior/script-environment.md`, `docs/behavior/script-loading.md` (the runtime, one number type,
  namespaces, the 250 loadable scripts).
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md` (how arguments are matched,
  calls of missing members, what a mismatch does; its 5.3, the fatal `LUA error: %s`); spec M,
  `docs/behavior/script-api-game-object-basics.md` (the handle and how it is made, 2.1; the **kinds** of object,
  2.2; what a member does on an object of the wrong kind and the diagnostics routine that returns at once, 2.3;
  `invulnerable`, 5.5); spec B, `docs/behavior/script-api-engine-services.md` (the game type, its `IsGameTypeSingle`);
  spec D, `docs/behavior/script-api-engine-entry-points.md` (1.2, the call of a script
  function and its failure); spec F, `docs/behavior/script-api-configuration-and-files.md` (the class `ini_file`,
  a missing section, 2.4); spec C, `docs/behavior/script-api-class-ids-and-the-object-factory.md` (class ids);
  spec O, `docs/behavior/script-api-object-event-callbacks.md` (the callback id 15 `use_object`); spec R,
  `docs/behavior/script-api-dialogs-and-trade.md` (trade parameters, their item lists, the show list and the
  defaults, 5.2 and 5.3).
- `docs/behavior/player-state.md` (cited as `pstate`: the inventory's 11 slots, belt and backpack and where a
  taken item goes, 3.3; the active slot and the pending active slot, 3.3; eating, 5), `docs/behavior/use-action.md`
  (cited as `use`: inventory boxes, 5; the take request, its server log lines and event 1, and event 3, 9.2),
  `docs/behavior/player-hits.md` (hit types *t* 0 … 10, the outfit's protections *p_t* and condition *c*),
  `docs/behavior/character-identity.md` (specific characters, money), `docs/behavior/original-determinism.md`
  2.2 (the generator step), `docs/behavior/console-log.md` 3.8 (failed checks).

Conventions:

- "The game library" is `bin\xrGame.dll` (6,246,400 bytes, MD5 `19938b4b9fa11e3b4b6462ce7e7a12ac`) [S-1]. **The
  listing** is `scripts\lua_help.script` (scoping spec 2.1), a guide only; every member below is established from
  the game library's registration code.
- A call is written as scripts write it: `npc:item_in_slot(1)`. A **number** is the runtime's double; a **text** a
  Lua string; **nil** Lua's nil; a **handle** a `game_object` value (M 2.1).
- An **owner** is an object of the inventory-owner kind (M 2.2: the 19 creature classes and the player); its
  **inventory** is that of `pstate` 3.3 (slots, belt, backpack). An **item** is an object of the item kind; a
  **weapon** one of the 19 weapon classes `CWeapon…` of M 2.2 (the knife and the binoculars included). The
  **holder** of an item is the owner whose inventory holds it.
- **Fatal** means a failed check reported as `console-log` 3.8 describes, after which the program ends
  (**believed** for every failed check below, as R states for its own). **Diagnostics only** means the member does
  nothing (its fallback result, if any) and its text goes to the routine that returns at once (M 2.3).
- **Events** are the requests of `use` 9.2 that the program sends to the game's server, which handles them and
  sends the outcome back to the level (in single-player both run in one program): event 1, the take request
  naming the taker and the item; event 3, the drop of an item by its holder; and, below, event 8 (destroy an
  object), event 29 (put an item at a position) and event 32 (an owner's money). A member that sends an event
  changes nothing at once: the change happens when the outcome comes back (**believed**: within the next frames).
- Each fact is **static** (from static analysis of the game library, not observed at run time), **measured**
  (from game data or file bytes), **believed** (inferred; the reason is given) or **unestablished**. No binary was
  run.

**Checks.** Every claim carries a tag [S-*n*]. The checks are one script in the dirty workspace,
`checks/script-api-inventory/run_all.py` (`python3 -I run_all.py`): it reads the game library's bytes and run-time
type information, the evidence of read-only headless static analysis (produced by `gh.py` beside it; spec M's
registration evidence as M's checks produced it), the listing, the scoping spec's scan and the shipped scripts.
All checks pass (`ALL PASS 28`).

## 1. What this spec owns

### 1.1 Items (measured)

The scoping spec's split gives this spec 29 listed items, all functions of `game_object` [S-2]:

| Group | Members |
|---|---|
| items (section 3) | `object_count`, `object`, `iterate_inventory`, `inventory_for_each` |
| slots (section 4) | `item_in_slot`, `active_slot`, `active_item`, `activate_slot`, `get_current_outfit`, `get_current_outfit_protection` |
| moving items (section 5) | `drop_item`, `drop_item_and_teleport`, `transfer_item`, `mark_item_dropped`, `marked_dropped`, `eat`, `buy_supplies` |
| money (section 6) | `money`, `give_money`, `transfer_money` |
| ammunition (section 7) | `get_ammo_in_magazine`, `get_ammo_total`, `set_ammo_elapsed`, `unload_magazine` |
| other items (section 8) | `enable_attachable_item`, `attachable_item_enabled`, `is_inv_box_empty`, `best_item`, `best_weapon` |

and the entry points of the scoping spec's 4.5 row S, `iterate_inventory` and `inventory_for_each` (3.3, 3.4). The
scoping spec assigns this spec one candidate string, `invulnerable`: it is registered in the same registration
code, but it is M's member (M 5.5) [S-2, S-3]. The scripts use no unlisted or absent name of this spec (the
scoping spec's 2.2 lists none for S) [S-2]. Gap 2 of the scoping spec (engine properties the listing lacks) does
not arise here: all names of this spec are functions of `game_object`, whose properties M 1.2 gives in full.

### 1.2 As registered (static)

Each name is registered once on `game_object`, `object` twice (two forms); each form is paired with its own
implementation [S-3]. Arguments are matched by kind; what a call that matches no form does is A's [S-4]:

| Member | Arguments | Result |
|---|---|---|
| `object_count()` | — | whole number |
| `object(name)`, `object(i)` | a text; a number | handle or nil |
| `iterate_inventory(f, obj)` | a function or nil; any value | — |
| `inventory_for_each(f)` | a function or nil | — |
| `item_in_slot(n)`, `activate_slot(n)` | a number, **rounded** to an integer | handle or nil; — |
| `active_slot()`, `money()`, `get_ammo_in_magazine()`, `get_ammo_total()` | — | whole number |
| `active_item()`, `get_current_outfit()`, `best_item()`, `best_weapon()` | — | handle or nil |
| `get_current_outfit_protection(t)` | a number, made an integer | number |
| `drop_item(item)`, `mark_item_dropped(item)`, `eat(item)` | a handle | — |
| `marked_dropped(item)` | a handle | boolean |
| `drop_item_and_teleport(item, position)` | a handle; a `vector` | — |
| `transfer_item(item, to)` | two handles | — |
| `give_money(n)`, `set_ammo_elapsed(n)` | a number, made an integer | — |
| `transfer_money(n, to)` | a number, made an integer; a handle | — |
| `buy_supplies(ini, section)` | an `ini_file` or nil; a text | — |
| `unload_magazine()` | — | — |
| `enable_attachable_item(b)` | a boolean | — |
| `attachable_item_enabled()`, `is_inv_box_empty()` | — | boolean |

- "Rounded": to the nearest integer, then taken modulo 2³² (**believed** round-half-even, the processor's default
  rounding); so −1 gives 4,294,967,295, the value of "no slot" (section 4) [S-4]. "Made an integer": by the game
  library's conversion routine, **believed** truncation toward zero (as R 2.2) [S-4].
- A **whole number** result is a 32-bit value read as unsigned (0 … 4,294,967,295) [S-4].
- A handle result is nil when there is no object [S-4]. Where a handle argument is nil, the member's own
  test below applies (**depends on A** for whether nil matches a handle argument; the implementations test it).

## 2. Kinds, the inventory and the item list (static)

### 2.1 Kinds the members ask for

Besides M 2.2's inventory owner, fighter and stalker kinds, the members ask three kind queries of their own
[S-5]:

| Query | Objects that answer it |
|---|---|
| **item** | the 36 item classes of M 2.2, and also item classes scripts cannot name (checked: food, drink, medkits, antiradiation drugs, ammunition boxes, the PDA, the bolt, outfits; **believed** every item) |
| **attachable** | the same objects as the item query |
| **weapon** | the 19 weapon classes |

The **magazine weapons** are the 18 weapon classes other than `CWeaponKnife` (7.4) [S-5]. The **eatables** are the
eatable items of `pstate` 5 (food, drink, medkits, antiradiation drugs); no class scripts can name is one [S-5].

### 2.2 The inventory

Every inventory has the 11 slots, the belt and the backpack of `pstate` 3.3, no active slot when made, and per
slot two properties: **persistent** (`[inventory] slot_persistent_<n+1>`, the `pstate` 3.3 table) and **can be
active** (false for slots 6, 7, 8, 9) [S-6]. It also keeps a list of **all its items**, in every place, in the order
taken: each take appends the item to the end [S-6]; removing an item keeps the order of the rest (**believed**).
Whether the belt **counts** for 3.4 is a property of the inventory: false when made, set for the player (static:
the player's object sets it when made); no other owner is known to set it (**believed** false for all others)
[S-6]. Whether the slots **count** for 3.4 is a second property: true when made, set true again whenever an owner
reads its section's settings (the player's, a stalker's, the trader's and a monster's routines that do so; when
they run is **unestablished**), and set false only on a stalker's inventory, by the stalker routine that also sets
the dying-fire flag of 7.4 (**believed**: when the stalker dies) [S-9]. (An inventory a vehicle makes for itself
starts with it false; vehicles are not owners.) Each take also stamps the inventory as changed (7.2) [S-6].

Each item has a **dropped mark** (5.4), cleared when the item is made and whenever an inventory takes it [S-6,
S-16], and two section keys read when it loads: `can_trade` (true when absent) and `quest_item` (false when
absent), used by 3.4 [S-9].

## 3. Items (static)

### 3.1 `obj:object_count()`

The number of items in the owner's list of all items (2.2), every place counted [S-7]. 0 with diagnostics for an
object that is not an owner [S-7].

### 3.2 `obj:object(i)`, `obj:object(name)`

- `object(i)`: the handle of the item at position `i` of the owner's list of all items, counted from 0 (so
  `object(0)` is the item taken first). For `i` < 0 or `i` ≥ `object_count()`: nil, diagnostics only [S-7].
- `object(name)`: the handle of the first item in that list whose **section** name is `name` (compared by the
  CRC-32 of the text, **believed** equivalent to comparing the texts); nil when none [S-7].
- Both return nil with diagnostics for an object that is not an owner [S-7].

For example `self.npc:eat(self.npc:object("medkit"))` (`xr_wounded.script`) eats the first medkit the character
took, or, when it has none, calls `eat(nil)` (5.6) [S-25].

### 3.3 `obj:iterate_inventory(f, arg)` (entry point)

For each item of the owner's list of all items, in list order, `f` is called with **two arguments**: `arg` (the
value given, passed as it is) and the item's handle (made at the first need, M 2.1) [S-8]. The calls are protected:
an error raised in `f` calls the binding library's error callback, the fatal `LUA error: %s` of A 5.3, and the
program would end even if that callback returned (as D 1.2) [S-8]. Results are ignored. For an object that is not
an owner: diagnostics only, `f` is not called [S-8]. `f` = nil matches the argument but the call then reaches
through no function (**believed**: the program crashes) [S-4, S-8]. What `f` does to the inventory while the
iteration runs (for example releasing an item) is **unestablished** here; the shipped `keep_item` releases items
through the simulation (spec K), which takes effect later (believed).

### 3.4 `obj:inventory_for_each(f)` (entry point)

Calls `f` with **two arguments**, first the item's handle, then the handle of the owner `obj`, for each item the
owner **offers for trade**, in this order [S-9]:

1. the backpack, in its order;
2. the belt, only when it counts (2.2: the player);
3. the slots, only when they count (2.2; so a stalker whose slots no longer count, **believed** a dead one, offers
   no slot items), in slot order: the item of every slot that is not persistent (slots 1, 2, 6, 10 by `pstate`
   3.3), and an item whose slot is 3 (a grenade; **believed**: the slot its section's `slot` names) [S-9].

An item is offered when its `can_trade` is true, its `quest_item` false, and the owner allows it [S-9]. Which
items an owner allows [S-10]:

- the player, and every owner but stalkers and the trader: every item whose section is in neither its own nor the
  default trade parameters' item list that R 5.3's price routine tests first (one of the two parts of R 5.3;
  which part is **unestablished** here);
- a stalker with health 0 or less: the items its show list answers for (R 5.3); a live stalker: every item except
  those it keeps for itself (a choice made by the stalker's AI, **unestablished** here, spec X);
- the trader (`CAI_Trader`) with health 0 or less: every item; alive: never the PDA (class id `D_PDA`, C), the rest
  by the first rule.

The calls are protected, errors fatal and results ignored, as 3.3 [S-9]. For an object that is not an owner:
diagnostics only [S-9]. `f` = nil: as 3.3 (**believed** crash). The shipped scripts call it on the player and on
characters (`xr_effects.script`, to drop or transfer their items), with functions that take only the item [S-25].

## 4. Slots (static)

Slots are numbered 0 … 10 as in `pstate` 3.3; "no slot" is the value 4,294,967,295 [S-11]. All members of this
section apply to owners; for any other object they are diagnostics only with the fallback given.

| Member | Result |
|---|---|
| `obj:item_in_slot(n)` | the handle of the item in slot `n`, nil when the slot is empty; for `n` outside 0 … 10, nil (diagnostics only) [S-11]; nil for other objects |
| `obj:active_slot()` | the active slot's number, 4,294,967,295 when there is none [S-11]; 0 for other objects |
| `obj:active_item()` | the handle of the item in the active slot; nil when there is no active slot or it is empty [S-11]; nil for other objects |
| `obj:get_current_outfit()` | the handle of the item in slot 6 (the worn outfit), nil when empty [S-13]; nil for other objects |
| `obj:get_current_outfit_protection(t)` | for hit type `t`: 1 − (1 − *p_t* · *c*), with *p_t* the outfit's protection for that type and *c* its condition (`player-hits`); 0 when the item in slot 6 is not an outfit; 0 for other objects [S-13] |

The protections *p_t* are the outfit section's keys `burn_protection` (0), `shock_protection` (1),
`strike_protection` (2), `wound_protection` (3), `radiation_protection` (4), `telepatic_protection` (5),
`chemical_burn_protection` (6), `explosion_protection` (7), `fire_wound_protection` (8) and
`physic_strike_protection` (10, 0 when absent); type 9 is read from no key (its value is **unestablished**) [S-13].
`t` outside 0 … 10 is not tested (**believed**: a value read from beyond the table). On an owner whose slot 6 is
empty, `get_current_outfit_protection` reaches through no item (**believed**: the program crashes) [S-13].

### 4.1 `obj:activate_slot(n)`

Asks for slot `n` to become the active slot (the item in the hands), as the inventory's own slot switching does
[S-12]. In order:

1. In one marked frame (**believed**: the frame in which the inventory restores a saved active slot; when that
   happens is **unestablished**), only the slot recorded for it is accepted, and only when it holds an item;
   otherwise nothing happens [S-12]. Accepting it clears the mark, so the restriction holds for that one call:
   later calls in the same frame go on as for any other frame [S-12].
2. A slot whose block count is above 0 is refused (**believed**: the blocks of `use` 7.1's event 40) [S-12].
3. A slot number other than "no slot" outside the slots is a failed check (fatal) [S-12]. A slot that can never be
   active (6 … 9) is refused [S-12].
4. If `n` is already the active slot and holds an item, that item is activated again (**believed**: shown in the
   hands again if it was hidden); nothing else [S-12].
5. Otherwise, if `n` is already the pending slot and the active item answers that it is busy (**believed**: still
   putting itself away), nothing more [S-12].
6. With no active slot: an empty slot `n` is refused, except slot 3: then the first grenade of the backpack (an
   item whose section's `slot` is 3) is put into slot 3, which makes it pending and then active as `pstate` 3.3
   describes [S-12].
7. With an active slot: an empty slot `n` is refused (but "no slot" is accepted); otherwise the active item is
   told to put itself away [S-12].
8. `n` becomes the **pending** slot [S-12].

At the owner's next inventory update (**believed**: each frame while it is online) the pending slot becomes the
active slot once the active item is not busy [S-12]. So `activate_slot(-1)` (rounded to "no slot") puts the item in
the hands away, and `activate_slot(5)` (`xr_effects.script`) takes the bolt [S-4, S-25].

## 5. Moving items (static)

### 5.1 `obj:drop_item(item)`

Applies when `obj` is an owner and `item` an item; otherwise diagnostics only [S-14]. Sends event 3 for `obj`
naming `item` [S-14]. The server then [S-14]:

1. writes the log line `sv reject. id_parent <name> id_entity <name> [<frame>]` (**believed**: the owner's and the
   item's names, then the frame number);
2. a failed check (fatal) when either object does not exist on the server;
3. when the item has no holder: writes `~ ERROR: can't detach independant object. entity[%s:%d], parent[%s:%d],
   section[%s]` and does nothing more;
4. a failed check (fatal) when the item's holder is not `obj`;
5. otherwise the item gets no holder and leaves `obj`'s children, and the outcome goes back to the level, where
   the item is dropped into the world (`use` 9.2).

So `drop_item` on an item another owner holds ends the program (believed, by step 4).

### 5.2 `obj:drop_item_and_teleport(item, position)`

First exactly `drop_item(item)` (5.1, with its tests); then, whether or not that applied, event 29 naming `item`
with the three components of `position` [S-15]. The server passes event 29 on to the level unchanged [S-14].
What the item does with it is **unestablished** (**believed**: it is put at `position`).

### 5.3 `obj:transfer_item(item, to)`

Nothing (diagnostics only) when `item` or `to` is nil, or `item` is not an item [S-15]. Otherwise two events, in
this order: event 3 for `obj` naming `item` (5.1), then event 1, the take request of `use` 9.2, for `to` naming
`item` [S-15]. Neither `obj` nor `to` is tested for being an owner [S-15]. So, by 5.1 and `use` 9.2: an item `obj`
holds goes to `to`; an item lying in the world (no holder) only gives the error line of 5.1 step 3 and is then
taken by `to`; an item another owner holds ends the program (believed). What a `to` that is not an owner does with
a granted take is **unestablished**.

### 5.4 `obj:mark_item_dropped(item)`, `obj:marked_dropped(item)`

Both need `obj` to be an owner and `item` an item; otherwise diagnostics only (`marked_dropped` then gives `false`)
[S-16]. `mark_item_dropped` sets `item`'s dropped mark, whether or not `obj` holds it; `marked_dropped` returns the
mark [S-16].

At each inventory update (4.1), every item of its slots, backpack and belt whose mark is set loses the mark and,
when this side of the game runs the server (**believed** always in single-player), is dropped: event 3 for its
holder naming it (5.1) [S-16]. An item marked while no inventory holds it keeps the mark until an inventory takes
it, which clears it (2.2) [S-16].

### 5.5 `obj:eat(item)`

Nothing (diagnostics only) when `item` is nil or not an item, or `obj` is not an owner [S-17]. Otherwise [S-17]:

1. failed checks (fatal) when `item` is not in `obj`'s inventory, when it is not an eatable, and when `obj` is not
   living;
2. the eatable is used on `obj`: the eating of `pstate` 5 (its steps 1–8) on `obj`'s conditions (static for the
   routine; that a creature other than the player is affected the same way is **believed**);
3. when the game type is single-player or none (B's game type) and `obj` is the player: the player's callback 15
   `use_object` (O) is raised with the item's handle;
4. when the item has no portions left and `obj`'s object is controlled by this side (**believed** always in
   single-player): event 3 for `obj` naming the item, then event 8 (destroy) naming it.

### 5.6 `obj:buy_supplies(ini, section)`

Applies to owners; otherwise diagnostics only [S-18]. It restocks the owner from section `section` of the
`ini_file` `ini` (**depends on F**) [S-18]:

1. Every item in the owner's list of all items is destroyed (event 8, sent once per object), except the bolt
   (class id `II_BOLT`, C) and a PDA (class id `D_PDA`) whose original owner is `obj` [S-18].
2. The owner's earlier restock record is emptied [S-18].
3. The section is read (a missing section is fatal, F 2.4; `ini` = nil reaches through no file, **believed** a
   crash) and each line `item section = count, probability` is taken in the section's sorted order, the order of
   F 3.7's `r_line` (`ltx-config` 6: keys by unsigned byte comparison, not file order): `count` is the first
   comma-separated part read as a decimal integer, `probability` the second read as a decimal number [S-18].
4. For each line, a random generator of its own is seeded with the low 32 bits of the processor's performance
   counter; then `count` times, `count` taken as an unsigned 32-bit value (0 gives no tries; a negative `count`
   gives `count` + 2³², about 4 billion, tries): with *d* a draw of that generator (the step of
   `original-determinism` 2.2: *s* ← *s* · 214,013 + 2,531,011 mod 2³², *d* = (*s* ≫ 16) AND 32,767), when
   *d* / 32,767 ≤ `probability` an item of the line's section is created at the owner's position with the owner as
   its holder [S-18]. A section that names no item is a failed check (fatal) [S-18].
5. The restock record keeps, for the line's section, the value `count` / max(*k*, 0.3) · `probability`, *k* being
   the number of items created, both read as unsigned values (so a negative `count` counts as `count` + 2³²)
   [S-18]; what reads it is **unestablished** (**believed**: the trader's prices,
   #284).

So the items created are not reproducible from a saved state (static: the seed is the performance counter).
`trade_manager.script` calls it as `npc:buy_supplies(tt.config, str)` on a character that trades (a stalker or the
trader): `tt.config` is the `ini_file` of the character's trade file (the file its logic names under `trade`,
`misc\trade_generic.ltx` when none), `str` the section named by that file's `[trader]` key `buy_supplies`, a
condition list from which the script picks one section; it calls it again only when the picked section changes
[S-25]. Five shipped trade files have the key (`trade_barman`, `trade_dolg`, `trade_ecolog`, `trade_freedom`,
`trade_trader` in `config\misc\`) [S-25]. For example `[supplies_start]` of `trade_trader.ltx` is taken in the
order `ammo_5.45x39_fmj`, `antirad`, `bandage`, `bread`, `conserva`, `energy_drink`, `killer_outfit`, `kolbasa`,
`medkit`, `vodka`, `wpn_groza_m1`, not in the file's order [S-25].

## 6. Money (static)

An owner's money is a 32-bit unsigned value [S-19].

| Member | Effect |
|---|---|
| `obj:money()` | the owner's money [S-19] |
| `obj:give_money(n)` | sets the money to money + `n` [S-19] |
| `obj:transfer_money(n, to)` | sets `obj`'s money to money − `n`, then `to`'s to money + `n`; nothing (diagnostics only) when `to` is nil [S-19] |

The sums wrap modulo 2³² (a transfer of more than `obj` has gives it a very large amount; static) [S-19]. None of
the three tests that `obj` (or `to`) is an owner: on another object they reach through no owner (**believed**: the
program crashes) [S-19]. Setting an owner's money: when the owner's specific character has the attribute
`infinitive` of its `money` element set (read as an integer, non-zero = set), the money becomes the larger of the
old and the new value (so it never falls); otherwise the new value [S-19]. Each setting also sends event 32 for the
owner with the new money, and the server stores it as the owner's money on its side (**believed**: the money a
saved game holds) [S-19, S-14].

For example `dialogs.relocate_money` gives the player `num` with `give_money(num)`, or moves `num` from the player
to the partner with `db.actor:transfer_money(num, victim)` [S-25].

## 7. Ammunition (static)

### 7.1 `item:get_ammo_in_magazine()`

The number of rounds in the weapon's magazine; 0 for any other object [S-20].

### 7.2 `item:get_ammo_total()`

For a weapon, the rounds in its magazine plus, when it has a holder [S-20]:

- the rounds of every ammunition item in the holder's belt and backpack (not in slots) whose section is one of the
  weapon's ammunition types (**believed**: its section's `ammo_class` list), each counted at its current number of
  rounds;
- a further count the holder keeps beside a second item-section text when that text is set (what sets them is
  **unestablished**; **believed**: rounds the holder is yet to receive).

The part beyond the magazine is counted again only when the holder's inventory was changed (2.2) after it was last
counted; otherwise the last count is used [S-20]. 0 for any object that is not a weapon [S-20].

### 7.3 `item:set_ammo_elapsed(n)`

For a weapon, the magazine then holds `n` rounds: when `n` is more than it holds, rounds of the weapon's current
ammunition type are added until it holds `n`; when fewer, rounds are removed from the end [S-20]. `n` is not limited
to the magazine's size [S-20]; a negative `n` becomes a very large count (**believed**: the program runs out of
memory). Nothing for any other object [S-20].

### 7.4 `item:unload_magazine()`

For a magazine weapon (2.1): unless its holder is a stalker with its **dying-fire flag** set, all rounds are taken
out of the magazine and discarded (no ammunition item is made from them) [S-21]. For any other object: diagnostics
only [S-21]. The dying-fire flag is set at random (one chance in two, when a setting of the stalker allows it) by a
routine of the stalker (**believed**: when it dies); while it is set the stalker goes on firing for up to 500 ms
(**believed**) [S-21]. The same routine makes the stalker's slots no longer count for trade (2.2) [S-9].
`death_manager.keep_item` unloads the weapons of a dead character's slots 1 and 2 [S-25].

## 8. Other item members (static)

### 8.1 `item:enable_attachable_item(b)`, `item:attachable_item_enabled()`

For an item (the attachable query, 2.1); for any other object diagnostics only (`attachable_item_enabled` then
gives `false`) [S-22].

- With no holder: the item's **enabled** flag becomes `b` [S-22].
- With a holder that can carry attachments (only the player, stalkers and the trader can): `b` = true on a
  disabled item sets the flag, attaches the item to the holder (**believed**: shown on the holder's model at the
  bone and offset of its section's `attach_bone_name` and `attach_angle_offset`) and makes it visible; `b` = false
  on an enabled item clears the flag, detaches it and makes it invisible [S-22]. Otherwise nothing [S-22].
- `attachable_item_enabled()` returns the flag [S-22].

When an item whose section has `attach_angle_offset` loads, its flag is cleared (and the item detached if it was
attached) [S-22]; the initial flag of other items is **unestablished**. `sr_light.script` uses it for a
character's torch, `enable.script` for a radio [S-25].

### 8.2 `obj:is_inv_box_empty()`

`true` for an inventory box (`use` 5; class id `O_INVBOX`) that holds no item, `false` for one that holds any
and for every other object (no diagnostics) [S-23].

### 8.3 `obj:best_item()`, `obj:best_weapon()`

- `best_item()`: for a fighter (M 2.2), the handle of the item its item memory has chosen (spec X's), nil when
  none; nil for any other object, without diagnostics [S-24].
- `best_weapon()`: for a stalker, nil when its health is 0 or less; otherwise the handle of its **best weapon**,
  nil when none [S-24]. The best weapon is kept while it stays usable and its rating is unchanged (within 0.00001);
  otherwise it is chosen again among the weapons of its inventory by the AI's weapon rating, and, failing those,
  with the help of items its item memory knows of (**believed**: a weapon it could load with ammunition it has
  seen) [S-24]; the rating and that choice are **unestablished** here (spec X). For any other object: nil,
  diagnostics only [S-24].

## 9. Worked example: a dead character's items

`death_manager.drop_manager:create_release_item` runs once per dead character (it marks the simulation object)
and calls `self.npc:iterate_inventory(keep_item, self.npc)` [S-25]. By 3.3, `keep_item` is called once per item of
the character's list of all items, in the order taken, with the character's handle and the item's handle. For an
item it keeps in slot 1 or 2 (`npc:item_in_slot(1)`, `item_in_slot(2)`, 4) it calls `item:unload_magazine()`, which
discards the rounds of a magazine weapon (7.4); other items it releases through the simulation. Later
`check_item_dependence` uses `npc:object(k)` (3.2) and `npc:marked_dropped(obj)` (5.4) to test whether the
character still has an item a spawned item depends on [S-25].

## 10. Use by the shipped scripts (measured)

By the scoping spec's scan, 21 of the 29 names are used: by 26 loadable scripts counting only names no other spec
has, 54 counting also `object`, which classes of other specs declare too (68 of all scripts) [S-2, S-25]. Loadable scripts (all
scripts) per name [S-25]: `object` 41 (51), `money` 10 (10), `active_item` 8 (10), `best_weapon` 3 (8),
`enable_attachable_item` 3 (5), `activate_slot`, `item_in_slot`, `transfer_item` 3 each, `transfer_money` 2,
`get_ammo_in_magazine` 1 (2), and 1 each `active_slot`, `buy_supplies`, `drop_item`, `drop_item_and_teleport`,
`eat`, `give_money`, `inventory_for_each`, `is_inv_box_empty`, `iterate_inventory`, `marked_dropped`,
`unload_magazine`. Used by none: `attachable_item_enabled`, `best_item`, `get_ammo_total`, `get_current_outfit`,
`get_current_outfit_protection`, `mark_item_dropped`, `object_count`, `set_ammo_elapsed` [S-2].

## 11. Verification facts

All reproduced by `run_all.py` (28 PASS lines, no failure):

- Scope: 29 listed items, the candidate `invulnerable` (M's), 21 used names, 26 (54) loadable scripts [S-2]; 30
  registered forms paired with implementations [S-3].
- Arguments and results: number, text, boolean, handle, vector, function-or-nil, any-value and ini-or-nil
  arguments; rounding for slot numbers, truncation elsewhere; unsigned whole-number, handle-or-nil and boolean
  results [S-4].
- Kinds: item and attachable queries answered by every item, the weapon query by 19 classes, 18 magazine weapons
  [S-5]; the inventory's slot properties, its item list and the belt flag [S-6].
- Members: items and iteration, with `inventory_for_each`'s two arguments and the slots' trade property and
  its writers [S-7 – S-10]; slots and the outfit with its 10 protection keys, and the single call the marked
  frame admits [S-11 – S-13]; the drop, the server's handling of events 1, 3, 8, 29 and 32 [S-14 – S-16]; eating
  [S-17]; restocking with its generator (multiplier 214,013, increment 2,531,011, scale 1/32,767, floor 0.3), the
  unsigned count and the section's sorted line order [S-18]; money with `infinitive` [S-19]; ammunition [S-20,
  S-21]; attachable items, boxes, best item and weapon [S-22 – S-24].
- Script use, the worked example, and `buy_supplies`' caller, its trade files and the sorted order of
  `[supplies_start]` [S-25].

## 12. Unestablished

- What an item does with event 29 (5.2; believed moved to the position); what a non-owner does with a granted
  take (5.3).
- Which part of the trade parameters the default trade permission tests, and a live stalker's own keep choice
  (3.4); what reads the restock record (5.6; believed the trader's prices).
- The holder's further count in `get_ammo_total` and what sets it (7.2).
- When the inventory restores a saved active slot (4.1); the protection value of hit type 9 (4).
- The initial enabled flag of attachable items without `attach_angle_offset` (8.1); the AI's weapon rating and the
  item memory's choice (8.3, spec X); the conditions of the dying-fire flag (7.4).
- What `iterate_inventory`'s function may do to the inventory during the iteration (3.3).
- Everything that depends on A (argument matching, nil for handle arguments, calls of missing members).

## Provenance

- Date: 2026-11-28.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); the game library identified above;
  the archive file tree of `gamedata.db0` … `gamedata.dbd` as extracted for earlier approved specs
  (`scripts\*.script`, `config\` item sections).
- Analysis: static analysis with Ghidra (headless, read-only) of the registration of these `game_object` members
  and of their argument matching and conversions; of the member implementations and the engine routines they
  reach (the inventory's item lists, slots and slot switching, its update, taking items, the item's section keys,
  trade permissions, eating, the restock routine and its spawning, money and its event, the server's handling of
  the drop, take, destroy, position and money events, the weapon's magazine and ammunition counting, unloading,
  attachments, the inventory box, the best item and weapon); run-time type information read from the file bytes;
  the scoping spec's script scan reused; the listing and the scripts read as text. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used.

