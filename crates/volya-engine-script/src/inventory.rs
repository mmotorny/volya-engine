// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The inventory as scripts reach it (script API S,
//! docs/behavior/script-api-inventory.md; section numbers are its): the
//! `game_object` members for items, slots, moving items, money,
//! ammunition and attachable items, and the entry points
//! `iterate_inventory` and `inventory_for_each`.
//!
//! What scripts read and change is [`Inventories`]: each owner's
//! inventory (`volya_engine_core`'s [`Inventory`], made on first need,
//! with its money and pending slot), the state of the items no inventory
//! holds, the items' dropped and enabled marks, and the events members send
//! to the game's server ([`Event`]). The engine serves the take and drop
//! events ([`Scripts::serve_inventory_events`]), takes items into an
//! inventory ([`Scripts::take_item`]) and runs the inventories' update
//! ([`Scripts::update_inventories`]). The `game_object` members are
//! defined by the Lua side (`inventory.lua`) over one hidden Rust function,
//! as the story's are. See docs/architecture/scripts.md, Inventory.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::rc::Rc;

use volya_engine_core::character::condition::Eatable;
use volya_engine_core::character::inventory::{
    GRENADE_SLOT, INACTIVE_SLOTS, Inventory, InventoryConfig, InventoryItem, ItemConfig, ItemState,
    OUTFIT_SLOT, Place, SLOT_COUNT,
};
use volya_engine_core::config::{Config, value};
use volya_engine_formats::cp1251;
use volya_engine_formats::object::Kind;

use crate::classes::ObjectMaker;
use crate::dialogs::{DialogsAndTrade, TradeParameters, trade};
use crate::files::{FileSystem, IniFile};
use crate::game_object::members::Handle;
use crate::game_object::{GameObject, GameObjects, Kinds, ObjectClass};
use crate::loader::Scripts;
use crate::object_factory::class_id;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::values::Vector;
use crate::{Error, ErrorKind, Value};

/// "No slot" (4).
pub const NO_SLOT: u32 = u32::MAX;

/// The class `CAI_Trader` (3.4).
const TRADER: &str = "CAI_Trader";

/// What the members send to the game's server or ask of the engine, in
/// order ([`Inventories::take_events`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Event 1, the take request (`use` 9.2): `taker` asks for `item`.
    Take { taker: u16, item: u16 },
    /// Event 3: `holder` drops `item` (5.1).
    Drop { holder: u16, item: u16 },
    /// Event 8: destroy `object`.
    Destroy { object: u16 },
    /// Event 29: put `item` at `position` (5.2).
    PutAt { item: u16, position: [f32; 3] },
    /// `buy_supplies` (5.6 step 4): an item of `section` created at
    /// `holder`'s position with `holder` as its holder (the simulation's
    /// to spawn).
    Create { section: Vec<u8>, holder: u16 },
    /// Event 32: `owner`'s money is now `money` (6).
    Money { owner: u16, money: u32 },
    /// `eat` (5.5 step 2): the eating of `pstate` 5 on `eater`'s
    /// conditions, which are the engine's.
    Eat {
        eater: u16,
        item: u16,
        eatable: Eatable,
    },
    /// `eat` (5.5 step 3): the player's callback 15 `use_object` with the
    /// item, for the engine to raise (spec O).
    UseObject { player: u16, item: u16 },
    /// `enable_attachable_item` (8.1): `item` attached to `holder`'s model
    /// and shown, or detached and hidden.
    Attachment {
        item: u16,
        holder: u16,
        attached: bool,
    },
}

/// An owner's inventory and what goes with it (2.2, 4.1, 6).
#[derive(Clone, Debug, PartialEq)]
pub struct OwnerInventory {
    pub inventory: Inventory,
    /// 32-bit unsigned (6).
    pub money: u32,
    /// The `infinitive` attribute of its specific character's `money`
    /// element (6): the money never falls. The engine sets it.
    pub infinite_money: bool,
    /// Whether the belt counts for `inventory_for_each` (2.2): the
    /// player's.
    pub belt_counts: bool,
    /// Whether the slots count for `inventory_for_each` (2.2): true when
    /// made; the engine clears it on a stalker's (believed when it dies).
    pub slots_count: bool,
    /// The restock record (5.6 step 5): each line's section with its value.
    pub restock: Vec<(Vec<u8>, f32)>,
    /// The pending slot (4.1): `Some(None)` puts the item in the hands
    /// away.
    pub pending: Option<Option<u8>>,
    /// Stamped by each take (2.2).
    pub changed: u32,
}

/// An item's marks (2.2, 8.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Marks {
    /// The dropped mark (5.4).
    pub dropped: bool,
    /// The attachable item's enabled flag (8.1).
    pub enabled: bool,
}

/// The inventories scripts see, and what their members ask of the server
/// and the engine.
#[derive(Default)]
pub struct Inventories {
    owners: BTreeMap<u16, OwnerInventory>,
    /// The state of the items no inventory holds, made on first need.
    loose: BTreeMap<u16, InventoryItem>,
    marks: BTreeMap<u16, Marks>,
    /// The ammunition items' rounds (7.5), once set from their spawn data;
    /// an item without an entry holds its box size.
    rounds: BTreeMap<u16, u16>,
    /// The stalkers' best-weapon state (8.4).
    best_weapons: BTreeMap<u16, BestWeapon>,
    events: Vec<Event>,
    /// The low 32 bits of the processor's performance counter, which
    /// seeds `buy_supplies`' generators (5.6 step 4): the engine sets it.
    pub performance_counter: u32,
}

impl Inventories {
    pub fn new() -> Self {
        Self::default()
    }

    /// Owner `id`'s inventory, once made.
    pub fn owner(&self, id: u16) -> Option<&OwnerInventory> {
        self.owners.get(&id)
    }

    /// Owner `id`'s inventory, for the engine to set its money and the
    /// rest; `None` before it is made ([`Scripts::take_item`] makes it).
    pub fn owner_mut(&mut self, id: u16) -> Option<&mut OwnerInventory> {
        self.owners.get_mut(&id)
    }

    /// Item `id`'s marks.
    pub fn marks(&self, id: u16) -> Marks {
        self.marks.get(&id).copied().unwrap_or_default()
    }

    /// Clears item `id`'s enabled flag, as the engine does once it has left
    /// its holder (docs/behavior/character-items.md 8.4).
    pub fn disable_attachable(&mut self, id: u16) {
        if let Some(m) = self.marks.get_mut(&id) {
            m.enabled = false;
        }
    }

