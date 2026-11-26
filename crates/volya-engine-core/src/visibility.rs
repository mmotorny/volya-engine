// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The camera pass's sectors and portals in the first renderer: the camera
//! sector (docs/formats/level.md 9.1, docs/behavior/visibility-scope.md 6),
//! the traversal through portals with its options
//! (level.md 9.2, docs/behavior/traversal-options.md), and the static
//! visuals of each visited sector (level.md 9.3, docs/formats/ogf.md 8).
//!
//! Frusta are sets of planes `(a, b, c, d)`, inside where
//! `a x + b y + c z + d ≥ 0`. The camera pass's occlusion tests
//! (docs/behavior/occlusion-tests.md 6) are [`Occlusion::portal_hidden`]
//! and [`Occlusion::visual_hidden`].

use std::io;

use crate::camera::{Mat4, Vec3};
use crate::collision::CollisionWorld;
use crate::collision::geometry::{box_intersects, cross, dot, ray_triangle_distance, sub};
use crate::occlusion::{Occlusion, View};
use crate::particles::random::Generator;
use volya_engine_formats::level::{Level, Thresholds};
use volya_engine_formats::ogf::{VisualType, coverage_factor};

/// The downward ray of level.md 9.1, metres.
pub const SECTOR_RAY: f32 = 500.0;
/// A portal is taken over a collision triangle unless the triangle is
/// nearer by more than this (level.md 9.1).
const PORTAL_PREFERENCE: f32 = 1e-5;
/// The camera sector is re-determined only after a move by more than this
/// on an axis (level.md 9.1).
const MOVED: f32 = 1e-7;
/// Half-size of the box around the camera that marks portals as touched
/// (level.md 9.2 rule 2).
const TOUCH: f32 = 0.002;

/// A plane `(a, b, c, d)`, inside where `a x + b y + c z + d ≥ 0`.
pub type Plane = [f32; 4];

fn distance(p: &Plane, v: Vec3) -> f32 {
    p[0] * v[0] + p[1] * v[1] + p[2] * v[2] + p[3]
}

/// A convex volume bounded by planes.
#[derive(Clone, Debug, PartialEq)]
pub struct Frustum {
    pub planes: Vec<Plane>,
}

