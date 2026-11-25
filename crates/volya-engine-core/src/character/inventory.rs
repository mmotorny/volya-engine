// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player character's inventory (docs/behavior/player-state.md, cited
//! by section): the items supplied at a New Game (3.1, 3.2), their state
//! and the records a save holds for them (3.1), where each goes (3.3, 6),
//! the active slot (3.3) and the carried weight (3.4).

use std::io;

use volya_engine_formats::cp1251;
use volya_engine_formats::ltx;
use volya_engine_formats::object::{
    self, ClassId, Item, ItemUpdate, Kind, Knife, MagazineWeapon, Object, ObjectBlock,
    ObjectRecord, Outfit, Pda, PlainItem, Q8, SpawnHeader, Torch, Visual, WeaponState,
    WeaponUpdate,
};

use super::condition::{DRINK_CLASS, Eatable};
use crate::config::{Config, Section, value};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The inventory's slots, 0 … 10 (section 1: `slots` is not read).
pub const SLOT_COUNT: usize = 11;
/// The outfit slot: an outfit there is worn (3.3).
pub const OUTFIT_SLOT: u8 = 6;
/// Slots that can never be the active slot (3.3).
pub const INACTIVE_SLOTS: [u8; 4] = [6, 7, 8, 9];
/// The PDA every player receives (3.1 item 2).
pub const PDA_SECTION: &str = "device_pda";
/// The bolt the online player receives (3.2).
pub const BOLT_SECTION: &str = "bolt";
/// Food's class id; food and drink always go to the backpack (3.3).
pub const FOOD_CLASS: &str = "II_FOOD";
/// Ammunition a created weapon holds in reserve, whatever the weapon (3.1).
pub const AMMO_IN_RESERVE: u16 = 90;
/// Installed-addon bits of the Weapon state (3.1).
pub const ADDON_SCOPE: u8 = 0x01;
pub const ADDON_LAUNCHER: u8 = 0x02;
pub const ADDON_SILENCER: u8 = 0x04;
/// An addon status of 2: attachable (3.1).
const ATTACHABLE: i64 = 2;
/// State flags of a created item: every bit but 0x40 (3.1).
pub const ITEM_STATE_FLAGS: u32 = 0xFFFF_FFBF;
/// Spawn flags a save adds (3.1, sav.md 7.2): a version follows, local.
const SAVED_FLAGS: u16 = object::FLAG_VERSION | object::FLAG_LOCAL;
/// No object (ids, spawn id, phantom id).
pub const NO_ID: u16 = 0xFFFF;
/// Probabilities this small count as 1 (3.1).
const PROBABILITY_EPSILON: f32 = 1e-7;

/// `[inventory]` (section 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InventoryConfig {
    pub max_weight: f32,
    pub max_belt: usize,
}

fn required<'a>(section: &'a Section, key: &str) -> io::Result<&'a str> {
    section
        .get(key)
        .flatten()
        .ok_or_else(|| invalid(format!("[{}] has no {key}", section.name())))
}

impl InventoryConfig {
    /// `[inventory]` of `system`; a missing key is an error (our choice).
    pub fn read(system: &Config) -> io::Result<Self> {
        let s = system
            .section("inventory")
            .ok_or_else(|| invalid("no section [inventory]"))?;
        Ok(Self {
            max_weight: value::float(required(s, "max_weight")?),
            max_belt: value::int(required(s, "max_belt")?).max(0) as usize,
        })
    }
}

/// An item section's keys that the player's state reads.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemConfig {
    pub section: String,
    pub class_id: ClassId,
    /// `slot`, as written (only 0 … 10 is a slot, 3.3).
    pub slot: Option<i64>,
    pub belt: bool,
    /// True when absent (3.3).
    pub default_to_ruck: bool,
    /// 0 when absent (our choice).
    pub inv_weight: f32,
    /// `visual`, empty when absent (our choice).
    pub visual: String,
    /// `condition`: the default of a supplied item's condition (3.1).
    pub condition: Option<f32>,
    /// `ammo_mag_size`, 0 when absent (3.1).
    pub ammo_mag_size: u16,
    /// `scope_status`, `grenade_launcher_status`, `silencer_status`, 0 when
    /// absent (our choice).
    pub scope_status: i64,
    pub grenade_launcher_status: i64,
    pub silencer_status: i64,
    /// An outfit's `additional_inventory_weight` and `..._weight2`, 0 when
    /// absent (our choice).
    pub additional_inventory_weight: f32,
    pub additional_inventory_weight2: f32,
    /// An outfit's `power_loss`, `None` when absent (3.4).
    pub power_loss: Option<f32>,
    /// The eatable keys (section 5), for eatable items.
    pub eatable: Option<Eatable>,
}

