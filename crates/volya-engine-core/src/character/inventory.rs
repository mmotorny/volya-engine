// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player character's inventory (docs/behavior/player-state.md, cited
//! by section): the items supplied at a New Game (3.1, 3.2), their state
//! and the records a save holds for them (3.1), where each goes (3.3, 6),
//! the active slot (3.3) and the carried weight (3.4); and the moves
//! between places the inventory screen makes
//! (docs/behavior/inventory-screen.md section 5.4, cited as `screen`).

use std::cmp::Ordering;
use std::io;

use volya_engine_formats::cp1251;
use volya_engine_formats::ltx;
use volya_engine_formats::object::{
    self, Ammunition, ClassId, Item, ItemUpdate, Kind, Knife, MagazineWeapon, Object, ObjectBlock,
    ObjectRecord, Outfit, Pda, PlainItem, Q8, Shotgun, SpawnHeader, Torch, Visual, WeaponState,
    WeaponUpdate, WeaponWithGrenadeLauncher,
};

use super::condition::{DRINK_CLASS, Eatable};
use super::hits::OutfitProtection;
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

/// The slot of grenades, whose item the screen shows in the backpack list
/// (screen 3.2).
pub const GRENADE_SLOT: u8 = 3;

/// `[inventory]` (section 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InventoryConfig {
    pub max_weight: f32,
    pub max_belt: usize,
    /// Each slot's `slot_persistent_<n + 1>`, false when absent (screen
    /// section 9: slot 10 has no key and is not persistent).
    pub persistent: [bool; SLOT_COUNT],
}

fn required<'a>(section: Section<'a>, key: &str) -> io::Result<&'a str> {
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
        let mut persistent = [false; SLOT_COUNT];
        for (n, p) in persistent.iter_mut().enumerate() {
            *p = s
                .get(&format!("slot_persistent_{}", n + 1))
                .flatten()
                .is_some_and(value::bool);
        }
        Ok(Self {
            max_weight: value::float(required(s, "max_weight")?),
            max_belt: value::int(required(s, "max_belt")?).max(0) as usize,
            persistent,
        })
    }
}

/// An item's grid keys (screen 4.2): its icon at (`inv_grid_x`,
/// `inv_grid_y`) grid units of the icon texture, `inv_grid_width` ×
/// `inv_grid_height` cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvGrid {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
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
    /// Read for every item (3.4); missing, it is fatal (ltx-config.md 11).
    pub inv_weight: f32,
    /// `visual`, the Visual block's model (3.1); empty when absent, as the
    /// key is set only if present (object-visual-and-transform.md 2 item 1).
    pub visual: String,
    /// `condition`: the default of a supplied item's condition (3.1).
    pub condition: Option<f32>,
    /// `ammo_mag_size`, 0 when absent (3.1).
    pub ammo_mag_size: u16,
    /// An ammunition item's `box_size` (script-api-inventory.md 7.5), kept
    /// as 16 bits; 0 when absent.
    pub box_size: u16,
    /// `scope_status`, `grenade_launcher_status`, `silencer_status`: read
    /// for a weapon (3.1; screen 4.2 reads all three for each weapon cell
    /// item), missing it is fatal; 0 for other items, which do not read
    /// them.
    pub scope_status: i64,
    pub grenade_launcher_status: i64,
    pub silencer_status: i64,
    /// An outfit's `additional_inventory_weight` and `..._weight2`: read
    /// for an outfit (3.4), missing it is fatal; 0 for other items.
    pub additional_inventory_weight: f32,
    pub additional_inventory_weight2: f32,
    /// An outfit's `power_loss`, `None` when absent (3.4).
    pub power_loss: Option<f32>,
    /// The eatable keys (section 5), for eatable items.
    pub eatable: Option<Eatable>,
    /// The four `inv_grid_*` keys, read as unsigned 32-bit; `None` when one
    /// is missing (the screen stops with an error then, screen 4.2).
    pub inv_grid: Option<InvGrid>,
    /// `quest_item = true`: never dropped (screen 5.6, 5.7).
    pub quest_item: bool,
    /// An outfit's `nightvision_sect`, the section of its night-vision
    /// effect; `None` when absent or for other items (torch.md 15.1).
    pub nightvision_sect: Option<String>,
    /// An outfit's protection against hits (player-hits.md 4.4), read for
    /// an outfit (a missing key is fatal); `None` for other items.
    pub protection: Option<OutfitProtection>,
}