impl Frustum {
    /// The camera pass's view frustum from the view-projection `m`
    /// (row-vector clip = (p, 1) · M, −w ≤ x, y ≤ w, z ≤ w): the left,
    /// right, bottom, top and far planes and no near plane
    /// (visibility-scope.md 4).
    pub fn view(m: &Mat4) -> Self {
        let col = |c: usize| [m[0][c], m[1][c], m[2][c], m[3][c]];
        let (x, y, z, w) = (col(0), col(1), col(2), col(3));
        let plane = |p: Plane| {
            let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-20);
            p.map(|v| v / l)
        };
        let add = |a: Plane, b: Plane, s: f32| std::array::from_fn(|i| a[i] + s * b[i]);
        Self {
            planes: vec![
                plane(add(w, x, 1.0)),
                plane(add(w, x, -1.0)),
                plane(add(w, y, 1.0)),
                plane(add(w, y, -1.0)),
                plane(add(w, z, -1.0)),
            ],
        }
    }

    /// Whether the sphere is not outside: for no plane does its centre lie
    /// outside by more than its radius.
    pub fn meets(&self, centre: Vec3, radius: f32) -> bool {
        self.planes.iter().all(|p| distance(p, centre) >= -radius)
    }

    /// The sphere-and-box test (occlusion-tests.md 7) of the sphere
    /// `centre`, `radius` and the box `min` … `max` against the planes in
    /// `planes` (a bit per plane): none when outside, else the planes that
    /// remain (none when wholly inside). Exact, no margin.
    pub fn sphere_box(
        &self,
        planes: u32,
        centre: Vec3,
        radius: f32,
        min: Vec3,
        max: Vec3,
    ) -> Option<u32> {
        let mut remain = planes;
        for (i, p) in self.planes.iter().enumerate() {
            if planes >> i & 1 == 0 {
                continue;
            }
            // The spec's plane, positive outside: these are negated.
            let n = [-p[0], -p[1], -p[2]];
            let s = -distance(p, centre);
            if s > radius {
                return None;
            }
            if s.abs() < radius {
                let value = |v: Vec3| n[0] * v[0] + n[1] * v[1] + n[2] * v[2] - p[3];
                // The corner of smallest (`lowest`) or largest value, by
                // the normal's sign bits.
                let corner = |lowest: bool| -> Vec3 {
                    std::array::from_fn(|k| {
                        if n[k].is_sign_negative() == lowest {
                            max[k]
                        } else {
                            min[k]
                        }
                    })
                };
                if value(corner(true)) > 0.0 {
                    return None;
                }
                if value(corner(false)) > 0.0 {
                    continue;
                }
            }
            remain &= !(1 << i);
        }
        Some(remain)
    }

    /// Every plane, as [`Frustum::sphere_box`] takes them.
    pub fn all(&self) -> u32 {
        (1u32 << self.planes.len()) - 1
    }

    /// The convex `polygon` clipped against every plane; fewer than three
    /// vertices left is empty (our choice: such a remainder has no area).
    pub fn clip(&self, polygon: &[Vec3]) -> Vec<Vec3> {
        let mut poly = polygon.to_vec();
        let mut next = Vec::with_capacity(poly.len() + 4);
        for p in &self.planes {
            next.clear();
            for (i, &a) in poly.iter().enumerate() {
                let b = poly[(i + 1) % poly.len()];
                let (da, db) = (distance(p, a), distance(p, b));
                if da >= 0.0 {
                    next.push(a);
                }
                if (da >= 0.0) != (db >= 0.0) {
                    let t = da / (da - db);
                    next.push(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t));
                }
            }
            std::mem::swap(&mut poly, &mut next);
            if poly.len() < 3 {
                return Vec::new();
            }
        }
        poly
    }

    /// The far plane `z ≤ w` of the row-vector matrix `m`
    /// (traversal-options.md 5 item 5).
    fn far(m: &Mat4) -> Plane {
        Self::view(m).planes[4]
    }

    /// The frustum through the clipped `polygon` seen from `eye`, with the
    /// far plane of the traversal's matrix `m` (traversal-options.md 5):
    /// the polygon, beyond six vertices replaced by its bounding rectangle,
    /// gives the edge planes through the eye, then the portal plane, then
    /// the far plane. The spec's planes are inside where negative; here
    /// they are negated.
    fn through(eye: Vec3, polygon: &[Vec3], m: &Mat4) -> Self {
        // Items 2 and 3: a zero normal becomes (0, 1, 0).
        let unit = |c: Vec3| {
            let len = dot(c, c).sqrt();
            if len > 0.0 {
                c.map(|v| v / len)
            } else {
                [0.0, 1.0, 0.0]
            }
        };
        // The spec's plane through `p` with normal `n`, negated.
        let plane = |n: Vec3, p: Vec3| [-n[0], -n[1], -n[2], dot(n, p)];
        let normal = |v: &[Vec3]| unit(cross(sub(v[0], v[1]), sub(v[0], v[2])));
        let mut poly = polygon.to_vec();
        // Item 1.
        if poly.len() > 6 {
            let n = normal(&poly);
            let r = if n[1].abs() > 0.99 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let u = unit(sub(r, n.map(|v| v * dot(r, n))));
            let w = cross(n, u);
            let o = poly[0];
            let (mut a0, mut a1, mut b0, mut b1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for &v in &poly {
                let (a, b) = (dot(sub(v, o), u), dot(sub(v, o), w));
                (a0, a1, b0, b1) = (a0.min(a), a1.max(a), b0.min(b), b1.max(b));
            }
            let at = |a: f32, b: f32| std::array::from_fn(|k| o[k] + a * u[k] + b * w[k]);
            // Counter-clockwise about n, as the polygon was.
            poly = vec![at(a0, b0), at(a1, b0), at(a1, b1), at(a0, b1)];
        }
        // Item 2.
        let mut n = normal(&poly);
        if dot(n, eye) - dot(n, poly[0]) < 0.0 {
            poly.reverse();
            n = normal(&poly);
        }
        let portal = plane(n, poly[0]);
        // Items 3 – 5.
        let mut planes = Vec::with_capacity(poly.len() + 2);
        for (i, &a) in poly.iter().enumerate() {
            let b = poly[(i + 1) % poly.len()];
            planes.push(plane(unit(cross(sub(eye, a), sub(eye, b))), eye));
        }
        planes.push(portal);
        planes.push(Self::far(m));
        Self { planes }
    }
}