impl ItemConfig {
    /// Section `name` of `system`. The section and its `class` must exist.
    pub fn read(system: &Config, name: &str) -> io::Result<Self> {
        let s = system
            .section(name)
            .ok_or_else(|| invalid(format!("no item section [{name}]")))?;
        let class_id = ClassId::new(required(s, "class")?.trim())?;
        let get = |key: &str| s.get(key).flatten();
        let float = |key: &str| get(key).map_or(0.0, value::float);
        let int = |key: &str| get(key).map_or(0, value::int);
        let eatable = Eatable::read(s, class_id.as_str())?;
        Ok(Self {
            section: s.name().to_owned(),
            slot: get("slot").map(value::int),
            belt: get("belt").is_some_and(value::bool),
            default_to_ruck: get("default_to_ruck").is_none_or(value::bool),
            inv_weight: float("inv_weight"),
            visual: get("visual").map(value::unquote).unwrap_or("").to_owned(),
            condition: get("condition").map(value::float),
            ammo_mag_size: int("ammo_mag_size").clamp(0, i64::from(u16::MAX)) as u16,
            scope_status: int("scope_status"),
            grenade_launcher_status: int("grenade_launcher_status"),
            silencer_status: int("silencer_status"),
            additional_inventory_weight: float("additional_inventory_weight"),
            additional_inventory_weight2: float("additional_inventory_weight2"),
            power_loss: get("power_loss").map(value::float),
            eatable,
            class_id,
        })
    }

    /// The slot this item goes to, if its `slot` names one (3.3).
    pub fn slot_number(&self) -> Option<u8> {
        self.slot
            .filter(|s| (0..SLOT_COUNT as i64).contains(s))
            .map(|s| s as u8)
    }

    /// The Visual block's model (3.1): `visual` with everything from its
    /// last `.` removed, lowercased.
    pub fn model(&self) -> String {
        let v = self
            .visual
            .rfind('.')
            .map_or(&*self.visual, |i| &self.visual[..i]);
        v.to_lowercase()
    }

    fn food_or_drink(&self) -> bool {
        matches!(self.class_id.as_str(), FOOD_CLASS | DRINK_CLASS)
    }
}

/// Where an item is in the inventory (3.3, 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// In the inventory but in no place (section 6).
    None,
    Slot(u8),
    Belt,
    Backpack,
}

impl Place {
    /// The first byte of the item's client-data item block (section 6).
    pub fn code(self) -> u8 {
        match self {
            Place::None => 0,
            Place::Slot(_) => 1,
            Place::Belt => 2,
            Place::Backpack => 3,
        }
    }
}

/// A weapon's state (3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponItem {
    pub ammo_in_reserve: u16,
    pub ammo_in_magazine: u16,
    pub weapon_state: u8,
    pub installed_addons: u8,
    pub ammo_type: u8,
    /// A magazine weapon's fire mode.
    pub fire_mode: u8,
}

/// The kind-specific state of an item (3.1).
#[derive(Clone, Debug, PartialEq)]
pub enum ItemState {
    /// An item without further state (detectors, food, medkits, …).
    Plain,
    /// The torch byte: bit 0 on, bit 1 night vision on.
    Torch {
        torch_flags: u8,
    },
    Pda {
        original_owner_id: u16,
        specific_character: String,
        info_portion: String,
    },
    Outfit,
    /// Knives and magazine weapons.
    Weapon(WeaponItem),
    /// A kind whose created state the spec does not give (ammunition,
    /// documents, weapons with a grenade launcher, shotguns): held, but no
    /// record is made for it.
    Undescribed,
}

