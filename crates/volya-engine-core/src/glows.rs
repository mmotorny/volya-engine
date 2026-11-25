// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Glows (docs/behavior/glows.md): the level's (`level` chunk 7) and the
//! objects' (hanging lamps; searchlights and torches later), gathered and
//! faded each frame (section 3), tested for occlusion (section 5) and
//! turned into camera-facing sprites (section 6).
//!
//! As for the dynamic lights ([`crate::lights`]), every sector counts as
//! visited with the full view frustum (the superset `level.md` 9.2
//! allows): a glow's sector matters only as whether it has one.

use crate::camera::Vec3;
use crate::lights::Frustum;

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The smallest normal f32: a direction whose squared length is not above
/// it is ignored (section 2.2).
const MIN_NORMAL: f32 = 1.175_494_4e-38;

/// One glow (section 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Glow {
    pub position: Vec3,
    /// Half-size of the sprite, metres.
    pub radius: f32,
    /// A unit vector; `None`: seen from all sides.
    pub direction: Option<Vec3>,
    /// The caller's index of its shader (texture and pass).
    pub shader: u32,
    pub on: bool,
    /// 1 … 255.
    pub fade: f32,
    /// The latest occlusion test's result.
    pub occluded: bool,
    /// Whether it has a sector (module documentation).
    pub in_sector: bool,
    /// Marked for sector re-detection.
    sector_due: bool,
    /// The frame it was last tested for occlusion in.
    tested: Option<u64>,
}

/// A glow of [`Glows`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlowId(u32);

/// The camera of a frame (section 3, conventions).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowView {
    /// E.
    pub eye: Vec3,
    /// The view direction d, the up vector u and the right vector r.
    pub forward: Vec3,
    pub up: Vec3,
    pub right: Vec3,
    pub frustum: Frustum,
    /// Z: the far plane's target value.
    pub far: f32,
    /// Whether the camera's sector is known.
    pub camera_in_sector: bool,
}

/// A glow's sprite (section 6): the corners of table 6 in order, with
/// texture coordinates (0, 1), (0, 0), (1, 1), (1, 0), drawn as the
/// triangles (0, 1, 2) and (3, 2, 1) with the colour (b, b, b, b) / 255.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowSprite {
    pub shader: u32,
    pub corners: [Vec3; 4],
    pub b: u8,
}

/// The texture coordinate of each sprite corner (section 6).
pub const CORNER_UV: [[f32; 2]; 4] = [[0.0, 1.0], [0.0, 0.0], [1.0, 1.0], [1.0, 0.0]];

/// The level's glows and the objects'.
#[derive(Clone, Debug, Default)]
pub struct Glows {
    glows: Vec<Option<Glow>>,
    free: Vec<u32>,
    /// The occlusion counter k (section 5): 0 at load, never reset.
    k: u64,
    frame: u64,
}

impl Glows {
    fn insert(&mut self, glow: Glow) -> GlowId {
        match self.free.pop() {
            Some(i) => {
                self.glows[i as usize] = Some(glow);
                GlowId(i)
            }
            None => {
                self.glows.push(Some(glow));
                GlowId(self.glows.len() as u32 - 1)
            }
        }
    }

    /// A level glow (section 2.1): on, fade 255, occluded, sector found at
    /// the first gather.
    pub fn add_level(&mut self, position: Vec3, radius: f32, shader: u32) -> GlowId {
        self.insert(Glow {
            position,
            radius,
            direction: None,
            shader,
            on: true,
            fade: 255.0,
            occluded: true,
            in_sector: false,
            sector_due: true,
            tested: None,
        })
    }

    /// An object's glow (section 2.2): off, fade 1, not occluded, radius
    /// 0.1, at the origin, no direction.
    pub fn create(&mut self, shader: u32) -> GlowId {
        self.insert(Glow {
            position: [0.0; 3],
            radius: 0.1,
            direction: None,
            shader,
            on: false,
            fade: 1.0,
            occluded: false,
            in_sector: false,
            sector_due: true,
            tested: None,
        })
    }

    pub fn destroy(&mut self, id: GlowId) {
        if let Some(slot) = self.glows.get_mut(id.0 as usize)
            && slot.take().is_some()
        {
            self.free.push(id.0);
        }
    }

