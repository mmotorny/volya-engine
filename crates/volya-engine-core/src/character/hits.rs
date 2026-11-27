// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Hits on the player character (docs/behavior/player-hits.md, cited by
//! section): the hit and its types (2), the immunities the difficulty
//! chooses (3), the bone factors of `[actor_damage]` (4.3), the worn
//! outfit's protection and bone protection table (4.4), the changes by
//! type (4.5) and the wounds (4.6, 5). [`crate::character::CharacterState::hit`]
//! applies a hit in the order of section 4.
//!
//! What makes hits is not here: a hit arrives with the values of section
//! 2.

use std::io;

use crate::config::{Config, Section, value};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The number of hit types (2).
pub const TYPE_COUNT: usize = 11;

/// The hit types' names, by type (2).
pub const NAMES: [&str; TYPE_COUNT] = [
    "burn",
    "shock",
    "strike",
    "wound",
    "radiation",
    "telepatic",
    "chemical_burn",
    "explosion",
    "fire_wound",
    "wound_2",
    "physic_strike",
];

/// A hit type `t`, 0 … 10 (2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HitType(u8);

impl HitType {
    pub const BURN: Self = Self(0);
    pub const SHOCK: Self = Self(1);
    pub const STRIKE: Self = Self(2);
    pub const WOUND: Self = Self(3);
    pub const RADIATION: Self = Self(4);
    pub const TELEPATIC: Self = Self(5);
    pub const CHEMICAL_BURN: Self = Self(6);
    pub const EXPLOSION: Self = Self(7);
    pub const FIRE_WOUND: Self = Self(8);
    pub const WOUND_2: Self = Self(9);
    pub const PHYSIC_STRIKE: Self = Self(10);

    /// Type `t`; one outside 0 … 10 is an error (2).
    pub fn new(t: u32) -> io::Result<Self> {
        if (t as usize) < TYPE_COUNT {
            Ok(Self(t as u8))
        } else {
            Err(invalid(format!("Unknown/unregistered hit type {t}")))
        }
    }

    /// The type named `name` (2).
    pub fn named(name: &str) -> io::Result<Self> {
        NAMES
            .iter()
            .position(|n| *n == name)
            .map(|t| Self(t as u8))
            .ok_or_else(|| invalid(format!("Unknown/unregistered hit type {name}")))
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    pub fn name(self) -> &'static str {
        NAMES[self.index()]
    }

    /// The type a hit has from 4.2 item 3 on: `wound_2` is `wound`.
    pub fn as_applied(self) -> Self {
        if self == Self::WOUND_2 {
            Self::WOUND
        } else {
            self
        }
    }
}

/// A hit (2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub kind: HitType,
    /// The power `P`.
    pub power: f32,
    /// The bone of the player's model, by its number; `None` for no bone.
    pub bone: Option<u16>,
    /// Armour piercing `ap`, used by `fire_wound` hits only (4.4).
    pub armour_piercing: f32,
}

/// A coefficient per hit type.
pub type PerType = [f32; TYPE_COUNT];

/// Reads one value per type with `key(t)`: `None` keys (`wound_2`) are
/// `missing`; a key of `optional` reads as `missing` when absent; every
/// other key is read without a default (2).
fn per_type(
    section: Section,
    key: impl Fn(usize) -> Option<String>,
    optional: usize,
    missing: f32,
) -> io::Result<PerType> {
    let mut out = [missing; TYPE_COUNT];
    for (t, v) in out.iter_mut().enumerate() {
        let Some(key) = key(t) else { continue };
        if t == optional {
            if let Some(value) = section.get(&key) {
                *v = value::float(value.unwrap_or(""));
            }
        } else {
            *v = value::float(section.read(&key)?);
        }
    }
    Ok(out)
}

/// The immunity coefficients of `section` (2, 3): `<name>_immunity` per
/// type, `physic_strike_wound_immunity` for `physic_strike` (absent: 1.0);
/// `wound_2` has none (1.0).
pub fn read_immunities(section: Section) -> io::Result<PerType> {
    per_type(
        section,
        |t| match t {
            9 => None,
            10 => Some("physic_strike_wound_immunity".to_owned()),
            _ => Some(format!("{}_immunity", NAMES[t])),
        },
        HitType::PHYSIC_STRIKE.index(),
        1.0,
    )
}

/// An outfit's protections `p_t` (2, 4.4): `<name>_protection` per type,
/// `physic_strike_protection` absent: 0; `p_9` is 1.0 and unused.
pub fn read_protections(section: Section) -> io::Result<PerType> {
    let mut p = per_type(
        section,
        |t| (t != 9).then(|| format!("{}_protection", NAMES[t])),
        HitType::PHYSIC_STRIKE.index(),
        0.0,
    )?;
    p[HitType::WOUND_2.index()] = 1.0;
    Ok(p)
}