/// A portal with the values the traversal derives (level.md 5).
#[derive(Clone, Debug, PartialEq)]
pub struct Portal {
    pub sector_a: u16,
    pub sector_b: u16,
    pub polygon: Vec<Vec3>,
    /// Plane normal and offset: sector A lies where `n·p + d > 0`.
    pub normal: Vec3,
    pub d: f32,
    pub centre: Vec3,
    pub radius: f32,
}

impl Portal {
    /// The sector other than `sector`.
    fn other(&self, sector: u16) -> u16 {
        if sector == self.sector_a {
            self.sector_b
        } else {
            self.sector_a
        }
    }

    fn side(&self, p: Vec3) -> f32 {
        dot(self.normal, p) + self.d
    }

    /// The triangles of the polygon (a fan).
    fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        let v = &self.polygon;
        (1..v.len().saturating_sub(1)).map(move |i| [v[0], v[i], v[i + 1]])
    }
}

/// What the traversal and the visuals' descent need of one static visual.
#[derive(Clone, Debug, PartialEq)]
struct Node {
    kind: VisualType,
    centre: Vec3,
    radius: f32,
    box_min: Vec3,
    box_max: Vec3,
    /// Billboard groups' coverage factor (ogf.md 7), else 1.
    coverage: f32,
    children: Vec<u32>,
}

/// A level's sectors, portals and static visual hierarchy, as the camera
/// pass reads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sectors {
    pub portals: Vec<Portal>,
    /// Each sector's portal list.
    pub sector_portals: Vec<Vec<u16>>,
    /// Each sector's root visual.
    pub roots: Vec<u32>,
    nodes: Vec<Node>,
}

/// Which of the traversal's options are on (traversal-options.md 1); the
/// screen-rectangle mode is used by no pass (4) and is not offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Bit 1: rule 6, the occlusion test of the clipped portal.
    pub occlusion: bool,
    /// Bit 2: rule 4's *T*\_min test.
    pub size: bool,
    /// Bit 8: the fade list and the *T*\_B test (only with `size`).
    pub fade: bool,
}

/// The camera pass's options (traversal-options.md 4).
pub const CAMERA_PASS: Options = Options {
    occlusion: true,
    size: true,
    fade: true,
};

/// The result of one traversal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Traversal {
    /// The sectors entered, in the order of their first entry.
    pub visited: Vec<u16>,
    /// By sector: every frustum it was entered with (empty when not
    /// visited).
    pub frusta: Vec<Vec<Frustum>>,
    /// The fade list: portals collected for the fade with their measure
    /// (traversal-options.md 2.2); empty without the fade option.
    pub fade: Vec<(u16, f32)>,
}

/// The camera pass's result for one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraFrame {
    pub traversal: Traversal,
    /// By visited sector (aligned with `traversal.visited`): the static
    /// visuals drawn from it, each visual once a frame. A mesh or tree is
    /// drawn; a billboard group listed here has its billboard drawn
    /// (ogf.md 8.2).
    pub visuals: Vec<Vec<u32>>,
}

/// The camera pass's working lists, kept from frame to frame with the
/// result they fill ([`Sectors::camera_pass_into`]), so a running frame
/// allocates none of them (docs/architecture/memory.md).
#[derive(Clone, Debug, Default)]
pub struct PassBuffers {
    passed: Vec<bool>,
    touched: Vec<bool>,
    drawn: Vec<bool>,
    stack: Vec<(u32, u32)>,
}

