# Character items and money

Behaviour specification, for S.T.A.L.K.E.R.: Shadow of Chernobyl (Russian edition), of the items and money of the
characters (stalkers and the trader) and of the items monsters carry: what they start with beyond what earlier
specifications settle, their money, how much they may carry, what a stalker does when it takes an item (the
single-item rule), which of its items a live stalker keeps for itself (`can_select_items`), the items shown on a
character's body (`attachable_items` and the item keys `attach_*`) and when such an item is hidden again, the keys
of this area the game does not read, the items a dead stalker leaves (`death_manager` and `config\misc\death_*.ltx`)
and the trade timers of `trade_manager`. It is CH-6 of the approved scoping specification
`docs/behavior/characters-scope.md` (section 5; its section 2.1 item "Items and money", B-chars 6 of its section 3
and its "Gaps found in review" apply). It ends with a worked example: a novice at the campfire of the beginners'
village on `l01_escape` playing the guitar (`guitar_a`).

Builds on (cited, not repeated):

- `docs/behavior/characters-scope.md` (cited as `scope`): the records, sections and conventions ("character",
  "monster", "named in `xrGame.dll`").
- `docs/behavior/script-api-inventory.md` (cited as S): the members of `game_object` for items, slots, money and
  ammunition; its conventions **owner**, **item**, **weapon**, **holder**, **events** (event 1 the take request,
  event 3 the drop, event 8 destroy) and **fatal**; its 2.2 (the inventory, its list of all items, the **dropped
  mark**, the belt's "counts" property), 3.2 (`object`), 3.3 (`iterate_inventory`), 3.4 (what an owner offers for
  trade), 5.4 (the dropped mark and the drop at the inventory update), 6 (money, `infinitive`), 7.4
  (`unload_magazine`), 8.1 (`enable_attachable_item`, `attachable_item_enabled`), 8.3 and 8.4 (the item memory,
  the weapons a stalker picks up and carries).
- `docs/behavior/new-game-supplies.md` (cited as `supplies`): the supplies step of every kind, the `[spawn]` texts,
  the fragments' probabilities, the per-object random generator (its 2), the ids (its 3), and its section 4 (the
  novices of the beginners' village and their supplies).
- `docs/behavior/character-identity.md` (cited as `identity`): the money of the Trader-like block and of a specific
  character (its 1.1, 3 step 7), the shared random generator.
- `docs/behavior/player-state.md` (cited as `pstate`): the inventory's places (3.3; an item's place 1 slot, 2 belt,
  3 backpack), the carried weight and an item's weight (3.4).
