// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player character's state (docs/behavior/player-state.md, cited by
//! section): what a New Game gives it from the player's `all.spawn` record
//! and the configuration (section 2), its inventory (section 3,
//! [`inventory`]), its conditions over game time and their limits on
//! movement (section 4, [`condition`]), eating (section 5), and where a
//! save holds each value (section 6): the player's record, the items'
//! records and the client-data blocks, which [`CharacterState`] fills for
//! a save writer.
//!
//! Combat, wounds, death, artefacts, sleeping, trading, dialogs and scripts
//! are out of scope (section 9). The game here is always single-player.

pub mod condition;
pub mod inventory;

use std::io;

use volya_engine_formats::object::{Object, ObjectRecord, TraderLike};
use volya_engine_formats::packet::Writer;

use crate::config::Config;
use crate::movement::Limits;
use condition::{Activity, ConditionConfig, Conditions, Load};
use inventory::{
    BOLT_SECTION, IdPool, Inventory, InventoryConfig, InventoryItem, ItemConfig, Owner,
    PDA_SECTION, Rand15, Supplies, SupplyEntry,
};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The player's object id at a New Game (2.1).
pub const PLAYER_ID: u16 = 0;
/// A rank or reputation that is not set (`0x80000001`, 2.2).
pub const NOT_SET: i32 = -2_147_483_647;
/// A community that is not set (2.2).
pub const NO_COMMUNITY: i32 = -1;

/// A specific character's description (2.2): from
/// `character_desc_general.xml` and the other files of `[profiles]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpecificCharacter {
    pub id: String,
    /// The community's name.
    pub community: String,
    pub rank: i32,
    pub reputation: i32,
    /// The name's string-table key.
    pub name: String,
    pub visual: String,
    /// The `<supplies>` text (3.1 item 3).
    pub supplies: String,
}

/// `[game_relations] communities`: its names with the number after each
/// (2.2).
pub fn communities(value: &str) -> Vec<(String, i32)> {
    let items: Vec<&str> = value.split(',').map(str::trim).collect();
    items
        .chunks(2)
        .filter_map(|pair| match pair {
            [name, number] => Some(((*name).to_owned(), crate::config::value::int(number) as i32)),
            _ => None,
        })
        .collect()
}

/// Money, profile, community, rank, reputation and name: the player
/// record's Trader-like block (2.2, 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub money: u32,
    pub specific_character: String,
    pub trader_flags: u32,
    pub character_profile: String,
    pub community: i32,
    pub rank: i32,
    pub reputation: i32,
    pub character_name: String,
}

impl Profile {
    pub fn from_record(t: &TraderLike) -> Self {
        Self {
            money: t.money,
            specific_character: t.specific_character.clone(),
            trader_flags: t.trader_flags,
            character_profile: t.character_profile.clone(),
            community: t.community,
            rank: t.rank,
            reputation: t.reputation,
            character_name: t.character_name.clone(),
        }
    }

    /// Fills the unset values from specific character `c` (2.2): its id, the
    /// community's number among `communities` (the number after the name:
    /// our choice, whether it is that or the list position is
    /// unestablished; an unlisted community stays unset), its rank and
    /// reputation, and its name translated as `name`. A name already set is
    /// kept (our reading).
    pub fn resolve(&mut self, c: &SpecificCharacter, communities: &[(String, i32)], name: &str) {
        if self.specific_character.is_empty() {
            self.specific_character = c.id.clone();
        }
        if self.community == NO_COMMUNITY
            && let Some((_, n)) = communities.iter().find(|(n, _)| *n == c.community)
        {
            self.community = *n;
        }
        if self.rank == NOT_SET {
            self.rank = c.rank;
        }
        if self.reputation == NOT_SET {
            self.reputation = c.reputation;
        }
        if self.character_name.is_empty() {
            self.character_name = name.to_owned();
        }
    }

    pub fn apply(&self, t: &mut TraderLike) {
        t.money = self.money;
        t.specific_character = self.specific_character.clone();
        t.trader_flags = self.trader_flags;
        t.character_profile = self.character_profile.clone();
        t.community = self.community;
        t.rank = self.rank;
        t.reputation = self.reputation;
        t.character_name = self.character_name.clone();
    }
}

/// What a New Game reads for the player (sections 2, 3).
pub struct NewGame<'a> {
    /// The player's `all.spawn` record.
    pub record: &'a ObjectRecord,
    /// The system configuration.
    pub system: &'a Config,
    /// The specific character the profile resolves to (2.2 item 1).
    pub character: &'a SpecificCharacter,
    /// `[game_relations] communities` ([`communities`]).
    pub communities: &'a [(String, i32)],
    /// The character's name translated (2.2 item 5).
    pub character_name: &'a str,
    /// Seeds the supply's generator (the game seeds it from a timer, 3.1).
    pub seed: u32,
    /// The first id the items take from the simulation's id pool
    /// (offline-simulation.md 8.1: the ids after the player's
    /// registration), each next item the next id.
    pub first_id: u16,
}

