// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-inventory.md on every target, one check per
//! section, over owners, items and configuration sections of our own.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;

use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::dialogs::DialogsAndTrade;
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass, SpawnData};
use crate::inventory::{Event, Inventories, NO_SLOT, rounded};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::services::EngineServices;
use crate::story::Story;
use crate::{Error, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("3.1 3.2 items", items),
    ("3.3 iterate_inventory", iterate_inventory),
    ("3.4 inventory_for_each", inventory_for_each),
    ("4 slots", slots),
    ("4.1 activate_slot", activate_slot),
    ("5.1 5.3 drop and transfer", drop_and_transfer),
    ("5.4 dropped marks", dropped_marks),
    ("5.5 eat", eat),
    ("5.6 buy_supplies", buy_supplies),
    ("6 money", money),
    ("7 ammunition", ammunition),
    ("8 other item members", other_items),
    ("8.4 best_weapon", best_weapon),
    ("9 worked example", worked_example),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("inventory {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// The 29 names of 1.1.
pub const NAMES: [&str; 29] = [
    "object_count",
    "object",
    "iterate_inventory",
    "inventory_for_each",
    "item_in_slot",
    "active_slot",
    "active_item",
    "activate_slot",
    "get_current_outfit",
    "get_current_outfit_protection",
    "drop_item",
    "drop_item_and_teleport",
    "transfer_item",
    "mark_item_dropped",
    "marked_dropped",
    "eat",
    "buy_supplies",
    "money",
    "give_money",
    "transfer_money",
    "get_ammo_in_magazine",
    "get_ammo_total",
    "set_ammo_elapsed",
    "unload_magazine",
    "enable_attachable_item",
    "attachable_item_enabled",
    "is_inv_box_empty",
    "best_item",
    "best_weapon",
];

const PLAYER: u16 = 0;
const STALKER: u16 = 30;
const DOG: u16 = 40;
const TRADER: u16 = 60;
const BOX: u16 = 70;

/// Items: id, section.
const ITEMS: &[(u16, &str)] = &[
    (100, "medkit"),
    (101, "bread"),
    (102, "device_pda"),
    (103, "bolt"),
    (104, "dar_document4"),
    (105, "quest"),
    (106, "notrade"),
    (107, "wpn_ak74"),
    (108, "wpn_knife"),
    (109, "ammo_545"),
    (110, "grenade"),
    (111, "outfit"),
    (112, "artefact"),
    (113, "medkit"),
    (114, "listed"),
];

const SYSTEM: &str = "[inventory]\r\nmax_weight = 50\r\nmax_belt = 5\r\n\
slot_persistent_1 = true\r\nslot_persistent_2 = false\r\nslot_persistent_3 = false\r\n\
slot_persistent_4 = true\r\nslot_persistent_5 = true\r\nslot_persistent_6 = true\r\n\
slot_persistent_7 = false\r\nslot_persistent_8 = true\r\nslot_persistent_9 = true\r\n\
slot_persistent_10 = true\r\n\
[trade]\r\nbuy_price_factor_hostile = 1\r\nbuy_price_factor_friendly = 2\r\n\
sell_price_factor_hostile = 3\r\nsell_price_factor_friendly = 4\r\n\
[eatable]\r\ninv_weight = 0.1\r\neat_health = 0.5\r\neat_power = 0\r\neat_satiety = 0\r\n\
eat_radiation = 0\r\nwounds_heal_perc = 0\r\neat_portions_num = 1\r\n\
[medkit]:eatable\r\nclass = II_MEDKI\r\n\
[bread]:eatable\r\nclass = II_FOOD\r\neat_portions_num = 2\r\n\
[device_pda]\r\nclass = D_PDA\r\ninv_weight = 0.1\r\n\
[bolt]\r\nclass = II_BOLT\r\ninv_weight = 0.1\r\nslot = 5\r\n\
[dar_document4]\r\nclass = II_DOC\r\ninv_weight = 0.1\r\n\
[quest]\r\nclass = II_DOC\r\ninv_weight = 0.1\r\nquest_item = true\r\n\
[notrade]\r\nclass = II_DOC\r\ninv_weight = 0.1\r\ncan_trade = false\r\n\
[listed]\r\nclass = II_DOC\r\ninv_weight = 0.1\r\n\
[weapon]\r\ninv_weight = 3\r\nscope_status = 0\r\ngrenade_launcher_status = 0\r\n\
silencer_status = 0\r\n\
[wpn_ak74]:weapon\r\nclass = WP_PM\r\nslot = 2\r\nammo_mag_size = 30\r\nammo_class = ammo_545, ammo_545_ap\r\n\
ef_weapon_type = 6\r\n\
[wpn_knife]:weapon\r\nclass = W_KNIFE\r\nslot = 1\r\nef_weapon_type = 1\r\n\
[ammo_545]\r\nclass = AMMO\r\ninv_weight = 0.2\r\nbox_size = 30\r\n\
[grenade]\r\nclass = G_F1\r\ninv_weight = 0.3\r\nslot = 3\r\n\
[artefact]\r\nclass = ARTEFACT\r\ninv_weight = 0.5\r\nbelt = true\r\ndefault_to_ruck = false\r\n\
[outfit]\r\nclass = EQU_STLK\r\ninv_weight = 5\r\nslot = 6\r\n\
additional_inventory_weight = 0\r\nadditional_inventory_weight2 = 0\r\n\
burn_protection = 0.1\r\nshock_protection = 0.2\r\nstrike_protection = 0.3\r\n\
wound_protection = 0.4\r\nradiation_protection = 0.5\r\ntelepatic_protection = 0.6\r\n\
chemical_burn_protection = 0.7\r\nexplosion_protection = 0.8\r\nfire_wound_protection = 0.25\r\n\
immunities_sect = outfit_immunities\r\n\
[outfit_immunities]\r\nburn_immunity = 1\r\nshock_immunity = 1\r\nstrike_immunity = 1\r\n\
wound_immunity = 1\r\nradiation_immunity = 1\r\ntelepatic_immunity = 1\r\n\
chemical_burn_immunity = 1\r\nexplosion_immunity = 1\r\nfire_wound_immunity = 1\r\n\
[inventory_box]\r\nclass = O_INVBOX\r\n\
[dog]\r\npanic_threshold = 0.1\r\n";

/// A test state: the scripts and the objects.
struct State {
    s: Scripts,
    objects: Rc<RefCell<GameObjects>>,
    services: Rc<RefCell<EngineServices>>,
}

impl State {
    fn inventories(&self) -> Rc<RefCell<Inventories>> {
        self.objects.borrow().inventories()
    }

    fn take(&mut self, owner: u16, item: u16) -> Result<(), String> {
        self.s.take_item(owner, item).map_err(message)
    }

    /// Serves the events; the other events.
    fn serve(&mut self) -> Result<Vec<Event>, String> {
        self.s.serve_inventory_events().map_err(message)
    }

    fn lua(&mut self, name: &str, body: &str) -> Result<(), String> {
        run(&mut self.s, name, &format!("{PRELUDE}{body}"))
    }

    fn parent(&self, id: u16) -> Option<u16> {
        self.objects.borrow().get(id).and_then(|o| o.parent)
    }
}

const PRELUDE: &str = "local actor, stalker, dog, trader, box = obj(0), obj(30), obj(40), obj(60), obj(70)\n\
local function names(list) local t = {} for i, o in ipairs(list) do t[i] = o:section() end return table.concat(t, ' ') end\n";

fn data(id: u16, section: &str, class: &str) -> SpawnData {
    SpawnData {
        id,
        name: format!("{section}{id}").into_bytes(),
        section: section.as_bytes().to_vec(),
        class_id: class_id(class.as_bytes()),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
        server: None,
    }
}

fn message(e: Error) -> String {
    e.message
}

/// A state with everything up to the inventory registered, `_G` loaded,
/// a global `obj(id)` giving handles, the owners, a box and the items
/// lying in the world.
fn state() -> Result<State, String> {
    let tree = Rc::new(Tree(
        [("gamedata/scripts/_g.script".to_owned(), Vec::new())]
            .into_iter()
            .collect(),
    ));
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(message)?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(message)?;
    s.register_story(Rc::new(RefCell::new(Story::new())))
        .map_err(message)?;
    s.register_dialogs_and_trade(Rc::new(RefCell::new(DialogsAndTrade::new())))
        .map_err(message)?;
    s.register_inventory().map_err(message)?;
    let shared = Rc::clone(&objects);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(shared.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(message)?;
    crate::entry_points::install(s.lua_mut()).map_err(message)?;
    s.load_globals().map_err(message)?;
    let mut spawned = vec![
        (ObjectClass::Player, data(PLAYER, "actor", "O_ACTOR")),
        (
            ObjectClass::Named("CAI_Stalker"),
            data(STALKER, "stalker", "AI_STL_S"),
        ),
        (ObjectClass::Named("CAI_Dog"), data(DOG, "dog", "AI_DOG_R")),
        (
            ObjectClass::Named("CAI_Trader"),
            data(TRADER, "trader", "AI_TRADE"),
        ),
        (
            ObjectClass::Unnamed(class_id(b"O_INVBOX")),
            data(BOX, "inventory_box", "O_INVBOX"),
        ),
    ];
    for &(id, section) in ITEMS {
        let class = match section {
            "wpn_ak74" => ObjectClass::Named("CWeaponAK74"),
            "wpn_knife" => ObjectClass::Named("CWeaponKnife"),
            "outfit" => ObjectClass::Named("CStalkerOutfit"),
            "grenade" => ObjectClass::Named("CF1"),
            _ => ObjectClass::Unnamed(class_id(b"II_DOC")),
        };
        spawned.push((class, data(id, section, "II_DOC")));
    }
    for (class, data) in spawned {
        s.spawn_engine_object(class, data)
            .map_err(|e| e.error.message)?;
    }
    Ok(State {
        s,
        objects,
        services,
    })
}

/// 1.1, 1.2: the 29 names are functions of `game_object`; rounding.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    let list = NAMES.map(|n| format!("'{n}'")).join(", ");
    t.lua(
        "registered",
        &format!(
            "for _, n in ipairs({{{list}}}) do eq(type(game_object[n]), 'function', n) end\n\
"
        ),
    )?;
    ensure(rounded(-1.0) == NO_SLOT, || "-1 rounds to no slot".into())?;
    ensure(
        rounded(2.5) == 2 && rounded(3.5) == 4 && rounded(1.4) == 1,
        || "round half even".into(),
    )
}

/// 3.1, 3.2.
pub fn items() -> Result<(), String> {
    let mut t = state()?;
    for item in [100, 113, 104] {
        t.take(STALKER, item)?;
    }
    t.lua(
        "items",
        "eq(stalker:object_count(), 3, 'count')\n\
         eq(stalker:object(0):id(), 100, 'first taken')\n\
         eq(stalker:object(2):id(), 104, 'third')\n\
         eq(stalker:object(3), nil, 'past the end')\n\
         eq(stalker:object(-1), nil, 'negative')\n\
         eq(stalker:object(1.9):id(), 113, 'truncated')\n\
         eq(stalker:object('medkit'):id(), 100, 'the first of its section')\n\
         eq(stalker:object('bread'), nil, 'none')\n\
         eq(box:object_count(), 0, 'not an owner')\n\
         eq(box:object('medkit'), nil, 'not an owner')\n\
         eq(actor:object_count(), 0, 'empty')",
    )
}

/// 3.3: two arguments, list order, failures fatal.
pub fn iterate_inventory() -> Result<(), String> {
    let mut t = state()?;
    for item in [104, 100, 107] {
        t.take(STALKER, item)?;
    }
    t.lua(
        "iterate",
        "local seen = {}\n\
         stalker:iterate_inventory(function(a, item) table.insert(seen, a .. ':' .. item:id()) end, 'x')\n\
         eq(table.concat(seen, ' '), 'x:104 x:100 x:107', 'order and arguments')\n\
         seen = {}\n\
         box:iterate_inventory(function() table.insert(seen, 1) end, 1)\n\
         eq(#seen, 0, 'not an owner')\n\
         actor:iterate_inventory(nil, 1)\n\
         raises(function() stalker:iterate_inventory(5, 1) end, 'no form', 'not a function')",
    )?;
    let r = t.s.exec(
        "=fails",
        b"obj(30):iterate_inventory(function() error('boom', 0) end, nil)",
    );
    ensure(
        r.is_err_and(|e| e.message.contains("LUA error: boom")),
        || "a failing function is fatal".into(),
    )
}

/// 3.4: the order and the owners' rules.
pub fn inventory_for_each() -> Result<(), String> {
    let mut t = state()?;
    // The player: backpack, belt, then the slots that aren't persistent.
    for item in [107, 112, 100, 105, 106, 103, 110, 111] {
        t.take(PLAYER, item)?;
    }
    t.lua(
        "player",
        "local seen = {}\n\
         actor:inventory_for_each(function(item, owner) table.insert(seen, item:section() .. ':' .. owner:id()) end)\n\
         eq(table.concat(seen, ' '), 'medkit:0 artefact:0 wpn_ak74:0 grenade:0 outfit:0', 'player')",
    )?;
    // A stalker's belt doesn't count; its slots do until the engine clears
    // them; a dead one offers what its show list doesn't hold.
    for item in [113, 108, 104] {
        t.take(STALKER, item)?;
    }
    t.lua(
        "stalker",
        "local seen = {}\n\
         stalker:inventory_for_each(function(item) table.insert(seen, item:section()) end)\n\
         eq(table.concat(seen, ' '), 'medkit dar_document4 wpn_knife', 'stalker')",
    )?;
    (t.inventories().borrow_mut().owner_mut(STALKER))
        .ok_or("no stalker inventory")?
        .slots_count = false;
    t.objects
        .borrow_mut()
        .get_mut(STALKER)
        .and_then(|o| o.entity.as_mut())
        .ok_or("no stalker")?
        .health = 0.0;
    t.lua(
        "dead stalker",
        "local seen = {}\n\
         stalker:inventory_for_each(function(item) table.insert(seen, item:section()) end)\n\
         eq(table.concat(seen, ' '), 'medkit dar_document4', 'no slot items')",
    )?;
    // The trader alive: never the PDA, nor an item of an item list.
    for item in [102, 101, 114] {
        t.take(TRADER, item)?;
    }
    t.lua(
        "trader",
        "sell_condition(create_ini_file('[s]\\r\\nlisted\\r\\n'), 's')\n\
         local seen = {}\n\
         trader:inventory_for_each(function(item) table.insert(seen, item:section()) end)\n\
         eq(table.concat(seen, ' '), 'bread', 'trader')\n\
         seen = {}\n\
         box:inventory_for_each(function(item) table.insert(seen, 1) end)\n\
         eq(#seen, 0, 'not an owner')",
    )?;
    t.objects
        .borrow_mut()
        .get_mut(TRADER)
        .and_then(|o| o.entity.as_mut())
        .ok_or("no trader")?
        .health = 0.0;
    t.lua(
        "dead trader",
        "local seen = {}\n\
         trader:inventory_for_each(function(item) table.insert(seen, item:section()) end)\n\
         eq(table.concat(seen, ' '), 'device_pda bread listed', 'every item')",
    )
}

/// 4: the slot members and the outfit.
pub fn slots() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "empty",
        "eq(actor:active_slot(), 4294967295, 'no active slot')\n\
         eq(actor:active_item(), nil, 'nothing in the hands')\n\
         eq(box:active_slot(), 0, 'not an owner')\n\
         eq(box:get_current_outfit_protection(0), 0, 'not an owner')",
    )?;
    for item in [107, 111] {
        t.take(PLAYER, item)?;
    }
    t.lua(
        "slots",
        "eq(actor:item_in_slot(2):id(), 107, 'slot 2')\n\
         eq(actor:item_in_slot(1.6):id(), 107, 'rounded')\n\
         eq(actor:item_in_slot(1), nil, 'empty slot')\n\
         eq(actor:item_in_slot(11), nil, 'outside')\n\
         eq(actor:item_in_slot(-1), nil, 'no slot')\n\
         eq(actor:active_slot(), 2, 'the first active slot')\n\
         eq(actor:active_item():id(), 107, 'in the hands')\n\
         eq(actor:get_current_outfit():id(), 111, 'worn outfit')\n\
         eq(math.abs(actor:get_current_outfit_protection(1) - 0.2) < 1e-6, true, 'p times c')\n\
         eq(math.abs(actor:get_current_outfit_protection(10.7)) < 1e-6, true, 'absent key, truncated')\n\
         eq(stalker:get_current_outfit(), nil, 'none')",
    )?;
    t.take(STALKER, 100)?;
    let r =
        t.s.exec("=crash", b"obj(30):get_current_outfit_protection(0)");
    ensure(r.is_err(), || "slot 6 empty is fatal".into())
}