impl Sectors {
    /// The sectors, portals and visual hierarchy of `level`; fails on an
    /// invalid portal (level.md 5) or a reference out of range.
    pub fn new(level: &Level) -> io::Result<Self> {
        let invalid = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
        let sectors = level.sectors.len();
        let portals = level
            .portals
            .iter()
            .map(|p| {
                if usize::from(p.sector_a) >= sectors || usize::from(p.sector_b) >= sectors {
                    return Err(invalid("portal's sector out of range"));
                }
                let (normal, d) = p.plane()?;
                let (centre, radius) = p.sphere();
                Ok(Portal {
                    sector_a: p.sector_a,
                    sector_b: p.sector_b,
                    polygon: p.vertices.clone(),
                    normal,
                    d,
                    centre,
                    radius,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let visuals = level.visuals.len();
        for s in &level.sectors {
            if s.portals.iter().any(|&p| usize::from(p) >= portals.len()) {
                return Err(invalid("sector's portal out of range"));
            }
            if s.root as usize >= visuals {
                return Err(invalid("sector's root visual out of range"));
            }
        }
        let nodes = level
            .visuals
            .iter()
            .map(|v| {
                let h = &v.header;
                if v.children.iter().any(|&c| c as usize >= visuals) {
                    return Err(invalid("visual's child out of range"));
                }
                Ok(Node {
                    kind: h.kind,
                    centre: h.sphere_centre,
                    radius: h.sphere_radius,
                    box_min: h.box_min,
                    box_max: h.box_max,
                    coverage: if h.kind == VisualType::BillboardGroup {
                        coverage_factor(h)
                    } else {
                        1.0
                    },
                    children: v.children.clone(),
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            portals,
            sector_portals: level.sectors.iter().map(|s| s.portals.clone()).collect(),
            roots: level.sectors.iter().map(|s| s.root).collect(),
            nodes,
        })
    }

    /// The nearest portal the downward ray from `origin` meets within
    /// [`SECTOR_RAY`], with the sector it puts `origin` in (level.md 9.1).
    pub fn portal_below(&self, origin: Vec3) -> Option<(f32, u16)> {
        let down = [0.0, -1.0, 0.0];
        let mut best: Option<(f32, u16)> = None;
        for p in &self.portals {
            let hit = p
                .triangles()
                .filter_map(|t| ray_triangle_distance(origin, down, &t))
                .filter(|t| (0.0..=SECTOR_RAY).contains(t))
                .min_by(f32::total_cmp);
            if let Some(t) = hit
                && best.is_none_or(|(b, _)| t < b)
            {
                let sector = if p.side(origin) > 0.0 {
                    p.sector_a
                } else {
                    p.sector_b
                };
                best = Some((t, sector));
            }
        }
        best
    }

    /// The sector the downward ray from `origin` finds, from the nearest
    /// portal and the nearest collision triangle `triangle` gives (distance
    /// and sector), if any (level.md 9.1).
    pub fn sector_below(&self, origin: Vec3, triangle: Option<(f32, u16)>) -> Option<u16> {
        match (self.portal_below(origin), triangle) {
            (Some((p, s)), Some((t, _))) if t >= p - PORTAL_PREFERENCE => Some(s),
            (Some((_, s)), None) => Some(s),
            (_, Some((_, s))) => Some(s),
            (None, None) => None,
        }
    }

    /// Traverses from sector `start` with `frustum`, seen from `eye`, with
    /// `options` and the frame's `thresholds` (level.md 9.2,
    /// traversal-options.md 1 – 2), every portal passed giving its target
    /// a frustum built with the matrix `m` (traversal-options.md 5).
    /// `hidden` is the occlusion test of a clipped portal (rule 6), asked
    /// only with the occlusion option.
    #[allow(clippy::too_many_arguments)]
    pub fn traverse(
        &self,
        start: u16,
        frustum: Frustum,
        eye: Vec3,
        m: &Mat4,
        options: Options,
        thresholds: &Thresholds,
        hidden: impl FnMut(&[Vec3]) -> bool,
    ) -> Traversal {
        let mut out = Traversal::default();
        let mut buffers = PassBuffers::default();
        let mut hidden = hidden;
        self.traverse_into(
            (start, frustum, eye, m),
            options,
            thresholds,
            &mut hidden,
            &mut out,
            &mut buffers,
        );
        out
    }

    /// [`Sectors::traverse`] into `out`, its lists cleared first, with
    /// `buffers` (kept between frames): from `start` with the frustum,
    /// eye and matrix of `from`.
    pub fn traverse_into(
        &self,
        (start, frustum, eye, m): (u16, Frustum, Vec3, &Mat4),
        options: Options,
        thresholds: &Thresholds,
        hidden: &mut impl FnMut(&[Vec3]) -> bool,
        out: &mut Traversal,
        buffers: &mut PassBuffers,
    ) {
        // Portals the box around the eye meets are touched (rule 2).
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        buffers.touched.clear();
        buffers.touched.extend(self.portals.iter().map(|p| {
            p.triangles()
                .any(|t| box_intersects(eye, axes, [TOUCH; 3], &t))
        }));
        buffers.passed.clear();
        buffers.passed.resize(self.portals.len(), false);
        out.visited.clear();
        out.fade.clear();
        out.frusta.resize_with(self.sector_portals.len(), Vec::new);
        for f in &mut out.frusta {
            f.clear();
        }
        let mut walk = Walk {
            sectors: self,
            start,
            eye,
            options,
            thresholds,
            m,
            passed: &mut buffers.passed,
            touched: &mut buffers.touched,
            hidden,
            out,
        };
        walk.enter(start, frustum);
    }

    /// The static visuals of the visited sectors (level.md 9.3, ogf.md 8)
    /// seen from `eye`: each root descended once per frustum its sector
    /// was entered with, culled by the sphere-and-box test (none below a
    /// node wholly inside) and then by `hidden` (the occlusion test of a
    /// visual, given its index and box; occlusion-tests.md 6), and each
    /// visual drawn once.
    pub fn static_visuals(
        &self,
        traversal: &Traversal,
        eye: Vec3,
        thresholds: &Thresholds,
        mut hidden: impl FnMut(u32, Vec3, Vec3) -> bool,
    ) -> Vec<Vec<u32>> {
        let mut out = Vec::new();
        let mut buffers = PassBuffers::default();
        self.static_visuals_into(
            traversal,
            eye,
            thresholds,
            &mut hidden,
            &mut out,
            &mut buffers,
        );
        out
    }

    /// [`Sectors::static_visuals`] into `out` (one list per visited
    /// sector, each cleared first), with `buffers` (kept between frames).
    pub fn static_visuals_into(
        &self,
        traversal: &Traversal,
        eye: Vec3,
        thresholds: &Thresholds,
        hidden: &mut impl FnMut(u32, Vec3, Vec3) -> bool,
        out: &mut Vec<Vec<u32>>,
        buffers: &mut PassBuffers,
    ) {
        let PassBuffers { drawn, stack, .. } = buffers;
        drawn.clear();
        drawn.resize(self.nodes.len(), false);
        stack.clear();
        out.resize_with(traversal.visited.len(), Vec::new);
        out.truncate(traversal.visited.len());
        for (&s, out) in traversal.visited.iter().zip(out.iter_mut()) {
            out.clear();
            for f in &traversal.frusta[usize::from(s)] {
                stack.push((self.roots[usize::from(s)], f.all()));
                while let Some((i, planes)) = stack.pop() {
                    let n = &self.nodes[i as usize];
                    let planes = match planes {
                        0 => 0,
                        p => match f.sphere_box(p, n.centre, n.radius, n.box_min, n.box_max) {
                            Some(p) => p,
                            None => continue,
                        },
                    };
                    if hidden(i, n.box_min, n.box_max) {
                        continue;
                    }
                    let children = n.children.iter().rev().map(|&c| (c, planes));
                    let d = sub(n.centre, eye);
                    let sigma = n.radius / (dot(d, d) + 1e-5) * n.coverage;
                    let mut draw = |out: &mut Vec<u32>| {
                        if !std::mem::replace(&mut drawn[i as usize], true) {
                            out.push(i);
                        }
                    };
                    match n.kind {
                        VisualType::Group => stack.extend(children),
                        VisualType::BillboardGroup => {
                            if sigma < thresholds.t_min {
                                continue;
                            }
                            if sigma < thresholds.t_a {
                                draw(out);
                            }
                            if sigma > thresholds.t_b {
                                stack.extend(children);
                            }
                        }
                        _ => {
                            if sigma > thresholds.t_min {
                                draw(out);
                            }
                        }
                    }
                }
            }
        }
    }

    /// The camera pass for one frame (visibility-scope.md 1): the
    /// occlusion grid drawn (occlusion-grid.md 2), then from the camera
    /// sector `sector` with the view frustum of the view's matrix and eye,
    /// with the camera pass's options, portals and static visuals
    /// occlusion-tested (occlusion-tests.md 6), the tests' random counts
    /// from the shared generator `random`. Without a camera sector nothing
    /// is visited (6).
    pub fn camera_pass(
        &self,
        sector: Option<u16>,
        view: &View,
        thresholds: &Thresholds,
        occlusion: &mut Occlusion,
        random: &mut Generator,
    ) -> CameraFrame {
        let mut out = CameraFrame::default();
        let mut buffers = PassBuffers::default();
        let pass = (sector, view, thresholds);
        self.camera_pass_into(pass, occlusion, random, &mut out, &mut buffers);
        out
    }

    /// [`Sectors::camera_pass`] of the camera sector, view and thresholds
    /// of `pass` into `out`, with `buffers`, both kept between frames.
    pub fn camera_pass_into(
        &self,
        (sector, view, thresholds): (Option<u16>, &View, &Thresholds),
        occlusion: &mut Occlusion,
        random: &mut Generator,
        out: &mut CameraFrame,
        buffers: &mut PassBuffers,
    ) {
        let (m, eye) = (&view.view_projection, view.eye);
        occlusion.camera_pass(view, random);
        let Some(sector) = sector.filter(|&s| usize::from(s) < self.roots.len()) else {
            out.traversal.visited.clear();
            out.traversal.fade.clear();
            out.traversal.frusta.clear();
            out.visuals.clear();
            return;
        };
        self.traverse_into(
            (sector, Frustum::view(m), eye, m),
            CAMERA_PASS,
            thresholds,
            &mut |p| occlusion.portal_hidden(p, view, random),
            &mut out.traversal,
            buffers,
        );
        self.static_visuals_into(
            &out.traversal,
            eye,
            thresholds,
            &mut |i, a, b| occlusion.visual_hidden(i, a, b, view, random),
            &mut out.visuals,
            buffers,
        );
    }

    /// The number of static visuals (the size of [`Occlusion`]'s cache).
    pub fn visual_count(&self) -> usize {
        self.nodes.len()
    }

    /// The leaf visuals of sector `sector` a projected-light cell draws
    /// (object-projected-light.md 4.3), into `out` (cleared first): from
    /// the sector's root alone (no portals), the root taken without a box
    /// test, a group's or billboard group's children whose box meets the
    /// box `min` … `max` (closed intervals) processed, every other visual
    /// listed without testing its own box; billboards are never listed.
    pub fn cell_visuals(&self, sector: u16, min: Vec3, max: Vec3, out: &mut Vec<u32>) {
        out.clear();
        let Some(&root) = self.roots.get(usize::from(sector)) else {
            return;
        };
        let meets = |n: &Node| (0..3).all(|i| n.box_min[i] <= max[i] && n.box_max[i] >= min[i]);
        let mut stack = vec![root];
        while let Some(i) = stack.pop() {
            let Some(n) = self.nodes.get(i as usize) else {
                continue;
            };
            match n.kind {
                VisualType::Group | VisualType::BillboardGroup => {
                    stack.extend(
                        n.children
                            .iter()
                            .rev()
                            .filter(|&&c| self.nodes.get(c as usize).is_some_and(meets)),
                    );
                }
                _ => out.push(i),
            }
        }
    }
}

/// One traversal in progress.
struct Walk<'a, H> {
    sectors: &'a Sectors,
    start: u16,
    eye: Vec3,
    options: Options,
    thresholds: &'a Thresholds,
    m: &'a Mat4,
    passed: &'a mut Vec<bool>,
    touched: &'a mut Vec<bool>,
    hidden: &'a mut H,
    out: &'a mut Traversal,
}

impl<H: FnMut(&[Vec3]) -> bool> Walk<'_, H> {
    /// Enters `sector` with `frustum` and goes on through its portals by
    /// rules 1 – 6 of level.md 9.2.
    fn enter(&mut self, sector: u16, frustum: Frustum) {
        let s = usize::from(sector);
        // The lists are emptied at a sector's first entry in a traversal
        // (traversal-options.md 3, as disputed in review).
        if self.out.frusta[s].is_empty() {
            self.out.visited.push(sector);
        }
        self.out.frusta[s].push(frustum.clone());
        for &p in &self.sectors.sector_portals[s] {
            let portal = &self.sectors.portals[usize::from(p)];
            let p = usize::from(p);
            // Rule 1.
            if self.passed[p] {
                continue;
            }
            // Rule 2, its skip against the start sector
            // (traversal-options.md 2.4).
            let touched = self.touched[p];
            let target = if touched {
                portal.other(sector)
            } else if portal.side(self.eye) <= 0.0 {
                portal.sector_a
            } else {
                portal.sector_b
            };
            if !touched && (target == sector || target == self.start) {
                continue;
            }
            // Rule 3.
            if !frustum.meets(portal.centre, portal.radius) {
                continue;
            }
            // Rule 4 (traversal-options.md 2.1, 2.2).
            if self.options.size {
                let m = self.measure(portal);
                if m < self.thresholds.t_min {
                    continue;
                }
                if self.options.fade {
                    if m < self.thresholds.t_a {
                        self.out.fade.push((p as u16, m));
                    }
                    if m < self.thresholds.t_b {
                        continue;
                    }
                }
            }
            // Rule 5.
            let clipped = frustum.clip(&portal.polygon);
            if clipped.is_empty() {
                continue;
            }
            // Rule 6 (traversal-options.md 2.3).
            if self.options.occlusion && (self.hidden)(&clipped) {
                continue;
            }
            self.passed[p] = true;
            self.touched[p] = false;
            // Touched or not (traversal-options.md 5).
            let narrowed = Frustum::through(self.eye, &clipped, self.m);
            self.enter(target, narrowed);
        }
    }

    /// The portal's screen measure `m = r² / D² × |cos|` (rule 4); a portal
    /// centred on the eye is as large as can be (our choice).
    fn measure(&self, portal: &Portal) -> f32 {
        let v = sub(portal.centre, self.eye);
        let d2 = dot(v, v);
        if d2.is_nan() || d2 <= 0.0 {
            return f32::INFINITY;
        }
        let cos = (dot(portal.normal, v) / d2.sqrt()).abs();
        portal.radius * portal.radius / d2 * cos
    }
}

/// The camera sector (level.md 9.1): none after a level loads, kept when
/// the ray finds nothing, re-determined only when the camera moved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraSector {
    sector: Option<u16>,
    at: Option<Vec3>,
}

impl CameraSector {
    /// The sector, none until a ray first finds one.
    pub fn get(&self) -> Option<u16> {
        self.sector
    }

    /// The sector with the camera at `eye`: re-determined from `sectors`
    /// and `collision` when it moved by more than 10⁻⁷ on an axis.
    pub fn update(
        &mut self,
        eye: Vec3,
        sectors: &Sectors,
        collision: &CollisionWorld,
    ) -> Option<u16> {
        let moved = self
            .at
            .is_none_or(|c| (0..3).any(|k| (c[k] - eye[k]).abs() > MOVED));
        if moved {
            self.at = Some(eye);
            if let Some(s) = sectors.sector_below(eye, triangle_below(collision, eye)) {
                self.sector = Some(s);
            }
        }
        self.sector
    }
}

/// The nearest collision triangle the downward ray from `origin` meets
/// within [`SECTOR_RAY`] (either side), its distance and sector.
pub fn triangle_below(collision: &CollisionWorld, origin: Vec3) -> Option<(f32, u16)> {
    let down = [0.0, -1.0, 0.0];
    collision
        .ray_nearest(origin, down, SECTOR_RAY, |t| {
            ray_triangle_distance(origin, down, &collision.corners(t))
                .filter(|d| (0.0..=SECTOR_RAY).contains(d))
        })
        .map(|(d, t)| (d, collision.sector(t)))
}

#[cfg(test)]
mod tests;
