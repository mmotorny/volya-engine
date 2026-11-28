// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's hint (docs/behavior/hud-pickup-hint.md 1, cited by
//! section): what the player's update chooses for the HUD's hint line
//! from the object of the crosshair record (use-action.md 2.2), right
//! after it fills the record (1.2), by the first rule of 1.3 that
//! applies.

use std::collections::HashSet;

use volya_engine_formats::ltx::{Config, value};
use volya_engine_formats::object::Kind;

/// The built-in keys the player keeps for its hint (1.1).
pub const CHARACTER_USE: &str = "character_use";
pub const DEAD_CHARACTER_USE: &str = "dead_character_use";
pub const DEAD_CHARACTER_USE_OR_DRAG: &str = "dead_character_use_or_drag";
pub const CAR_CHARACTER_USE: &str = "car_character_use";
pub const INVENTORY_ITEM_USE: &str = "inventory_item_use";
/// The hint text an inventory box sets as it comes online (1.4).
pub const INVENTORY_BOX_USE: &str = "inventory_box_use";

/// The player's hint when not none (1.3): rule 1's hint text, whose
/// lookup (`TF` 1.4) the hint is, or the key of rules 2 to 5, stored as
/// the key itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerHint {
    /// The object's usable part's hint text, before its lookup.
    Text(String),
    Key(&'static str),
}

impl PlayerHint {
    /// The hint as the line compares and looks it up (2.2, 2.3): rule
    /// 1's text through `lookup` (the string table's, `TF` 1.4), else the
    /// key.
    pub fn resolve<'a>(&'a self, lookup: impl FnOnce(&'a str) -> &'a str) -> &'a str {
        match self {
            Self::Text(t) => lookup(t),
            Self::Key(k) => k,
        }
    }

    /// The hint text or the key, unlooked (the engine interface's report).
    pub fn source(&self) -> &str {
        match self {
            Self::Text(t) => t,
            Self::Key(k) => k,
        }
    }
}

/// What the rules of 1.3 tell apart about an object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HintKind {
    /// A living kind (conventions), a character: its health and whether
    /// its model name is a key of `[ph_capture_visuals]` (rule 3).
    Living {
        health: f32,
        listed: bool,
    },
    /// A holder: a car, a mounted or a stationary gun (rule 4).
    Holder,
    /// An item, the inventory's target item (`UA` 2.2, 9.1), and whether
    /// it passes the take test (rule 5).
    Item {
        take: bool,
    },
    Other,
}

/// The player's hint for the crosshair record's object (1.3): none when
/// the record is cleared (`object` none), else by the first rule that
/// applies, `tip` the object's usable part's hint text (none: no text;
/// an empty text counts).
pub fn choose(object: Option<HintKind>, tip: Option<&str>) -> Option<PlayerHint> {
    let kind = object?;
    if let Some(t) = tip {
        return Some(PlayerHint::Text(t.to_owned()));
    }
    let key = match kind {
        HintKind::Living { health, .. } if health > 0.0 => CHARACTER_USE,
        HintKind::Living { listed: true, .. } => DEAD_CHARACTER_USE_OR_DRAG,
        HintKind::Living { .. } => DEAD_CHARACTER_USE,
        HintKind::Holder => CAR_CHARACTER_USE,
        HintKind::Item { take: true } => INVENTORY_ITEM_USE,
        HintKind::Item { take: false } | HintKind::Other => return None,
    };
    Some(PlayerHint::Key(key))
}

/// What the rules read from the configuration (1.3): the model names of
/// `[ph_capture_visuals]` and the item sections whose take flag is off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HintConfig {
    capture_visuals: HashSet<String>,
    no_take: HashSet<String>,
}

impl HintConfig {
    /// Read from `system` (`config\system.ltx` and its includes).
    pub fn read(system: &Config) -> Self {
        let capture_visuals = (system.section("ph_capture_visuals"))
            .map(|s| s.entries().map(|e| e.key().to_owned()).collect())
            .unwrap_or_default();
        // `UA` 9.1: the take flag is the section's `can_take`, true when
        // absent.
        let no_take = (system.sections())
            .filter(|s| matches!(s.get("can_take"), Some(Some(v)) if !value::bool(v)))
            .map(|s| s.name().to_owned())
            .collect();
        Self {
            capture_visuals,
            no_take,
        }
    }