    /// The events not yet taken, oldest first.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The owner whose inventory holds item `item` (its holder).
    pub fn holder(&self, item: u16) -> Option<u16> {
        (self.owners.iter())
            .find(|(_, o)| o.inventory.item(item).is_some())
            .map(|(&id, _)| id)
    }

    /// Item `id`'s state, held or loose.
    pub fn item(&self, id: u16) -> Option<&InventoryItem> {
        (self.owners.values())
            .find_map(|o| o.inventory.item(id))
            .or_else(|| self.loose.get(&id))
    }

    fn item_mut(&mut self, id: u16) -> Option<&mut InventoryItem> {
        for o in self.owners.values_mut() {
            if let Some(i) = o.inventory.items.iter_mut().find(|i| i.id == id) {
                return Some(i);
            }
        }
        self.loose.get_mut(&id)
    }

    /// Forgets object `id`: an item leaves its inventory, an owner's
    /// inventory goes.
    pub fn forget(&mut self, id: u16) {
        self.owners.remove(&id);
        for o in self.owners.values_mut() {
            o.inventory.remove(id);
        }
        self.loose.remove(&id);
        self.marks.remove(&id);
        self.rounds.remove(&id);
        self.best_weapons.remove(&id);
        // A destroyed best weapon is none (our choice: the spec keeps an
        // object, not its absence).
        for b in self.best_weapons.values_mut() {
            if b.weapon == Some(id) {
                b.weapon = None;
            }
        }
    }
}

/// A stalker's best-weapon state (8.4). Its remembered pick-ups are never
/// set: only steps 3 and 4 set them, from the item memory, which stays
/// empty while no fighter perceives (TODO(#1211)).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct BestWeapon {
    weapon: Option<u16>,
    /// The stored rating.
    rating: f32,
    /// The mark "chosen": clear when made (believed), and only a change of
    /// the enemy choice's enemy clears it, which none makes here
    /// (TODO(#1211)).
    chosen: bool,
}

/// What the members share.
#[derive(Clone)]
pub(crate) struct Context {
    pub state: Rc<RefCell<Inventories>>,
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    pub dialogs: Option<Rc<RefCell<DialogsAndTrade>>>,
    pub fatal: Rc<RefCell<Option<String>>>,
}

type Results = Result<Vec<Value>, String>;

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    fn report(&self, function: &str, line: u32, description: &str, arguments: &[&str]) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression: "fatal error",
                description,
                arguments,
            },
        )
    }

    /// A fatal error standing for a crash the spec believes (our wording).
    fn crash(&self, function: &str, text: &str) -> String {
        self.report(function, line!(), "<no expression>", &[text])
    }

    /// A failed check (`console-log` 3.8): fatal.
    fn failed_check(&self, function: &str, description: &str) -> String {
        self.report(function, line!(), description, &[])
    }

    fn system(&self) -> Rc<Config> {
        Rc::clone(&self.files.borrow().system)
    }

    fn log(&self, line: &str) {
        self.services.borrow_mut().console.log_mut().write(line);
    }
}

/// The section of `object`, decoded.
fn section_of(object: &GameObject) -> Option<String> {
    object.section.as_deref().map(cp1251::decode)
}

/// The kind of class id the section of `object` names, if any.
fn class_kind(system: &Config, object: &GameObject) -> Option<(Kind, String)> {
    let section = section_of(object)?;
    let class = system.get(&section, "class").flatten()?.trim().to_owned();
    let id = volya_engine_formats::object::ClassId::new(&class).ok()?;
    Some((id.kind(), class))
}

/// The item query (2.1): the item classes of M 2.2, and every object whose
/// section's class id is an item's.
fn is_item(system: &Config, object: &GameObject) -> bool {
    object.kinds.has(Kinds::ITEM)
        || class_kind(system, object).is_some_and(|(kind, _)| is_item_kind(kind))
}

/// Whether a class id of kind `kind` is an item's.
fn is_item_kind(kind: Kind) -> bool {
    use Kind::*;
    matches!(
        kind,
        Item | Torch
            | Pda
            | Document
            | Outfit
            | Ammunition
            | Knife
            | MagazineWeapon
            | WeaponWithGrenadeLauncher
            | Shotgun
    )
}

fn is_owner(object: &GameObject) -> bool {
    object.kinds.has(Kinds::INVENTORY_OWNER)
}

fn is_trader(object: &GameObject) -> bool {
    matches!(object.class, ObjectClass::Named(TRADER))
}

fn health(object: &GameObject) -> f32 {
    object.entity.as_ref().map_or(1.0, |e| e.health)
}

/// Whether item `item` is a weapon (2.1): the 19 weapon classes, the knife
/// and the binoculars included.
fn is_weapon(item: &InventoryItem) -> bool {
    item.config.is_weapon()
}

/// A magazine weapon (2.1): a weapon but the knife.
fn is_magazine_weapon(item: &InventoryItem) -> bool {
    is_weapon(item) && item.config.class_id.kind() != Kind::Knife
}

/// "Rounded" (1.2): to the nearest integer, ties to even (believed), then
/// modulo 2³².
pub fn rounded(n: f64) -> u32 {
    n.round_ties_even() as i64 as u32
}

/// "Made an integer" (1.2): truncated toward zero (believed).
fn integer(n: f64) -> i32 {
    n as i32
}

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

fn number(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match arg(args, i) {
        Value::Number(n) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

/// A handle argument: `None` for nil (each member tests it, depends on A).
fn handle(
    maker: &ObjectMaker,
    args: &[Value],
    i: usize,
    what: &str,
) -> Result<Option<u16>, String> {
    match arg(args, i) {
        Value::Nil => Ok(None),
        v => maker
            .get::<Handle>(v)
            .map(|h| Some(h.id))
            .ok_or_else(|| format!("{what}: game object expected")),
    }
}

fn one(v: Value) -> Results {
    Ok(vec![v])
}

fn whole(n: u32) -> Results {
    one(Value::Number(f64::from(n)))
}

fn boolean(b: bool) -> Results {
    one(Value::Boolean(b))
}

fn nothing() -> Results {
    Ok(Vec::new())
}

/// The state the raw function works on, borrowed once.
struct Call<'a> {
    ctx: &'a Context,
    objects: &'a mut GameObjects,
    state: &'a mut Inventories,
    system: Rc<Config>,
}

impl Call<'_> {
    fn object(&self, id: u16) -> Result<&GameObject, String> {
        self.objects
            .get(id)
            .ok_or_else(|| format!("game object {id} no longer exists"))
    }

    fn is_item(&self, id: u16) -> bool {
        self.objects
            .get(id)
            .is_some_and(|o| is_item(&self.system, o))
    }

    fn is_owner(&self, id: u16) -> bool {
        self.objects.get(id).is_some_and(is_owner)
    }

    fn handle(&self, id: Option<u16>) -> Value {
        self.objects.handle_value(id)
    }

    /// Owner `id`'s inventory, made on first need (2.2).
    fn owner(&mut self, id: u16) -> Result<&mut OwnerInventory, String> {
        make_owner(self.ctx, self.objects, self.state, &self.system, id)
    }

    /// Item `id`'s state, made from its section on first need.
    fn item(&mut self, id: u16) -> Result<&mut InventoryItem, String> {
        make_item(self.ctx, self.objects, self.state, &self.system, id)?;
        Ok(self.state.item_mut(id).expect("made above"))
    }
}