/// The player character's state.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterState {
    /// The player's object, as its items' records take it (3.1).
    pub owner: Owner,
    pub profile: Profile,
    pub conditions: Conditions,
    pub condition_config: ConditionConfig,
    pub inventory: Inventory,
    /// The game time of the latest update, game ms; `None` until the first
    /// update after coming online, whose `dt` is 0 (4.1).
    last_update: Option<u64>,
}

impl CharacterState {
    /// A state from its parts, not yet updated (the first update's `dt` is
    /// 0, 4.1).
    pub fn new(
        owner: Owner,
        profile: Profile,
        conditions: Conditions,
        condition_config: ConditionConfig,
        inventory: Inventory,
    ) -> Self {
        Self {
            owner,
            profile,
            conditions,
            condition_config,
            inventory,
            last_update: None,
        }
    }

    /// The state after a New Game (sections 2, 3): the record's health and
    /// profile resolved, the conditions of 2.3, the items supplied by the
    /// simulation (3.1) taken by the online character in their order. The
    /// items take consecutive ids from `first_id`. The bolt (3.2) comes
    /// later, when the player comes online: [`CharacterState::receive_bolt`].
    pub fn new_game(n: &NewGame) -> io::Result<Self> {
        let Object::Player(player) = &n.record.object else {
            return Err(invalid(format!(
                "{}: not the player's record",
                n.record.header.name
            )));
        };
        let section = &n.record.header.section;
        let condition_sect = n
            .system
            .get(section, "condition_sect")
            .flatten()
            .ok_or_else(|| invalid(format!("[{section}] has no condition_sect")))?;
        let condition_config = ConditionConfig::read(n.system, condition_sect.trim())?;
        let mut profile = Profile::from_record(&player.trader);
        profile.resolve(n.character, n.communities, n.character_name);
        let block = &player.creature.object;
        let owner = Owner {
            id: PLAYER_ID,
            position: n.record.header.position,
            game_graph_vertex: block.game_graph_vertex,
            level_graph_node: block.level_graph_node,
        };

        // 3.1: what the simulation creates, in order.
        let mut ids = IdPool::starting_at(n.first_id);
        let mut rng = Rand15(n.seed);
        let mut children = Vec::new();
        let mut supply = |entries: &[SupplyEntry], children: &mut Vec<InventoryItem>| {
            for e in entries {
                for _ in 0..e.count {
                    if rng.chance(e.probability) {
                        children.push(InventoryItem::create(
                            ItemConfig::read(n.system, &e.section)?,
                            ids.take()?,
                            Some(e),
                            owner.id,
                            &profile.specific_character,
                        ));
                    }
                }
            }
            io::Result::Ok(())
        };
        let custom = Supplies::parse(&block.custom_data)?;
        supply(&custom.entries, &mut children)?;
        let pda = SupplyEntry {
            section: PDA_SECTION.to_owned(),
            count: 1,
            probability: 1.0,
            condition: None,
            scope: false,
            silencer: false,
            launcher: false,
        };
        supply(std::slice::from_ref(&pda), &mut children)?;
        if !custom.dont_spawn_character_supplies {
            supply(
                &Supplies::parse(&n.character.supplies)?.entries,
                &mut children,
            )?;
        }
        // The online character takes them in their order (3.3: believed).
        let mut inventory = Inventory::new(InventoryConfig::read(n.system)?);
        for item in children {
            inventory.take(item);
        }
        Ok(Self {
            owner,
            profile,
            conditions: Conditions::new_game(player.creature.health),
            condition_config,
            inventory,
            last_update: None,
        })
    }

    /// The bolt the online player receives (3.2), after its supplies, with
    /// id `id` from the simulation's pool (offline-simulation.md: requested
    /// once every New Game record exists).
    pub fn receive_bolt(&mut self, system: &Config, id: u16) -> io::Result<()> {
        self.inventory.take(InventoryItem::create(
            ItemConfig::read(system, BOLT_SECTION)?,
            id,
            None,
            self.owner.id,
            &self.profile.specific_character,
        ));
        Ok(())
    }

    /// The carried weight and the worn outfit's limits (3.4, 4.2).
    pub fn load(&self) -> Load {
        let inv = &self.inventory;
        let outfit = inv.worn_outfit();
        let extra = |f: fn(&ItemConfig) -> f32| outfit.map_or(0.0, |o| f(&o.config));
        let power_loss = outfit
            .and_then(|o| {
                o.config
                    .power_loss
                    .filter(|&l| !(l < 1.0 && o.condition <= 0.0))
            })
            .unwrap_or(1.0);
        Load {
            weight: inv.weight(),
            max_weight: inv.config.max_weight,
            carry_limit: inv.config.max_weight + extra(|c| c.additional_inventory_weight2),
            walk_limit: self.condition_config.max_walk_weight
                + extra(|c| c.additional_inventory_weight),
            power_loss,
        }
    }

    /// The thresholds re-evaluated for the movement (4.3), and what they
    /// allow it. Evaluated once per frame before the movement update (our
    /// choice: the game evaluates each when the movement asks). The third
    /// unable-to-move condition, a conversation, never holds here (no
    /// dialogs).
    pub fn movement_limits(&mut self) -> Limits {
        self.conditions.evaluate_thresholds(&self.condition_config);
        let load = self.load();
        Limits {
            limping: self.conditions.limping,
            cant_sprint: self.conditions.cant_sprint,
            unable_to_move: self.conditions.cant_walk || load.weight > load.walk_limit,
        }
    }