/// 4.1.
pub fn activate_slot() -> Result<(), String> {
    let mut t = state()?;
    for item in [108, 107, 110] {
        t.take(PLAYER, item)?;
    }
    t.lua(
        "pending",
        "eq(actor:active_slot(), 1, 'the knife first')\n\
         actor:activate_slot(2)\n\
         eq(actor:active_slot(), 1, 'pending until the update')",
    )?;
    t.s.update_inventories().map_err(message)?;
    t.lua(
        "after update",
        "eq(actor:active_slot(), 2, 'active')\n\
         actor:activate_slot(7) actor:activate_slot(4)",
    )?;
    t.s.update_inventories().map_err(message)?;
    t.lua(
        "refused",
        "eq(actor:active_slot(), 2, 'inactive and empty slots refused')\n\
         actor:activate_slot(-1)",
    )?;
    t.s.update_inventories().map_err(message)?;
    (t.inventories().borrow_mut().owner_mut(PLAYER))
        .ok_or("no player inventory")?
        .inventory
        .move_to_backpack(110);
    t.lua(
        "put away",
        "eq(actor:active_slot(), 4294967295, 'no slot')\n\
         eq(actor:item_in_slot(3), nil, 'the grenade in the backpack')\n\
         actor:activate_slot(3)",
    )?;
    t.s.update_inventories().map_err(message)?;
    t.lua(
        "grenade",
        "eq(actor:active_slot(), 3, 'the grenade slot')\n\
         eq(actor:item_in_slot(3):id(), 110, 'the first grenade of the backpack')",
    )?;
    let r = t.s.exec("=fatal", b"obj(0):activate_slot(11)");
    ensure(r.is_err(), || "a slot outside the slots is fatal".into())
}

