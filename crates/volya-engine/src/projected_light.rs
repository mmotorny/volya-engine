// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The projected light texture's bookkeeping
//! (docs/behavior/object-projected-light.md sections 2, 3, 5): which
//! objects are receivers in a frame, the 100 slots and their cells, when a
//! cell is redrawn, and the approximate colour a cell is cleared with. The
//! renderer draws the cells and samples them (`volya-engine-render`).

use crate::core::camera::Vec3;
use crate::core::collision::CollisionWorld;
use crate::core::lights::Lights;
use crate::core::particles::random::Generator;

/// Slots, and so cells and receivers a frame (section 2).
pub const SLOTS: usize = 100;

/// A receiver taken in a frame (3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Receiver {
    /// The object instance.
    pub instance: usize,
    /// Its model's header sphere under its transform.
    pub centre: Vec3,
    pub radius: f32,
    /// Its current world box (3.2).
    pub min: Vec3,
    pub max: Vec3,
}

/// A slot (section 2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Slot {
    receiver: Option<usize>,
    /// The frame it was last used in.
    used: Option<u64>,
    /// The recorded centre C′ and h = 1.1 R (3.2, 4.1).
    centre: Vec3,
    h: f32,
    /// The recorded box.
    min: Vec3,
    max: Vec3,
    /// The re-draw time, ms of the global clock.
    redraw: u64,
}

/// A cell as last drawn for its slot (section 4): drawn again by the
/// renderer whenever its `version` is new to it, so a cell is drawn in
/// the first frame drawn after its slot was given out (one the renderer
/// may not have drawn: a frame computed but not drawn, a new renderer).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellDraw {
    /// Unique to each giving out of a slot in a level's run, from 1.
    pub version: u64,
    pub slot: u32,
    pub centre: Vec3,
    pub h: f32,
    /// The clear colour as bytes R, G, B, A (4.2).
    pub clear: [u8; 4],
    /// The static draws (draw items) of the receiver's sector it draws
    /// (4.3).
    pub draws: Vec<u32>,
}

/// A receiver drawn with its slot's constants (section 6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReceiverSlot {
    pub instance: usize,
    pub slot: u32,
    pub centre: Vec3,
    pub h: f32,
}

/// The slots for a level's run.
#[derive(Debug)]
pub struct ProjectedLight {
    slots: [Slot; SLOTS],
    /// Each instance's slot at its last use.
    slot_of: Vec<Option<u8>>,
    /// The re-draw times' values: a generator of our own (which of the
    /// game's generators is not stated; our choice).
    random: Generator,
    /// Each slot's cell as last given out, and the last version.
    cells: Vec<Option<std::sync::Arc<CellDraw>>>,
    version: u64,
}

impl Default for ProjectedLight {
    fn default() -> Self {
        Self {
            slots: [Slot::default(); SLOTS],
            slot_of: Vec::new(),
            random: Generator::new(0),
            cells: vec![None; SLOTS],
            version: 0,
        }
    }
}

/// Whether box `a` lies inside box `b` on all three axes.
fn inside(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
    (0..3).all(|i| a.0[i] >= b.0[i] && a.1[i] <= b.1[i])
}