/// One item in the inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryItem {
    pub id: u16,
    pub config: ItemConfig,
    /// 0 … 1.
    pub condition: f32,
    pub place: Place,
    pub state: ItemState,
    /// An eatable's remaining portions (section 5 item 8).
    pub portions: Option<i32>,
}

/// One entry of a `[spawn]` supply section (3.1 item 1).
#[derive(Clone, Debug, PartialEq)]
pub struct SupplyEntry {
    pub section: String,
    pub count: u32,
    pub probability: f32,
    /// `cond=`, when given.
    pub condition: Option<f32>,
    pub scope: bool,
    pub silencer: bool,
    pub launcher: bool,
}

impl SupplyEntry {
    /// Entry `key` with `value` (3.1 item 1).
    fn parse(key: &str, value: Option<&str>) -> Self {
        let v = value.unwrap_or("");
        let count = value::int(v.split(',').next().unwrap_or(""));
        let after = |name: &str| v.find(name).map(|i| value::float(&v[i + name.len()..]));
        let probability = after("prob=")
            .filter(|p| p.abs() >= PROBABILITY_EPSILON)
            .unwrap_or(1.0);
        Self {
            section: key.to_owned(),
            // No shipped line has a negative count; one counts as 1 (our
            // choice), like 0 and a value that is not a number.
            count: if count <= 0 { 1 } else { count as u32 },
            probability,
            condition: after("cond="),
            scope: v.contains("scope"),
            silencer: v.contains("silencer"),
            launcher: v.contains("launcher"),
        }
    }
}

/// A supply text read as a configuration (3.1 items 1 and 3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Supplies {
    /// The `[spawn]` entries in their sorted order.
    pub entries: Vec<SupplyEntry>,
    /// The text has `[dont_spawn_character_supplies]`.
    pub dont_spawn_character_supplies: bool,
}

impl Supplies {
    /// Reads LTX text `text` (a custom data or a `<supplies>` text).
    pub fn parse(text: &str) -> io::Result<Self> {
        let bytes = cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec());
        let config = ltx::parse(&bytes)?;
        Ok(Self {
            entries: config.section("spawn").map_or_else(Vec::new, |s| {
                s.entries()
                    .map(|e| SupplyEntry::parse(e.key(), e.value()))
                    .collect()
            }),
            dont_spawn_character_supplies: config
                .section("dont_spawn_character_supplies")
                .is_some(),
        })
    }
}

/// The 15-bit generator of the supplied object (3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rand15(pub u32);

impl Rand15 {
    pub fn rand15(&mut self) -> u16 {
        self.0 = self.0.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        ((self.0 >> 16) & 0x7FFF) as u16
    }

    /// Whether an item of probability `p` is created: `p` greater than a
    /// draw `r = rand15 × (1/32767)` in single precision. Probability 1 or
    /// more is certain without a draw (the reimplementation the spec
    /// allows).
    pub fn chance(&mut self, p: f32) -> bool {
        p >= 1.0 || p > f32::from(self.rand15()) * (1.0 / 32767.0)
    }
}

/// The ids of the simulation's id pool (3.1). Which ids the game hands out
/// is unestablished; ours are the lowest unused from 1 (our choice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdPool {
    next: u16,
}