/// 5.1, 5.2, 5.3: the events and the server's handling.
pub fn drop_and_transfer() -> Result<(), String> {
    let mut t = state()?;
    t.take(TRADER, 104)?;
    t.lua(
        "transfer",
        "trader:transfer_item(trader:object('dar_document4'), actor)\n\
         trader:transfer_item(nil, actor)\n\
         trader:transfer_item(stalker, actor)",
    )?;
    let events = t.inventories().borrow().events().to_vec();
    ensure(
        events
            == [
                Event::Drop {
                    holder: TRADER,
                    item: 104,
                },
                Event::Take {
                    taker: PLAYER,
                    item: 104,
                },
            ],
        || format!("transfer events {events:?}"),
    )?;
    ensure(t.parent(104) == Some(TRADER), || "nothing at once".into())?;
    t.serve()?;
    ensure(t.parent(104) == Some(PLAYER), || {
        "the player holds it".into()
    })?;
    t.lua(
        "player",
        "eq(actor:object('dar_document4'):id(), 104, 'taken')\n\
                     eq(trader:object_count(), 0, 'given')",
    )?;
    // A loose item: the error line, then taken.
    t.lua("loose", "trader:transfer_item(obj(100), stalker)")?;
    t.serve()?;
    ensure(t.parent(100) == Some(STALKER), || {
        "a loose item is taken".into()
    })?;
    let log = t.services.borrow().console.log().to_vec();
    ensure(
        log.iter()
            .any(|l| l.starts_with("~ ERROR: can't detach independant object."))
            && log
                .iter()
                .any(|l| l.starts_with("sv ownership id_parent stalker30 id_entity medkit100")),
        || format!("log {log:?}"),
    )?;
    // drop_item and drop_item_and_teleport.
    t.lua(
        "drop",
        "stalker:drop_item(obj(100))\n\
         actor:drop_item_and_teleport(obj(104), vector():set(1, 2, 3))",
    )?;
    let others = t.serve()?;
    ensure(t.parent(100).is_none() && t.parent(104).is_none(), || {
        "dropped".into()
    })?;
    ensure(
        others
            == [Event::PutAt {
                item: 104,
                position: [1.0, 2.0, 3.0],
            }],
        || format!("put at {others:?}"),
    )?;
    // Another owner's item: fatal.
    t.take(STALKER, 113)?;
    t.lua("other", "actor:drop_item(obj(113))")?;
    ensure(t.serve().is_err(), || {
        "another owner's item is fatal".into()
    })
}