impl ProjectedLight {
    /// The slots of frame `frame` at global time `time_ms` for its
    /// receivers in the order taken (3.2): those kept and those given a
    /// slot, each with its slot's constants, and the receivers given a
    /// slot, whose cells are drawn (`(receiver index, slot)`).
    pub fn frame(
        &mut self,
        frame: u64,
        time_ms: u64,
        receivers: &[Receiver],
        drawn: &mut Vec<ReceiverSlot>,
        new: &mut Vec<(usize, u32)>,
    ) {
        drawn.clear();
        new.clear();
        let mut queue = Vec::new();
        for (k, r) in receivers.iter().enumerate() {
            if self.slot_of.len() <= r.instance {
                self.slot_of.resize(r.instance + 1, None);
            }
            let kept = self.slot_of[r.instance].map(usize::from).filter(|&s| {
                let slot = &self.slots[s];
                slot.receiver == Some(r.instance)
                    && inside((r.min, r.max), (slot.min, slot.max))
                    && time_ms <= slot.redraw
            });
            match kept {
                Some(s) => {
                    let slot = &mut self.slots[s];
                    slot.used = Some(frame);
                    drawn.push(ReceiverSlot {
                        instance: r.instance,
                        slot: s as u32,
                        centre: slot.centre,
                        h: slot.h,
                    });
                }
                None => queue.push(k),
            }
        }
        for s in 0..SLOTS {
            if self.slots[s].used == Some(frame) {
                continue;
            }
            // The last queued first.
            let Some(k) = queue.pop() else { break };
            let r = &receivers[k];
            let grow = |i: usize| 0.1 * (r.max[i] - r.min[i]);
            let n = (self.random.next_k() << 15) | self.random.next_k();
            let slot = Slot {
                receiver: Some(r.instance),
                used: Some(frame),
                centre: [r.centre[0], r.centre[1] + 0.1 * r.radius, r.centre[2]],
                h: 1.1 * r.radius,
                min: std::array::from_fn(|i| r.min[i] - grow(i)),
                max: std::array::from_fn(|i| r.max[i] + grow(i)),
                redraw: time_ms + 30_000 + u64::from(n % 60_000),
            };
            self.slots[s] = slot;
            self.slot_of[r.instance] = Some(s as u8);
            drawn.push(ReceiverSlot {
                instance: r.instance,
                slot: s as u32,
                centre: slot.centre,
                h: slot.h,
            });
            new.push((k, s as u32));
        }
        // A receiver left without a slot draws as one that is not (what
        // the original shows is unestablished, 3.2; our choice).
    }
}

impl ProjectedLight {
    /// Keeps `cell` as its slot's, with a new version.
    pub fn set_cell(&mut self, mut cell: CellDraw) {
        self.version += 1;
        cell.version = self.version;
        if let Some(c) = self.cells.get_mut(cell.slot as usize) {
            *c = Some(std::sync::Arc::new(cell));
        }
    }

    /// Every slot's cell as last given out.
    pub fn cells(&self) -> impl Iterator<Item = &std::sync::Arc<CellDraw>> {
        self.cells.iter().flatten()
    }
}

/// Whether a receiver at `centre` with radius `radius` is close enough
/// to a camera at `eye` (3.1 rule 4).
pub fn close_enough(eye: Vec3, centre: Vec3, radius: f32) -> bool {
    let d: Vec3 = std::array::from_fn(|i| eye[i] - centre[i]);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() + radius < 50.0 * radius
}

/// Whether an object of class `class_id` receives shadows (3.1 rule 2).
pub fn receives_shadows(class_id: &str) -> bool {
    class_id != "AI_CROW"
}

/// The weather's values the approximate colour takes (5.2 step 4).
#[derive(Clone, Copy, Debug)]
pub struct LightEnvironment {
    pub ambient: Vec3,
    pub hemi: Vec3,
    pub sun_color: Vec3,
}

/// The approximate colour **A** of an object with header sphere
/// (`centre`, `radius`), sky and sun visibility `h`, `s` (5.2), with the
/// light records of `lights` and the collision world `collision`.
///
/// Taken converged at once (#565), as the objects' visibility values are
/// (docs/architecture/rendering.md, Objects; our choice): each tracked
/// light's smoothed visibility σ is 1 when the ray from the light to **o**
/// hits nothing (*e* settles at 1) and below the threshold otherwise (*e*
/// settles at −0.5), the ray aimed at **o** itself rather than at a random
/// point within *R*/2 of it.
pub fn approximate_colour(
    centre: Vec3,
    radius: f32,
    h: f32,
    s: f32,
    env: &LightEnvironment,
    lights: &Lights,
    collision: &CollisionWorld,
) -> Vec3 {
    let o = [centre[0], centre[1] + 0.3 * radius, centre[2]];
    let mut a: Vec3 =
        std::array::from_fn(|i| env.ambient[i] + h * env.hemi[i] + s * env.sun_color[i]);
    let mean = |c: Vec3| (c[0] + c[1] + c[2]) / 3.0;
    let mut add = |c: Vec3, l: Vec3, r: f32, w: f32| {
        let d: Vec3 = std::array::from_fn(|i| l[i] - o[i]);
        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let f = (1.0 - dist / (r + 1e-5)).clamp(0.0, 1.0);
        for i in 0..3 {
            a[i] += w * c[i] * f;
        }
    };
    for (_, l) in lights.iter() {
        if !l.active {
            continue;
        }
        // The spatial query: the light's bounding sphere meets o ± R.
        let meets = (0..3).all(|i| {
            l.centre[i] + l.radius >= o[i] - radius && l.centre[i] - l.radius <= o[i] + radius
        });
        let d: Vec3 = std::array::from_fn(|i| o[i] - l.position[i]);
        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if !meets || dist >= l.range + radius {
            continue;
        }
        let c = [l.colour[0], l.colour[1], l.colour[2]];
        let free = dist <= 0.0 || !collision.ray_hits(l.position, d.map(|x| x / dist), dist);
        if free && mean(c) > 1e-5 {
            add(c, l.position, l.range, 0.5);
        }
    }
    if let Some(b) = lights.sun().and_then(|(_, b)| lights.get(b)) {
        let c = [b.colour[0], b.colour[1], b.colour[2]];
        if mean(c) * s > 1e-5 {
            add(c, b.position, b.range, s / 2.0);
        }
    }
    a
}