fn make_owner<'a>(
    ctx: &Context,
    objects: &GameObjects,
    state: &'a mut Inventories,
    system: &Config,
    id: u16,
) -> Result<&'a mut OwnerInventory, String> {
    match state.owners.entry(id) {
        Entry::Occupied(o) => Ok(o.into_mut()),
        Entry::Vacant(v) => {
            let config = InventoryConfig::read(system)
                .map_err(|e| ctx.failed_check("CInventory", &e.to_string()))?;
            let player = objects.get(id).is_some_and(|o| o.kinds.has(Kinds::PLAYER));
            Ok(v.insert(OwnerInventory {
                inventory: Inventory::new(config),
                money: 0,
                infinite_money: false,
                belt_counts: player,
                slots_count: true,
                restock: Vec::new(),
                pending: None,
                changed: 0,
            }))
        }
    }
}

fn make_item(
    ctx: &Context,
    objects: &GameObjects,
    state: &mut Inventories,
    system: &Config,
    id: u16,
) -> Result<(), String> {
    if state.item(id).is_some() {
        return Ok(());
    }
    let object = objects
        .get(id)
        .ok_or_else(|| format!("game object {id} no longer exists"))?;
    let section = section_of(object).unwrap_or_default();
    // A missing key is fatal (ltx-config.md 11).
    let config = ItemConfig::read(system, &section)
        .map_err(|e| ctx.failed_check("CInventoryItem", &e.to_string()))?;
    let item = InventoryItem::create(
        config,
        id,
        None,
        volya_engine_core::character::inventory::NO_ID,
        "",
    );
    state.loose.insert(id, item);
    Ok(())
}

/// Takes item `item` into owner `owner`'s inventory (2.2, `pstate` 3.3):
/// appended to its list of all items, its dropped mark cleared, the
/// inventory stamped as changed, the object's parent the owner.
fn take(
    ctx: &Context,
    objects: &mut GameObjects,
    state: &mut Inventories,
    system: &Config,
    owner: u16,
    item: u16,
) -> Result<(), String> {
    make_item(ctx, objects, state, system, item)?;
    let taken = match state.loose.remove(&item) {
        Some(i) => i,
        None => {
            let holder = state.holder(item).expect("held when not loose");
            if holder == owner {
                return Ok(());
            }
            state
                .owners
                .get_mut(&holder)
                .and_then(|o| o.inventory.remove(item))
                .expect("held")
        }
    };
    let inventory = make_owner(ctx, objects, state, system, owner)?;
    inventory.inventory.take(taken);
    inventory.changed = inventory.changed.wrapping_add(1);
    state.marks.entry(item).or_default().dropped = false;
    if let Some(o) = objects.get_mut(item) {
        o.parent = Some(owner);
    }
    Ok(())
}

/// Item `item` leaves its holder's inventory and lies in the world.
fn release(objects: &mut GameObjects, state: &mut Inventories, item: u16) {
    if let Some(holder) = state.holder(item)
        && let Some(i) = state
            .owners
            .get_mut(&holder)
            .and_then(|o| o.inventory.remove(item))
    {
        let mut i = i;
        i.place = Place::None;
        state.loose.insert(item, i);
    }
    if let Some(o) = objects.get_mut(item) {
        o.parent = None;
    }
}

/// Sets owner `id`'s money to `new` (6): never lower with the
/// `infinitive` attribute; event 32 sent.
fn set_money(state: &mut Inventories, id: u16, new: u32) {
    let Some(o) = state.owners.get_mut(&id) else {
        return;
    };
    o.money = if o.infinite_money {
        o.money.max(new)
    } else {
        new
    };
    let money = o.money;
    state.events.push(Event::Money { owner: id, money });
}

/// Registers the raw function and the Lua side.
fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    let lua = scripts.lua_mut();
    let c = ctx.clone();
    lua.register_with_objects("__volya_inventory", move |maker, args| raw(&c, maker, args))?;
    lua.exec("=inventory", include_bytes!("inventory.lua"))
        .map(drop)
}