- `docs/behavior/torch.md` (cited as `torch`): its section 6 (attachment: the owner's conditions, when the owner
  attaches and detaches, the transform of an attached item) and section 10 (characters' torches).
- `docs/behavior/script-api-object-event-callbacks.md` (cited as O): callbacks 27 `on_item_take`, 28 `on_item_drop`,
  29 `script_animation`.
- `docs/behavior/script-api-simulation-objects.md` (cited as J): 5.4 `on_spawn`, 5.5 release.
- `docs/behavior/script-api-dialogs-and-trade.md` (cited as R): trade parameters, `buy_condition`, `sell_condition`.
- `docs/behavior/original-determinism.md` (cited as `determinism`): 2.2 the shared generator, 3 the scripts'
  `math.random`.
- `docs/formats/sav.md` (cited as `sav`): 7.4 the Trader-like block, 7.4.2 the stalker's preference arrays.
- `docs/formats/ltx-config.md` (cited as `ltx`): sections, parents, includes.

Conventions:

- **Character**, **monster**: as `scope`. **Holder**, **item**, **weapon**: as S. A weapon's **type** is its
  section's `ef_weapon_type` (unsigned; S 8.3).
- **Attached**: an item hangs on one of its holder's bones and is drawn with it (section 6).
- Every statement is **static** (from static analysis of `bin\xrGame.dll` and `bin\xrCore.dll`, or read from the
  game data and the shipped scripts); the game was not run. **Believed** marks an inference (with its reason),
  **unestablished** what could not be settled.
- Bracketed labels [CI-*n*] are the checks: assertions of `checks/character-items/checks.py` in the dirty
  workspace (`python3 -I checks.py`; it prints `ALL PASS 25`).

## 1. The data

The 164 character and monster sections of `scope` 1.2 (parents merged) carry these keys of this specification
[CI-2]:

| Key | Sections and values |
|---|---|
| `attachable_items` | 86 stalker sections: `device_torch,attachable_item,hand_radio,bread, vodka, energy_drink, kolbasa, guitar_a, harmonica_a, wpn_binoc, kruglov_flash`; 8 stalker sections: `device_torch,attachable_item`; `m_trader` and `m_izgoy`: empty |
| `can_select_items` | stalker sections: `on` 91, `off` 3 |
| `use_single_item_rule` | stalker sections: `on` 91, `off` 2, `false` 1 |
| `inv_max_weight` | every stalker section: 1000.0 |
| `max_item_mass` | stalker sections 1000.0, `m_trader` 10000.0, `m_izgoy` 40.0 |
| `cost` | `m_trader`: 1 |
| `supplies` | `m_trader`: `grenade_f1,15,0,1.0, ammo_5.45x39_ap,…` (a list of item sections and numbers) |
| `Spawn_Inventory_Item_Section`, `Spawn_Inventory_Item_Probability` | 46 monster sections: one of `mutant_boar_leg`, `mutant_burer_hand`, `mutant_dog_tail`, `mutant_flesh_eye`, `mutant_krovosos_jaw`, `mutant_psevdodog_tail`, `mutant_snork_leg`, `mutant_zombie_hand`; probability 0.2 in all |
| `money` | `identity` 1.1 |

The keys of this table, the three `attach_*` keys and `additional_inventory_weight2` are named in `xrGame.dll`;
`auto_attach` (in the item sections, always `false`) is not, and is believed not read [CI-2].

Each of the 11 items of the long `attachable_items` list has a section with `attach_bone_name`,
`attach_angle_offset` and `attach_position_offset` [CI-3]:

| Item | Class id | `attach_bone_name` |
|---|---|---|
| `device_torch`, `attachable_item` | `TORCH_S`, `II_ATTCH` | `bip01_head` |
| `hand_radio`, `guitar_a`, `harmonica_a`, `kruglov_flash` | `II_ATTCH` (three), `D_SIMDET` | `bip01_l_hand` |
| `bread`, `kolbasa`, `vodka`, `energy_drink`, `wpn_binoc` | `II_FOOD` (two), `II_BOTTL` (two), `WP_BINOC` | `bip01_r_hand` |

60 item sections of the configuration have `attach_angle_offset` [CI-3]. `guitar_a`: `attach_angle_offset` =
−1.651084, −1.341824, 1.200789; `attach_position_offset` = 0.077492, −0.030020, 0.090059; `visual` =
`Equipments\item_guitar.ogf`; `inv_weight` 1.5; `cost` 35 [CI-3].

## 2. Starting items

What a character and a monster are created with at a New Game, and in which order and with which draws, is
`supplies` 1–3 (a character's specific-character `supplies`, the record's custom data `[spawn]`, the fragments'
probabilities) [`supplies`]. This specification adds:

- **A monster's item.** After a monster of an `SM_` class id is created and its supplies step has run, its
  creation step (J 5.4, `on_spawn`) creates at most one item: when its section has `Spawn_Inventory_Item_Section`,
  one draw *r* is taken from the **monster's own generator** (`supplies` 2: the same per-object state and stepping
  as its supply draws), and an item of that section is created when *r* · (1/32,767) < *p*, or when |*p* − 1| <
  0.00001, *p* being `Spawn_Inventory_Item_Probability` [CI-20]. The item is created as the monster's child at the
  monster's position, level vertex and game vertex, registered like a supply item [CI-20]. With *p* = 0.2 the item
  is created for 6,554 of the 32,768 values of *r* [CI-20] (the comparison is made in the processor's
  floating-point unit; that a higher-precision product changes no count for 0.2 is **believed**). This runs for
  every monster created by the simulation (at a New Game for every monster record, and for monsters created in
  play; J 5.4), not for monsters read from a save. The monsters' supplies step itself creates nothing (it is empty
  for the `SM_` class ids' engine kind [CI-20]; **believed** also for their script classes, which J does not let
  override it); no shipped monster record has a `[spawn]` custom data (`supplies` 1.2).
  246 of the 276 monster records have such a section; on `l01_escape` 48 `mutant_dog_tail`, 10 `mutant_boar_leg`,
  8 `mutant_flesh_eye` and 2 `mutant_psevdodog_tail` [CI-21]. At a New Game the item, when created, takes the
  next id right after the monster's object (`supplies` 3.2's sequence, with this item after the monster's
  supplies) [CI-20].