impl Default for IdPool {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl IdPool {
    /// The next id; fails when 1 … 65534 are used up.
    pub fn take(&mut self) -> io::Result<u16> {
        if self.next == NO_ID {
            return Err(invalid("the id pool is used up"));
        }
        self.next += 1;
        Ok(self.next - 1)
    }
}

impl InventoryItem {
    /// An item of `config` created with `id` (3.1): condition from the
    /// supply line, else the section's `condition`, else 1.0 (our reading
    /// of 3.1), addons from the line where attachable. `owner` is the
    /// player's id and `specific_character` its specific character (the
    /// PDA's).
    pub fn create(
        config: ItemConfig,
        id: u16,
        line: Option<&SupplyEntry>,
        owner: u16,
        specific_character: &str,
    ) -> Self {
        let condition = line
            .and_then(|l| l.condition)
            .or(config.condition)
            .unwrap_or(1.0);
        let mut addons = 0;
        if let Some(l) = line {
            for (word, status, bit) in [
                (l.scope, config.scope_status, ADDON_SCOPE),
                (l.launcher, config.grenade_launcher_status, ADDON_LAUNCHER),
                (l.silencer, config.silencer_status, ADDON_SILENCER),
            ] {
                if word && status == ATTACHABLE {
                    addons |= bit;
                }
            }
        }
        let state = match config.class_id.kind() {
            Kind::Item => ItemState::Plain,
            Kind::Torch => ItemState::Torch { torch_flags: 0 },
            Kind::Pda => ItemState::Pda {
                original_owner_id: owner,
                specific_character: specific_character.to_owned(),
                info_portion: String::new(),
            },
            Kind::Outfit => ItemState::Outfit,
            Kind::Knife | Kind::MagazineWeapon => ItemState::Weapon(WeaponItem {
                ammo_in_reserve: AMMO_IN_RESERVE,
                ammo_in_magazine: config.ammo_mag_size,
                weapon_state: 0,
                installed_addons: addons,
                ammo_type: 0,
                fire_mode: 0,
            }),
            _ => ItemState::Undescribed,
        };
        Self {
            id,
            portions: config.eatable.map(|e| e.eat_portions_num),
            config,
            condition,
            place: Place::None,
            state,
        }
    }

    /// Whether a save writes it: never the bolt (sav.md 7.1).
    pub fn can_be_saved(&self) -> bool {
        self.config.class_id.as_str() != "II_BOLT"
    }

    /// The item's record as a save writes it (3.1, with the flags a save
    /// adds): a child of `owner`, at its position, game vertex and level
    /// node. The script version is the one this game writes (our choice: the
    /// spec does not give a created item's). The Item update's physics byte
    /// is 0 (believed for items in an inventory) and the weapon update's
    /// zoom 0 (our choice; both unestablished).
    pub fn record(&self, owner: &Owner) -> io::Result<ObjectRecord> {
        let c = &self.config;
        let item = Item {
            object: ObjectBlock {
                game_graph_vertex: owner.game_graph_vertex,
                distance: 0.0,
                direct_control: 1,
                level_graph_node: owner.level_graph_node,
                state_flags: ITEM_STATE_FLAGS,
                custom_data: String::new(),
                story_id: -1,
                spawn_story_id: -1,
            },
            visual: Visual {
                model: c.model(),
                flags: 0,
            },
            condition: self.condition,
        };
        let update = ItemUpdate {
            flags: 0,
            physics: None,
        };
        let q8 = Q8::quantise(self.condition, 0.0, 1.0);
        let weapon = |w: &WeaponItem| {
            (
                WeaponState {
                    item: item.clone(),
                    ammo_in_reserve: w.ammo_in_reserve,
                    ammo_in_magazine: w.ammo_in_magazine,
                    weapon_state: w.weapon_state,
                    installed_addons: w.installed_addons,
                    ammo_type: w.ammo_type,
                },
                WeaponUpdate {
                    item: update.clone(),
                    condition: q8,
                    weapon_flags: 0,
                    ammo_in_magazine: w.ammo_in_magazine,
                    addons: w.installed_addons,
                    ammo_type: w.ammo_type,
                    weapon_state: w.weapon_state,
                    zoom: 0,
                },
            )
        };
        let object = match (&self.state, c.class_id.kind()) {
            (ItemState::Plain, Kind::Item) => Object::Item(PlainItem {
                item: item.clone(),
                update,
            }),
            (ItemState::Torch { torch_flags }, Kind::Torch) => Object::Torch(Torch {
                item: item.clone(),
                update,
                torch_flags: *torch_flags,
            }),
            (
                ItemState::Pda {
                    original_owner_id,
                    specific_character,
                    info_portion,
                },
                Kind::Pda,
            ) => Object::Pda(Pda {
                item: item.clone(),
                original_owner_id: *original_owner_id,
                specific_character: specific_character.clone(),
                info_portion: info_portion.clone(),
                update,
            }),
            (ItemState::Outfit, Kind::Outfit) => Object::Outfit(Outfit {
                item: item.clone(),
                update,
                condition: q8,
            }),
            (ItemState::Weapon(w), Kind::Knife) => {
                let (weapon, update) = weapon(w);
                Object::Knife(Knife { weapon, update })
            }
            (ItemState::Weapon(w), Kind::MagazineWeapon) => {
                let (weapon, update) = weapon(w);
                Object::MagazineWeapon(MagazineWeapon {
                    weapon,
                    update,
                    fire_mode: w.fire_mode,
                })
            }
            _ => {
                return Err(invalid(format!(
                    "{}: no record for a created {} (not in the spec)",
                    c.section,
                    c.class_id.kind().name()
                )));
            }
        };
        Ok(ObjectRecord {
            header: SpawnHeader {
                section: c.section.clone(),
                name: format!("{}{:04}", c.section, self.id),
                game_type: 1,
                after_game_type: 255,
                position: owner.position,
                angles: [0.0; 3],
                respawn_time: 0,
                id: self.id,
                parent_id: owner.id,
                phantom_id: NO_ID,
                flags: SAVED_FLAGS,
                version: object::VERSION,
                script_version: object::SCRIPT_VERSION,
                client_data: Vec::new(),
                spawn_id: NO_ID,
            },
            class_id: c.class_id.clone(),
            single_player: true,
            object,
        })
    }
}

/// What an item's record takes from the player that holds it (3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Owner {
    pub id: u16,
    pub position: [f32; 3],
    pub game_graph_vertex: u16,
    pub level_graph_node: u32,
}

