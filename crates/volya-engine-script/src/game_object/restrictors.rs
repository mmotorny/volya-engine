// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's registered restrictors and the characters' restrictions
//! over them (docs/behavior/character-paths.md, cited by section): the
//! restrictors registered as they come online (2.1), the default out- and
//! in-lists (2.2), a character's combined texts and the restrictions they
//! put in effect ([`Restrictions`]), its border kept until the
//! registered restrictors change (3.6).

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use super::{GameObject, GameObjects};
use volya_engine_core::simulation::level_graph::{Accessibility, LevelGraph};
use volya_engine_core::simulation::restrictions::{
    self, DEFAULT_IN_RESTRICTOR, DEFAULT_OUT_RESTRICTOR, Restrictions, RestrictorShape,
};

/// A combined out- and in-text.
type Texts = (Vec<u8>, Vec<u8>);

/// A registered restrictor (2.1).
#[derive(Clone, Debug)]
pub struct Registered {
    pub name: Vec<u8>,
    pub restrictor_type: u8,
    pub shape: Rc<RestrictorShape>,
}

/// The restrictors registered on the level, by object id, and the
/// characters' borders computed over them.
#[derive(Clone, Debug, Default)]
pub struct Restrictors {
    registered: BTreeMap<u16, Registered>,
    /// The order of registration (the default lists' order before they
    /// are normalised).
    order: Vec<u16>,
    /// Borders by combined out- and in-text (3.6), emptied whenever a
    /// restrictor registers or unregisters.
    borders: HashMap<Texts, Rc<Vec<u32>>>,
}

impl Restrictors {
    /// Restrictor `id` named `name` of `restrictor_type` comes online with
    /// `shape` (2.1): registered when its type is 0, 1 or 2; `false` when
    /// it is not.
    pub fn register(
        &mut self,
        id: u16,
        name: Vec<u8>,
        restrictor_type: u8,
        shape: RestrictorShape,
    ) -> bool {
        if !restrictions::registered(restrictor_type) {
            return false;
        }
        self.unregister(id);
        self.registered.insert(
            id,
            Registered {
                name,
                restrictor_type,
                shape: Rc::new(shape),
            },
        );
        self.order.push(id);
        self.borders.clear();
        true
    }

    /// Restrictor `id` unregisters (2.1).
    pub fn unregister(&mut self, id: u16) {
        if self.registered.remove(&id).is_some() {
            self.order.retain(|&o| o != id);
            self.borders.clear();
        }
    }

    /// Every restrictor unregisters (the level ends).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn len(&self) -> usize {
        self.registered.len()
    }

    pub fn is_empty(&self) -> bool {
        self.registered.is_empty()
    }

    /// The default list of `restrictor_type`'s restrictors, normalised
    /// (2.1, 2.2).
    fn default_list(&self, restrictor_type: u8) -> Vec<u8> {
        let names: Vec<&[u8]> = (self.order.iter())
            .filter_map(|id| self.registered.get(id))
            .filter(|r| r.restrictor_type == restrictor_type)
            .map(|r| r.name.as_slice())
            .collect();
        restrictions::normalised(&names.join(&b","[..]))
    }

    /// The default out-list (2.2).
    pub fn default_out(&self) -> Vec<u8> {
        self.default_list(DEFAULT_OUT_RESTRICTOR)
    }

    /// The default in-list (2.2).
    pub fn default_in(&self) -> Vec<u8> {
        self.default_list(DEFAULT_IN_RESTRICTOR)
    }

    /// The shape of the registered restrictor `name` (names are looked up
    /// as given; of two with one name, the first registered: our choice,
    /// names are unique within a level, 2.1).
    pub fn shape(&self, name: &[u8]) -> Option<&RestrictorShape> {
        (self.order.iter())
            .filter_map(|id| self.registered.get(id))
            .find(|r| r.name == name)
            .map(|r| &*r.shape)
    }

    /// A character's combined out- and in-texts (2.2) from its own.
    pub fn combined(&self, own_out: &[u8], own_in: &[u8]) -> (Vec<u8>, Vec<u8>) {
        restrictions::combined(own_out, own_in, &self.default_out(), &self.default_in())
    }

    /// The restrictions combined texts `out` and `inside` put in effect.
    pub fn restrictions<'a>(&'a self, out: &[u8], inside: &[u8]) -> Restrictions<'a> {
        Restrictions::of(out, inside, |n| self.shape(n))
    }

    /// The border of the character whose combined texts are `out` and
    /// `inside` (3.6), computed once while the registered restrictors stay.
    pub fn border(&mut self, graph: &LevelGraph, out: &[u8], inside: &[u8]) -> Rc<Vec<u32>> {
        let key = (out.to_vec(), inside.to_vec());
        if let Some(b) = self.borders.get(&key) {
            return Rc::clone(b);
        }
        let border = Rc::new(self.restrictions(out, inside).border(graph));
        self.borders.insert(key, Rc::clone(&border));
        border
    }
}

/// A character's own texts as the scripts keep them (2.2, 2.3).
pub fn own_texts(o: &GameObject) -> Option<(&[u8], &[u8])> {
    let f = o.movement.fighter.as_ref()?;
    Some((&f.own_out_restrictions, &f.own_in_restrictions))
}

impl GameObjects {
    /// Character `id`'s combined out- and in-texts (2.2); `None` for an
    /// object that isn't a fighter.
    pub fn combined_restrictions(&self, id: u16) -> Option<(Vec<u8>, Vec<u8>)> {
        let (out, inside) = own_texts(self.get(id)?)?;
        Some(self.restrictors.combined(out, inside))
    }

    /// Runs `f` with the level graph (every vertex enabled), character
    /// `id`'s restrictions in effect and its border; `None` without a
    /// level graph or for an object that isn't a fighter.
    pub fn with_restrictions<T>(
        &mut self,
        id: u16,
        f: impl FnOnce(&LevelGraph, &Restrictions, &[u32]) -> T,
    ) -> Option<T> {
        let (out, inside) = self.combined_restrictions(id)?;
        let vertices = self.level_graph.clone()?;
        let accessibility = Accessibility::default();
        let graph = LevelGraph {
            ai: &vertices.ai,
            links: &vertices.links,
            accessibility: &accessibility,
        };
        let border = self.restrictors.border(&graph, &out, &inside);
        let restrictions = self.restrictors.restrictions(&out, &inside);
        Some(f(&graph, &restrictions, &border))
    }
}