/// Item `i` of a comma-separated value, read as a decimal number (0 when
/// absent: our choice, every shipped entry has the items read).
fn item(v: &str, i: usize) -> f32 {
    value::list(v)
        .nth(i)
        .map_or(0.0, |s| value::float(s.trim()))
}

fn section<'a>(system: &'a Config, name: &str) -> io::Result<Section<'a>> {
    system
        .section(name)
        .ok_or_else(|| invalid(format!("no section [{name}]")))
}

/// A bone's factors of the damage table (4.3): the hit scale `h` and the
/// wound scale `w`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneScales {
    pub hit: f32,
    pub wound: f32,
}

const UNIT_SCALES: BoneScales = BoneScales {
    hit: 1.0,
    wound: 1.0,
};

/// The player's damage table (4.3), as read and as resolved against the
/// player's model.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageTable {
    default: BoneScales,
    /// The entries other than `default`, by bone name.
    entries: Vec<(String, BoneScales)>,
    /// Each bone's factors, by the model's bone number.
    by_bone: Vec<BoneScales>,
}

impl Default for DamageTable {
    /// Without a `damage` key: 1.0 for every bone.
    fn default() -> Self {
        Self {
            default: UNIT_SCALES,
            entries: Vec::new(),
            by_bone: Vec::new(),
        }
    }
}

impl DamageTable {
    /// Section `[name]`: `<bone> = <h>, <animation>, <w>` per bone and the
    /// `default` entry (both 1.0 without one).
    pub fn read(section: Section) -> Self {
        let scales = |v: &str| BoneScales {
            hit: item(v, 0),
            wound: item(v, 2),
        };
        let mut table = Self::default();
        for e in section.entries() {
            let v = e.value().unwrap_or("");
            if e.key() == "default" {
                table.default = scales(v);
            } else {
                table.entries.push((e.key().to_lowercase(), scales(v)));
            }
        }
        table
    }

    /// Resolves the table against the model's `bones` (4.3): a bone name
    /// the model lacks is an error, as is a first bone with `h` or `w` 0.
    pub fn set_model(&mut self, bones: &[String]) -> io::Result<()> {
        let mut by_bone = vec![self.default; bones.len()];
        for (name, s) in &self.entries {
            let i = bones
                .iter()
                .position(|b| b == name)
                .ok_or_else(|| invalid(format!("damage table: the model has no bone {name}")))?;
            by_bone[i] = *s;
        }
        if let Some(first) = by_bone.first()
            && (first.hit == 0.0 || first.wound == 0.0)
        {
            return Err(invalid(format!(
                "damage table: the model's first bone {} has a factor 0",
                bones[0]
            )));
        }
        self.by_bone = by_bone;
        Ok(())
    }

    /// The factors of `bone` (`None`: the `default` values).
    pub fn scales(&self, bone: Option<u16>) -> BoneScales {
        bone.and_then(|b| self.by_bone.get(usize::from(b)).copied())
            .unwrap_or(self.default)
    }
}

/// A bone's entry of an outfit's bone protection table (4.4): its
/// protection factor `K` and armour `A`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Armour {
    pub factor: f32,
    pub armour: f32,
}

/// An outfit's bone protection table (4.4).
#[derive(Clone, Debug, PartialEq)]
pub struct BoneProtection {
    /// `hit_fraction` `F`.
    pub hit_fraction: f32,
    pub default: Armour,
    /// The other entries, by bone name.
    pub bones: Vec<(String, Armour)>,
}

/// `hit_fraction` when absent (4.4).
const HIT_FRACTION: f32 = 0.1;

impl BoneProtection {
    /// A table without keys: `hit_fraction` absent, the default entry.
    fn empty() -> Self {
        Self {
            hit_fraction: HIT_FRACTION,
            default: Armour {
                factor: 1.0,
                armour: 0.0,
            },
            bones: Vec::new(),
        }
    }

    /// Section `[name]` (4.4).
    pub fn read(section: Section) -> Self {
        let armour = |v: &str| Armour {
            factor: item(v, 0),
            armour: item(v, 1),
        };
        let mut table = Self::empty();
        for e in section.entries() {
            let v = e.value().unwrap_or("");
            match e.key() {
                "hit_fraction" => table.hit_fraction = value::float(v),
                "default" => table.default = armour(v),
                key => table.bones.push((key.to_lowercase(), armour(v))),
            }
        }
        table
    }