    pub fn get(&self, id: GlowId) -> Option<&Glow> {
        self.glows.get(id.0 as usize)?.as_ref()
    }

    fn glow(&mut self, id: GlowId) -> Option<&mut Glow> {
        self.glows.get_mut(id.0 as usize)?.as_mut()
    }

    /// Every glow.
    pub fn iter(&self) -> impl Iterator<Item = &Glow> {
        self.glows.iter().flatten()
    }

    /// Taken only when some axis differs by at least 0.001 (section 2.2).
    pub fn set_position(&mut self, id: GlowId, p: Vec3) {
        let Some(g) = self.glow(id) else { return };
        if (0..3).all(|k| (p[k] - g.position[k]).abs() < 0.001) {
            return;
        }
        g.position = p;
        g.sector_due = true;
    }

    /// Taken only when it differs by at least 10⁻⁵ (section 2.2).
    pub fn set_radius(&mut self, id: GlowId, radius: f32) {
        let Some(g) = self.glow(id) else { return };
        if (radius - g.radius).abs() < 1e-5 {
            return;
        }
        g.radius = radius;
        g.sector_due = true;
    }

    /// Normalised; ignored when its squared length is not above the
    /// smallest normal f32 (section 2.2).
    pub fn set_direction(&mut self, id: GlowId, d: Vec3) {
        let Some(g) = self.glow(id) else { return };
        let l2 = dot(d, d);
        if l2 <= MIN_NORMAL {
            return;
        }
        let l = l2.sqrt();
        g.direction = Some(d.map(|c| c / l));
    }

    /// Switching on marks the sector for re-detection (section 2.2).
    pub fn set_on(&mut self, id: GlowId, on: bool) {
        let Some(g) = self.glow(id) else { return };
        if on && !g.on {
            g.sector_due = true;
        }
        g.on = on;
    }