/// 5.4.
pub fn dropped_marks() -> Result<(), String> {
    let mut t = state()?;
    t.take(STALKER, 100)?;
    t.lua(
        "marks",
        "eq(stalker:marked_dropped(obj(100)), false, 'clear')\n\
         stalker:mark_item_dropped(obj(100))\n\
         eq(stalker:marked_dropped(obj(100)), true, 'set')\n\
         eq(box:marked_dropped(obj(100)), false, 'not an owner')\n\
         actor:mark_item_dropped(obj(101))\n\
         eq(actor:marked_dropped(obj(101)), true, 'a loose item keeps it')",
    )?;
    t.s.update_inventories().map_err(message)?;
    t.serve()?;
    ensure(t.parent(100).is_none(), || "dropped at the update".into())?;
    t.take(PLAYER, 101)?;
    t.lua(
        "cleared",
        "eq(stalker:marked_dropped(obj(100)), false, 'mark cleared')\n\
         eq(actor:marked_dropped(obj(101)), false, 'a take clears it')",
    )
}

/// 5.5.
pub fn eat() -> Result<(), String> {
    let mut t = state()?;
    t.take(PLAYER, 101)?;
    t.take(PLAYER, 100)?;
    t.lua("eat", "actor:eat(obj(101)) actor:eat(nil)")?;
    let events = t.inventories().borrow().events().to_vec();
    ensure(
        matches!(
            events.as_slice(),
            [
                Event::Eat {
                    eater: PLAYER,
                    item: 101,
                    ..
                },
                Event::UseObject {
                    player: PLAYER,
                    item: 101
                }
            ]
        ),
        || format!("two portions: {events:?}"),
    )?;
    t.serve()?;
    t.lua("eat again", "actor:eat(obj(101))")?;
    let events = t.inventories().borrow().events().to_vec();
    ensure(
        matches!(
            events.as_slice(),
            [
                Event::Eat { .. },
                Event::UseObject { .. },
                Event::Drop {
                    holder: PLAYER,
                    item: 101
                },
                Event::Destroy { object: 101 }
            ]
        ),
        || format!("used up: {events:?}"),
    )?;
    t.serve()?;
    let r = t.s.exec("=not held", b"obj(30):eat(obj(100))");
    ensure(r.is_err(), || {
        "an item not in the inventory is fatal".into()
    })
}