/// The engine part of the Lua side's members: `args[0]` names it,
/// `args[1]` is the member's object.
fn raw(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    let op = match arg(args, 0) {
        Value::String(op) => String::from_utf8_lossy(op).into_owned(),
        _ => return Err("inventory: operation expected".into()),
    };
    let op = op.as_str();
    if op == "fatal" {
        // A failing script function (3.3): fatal as D 1.2.
        let text = match arg(args, 1) {
            Value::String(s) => String::from_utf8_lossy(s).into_owned(),
            _ => String::new(),
        };
        let message = format!("LUA error: {text}");
        *ctx.fatal.borrow_mut() = Some(message.clone());
        return one(Value::String(message.into_bytes()));
    }
    ctx.stopped()?;
    if op == "crash" {
        return Err(ctx.crash(op, "the function is nil"));
    }
    let id = handle(maker, args, 1, op)?.ok_or_else(|| format!("{op}: game object expected"))?;
    let args = args.get(2..).unwrap_or(&[]);
    let system = ctx.system();
    let mut objects = ctx.objects.borrow_mut();
    let mut state = ctx.state.borrow_mut();
    let mut call = Call {
        ctx,
        objects: &mut objects,
        state: &mut state,
        system,
    };
    let c = &mut call;
    let owner = c.is_owner(id);
    match op {
        // 3.1, 3.2.
        "object_count" => {
            if !owner {
                return whole(0);
            }
            let n = c.owner(id)?.inventory.items.len();
            whole(n as u32)
        }
        "object" => {
            if !owner {
                return one(Value::Nil);
            }
            let items = &c.owner(id)?.inventory.items;
            let found = match arg(args, 0) {
                Value::Number(i) => {
                    let i = integer(*i);
                    usize::try_from(i)
                        .ok()
                        .and_then(|i| items.get(i))
                        .map(|i| i.id)
                }
                Value::String(name) => {
                    let name = cp1251::decode(name);
                    items
                        .iter()
                        .find(|i| i.config.section == name)
                        .map(|i| i.id)
                }
                _ => return Err("object: no form of it takes these arguments".into()),
            };
            one(c.handle(found))
        }
        // 3.3: the items' handles, in list order; nil for a non-owner.
        "iterate_inventory" => {
            if !owner {
                return one(Value::Nil);
            }
            let ids: Vec<u16> = c.owner(id)?.inventory.items.iter().map(|i| i.id).collect();
            one(list(c, &ids))
        }
        "inventory_for_each" => {
            if !owner {
                return one(Value::Nil);
            }
            let ids = offered(c, id)?;
            one(list(c, &ids))
        }
        // 4.
        "item_in_slot" => {
            let n = rounded(number(args, 0, op)?);
            if !owner || n as usize >= SLOT_COUNT {
                return one(Value::Nil);
            }
            let found = c.owner(id)?.inventory.slots[n as usize];
            one(c.handle(found))
        }
        "active_slot" => {
            if !owner {
                return whole(0);
            }
            let active = c.owner(id)?.inventory.active_slot;
            whole(active.map_or(NO_SLOT, u32::from))
        }
        "active_item" => {
            if !owner {
                return one(Value::Nil);
            }
            let found = c.owner(id)?.inventory.in_hands();
            one(c.handle(found))
        }
        "activate_slot" => {
            let n = rounded(number(args, 0, op)?);
            if owner {
                activate_slot(c, id, n)?;
            }
            nothing()
        }
        "get_current_outfit" => {
            if !owner {
                return one(Value::Nil);
            }
            let found = c.owner(id)?.inventory.slots[usize::from(OUTFIT_SLOT)];
            one(c.handle(found))
        }
        "get_current_outfit_protection" => {
            let t = integer(number(args, 0, op)?);
            if !owner {
                return one(Value::Number(0.0));
            }
            let inventory = &c.owner(id)?.inventory;
            let Some(outfit) = inventory.slots[usize::from(OUTFIT_SLOT)] else {
                // Believed a crash (4).
                return Err(ctx.crash(op, "slot 6 is empty"));
            };
            let item = inventory.item(outfit).expect("in its slot");
            let Some(p) = &item.config.protection else {
                return one(Value::Number(0.0));
            };
            // `t` outside 0 … 10 reads beyond the table (believed): fatal
            // here (our wording).
            let Some(&p_t) = usize::try_from(t).ok().and_then(|t| p.protections.get(t)) else {
                return Err(ctx.crash(op, "no such hit type"));
            };
            let condition = item.condition;
            one(Value::Number(f64::from(1.0 - (1.0 - p_t * condition))))
        }
        // 5.
        "drop_item" => {
            if let Some(item) = handle(maker, args, 0, op)? {
                drop_item(c, id, item);
            }
            nothing()
        }
        "drop_item_and_teleport" => {
            let item = handle(maker, args, 0, op)?;
            let position = maker
                .get::<Vector>(arg(args, 1))
                .ok_or_else(|| format!("{op}: vector expected"))?;
            if let Some(item) = item {
                drop_item(c, id, item);
                c.state.events.push(Event::PutAt {
                    item,
                    position: [position.x, position.y, position.z],
                });
            }
            nothing()
        }
        "transfer_item" => {
            let item = handle(maker, args, 0, op)?;
            let to = handle(maker, args, 1, op)?;
            if let (Some(item), Some(to)) = (item, to)
                && c.is_item(item)
            {
                c.state.events.push(Event::Drop { holder: id, item });
                c.state.events.push(Event::Take { taker: to, item });
            }
            nothing()
        }
        "mark_item_dropped" | "marked_dropped" => {
            let item = handle(maker, args, 0, op)?;
            let Some(item) = item.filter(|&i| owner && c.is_item(i)) else {
                return if op == "marked_dropped" {
                    boolean(false)
                } else {
                    nothing()
                };
            };
            if op == "marked_dropped" {
                return boolean(c.state.marks(item).dropped);
            }
            c.state.marks.entry(item).or_default().dropped = true;
            nothing()
        }
        "eat" => {
            if let Some(item) = handle(maker, args, 0, op)?
                && owner
                && c.is_item(item)
            {
                eat(c, id, item)?;
            }
            nothing()
        }
        "buy_supplies" => {
            let ini = match arg(args, 0) {
                Value::Nil => None,
                v => Some(
                    maker
                        .get::<IniFile>(v)
                        .ok_or_else(|| format!("{op}: ini_file expected"))?,
                ),
            };
            let Value::String(section) = arg(args, 1) else {
                return Err(format!("{op}: text expected"));
            };
            if owner {
                buy_supplies(c, id, ini, section)?;
            }
            nothing()
        }
        // 6.
        "money" => {
            if !owner {
                return Err(ctx.crash(op, "the object is not an inventory owner"));
            }
            whole(c.owner(id)?.money)
        }
        "give_money" => {
            let n = integer(number(args, 0, op)?);
            if !owner {
                return Err(ctx.crash(op, "the object is not an inventory owner"));
            }
            let new = c.owner(id)?.money.wrapping_add(n as u32);
            set_money(c.state, id, new);
            nothing()
        }
        "transfer_money" => {
            let n = integer(number(args, 0, op)?) as u32;
            let Some(to) = handle(maker, args, 1, op)? else {
                return nothing();
            };
            if !owner || !c.is_owner(to) {
                return Err(ctx.crash(op, "the object is not an inventory owner"));
            }
            let new = c.owner(id)?.money.wrapping_sub(n);
            set_money(c.state, id, new);
            let new = c.owner(to)?.money.wrapping_add(n);
            set_money(c.state, to, new);
            nothing()
        }
        // 7.
        "get_ammo_in_magazine" => {
            if !c.is_item(id) {
                return whole(0);
            }
            whole(magazine(c.item(id)?))
        }
        "get_ammo_total" => {
            if !c.is_item(id) {
                return whole(0);
            }
            whole(ammo_total(c, id)?)
        }
        "set_ammo_elapsed" => {
            let n = integer(number(args, 0, op)?);
            if !c.is_item(id) {
                return nothing();
            }
            let item = c.item(id)?;
            if let ItemState::Weapon(w) = &mut item.state {
                // A negative count is believed to run out of memory: fatal
                // here (our wording); the magazine's count is 16-bit.
                let Ok(n) = u16::try_from(n) else {
                    return Err(ctx.crash(op, "out of memory"));
                };
                w.ammo_in_magazine = n;
            }
            nothing()
        }
        "unload_magazine" => {
            // No stalker's dying-fire flag is ever set here (7.4).
            if c.is_item(id) {
                let item = c.item(id)?;
                if is_magazine_weapon(item)
                    && let ItemState::Weapon(w) = &mut item.state
                {
                    w.ammo_in_magazine = 0;
                }
            }
            nothing()
        }
        // 8.
        "enable_attachable_item" => {
            let b = match arg(args, 0) {
                Value::Boolean(b) => *b,
                _ => return Err(format!("{op}: boolean expected")),
            };
            if c.is_item(id) {
                enable_attachable(c, id, b);
            }
            nothing()
        }
        "attachable_item_enabled" => boolean(c.is_item(id) && c.state.marks(id).enabled),
        "is_inv_box_empty" => {
            let object = c.object(id)?;
            let inv_box = matches!(object.class, ObjectClass::Unnamed(i) if i == class_id(b"O_INVBOX"))
                || class_kind(&c.system, object).is_some_and(|(_, class)| class == "O_INVBOX");
            boolean(inv_box && !c.objects.iter().any(|o| o.parent == Some(id)))
        }
        // 8.3: no fighter perceives, so every item memory's list stays
        // empty and its selection none (TODO(#1211)).
        "best_item" => one(Value::Nil),
        "best_weapon" => {
            let object = c.object(id)?;
            if !object.kinds.has(Kinds::STALKER) || health(object) <= 0.0 {
                return one(Value::Nil);
            }
            let weapon = best_weapon(c, id)?;
            one(c.handle(weapon))
        }
        _ => Err(format!("inventory: unknown operation {op}")),
    }
}

