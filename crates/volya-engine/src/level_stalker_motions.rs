// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's stalkers' motions (docs/behavior/stalker-motions.md,
//! CH-11): each stalker's model's motion lists, built once per model and
//! shared by its instances (2.2), given to the stalker's motion state the
//! first time it plays; the ends the frame's tracks updates reached on its
//! model, kept for its next motion update (3.5); and the generator its
//! choices draw from.
//!
//! The motion state itself is the scripts' stalker's
//! (`volya_engine_script`'s `StalkerParts`), as its script animations are
//! (8.1); [`crate::scripts::stalker_motions_frame`] runs the update.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::core::motion_playback::{Motions, Notified};
use crate::core::sound::random::SharedRandom;
use crate::core::stalker_motions::StalkerModel;

/// The class id of the stalker class, the one with these motions.
pub const STALKER_CLASS: &str = "AI_STL_S";

#[derive(Debug, Default)]
pub struct LevelStalkerMotions {
    /// By model: its motions and lists.
    models: HashMap<usize, StalkerModel>,
    /// The online stalkers' instances, and those whose motion state has
    /// its model.
    online: HashSet<usize>,
    given: HashSet<usize>,
    /// By instance: the ends reached since its last motion update.
    notified: HashMap<usize, Vec<Notified>>,
    /// The global generator (conventions; our own instance: the
    /// original's sequence, seeded from the processor's counter and shared
    /// with other uses, can't be reproduced, sound/random.rs).
    pub random: SharedRandom,
}

impl LevelStalkerMotions {
    /// Instance `k` of class `class_id` came online (or was there at the
    /// start): a stalker's model is given afresh at its next update.
    pub fn spawned(&mut self, k: usize, class_id: &str) {
        if class_id == STALKER_CLASS {
            self.online.insert(k);
            self.given.remove(&k);
            self.notified.remove(&k);
        }
    }

    /// Instance `k` went offline or was removed.
    pub fn destroyed(&mut self, k: usize) {
        self.online.remove(&k);
        self.given.remove(&k);
        self.notified.remove(&k);
    }

    /// The frame's tracks updates' ends, by instance: the online
    /// stalkers' kept.
    pub fn tracks_updated(&mut self, notified: &[(usize, Notified)]) {
        for (k, n) in notified {
            if self.online.contains(k) {
                self.notified.entry(*k).or_default().push(*n);
            }
        }
    }

    /// Whether instance `k` is an online stalker.
    pub fn is_online(&self, k: usize) -> bool {
        self.online.contains(&k)
    }

    /// Instance `k`'s model `model` with `motions` (its lists built the
    /// first time), when its motion state doesn't have it yet.
    pub fn model_to_give(
        &mut self,
        k: usize,
        model: usize,
        motions: &Arc<Motions>,
    ) -> Option<StalkerModel> {
        if !self.given.insert(k) {
            return None;
        }
        let m =
            (self.models.entry(model)).or_insert_with(|| StalkerModel::new(Arc::clone(motions)));
        Some(m.clone())
    }

    /// The ends reached on instance `k`'s model since they were last
    /// taken.
    pub fn take_notified(&mut self, k: usize) -> Vec<Notified> {
        self.notified.remove(&k).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_online_stalkers_are_followed() {
        let mut m = LevelStalkerMotions::default();
        m.spawned(3, STALKER_CLASS);
        m.spawned(4, "AI_TRD_S");
        assert!(m.is_online(3) && !m.is_online(4));
        m.destroyed(3);
        assert!(!m.is_online(3));
        assert!(m.take_notified(3).is_empty());
    }
}