/// 5.6.
pub fn buy_supplies() -> Result<(), String> {
    let mut t = state()?;
    for item in [100, 103, 104] {
        t.take(TRADER, item)?;
    }
    t.inventories().borrow_mut().performance_counter = 1;
    t.lua(
        "buy",
        "local ini = create_ini_file('[supplies]\\r\\nmedkit = 2, 1\\r\\nbread = 3, 0\\r\\ndevice_pda = 0, 1\\r\\n')\n\
         trader:buy_supplies(ini, 'supplies')\n\
         box:buy_supplies(ini, 'supplies')",
    )?;
    let events = t.inventories().borrow().events().to_vec();
    let create = |s: &str| Event::Create {
        section: s.as_bytes().to_vec(),
        holder: TRADER,
    };
    ensure(
        events
            == [
                Event::Destroy { object: 100 },
                Event::Destroy { object: 104 },
                create("medkit"),
                create("medkit"),
            ],
        || format!("events {events:?}"),
    )?;
    let record = (t.inventories().borrow().owner(TRADER))
        .map(|o| o.restock.clone())
        .unwrap_or_default();
    let want: Vec<(Vec<u8>, f32)> = [("bread", 0.0), ("device_pda", 0.0), ("medkit", 1.0)]
        .iter()
        .map(|(s, v)| (s.as_bytes().to_vec(), *v))
        .collect();
    ensure(record == want, || {
        format!("record {record:?}, sorted lines")
    })?;
    let r = t.s.exec(
        "=missing",
        b"obj(60):buy_supplies(create_ini_file(''), 'absent')",
    );
    ensure(r.is_err(), || "a missing section is fatal".into())
}

