// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The per-object light inputs of docs/behavior/level-objects.md section
//! 5.4: sky visibility from 26 fixed rays, sun visibility from one ray
//! towards the sun, both against the static collision, and the shader
//! constant `L_dynamic_props` made of them.
//!
//! The game moves each value towards its target over frames; here the
//! converged values are taken at once (the values the section gives for a
//! still frame long after spawn; those of the first frame are
//! unestablished).

use crate::collision::CollisionWorld;
use crate::collision::geometry::{Vec3, normalize};

/// The 26 sky directions, in the order they are tested (normalised before
/// use).
pub const SKY_DIRECTIONS: [Vec3; 26] = [
    [0.0, 1.0, 0.0],
    [0.52573, 0.85065, 0.0],
    [0.16246, 0.85065, 0.5],
    [-0.42533, 0.85065, 0.30902],
    [-0.42533, 0.85065, -0.30902],
    [0.16246, 0.85065, -0.5],
    [0.89443, 0.44721, 0.0],
    [0.27639, 0.44721, 0.85065],
    [-0.72361, 0.44721, 0.52573],
    [-0.72361, 0.44721, -0.52573],
    [0.27639, 0.44721, -0.85065],
    [0.68819, 0.52573, 0.5],
    [-0.26287, 0.52573, 0.80902],
    [-0.85065, 0.52573, 0.0],
    [-0.26287, 0.52573, -0.80902],
    [0.68819, 0.52573, -0.5],
    [0.95106, 0.0, 0.30902],
    [0.58779, 0.0, 0.80902],
    [0.0, 0.0, 1.0],
    [-0.58779, 0.0, 0.80902],
    [-0.95106, 0.0, 0.30902],
    [-0.95106, 0.0, -0.30902],
    [-0.58779, 0.0, -0.80902],
    [0.0, 0.0, -1.0],
    [0.58779, 0.0, -0.80902],
    [0.95106, 0.0, -0.30902],
];

/// Length of a sky ray, metres.
pub const SKY_RAY: f32 = 50.0;
/// Length of the sun ray, metres.
pub const SUN_RAY: f32 = 500.0;

/// The point the rays start from: `C + (0, 0.3 R, 0)` for the world
/// sphere (C, R) of the object's model.
pub fn ray_origin(centre: Vec3, radius: f32) -> Vec3 {
    [centre[0], centre[1] + 0.3 * radius, centre[2]]
}

/// The converged sky visibility `h`: the fraction of the 26 directions
/// whose ray from `origin` hits no static triangle.
pub fn sky_visibility(world: &CollisionWorld, origin: Vec3) -> f32 {
    let free = SKY_DIRECTIONS
        .iter()
        .filter_map(|&d| normalize(d))
        .filter(|&d| !world.ray_hits(origin, d, SKY_RAY))
        .count();
    free as f32 / SKY_DIRECTIONS.len() as f32
}

/// The converged sun visibility `s`: 1 if the ray from `origin` towards the
/// sun (`−sun_dir`, `sun_dir` the direction the light travels) hits no
/// static triangle, else 0. Other objects do not block it here (our
/// choice: objects have no collision shapes yet).
pub fn sun_visibility(world: &CollisionWorld, origin: Vec3, sun_dir: Vec3) -> f32 {
    match normalize([-sun_dir[0], -sun_dir[1], -sun_dir[2]]) {
        Some(d) if !world.ray_hits(origin, d, SUN_RAY) => 1.0,
        _ => 0.0,
    }
}

/// Directions tested per update (5.4).
const PER_UPDATE: usize = 5;

/// An object's sky visibility *h* followed over frames
/// (level-objects.md 5.4, sound-weather-ambient.md 4.3): each update tests
/// five of the 26 directions, cycling in order, at most once per frame;
/// the target is the fraction of free results among the tested (the 26
/// latest once all were tested); *h* ← *k* · target + (1 − *k*) · *h*
/// with *k* = clamp(frame time, 0, 1) once per frame, by the update or by
/// a read that comes first; 0.5 initially and the target itself at the
/// first update.
#[derive(Clone, Debug, PartialEq)]
pub struct SkyVisibility {
    free: [Option<bool>; 26],
    next: usize,
    h: f32,
    target: f32,
    updated: bool,
    /// The frames of the last smoothing step and the last rays.
    smoothed: Option<u64>,
    cast: Option<u64>,
}

impl Default for SkyVisibility {
    fn default() -> Self {
        Self {
            free: [None; 26],
            next: 0,
            h: 0.5,
            target: 0.5,
            updated: false,
            smoothed: None,
            cast: None,
        }
    }
}

impl SkyVisibility {
    /// *h* as last smoothed.
    pub fn value(&self) -> f32 {
        self.h
    }

