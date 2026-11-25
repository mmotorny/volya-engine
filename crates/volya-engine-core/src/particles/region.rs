// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Regions in use (particle-lifecycle.md section 4): points drawn from a
//! region (4.1), positions tested against one (4.2), and a region placed by
//! a matrix (4.3).

use volya_engine_formats::particles::{Region, region_kind as kind};

use super::{FLT_MIN, Matrix, Random, Vec3, add, dot, place_direction, place_point, scale, sub};

/// A point drawn from `region` (P2 4.1), each U and θ fresh, in the order
/// written there.
pub fn generate(region: &Region, random: &mut Random) -> Vec3 {
    let Region {
        p1, p2, u, v, r, ..
    } = *region;
    match region.kind {
        kind::POINT | kind::PLANE => p1,
        kind::LINE => add(p1, scale(p2, random.u())),
        kind::TRIANGLE => {
            let (mut a, mut b) = (random.u(), random.u());
            if a + b >= 1.0 {
                a = 1.0 - a;
                b = 1.0 - b;
            }
            add(add(p1, scale(u, a)), scale(v, b))
        }
        kind::BOX => [0, 1, 2].map(|i| p1[i] + random.u() * (p2[i] - p1[i])),
        kind::SPHERE => {
            // The first U gives z, the third x.
            let z = random.u() - 0.5;
            let y = random.u() - 0.5;
            let x = random.u() - 0.5;
            let mut d = [x, y, z];
            let l2 = dot(d, d);
            if l2 > FLT_MIN {
                let l = l2.sqrt();
                d = d.map(|c| c / l);
            }
            let l = if r[0] == r[1] {
                r[0]
            } else {
                r[1] + random.u() * (r[0] - r[1])
            };
            add(p1, scale(d, l))
        }
        kind::CYLINDER | kind::CONE => {
            let h = random.u();
            let theta = random.angle();
            let mut l = r[1] + random.u() * (r[0] - r[1]);
            if region.kind == kind::CONE {
                l *= h;
            }
            let base = add(p1, scale(p2, h));
            add(
                add(base, scale(u, l * theta.cos())),
                scale(v, l * theta.sin()),
            )
        }
        kind::GAUSSIAN => {
            let x = random.normal(r[0]);
            let y = random.normal(r[0]);
            let z = random.normal(r[0]);
            add(p1, [x, y, z])
        }
        kind::DISC => {
            let theta = random.angle();
            let l = r[1] + random.u() * (r[0] - r[1]);
            add(
                add(p1, scale(u, l * theta.cos())),
                scale(v, l * theta.sin()),
            )
        }
        kind::RECTANGLE => {
            let a = random.u();
            let b = random.u();
            add(add(p1, scale(u, b)), scale(v, a))
        }
        _ => [0.0; 3],
    }
}

/// Whether `x` is inside `region` (P2 4.2), reading the derived values as
/// they stand. The Gaussian cloud draws a fresh U per test.
pub fn within(region: &Region, x: Vec3, random: &mut Random) -> bool {
    let Region { p1, p2, r, .. } = *region;
    let d = sub(x, p1);
    match region.kind {
        kind::PLANE => dot(p2, x) >= -r[0],
        kind::BOX => (0..3).all(|i| p1[i] <= x[i] && x[i] <= p2[i]),
        kind::SPHERE => {
            let l2 = dot(d, d);
            r[3] <= l2 && l2 <= r[2]
        }
        kind::CYLINDER | kind::CONE => {
            let t = dot(d, p2) * r[3];
            let e = sub(d, scale(p2, t));
            let q = dot(e, e);
            if !(0.0..=1.0).contains(&t) {
                return false;
            }
            if region.kind == kind::CYLINDER {
                r[1] * r[1] <= q && q <= r[2]
            } else {
                let (inner, outer) = (r[1] * t, r[0] * t);
                inner * inner <= q && q <= outer * outer
            }
        }
        kind::GAUSSIAN => r[1] * (r[3] * dot(d, d)).exp() > random.u(),
        _ => false,
    }
}