/// 6.
pub fn money() -> Result<(), String> {
    let mut t = state()?;
    t.lua(
        "money",
        "actor:give_money(100.9)\n\
         eq(actor:money(), 100, 'given, truncated')\n\
         actor:transfer_money(30, trader)\n\
         eq(actor:money(), 70, 'from')\n\
         eq(trader:money(), 30, 'to')\n\
         actor:transfer_money(80, trader)\n\
         eq(actor:money(), 4294967286, 'wraps')\n\
         actor:transfer_money(1, nil)\n\
         eq(actor:money(), 4294967286, 'nil changes nothing')",
    )?;
    (t.inventories().borrow_mut().owner_mut(TRADER))
        .ok_or("no trader inventory")?
        .infinite_money = true;
    t.lua(
        "infinitive",
        "trader:give_money(-20)\n\
         eq(trader:money(), 110, 'never falls')\n\
         trader:give_money(5)\n\
         eq(trader:money(), 115, 'rises')",
    )?;
    let events = t.inventories().borrow().events().to_vec();
    ensure(
        events.first()
            == Some(&Event::Money {
                owner: PLAYER,
                money: 100,
            }),
        || format!("event 32: {events:?}"),
    )?;
    // The engine's setting as an owner comes online sends no event.
    let sent = events.len();
    t.s.set_owner_money(STALKER, 500).map_err(message)?;
    t.lua("set", "eq(stalker:money(), 500, 'set by the engine')")?;
    ensure(t.inventories().borrow().events().len() == sent, || {
        "the engine's setting sent an event".into()
    })?;
    let r = t.s.exec("=crash", b"obj(70):money()");
    ensure(r.is_err(), || "money on a non-owner is fatal".into())
}

