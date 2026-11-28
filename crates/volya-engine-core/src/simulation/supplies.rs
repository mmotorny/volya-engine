// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A New Game's supplies (docs/behavior/new-game-supplies.md, cited by
//! section): the items each record object creates right after its
//! registration, by kind (section 1), drawn from the object's own
//! generator (section 2), each taking the next id (3.2). The player's are
//! its character state's (player-state.md 3.1), not made here.

use std::collections::HashMap;
use std::io;

use volya_engine_formats::object::{Kind, ObjectRecord};

use crate::character::inventory::{
    InventoryItem, ItemConfig, Owner, PDA_SECTION, Rand15, Supplies, SupplyEntry, read_supplies,
};
use crate::character_identity::{Identities, InUse, trader_like};
use crate::config::Config;
use volya_engine_formats::game_graph::TerrainMask;

/// The seed of object `id`'s own generator (section 2). The game's is a
/// performance-counter reading at the object's construction, not
/// reproducible; ours is fixed by the id (our choice), mixed so that
/// nearby ids' first draws are far apart (successive seeds would give
/// nearly equal first draws).
pub fn seed(id: u16) -> u32 {
    u32::from(id).wrapping_mul(0x9E37_79B9)
}

/// Makes the supply items' records, each item section's keys read once.
pub(super) struct Supplier<'a> {
    system: &'a Config,
    configs: HashMap<String, ItemConfig>,
}

impl<'a> Supplier<'a> {
    pub(super) fn new(system: &'a Config) -> Self {
        Self {
            system,
            configs: HashMap::new(),
        }
    }

    /// The supplies step (section 1) of record object `owner`, not the
    /// player: the records of the items it creates, in order, each with the
    /// id `take` gives it as it is created (3.2). A stalker's or trader's
    /// identity is chosen with `identities` (character-identity.md 4.1;
    /// `masks` its terrain masks, `in_use` the registry); without them its
    /// PDA takes the record's specific character and no description's
    /// supplies are read.
    pub(super) fn supply(
        &mut self,
        owner: &mut ObjectRecord,
        masks: Option<&mut Vec<TerrainMask>>,
        identities: Option<&mut Identities>,
        in_use: &mut InUse,
        take: &mut dyn FnMut() -> io::Result<u16>,
    ) -> io::Result<Vec<ObjectRecord>> {
        let kind = owner.kind();
        // A group creates its members instead (section 1).
        if matches!(kind, Kind::MonsterGroup | Kind::OnOffGroup) {
            return Ok(Vec::new());
        }
        let block = owner.object.object_block();
        let custom = if block.custom_data.is_empty() {
            Supplies::default()
        } else {
            Supplies::parse(&block.custom_data)?
        };
        let holder = Owner {
            id: owner.header.id,
            position: owner.header.position,
            game_graph_vertex: block.game_graph_vertex,
            level_graph_node: block.level_graph_node,
        };
        let mut rng = Rand15(seed(holder.id));
        let mut made = Vec::new();
        let character = matches!(kind, Kind::Stalker | Kind::Trader);
        if !character || kind == Kind::Trader {
            self.read(&custom.entries, &mut rng, &holder, "", take, &mut made)?;
        }
        if !character {
            return Ok(made);
        }
        // character-identity.md 4.1 steps 1 – 4: the choice, the PDA, the
        // choice again, the PDA's specific character the result. The
        // choices take no id and draw from the shared generator, not the
        // object's (section 2), so making them before the PDA changes
        // nothing. The PDA is made without a draw.
        let mut description = None;
        let specific = match identities {
            Some(identities) => {
                let chosen = identities.supplies(owner, masks, in_use)?;
                if let Some(id) = &chosen
                    && !custom.dont_spawn_character_supplies
                {
                    description = Some(identities.descriptions.supplies(id)?);
                }
                chosen.unwrap_or_default()
            }
            None => {
                trader_like(owner).map_or_else(String::new, |t| t.specific_character.to_string())
            }
        };
        let pda = SupplyEntry::one(PDA_SECTION);
        made.push(self.make(&pda, &holder, &specific, take()?)?);
        // Step 5: the description's supplies.
        if let Some(supplies) = description {
            self.read(
                &supplies.entries,
                &mut rng,
                &holder,
                &specific,
                take,
                &mut made,
            )?;
        }
        if kind == Kind::Stalker {
            self.read(
                &custom.entries,
                &mut rng,
                &holder,
                &specific,
                take,
                &mut made,
            )?;
        }
        Ok(made)
    }

    /// Reads `entries` (1.1) for `holder`, appending the created items'
    /// records to `made`.
    fn read(
        &mut self,
        entries: &[SupplyEntry],
        rng: &mut Rand15,
        holder: &Owner,
        specific: &str,
        take: &mut dyn FnMut() -> io::Result<u16>,
        made: &mut Vec<ObjectRecord>,
    ) -> io::Result<()> {
        read_supplies(entries, rng, |e| {
            let id = take()?;
            made.push(self.make(e, holder, specific, id)?);
            Ok(())
        })
    }

    /// The record of item `e` created for `holder` with id `id`.
    fn make(
        &mut self,
        e: &SupplyEntry,
        holder: &Owner,
        specific: &str,
        id: u16,
    ) -> io::Result<ObjectRecord> {
        let config = match self.configs.get(&e.section) {
            Some(c) => c.clone(),
            None => {
                let c = ItemConfig::read(self.system, &e.section)?;
                self.configs.insert(e.section.clone(), c.clone());
                c
            }
        };
        InventoryItem::create(config, id, Some(e), holder.id, specific).record(holder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::inventory::creates;

    /// Section 2's table: of the 32,768 draws, how many create an item of
    /// each shipped probability.
    #[test]
    fn the_draws_creating_an_item() {
        for (p, n) in [
            (1.0, 32_767),
            (0.7, 22_937),
            (0.5, 16_384),
            (0.4, 13_107),
            (0.3, 9_831),
            (0.2, 6_554),
        ] {
            let created = (0..=0x7FFFu16).filter(|&r| creates(p, r)).count();
            assert_eq!(created, n, "probability {p}");
        }
        // Probability 1 draws too: the draw 32,767 creates nothing.
        assert!(!creates(1.0, 32_767));
        let mut g = Rand15(0);
        let first = Rand15(0).rand15();
        assert_eq!(g.chance(1.0), first != 32_767);
        assert_ne!(g, Rand15(0), "probability 1 draws");
    }

    #[test]
    fn nearby_ids_draw_far_apart() {
        let a = Rand15(seed(100)).rand15();
        let b = Rand15(seed(101)).rand15();
        assert!(a.abs_diff(b) > 100, "{a} {b}");
    }
}
