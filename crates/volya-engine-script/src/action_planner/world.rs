// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! World properties, world states and the property store (sections 2 and
//! 3 of docs/behavior/script-api-action-planner.md).

use std::cell::RefCell;
use std::rc::Rc;

/// A world property (2.1): a condition (an id) and a value. Ordered by
/// condition, then value (false before true).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorldProperty {
    pub condition: u32,
    pub value: bool,
}

/// `r` of 2.1's hash.
fn r(x: u32) -> u32 {
    x.wrapping_mul(134_775_813).wrapping_add(1)
}

/// `s` of 2.1's hash: the upper 32 bits of the 64-bit product with
/// 4,294,967,295.
fn s(x: u32) -> u32 {
    ((u64::from(x) * 4_294_967_295) >> 32) as u32
}

impl WorldProperty {
    pub fn new(condition: u32, value: bool) -> Self {
        WorldProperty { condition, value }
    }

    /// The property's hash (2.1).
    pub fn hash(&self) -> u32 {
        let a = s(r(self.condition.wrapping_add(1)));
        a ^ s(r(u32::from(self.value).wrapping_add(a)))
    }
}

/// A world state (2.2): properties sorted by condition, then value, and
/// the XOR of their hashes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WorldState {
    properties: Vec<WorldProperty>,
    hash: u32,
}

impl WorldState {
    pub fn new() -> Self {
        WorldState::default()
    }

    /// A state of `properties` added in order.
    pub fn of(properties: &[WorldProperty]) -> Self {
        let mut s = WorldState::new();
        for p in properties {
            s.add(*p);
        }
        s
    }

    pub fn properties(&self) -> &[WorldProperty] {
        &self.properties
    }

    pub fn hash(&self) -> u32 {
        self.hash
    }

    /// `add_property` (2.2): inserted at its sorted place, without testing
    /// whether its condition is there already.
    pub fn add(&mut self, p: WorldProperty) {
        let at = self.properties.partition_point(|q| *q <= p);
        self.properties.insert(at, p);
        self.hash ^= p.hash();
    }

    /// The place of the first property not sorted below (c, false): the
    /// first whose condition is at least `c` (2.2's `remove_property` and
    /// `property`).
    pub fn at_least(&self, c: u32) -> Option<usize> {
        let at = self.properties.partition_point(|q| q.condition < c);
        (at < self.properties.len()).then_some(at)
    }

    /// Removes the property at `at` ([`WorldState::at_least`]).
    pub fn remove_at(&mut self, at: usize) {
        let p = self.properties.remove(at);
        self.hash ^= p.hash();
    }

    /// `includes` (2.2): every property of `t` is here, with its condition
    /// and value.
    pub fn includes(&self, t: &WorldState) -> bool {
        t.properties
            .iter()
            .all(|p| self.properties.binary_search(p).is_ok())
    }

    /// `clear` (2.2).
    pub fn clear(&mut self) {
        self.properties.clear();
        self.hash = 0;
    }

    /// `==` (2.2): the hashes alone.
    pub fn hash_equal(&self, t: &WorldState) -> bool {
        self.hash == t.hash
    }

    /// `<` (2.2): property by property; a proper beginning is the smaller.
    pub fn less(&self, t: &WorldState) -> bool {
        self.properties < t.properties
    }

    /// The same properties (the same hash and the same sorted list, 6.2's
    /// and 7.1's comparison).
    pub fn same(&self, t: &WorldState) -> bool {
        self.hash == t.hash && self.properties == t.properties
    }

    /// The state's own value of condition `c` (7.1), if it has one.
    pub fn value_of(&self, c: u32) -> Option<bool> {
        let at = self.properties.partition_point(|q| q.condition < c);
        (self.properties.get(at))
            .filter(|p| p.condition == c)
            .map(|p| p.value)
    }

    /// Sets condition `c` to `value`, replacing its properties.
    pub(crate) fn set(&mut self, c: u32, value: bool) {
        self.unset(c);
        self.add(WorldProperty::new(c, value));
    }

    /// Removes every property of condition `c`.
    pub(crate) fn unset(&mut self, c: u32) {
        while let Some(at) = self
            .at_least(c)
            .filter(|&i| self.properties[i].condition == c)
        {
            self.remove_at(at);
        }
    }
}

/// A property store (3): (id, value) pairs in the order added, shared by
/// the planner that owns it and the parts it hands it to.
#[derive(Clone, Debug, Default)]
pub struct PropertyStorage(pub Rc<RefCell<Vec<(u32, bool)>>>);

impl PropertyStorage {
    /// `set_property` (3): the id's value replaced, else appended.
    pub fn set(&self, id: u32, value: bool) {
        let mut pairs = self.0.borrow_mut();
        match pairs.iter_mut().find(|(i, _)| *i == id) {
            Some(pair) => pair.1 = value,
            None => pairs.push((id, value)),
        }
    }

    /// `property` (3): the id's value, `None` for an id not stored.
    pub fn get(&self, id: u32) -> Option<bool> {
        self.0.borrow().iter().find(|(i, _)| *i == id).map(|p| p.1)
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    /// The store as a planner saves it (11): a 32-bit count, then each
    /// pair's id (32 bits) and value (8 bits), little-endian as save
    /// packets are.
    pub fn save(&self, out: &mut Vec<u8>) {
        let pairs = self.0.borrow();
        out.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
        for (id, value) in pairs.iter() {
            out.extend_from_slice(&id.to_le_bytes());
            out.push(u8::from(*value));
        }
    }

    /// Reads what [`PropertyStorage::save`] wrote, setting each pair as
    /// `set_property` does (11); the bytes read, or `None` when `bytes`
    /// ends early.
    pub fn load(&self, bytes: &[u8]) -> Option<usize> {
        let count = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        let mut at = 4;
        for _ in 0..count {
            let id = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
            let value = *bytes.get(at + 4)? != 0;
            self.set(id, value);
            at += 5;
        }
        Some(at)
    }
}
