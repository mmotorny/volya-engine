// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Planning (section 7 of docs/behavior/script-api-action-planner.md): the
//! best-first search from the current world to the goal over search
//! states, which hold only the conditions whose values differ from the
//! world, with its open states in the array heap of 7.4.

use std::collections::HashMap;

use super::world::{WorldProperty, WorldState};

/// The most states one search reaches (7.3, the start included).
const MOST_REACHED: usize = 8_000;

/// The f at which the search stops (7.3): the largest.
const STOP_F: u16 = 65_535;

/// An action as the search sees it: its id, preconditions and effects.
pub struct Step<'a> {
    pub id: u32,
    pub preconditions: &'a WorldState,
    pub effects: &'a WorldState,
}

/// What the search asks the planner: a condition's world value (7.2,
/// computed when first needed and cached by the planner) and a step's
/// cost (the action's `weight` with the new state and the state being
/// expanded, 7.3).
pub trait Planning {
    fn world_value(&mut self, condition: u32) -> Result<bool, String>;
    fn weight(&mut self, step: usize, new: &WorldState, from: &WorldState) -> Result<u16, String>;
}

struct Node {
    state: WorldState,
    g: u16,
    f: u16,
    predecessor: Option<usize>,
    action: u32,
    open: bool,
    /// Its place in the heap while open.
    place: usize,
}

/// The open states (7.4): node indices in an array heap on f.
struct Open {
    heap: Vec<usize>,
}

impl Open {
    fn f(nodes: &[Node], i: usize) -> u16 {
        nodes[i].f
    }

    fn put(&mut self, nodes: &mut [Node], place: usize, node: usize) {
        self.heap[place] = node;
        nodes[node].place = place;
    }

    /// Moves the element at `place` up while its parent's f is above its
    /// own.
    fn up(&mut self, nodes: &mut [Node], mut place: usize) {
        let node = self.heap[place];
        while place > 0 {
            let parent = (place - 1) / 2;
            if Self::f(nodes, self.heap[parent]) <= Self::f(nodes, node) {
                break;
            }
            let p = self.heap[parent];
            self.put(nodes, place, p);
            place = parent;
        }
        self.put(nodes, place, node);
    }

    /// Adding (7.4): a first element whose f is at least the new one's
    /// moves to the end and the new one goes first; else the new one goes
    /// at the end. Then the end moves up.
    fn add(&mut self, nodes: &mut [Node], node: usize) {
        match self.heap.first().copied() {
            Some(first) if Self::f(nodes, first) >= Self::f(nodes, node) => {
                self.heap.push(first);
                let end = self.heap.len() - 1;
                self.put(nodes, end, first);
                self.put(nodes, 0, node);
            }
            _ => {
                self.heap.push(node);
                let end = self.heap.len() - 1;
                self.put(nodes, end, node);
            }
        }
        let end = self.heap.len() - 1;
        self.up(nodes, end);
    }

    /// Taking the first element (7.4).
    fn take(&mut self, nodes: &mut [Node]) -> Option<usize> {
        let first = *self.heap.first()?;
        let last = self.heap.pop()?;
        let len = self.heap.len();
        if len == 0 {
            return Some(first);
        }
        let mut place = 0;
        while 2 * place + 2 < len {
            let (left, right) = (2 * place + 1, 2 * place + 2);
            let child = if Self::f(nodes, self.heap[left]) < Self::f(nodes, self.heap[right]) {
                left
            } else {
                right
            };
            let c = self.heap[child];
            self.put(nodes, place, c);
            place = child;
        }
        if 2 * place + 1 < len {
            let c = self.heap[2 * place + 1];
            self.put(nodes, place, c);
            place = 2 * place + 1;
        }
        self.put(nodes, place, last);
        self.up(nodes, place);
        Some(first)
    }
}