/// The inventory: its items in the order the player lists them as
/// children (3.1), with their places and the active slot (3.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub config: InventoryConfig,
    pub items: Vec<InventoryItem>,
    /// The item in each slot.
    pub slots: [Option<u16>; SLOT_COUNT],
    pub belt: Vec<u16>,
    /// None at first (3.3).
    pub active_slot: Option<u8>,
}

impl Inventory {
    pub fn new(config: InventoryConfig) -> Self {
        Self {
            config,
            items: Vec::new(),
            slots: [None; SLOT_COUNT],
            belt: Vec::new(),
            active_slot: None,
        }
    }

    /// Takes `item` into the inventory by the rules of 3.3 and attaches it
    /// as the last child. Returns its place.
    pub fn take(&mut self, mut item: InventoryItem) -> Place {
        let c = &item.config;
        let place = match c.slot_number() {
            _ if c.food_or_drink() => Place::Backpack,
            Some(s) if self.slots[usize::from(s)].is_none() => Place::Slot(s),
            // The further test before the belt is unestablished: none here.
            _ if c.belt && !c.default_to_ruck && self.belt.len() < self.config.max_belt => {
                Place::Belt
            }
            _ => Place::Backpack,
        };
        item.place = place;
        self.attach(item);
        place
    }

    /// Takes `item` whose client-data item block stores place `code`
    /// (section 6); code 0 follows 3.3. A stored slot that is not
    /// available, and a stored belt that is full (our choice), leave it in
    /// no place.
    pub fn take_stored(&mut self, mut item: InventoryItem, code: u8) -> Place {
        let code = if code != 0 && item.config.food_or_drink() {
            3
        } else {
            code
        };
        let place = match code {
            0 => return self.take(item),
            1 => match item.config.slot_number() {
                Some(s) if self.slots[usize::from(s)].is_none() => Place::Slot(s),
                _ => Place::None,
            },
            2 if self.belt.len() < self.config.max_belt => Place::Belt,
            3 => Place::Backpack,
            // Our choice: an unknown code is no place.
            _ => Place::None,
        };
        item.place = place;
        self.attach(item);
        place
    }

    fn attach(&mut self, item: InventoryItem) {
        match item.place {
            Place::Slot(s) => {
                self.slots[usize::from(s)] = Some(item.id);
                // 3.3: the first item put into a slot that can be active
                // makes it the active slot (no pending state: our choice,
                // nothing animates bringing it into the hands).
                if self.active_slot.is_none() && !INACTIVE_SLOTS.contains(&s) {
                    self.active_slot = Some(s);
                }
            }
            Place::Belt => self.belt.push(item.id),
            Place::Backpack | Place::None => {}
        }
        self.items.push(item);
    }

