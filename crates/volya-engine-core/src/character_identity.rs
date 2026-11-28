// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Character identity and profile choice
//! (docs/behavior/character-identity.md, cited by section): the **choice**
//! of a stalker's or trader's specific character from its profile (section
//! 2), what **setting** one fills in (section 3), the **in-use registry**
//! of the specific characters taken (2.1), and the steps of a New Game that
//! make the choices (4.1, 4.2: the **spawn-file choice** and the **supplies
//! choice**, [`Identities::spawn_file_choice`], [`Identities::supplies`]).
//!
//! The descriptions come from CH-1 (docs/formats/character-descriptions.md)
//! through [`Descriptions`]; what the spec calls a **description** and its
//! **description index** are CH-1's specific character and registration
//! index. The game here is always single-player (2 step 1). The player's
//! identity (section 7) is player-state.md's, not made here.

use std::collections::BTreeMap;
use std::io;

use volya_engine_formats::character_descriptions::{NOT_SET, Profile, SpecificCharacter};
use volya_engine_formats::failed_check::failed;
use volya_engine_formats::game_graph::TerrainMask;
use volya_engine_formats::object::{Object, ObjectRecord, Text, TraderLike};

use crate::config::{Config, value};
use crate::particles::random::Shared;
use crate::simulation::graph::{ANY_TERRAIN, mask_key};
use crate::text::StringTable;

/// A community that is not set (1.1).
pub const NO_COMMUNITY: i32 = -1;
/// The text a generated name contains (3 step 6).
pub const GENERATE_NAME: &str = "GENERATE_NAME_";
/// A profile's rank or reputation and a candidate's may differ by less
/// than this (2 step 4.4).
const LIMIT: i64 = 10;
/// The value of every entry of the in-use registry (2.1).
const IN_USE: i32 = 1;

/// The in-use registry (2.1): the specific characters already taken, part 6
/// of a save's chunk 9 ("named counters", sav.md 9). Kept in name order
/// (the saved order is not defined).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InUse(BTreeMap<String, i32>);

impl InUse {
    /// A registry holding `entries`, as loading chunk 9 fills it (2.1,
    /// 4.3: emptied first).
    pub fn from_saved(entries: impl IntoIterator<Item = (String, i32)>) -> Self {
        Self(entries.into_iter().collect())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.0.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The entries as a save writes them.
    pub fn entries(&self) -> impl Iterator<Item = (&str, i32)> {
        self.0.iter().map(|(k, &v)| (k.as_str(), v))
    }

    /// An object's specific character goes from `previous` to `id` (2.1):
    /// `previous` removed if non-empty and present, `id` added with value 1
    /// if absent.
    fn change(&mut self, previous: &str, id: &str) {
        if !previous.is_empty() {
            self.0.remove(previous);
        }
        self.0.entry(id.to_owned()).or_insert(IN_USE);
    }
}

/// The profiles and descriptions CH-1 reads (1.2), loaded on demand.
pub trait Descriptions {
    /// The profile `id`; an id no profile has is CH-1 3.2's fatal error.
    fn profile(&mut self, id: &str) -> io::Result<&Profile>;
    /// The number of descriptions (indices 0 … count − 1).
    fn count(&self) -> usize;
    /// The description of index `index`.
    fn at(&mut self, index: usize) -> io::Result<&SpecificCharacter>;
    /// The description `id`; an id no description has is CH-1 3.2's fatal
    /// error.
    fn find(&mut self, id: &str) -> io::Result<&SpecificCharacter>;
}

/// The visual name a description's `visual` gives (3 step 2): its
/// extension removed, lower-cased.
pub fn model_name(visual: &str) -> String {
    let name = visual.to_lowercase();
    match name.rfind('.') {
        Some(dot) if !name[dot..].contains('\\') => name[..dot].to_owned(),
        _ => name,
    }
}

/// The Trader-like block of a stalker or trader (`sav` 7.4.2's kinds);
/// `None` for every other kind.
pub fn trader_like(record: &ObjectRecord) -> Option<&TraderLike> {
    match &record.object {
        Object::Stalker(o) => Some(&o.trader),
        Object::Trader(o) => Some(&o.trader),
        _ => None,
    }
}

/// [`trader_like`], to change.
pub fn trader_like_mut(record: &mut ObjectRecord) -> Option<&mut TraderLike> {
    match &mut record.object {
        Object::Stalker(o) => Some(&mut o.trader),
        Object::Trader(o) => Some(&mut o.trader),
        _ => None,
    }
}

/// What a spawn-file choice leaves in a record object's packets (4.2 step
/// 3): its Trader-like block, and a trader's model name (a stalker's
/// Monster-like block, read after, gives back the record's model and
/// team).
#[derive(Clone, Debug, PartialEq)]
pub struct RecordChoice {
    pub trader: TraderLike,
    pub model: Option<Text>,
}

impl RecordChoice {
    /// Gives the created object `record` these values (4.2 step 3).
    pub fn apply(&self, record: &mut ObjectRecord) {
        match &mut record.object {
            Object::Stalker(o) => o.trader = self.trader.clone(),
            Object::Trader(o) => {
                o.trader = self.trader.clone();
                if let Some(m) = &self.model {
                    o.visual.model = m.clone();
                }
            }
            _ => {}
        }
    }
}

/// What the choice and the set read besides the descriptions.
pub struct Identities<'a> {
    pub descriptions: &'a mut dyn Descriptions,
    /// The configuration: `[stalker_names_<suffix>]` (3 step 6) and the
    /// terrain sections (3 step 4).
    pub system: &'a Config,
    /// Text of an id (text-and-fonts.md 1.4).
    pub strings: &'a StringTable,
    /// `[game_relations] communities` (1.3): names with their numbers.
    pub communities: &'a [(String, i32)],
    /// The shared generator (original-determinism.md 2.2).
    pub random: &'a Shared,
}