/// The handles of `ids` as a list.
fn list(c: &Call, ids: &[u16]) -> Value {
    Value::Table(
        ids.iter()
            .enumerate()
            .map(|(i, &id)| (Value::Number((i + 1) as f64), c.handle(Some(id))))
            .collect(),
    )
}

/// The items owner `id` offers for trade (3.4), in order: the backpack,
/// the belt when it counts, the slots' items in slot order (every slot that
/// is not persistent, and slot 3).
fn offered(c: &mut Call, id: u16) -> Result<Vec<u16>, String> {
    let object = c.object(id)?.clone();
    let own = object
        .owner
        .as_ref()
        .map(|o| o.trade.clone())
        .unwrap_or_default();
    let defaults = default_trade(c)?;
    let system = Rc::clone(&c.system);
    let inventory = c.owner(id)?;
    let belt_counts = inventory.belt_counts;
    let slots_count = inventory.slots_count;
    let inventory = &inventory.inventory;
    let mut ids: Vec<u16> = (inventory.items.iter())
        .filter(|i| i.place == Place::Backpack)
        .map(|i| i.id)
        .collect();
    if belt_counts {
        ids.extend(&inventory.belt);
    }
    let slots = if slots_count {
        &inventory.slots[..]
    } else {
        &[]
    };
    for (slot, item) in slots.iter().enumerate() {
        let slot = slot as u8;
        if let Some(item) = item
            && (!inventory.is_persistent(slot) || slot == GRENADE_SLOT)
        {
            ids.push(*item);
        }
    }
    let can_trade = |section: &str| {
        system
            .get(section, "can_trade")
            .flatten()
            .is_none_or(value::bool)
    };
    let allowed = |item: &InventoryItem| {
        let section = cp1251::encode(&item.config.section)
            .unwrap_or_else(|| item.config.section.as_bytes().to_vec());
        // Which part of the trade parameters is unestablished: the sell
        // part here (our choice).
        let by_lists = trade::item_factors(&own.sell, &defaults.sell, &section).is_some();
        let dead = health(&object) <= 0.0;
        if object.kinds.has(Kinds::STALKER) {
            // A live stalker's own keep choice is spec X's: none here.
            !dead || trade::shown(&own.show, &defaults.show, &section)
        } else if is_trader(&object) {
            dead || (item.config.class_id.as_str() != "D_PDA" && by_lists)
        } else {
            by_lists
        }
    };
    Ok(ids
        .into_iter()
        .filter(|&i| {
            inventory.item(i).is_some_and(|item| {
                can_trade(&item.config.section) && !item.config.quest_item && allowed(item)
            })
        })
        .collect())
}

/// The default trade parameters (script API R 5.2), made on first use.
fn default_trade(c: &Call) -> Result<TradeParameters, String> {
    let Some(dialogs) = &c.ctx.dialogs else {
        return Ok(TradeParameters::default());
    };
    let mut d = dialogs.borrow_mut();
    if d.default_trade.is_none() {
        let made = TradeParameters::defaults(&c.system)
            .map_err(|key| c.ctx.failed_check("CTradeParameters", &key))?;
        d.default_trade = Some(made);
    }
    Ok(d.default_trade.clone().expect("made above"))
}

/// `activate_slot(n)` (4.1). No restored slot is ever marked, no slot
/// blocked and no item busy here (the engine has neither).
fn activate_slot(c: &mut Call, id: u16, n: u32) -> Result<(), String> {
    if n != NO_SLOT && n as usize >= SLOT_COUNT {
        return Err(c.ctx.failed_check("activate_slot", "slot out of range"));
    }
    let slot = (n != NO_SLOT).then_some(n as u8);
    if slot.is_some_and(|s| INACTIVE_SLOTS.contains(&s)) {
        return Ok(());
    }
    let o = c.owner(id)?;
    let held = |o: &OwnerInventory, s: u8| o.inventory.slots[usize::from(s)].is_some();
    match (o.inventory.active_slot, slot) {
        (Some(a), Some(s)) if a == s && held(o, s) => return Ok(()),
        (None, None) => return Ok(()),
        (None, Some(s)) if !held(o, s) => {
            if s != GRENADE_SLOT {
                return Ok(());
            }
            let grenade = (o.inventory.items.iter())
                .find(|i| {
                    i.place == Place::Backpack && i.config.slot_number() == Some(GRENADE_SLOT)
                })
                .map(|i| i.id);
            let Some(grenade) = grenade else {
                return Ok(());
            };
            o.inventory.slots[usize::from(GRENADE_SLOT)] = Some(grenade);
            if let Some(i) = o.inventory.items.iter_mut().find(|i| i.id == grenade) {
                i.place = Place::Slot(GRENADE_SLOT);
            }
        }
        (Some(_), Some(s)) if !held(o, s) => return Ok(()),
        _ => {}
    }
    o.pending = Some(slot);
    Ok(())
}