    /// A jump started (4.4).
    pub fn jump(&mut self) {
        let load = self.load();
        self.conditions.jump(&self.condition_config, &load);
    }

    /// The condition update at game time `now` (game ms) after a movement
    /// update that left `activity` (4.1, 4.2): `dt` is the game time since
    /// the previous update, 0 for the first. Nothing changes once health is
    /// 0 or less (dead; death is out of scope).
    pub fn update(&mut self, now: u64, activity: Activity) {
        let dt = self
            .last_update
            .map_or(0.0, |last| now.saturating_sub(last) as f32 * 0.001);
        self.last_update = Some(now);
        if !self.conditions.alive() {
            return;
        }
        let load = self.load();
        self.conditions
            .update(&self.condition_config, dt, activity, &load);
    }

    /// Uses eatable item `id` (section 5): its effects, then one portion
    /// less, the item removed when none remain.
    pub fn eat(&mut self, id: u16) -> io::Result<()> {
        let item = self
            .inventory
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| invalid(format!("no item {id} in the inventory")))?;
        let eatable = item
            .config
            .eatable
            .ok_or_else(|| invalid(format!("{} is not eatable", item.config.section)))?;
        self.conditions.eat(&eatable);
        let left = match item.portions.unwrap_or(0) {
            p if p >= 1 => p - 1,
            _ => 0,
        };
        item.portions = Some(left);
        if left == 0 {
            self.inventory.remove(id);
        }
        Ok(())
    }

    /// **Drop** of item `id` (inventory-screen.md section 5.7): nothing for
    /// an item of a quest (`quest_item = true`) or one not in the inventory;
    /// otherwise it leaves its place (put away if in the hands) and the
    /// inventory, lowering the carried weight, and is returned. Where it
    /// goes in the world is the caller's.
    pub fn drop_item(&mut self, id: u16) -> Option<InventoryItem> {
        if self.inventory.item(id)?.config.quest_item {
            return None;
        }
        self.inventory.remove(id)
    }

    /// Writes the state into the player's record (section 6): its id (2.1),
    /// health in the Creature block and update, the Trader-like block, and
    /// the update's radiation (believed to carry the condition's).
    pub fn apply_to_record(&self, record: &mut ObjectRecord) -> io::Result<()> {
        let Object::Player(p) = &mut record.object else {
            return Err(invalid(format!(
                "{}: not the player's record",
                record.header.name
            )));
        };
        record.header.id = self.owner.id;
        p.creature.health = self.conditions.health;
        p.update.creature.health = self.conditions.health;
        p.update.radiation = self.conditions.radiation;
        self.profile.apply(&mut p.trader);
        Ok(())
    }

    /// How many items it holds (after a New Game, before the bolt: how many
    /// ids its supplies took).
    pub fn item_count(&self) -> usize {
        self.inventory.items.len()
    }

    /// The records of the items a save writes after the player's, in the
    /// order it lists them (3.1; the bolt is never saved).
    pub fn item_records(&self) -> io::Result<Vec<ObjectRecord>> {
        self.inventory
            .items
            .iter()
            .filter(|i| i.can_be_saved())
            .map(|i| i.record(&self.owner))
            .collect()
    }

    /// The client data's condition block (section 6): alive, then power,
    /// radiation, morale and psy health, and no wounds.
    pub fn condition_block(&self, w: &mut Writer) {
        let c = &self.conditions;
        w.bool8(c.alive());
        if c.alive() {
            for v in [c.power, c.radiation, c.morale, c.psy_health] {
                w.float(v);
            }
            w.u8(0);
        }
    }

    /// The player condition block (section 6): the condition block, alcohol,
    /// the flags (0: tutorial hints and over-weight state are out of scope)
    /// and satiety.
    pub fn player_condition_block(&self, w: &mut Writer) {
        self.condition_block(w);
        w.float(self.conditions.alcohol);
        w.u16(0);
        w.float(self.conditions.satiety);
    }

    /// The active slot as the inventory-owner block stores it (section 6:
    /// believed its `u8`, 0xFF none).
    pub fn active_slot_byte(&self) -> u8 {
        self.inventory.active_slot.unwrap_or(0xFF)
    }
}

impl InventoryItem {
    /// Whether the item can still be used (inventory-screen.md section 5.7,
    /// believed: it has portions left): an eatable whose portions are not 0.
    /// A count of −1, which every shipped food has, is not 0 (section 5
    /// item 8 uses it up at once).
    pub fn usable(&self) -> bool {
        self.config.is_eatable() && self.portions != Some(0)
    }

    /// The client data's item block (section 6, believed): the place, the
    /// condition, and no physics state.
    pub fn item_block(&self, w: &mut Writer) {
        w.u8(self.place.code());
        w.float(self.condition);
        w.u8(0);
    }
}

#[cfg(test)]
pub(crate) mod tests;