    /// The kind of an object of kind `kind`, class id `class_id`, section
    /// `section` and model name `visual`, with health `health` (living
    /// kinds). The model name is matched as the record gives it (`UA` 7.1,
    /// believed). The take test's artefact state is unestablished (1.3):
    /// taken as clear.
    pub fn kind(
        &self,
        kind: Kind,
        class_id: &str,
        section: &str,
        visual: &str,
        health: f32,
    ) -> HintKind {
        use Kind::*;
        match kind {
            Player | Stalker | Trader | Monster => HintKind::Living {
                health,
                listed: self.capture_visuals.contains(visual),
            },
            Vehicle | StationaryGun => HintKind::Holder,
            _ if class_id == "W_MOUNTD" => HintKind::Holder,
            Item
            | Torch
            | Pda
            | Document
            | Outfit
            | Ammunition
            | Knife
            | MagazineWeapon
            | WeaponWithGrenadeLauncher
            | Shotgun => HintKind::Item {
                take: !self.no_take.contains(section),
            },
            _ => HintKind::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &'static str) -> Option<PlayerHint> {
        Some(PlayerHint::Key(k))
    }

    fn living(health: f32, listed: bool) -> Option<HintKind> {
        Some(HintKind::Living { health, listed })
    }

    #[test]
    fn a_cleared_record_has_no_hint() {
        assert_eq!(choose(None, Some("character_use")), None);
    }

    #[test]
    fn a_hint_text_comes_first_an_empty_one_too() {
        let text = |t: &str| Some(PlayerHint::Text(t.to_owned()));
        assert_eq!(
            choose(living(1.0, false), Some("character_use")),
            text("character_use")
        );
        // The trader with his talk disabled (1.5, 4.1).
        assert_eq!(choose(living(1.0, false), Some("")), text(""));
        assert_eq!(choose(living(0.0, true), Some("")), text(""));
        assert_eq!(
            choose(Some(HintKind::Other), Some("tip_door_locked")),
            text("tip_door_locked")
        );
    }

    #[test]
    fn the_built_in_keys_by_kind() {
        assert_eq!(choose(living(0.5, true), None), key(CHARACTER_USE));
        assert_eq!(
            choose(living(0.0, true), None),
            key(DEAD_CHARACTER_USE_OR_DRAG)
        );
        assert_eq!(choose(living(-1.0, false), None), key(DEAD_CHARACTER_USE));
        assert_eq!(choose(Some(HintKind::Holder), None), key(CAR_CHARACTER_USE));
        let item = |take| Some(HintKind::Item { take });
        assert_eq!(choose(item(true), None), key(INVENTORY_ITEM_USE));
        assert_eq!(choose(item(false), None), None);
        assert_eq!(choose(Some(HintKind::Other), None), None);
    }

    #[test]
    fn the_configuration_gives_the_take_flag_and_the_listed_models() {
        let system = volya_engine_formats::ltx::parse(
            b"[ph_capture_visuals]\nactors\\bandit\\stalker_bandit_1\n\
              [medkit]\nclass = II_MEDKI\n\
              [explosive_barrel]\ncan_take = false\n",
        )
        .unwrap();
        let c = HintConfig::read(&system);
        let k = |kind, class, section, visual| c.kind(kind, class, section, visual, 0.0);
        assert_eq!(
            k(
                Kind::Stalker,
                "AI_STL_S",
                "stalker",
                "actors\\bandit\\stalker_bandit_1"
            ),
            HintKind::Living {
                health: 0.0,
                listed: true
            }
        );
        assert_eq!(
            k(
                Kind::Trader,
                "AI_TRD_S",
                "m_trader",
                "actors\\trader\\trader"
            ),
            HintKind::Living {
                health: 0.0,
                listed: false
            }
        );
        assert_eq!(
            k(Kind::Item, "II_MEDKI", "medkit", "m"),
            HintKind::Item { take: true }
        );
        assert_eq!(
            k(Kind::Item, "II_EXPLO", "explosive_barrel", "m"),
            HintKind::Item { take: false }
        );
        assert_eq!(k(Kind::Vehicle, "C_NIVA", "car", "m"), HintKind::Holder);
        assert_eq!(
            k(Kind::StationaryGun, "W_STMGUN", "gun", "m"),
            HintKind::Holder
        );
        assert_eq!(
            k(Kind::PlainVisual, "W_MOUNTD", "gun", "m"),
            HintKind::Holder
        );
        assert_eq!(
            k(Kind::PlainVisual, "O_INVBOX", "box", "m"),
            HintKind::Other
        );
        assert_eq!(
            k(Kind::PhysicsObject, "O_PHYSIC", "door", "m"),
            HintKind::Other
        );
    }
}