/// The clear bytes of approximate colour `a` (4.2).
pub fn clear_bytes(a: Vec3) -> [u8; 4] {
    let byte = |x: f32| (255.0 * x).trunc().clamp(0.0, 255.0) as u8;
    [
        byte(a[0]),
        byte(a[1]),
        byte(a[2]),
        byte((a[0] + a[1] + a[2]) / 4.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receiver(instance: usize, x: f32) -> Receiver {
        Receiver {
            instance,
            centre: [x, 0.0, 0.0],
            radius: 1.0,
            min: [x - 1.0, -1.0, -1.0],
            max: [x + 1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn slots_are_kept_and_given_out() {
        let mut p = ProjectedLight::default();
        let (mut drawn, mut new) = (Vec::new(), Vec::new());
        let rs = [receiver(7, 0.0), receiver(3, 10.0)];
        p.frame(1, 0, &rs, &mut drawn, &mut new);
        // The last queued first: receiver 3 takes slot 0.
        assert_eq!(new, [(1, 0), (0, 1)]);
        let slot = drawn.iter().find(|d| d.instance == 7).unwrap();
        assert_eq!((slot.slot, slot.centre, slot.h), (1, [0.0, 0.1, 0.0], 1.1));
        // Unmoved, before the re-draw time: kept, nothing drawn.
        p.frame(2, 1000, &rs, &mut drawn, &mut new);
        assert!(new.is_empty());
        assert_eq!(drawn.len(), 2);
        // Moved out of its recorded box (grown by 0.1 of the size, 0.2 m):
        // redrawn, in a slot not used this frame.
        let moved = [receiver(7, 0.3), receiver(3, 10.0)];
        p.frame(3, 2000, &moved, &mut drawn, &mut new);
        assert_eq!(new, [(0, 1)]);
        let small = [receiver(7, 0.15), receiver(3, 10.0)];
        p.frame(4, 3000, &small, &mut drawn, &mut new);
        assert!(new.is_empty());
        // Past the re-draw time (30 … 90 s): redrawn.
        p.frame(5, 100_000, &small, &mut drawn, &mut new);
        assert_eq!(new.len(), 2);
    }

    #[test]
    fn at_most_a_hundred_slots() {
        let mut p = ProjectedLight::default();
        let (mut drawn, mut new) = (Vec::new(), Vec::new());
        let rs: Vec<Receiver> = (0..120).map(|i| receiver(i, i as f32 * 10.0)).collect();
        p.frame(1, 0, &rs, &mut drawn, &mut new);
        assert_eq!(new.len(), SLOTS);
        assert_eq!(drawn.len(), SLOTS);
    }

    #[test]
    fn receivers_and_clear_bytes() {
        // An object of radius 0.15 m is a receiver up to 7.35 m (7).
        assert!(close_enough([7.34, 0.0, 0.0], [0.0; 3], 0.15));
        assert!(!close_enough([7.36, 0.0, 0.0], [0.0; 3], 0.15));
        assert!(!receives_shadows("AI_CROW"));
        assert!(receives_shadows("AI_TRADE"));
        // Section 7: the start's ambient clears to 12, 13, 14 and 9.
        assert_eq!(clear_bytes([0.04814, 0.05170, 0.05525]), [12, 13, 14, 9]);
    }
}