- **Preferences.** Each stalker's simulation object keeps 5 equipment preferences and 4 main-weapon preferences
  (`sav` 7.4.2). When its simulation behaviour is made (**believed**: when the object is made), each of the 9 is
  set to *a* mod 3 with one draw *a* of the **shared** generator (`identity`, `determinism` 2.2), the 5 first
  [CI-19]; the program requires the two counts to be 5 and 4 (a failed check otherwise) [CI-19]. They are the
  inputs of two evaluation functions of the simulation (equipment and main-weapon preference), believed to rate
  items offline; what reads those functions, and whether a saved object's arrays replace the drawn values, is
  **unestablished**.
- The keys `supplies` and `cost` of a character's section are not read (section 7).

## 3. Money

A character's money is the Trader-like block's (`sav` 7.4), set at creation as `identity` 1.1 and 3 step 7 state.
The simulation object also reads its section's `money` (when present) and `max_item_mass` when it is made
[CI-15]. Online, the money is the owner's (S 6: its members, the wrapping, `infinitive`, event 32). Dialog scripts
ask for and move items and money through the speaker object's S members (`object`, `money`, `transfer_item`,
`transfer_money`, `give_money`; S 3.2, 5.3, 6) and R's trade members; this specification adds nothing to them
(`scope` B-chars 6).

## 4. Carrying limits

- A **stalker's carrying limit** is its inventory's maximum weight, which the stalker's section's `inv_max_weight`
  sets when the stalker loads its section (when the key is present), plus the worn outfit's
  `additional_inventory_weight2` when it wears one [CI-15]. Shipped: 1000.0 for every stalker section.
- The **trader's** maximum weight is its section's `max_item_mass` × 1000 (10,000,000 for `m_trader`); it wears no
  outfit (believed) [CI-15].
- **A take is refused** when the item is marked by a flag whose meaning is **unestablished** (believed: an object
  being destroyed), when the item's own take test fails (S 8.3's `can_take`, believed), or — for every holder but
  the player — when the inventory's carried weight plus the item's weight (`pstate` 3.4) exceeds the holder's
  carrying limit [CI-15]. The player is never refused for weight here [CI-15].
- The stalkers' `max_item_mass` is read only into the simulation object (section 3); what uses it there is
  **unestablished**.

## 5. Taking an item; the single-item rule

When a stalker's take request (event 1, S) arrives [CI-16]:

1. if the take is refused (section 4), the stalker sends event 3 naming the item (the item is not taken);
2. otherwise the item becomes the stalker's child and is taken into the inventory (its place by `pstate` 3.3);
   then (**unestablished** here) a weapon may be made the active item when the stalker has none;
3. then the **single-item rule**: when the stalker is alive (health > 0), its section's `use_single_item_rule` is
   true or absent, and the taken object is a weapon (the 19 weapon classes, the knife and the binoculars included;
   grenades are not) [CI-17]: for every item *I* of its list of all items (S 2.2) that is a weapon other than the
   taken weapon *W* and is **comparable** with it [CI-17]:
   - its ammunition is given up: for each of *I*'s ammunition types (S 7.2) that no carried weapon other than *I*
     (the taken *W* included) has among its ammunition types, every carried item of that section is destroyed
     (event 8) [CI-17];
   - *I* gets the dropped mark, so the next inventory update drops it (S 5.4) [CI-17].