/// The generator of `original-determinism` 2.2: a draw 0 … 32,767.
fn draw(s: &mut u32) -> u16 {
    *s = s.wrapping_mul(214_013).wrapping_add(2_531_011);
    ((*s >> 16) & 32_767) as u16
}

/// `buy_supplies(ini, section)` (5.6).
fn buy_supplies(c: &mut Call, id: u16, ini: Option<IniFile>, section: &[u8]) -> Result<(), String> {
    let ctx = c.ctx;
    // Step 1: every item destroyed, but the bolt and a PDA `obj` owned
    // first.
    let o = c.owner(id)?;
    let destroyed: Vec<u16> = (o.inventory.items.iter())
        .filter(|i| {
            let pda_of_owner = matches!(
                i.state,
                ItemState::Pda { original_owner_id, .. } if original_owner_id == id
            );
            !i.config.is_bolt() && !pda_of_owner
        })
        .map(|i| i.id)
        .collect();
    // Step 2.
    o.restock.clear();
    c.state.events.extend(
        destroyed
            .into_iter()
            .map(|object| Event::Destroy { object }),
    );
    // Step 3: `ini` = nil is believed a crash; a missing section fatal (F 2.4).
    let Some(ini) = ini else {
        return Err(ctx.crash("buy_supplies", "the ini_file is nil"));
    };
    let name = cp1251::decode(section);
    let Some(found) = ini.0.section(&name) else {
        return Err(ctx.failed_check(
            "CInifile::r_section",
            &format!("Can't open section '{name}'"),
        ));
    };
    // In the section's sorted order (F 3.7's `r_line`).
    let lines: Vec<(String, String)> = (found.entries())
        .map(|e| (e.key().to_owned(), e.value().unwrap_or("").to_owned()))
        .collect();
    let mut record = Vec::new();
    for (item, v) in lines {
        let mut parts = v.split(',');
        let count = value::int(parts.next().unwrap_or("").trim()) as u32;
        let probability = value::float(parts.next().unwrap_or("").trim());
        // Step 4: a generator of its own per line.
        let mut s = c.state.performance_counter;
        let mut k: u32 = 0;
        let mut checked = false;
        for _ in 0..count {
            if f32::from(draw(&mut s)) / 32_767.0 <= probability {
                if !checked {
                    let names_item = (c.system.get(&item, "class").flatten())
                        .and_then(|class| {
                            volya_engine_formats::object::ClassId::new(class.trim()).ok()
                        })
                        .is_some_and(|id| is_item_kind(id.kind()));
                    if !names_item {
                        return Err(ctx.failed_check("buy_supplies", "the section names no item"));
                    }
                    checked = true;
                }
                k += 1;
                c.state.events.push(Event::Create {
                    section: crate::game_object::game_bytes(&item),
                    holder: id,
                });
            }
        }
        // Step 5, both unsigned.
        let value = count as f32 / (k as f32).max(0.3) * probability;
        record.push((crate::game_object::game_bytes(&item), value));
    }
    c.owner(id)?.restock = record;
    Ok(())
}

/// `drop_item(item)` (5.1): event 3 for an owner and an item.
fn drop_item(c: &mut Call, id: u16, item: u16) {
    if c.is_owner(id) && c.is_item(item) {
        c.state.events.push(Event::Drop { holder: id, item });
    }
}

/// `eat(item)` (5.5 steps 1 to 4).
fn eat(c: &mut Call, id: u16, item: u16) -> Result<(), String> {
    let ctx = c.ctx;
    let object = c.object(id)?.clone();
    let inventory = c.owner(id)?;
    let Some(held) = inventory.inventory.items.iter_mut().find(|i| i.id == item) else {
        return Err(ctx.failed_check("eat", "the item is not in the inventory"));
    };
    let Some(eatable) = held.config.eatable else {
        return Err(ctx.failed_check("eat", "the item is not eatable"));
    };
    if !object.kinds.has(Kinds::LIVING) {
        return Err(ctx.failed_check("eat", "the object is not living"));
    }
    // `pstate` 5 step 8: the remaining portions.
    let portions = held.portions.get_or_insert(eatable.eat_portions_num);
    *portions = if *portions >= 1 { *portions - 1 } else { 0 };
    let used_up = *portions == 0;
    c.state.events.push(Event::Eat {
        eater: id,
        item,
        eatable,
    });
    // B's game type is single-player here.
    if object.kinds.has(Kinds::PLAYER) {
        c.state.events.push(Event::UseObject { player: id, item });
    }
    if used_up {
        c.state.events.push(Event::Drop { holder: id, item });
        c.state.events.push(Event::Destroy { object: item });
    }
    Ok(())
}

/// The rounds in a weapon's magazine (7.1): 0 for any other item.
fn magazine(item: &InventoryItem) -> u32 {
    match &item.state {
        ItemState::Weapon(w) if is_weapon(item) => u32::from(w.ammo_in_magazine),
        _ => 0,
    }
}