/// 7.
pub fn ammunition() -> Result<(), String> {
    let mut t = state()?;
    for item in [107, 109, 108] {
        t.take(STALKER, item)?;
    }
    t.lua(
        "ammo",
        "local rifle, knife = obj(107), obj(108)\n\
         eq(rifle:get_ammo_in_magazine(), 30, 'a full magazine')\n\
         eq(rifle:get_ammo_total(), 60, 'a box of its type at its box size')\n\
         eq(knife:get_ammo_total(), 0, 'no box of its type: the magazine')\n\
         eq(stalker:get_ammo_total(), 0, 'not a weapon')\n\
         rifle:set_ammo_elapsed(45)\n\
         eq(rifle:get_ammo_in_magazine(), 45, 'not limited')\n\
         rifle:unload_magazine()\n\
         eq(rifle:get_ammo_in_magazine(), 0, 'unloaded')\n\
         knife:unload_magazine()\n\
         eq(box:get_ammo_in_magazine(), 0, 'other objects')",
    )?;
    // 7.5: the spawn data's rounds, at most the box size.
    t.s.spawn_ammunition(109, 12).map_err(message)?;
    t.lua(
        "spawned",
        "eq(obj(107):get_ammo_total(), 12, 'the spawn data rounds')",
    )?;
    t.s.spawn_ammunition(109, 31).map_err(message)?;
    t.lua(
        "above",
        "eq(obj(107):get_ammo_total(), 30, 'at most the box size')",
    )
}

