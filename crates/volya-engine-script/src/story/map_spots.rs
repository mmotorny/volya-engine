// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's map spots (9.1): per pair (object id, spot type) a count,
//! a hint and the saved mark. What a spot shows, and the types' lifetimes,
//! are `hud-scope` 11's.

/// No object: an objective's target when it has none (3.3).
pub const NO_OBJECT: u16 = 65_535;

/// A spot (9.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spot {
    pub object: u16,
    pub spot_type: Vec<u8>,
    pub count: u32,
    pub hint: Vec<u8>,
    /// Marked saved: only these are in `sav` 9 part 7 (9.2).
    pub saved: bool,
}

/// The player's map spots, in the order made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapSpots {
    pub spots: Vec<Spot>,
    /// The spot marked as the active task's (5.1), by its pair.
    pub active: Option<(u16, Vec<u8>)>,
}

impl MapSpots {
    pub fn get(&self, object: u16, spot_type: &[u8]) -> Option<&Spot> {
        self.spots
            .iter()
            .find(|s| s.object == object && s.spot_type == spot_type)
    }

    fn get_mut(&mut self, object: u16, spot_type: &[u8]) -> Option<&mut Spot> {
        self.spots
            .iter_mut()
            .find(|s| s.object == object && s.spot_type == spot_type)
    }

    /// Adds a spot (9.1): one count more on a pair that has one, else a new
    /// spot of count 1; `true` for a new one (whose callback the caller
    /// raises). A non-empty `hint` becomes its hint; `saved` marks it.
    /// TODO(#1061): a spot type's lifetime, restarted on adding.
    pub fn add(&mut self, object: u16, spot_type: &[u8], hint: &[u8], saved: bool) -> bool {
        let new = match self.get_mut(object, spot_type) {
            Some(spot) => {
                spot.count += 1;
                false
            }
            None => {
                self.spots.push(Spot {
                    object,
                    spot_type: spot_type.to_vec(),
                    count: 1,
                    hint: Vec::new(),
                    saved: false,
                });
                true
            }
        };
        let spot = self.get_mut(object, spot_type).expect("just added");
        if !hint.is_empty() {
            spot.hint = hint.to_vec();
        }
        if saved {
            spot.saved = true;
        }
        new
    }

    /// Removes one count (9.1): the spot at count 1; nothing for a pair
    /// without one.
    pub fn remove(&mut self, object: u16, spot_type: &[u8]) {
        let Some(i) = self
            .spots
            .iter()
            .position(|s| s.object == object && s.spot_type == spot_type)
        else {
            return;
        };
        if self.spots[i].count <= 1 {
            self.spots.remove(i);
        } else {
            self.spots[i].count -= 1;
        }
    }

    /// The count, 0 when there is no spot (9.2).
    pub fn count(&self, object: u16, spot_type: &[u8]) -> u32 {
        self.get(object, spot_type).map_or(0, |s| s.count)
    }

    /// `map_change_spot_hint` (9.2): nothing when there is no spot.
    pub fn change_hint(&mut self, object: u16, spot_type: &[u8], hint: &[u8]) {
        if let Some(spot) = self.get_mut(object, spot_type) {
            spot.hint = hint.to_vec();
        }
    }
}