impl Identities<'_> {
    fn draw(&self) -> u32 {
        self.random.next_k()
    }

    /// The choice (section 2) on stalker or trader `record` (any other kind
    /// is left alone): its specific character, made first if unset. A set
    /// appends to `masks` (the object's terrain masks, 3 step 4) and
    /// changes `in_use` (the simulation's registry, 2.1) when given.
    pub fn choose(
        &mut self,
        record: &mut ObjectRecord,
        masks: Option<&mut Vec<TerrainMask>>,
        in_use: Option<&mut InUse>,
    ) -> io::Result<Option<String>> {
        let Some(trader) = trader_like(record) else {
            return Ok(None);
        };
        // Step 2.
        if !trader.specific_character.is_empty() {
            return Ok(Some(trader.specific_character.to_string()));
        }
        // Step 3.
        let profile = self
            .descriptions
            .profile(&trader.character_profile)?
            .clone();
        let chosen = match &profile.specific_character {
            Some(id) => self.descriptions.find(id)?.clone(),
            None => {
                let index = self.random_choice(&profile, in_use.as_deref())?;
                self.descriptions.at(index)?.clone()
            }
        };
        self.set(record, &chosen, masks, in_use)?;
        Ok(Some(chosen.id))
    }

    /// Step 4: the index of the description a random choice for `profile`
    /// takes, with one draw.
    fn random_choice(&mut self, profile: &Profile, in_use: Option<&InUse>) -> io::Result<usize> {
        let class = profile.class.as_deref().unwrap_or_default();
        let (mut candidates, mut defaults) = (Vec::new(), Vec::new());
        for i in 0..self.descriptions.count() {
            let d = self.descriptions.at(i)?;
            if d.no_random || (!class.is_empty() && !d.classes.iter().any(|c| c == class)) {
                continue;
            }
            if d.team_default {
                defaults.push(i);
            }
            let far = |ours: i32, theirs: i32| {
                ours != NOT_SET && (i64::from(theirs) - i64::from(ours)).abs() >= LIMIT
            };
            if far(profile.rank, d.rank) || far(profile.reputation, d.reputation) {
                continue;
            }
            if in_use.is_some_and(|u| u.contains(&d.id)) {
                continue;
            }
            candidates.push(i);
        }
        if defaults.is_empty() {
            return Err(failed(
                "no default specific character set for class",
                Some(class),
            ));
        }
        let a = self.draw() as usize;
        Ok(if candidates.is_empty() {
            defaults[a % defaults.len()]
        } else {
            candidates[a % candidates.len()]
        })
    }

    /// Setting description `d` as `record`'s specific character (section
    /// 3), in its order; 0 to 3 draws (first name, last name, money). An
    /// empty id is an assertion failure before any step (section 3; the
    /// spec states no text, so the description is ours); no choice passes
    /// one.
    pub fn set(
        &self,
        record: &mut ObjectRecord,
        d: &SpecificCharacter,
        masks: Option<&mut Vec<TerrainMask>>,
        in_use: Option<&mut InUse>,
    ) -> io::Result<()> {
        if d.id.is_empty() {
            return Err(failed("empty specific character id", None));
        }
        let stalker = matches!(record.object, Object::Stalker(_));
        // Step 1.
        {
            let Some(trader) = trader_like_mut(record) else {
                return Ok(());
            };
            if let Some(u) = in_use {
                u.change(&trader.specific_character, &d.id);
            }
            trader.specific_character = d.id.clone().into();
        }
        // Step 2. The text is not checked: one that names no model file
        // (`ds_zomb_12`, section 9) fails the model lookup when the
        // character comes online.
        if !d.visual.is_empty() {
            let model: Text = model_name(&d.visual).into();
            match &mut record.object {
                Object::Stalker(o) => o.monster.creature.visual.model = model,
                Object::Trader(o) => o.visual.model = model,
                _ => {}
            }
        }
        // Step 3.
        let trader = trader_like_mut(record).expect("a stalker or trader");
        let mut team = None;
        if trader.community == NO_COMMUNITY {
            // CH-1 5.5 checked the community is listed.
            let position = self
                .communities
                .iter()
                .position(|(n, _)| *n == d.community)
                .ok_or_else(|| failed("item not found, id", Some(&d.community)))?;
            trader.community = position as i32;
            team = Some(self.communities[position].1 as u8);
        }
        // Step 5.
        if trader.rank == NOT_SET {
            trader.rank = d.rank;
        }
        if trader.reputation == NOT_SET {
            trader.reputation = d.reputation;
        }
        // Step 6.
        trader.character_name = self.name(d)?.into();
        // Step 7.
        let money = d.money;
        if money.min != 0 && money.max != 0 {
            trader.money = money.min;
            if money.max != money.min {
                trader.money += self.draw() % (money.max - money.min);
            }
        }
        if let (Object::Stalker(o), Some(team)) = (&mut record.object, team) {
            o.monster.creature.team = team;
        }
        // Step 4.
        if stalker && !d.terrain_sect.is_empty() {
            let section = self.system.section(&d.terrain_sect).ok_or_else(|| {
                // Believed: as for any section read (3 step 4).
                failed("section not found", Some(&d.terrain_sect))
            })?;
            if let Some(masks) = masks {
                masks.extend(section.entries().filter_map(|e| mask_key(e.key())));
                if masks.is_empty() {
                    masks.push(ANY_TERRAIN);
                }
            }
        }
        Ok(())
    }

    /// The character name a set gives (3 step 6): the text of `d`'s `name`,
    /// or a generated name with two draws.
    fn name(&self, d: &SpecificCharacter) -> io::Result<String> {
        let text = self.strings.get(&d.name);
        if !text.contains(GENERATE_NAME) {
            return Ok(text.to_owned());
        }
        let suffix: String = text.chars().skip(GENERATE_NAME.chars().count()).collect();
        let section = format!("stalker_names_{suffix}");
        // A missing section or key is fatal, as for any read (believed);
        // a count of 0 too (our choice: the spec leaves dividing by zero
        // open, and no shipped section has one, 9).
        let count = |key: &str| -> io::Result<u32> {
            let n = value::int(self.system.read(&section, key)?);
            u32::try_from(n)
                .ok()
                .filter(|&n| n > 0)
                .ok_or_else(|| failed(format!("[{section}] {key} is not positive"), None))
        };
        let (n, m) = (count("name_cnt")?, count("last_name_cnt")?);
        let first = self.draw() % n;
        let last = self.draw() % m;
        Ok(format!(
            "{} {}",
            self.strings.get(&format!("name_{suffix}_{first}")),
            self.strings.get(&format!("lname_{suffix}_{last}"))
        ))
    }

    /// The spawn-file choice (4.2 step 2) of character record `record`
    /// while the spawn file is read, with the simulation's `in_use`: what
    /// its record object then passes on to the created object.
    pub fn spawn_file_choice(
        &mut self,
        record: &mut ObjectRecord,
        in_use: &mut InUse,
    ) -> io::Result<Option<RecordChoice>> {
        if trader_like(record).is_none() {
            return Ok(None);
        }
        self.choose(record, None, Some(in_use))?;
        Ok(match &record.object {
            Object::Stalker(o) => Some(RecordChoice {
                trader: o.trader.clone(),
                model: None,
            }),
            Object::Trader(o) => Some(RecordChoice {
                trader: o.trader.clone(),
                model: Some(o.visual.model.clone()),
            }),
            _ => None,
        })
    }

    /// The supplies step's identity (4.1) for a created stalker or trader
    /// `record` with terrain masks `masks` (a stalker's): a stalker's
    /// choice, then the specific character emptied, the community unset
    /// and the choice made again. Gives the result, the PDA's specific
    /// character.
    // TODO(#1034): the PDA (section `device_pda`, the object its original
    // owner) and the description's `supplies` (CH-6) are not created.
    pub fn supplies(
        &mut self,
        record: &mut ObjectRecord,
        mut masks: Option<&mut Vec<TerrainMask>>,
        in_use: &mut InUse,
    ) -> io::Result<Option<String>> {
        if matches!(record.object, Object::Stalker(_)) {
            self.choose(record, masks.as_deref_mut(), Some(&mut *in_use))?;
        }
        let Some(trader) = trader_like_mut(record) else {
            return Ok(None);
        };
        trader.specific_character = Text::default();
        trader.community = NO_COMMUNITY;
        self.choose(record, masks, Some(in_use))
    }
}

#[cfg(test)]
mod tests;