impl ItemConfig {
    /// Section `name` of `system`. The section and its `class` must exist.
    pub fn read(system: &Config, name: &str) -> io::Result<Self> {
        let s = system
            .section(name)
            .ok_or_else(|| invalid(format!("no item section [{name}]")))?;
        let class_id = ClassId::new(required(s, "class")?.trim())?;
        let get = |key: &str| s.get(key).flatten();
        let int = |key: &str| get(key).map_or(0, value::int);
        // Keys read only for some kinds: missing, fatal for those
        // (ltx-config.md 11), 0 for the rest.
        // TODO(#330): the specs read the addon statuses of "a weapon"
        // (pstate 3.1, screen 4.2) without saying whether a knife reads them;
        // it is counted as a weapon, as `is_weapon` does.
        let kind = class_id.kind();
        let weapon = matches!(
            kind,
            Kind::Knife | Kind::MagazineWeapon | Kind::WeaponWithGrenadeLauncher | Kind::Shotgun
        );
        let int_of = |read: bool, key: &str| -> io::Result<i64> {
            if read {
                s.read(key).map(value::int)
            } else {
                Ok(0)
            }
        };
        let float_of = |read: bool, key: &str| -> io::Result<f32> {
            if read {
                s.read(key).map(value::float)
            } else {
                Ok(0.0)
            }
        };
        let outfit = kind == Kind::Outfit;
        let eatable = Eatable::read(s, class_id.as_str())?;
        let grid = |key: &str| get(key).map(|v| value::int(v) as u32);
        let inv_grid = match (
            grid("inv_grid_x"),
            grid("inv_grid_y"),
            grid("inv_grid_width"),
            grid("inv_grid_height"),
        ) {
            (Some(x), Some(y), Some(width), Some(height)) => Some(InvGrid {
                x,
                y,
                width,
                height,
            }),
            _ => None,
        };
        Ok(Self {
            section: s.name().to_owned(),
            slot: get("slot").map(value::int),
            belt: get("belt").is_some_and(value::bool),
            default_to_ruck: get("default_to_ruck").is_none_or(value::bool),
            inv_weight: value::float(s.read("inv_weight")?),
            visual: get("visual").map(value::unquote).unwrap_or("").to_owned(),
            condition: get("condition").map(value::float),
            ammo_mag_size: int("ammo_mag_size").clamp(0, i64::from(u16::MAX)) as u16,
            box_size: int("box_size") as u16,
            scope_status: int_of(weapon, "scope_status")?,
            grenade_launcher_status: int_of(weapon, "grenade_launcher_status")?,
            silencer_status: int_of(weapon, "silencer_status")?,
            additional_inventory_weight: float_of(outfit, "additional_inventory_weight")?,
            additional_inventory_weight2: float_of(outfit, "additional_inventory_weight2")?,
            power_loss: get("power_loss").map(value::float),
            eatable,
            inv_grid,
            quest_item: get("quest_item").is_some_and(value::bool),
            nightvision_sect: (get("nightvision_sect").filter(|_| outfit))
                .map(|v| value::unquote(v).to_owned()),
            protection: if outfit {
                Some(OutfitProtection::read(system, s)?)
            } else {
                None
            },
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

    /// The grid size *w* × *h* in cells (screen 4.2), 0 × 0 without the
    /// grid keys.
    pub fn grid_size(&self) -> (u32, u32) {
        self.inv_grid.map_or((0, 0), |g| (g.width, g.height))
    }

    /// Whether the item is a weapon (knives and the guns).
    pub fn is_weapon(&self) -> bool {
        matches!(
            self.class_id.kind(),
            Kind::Knife | Kind::MagazineWeapon | Kind::WeaponWithGrenadeLauncher | Kind::Shotgun
        )
    }

    /// Whether the item is eatable (section 5).
    pub fn is_eatable(&self) -> bool {
        self.eatable.is_some()
    }

    /// The bolt (screen 5.6: the one item that may not go to the
    /// backpack).
    pub fn is_bolt(&self) -> bool {
        self.class_id.as_str() == "II_BOLT"
    }
}

/// The order of the screen's backpack and of the belt test (screen 4.3):
/// `a` before `b` when its grid is wider, then taller; of the same section,
/// the higher object id first; of different sections, by section name (the
/// original's order there is unestablished; the spec allows this one).
pub fn placement_order(a: &InventoryItem, b: &InventoryItem) -> Ordering {
    let (aw, ah) = a.config.grid_size();
    let (bw, bh) = b.config.grid_size();
    bw.cmp(&aw).then(bh.cmp(&ah)).then_with(|| {
        if a.config.section == b.config.section {
            b.id.cmp(&a.id)
        } else {
            a.config.section.cmp(&b.config.section)
        }
    })
}

/// What a move to an item's slot did (screen 5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotMove {
    /// Nothing changed.
    Failed,
    /// The item is in its slot now (and in the hands when the slot can be
    /// active).
    Moved,
    /// The slot is the grenades' (3), shown in no slot list: nothing
    /// further happens, but the move counts as done.
    Done,
}

impl SlotMove {
    pub fn succeeded(self) -> bool {
        self != SlotMove::Failed
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
    /// Knives, magazine weapons, weapons with a grenade launcher and
    /// shotguns.
    Weapon(WeaponItem),
    /// Ammunition and its rounds left.
    Ammunition {
        rounds_left: u16,
    },
    /// A kind whose created state the spec does not give (documents): held,
    /// but no record is made for it.
    Undescribed,
}

impl ItemState {
    /// The state an item's record `object` holds (what
    /// [`InventoryItem::record`] writes); `None` for a kind without one.
    pub fn of_record(object: &Object) -> Option<Self> {
        let weapon = |w: &WeaponState, fire_mode: u8| {
            ItemState::Weapon(WeaponItem {
                ammo_in_reserve: w.ammo_in_reserve,
                ammo_in_magazine: w.ammo_in_magazine,
                weapon_state: w.weapon_state,
                installed_addons: w.installed_addons,
                ammo_type: w.ammo_type,
                fire_mode,
            })
        };
        Some(match object {
            Object::Item(_) => ItemState::Plain,
            Object::Torch(t) => ItemState::Torch {
                torch_flags: t.torch_flags,
            },
            Object::Pda(p) => ItemState::Pda {
                original_owner_id: p.original_owner_id,
                specific_character: p.specific_character.to_string(),
                info_portion: p.info_portion.to_string(),
            },
            Object::Outfit(_) => ItemState::Outfit,
            Object::Ammunition(a) => ItemState::Ammunition {
                rounds_left: a.rounds_left,
            },
            Object::Knife(k) => weapon(&k.weapon, 0),
            Object::MagazineWeapon(w) => weapon(&w.weapon, w.fire_mode),
            Object::WeaponWithGrenadeLauncher(w) => weapon(&w.weapon, w.fire_mode),
            Object::Shotgun(w) => weapon(&w.weapon, w.fire_mode),
            _ => return None,
        })
    }
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
    /// One item of `section` with probability 1 (the supplies step's PDA,
    /// character-identity.md 4.1 step 2).
    pub fn one(section: &str) -> Self {
        Self::parse(section, None)
    }

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
    /// Reads LTX text `text` (a custom data or a `<supplies>` text); an
    /// include line is an error ([`Supplies::of`] reads one with its
    /// includes).
    pub fn parse(text: &str) -> io::Result<Self> {
        Ok(Self::of(&ltx::parse(&Self::bytes(text))?))
    }

    /// The text's bytes as a configuration reads them.
    pub fn bytes(text: &str) -> Vec<u8> {
        cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec())
    }

    /// The supplies of a text read as configuration `config`
    /// (new-game-supplies.md 1.1).
    pub fn of(config: &Config) -> Self {
        Self {
            entries: config.section("spawn").map_or_else(Vec::new, |s| {
                s.entries()
                    .map(|e| SupplyEntry::parse(e.key(), e.value()))
                    .collect()
            }),
            dont_spawn_character_supplies: config
                .section("dont_spawn_character_supplies")
                .is_some(),
        }
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

    /// Whether an item of probability `p` is created: one draw, also for
    /// probability 1, and `p` greater than `r = rand15 × (1/32767)` in
    /// single precision (new-game-supplies.md 2).
    pub fn chance(&mut self, p: f32) -> bool {
        creates(p, self.rand15())
    }
}

/// Whether draw `r` creates an item of probability `p`
/// (new-game-supplies.md 2): `p` greater than `r × (1/32767)`, both in
/// single precision.
pub fn creates(p: f32, r: u16) -> bool {
    p > f32::from(r) * (1.0f32 / 32767.0)
}

/// Reads supply `entries` (new-game-supplies.md 1.1, 2) with the supplied
/// object's generator `rng`: each count unit of each entry one draw, in
/// order, and `create` called for each item the draw creates before the
/// next draw.
pub fn read_supplies(
    entries: &[SupplyEntry],
    rng: &mut Rand15,
    mut create: impl FnMut(&SupplyEntry) -> io::Result<()>,
) -> io::Result<()> {
    for e in entries {
        for _ in 0..e.count {
            if rng.chance(e.probability) {
                create(e)?;
            }
        }
    }
    Ok(())
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
    /// A pool whose first id is `first` (our choice: 0 and 0xFFFF are not
    /// ids, so they start it at 1).
    pub fn starting_at(first: u16) -> Self {
        Self {
            next: first.clamp(1, NO_ID),
        }
    }

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
            // A created ammunition item holds a full box (script-api-inventory.md
            // 7.5, believed for supplies).
            Kind::Ammunition => ItemState::Ammunition {
                rounds_left: config.box_size,
            },
            // A weapon with a grenade launcher and a shotgun as a magazine
            // weapon (our choice: 3.1 gives the knife's and the magazine
            // weapon's state only).
            Kind::Knife
            | Kind::MagazineWeapon
            | Kind::WeaponWithGrenadeLauncher
            | Kind::Shotgun => ItemState::Weapon(WeaponItem {
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
                custom_data: String::new().into(),
                story_id: -1,
                spawn_story_id: -1,
            },
            visual: Visual {
                model: c.model().into(),
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
            ) => Object::Pda(Box::new(Pda {
                item: item.clone(),
                original_owner_id: *original_owner_id,
                specific_character: specific_character.clone().into(),
                info_portion: info_portion.clone().into(),
                update,
            })),
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
            // Our choice: the grenade mode off and no loaded rounds listed
            // (the update's values are unestablished, as 3.1's).
            (ItemState::Weapon(w), Kind::WeaponWithGrenadeLauncher) => {
                let (weapon, update) = weapon(w);
                Object::WeaponWithGrenadeLauncher(WeaponWithGrenadeLauncher {
                    weapon,
                    grenade_mode: false,
                    update,
                    fire_mode: w.fire_mode,
                })
            }
            (ItemState::Weapon(w), Kind::Shotgun) => {
                let (weapon, update) = weapon(w);
                Object::Shotgun(Box::new(Shotgun {
                    weapon,
                    update,
                    fire_mode: w.fire_mode,
                    loaded_ammo_types: Vec::new(),
                }))
            }
            (ItemState::Ammunition { rounds_left }, Kind::Ammunition) => {
                Object::Ammunition(Ammunition {
                    item: item.clone(),
                    rounds_left: *rounds_left,
                    update,
                    update_rounds_left: *rounds_left,
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
                section: c.section.clone().into(),
                name: format!("{}{:04}", c.section, self.id).into(),
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
    /// (our omission: the spec does not give their weights' keys; TODO(#111)).
    pub fn weight(&self) -> f32 {
        self.items.iter().map(|i| i.config.inv_weight).sum()
    }

    /// The worn outfit: an outfit in the outfit slot.
    pub fn worn_outfit(&self) -> Option<&InventoryItem> {
        self.slots[usize::from(OUTFIT_SLOT)]
            .and_then(|id| self.item(id))
            .filter(|i| i.state == ItemState::Outfit)
    }

    /// The item in the hands: the active slot's.
    pub fn in_hands(&self) -> Option<u16> {
        self.slots[usize::from(self.active_slot?)]
    }

    /// Whether slot `slot` is persistent (`slot_persistent_<slot + 1>`).
    pub fn is_persistent(&self, slot: u8) -> bool {
        self.config.persistent.get(usize::from(slot)) == Some(&true)
    }

    /// Item `id` leaves its slot or the belt; if it was in the hands it is
    /// put away first: the active slot becomes none (screen 5.4).
    fn leave_place(&mut self, id: u16) {
        if self.in_hands() == Some(id) {
            self.active_slot = None;
        }
        for s in self.slots.iter_mut().filter(|s| **s == Some(id)) {
            *s = None;
        }
        self.belt.retain(|&b| b != id);
    }

    fn set_place(&mut self, id: u16, place: Place) {
        if let Some(i) = self.items.iter_mut().find(|i| i.id == id) {
            i.place = place;
        }
    }

    /// **To backpack** (screen 5.4): fails for an item not in the
    /// inventory or already in the backpack; otherwise it leaves its slot or
    /// the belt (put away if in the hands) and goes to the backpack. The
    /// carried weight is unchanged.
    pub fn move_to_backpack(&mut self, id: u16) -> bool {
        match self.item(id).map(|i| i.place) {
            None | Some(Place::Backpack) => false,
            Some(_) => {
                self.leave_place(id);
                self.set_place(id, Place::Backpack);
                true
            }
        }
    }

    /// Whether item `id` **can go on the belt** (screen 5.4): it is not on
    /// it, its section has `belt = true`, the belt holds fewer than
    /// `max_belt` items, and the belt's items with it, in the order of
    /// [`placement_order`], fit one row of `max_belt` cells by first-free
    /// placement.
    pub fn can_go_on_belt(&self, id: u16) -> bool {
        let Some(item) = self.item(id) else {
            return false;
        };
        if self.belt.contains(&id) || !item.config.belt || self.belt.len() >= self.config.max_belt {
            return false;
        }
        let mut items: Vec<&InventoryItem> =
            self.belt.iter().filter_map(|&b| self.item(b)).collect();
        items.push(item);
        items.sort_by(|a, b| placement_order(a, b));
        let sizes: Vec<(u32, u32)> = items.iter().map(|i| i.config.grid_size()).collect();
        fits_in_row(&sizes, self.config.max_belt)
    }

    /// **To belt** (screen 5.4): fails unless the item can go on the belt;
    /// it then leaves its slot (put away if in the hands) or the backpack
    /// and is appended to the belt.
    pub fn move_to_belt(&mut self, id: u16) -> bool {
        if !self.can_go_on_belt(id) {
            return false;
        }
        self.leave_place(id);
        self.belt.push(id);
        self.set_place(id, Place::Belt);
        true
    }

    /// Whether item `id` **can go to its slot** (screen 5.4): its section's
    /// `slot` names a slot and that slot is empty.
    pub fn can_go_to_slot(&self, id: u16) -> bool {
        self.item(id)
            .and_then(|i| i.config.slot_number())
            .is_some_and(|s| self.slots[usize::from(s)].is_none())
    }

    /// **To slot**, without swap (screen 5.4): fails unless the item can go
    /// to its slot. For the grenades' slot nothing further happens
    /// ([`SlotMove::Done`]). Otherwise the item goes into the slot (out of
    /// the backpack and the belt) and the slot is asked to become the
    /// active slot: when it can be active, the item in the hands is put away
    /// and this one taken. For slots 0, 4, 5 and 7–10 the original's outcome
    /// is unestablished; they are treated as slots 1, 2 and 6 (our choice).
    pub fn move_to_slot(&mut self, id: u16) -> SlotMove {
        if !self.can_go_to_slot(id) {
            return SlotMove::Failed;
        }
        let Some(slot) = self.item(id).and_then(|i| i.config.slot_number()) else {
            return SlotMove::Failed;
        };
        if slot == GRENADE_SLOT {
            return SlotMove::Done;
        }
        self.leave_place(id);
        self.slots[usize::from(slot)] = Some(id);
        self.set_place(id, Place::Slot(slot));
        if !INACTIVE_SLOTS.contains(&slot) {
            // Taken at once, nothing animates it (our choice, as in 3.3).
            self.active_slot = Some(slot);
        }
        SlotMove::Moved
    }

    /// **To slot with swap** (screen 5.4): as [`Inventory::move_to_slot`] when
    /// the item can go to its slot; otherwise, if its slot is 1, 2 or 6 and
    /// not persistent, the slot's item goes to the backpack first and the
    /// move is tried once more without swap.
    pub fn move_to_slot_with_swap(&mut self, id: u16) -> SlotMove {
        if self.can_go_to_slot(id) {
            return self.move_to_slot(id);
        }
        let Some(slot) = self.item(id).and_then(|i| i.config.slot_number()) else {
            return SlotMove::Failed;
        };
        if !matches!(slot, 1 | 2 | OUTFIT_SLOT) || self.is_persistent(slot) {
            return SlotMove::Failed;
        }
        match self.slots[usize::from(slot)] {
            Some(old) if old != id => {
                self.move_to_backpack(old);
                self.move_to_slot(id)
            }
            _ => SlotMove::Failed,
        }
    }
}

/// Whether items of grid sizes `sizes`, in order, fit one row of `cells`
/// cells by first-free placement (screen 5.4 rule 4). In a single row each
/// item lands right after the previous ones, so they fit exactly when none
/// is taller than one cell and their widths add up to at most `cells`.
pub fn fits_in_row(sizes: &[(u32, u32)], cells: usize) -> bool {
    let mut used = 0u64;
    for &(w, h) in sizes {
        if h > 1 {
            return false;
        }
        used += u64::from(w);
    }
    used <= cells as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::hits::{test_outfit_keys, test_outfit_sections};

    const ITEMS: &str = concat!(
        "\
        [detector_simple]\nclass = D_SIMDET\nslot = 8\ndefault_to_ruck = false\ninv_weight = 0\n\
        visual = equipments\\item_datchik1.ogf\n\
        [device_torch]\nclass = TORCH_S\nslot = 9\ninv_weight = 0\nvisual = equipments\\light_night\n\
        [novice_outfit]\nclass = E_STLK\nslot = 6\ninv_weight = 3.0\n\
        visual = equipments\\novice_suit.ogf\nadditional_inventory_weight = 0\n\
        additional_inventory_weight2 = 0\n",
        test_outfit_keys!(),
        test_outfit_sections!(),
        "[wpn_binoc]\nclass = WP_BINOC\nslot = 4\ninv_weight = 1.0\nammo_mag_size = 0\n\
        scope_status = 1\nsilencer_status = 0\ngrenade_launcher_status = 0\n\
        visual = weapons\\binoculars\\wpn_binoculars.ogf\n\
        [wpn_pm]\nclass = WP_PM\nvisual = v\nslot = 1\ninv_weight = 1.0\nammo_mag_size = 8\n\
        scope_status = 0\nsilencer_status = 2\ngrenade_launcher_status = 0\n\
        [device_pda]\nclass = D_PDA\nslot = 7\ninv_weight = 0.3\nvisual = equipments\\item_pda\n\
        [bolt]\nclass = II_BOLT\nvisual = v\nslot = 5\ninv_weight = 0\n\
        [bread]\nclass = II_FOOD\nvisual = v\nslot = 4\ninv_weight = 0.3\neat_health = 0.05\neat_power = 0\n\
        eat_satiety = 0.2\neat_radiation = 0\nwounds_heal_perc = 0\neat_portions_num = -1\n\
        [af_medusa]\nclass = ARTEFACT\nvisual = v\nbelt = true\ndefault_to_ruck = false\ninv_weight = 0.5\n\
        [medkit]\nclass = II_MEDKI\nvisual = v\ninv_weight = 0.1\n\
        [inventory]\nmax_weight = 50\nmax_belt = 5\n"
    );

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

    /// Sections with the screen's keys (inventory-screen.md 4.2, 5.4).
    const SCREEN_ITEMS: &str = concat!(
        "\
        [inventory]\nmax_weight = 50\nmax_belt = 5\nslot_persistent_1 = true\n\
        slot_persistent_2 = false\nslot_persistent_3 = false\nslot_persistent_4 = true\n\
        slot_persistent_5 = true\nslot_persistent_7 = false\n\
        [wpn_pm]\nclass = WP_PM\nvisual = v\nscope_status = 0\nsilencer_status = 0\ngrenade_launcher_status = 0\nslot = 1\ninv_weight = 1\ninv_grid_x = 0\ninv_grid_y = 0\n\
        inv_grid_width = 2\ninv_grid_height = 1\n\
        [wpn_ak74]\nclass = WP_AK74\nvisual = v\nscope_status = 0\nsilencer_status = 0\ngrenade_launcher_status = 0\nslot = 2\ninv_weight = 3\ninv_grid_x = 5\ninv_grid_y = 5\n\
        inv_grid_width = 5\ninv_grid_height = 2\n\
        [grenade_f1]\nclass = G_F1\nvisual = v\nslot = 3\ninv_weight = 0.6\ninv_grid_x = 0\ninv_grid_y = 1\n\
        inv_grid_width = 1\ninv_grid_height = 1\n\
        [novice_outfit]\nclass = E_STLK\nvisual = v\nadditional_inventory_weight = 0\nadditional_inventory_weight2 = 0\nslot = 6\ninv_weight = 3\ninv_grid_x = 0\n\
        inv_grid_y = 18\ninv_grid_width = 2\ninv_grid_height = 2\n",
        test_outfit_keys!(),
        test_outfit_sections!(),
        "[af_medusa]\nclass = ARTEFACT\nvisual = v\nbelt = true\ninv_weight = 0.5\ninv_grid_x = 1\n\
        inv_grid_y = 1\ninv_grid_width = 1\ninv_grid_height = 1\n\
        [af_wide]\nclass = ARTEFACT\nvisual = v\nbelt = true\ninv_weight = 0.5\ninv_grid_x = 1\n\
        inv_grid_y = 1\ninv_grid_width = 2\ninv_grid_height = 1\n\
        [af_tall]\nclass = ARTEFACT\nvisual = v\nbelt = true\ninv_weight = 0.5\ninv_grid_x = 1\n\
        inv_grid_y = 1\ninv_grid_width = 1\ninv_grid_height = 2\n\
        [quest]\nclass = II_DOC\nvisual = v\nquest_item = true\ninv_weight = 0.1\ninv_grid_x = 1\n\
        inv_grid_y = 1\ninv_grid_width = 1\ninv_grid_height = 1\n\
        [no_grid]\nclass = II_DOC\nvisual = v\ninv_weight = 0.1\ninv_grid_x = 1\n"
    );

    fn screen_inventory() -> (Config, Inventory) {
        let system = ltx::parse(SCREEN_ITEMS.as_bytes()).unwrap();
        let inv = Inventory::new(InventoryConfig::read(&system).unwrap());
        (system, inv)
    }

    /// ltx-config.md 11: a key read for the item's kind, missing, is fatal;
    /// a kind that does not read it, or an optional key, is not needed.
    #[test]
    fn a_missing_item_key_is_fatal() {
        let system = ltx::parse(
            b"[no_weight]\nclass = II_MEDKI\nvisual = v\n\
              [no_visual]\nclass = II_MEDKI\ninv_weight = 1\n\
              [gun]\nclass = WP_PM\nvisual = v\ninv_weight = 1\nscope_status = 0\nsilencer_status = 0\n\
              [suit]\nclass = E_STLK\nvisual = v\ninv_weight = 1\nadditional_inventory_weight = 0\n\
              [food]\nclass = II_FOOD\nvisual = v\ninv_weight = 1\neat_health = 0\neat_power = 0\n\
              eat_satiety = 0\neat_radiation = 0\nwounds_heal_perc = 0\neat_portions_num = 1\n",
        )
        .unwrap();
        for (section, key) in [
            ("no_weight", "inv_weight"),
            ("gun", "grenade_launcher_status"),
            ("suit", "additional_inventory_weight2"),
        ] {
            let err = ItemConfig::read(&system, section).unwrap_err();
            assert!(err.to_string().contains(key), "{section}: {err}");
        }
        // `visual` is set only if present (object-visual-and-transform.md 2).
        assert_eq!(ItemConfig::read(&system, "no_visual").unwrap().visual, "");
        let food = ItemConfig::read(&system, "food").unwrap();
        assert_eq!(
            (food.scope_status, food.additional_inventory_weight),
            (0, 0.0)
        );
    }

    #[test]
    fn reads_the_screen_keys() {
        let (system, inv) = screen_inventory();
        assert_eq!(
            inv.config.persistent,
            [
                true, false, false, true, true, false, false, false, false, false, false
            ]
        );
        let ak = ItemConfig::read(&system, "wpn_ak74").unwrap();
        assert_eq!(
            ak.inv_grid,
            Some(InvGrid {
                x: 5,
                y: 5,
                width: 5,
                height: 2
            })
        );
        assert!(ak.is_weapon() && !ak.quest_item);
        assert!(ItemConfig::read(&system, "quest").unwrap().quest_item);
        assert_eq!(ItemConfig::read(&system, "no_grid").unwrap().inv_grid, None);
        assert_eq!(
            ItemConfig::read(&system, "no_grid").unwrap().grid_size(),
            (0, 0)
        );
    }

    #[test]
    fn orders_by_size_then_id_then_section() {
        let (system, _) = screen_inventory();
        // inventory-screen.md 4.3's example, by our rule 4 for sections.
        let mut items = [
            item(&system, "af_medusa", 103),
            item(&system, "novice_outfit", 101),
            item(&system, "wpn_ak74", 100),
            item(&system, "novice_outfit", 102),
            item(&system, "af_wide", 104),
            item(&system, "wpn_pm", 105),
        ];
        items.sort_by(placement_order);
        let order: Vec<u16> = items.iter().map(|i| i.id).collect();
        // 5 × 2, then 2 × 2 (higher id first), then 2 × 1 by section name,
        // then 1 × 1.
        assert_eq!(order, [100, 102, 101, 104, 105, 103]);
    }

    #[test]
    fn a_row_fits_by_widths_and_single_heights() {
        assert!(fits_in_row(&[(1, 1); 5], 5));
        assert!(!fits_in_row(&[(1, 1); 6], 5));
        assert!(fits_in_row(&[(2, 1), (3, 1)], 5));
        assert!(!fits_in_row(&[(2, 1), (3, 1), (1, 1)], 5));
        assert!(!fits_in_row(&[(1, 2)], 5));
        assert!(fits_in_row(&[], 0));
    }

    #[test]
    fn moves_to_the_backpack_and_the_belt() {
        let (system, mut inv) = screen_inventory();
        inv.take(item(&system, "wpn_pm", 1));
        assert_eq!((inv.slots[1], inv.active_slot), (Some(1), Some(1)));
        assert_eq!(inv.in_hands(), Some(1));
        // The item in the hands is put away.
        assert!(inv.move_to_backpack(1));
        assert_eq!(inv.item(1).unwrap().place, Place::Backpack);
        assert_eq!(
            (inv.slots[1], inv.active_slot, inv.in_hands()),
            (None, None, None)
        );
        // Already in the backpack: fails; unknown: fails.
        assert!(!inv.move_to_backpack(1));
        assert!(!inv.move_to_backpack(99));
        // No `belt = true`: never on the belt.
        assert!(!inv.can_go_on_belt(1) && !inv.move_to_belt(1));
        // Belt items until the row is full.
        for id in 10..13 {
            inv.take_stored(item(&system, "af_medusa", id), 3);
            assert!(inv.move_to_belt(id));
        }
        inv.take_stored(item(&system, "af_wide", 13), 3);
        assert!(inv.move_to_belt(13), "3 + 2 cells fit 5");
        assert_eq!(inv.belt, [10, 11, 12, 13]);
        inv.take_stored(item(&system, "af_medusa", 14), 3);
        assert!(!inv.can_go_on_belt(14), "6 cells do not fit 5");
        // Fewer than max_belt items but no room; a tall item never fits.
        inv.move_to_backpack(13);
        assert_eq!(inv.item(13).unwrap().place, Place::Backpack);
        inv.take_stored(item(&system, "af_tall", 15), 3);
        assert!(!inv.can_go_on_belt(15));
        assert!(inv.move_to_belt(14));
        assert!(!inv.can_go_on_belt(14), "already on it");
        // Weight is unchanged by moves.
        assert!((inv.weight() - (1.0 + 0.5 * 6.0)).abs() < 1e-6);
    }

    #[test]
    fn moves_to_slots_and_swaps() {
        let (system, mut inv) = screen_inventory();
        inv.take_stored(item(&system, "wpn_ak74", 1), 3);
        inv.take_stored(item(&system, "wpn_ak74", 2), 3);
        inv.take_stored(item(&system, "wpn_pm", 3), 3);
        // To its slot: taken into the hands.
        assert_eq!(inv.move_to_slot(1), SlotMove::Moved);
        assert_eq!((inv.slots[2], inv.active_slot), (Some(1), Some(2)));
        // The slot is taken: no swap fails, with swap the old rifle goes to
        // the backpack and the new one into the hands.
        assert_eq!(inv.move_to_slot(2), SlotMove::Failed);
        assert_eq!(inv.move_to_slot_with_swap(2), SlotMove::Moved);
        assert_eq!(inv.slots[2], Some(2));
        assert_eq!(inv.item(1).unwrap().place, Place::Backpack);
        assert_eq!(inv.in_hands(), Some(2));
        // Another active slot: its item is put away, this one taken.
        assert_eq!(inv.move_to_slot(3), SlotMove::Moved);
        assert_eq!((inv.active_slot, inv.in_hands()), (Some(1), Some(3)));
        assert_eq!(inv.item(2).unwrap().place, Place::Slot(2));
        // The outfit slot cannot be active.
        inv.take_stored(item(&system, "novice_outfit", 4), 3);
        assert_eq!(inv.move_to_slot(4), SlotMove::Moved);
        assert_eq!((inv.slots[6], inv.active_slot), (Some(4), Some(1)));
        assert_eq!(inv.worn_outfit().map(|o| o.id), Some(4));
        // Grenades: their slot empty, the move counts as done, nothing moves.
        inv.take_stored(item(&system, "grenade_f1", 5), 3);
        assert_eq!(inv.move_to_slot(5), SlotMove::Done);
        assert_eq!(inv.item(5).unwrap().place, Place::Backpack);
        // A persistent slot is never swapped: slot 3 taken, a second
        // grenade fails with swap.
        inv.slots[3] = Some(5);
        inv.set_place(5, Place::Slot(3));
        inv.take_stored(item(&system, "grenade_f1", 6), 3);
        assert_eq!(inv.move_to_slot_with_swap(6), SlotMove::Failed);
        // No slot: fails.
        inv.take_stored(item(&system, "af_medusa", 7), 3);
        assert_eq!(inv.move_to_slot_with_swap(7), SlotMove::Failed);
        assert!(!SlotMove::Failed.succeeded() && SlotMove::Done.succeeded());
    }
}