/// `region` placed by `m` (P2 4.3): fields not listed there keep their
/// stored values; derived values are not recomputed, directions not
/// renormalised. A kind above 10 is left as stored (our choice: the spec
/// leaves it unestablished, believed a crash).
pub fn place(region: &Region, m: &Matrix) -> Region {
    let mut out = *region;
    match region.kind {
        kind::POINT | kind::SPHERE | kind::GAUSSIAN => {
            out.p1 = place_point(region.p1, m);
        }
        kind::LINE => {
            out.p1 = place_point(region.p1, m);
            out.p2 = place_direction(region.p2, m);
        }
        kind::TRIANGLE | kind::CYLINDER | kind::CONE | kind::DISC | kind::RECTANGLE => {
            out.p1 = place_point(region.p1, m);
            out.p2 = place_direction(region.p2, m);
            out.u = place_direction(region.u, m);
            out.v = place_direction(region.v, m);
        }
        kind::PLANE => {
            out.p1 = place_point(region.p1, m);
            out.p2 = place_direction(region.p2, m);
            out.r[0] = -dot(out.p1, out.p2);
        }
        kind::BOX => {
            let corner = place_point(region.p1, m);
            let (mut lo, mut hi) = (corner, corner);
            let extent = sub(region.p2, region.p1);
            for (i, row) in m.iter().take(3).enumerate() {
                for j in 0..3 {
                    let t = row[j] * extent[i];
                    if t.is_sign_negative() {
                        lo[j] += t;
                    } else {
                        hi[j] += t;
                    }
                }
            }
            out.p1 = lo;
            out.p2 = hi;
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::without_translation;
    use super::*;

    fn close(a: Vec3, b: Vec3, tolerance: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() <= tolerance)
    }

    /// *M* of P2 9.2: 90° about y, translation (10, 2, −3).
    pub(crate) const M: Matrix = [
        [0.0, 0.0, -1.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [10.0, 2.0, -3.0, 1.0],
    ];

    fn region(kind: u32) -> Region {
        Region {
            kind,
            ..Region::default()
        }
    }

    #[test]
    fn a_box_is_placed_by_its_corners() {
        // P2 9.2.
        let b = Region {
            p1: [-1.0, 0.0, -2.0],
            p2: [3.0, 1.0, 2.0],
            ..region(kind::BOX)
        };
        let placed = place(&b, &M);
        assert_eq!(placed.p1, [8.0, 2.0, -6.0]);
        assert_eq!(placed.p2, [12.0, 3.0, -2.0]);
        let (c, s) = (
            std::f32::consts::FRAC_PI_4.cos(),
            std::f32::consts::FRAC_PI_4.sin(),
        );
        let rot = [
            [c, 0.0, -s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let placed = place(&b, &rot);
        assert!(close(placed.p1, [-2.12132, 0.0, -3.535534], 1e-5));
        assert!(close(placed.p2, [3.535534, 1.0, 2.12132], 1e-5));
    }

    #[test]
    fn a_plane_takes_its_offset_from_the_placed_values() {
        let p = Region {
            p1: [0.0, 1.0, 0.0],
            p2: [1.0, 0.0, 0.0],
            r: [-1.0, 0.0, 0.0, 0.0],
            ..region(kind::PLANE)
        };
        let placed = place(&p, &M);
        assert_eq!(placed.p1, [10.0, 3.0, -3.0]);
        assert_eq!(placed.p2, [0.0, 0.0, -1.0]);
        assert_eq!(placed.r[0], -3.0);
        // Without translation the points only turn.
        let turned = place(&p, &without_translation(&M));
        assert_eq!(turned.p1, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn containment_follows_p2s_vectors() {
        // P2 9.2.
        let mut random = Random::new(1);
        let sphere = Region {
            r: [0.3, 0.0, 0.09, 0.0],
            ..region(kind::SPHERE)
        };
        assert!(within(&sphere, [0.0, 0.0, 0.0], &mut random));
        assert!(within(&sphere, [0.3, 0.0, 0.0], &mut random));
        assert!(!within(&sphere, [0.31, 0.0, 0.0], &mut random));
        // `tests\muhi`'s velocity cone (P1 8).
        let cone = Region {
            kind: kind::CONE,
            p2: [0.0, 0.1, 0.0],
            u: [1.0, 0.0, 0.0],
            v: [0.0, 0.0, -1.0],
            r: [0.3, 0.0, 0.09, 99.999_99],
            ..Region::default()
        };
        for x in [[0.0, 0.05, 0.0], [0.14, 0.05, 0.0], [0.0, 0.0, 0.0]] {
            assert!(within(&cone, x, &mut random), "{x:?}");
        }
        for x in [[0.16, 0.05, 0.0], [0.0, 0.11, 0.0]] {
            assert!(!within(&cone, x, &mut random), "{x:?}");
        }
        assert_eq!(random.uniform.state, 1, "no draws");
    }

    #[test]
    fn kinds_that_cannot_contain_never_do() {
        let mut random = Random::new(1);
        for k in [0, 1, 2, 9, 10, 11] {
            assert!(!within(&region(k), [0.0; 3], &mut random));
        }
        let b = Region {
            p1: [0.0; 3],
            p2: [1.0; 3],
            ..region(kind::BOX)
        };
        assert!(within(&b, [1.0, 0.0, 0.5], &mut random));
        assert!(!within(&b, [1.0, -0.01, 0.5], &mut random));
    }

    #[test]
    fn a_gaussian_cloud_answers_at_random() {
        let sigma = 0.5f32;
        let cloud = Region {
            kind: kind::GAUSSIAN,
            r: [sigma, 0.398_942_3 / sigma, 0.0, -0.5 / (sigma * sigma)],
            ..Region::default()
        };
        let mut random = Random::new(3);
        let inside = (0..1000)
            .filter(|_| within(&cloud, [0.0; 3], &mut random))
            .count();
        // r₂ = 0.798 at the centre.
        assert!((700..900).contains(&inside), "{inside}");
    }

    #[test]
    fn points_are_drawn_in_the_order_written() {
        let line = Region {
            p1: [1.0, 1.0, 1.0],
            p2: [2.0, 0.0, -2.0],
            ..region(kind::LINE)
        };
        let mut random = Random::new(1);
        let u = 41.0 / 32767.0;
        assert!(close(
            generate(&line, &mut random),
            [1.0 + 2.0 * u, 1.0, 1.0 - 2.0 * u],
            1e-6
        ));
        // A sphere with r₁ = r₂ takes three draws, otherwise four.
        let mut sphere = Region {
            r: [1.0, 1.0, 1.0, 1.0],
            ..region(kind::SPHERE)
        };
        let mut random = Random::new(1);
        let p = generate(&sphere, &mut random);
        assert!((dot(p, p) - 1.0).abs() < 1e-6);
        assert_eq!(random.uniform, {
            let mut g = super::super::random::Generator::new(1);
            (0..3).for_each(|_| {
                g.next_k();
            });
            g
        });
        sphere.r[1] = 0.0;
        let mut random = Random::new(1);
        generate(&sphere, &mut random);
        let mut g = super::super::random::Generator::new(1);
        (0..4).for_each(|_| {
            g.next_k();
        });
        assert_eq!(random.uniform, g);
        // Kinds above 10 give the origin.
        assert_eq!(generate(&region(11), &mut random), [0.0; 3]);
    }

    #[test]
    fn a_triangle_folds_points_back_inside() {
        let t = Region {
            u: [1.0, 0.0, 0.0],
            v: [0.0, 0.0, 1.0],
            ..region(kind::TRIANGLE)
        };
        let mut random = Random::new(99);
        for _ in 0..200 {
            let p = generate(&t, &mut random);
            assert!(p[0] >= 0.0 && p[2] >= 0.0 && p[0] + p[2] <= 1.0 + 1e-6);
        }
    }
}