*I* is comparable with *W*, by the type *t*(*I*): *t*(*I*) = 1 or 5: when *t*(*W*) = *t*(*I*); 6, 7 or 8: when
*t*(*W*) is 6, 7 or 8; 9 or 10: when *t*(*W*) = *t*(*I*); any other value (0, 2, 3, 4, above 10): always [CI-17].
So a stalker that picks up a pistol (type 5) drops the pistols it carried, and one that picks up any weapon drops
carried binoculars (`wpn_binoc`, type 0). Which weapons a stalker decides to pick up is S 8.3 and 8.4 and its
planner's (`scope` CH-17).

## 6. What a live stalker keeps for itself (`can_select_items`)

A stalker offers for trade every item it does not keep (S 3.4); when it is dead, its show list decides instead
(S 3.4) [CI-18]. Which items it **keeps** is computed once and then reused (when it is computed again is
**unestablished**) [CI-18]:

1. The **budget** is the stalker's money plus the sum of the prices (the item sections' `cost`, read when the item
   loads) of its **selectable** items [CI-18]. An item is selectable when an answer of the item that is
   **unestablished** holds and a flag of the item that is **unestablished** is set, it is not a PDA whose original
   owner is this stalker, and a further test on its section (believed: the trade parameters' item lists, R) holds
   [CI-18]. The selectable items form a list, sorted by object id [CI-18].
2. When the stalker's section has `can_select_items` true [CI-18], for each of four **groups** of weapon types in
   this order — type 1; type 5; types 6, 8 and 9; type 7 — the selectable item with the greatest value of the
   group's evaluation function among those whose price does not exceed the budget is kept (the first of equal
   ones; none when no value exceeds −1), and its price is taken from the budget [CI-18]; after each group up to 10
   further items whose sections are among the kept item's ammunition types and whose prices fit the budget are
   kept likewise (**believed**: its ammunition) [CI-18]. Then the detector with the greatest value of a fourth
   evaluation function among those within the budget is kept [CI-18]. Which evaluation-function files the four
   functions are, and the exact type test (believed the item's `ef_weapon_type` for a weapon), are
   **unestablished**.
3. Every carried item that is not selectable is kept [CI-18].

With `can_select_items` false (3 sections) only step 3 keeps items, so such a stalker offers all its selectable
items.

## 7. The trader's `cost` and other keys not read

The game library reads `cost` only as an item's price (the item loaded on the level and the item's simulation
object) [CI-22]; the `cost` of `m_trader` is believed not read. A `supplies` key is read only from the character
descriptions' XML (`identity`, CH-1), so the `supplies` list of `m_trader`'s section is believed not read [CI-22].
`auto_attach` is believed not read (section 1).

## 8. Items shown on the body

`torch` 6 specifies attachment for every owner: the conditions under which an owner attaches an item, that the
owner tries to attach an item when it takes it and when the item is put into a slot and detaches it when the item
goes to the backpack or is dropped, what attaching and detaching do, and the transform of an attached item
[`torch` 6]. S 8.1 specifies `enable_attachable_item`. This section adds what applies to characters and to items
other than the torch.

### 8.1 The holder's list

A holder reads its section's `attachable_items` when it loads its section: the text split at commas, each item
with the characters up to and including space removed from both ends; an empty text gives an empty list, an
absent key empties the list [CI-4]. So the stalkers' list holds the 11 items of section 1 (the spaces after its
commas are dropped), the trader's is empty [CI-4, CI-2].

### 8.2 The item

- Every item is made **enabled**, with an identity offset and no bone name [CI-5].
- When an item whose section has `attach_angle_offset` loads its section, it reads `attach_angle_offset`,
  `attach_position_offset` and `attach_bone_name`, and its enabled flag is cleared (the item is detached first if
  it is attached) [CI-5] (S 8.1). An item of another section keeps the identity offset, no bone and the flag set
  [CI-5]. **Disputed in review:** the flag is cleared only when the item has no holder, or when it is enabled and
  its holder is one of those that have the attachment view of 8.3 (player, stalkers, trader); in that second case
  the holder is told to detach it and the item is also made invisible. An item held by any other holder (a monster)
  keeps its flag.