/// A condition's value in `state`: its own, else the world's (7.1).
fn value_in(p: &mut dyn Planning, state: &WorldState, c: u32) -> Result<bool, String> {
    match state.value_of(c) {
        Some(v) => Ok(v),
        None => p.world_value(c),
    }
}

/// Whether `state` satisfies `w` (7.1): in increasing condition order,
/// stopping at the first property that fails.
fn satisfies(p: &mut dyn Planning, state: &WorldState, w: &WorldState) -> Result<bool, String> {
    for q in w.properties() {
        if value_in(p, state, q.condition)? != q.value {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `state` with `effects` applied (7.1): overwritten by the effects, each
/// condition whose value then equals its world value left out.
fn apply(
    p: &mut dyn Planning,
    state: &WorldState,
    effects: &WorldState,
) -> Result<WorldState, String> {
    let mut new = state.clone();
    for e in effects.properties() {
        if p.world_value(e.condition)? == e.value {
            new.unset(e.condition);
        } else {
            new.set(e.condition, e.value);
        }
    }
    Ok(new)
}

/// h (7.3): the goal properties `state` doesn't itself hold with the
/// goal's value.
fn estimate(state: &WorldState, goal: &WorldState) -> u16 {
    let missing = (goal.properties().iter())
        .filter(|q| state.value_of(q.condition) != Some(q.value))
        .count();
    missing as u16
}

/// The search (7.3): the plan, the ids of the actions from the start state
/// to a state that satisfies `goal`, first step first; `None` when it
/// fails. `steps` are in increasing id order.
pub fn search(
    p: &mut dyn Planning,
    goal: &WorldState,
    steps: &[Step],
) -> Result<Option<Vec<u32>>, String> {
    let start = WorldState::new();
    let h = estimate(&start, goal);
    let mut nodes = vec![Node {
        state: start.clone(),
        g: 0,
        f: h,
        predecessor: None,
        action: 0,
        open: true,
        place: 0,
    }];
    let mut reached: HashMap<Vec<WorldProperty>, usize> = HashMap::new();
    reached.insert(start.properties().to_vec(), 0);
    let mut open = Open { heap: vec![0] };
    let mut expanded: u32 = 0;
    loop {
        let Some(&top) = open.heap.first() else {
            return Ok(None);
        };
        if nodes[top].f == STOP_F || expanded == u32::MAX || nodes.len() >= MOST_REACHED {
            return Ok(None);
        }
        let current = open.take(&mut nodes).unwrap_or(top);
        let state = nodes[current].state.clone();
        if satisfies(p, &state, goal)? {
            let mut plan = Vec::new();
            let mut at = current;
            while let Some(before) = nodes[at].predecessor {
                plan.push(nodes[at].action);
                at = before;
            }
            plan.reverse();
            return Ok(Some(plan));
        }
        nodes[current].open = false;
        expanded += 1;
        for (i, step) in steps.iter().enumerate() {
            if !satisfies(p, &state, step.preconditions)? {
                continue;
            }
            let new = apply(p, &state, step.effects)?;
            let cost = p.weight(i, &new, &state)?;
            let g = nodes[current].g.wrapping_add(cost);
            match reached.get(new.properties()).copied() {
                None => {
                    let h = estimate(&new, goal);
                    let index = nodes.len();
                    reached.insert(new.properties().to_vec(), index);
                    nodes.push(Node {
                        state: new,
                        g,
                        f: g.wrapping_add(h),
                        predecessor: Some(current),
                        action: step.id,
                        open: true,
                        place: 0,
                    });
                    open.add(&mut nodes, index);
                }
                Some(index) if nodes[index].open && g < nodes[index].g => {
                    let h = nodes[index].f.wrapping_sub(nodes[index].g);
                    let n = &mut nodes[index];
                    n.g = g;
                    n.f = g.wrapping_add(h);
                    n.predecessor = Some(current);
                    n.action = step.id;
                    let place = n.place;
                    open.up(&mut nodes, place);
                }
                Some(_) => {}
            }
        }
    }
}