/// 8.4: with no enemy, the carried weapons by type, kept while usable.
pub fn best_weapon() -> Result<(), String> {
    let mut t = state()?;
    t.lua("none", "eq(stalker:best_weapon(), nil, 'no weapon')")?;
    t.take(STALKER, 107)?;
    t.lua(
        "rifle",
        "eq(stalker:best_weapon():id(), 107, 'its only weapon')",
    )?;
    t.take(STALKER, 108)?;
    t.lua(
        "kept",
        "eq(stalker:best_weapon():id(), 107, 'kept while usable')\n\
         obj(107):unload_magazine()\n\
         eq(stalker:best_weapon():id(), 108, 'no rounds: chosen again')",
    )?;
    t.take(STALKER, 109)?;
    t.lua(
        "rounds",
        "eq(obj(107):get_ammo_total(), 30, 'a box')\n\
         eq(stalker:best_weapon():id(), 108, 'the knife is kept')",
    )?;
    let mut u = state()?;
    for item in [108, 107] {
        u.take(STALKER, item)?;
    }
    u.lua(
        "type",
        "eq(stalker:best_weapon():id(), 107, 'the greater type')",
    )?;
    if let Some(e) = (u.objects.borrow_mut().get_mut(STALKER)).and_then(|o| o.entity.as_mut()) {
        e.health = 0.0;
    }
    u.lua("dead", "eq(stalker:best_weapon(), nil, 'health 0')")
}

/// 8.
pub fn other_items() -> Result<(), String> {
    let mut t = state()?;
    t.take(STALKER, 107)?;
    t.lua(
        "attachable",
        "eq(obj(100):attachable_item_enabled(), false, 'initially')\n\
         obj(100):enable_attachable_item(true)\n\
         eq(obj(100):attachable_item_enabled(), true, 'no holder: the flag')\n\
         obj(107):enable_attachable_item(true)\n\
         eq(obj(107):attachable_item_enabled(), true, 'a stalker carries it')\n\
         eq(box:attachable_item_enabled(), false, 'not an item')\n\
         eq(box:is_inv_box_empty(), true, 'an empty box')\n\
         eq(stalker:is_inv_box_empty(), false, 'not a box')\n\
         eq(stalker:best_item(), nil, 'a fighter: an empty item memory')\n\
         eq(box:best_item(), nil, 'not a fighter')\n\
         eq(dog:best_weapon(), nil, 'not a stalker')",
    )?;
    let events = t.inventories().borrow().events().to_vec();
    ensure(
        events
            == [Event::Attachment {
                item: 107,
                holder: STALKER,
                attached: true,
            }],
        || format!("{events:?}"),
    )?;
    if let Some(o) = t.objects.borrow_mut().get_mut(100) {
        o.parent = Some(BOX);
    }
    t.lua(
        "box",
        "eq(box:is_inv_box_empty(), false, 'it holds an item')",
    )
}

/// 9: a dead character's items, as `death_manager` walks them.
pub fn worked_example() -> Result<(), String> {
    let mut t = state()?;
    for item in [100, 107, 108] {
        t.take(STALKER, item)?;
    }
    t.lua(
        "worked example",
        "local seen = {}\n\
         local function keep_item(npc, item)\n\
           if item:id() == npc:item_in_slot(1):id() or item:id() == npc:item_in_slot(2):id() then\n\
             item:unload_magazine()\n\
           end\n\
           table.insert(seen, item:section())\n\
         end\n\
         stalker:iterate_inventory(keep_item, stalker)\n\
         eq(table.concat(seen, ' '), 'medkit wpn_ak74 wpn_knife', 'list order')\n\
         eq(obj(107):get_ammo_in_magazine(), 0, 'slot 2 unloaded')\n\
         eq(stalker:object('wpn_ak74'):id(), 107, 'object(k)')\n\
         eq(stalker:marked_dropped(obj(107)), false, 'not marked')",
    )
}
