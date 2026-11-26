// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Glows (docs/behavior/glows.md): the level's (`level` chunk 7) and the
//! objects' (hanging lamps; searchlights and torches later), gathered and
//! faded each frame (section 3), tested for occlusion (section 5) and
//! turned into camera-facing sprites (section 6).
//!
//! Gathering takes the camera pass's sectors and frusta from the caller
//! ([`Glows::frame`]).

use crate::camera::Vec3;
use crate::collision::geometry::ray_triangle_distance;
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
    /// Its sector, none until the ray of `level.md` 9.1 first finds one.
    pub sector: Option<u16>,
    /// Marked for sector re-detection.
    sector_due: bool,
    /// The frame it was last tested for occlusion in.
    tested: Option<u64>,
    /// Its last static test (section 5.1).
    last: Option<StaticTest>,
}

/// A glow's remembered static occlusion test (section 5.1): the ray's
/// start, direction and length, the result and the triangle hit.
#[derive(Clone, Copy, Debug, PartialEq)]
struct StaticTest {
    from: Vec3,
    dir: Vec3,
    length: f32,
    hit: Option<[Vec3; 3]>,
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
    /// A frame's candidates and listed glows, kept between frames (no
    /// allocation in a running frame, docs/architecture/memory.md).
    candidates: Vec<(f32, usize)>,
    listed: Vec<usize>,
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
            sector: None,
            sector_due: true,
            tested: None,
            last: None,
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
            sector: None,
            sector_due: true,
            tested: None,
            last: None,
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
    /// `occluded(from, direction, length)` (the triangle of the static
    /// world the ray hits, if any), then the sprites of the listed glows.
    /// `sector` gives the sector the downward ray of `level.md` 9.1 finds
    /// from a point, if any; `reached` whether a sector was visited with a
    /// frustum a sphere (centre, radius) is not outside (steps 3, 4).
    pub fn frame(
        &mut self,
        view: &GlowView,
        dt: f32,
        per_frame: u32,
        sector: impl FnMut(Vec3) -> Option<u16>,
        reached: impl FnMut(u16, Vec3, f32) -> bool,
        occluded: impl FnMut(Vec3, Vec3, f32) -> Option<[Vec3; 3]>,
    ) -> Vec<GlowSprite> {
        let mut sprites = Vec::new();
        let tests = (sector, reached, occluded);
        self.frame_into(view, dt, per_frame, tests, &mut sprites);
        sprites
    }

