// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A character's talk state (section 4 of
//! docs/behavior/script-api-dialogs-and-trade.md, docs/behavior/talk.md 1):
//! its talk flag and partner (its side of a talk), its talk-enabled and
//! trade-enabled flags, the partner its trade keeps, and for stalkers and
//! traders the start dialog and default start dialog; and the offer of a
//! talk (`use` 6.2 steps 2 and 3, R 4.2). As the character's own state
//! (docs/behavior/character-talk-state.md, CH-7): made with each online
//! object, so enabled and not talking each time a character comes online
//! (2.1, 3), never saved (7), and the start dialogs taken at coming online
//! ([`Talk::come_online`], 6.1).

use crate::game_object::{CREATURE_CLASSES, GameObject, GameObjects, ObjectClass};

/// The classes with a start dialog (4.4).
pub const START_DIALOG_CLASSES: [&str; 2] = ["CAI_Stalker", "CAI_Trader"];

/// The trader class (section 6).
pub const TRADER: &str = "CAI_Trader";

/// A character's talk state.
#[derive(Clone, Debug, PartialEq)]
pub struct Talk {
    pub talking: bool,
    pub partner: Option<u16>,
    /// Set when the object is made (`talk` 1).
    pub talk_enabled: bool,
    /// Set when the object is made (`talk` 1).
    pub trade_enabled: bool,
    /// The partner the character's trade was readied with (`talk` 1).
    pub trade_partner: Option<u16>,
    /// The start dialog id (stalkers and traders only, 4.4).
    pub start_dialog: Vec<u8>,
    /// The default start dialog, which the engine sets (`talk` 6).
    pub default_start_dialog: Vec<u8>,
}

impl Default for Talk {
    fn default() -> Self {
        Talk {
            talking: false,
            partner: None,
            talk_enabled: true,
            trade_enabled: true,
            trade_partner: None,
            start_dialog: Vec::new(),
            default_start_dialog: Vec::new(),
        }
    }
}

impl Talk {
    /// Starts the character's side of a talk with `partner` (`talk` 1):
    /// the talk flag set, the partner kept and the trade readied with it.
    pub fn start(&mut self, partner: u16) {
        self.talking = true;
        self.partner = Some(partner);
        self.trade_partner = Some(partner);
    }

    /// A stalker or trader coming online (CH-7 6.1, `talk` 6) with its
    /// character information's start dialog `start_dialog` (CH-7 6.2):
    /// when its start dialog is empty (always, for a new online object)
    /// both its start dialog and its default start dialog become it, an id
    /// no dialog file defines included (CH-7 6.3: not looked up here).
    pub fn come_online(&mut self, start_dialog: &[u8]) {
        if self.start_dialog.is_empty() {
            self.start_dialog = start_dialog.to_vec();
            self.default_start_dialog = start_dialog.to_vec();
        }
    }

    /// Stops the character's side (`stop_talk`, 4.1): the talk flag
    /// cleared, the partner forgotten and the trade's partner cleared.
    pub fn stop(&mut self) {
        self.talking = false;
        self.partner = None;
        self.trade_partner = None;
    }
}

/// Whether `class` is a monster's (conventions: a creature class other
/// than the stalker's and the trader's).
pub fn is_monster(class: ObjectClass) -> bool {
    matches!(class, ObjectClass::Named(name)
        if CREATURE_CLASSES.contains(&name) && !START_DIALOG_CLASSES.contains(&name))
}

/// Whether objects of `class` have a start dialog (4.4).
pub fn has_start_dialog(class: ObjectClass) -> bool {
    matches!(class, ObjectClass::Named(name) if START_DIALOG_CLASSES.contains(&name))
}

/// A character's talk-enabled flag as `is_talk_enabled` reads it (4.1):
/// always false for a monster. `false` for an object that isn't a
/// character.
pub fn talk_enabled(object: &GameObject) -> bool {
    !is_monster(object.class) && object.owner.as_ref().is_some_and(|o| o.talk.talk_enabled)
}

fn alive(object: &GameObject) -> bool {
    object.entity.as_ref().is_some_and(|e| e.health > 0.0)
}

/// The offer of a talk from `player` to `partner` (`use` 6.2 steps 2 and
/// 3, R 4.2), whether either is talking untested: the partner accepts
/// when its talk is enabled and both have health above 0; then it starts
/// its side with the player and the player its side with the partner.
/// Whether it accepted; showing the talk window is the caller's. `None`
/// when either isn't a character.
pub fn offer_talk(objects: &mut GameObjects, player: u16, partner: u16) -> Option<bool> {
    let p = objects.get(player).filter(|o| o.owner.is_some())?;
    let q = objects.get(partner).filter(|o| o.owner.is_some())?;
    if !(talk_enabled(q) && alive(q) && alive(p)) {
        return Some(false);
    }
    let side = |objects: &mut GameObjects, id: u16, other: u16| {
        if let Some(owner) = objects.get_mut(id).and_then(|o| o.owner.as_mut()) {
            owner.talk.start(other);
        }
    };
    side(objects, partner, player);
    side(objects, player, partner);
    Some(true)
}