    /// The entry of bone `bone` of the wearer's model `bones` (the default
    /// entry for no bone or a bone without one). A bone the table names that
    /// the model lacks is an error (4.4).
    // TODO(#418): the spec reads the table, and checks its bones, when the
    // outfit is put on, after the model changes to its `actor_visual`; the
    // player's model does not change with the outfit here, and the check is
    // made at each hit.
    pub fn entry(&self, bones: &[String], bone: Option<u16>) -> io::Result<Armour> {
        if let Some((name, _)) = self.bones.iter().find(|(n, _)| !bones.contains(n)) {
            return Err(invalid(format!(
                "bone protection: the model has no bone {name}"
            )));
        }
        let name = bone.and_then(|b| bones.get(usize::from(b)));
        Ok(name
            .and_then(|n| self.bones.iter().find(|(b, _)| b == n))
            .map_or(self.default, |(_, a)| *a))
    }
}

/// What an outfit section gives a hit (4.4).
#[derive(Clone, Debug, PartialEq)]
pub struct OutfitProtection {
    /// `p_t`.
    pub protections: PerType,
    /// `m_t`, from the section `immunities_sect` names.
    pub immunities: PerType,
    pub bones: BoneProtection,
}

impl OutfitProtection {
    /// Outfit section `s` of `system`: its protections, `immunities_sect`
    /// and `bones_koeff_protection` (4.4).
    pub fn read(system: &Config, s: Section) -> io::Result<Self> {
        let immunities = section(system, value::unquote(s.read("immunities_sect")?.trim()))?;
        let bones = match s.get("bones_koeff_protection").flatten() {
            Some(name) => BoneProtection::read(section(system, value::unquote(name.trim()))?),
            // TODO(#419): an outfit without `bones_koeff_protection` has its
            // `hit_fraction` unset (unestablished); every shipped outfit has
            // the key. Ours takes the absent key's 0.1 and the default entry.
            None => BoneProtection::empty(),
        };
        Ok(Self {
            protections: read_protections(s)?,
            immunities: read_immunities(immunities)?,
            bones,
        })
    }

    /// `P2` from `P1` for applied type `t` on a bone with `armour`, the
    /// outfit at condition `c`, with armour piercing `ap` (4.4).
    pub fn power(&self, t: HitType, p1: f32, armour: Armour, c: f32, ap: f32) -> f32 {
        if t == HitType::FIRE_WOUND {
            (p1 - (1.0 - ap) * c * armour.armour).max(self.bones.hit_fraction * p1)
        } else {
            p1 * (1.0 - armour.factor * self.protections[t.index()] * c)
        }
    }

    /// The outfit's condition after a hit of applied type `t` and power
    /// `P1` (4.4).
    pub fn condition_after(&self, t: HitType, p1: f32, c: f32) -> f32 {
        (c - self.immunities[t.index()] * p1).clamp(0.0, 1.0)
    }
}

/// The hit keys of `[actor]` and its condition section (1, 3, 4.3).
#[derive(Clone, Debug, PartialEq)]
pub struct HitConfig {
    /// `I_t` (3).
    pub immunities: PerType,
    /// `actor_immunities_<d>` of each difficulty, read with the
    /// character; an error is kept to be met when that difficulty is set.
    difficulties: Vec<(String, Result<PerType, String>)>,
    pub damage: DamageTable,
    /// `health_hit_part` `H` and `power_hit_part` `Q` (4.5).
    pub health_hit_part: f32,
    pub power_hit_part: f32,
}

impl Default for HitConfig {
    /// Immunities 1.0 at every difficulty, no damage table, `H` 1.0, `Q` 0
    /// (tests that make a state from parts).
    fn default() -> Self {
        Self {
            immunities: [1.0; TYPE_COUNT],
            difficulties: (crate::console::DIFFICULTIES.iter())
                .map(|(d, _)| ((*d).to_owned(), Ok([1.0; TYPE_COUNT])))
                .collect(),
            damage: DamageTable::default(),
            health_hit_part: 1.0,
            power_hit_part: 0.0,
        }
    }
}

impl HitConfig {
    /// Player section `actor` of `system` with its condition section
    /// `condition` (3 item 1): immunities from `immunities_sect`, then, when
    /// [`HitConfig::set_difficulty`] is called, the difficulty's; the damage
    /// table `damage` names, unresolved until the model is set.
    pub fn read(system: &Config, actor: &str, condition: &str) -> io::Result<Self> {
        let a = section(system, actor)?;
        let c = section(system, condition)?;
        let immunities = read_immunities(section(system, a.read("immunities_sect")?.trim())?)?;
        let difficulties = crate::console::DIFFICULTIES
            .iter()
            .map(|(d, _)| {
                let name = format!("actor_immunities_{d}");
                let read = section(system, &name)
                    .and_then(read_immunities)
                    .map_err(|e| e.to_string());
                ((*d).to_owned(), read)
            })
            .collect();
        let damage = match a.get("damage").flatten() {
            Some(name) => DamageTable::read(section(system, name.trim())?),
            None => DamageTable::default(),
        };
        Ok(Self {
            immunities,
            difficulties,
            damage,
            health_hit_part: value::float(c.read("health_hit_part")?),
            power_hit_part: value::float(c.read("power_hit_part")?),
        })
    }