    /// The target of the latest update (0.5 before the first).
    pub fn target(&self) -> f32 {
        self.target
    }

    /// The smoothing step of `frame`, `frame_time` seconds long, unless it
    /// ran in that frame (4.3).
    fn smooth(&mut self, frame: u64, frame_time: f32) {
        if self.smoothed == Some(frame) {
            return;
        }
        self.smoothed = Some(frame);
        if self.updated {
            let k = frame_time.clamp(0.0, 1.0);
            self.h = k * self.target + (1.0 - k) * self.h;
        }
    }

    /// *h* read in `frame` (4.3): the frame's smoothing step applied if it
    /// has not run yet, no rays cast.
    pub fn read(&mut self, frame: u64, frame_time: f32) -> f32 {
        self.smooth(frame, frame_time);
        self.h
    }

    /// The update of `frame`, `frame_time` seconds long, from `origin`
    /// against `world`; nothing if `frame` already had one.
    pub fn update(&mut self, frame: u64, world: &CollisionWorld, origin: Vec3, frame_time: f32) {
        self.update_with(frame, frame_time, |d| !world.ray_hits(origin, d, SKY_RAY));
    }

    /// [`Self::update`] with `free` telling whether the ray along a
    /// (normalised) direction is free.
    pub fn update_with(&mut self, frame: u64, frame_time: f32, mut free: impl FnMut(Vec3) -> bool) {
        if self.cast == Some(frame) {
            return;
        }
        self.cast = Some(frame);
        for _ in 0..PER_UPDATE {
            let f = normalize(SKY_DIRECTIONS[self.next]).is_some_and(&mut free);
            self.free[self.next] = Some(f);
            self.next = (self.next + 1) % SKY_DIRECTIONS.len();
        }
        let tested = self.free.iter().flatten().count();
        let free = self.free.iter().flatten().filter(|&&f| f).count();
        self.target = free as f32 / tested as f32;
        if self.updated {
            self.smooth(frame, frame_time);
        } else {
            self.h = self.target;
            self.updated = true;
            self.smoothed = Some(frame);
        }
    }
}