    pub fn item(&self, id: u16) -> Option<&InventoryItem> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Removes and returns item `id`. Its slot stops being active (our
    /// choice).
    pub fn remove(&mut self, id: u16) -> Option<InventoryItem> {
        let index = self.items.iter().position(|i| i.id == id)?;
        let item = self.items.remove(index);
        if let Place::Slot(s) = item.place {
            self.slots[usize::from(s)] = None;
            if self.active_slot == Some(s) {
                self.active_slot = None;
            }
        }
        self.belt.retain(|&b| b != id);
        Some(item)
    }

    /// The carried weight `M` (3.4): every item's `inv_weight`, in any
    /// place. A weapon's installed addons and loaded rounds are not added
    /// (our omission: the spec does not give their weights' keys).
    pub fn weight(&self) -> f32 {
        self.items.iter().map(|i| i.config.inv_weight).sum()
    }

    /// The worn outfit: an outfit in the outfit slot.
    pub fn worn_outfit(&self) -> Option<&InventoryItem> {
        self.slots[usize::from(OUTFIT_SLOT)]
            .and_then(|id| self.item(id))
            .filter(|i| i.state == ItemState::Outfit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: &str = "\
        [detector_simple]\nclass = D_SIMDET\nslot = 8\ndefault_to_ruck = false\ninv_weight = 0\n\
        visual = equipments\\item_datchik1.ogf\n\
        [device_torch]\nclass = TORCH_S\nslot = 9\ninv_weight = 0\nvisual = equipments\\light_night\n\
        [novice_outfit]\nclass = E_STLK\nslot = 6\ninv_weight = 3.0\n\
        visual = equipments\\novice_suit.ogf\nadditional_inventory_weight = 0\n\
        additional_inventory_weight2 = 0\n\
        [wpn_binoc]\nclass = WP_BINOC\nslot = 4\ninv_weight = 1.0\nammo_mag_size = 0\n\
        scope_status = 1\nsilencer_status = 0\ngrenade_launcher_status = 0\n\
        visual = weapons\\binoculars\\wpn_binoculars.ogf\n\
        [wpn_pm]\nclass = WP_PM\nslot = 1\ninv_weight = 1.0\nammo_mag_size = 8\n\
        scope_status = 0\nsilencer_status = 2\ngrenade_launcher_status = 0\n\
        [device_pda]\nclass = D_PDA\nslot = 7\ninv_weight = 0.3\nvisual = equipments\\item_pda\n\
        [bolt]\nclass = II_BOLT\nslot = 5\ninv_weight = 0\n\
        [bread]\nclass = II_FOOD\nslot = 4\ninv_weight = 0.3\neat_health = 0.05\neat_power = 0\n\
        eat_satiety = 0.2\neat_radiation = 0\nwounds_heal_perc = 0\neat_portions_num = -1\n\
        [af_medusa]\nclass = ARTEFACT\nbelt = true\ndefault_to_ruck = false\ninv_weight = 0.5\n\
        [medkit]\nclass = II_MEDKI\ninv_weight = 0.1\n\
        [inventory]\nmax_weight = 50\nmax_belt = 5\n";

    fn system() -> Config {
        ltx::parse(ITEMS.as_bytes()).unwrap()
    }

    fn item(system: &Config, section: &str, id: u16) -> InventoryItem {
        InventoryItem::create(
            ItemConfig::read(system, section).unwrap(),
            id,
            None,
            0,
            "actor",
        )
    }

    #[test]
    fn parses_supply_lines() {
        let s = Supplies::parse(
            "[dont_spawn_character_supplies]\r\n\r\n[spawn]\r\nwpn_binoc\r\ndetector_simple\r\n\
             novice_outfit\r\ndevice_torch\r\n",
        )
        .unwrap();
        assert!(s.dont_spawn_character_supplies);
        let names: Vec<_> = s.entries.iter().map(|e| e.section.as_str()).collect();
        // Sorted byte-wise, not in file order.
        assert_eq!(
            names,
            [
                "detector_simple",
                "device_torch",
                "novice_outfit",
                "wpn_binoc"
            ]
        );
        assert!(
            s.entries
                .iter()
                .all(|e| e.count == 1 && e.probability == 1.0 && e.condition.is_none() && !e.scope)
        );
        let s = Supplies::parse(
            "[spawn]\nwpn_pm = 2, scope, silencer, prob=0.5, cond=0.25\nammo = 0\nb = x\n\
             c = 3, prob=0.00000001\nb = 4\n",
        )
        .unwrap();
        let e = &s.entries;
        assert!(!s.dont_spawn_character_supplies);
        assert_eq!(e.len(), 4);
        assert_eq!((e[0].section.as_str(), e[0].count), ("ammo", 1));
        // A key on several lines is one entry, the last value winning.
        assert_eq!((e[1].section.as_str(), e[1].count), ("b", 4));
        assert_eq!((e[2].count, e[2].probability), (3, 1.0));
        let pm = &e[3];
        assert_eq!(
            (pm.count, pm.probability, pm.condition),
            (2, 0.5, Some(0.25))
        );
        assert!(pm.scope && pm.silencer && !pm.launcher);
        assert_eq!(Supplies::parse("").unwrap(), Supplies::default());
    }

    #[test]
    fn draws_like_the_generator() {
        let mut r = Rand15(0);
        // x = 0x269EC3, then 0x269EC3 × 0x343FD + 0x269EC3.
        assert_eq!(r.rand15(), 0x0026);
        assert_eq!(
            r.rand15(),
            (0x269EC3u32.wrapping_mul(0x343FD).wrapping_add(0x269EC3) >> 16) as u16 & 0x7FFF
        );
        let mut r = Rand15(12345);
        assert!((0..100).all(|_| r.chance(1.0)));
        assert!((0..100).all(|_| !r.chance(0.0)));
        let mut r = Rand15(7);
        let hits = (0..10_000).filter(|_| r.chance(0.25)).count();
        assert!((2000..3000).contains(&hits), "{hits}");
    }

    #[test]
    fn creates_items_with_their_state() {
        let system = system();
        let binoc = item(&system, "wpn_binoc", 4);
        assert_eq!(
            binoc.state,
            ItemState::Weapon(WeaponItem {
                ammo_in_reserve: 90,
                ammo_in_magazine: 0,
                weapon_state: 0,
                installed_addons: 0,
                ammo_type: 0,
                fire_mode: 0,
            })
        );
        assert_eq!(binoc.condition, 1.0);
        // Addons only where attachable: the PM takes a silencer, not a scope.
        let line = SupplyEntry::parse("wpn_pm", Some("1,scope,silencer,launcher,cond=0.5"));
        let pm = InventoryItem::create(
            ItemConfig::read(&system, "wpn_pm").unwrap(),
            9,
            Some(&line),
            0,
            "actor",
        );
        let ItemState::Weapon(w) = pm.state else {
            panic!("{:?}", pm.state)
        };
        assert_eq!(
            (w.installed_addons, w.ammo_in_magazine),
            (ADDON_SILENCER, 8)
        );
        assert_eq!(pm.condition, 0.5);
        assert_eq!(
            item(&system, "device_pda", 5).state,
            ItemState::Pda {
                original_owner_id: 0,
                specific_character: "actor".into(),
                info_portion: String::new()
            }
        );
        assert_eq!(
            item(&system, "device_torch", 2).state,
            ItemState::Torch { torch_flags: 0 }
        );
        assert_eq!(item(&system, "bread", 7).portions, Some(-1));
        assert_eq!(item(&system, "detector_simple", 1).portions, None);
    }

    #[test]
    fn places_items_and_the_active_slot() {
        let system = system();
        let mut inv = Inventory::new(InventoryConfig::read(&system).unwrap());
        for (id, section) in [
            "detector_simple",
            "device_torch",
            "novice_outfit",
            "wpn_binoc",
            "device_pda",
            "bolt",
        ]
        .iter()
        .enumerate()
        {
            inv.take(item(&system, section, id as u16 + 1));
        }
        let places: Vec<_> = inv.items.iter().map(|i| i.place).collect();
        assert_eq!(places, [8, 9, 6, 4, 7, 5].map(Place::Slot),);
        // Detector, torch and outfit go to slots that cannot be active.
        assert_eq!(inv.active_slot, Some(4));
        assert!(inv.belt.is_empty());
        assert!((inv.weight() - 4.3).abs() < 1e-6);
        assert_eq!(inv.worn_outfit().map(|o| o.id), Some(3));
        // Food goes to the backpack despite its slot; a second binocular
        // finds its slot taken; a belt item goes to the belt until full.
        assert_eq!(inv.take(item(&system, "bread", 10)), Place::Backpack);
        assert_eq!(inv.take(item(&system, "wpn_binoc", 11)), Place::Backpack);
        assert_eq!(inv.take(item(&system, "medkit", 12)), Place::Backpack);
        for id in 20..25 {
            assert_eq!(inv.take(item(&system, "af_medusa", id)), Place::Belt);
        }
        assert_eq!(inv.take(item(&system, "af_medusa", 25)), Place::Backpack);
        // Children keep the order taken.
        assert_eq!(inv.items.last().unwrap().id, 25);
        // Removing the active slot's item clears it.
        inv.remove(4).unwrap();
        assert_eq!((inv.slots[4], inv.active_slot), (None, None));
    }

    #[test]
    fn stored_places_override_the_rules() {
        let system = system();
        let mut inv = Inventory::new(InventoryConfig::read(&system).unwrap());
        assert_eq!(
            inv.take_stored(item(&system, "wpn_binoc", 1), 3),
            Place::Backpack
        );
        assert_eq!(
            inv.take_stored(item(&system, "wpn_pm", 2), 1),
            Place::Slot(1)
        );
        // A stored slot that is taken, or a section without one: no place.
        assert_eq!(inv.take_stored(item(&system, "wpn_pm", 3), 1), Place::None);
        assert_eq!(inv.take_stored(item(&system, "medkit", 4), 1), Place::None);
        // Food is set to the backpack first.
        assert_eq!(
            inv.take_stored(item(&system, "bread", 5), 1),
            Place::Backpack
        );
        assert_eq!(inv.take_stored(item(&system, "medkit", 6), 2), Place::Belt);
        assert_eq!(
            inv.take_stored(item(&system, "wpn_binoc", 7), 0),
            Place::Slot(4)
        );
        assert_eq!(
            [Place::None, Place::Slot(3), Place::Belt, Place::Backpack].map(Place::code),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn records_round_trip() {
        let system = system();
        let owner = Owner {
            id: 0,
            position: [1.0, 2.0, 3.0],
            game_graph_vertex: 7,
            level_graph_node: 99,
        };
        for (section, id) in [
            ("detector_simple", 1),
            ("device_torch", 2),
            ("novice_outfit", 3),
            ("wpn_binoc", 4),
            ("device_pda", 5),
        ] {
            let record = item(&system, section, id).record(&owner).unwrap();
            let h = &record.header;
            assert_eq!(h.name, format!("{section}000{id}"));
            assert_eq!((h.game_type, h.after_game_type), (1, 255));
            assert_eq!((h.parent_id, h.phantom_id, h.spawn_id), (0, NO_ID, NO_ID));
            assert_eq!(h.position, owner.position);
            assert_eq!(h.flags, 0x21);
            let block = record.object.object_block();
            assert_eq!(block.state_flags, 0xFFFF_FFBF);
            assert_eq!((block.game_graph_vertex, block.level_graph_node), (7, 99));
            assert!(record.can_be_saved());
            let (spawn, update) = record.write().unwrap();
            let back =
                ObjectRecord::parse(&spawn, &update, record.class_id.as_str(), true).unwrap();
            assert_eq!(back, record);
        }
        let outfit = item(&system, "novice_outfit", 3).record(&owner).unwrap();
        assert_eq!(
            outfit.object.visual().unwrap().model,
            "equipments\\novice_suit"
        );
        let Object::Outfit(o) = &outfit.object else {
            panic!()
        };
        assert_eq!(o.condition, Q8(255));
        let torch = item(&system, "device_torch", 2).record(&owner).unwrap();
        assert_eq!(
            torch.object.visual().unwrap().model,
            "equipments\\light_night"
        );
        assert!(!item(&system, "bolt", 6).can_be_saved());
    }

    #[test]
    fn hands_out_ids() {
        let mut pool = IdPool::default();
        assert_eq!((pool.take().unwrap(), pool.take().unwrap()), (1, 2));
        let mut full = IdPool { next: 0xFFFE };
        assert_eq!(full.take().unwrap(), 0xFFFE);
        assert!(full.take().is_err());
    }
}