    /// The immunities of difficulty `name` (3 items 1–3): an error when its
    /// section was missing or incomplete.
    pub fn set_difficulty(&mut self, name: &str) -> io::Result<()> {
        let (_, read) = (self.difficulties.iter())
            .find(|(d, _)| d == name)
            .ok_or_else(|| invalid(format!("no difficulty {name}")))?;
        self.immunities = read.clone().map_err(invalid)?;
        Ok(())
    }
}

/// The greatest wound amount (4.6, 5.1).
pub const MAX_AMOUNT: f32 = 10.0;
/// A wound smaller than this heals to nothing at once (5.3).
const TINY_WOUND: f32 = 1e-7;

/// A wound (5.1): its bone (`None`: "no bone") and an amount per type.
#[derive(Clone, Debug, PartialEq)]
pub struct Wound {
    pub bone: Option<u16>,
    pub amounts: PerType,
}

impl Wound {
    /// The sum of its amounts (5.1).
    pub fn size(&self) -> f32 {
        self.amounts.iter().sum()
    }

    /// Heals by `a` (5.3): a wound of size below 1e−7 becomes 0; otherwise
    /// each amount less `a`, 0 below `min_wound_size`.
    pub fn heal(&mut self, a: f32, min_wound_size: f32) {
        if self.size().abs() < TINY_WOUND {
            self.amounts = [0.0; TYPE_COUNT];
            return;
        }
        for v in &mut self.amounts {
            *v -= a;
            if *v < min_wound_size {
                *v = 0.0;
            }
        }
    }
}

/// The character's wounds, in list order (5.1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wounds(pub Vec<Wound>);

impl Wounds {
    /// The total bleeding (5.2): the mean size, 0 without wounds.
    pub fn total_bleeding(&self) -> f32 {
        if self.0.is_empty() {
            return 0.0;
        }
        self.0.iter().map(Wound::size).sum::<f32>() / self.0.len() as f32
    }

    /// Bone `bone`'s wound grows by `z` of type `t` with draw `r` (0 …
    /// 32767) (4.6): a new wound is added at the end when the bone has none.
    pub fn grow(&mut self, bone: Option<u16>, t: HitType, z: f32, r: u16) {
        let k = match self.0.iter().position(|w| w.bone == bone) {
            Some(k) => k,
            None => {
                self.0.push(Wound {
                    bone,
                    amounts: [0.0; TYPE_COUNT],
                });
                self.0.len() - 1
            }
        };
        let v = &mut self.0[k].amounts[t.index()];
        *v = (*v + (0.5 + f32::from(r) / 32767.0) * z).clamp(0.0, MAX_AMOUNT);
    }

    /// Every wound heals by `a` (5.3, 5.4).
    pub fn heal(&mut self, a: f32, min_wound_size: f32) {
        for w in &mut self.0 {
            w.heal(a, min_wound_size);
        }
    }

    /// Wounds of size exactly 0 are removed (5.3).
    pub fn remove_healed(&mut self) {
        self.0.retain(|w| w.size() != 0.0);
    }
}

/// An outfit section's hit keys with the novice outfit's values
/// (player-hits.md 8), for test configurations; its sections are
/// [`test_outfit_sections`].
#[cfg(test)]
macro_rules! test_outfit_keys {
    () => {
        "burn_protection = 0.1\nshock_protection = 0.1\nstrike_protection = 0.1\n\
         wound_protection = 0.1\nradiation_protection = 0.0\ntelepatic_protection = 0.0\n\
         chemical_burn_protection = 0.1\nexplosion_protection = 0.1\n\
         fire_wound_protection = 0.1\nimmunities_sect = sect_novice_outfit_immunities\n\
         bones_koeff_protection = kurtka_mask_damage\n"
    };
}

/// The sections [`test_outfit_keys`] names (player-hits.md 8): two of the
/// bone table's bones, one of the body's and one of the head's.
#[cfg(test)]
macro_rules! test_outfit_sections {
    () => {
        "[sect_novice_outfit_immunities]\nburn_immunity = 0.04\nstrike_immunity = 0.01\n\
         shock_immunity = 0.04\nwound_immunity = 0.025\nradiation_immunity = 0.00\n\
         telepatic_immunity = 0.00\nchemical_burn_immunity = 0.04\n\
         explosion_immunity = 0.04\nfire_wound_immunity = 0.015\n\
         [kurtka_mask_damage]\nhit_fraction = 0.1\nbip01_spine = 1.0, 0.15\n\
         bip01_head = 1.0, 0.05\n"
    };
}

#[cfg(test)]
pub(crate) use {test_outfit_keys, test_outfit_sections};

#[cfg(test)]
mod tests;