    /// [`Glows::frame`] into `sprites`, cleared first; `tests` are its
    /// `sector`, `reached` and `occluded`.
    pub fn frame_into(
        &mut self,
        view: &GlowView,
        dt: f32,
        per_frame: u32,
        tests: (
            impl FnMut(Vec3) -> Option<u16>,
            impl FnMut(u16, Vec3, f32) -> bool,
            impl FnMut(Vec3, Vec3, f32) -> Option<[Vec3; 3]>,
        ),
        sprites: &mut Vec<GlowSprite>,
    ) {
        let (mut sector, mut reached, mut occluded) = tests;
        sprites.clear();
        self.frame += 1;
        let e = view.eye;
        let distance2 = |g: &Glow| {
            let v = sub(g.position, e);
            dot(v, v)
        };
        let mut listed = std::mem::take(&mut self.listed);
        listed.clear();
        if view.camera_in_sector {
            // Step 1: on and not outside the view, nearest first.
            let mut candidates = std::mem::take(&mut self.candidates);
            candidates.clear();
            candidates.extend(
                (self.glows.iter().enumerate())
                    .filter_map(|(i, g)| Some((i, g.as_ref()?)))
                    .filter(|(_, g)| g.on && view.frustum.meets(g.position, g.radius))
                    .map(|(i, g)| (distance2(g), i)),
            );
            candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
            let z2 = view.far * view.far;
            for &(d2, i) in &candidates {
                let g = self.glows[i].as_mut().expect("a candidate exists");
                // Step 2.
                if g.sector_due {
                    g.sector_due = false;
                    if let Some(s) = sector(g.position) {
                        g.sector = Some(s);
                    }
                }
                // Steps 3, 4.
                if !g.sector.is_some_and(|s| reached(s, g.position, g.radius)) {
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
            self.candidates = candidates;
        }
        if listed.is_empty() {
            self.listed = listed;
            return;
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
                let dir = to.map(|c| c / d);
                g.occluded = static_test(g, e, dir, d, &mut occluded);
            }
            self.k += 1;
        }
        // Section 6.
        let dl = dot(view.forward, view.forward).sqrt();
        let forward = view.forward.map(|c| c / dl);
        let z2 = view.far * view.far;
        for &i in &listed {
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
        self.listed = listed;
    }
}

/// Whether the static ray meets triangle `t` at 0 < t ≤ `length`
/// (section 5.1), from either side.
pub fn ray_meets(from: Vec3, dir: Vec3, length: f32, t: &[Vec3; 3]) -> bool {
    ray_triangle_distance(from, dir, t).is_some_and(|d| d > 0.0 && d <= length)
}

/// The static part of the occlusion ray (section 5.1) with the glow's
/// remembered test: reused when the ray is the same within the tolerances,
/// "hit" when the remembered triangle is hit again, otherwise the full
/// test, remembered.
fn static_test(
    g: &mut Glow,
    from: Vec3,
    dir: Vec3,
    length: f32,
    full: &mut impl FnMut(Vec3, Vec3, f32) -> Option<[Vec3; 3]>,
) -> bool {
    if let Some(last) = &g.last {
        let same = (0..3).all(|k| (from[k] - last.from[k]).abs() < 0.001)
            && (1.0 - dot(dir, last.dir)).abs() < 1e-5
            && (length - last.length).abs() < 1e-5;
        if same {
            return last.hit.is_some();
        }
        // Hit again at 0 < t < D.
        if let Some(t) = last.hit
            && ray_triangle_distance(from, dir, &t).is_some_and(|d| d > 0.0 && d < length)
        {
            return true;
        }
    }
    let hit = full(from, dir, length);
    g.last = Some(StaticTest {
        from,
        dir,
        length,
        hit,
    });
    hit.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A triangle the tests' rays report hit, away from them (so the
    /// remembered triangle is never hit again).
    const TRI: [Vec3; 3] = [[-1.0, -1.0, -50.0], [-1.0, 3.0, -50.0], [3.0, -1.0, -50.0]];

    /// A view from the origin along +z seeing everything.
    fn view() -> GlowView {
        view_at([0.0; 3])
    }

    /// The same from `eye`.
    fn view_at(eye: Vec3) -> GlowView {
        GlowView {
            eye,
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
        let clear = |_: Vec3, _: Vec3, _: f32| None;
        glows.frame(&view(), 0.031, 16, |_| Some(0), |_, _, _| true, clear);
        assert!((glows.get(id).unwrap().fade - (1.0 + 4096.0 * 0.031)).abs() < 1e-3);
        // 254 / 4096 = 0.0620 s in all.
        glows.frame(&view(), 0.032, 16, |_| Some(0), |_, _, _| true, clear);
        assert_eq!(glows.get(id).unwrap().fade, 255.0);
        // Occluded from now (seen from elsewhere, so the ray is tested
        // again): falls by 1024 a second, to 1.
        let moved = view_at([0.0, 0.0, 0.01]);
        glows.frame(
            &moved,
            0.0,
            16,
            |_| Some(0),
            |_, _, _| true,
            |_, _, _| Some(TRI),
        );
        assert!(glows.get(id).unwrap().occluded);
        glows.frame(
            &moved,
            0.124,
            16,
            |_| Some(0),
            |_, _, _| true,
            |_, _, _| Some(TRI),
        );
        assert!((glows.get(id).unwrap().fade - (255.0 - 1024.0 * 0.124)).abs() < 1e-3);
        // 254 / 1024 = 0.248 s in all.
        glows.frame(
            &moved,
            0.125,
            16,
            |_| Some(0),
            |_, _, _| true,
            |_, _, _| Some(TRI),
        );
        assert_eq!(glows.get(id).unwrap().fade, 1.0);
        // Beyond the far plane: not listed, fading out.
        glows.frame(&view(), 0.0, 16, |_| Some(0), |_, _, _| true, clear);
        glows.frame(&view(), 0.1, 16, |_| Some(0), |_, _, _| true, clear);
        assert!(glows.get(id).unwrap().fade > 255.0 - 1.0);
        glows.set_position(id, [0.0, 0.0, 150.0]);
        let sprites = glows.frame(&view(), 0.1, 16, |_| Some(0), |_, _, _| true, clear);
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
            |_| Some(0),
            |_, _, _| true,
            |_, _, l| {
                tested.push(l);
                None
            },
        );
        assert_eq!(tested, [10.0, 11.0, 12.0]);
        tested.clear();
        glows.frame(
            &view_at([0.0, 0.0, 0.01]),
            0.0,
            3,
            |_| Some(0),
            |_, _, _| true,
            |_, _, l| {
                tested.push(l);
                None
            },
        );
        // From elsewhere, so each ray is tested again; the lengths shrink.
        assert_eq!(tested, [12.99, 13.99, 9.99]);
        tested.clear();
        glows.frame(
            &view_at([0.0, 0.0, 0.02]),
            0.0,
            32,
            |_| Some(0),
            |_, _, _| true,
            |_, _, l| {
                tested.push(l);
                None
            },
        );
        assert_eq!(tested.len(), 5);
        assert!(ids.iter().all(|&id| !glows.get(id).unwrap().occluded));
    }

    #[test]
    fn the_static_test_is_remembered() {
        // Section 5.1: the same ray within the tolerances reuses the
        // result; the remembered triangle hit again is a hit.
        let wall = [[-1.0, -1.0, 5.0], [-1.0, 3.0, 5.0], [3.0, -1.0, 5.0]];
        let mut glows = Glows::default();
        let id = glows.add_level([0.0, 0.0, 10.0], 0.5, 0);
        let mut full = 0;
        let mut test = |glows: &mut Glows, eye: Vec3, hit: Option<[Vec3; 3]>| {
            glows.frame(
                &view_at(eye),
                0.0,
                16,
                |_| Some(0),
                |_, _, _| true,
                |_, _, _| {
                    full += 1;
                    hit
                },
            );
            glows.get(id).unwrap().occluded
        };
        assert!(test(&mut glows, [0.0; 3], Some(wall)));
        // The same ray: remembered, even if the world now says otherwise.
        assert!(test(&mut glows, [0.0005, 0.0, 0.0], None));
        // Another ray through the remembered triangle: a hit.
        assert!(test(&mut glows, [0.5, 0.0, 0.0], None));
        // One that misses it: the full test.
        assert!(!test(&mut glows, [0.0, 0.0, 6.0], None));
        assert_eq!(full, 2);
    }

    #[test]
    fn sprites_face_the_camera() {
        let mut glows = Glows::default();
        let id = glows.add_level([0.0, 0.0, 10.0], 2.0, 4);
        // Without a sector: never considered.
        assert!(
            glows
                .frame(&view(), 0.1, 16, |_| None, |_, _, _| true, |_, _, _| None)
                .is_empty()
        );
        let mut glows = Glows::default();
        glows.add_level([0.0, 0.0, 10.0], 2.0, 4);
        let s = glows.frame(
            &view(),
            0.0,
            16,
            |_| Some(0),
            |_, _, _| true,
            |_, _, _| None,
        );
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
        let s = glows.frame(
            &view(),
            0.0,
            16,
            |_| Some(0),
            |_, _, _| true,
            |_, _, _| None,
        );
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
                    .frame(
                        &view(),
                        0.1,
                        16,
                        |_| Some(0),
                        |_, _, _| true,
                        |_, _, _| None
                    )
                    .is_empty()
            );
        }
        glows.set_direction(id2, [0.0, 0.0, -1.0]);
        assert_eq!(
            glows
                .frame(
                    &view(),
                    0.1,
                    16,
                    |_| Some(0),
                    |_, _, _| true,
                    |_, _, _| None
                )
                .len(),
            1
        );
        let _ = id;
    }
}