- The **offset** *O* (rows 0–2 the rotation, row 3 the translation, last column 0, 0, 0, 1) from the angles (o₀,
  o₁, o₂) of `attach_angle_offset`, with sx = sin o₀, cx = cos o₀, sy = sin o₁, cy = cos o₁, sz = sin o₂, cz =
  cos o₂, is [CI-6]:

      O[0] = ( cz·cx − sz·sx·sy,   −sz·cy,   sz·cx·sy + cz·sx )
      O[1] = ( cz·sx·sy + sz·cx,    cz·cy,   sz·sx − cz·cx·sy )
      O[2] = ( −cy·sx,               sy,     cy·cx            )
      O[3] = ( attach_position_offset )

  (the same as `torch` 6) [CI-6].
- The item's **own test** (an item that is not a torch): it is in an inventory, and either that inventory's belt
  does not count (S 2.2: every holder but the player) or the item is on the belt (`pstate` 3.3, place 2) [CI-8].
  So a character's item may hang on it from any place; a player's only from the belt. (The torch's own test is
  `torch` 6.)

### 8.3 Which holders accept an item

`enable_attachable_item` finds the holder through a view of it that only the player, the stalkers and the trader
have; for an item a monster holds it finds none and changes nothing (S 8.1's "otherwise nothing") [CI-9]. A
monster's own attempts (8.4) accept nothing, since no monster section has `attachable_items` (8.1) [CI-2, CI-8]. A
holder **accepts** an item when [CI-8]:

| Holder | Accepts |
|---|---|
| stalker | when its corpse-model flag (below) is clear: the item is enabled, the item's own test holds, its section is in the holder's list, and no item of the same section is attached (an attached item with the dropped mark, or marked by the flag of section 4, is not counted) |
| trader | never |
| player | as the stalker's rule without the corpse-model flag and without asking whether the item is enabled |

The stalker's **corpse-model flag** is cleared each time the stalker's model is chosen when it is made on the level,
and set when the stalker is dead (health ≤ 0), has no story id, its death time is set, and the game time is more
than `[monsters_common]` `corpse_remove_game_time_interval` (36) game hours after its death time; its model is then
its section's `corpse_visual` [CI-14]. (How the death time is kept is `scope` CH-5's and CH-27's.)

### 8.4 When an item is attached and detached

The holder tries to attach (attaching only what it accepts, 8.3) [CI-11]:

- when it takes the item (after raising O's callback 27), and when the item is put into a slot;
- when `enable_attachable_item(true)` is called on a disabled item it holds (the flag is set first) [CI-10].

The holder detaches the item [CI-11]:

- when the item is moved to the backpack;
- when the item is dropped (after raising O's callback 28): every way an item leaves an inventory through its drop
  (S 5.1, 5.3, 5.4, 5.5; believed also the release of J 5.5);
- when `enable_attachable_item(false)` is called on an enabled item (the flag is cleared first) [CI-10];
- when the item loads a section with `attach_angle_offset` while attached (8.2).

Putting an item on the belt neither attaches nor detaches it [CI-11]. After an item has left its holder, its flag
is cleared [CI-12]. A stalker also switches an attached torch on when it attaches it while alive and off before it
detaches it (`torch` 10) [CI-11].

Attaching [CI-7]: an item already attached (same object id) is left alone; otherwise, if the holder accepts it, its
bone is looked up by its bone name in the holder's model, it is appended to the holder's attached items, it is made
visible and its own updates are switched on. With the first attached item the holder's skeleton gets a callback that
places the attached items each time the skeleton's bones are computed. Detaching [CI-7]: the item is removed from
the list and its updates are switched off; when it was the last one the callback is removed and the item is made
invisible. When the holder's model changes, every attached item's bone is looked up again (**believed** the
moment; the look-up is [CI-7]). An item's bone that the holder's model lacks is **unestablished** (believed: an
invalid bone index).

`enable_attachable_item(true)` makes the item visible also when the holder does not accept it (8.3, for example
the trader's items) [CI-10]; it is then not attached, so it is not placed or drawn by the holder; whether it is
drawn at all is **unestablished** (believed not: an item in an inventory is not drawn on its own).

### 8.5 Placement and drawing

Each time the holder's bones are computed (believed: before the holder is drawn, as `torch` 7.2), every attached
item's transform becomes *O* · *B* · *X*_holder, with *B* the current model-space matrix of the item's bone and
*X*_holder the holder's transform, applied to a point of the item's model in that order [CI-7] (`torch` 6).

When the holder is drawn, it draws its own model, then its active item (`scope` CH-20), then every attached item in
the order attached, each as its own model (the item section's `visual`) at its transform [CI-13]. A stalker with
the corpse-model flag set draws neither its active item nor its attached items [CI-13]. The player's drawing is
outside this specification.

### 8.6 When an attached item is hidden again

An attached item stops being drawn on its holder when it is detached (8.4): by a script (`enable_attachable_item(false)`),
by moving to the backpack, by leaving the inventory (drop, transfer, eating it up, release, destruction), or by
loading its section again. It is also not drawn while it stays attached when its holder is not drawn, or — for a
stalker — after the stalker came online with the corpse model (it is then never attached anew, 8.3). An item
that went offline with its holder and comes online again (or is loaded from a save) loads its section anew: an item
with `attach_angle_offset`, as every item of the stalkers' list, then starts disabled and unattached until a script
enables it (8.2) [CI-5]. Whether the holder's going offline detaches its items before it is destroyed is
**believed** (the holder checks, when destroyed, that nothing is attached [CI-12]).

## 9. Items a dead stalker leaves

The stalker binder's death callback, and the first update of a stalker that is already dead when its binder starts,
call `death_manager.drop_manager(npc):create_release_item()` [CI-23]. Its tables are filled when the player's
binder comes online, from `config\misc\death_generic.ltx`, which includes `death_items_by_communities.ltx`,
`death_items_by_levels.ltx` and `death_items_count.ltx` (`ltx` 4) [CI-23, CI-24]:

- per community (`stalker`, `dolg`, `freedom`, `bandit`, `military`, `zombied`, `ecolog`, `killer`, `monolith`,
  `arena_enemy`, `actor_dolg`), each line of its section: item section → 100 × the value (a chance in per cent);
- per item, a factor from the section named like the current level (`[default]` when absent; levels inherit it,
  `ltx` 3), and a count range from `[item_count_<d>]`, *d* the game difficulty (0 … 3), one value (min = max) or
  two; min and max are multiplied by the factor (an item with no factor gets 0) [CI-23];
- `[keep_items]`: the items whose value is `true` (20); `[ammo_sections]`: 20 ammunition sections; `[item_dependence]`:
  per item, a list of sections [CI-24].

`create_release_item` does, once per stalker (the simulation object's `death_droped` flag, saved by `se_stalker`)
[CI-23]:

1. for every item of the stalker's list of all items (`iterate_inventory`, S 3.3): the bolt is left alone; an item
   of `[keep_items]` is kept; the item in slot 1 or in slot 2 is kept, its magazine emptied (`unload_magazine`,
   S 7.4) and its condition set to (`math.random(15)` + 75)/100 (0.76 … 0.90); every other item is released from
   the simulation (J 5.5) [CI-23];
2. unless the stalker's custom data has a section `[dont_spawn_loot]`: for each item of the stalker's community's
   table (in the table's order, believed undefined), when the stalker still has (not marked dropped, S 5.4) one of
   the sections `[item_dependence]` lists for it (or it has no entry there): a number *k* =
   ⌈`math.random(min, max)`⌉ (`determinism` 3); for an ammunition section, *k* rounds created as boxes of its
   `box_size` (full boxes, then one box with the rest) as the stalker's children at its position; for any other
   item, *k* tries, each creating one item as the stalker's child when `math.random(100)` ≤ the chance [CI-23].

For a `stalker`-community character on `l01_escape` at difficulty 1: `ammo_9x18_fmj` (chance 100, factor 1, range
15 … 32 rounds) when it carries one of `wpn_pm`, `wpn_pb`, `wpn_fort`, `wpn_fort_m1`, `wpn_walther_m1`,
`wpn_mp5_m1`; `ammo_12x70_buck` (7 … 15) with `wpn_bm16` and the other shotguns; `bread` one try at 20 %,
`bandage` two tries at 40 %, `af_medusa` one try at 1.5 %; `antirad` (factor 0 on this level) never [CI-24].
`guitar_a` is in no table and not kept, so a dead novice's guitar is released (step 1).

## 10. The trade manager's timers

When `xr_logic` sets up a section of the logic of a stalker or of the trader (class `script_trader`), it calls
`trade_manager.trade_init` with the logic's `trade` file (default `misc\trade_generic.ltx`); it reads the
`[trader]` condition lists `buy_condition`, `sell_condition` (both required: missing is an `abort`) and
`buy_supplies` (optional) [CI-23]. `trade_manager.update` runs from the stalker binder's update while the stalker is
alive and its state manager is neither in combat nor in its simulation mode, and from the trader's binder's every
update [CI-23]. Each update [CI-23]:

1. returns at once when its update time is set and already passed; otherwise sets the update time to now + 3,600,000
   ms (so it runs on every call while calls come less than an hour apart, and never again after a gap of more than
   an hour);
2. picks a section from `buy_condition` and, when it differs from the current one, applies it with
   `buy_condition` (R); the same for `sell_condition`;
3. when `buy_supplies` is set: picks a section and, when it differs from the current one, unless the resupply time
   is set and already passed, restocks with `buy_supplies` (S 5.6) and sets the resupply time to now + 24 hours
   (so after 24 hours without a change of section it never restocks again).

"Now" is `time_global()`. The file name, the three current sections and the two times (as differences to now; −1
for none) are saved in the binder's state [CI-23] (`scope` CH-27).

## 11. Worked example: a novice plays the guitar at the campfire

The novices `esc_lager1` … `esc_lager7` of the beginners' village (`supplies` 4) are stalkers whose section lists
`guitar_a` among its `attachable_items` [CI-25]. Each receives a guitar at the New Game with a chance of 0.69998
(`supplies` 4, "The guitar").

1. **Coming online.** The guitar is made on the level and loads its section: `guitar_a` has
   `attach_angle_offset`, so the guitar reads its offsets and bone `bip01_l_hand` and becomes disabled (8.2;
   **believed** before the take below; were it after, the load would detach it, with the same outcome). The
   novice takes it (it has no `slot`, so it goes to the backpack, `pstate` 3.3): the take tries to attach it, but a
   disabled item is not accepted, and the move to the backpack detaches nothing (8.4). The guitar is not shown.
2. **Being chosen to play.** The campfire scheme (`xr_kamp`) offers the guitar states only to a member that has a
   `guitar_a` (`npc:object("guitar_a")`, S 3.2) [CI-23]; which member plays and when is the scheme's
   (`scope` CH-28).
3. **Taking the guitar up.** The state `guitar` (`state_lib`: weapon strapped, animation `guitar`) makes the
   animation part queue the motions `sit_1_guitar_0_0`, `sit_1_guitar_0_1`, then the idle `guitar_3`, with an
   attach entry for `guitar_a` tied to the first motion [CI-23]. When O's callback 29 reports that motion finished,
   the entry marks the guitar to be attached; the next update of the animation part calls
   `enable_attachable_item(true)` on the novice's first `guitar_a` (S 3.2) [CI-23].
4. **Attached.** The flag is set; the novice accepts it (any place, the section in its list, no other guitar
   attached, no corpse model; 8.3); `bip01_l_hand` is looked up in its model (the four novice models
   `actors\novice\green_stalker_2` … `_5` name that bone [CI-25]); the guitar is appended and made visible (8.4).
   Each time the novice's bones are computed the guitar's transform becomes *O* · *B* · *X*, with *B* the left
   hand's matrix and *O* (rounded) [CI-6]:

   | | x | y | z |
   |---|---|---|---|
   | O[0] | −0.9341 | −0.2116 | −0.2876 |
   | O[1] | 0.2763 | 0.0821 | −0.9576 |
   | O[2] | 0.2262 | −0.9739 | −0.0182 |
   | O[3] | 0.077492 | −0.030020 | 0.090059 |

   and the guitar's model `Equipments\item_guitar.ogf` is drawn with the novice (8.5).
5. **Putting it down.** When the novice leaves the state `guitar`, the out motions `guitar_0_sit_1_0`,
   `guitar_0_sit_1_1` are queued with a detach entry tied to the first; when it has finished, the next update calls
   `enable_attachable_item(false)`: the flag is cleared, the guitar detached and made invisible (S 8.1, 8.4)
   [CI-23]. When the state is left in the state manager's emergency mode, the guitar is detached at once, without
   the out motions; when the state changes again before the into motions have finished, every item the animation
   part has marked attached is detached at once [CI-23].
6. **Other ends.** A dying novice's guitar is released by the death drop (section 9) and leaves its holder (8.6). A
   saved and loaded game, or the novice going offline and online again, makes the guitar anew, disabled (step 1);
   the animation part starts with every entry detached, so the guitar shows again only at the next step 3.

## 12. Unestablished

- What a simulation stalker's preferences feed and whether saved arrays replace the drawn ones (2).
- What uses the simulation object's `max_item_mass` (4); the item's own take test and the flag of section 4.
- In a stalker's take: whether and when a taken weapon is made active (5).
- In the keep choice (6): when it is computed again; the first two selectable tests and the section test; the four
  evaluation functions and the exact type test.
- Whether the monster item's comparison, made at the floating-point unit's precision, gives other counts than single
  precision would for probabilities other than 0.2 (2).
- An attachment bone the holder's model lacks; when the bones are looked up again on a model change; whether an
  item made visible without being attached is drawn; whether a holder's going offline detaches its items first
  (8.4, 8.6).
- The order in which `death_manager` walks a community table (9; Lua table order).
- Everything here is static: the game was not run.

## 13. Provenance

- Date: 2026-11-28. Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG); `bin\xrGame.dll`
  (6,246,400 bytes) and `bin\xrCore.dll`.
- Data examined: the winning copies of the `config\system.ltx` tree (character, monster and item sections),
  `config\misc\death_*.ltx`, `config\gameplay\character_desc_escape.xml`, `spawns\all.spawn`, the shipped
  `scripts\` (`state_mgr_animation`, `state_mgr_animation_list`, `state_lib`, `state_mgr_goap`, `xr_kamp`,
  `death_manager`, `se_respawn`, `se_stalker`, `xr_motivator`, `bind_stalker`, `bind_monster`, `trade_manager`,
  `xr_logic`), and the novice models under `meshes\actors\novice\`.
- Analysis: static analysis of `xrGame.dll` (the existing headless Ghidra decompilation and instruction listing of
  the whole library, read-only headless Ghidra disassembly of code those lack, run-time type information read from
  the file bytes) of the attachable item and the attachment owner (lists, acceptance, attaching, detaching, the
  skeleton callback, the transform, drawing), the inventory owner's notifications, the stalker's, trader's and
  player's overrides, the corpse-model choice, carrying limits and the take test, the stalker's take event and
  single-item rule, the keep choice, the simulation human's preferences, the monster's creation step; of
  `xrCore.dll` (its headless decompilation) of the item-list splitting and trimming. The shipped scripts and the
  configuration were read as text. No debugging or execution; the game was not run.
- Checks: `checks/character-items/checks.py` (25 assertions, `ALL PASS 25`), with `fn.py`, `gh.py`, `scan.py`,
  `where.py` beside it and the helpers of `checks/characters-scope`, `checks/pda-scope` and
  `checks/script-api-inventory`.
- No leaked source code, derived projects or third-party documentation of the engine were used.

## Gaps found in review

- Section 8.2 makes every `device_torch` (it has `attach_angle_offset`) start disabled, and an item that leaves its holder is disabled too (8.4); `torch` 6 and 10.1 state that items stay enabled until a script changes that and that a character's torch is attached and on from the take. The specification does not reconcile the two.
- A stalker handles event 17 exactly as the take request (event 1) of section 5: refusal, take and single-item rule; what event 17 is, and when it is sent, is not stated.
- Section 9 step 2 asks for each dependency section only the stalker's first item of that section (`object`, S 3.2); whether the items released in step 1 are still found by that question at that moment is not established.
- Section 10: each `trade_init` replaces the character's trade state (the three current sections and both times are forgotten); loading a saved state applies the saved buy and sell sections again (`buy_condition`, `sell_condition`) but not the supplies section.