/// A weapon section's ammunition types (7.2): the comma-separated items of
/// its `ammo_class`, spaces around them dropped (believed); none without
/// the key.
fn ammunition_types(system: &Config, section: &str) -> Vec<String> {
    (system.get(section, "ammo_class").flatten())
        .map(|v| {
            (v.split(','))
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// A section's `ef_weapon_type`, unsigned: 4,294,967,295 when absent
/// (8.5).
fn weapon_type(system: &Config, section: &str) -> u32 {
    (system.get(section, "ef_weapon_type").flatten()).map_or(u32::MAX, |v| value::int(v) as u32)
}

/// An ammunition item's box size (7.5): its section's `box_size`, read as
/// an integer and kept as 16 bits; a missing key is fatal (ltx-config.md
/// 11).
fn box_size(ctx: &Context, system: &Config, section: &str) -> Result<u16, String> {
    let s = (system.section(section))
        .ok_or_else(|| ctx.failed_check("ammunition", &format!("no section [{section}]")))?;
    let v = s
        .read("box_size")
        .map_err(|e| ctx.failed_check("ammunition", &e.to_string()))?;
    Ok(value::int(v) as u16)
}

/// Ammunition item `item`'s rounds now (7.5): those its spawn data set,
/// else its box size. Nothing here takes rounds from a box or fills one
/// (no weapon loads or unloads keeping them), and the server's update data
/// never reaches the scripts.
fn rounds(c: &Call, item: &InventoryItem) -> Result<u16, String> {
    match c.state.rounds.get(&item.id) {
        Some(&n) => Ok(n),
        None => box_size(c.ctx, &c.system, &item.config.section),
    }
}

/// `get_ammo_total()` (7.2): the magazine, plus the rounds of the holder's
/// ammunition items in its belt and backpack of the weapon's ammunition
/// types, each at its rounds now; the holder's further count is
/// unestablished: none.
fn ammo_total(c: &mut Call, id: u16) -> Result<u32, String> {
    let item = c.item(id)?.clone();
    if !is_weapon(&item) {
        return Ok(0);
    }
    let mut total = magazine(&item);
    let types = ammunition_types(&c.system, &item.config.section);
    if let Some(holder) = c.state.holder(id) {
        let inventory = &c.state.owners[&holder].inventory;
        for i in &inventory.items {
            if matches!(i.place, Place::Backpack | Place::Belt)
                && i.config.class_id.kind() == Kind::Ammunition
                && types.contains(&i.config.section)
            {
                total = total.wrapping_add(u32::from(rounds(c, i)?));
            }
        }
    }
    Ok(total)
}

/// Whether item `id` is usable (8.4): a weapon whose `get_ammo_total()` is
/// not 0 or that has no ammunition types.
fn usable(c: &mut Call, id: u16) -> Result<bool, String> {
    let item = c.item(id)?;
    if !is_weapon(item) {
        return Ok(false);
    }
    let section = item.config.section.clone();
    Ok(ammunition_types(&c.system, &section).is_empty() || ammo_total(c, id)? != 0)
}

/// Stalker `id`'s best weapon brought up to date (8.4). No enemy is ever
/// chosen here (`best_enemy()` nil, TODO(#1131)): the keep needs no
/// rating, the carried weapons are valued by their type, and the item
/// memory the later steps walk is empty (TODO(#1211)).
fn best_weapon(c: &mut Call, id: u16) -> Result<Option<u16>, String> {
    let state = c.state.best_weapons.get(&id).copied().unwrap_or_default();
    // Step 1: with no enemy, kept while usable.
    if state.chosen
        && let Some(weapon) = state.weapon
        && c.objects.get(weapon).is_some()
        && usable(c, weapon)?
    {
        return Ok(Some(weapon));
    }
    // Step 2.
    let mut best = BestWeapon {
        weapon: None,
        rating: 0.0,
        chosen: true,
    };
    let items: Vec<u16> = c.owner(id)?.inventory.items.iter().map(|i| i.id).collect();
    for item in items {
        if !usable(c, item)? {
            continue;
        }
        let kind = |c: &Call, i: u16| {
            let section = (c.state.item(i)).map(|i| i.config.section.clone());
            weapon_type(&c.system, &section.unwrap_or_default())
        };
        let v = kind(c, item) as f32;
        let taken = v - best.rating >= 0.00001
            || ((v - best.rating).abs() < 0.00001
                && best.weapon.is_none_or(|b| kind(c, item) > kind(c, b)));
        if taken {
            best.weapon = Some(item);
            best.rating = v;
        }
    }
    c.state.best_weapons.insert(id, best);
    Ok(best.weapon)
}

/// `enable_attachable_item(b)` (8.1).
fn enable_attachable(c: &mut Call, id: u16, b: bool) {
    let holder = c.state.holder(id);
    let Some(holder) = holder else {
        c.state.marks.entry(id).or_default().enabled = b;
        return;
    };
    let carries = c
        .objects
        .get(holder)
        .is_some_and(|o| o.kinds.has(Kinds::PLAYER) || o.kinds.has(Kinds::STALKER) || is_trader(o));
    let enabled = c.state.marks(id).enabled;
    if !carries || enabled == b {
        return;
    }
    c.state.marks.entry(id).or_default().enabled = b;
    if let Some(o) = c.objects.get_mut(id) {
        o.visible = b;
    }
    c.state.events.push(Event::Attachment {
        item: id,
        holder,
        attached: b,
    });
}

/// What a state with the inventory registered keeps for the engine's
/// calls.
pub(crate) struct Registered {
    ctx: Context,
}

impl Scripts {
    /// Registers the inventory's names (1.1): after the game object basics
    /// (whose objects, services and files it shares) and the dialogs and
    /// trade, if registered (their default trade parameters), before
    /// [`Scripts::load_globals`].
    /// The state is the objects' ([`GameObjects::inventories`]).
    pub fn register_inventory(&mut self) -> Result<(), Error> {
        let basics = (self.game_object_state.as_ref()).ok_or_else(|| Error {
            kind: ErrorKind::Api,
            message: "the game object basics aren't registered".into(),
        })?;
        let ctx = Context {
            state: basics.objects.borrow().inventories(),
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            dialogs: self.dialogs_state.as_ref().map(|d| d.shared_state()),
            fatal: self.fatal_flag(),
        };
        register(self, &ctx)?;
        self.inventory_state = Some(Registered { ctx });
        Ok(())
    }

    fn inventory_context(&self) -> Result<Context, Error> {
        self.inventory_state
            .as_ref()
            .map(|r| r.ctx.clone())
            .ok_or_else(|| Error {
                kind: ErrorKind::Api,
                message: "the inventory isn't registered".into(),
            })
    }

    /// The inventories, shared.
    pub fn inventories(&self) -> Result<Rc<RefCell<Inventories>>, Error> {
        Ok(self.inventory_context()?.state)
    }

    /// The error of a failed engine call: fatal when it stopped the state.
    fn stop_with(&self, message: String) -> Error {
        let kind = if self.fatal_flag().borrow().is_some() {
            ErrorKind::Fatal
        } else {
            ErrorKind::Api
        };
        Error { kind, message }
    }

    /// Owner `owner`'s inventory takes item `item` (2.2): the outcome of a
    /// granted take, or an item spawned as the owner's child. A section
    /// missing an item's keys is fatal.
    pub fn take_item(&mut self, owner: u16, item: u16) -> Result<(), Error> {
        let ctx = self.inventory_context()?;
        let system = ctx.system();
        let r = take(
            &ctx,
            &mut ctx.objects.borrow_mut(),
            &mut ctx.state.borrow_mut(),
            &system,
            owner,
            item,
        );
        r.map_err(|m| self.stop_with(m))
    }

    /// Item `item`, made, takes the state its spawn data holds (`state`:
    /// a PDA's original owner, a weapon's magazine), in place of its
    /// section's; an ammunition item's rounds are [`Self::spawn_ammunition`]'s.
    pub fn set_item_state(&mut self, item: u16, state: ItemState) -> Result<(), Error> {
        let ctx = self.inventory_context()?;
        if let Some(i) = ctx.state.borrow_mut().item_mut(item) {
            i.state = state;
        }
        Ok(())
    }

    /// Sets owner `owner`'s money (6) to `money`, its inventory made on
    /// first need: the engine's setting as the owner comes online, which
    /// sends no event.
    pub fn set_owner_money(&mut self, owner: u16, money: u32) -> Result<(), Error> {
        let ctx = self.inventory_context()?;
        let system = ctx.system();
        let objects = ctx.objects.borrow();
        let mut state = ctx.state.borrow_mut();
        let r = make_owner(&ctx, &objects, &mut state, &system, owner).map(|o| o.money = money);
        r.map_err(|m| self.stop_with(m))
    }

    /// Ammunition item `item` is spawned with `rounds_left` in its spawn
    /// data (7.5): its rounds become that, a value above its box size the
    /// box size. A section without `box_size` is fatal.
    pub fn spawn_ammunition(&mut self, item: u16, rounds_left: u16) -> Result<(), Error> {
        let ctx = self.inventory_context()?;
        let system = ctx.system();
        let section = (ctx.objects.borrow().get(item))
            .and_then(section_of)
            .unwrap_or_default();
        let r = box_size(&ctx, &system, &section).map(|size| {
            let mut state = ctx.state.borrow_mut();
            state.rounds.insert(item, rounds_left.min(size));
        });
        r.map_err(|m| self.stop_with(m))
    }

    /// Serves the take and drop events as the game's server does in
    /// single play, in order, and hands their outcome back (5.1, `use`
    /// 9.2); the other events are returned, oldest first, for the engine.
    pub fn serve_inventory_events(&mut self) -> Result<Vec<Event>, Error> {
        let ctx = self.inventory_context()?;
        let events = ctx.state.borrow_mut().take_events();
        let mut others = Vec::new();
        for event in events {
            let r = match event {
                Event::Drop { holder, item } => serve_drop(&ctx, holder, item),
                Event::Take { taker, item } => serve_take(&ctx, taker, item),
                other => {
                    others.push(other);
                    Ok(())
                }
            };
            r.map_err(|m| self.stop_with(m))?;
        }
        Ok(others)
    }

    /// The inventories' update (4.1, 5.4), each frame: each pending slot
    /// becomes the active slot, and every marked item of a slot, the
    /// backpack or the belt loses its mark and is dropped (event 3).
    pub fn update_inventories(&mut self) -> Result<(), Error> {
        let ctx = self.inventory_context()?;
        let mut state = ctx.state.borrow_mut();
        let state = &mut *state;
        let mut drops = Vec::new();
        for (&id, o) in &mut state.owners {
            if let Some(pending) = o.pending.take() {
                o.inventory.active_slot = pending;
            }
            for i in &o.inventory.items {
                if i.place != Place::None
                    && let Some(m) = state.marks.get_mut(&i.id)
                    && m.dropped
                {
                    m.dropped = false;
                    drops.push(Event::Drop {
                        holder: id,
                        item: i.id,
                    });
                }
            }
        }
        state.events.extend(drops);
        Ok(())
    }

    /// Forgets destroyed object `id`'s inventory part.
    pub(crate) fn forget_inventory_object(&mut self, id: u16) {
        if let Ok(ctx) = self.inventory_context() {
            ctx.state.borrow_mut().forget(id);
        }
    }
}

fn name_of(objects: &GameObjects, id: u16) -> String {
    (objects.get(id).and_then(|o| o.name.as_deref()))
        .map(cp1251::decode)
        .unwrap_or_default()
}

/// The server's handling of event 3 (5.1).
fn serve_drop(ctx: &Context, holder: u16, item: u16) -> Result<(), String> {
    let frame = ctx.services.borrow().device.frame_count();
    let mut objects = ctx.objects.borrow_mut();
    let (p, e) = (name_of(&objects, holder), name_of(&objects, item));
    ctx.log(&format!("sv reject. id_parent {p} id_entity {e} [{frame}]"));
    if objects.get(holder).is_none() || objects.get(item).is_none() {
        return Err(ctx.failed_check("process_event_reject", "the object doesn't exist"));
    }
    let mut state = ctx.state.borrow_mut();
    let current = state
        .holder(item)
        .or(objects.get(item).and_then(|o| o.parent));
    let Some(current) = current else {
        let section = (objects.get(item).and_then(|o| o.section.as_deref()))
            .map(cp1251::decode)
            .unwrap_or_default();
        ctx.log(&format!(
            "~ ERROR: can't detach independant object. entity[{e}:{item}], parent[{p}:{holder}], section[{section}]"
        ));
        return Ok(());
    };
    if current != holder {
        return Err(ctx.failed_check(
            "process_event_reject",
            "the item's parent is another object",
        ));
    }
    release(&mut objects, &mut state, item);
    Ok(())
}

/// The server's handling of event 1 and its outcome (`use` 9.2): granted
/// when the item has no holder; the taker accepts unless the item is
/// marked for removal.
fn serve_take(ctx: &Context, taker: u16, item: u16) -> Result<(), String> {
    let frame = ctx.services.borrow().device.frame_count();
    let mut objects = ctx.objects.borrow_mut();
    let (p, e) = (name_of(&objects, taker), name_of(&objects, item));
    ctx.log(&format!(
        "sv ownership id_parent {p} id_entity {e} [{frame}]"
    ));
    let mut state = ctx.state.borrow_mut();
    let current = state
        .holder(item)
        .or(objects.get(item).and_then(|o| o.parent));
    if let Some(current) = current {
        let c = name_of(&objects, current);
        ctx.log(&format!(
            "sv !ownership (entity already has parent) new_parent {p} id_parent {c} id_entity {e} [{frame}]"
        ));
        return Ok(());
    }
    let Some(object) = objects.get(item) else {
        return Ok(());
    };
    // What a taker that isn't an owner does is unestablished (5.3):
    // nothing here.
    if object.destroying || !objects.get(taker).is_some_and(is_owner) {
        return Ok(());
    }
    let system = ctx.system();
    take(ctx, &mut objects, &mut state, &system, taker, item)
}