/// `L_dynamic_props` = (0.5 s, 0.5 s, 0.5 s, 0.5 h).
pub fn dynamic_props(h: f32, s: f32) -> [f32; 4] {
    [0.5 * s, 0.5 * s, 0.5 * s, 0.5 * h]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::test_world::Builder;

    #[test]
    fn visibility_under_a_roof() {
        // A floor at y = 0 and a 4 × 4 m roof at y = 3 over the origin.
        let mut b = Builder::default();
        b.floor([-100.0, 100.0], [-100.0, 100.0], 0.0, true);
        b.floor([-2.0, 2.0], [-2.0, 2.0], 3.0, false);
        let world = b.world();
        let under = ray_origin([0.0, 1.0, 0.0], 1.0);
        assert_eq!(under, [0.0, 1.3, 0.0]);
        // The roof (1.7 m up, its edges 2 … 2.83 m out) covers the
        // directions steeper than 31° … 40° from the horizon: the zenith
        // and the 5 at 58.3°; the 10 at 26.6° and 31.7° and the 10
        // horizontal ones are free.
        let h = sky_visibility(&world, under);
        assert!((h - 20.0 / 26.0).abs() < 1e-6, "{h}");
        let outside = ray_origin([20.0, 1.0, 0.0], 1.0);
        assert_eq!(sky_visibility(&world, outside), 1.0);
        // Sunlight straight down: blocked under the roof, not outside it.
        let down = [0.0, -1.0, 0.0];
        assert_eq!(sun_visibility(&world, under, down), 0.0);
        assert_eq!(sun_visibility(&world, outside, down), 1.0);
        // Low sun from +x: the roof does not block it.
        assert_eq!(sun_visibility(&world, under, [-1.0, -0.1, 0.0]), 1.0);
        assert_eq!(dynamic_props(0.5, 1.0), [0.5, 0.5, 0.5, 0.25]);
    }

    #[test]
    fn sky_visibility_follows_over_updates() {
        let mut b = Builder::default();
        b.floor([-100.0, 100.0], [-100.0, 100.0], 0.0, true);
        b.floor([-2.0, 2.0], [-2.0, 2.0], 3.0, false);
        let world = b.world();
        let under = ray_origin([0.0, 1.0, 0.0], 1.0);
        let mut h = SkyVisibility::default();
        assert_eq!(h.value(), 0.5);
        // The first update: its five directions (the zenith and four at
        // 58.3°, all under the roof), the target directly.
        h.update(0, &world, under, 0.016);
        assert_eq!(h.value(), 0.0);
        // After all 26 are tested, it moves towards 20/26 by the frame time.
        for frame in 1..200 {
            h.update(frame, &world, under, 0.5);
        }
        assert!((h.value() - 20.0 / 26.0).abs() < 1e-5, "{}", h.value());
    }

    /// The pattern of free directions `free` as a ray test.
    fn pattern(free: &[usize]) -> impl Fn(Vec3) -> bool + '_ {
        move |d| {
            let i = SKY_DIRECTIONS
                .iter()
                .position(|&s| normalize(s).is_some_and(|s| s == d))
                .unwrap();
            free.contains(&i)
        }
    }

    #[test]
    fn sky_visibility_updates_once_per_frame_and_reads_smooth_without_rays() {
        // sound-weather-ambient.md 4.3: before any update a read gives the
        // initial 0.5; an update casts at most once per frame and smooths
        // at most once per frame, also when a read of that frame came
        // first.
        let mut h = SkyVisibility::default();
        assert_eq!(h.read(0, 0.5), 0.5);
        let mut rays = 0;
        h.update_with(1, 0.5, |_| {
            rays += 1;
            true
        });
        assert_eq!((rays, h.value(), h.target()), (5, 1.0, 1.0));
        h.update_with(1, 0.5, |_| {
            rays += 1;
            false
        });
        assert_eq!((rays, h.value()), (5, 1.0));
        // Frame 2: the rule reads first (the previous frame's target, 1),
        // then the frame's rays make the target 5/10, not applied until
        // frame 3's read.
        assert_eq!(h.read(2, 0.5), 1.0);
        h.update_with(2, 0.5, |_| false);
        assert_eq!((h.value(), h.target()), (1.0, 0.5));
        assert_eq!(h.read(3, 0.5), 0.75);
        assert_eq!(h.read(3, 0.5), 0.75);
        // Without a read, the update smooths with the new target.
        h.update_with(4, 0.5, |_| false);
        assert!((h.target() - 5.0 / 15.0).abs() < 1e-6);
        assert!((h.value() - (0.75 + 5.0 / 15.0) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn sky_visibility_in_the_first_level() {
        // sound-weather-ambient.md 4.3 (H3, H5): in the bunker no ray is
        // free, so h = 0 from the first update on and stays there.
        let mut h = SkyVisibility::default();
        for frame in 0..30 {
            h.update_with(frame, 1.0 / 60.0, pattern(&[]));
            assert_eq!(h.value(), 0.0);
        }
        // Outdoors 13 of the 26 are free, 4 of the first 5: h = 0.8 at the
        // first update, the target 0.5 once all 26 are tested (6 updates).
        let free = [0, 1, 2, 3, 6, 7, 8, 11, 12, 16, 17, 18, 19];
        let mut h = SkyVisibility::default();
        h.update_with(0, 1.0 / 60.0, pattern(&free));
        assert!((h.value() - 0.8).abs() < 1e-6);
        for frame in 1..6 {
            h.update_with(frame, 1.0 / 60.0, pattern(&free));
        }
        assert_eq!(h.target(), 0.5);
        // The view is outdoors from h = 0.05 (4.3); from the bunker's h = 0
        // at 60 frames per second it takes several frames (8 – 10 for the
        // level's directions, H6; here, frames of 1/60 s, 7 – 12).
        let mut h = SkyVisibility::default();
        for frame in 0..30 {
            h.update_with(frame, 1.0 / 60.0, pattern(&[]));
        }
        let mut frames = 0;
        for frame in 30..100 {
            h.update_with(frame, 1.0 / 60.0, pattern(&free));
            frames += 1;
            if h.read(frame, 1.0 / 60.0) >= 0.05 {
                break;
            }
        }
        assert!((7..=12).contains(&frames), "{frames}");
    }

    #[test]
    fn rays_cross_cells_and_stop_at_their_length() {
        // A wall at x = 30, from z = −5 to 5, up to y = 10.
        let mut b = Builder::default();
        b.floor([-50.0, 50.0], [-50.0, 50.0], 0.0, true);
        b.wall_x(30.0, [0.0, 10.0], [-5.0, 5.0], true);
        let world = b.world();
        let o = [0.0, 1.0, 0.0];
        assert!(world.ray_hits(o, [1.0, 0.0, 0.0], 50.0));
        assert!(!world.ray_hits(o, [1.0, 0.0, 0.0], 29.0));
        assert!(!world.ray_hits(o, [-1.0, 0.0, 0.0], 50.0));
        assert!(world.ray_hits(o, [0.0, -1.0, 0.0], 2.0));
        let diagonal = normalize([1.0, 0.0, 0.1]).unwrap();
        assert!(world.ray_hits(o, diagonal, 40.0));
    }
}