    /// One frame (sections 3 – 6): the glows gathered from `view` and
    /// faded by the frame time `dt` (0 while paused), then up to
    /// `per_frame` (`r1_glows_per_frame`) of the listed ones tested with
    /// `occluded(from, direction, length)`, then the sprites of the listed
    /// glows. `in_sector` tells whether the downward ray of `level.md` 9.1
    /// from a point finds a sector.
    pub fn frame(
        &mut self,
        view: &GlowView,
        dt: f32,
        per_frame: u32,
        mut in_sector: impl FnMut(Vec3) -> bool,
        mut occluded: impl FnMut(Vec3, Vec3, f32) -> bool,
    ) -> Vec<GlowSprite> {
        self.frame += 1;
        let e = view.eye;
        let distance2 = |g: &Glow| {
            let v = sub(g.position, e);
            dot(v, v)
        };
        let mut listed: Vec<usize> = Vec::new();
        if view.camera_in_sector {
            // Step 1: on and not outside the view, nearest first.
            let mut candidates: Vec<(f32, usize)> = (self.glows.iter().enumerate())
                .filter_map(|(i, g)| Some((i, g.as_ref()?)))
                .filter(|(_, g)| g.on && view.frustum.meets(g.position, g.radius))
                .map(|(i, g)| (distance2(g), i))
                .collect();
            candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
            let z2 = view.far * view.far;
            for (d2, i) in candidates {
                let g = self.glows[i].as_mut().expect("a candidate exists");
                // Step 2.
                if g.sector_due {
                    g.sector_due = false;
                    g.in_sector |= in_sector(g.position);
                }
                // Steps 3, 4: every sector is visited with the full view
                // frustum, which step 1 tested.
                if !g.in_sector {
                    continue;
                }
                if d2 < z2 {
                    g.fade = if g.occluded {
                        (g.fade - 1024.0 * dt).max(1.0)
                    } else {
                        (g.fade + 4096.0 * dt).min(255.0)
                    };
                    listed.push(i);
                } else {
                    g.fade = (g.fade - 1024.0 * dt).max(1.0);
                }
            }
        }
        if listed.is_empty() {
            return Vec::new();
        }
        // Section 5.
        let len = listed.len() as u64;
        for _ in 0..per_frame {
            let i = listed[(self.k % len) as usize];
            let g = self.glows[i].as_mut().expect("a listed glow exists");
            if g.tested == Some(self.frame) {
                break;
            }
            g.tested = Some(self.frame);
            let to = sub(g.position, e);
            let d = dot(to, to).sqrt();
            if d > 1e-7 {
                g.occluded = occluded(e, to.map(|c| c / d), d);
            }
            self.k += 1;
        }
        // Section 6.
        let dl = dot(view.forward, view.forward).sqrt();
        let forward = view.forward.map(|c| c / dl);
        let z2 = view.far * view.far;
        let mut sprites = Vec::with_capacity(listed.len());
        for i in listed {
            let g = self.glows[i].as_ref().expect("a listed glow exists");
            if g.fade <= 1.0 {
                continue;
            }
            let to_eye = sub(e, g.position);
            let d2 = dot(to_eye, to_eye);
            let c = match g.direction.filter(|v| dot(*v, *v) > 1e-5) {
                None => 1.0,
                Some(v) => dot(v, to_eye) / d2.sqrt(),
            };
            if g.fade * c <= 1.0 {
                continue;
            }
            let z = dot(forward, sub(g.position, e)).abs();
            let t = ((z - 0.2) * 20.0 / 3.0).clamp(0.0, 1.0);
            if g.fade * t * c <= 1.0 {
                continue;
            }
            let a = (1.0 - d2 / z2) * g.fade * t * c;
            let (p, rho, u, r) = (g.position, g.radius, view.up, view.right);
            let corner = |su: f32, sr: f32| -> Vec3 {
                std::array::from_fn(|k| p[k] + su * rho * u[k] + sr * rho * r[k])
            };
            sprites.push(GlowSprite {
                shader: g.shader,
                corners: [
                    corner(-1.0, -1.0),
                    corner(1.0, -1.0),
                    corner(-1.0, 1.0),
                    corner(1.0, 1.0),
                ],
                b: a.clamp(0.0, 255.0) as u8,
            });
        }
        sprites
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view from the origin along +z seeing everything.
    fn view() -> GlowView {
        GlowView {
            eye: [0.0; 3],
            forward: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
            right: [1.0, 0.0, 0.0],
            frustum: Frustum([[0.0, 0.0, 0.0, 1.0]; 6]),
            far: 100.0,
            camera_in_sector: true,
        }
    }

    #[test]
    fn level_and_object_glows_start_as_stated() {
        let mut glows = Glows::default();
        let level = glows.add_level([1.0, 2.0, 3.0], 0.5, 7);
        let lamp = glows.create(2);
        let l = glows.get(level).unwrap();
        assert!(l.on && l.occluded && l.fade == 255.0 && l.direction.is_none());
        let o = glows.get(lamp).unwrap();
        assert!(!o.on && !o.occluded && o.fade == 1.0);
        assert_eq!((o.radius, o.position), (0.1, [0.0; 3]));
        // Small changes are ignored.
        glows.set_position(lamp, [0.0009, 0.0, 0.0]);
        glows.set_radius(lamp, 0.100_005);
        glows.set_direction(lamp, [0.0, 0.0, 0.0]);
        let o = glows.get(lamp).unwrap();
        assert_eq!((o.position, o.radius, o.direction), ([0.0; 3], 0.1, None));
        glows.set_direction(lamp, [0.0, 0.0, -3.0]);
        assert_eq!(glows.get(lamp).unwrap().direction, Some([0.0, 0.0, -1.0]));
    }

    #[test]
    fn fading_takes_the_stated_times() {
        // Section 3: a full rise takes 0.062 s, a full fall 0.248 s.
        let mut glows = Glows::default();
        let id = glows.create(0);
        glows.set_position(id, [0.0, 0.0, 10.0]);
        glows.set_on(id, true);
        let clear = |_: Vec3, _: Vec3, _: f32| false;
        glows.frame(&view(), 0.031, 16, |_| true, clear);
        assert!((glows.get(id).unwrap().fade - (1.0 + 4096.0 * 0.031)).abs() < 1e-3);
        // 254 / 4096 = 0.0620 s in all.
        glows.frame(&view(), 0.032, 16, |_| true, clear);
        assert_eq!(glows.get(id).unwrap().fade, 255.0);
        // Occluded from now: falls by 1024 a second, to 1.
        glows.frame(&view(), 0.0, 16, |_| true, |_, _, _| true);
        assert!(glows.get(id).unwrap().occluded);
        glows.frame(&view(), 0.124, 16, |_| true, |_, _, _| true);
        assert!((glows.get(id).unwrap().fade - (255.0 - 1024.0 * 0.124)).abs() < 1e-3);
        // 254 / 1024 = 0.248 s in all.
        glows.frame(&view(), 0.125, 16, |_| true, |_, _, _| true);
        assert_eq!(glows.get(id).unwrap().fade, 1.0);
        // Beyond the far plane: not listed, fading out.
        glows.frame(&view(), 0.0, 16, |_| true, clear);
        glows.frame(&view(), 0.1, 16, |_| true, clear);
        assert!(glows.get(id).unwrap().fade > 255.0 - 1.0);
        glows.set_position(id, [0.0, 0.0, 150.0]);
        let sprites = glows.frame(&view(), 0.1, 16, |_| true, clear);
        assert!(sprites.is_empty());
        assert!((glows.get(id).unwrap().fade - (255.0 - 102.4)).abs() < 1e-3);
    }

    #[test]
    fn occlusion_tests_cycle_through_the_list() {
        // Section 5: at most min(N, L) glows a frame, each once, the
        // counter carrying on from frame to frame.
        let mut glows = Glows::default();
        let ids: Vec<GlowId> = (0..5)
            .map(|i| glows.add_level([0.0, 0.0, 10.0 + i as f32], 0.5, 0))
            .collect();
        let mut tested = Vec::new();
        glows.frame(
            &view(),
            0.0,
            3,
            |_| true,
            |_, _, l| {
                tested.push(l);
                false
            },
        );
        assert_eq!(tested, [10.0, 11.0, 12.0]);
        tested.clear();
        glows.frame(
            &view(),
            0.0,
            3,
            |_| true,
            |_, _, l| {
                tested.push(l);
                false
            },
        );
        assert_eq!(tested, [13.0, 14.0, 10.0]);
        tested.clear();
        glows.frame(
            &view(),
            0.0,
            32,
            |_| true,
            |_, _, l| {
                tested.push(l);
                false
            },
        );
        assert_eq!(tested.len(), 5);
        assert!(ids.iter().all(|&id| !glows.get(id).unwrap().occluded));
    }

    #[test]
    fn sprites_face_the_camera() {
        let mut glows = Glows::default();
        let id = glows.add_level([0.0, 0.0, 10.0], 2.0, 4);
        // Without a sector: never considered.
        assert!(
            glows
                .frame(&view(), 0.1, 16, |_| false, |_, _, _| false)
                .is_empty()
        );
        let mut glows = Glows::default();
        glows.add_level([0.0, 0.0, 10.0], 2.0, 4);
        let s = glows.frame(&view(), 0.0, 16, |_| true, |_, _, _| false);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].shader, 4);
        assert_eq!(
            s[0].corners,
            [
                [-2.0, -2.0, 10.0],
                [-2.0, 2.0, 10.0],
                [2.0, -2.0, 10.0],
                [2.0, 2.0, 10.0]
            ]
        );
        // a = (1 − D²/Z²) · f = 0.99 · 255.
        assert_eq!(s[0].b, (0.99f32 * 255.0) as u8);
        // Within 0.2 m in front of the camera: t = 0, not drawn.
        glows.add_level([0.5, 0.0, 0.1], 0.1, 4);
        let s = glows.frame(&view(), 0.0, 16, |_| true, |_, _, _| false);
        assert_eq!(s.len(), 1);
        // A directed glow seen from behind is not drawn.
        let mut glows = Glows::default();
        let id2 = glows.create(1);
        glows.set_on(id2, true);
        glows.set_position(id2, [0.0, 0.0, 10.0]);
        glows.set_direction(id2, [0.0, 0.0, 1.0]);
        for _ in 0..3 {
            assert!(
                glows
                    .frame(&view(), 0.1, 16, |_| true, |_, _, _| false)
                    .is_empty()
            );
        }
        glows.set_direction(id2, [0.0, 0.0, -1.0]);
        assert_eq!(
            glows
                .frame(&view(), 0.1, 16, |_| true, |_, _, _| false)
                .len(),
            1
        );
        let _ = id;
    }
}
