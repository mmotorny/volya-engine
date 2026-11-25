// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The synthetic test vectors of particle-collision.md section 7.2 and the
//! rules of sections 2 – 5 they rest on.

use volya_engine_formats::particles::Collision;

use super::super::fixture::effect;
use super::super::{EffectInstance, Random};
use super::*;
use crate::collision::test_world::{Builder, materials};
use crate::collision::{Cform, CformTriangle};

/// The one triangle of the synthetic vectors, front normal (0, 1, 0).
fn triangle_world() -> CollisionWorld {
    let cform = Cform {
        bbox_min: [-50.0, 0.0, -50.0],
        bbox_max: [50.0, 0.0, 50.0],
        vertices: vec![[-50.0, 0.0, -50.0], [50.0, 0.0, 50.0], [50.0, 0.0, -50.0]],
        triangles: vec![CformTriangle {
            vertices: [0, 1, 2],
            attributes: 1,
        }],
    };
    CollisionWorld::new(cform, materials()).unwrap()
}

fn values(tangential: f32, resilience: f32, cutoff: f32) -> Collision {
    Collision {
        tangential,
        resilience,
        cutoff,
    }
}

/// A particle at **x**ₚ with velocity **w**, **x** = **x**ₚ + **w** Δ.
fn particle(previous: Vec3, velocity: Vec3) -> Particle {
    Particle {
        position: add(previous, scale(velocity, STEP)),
        previous,
        velocity,
        ..Particle::default()
    }
}

fn close(a: Vec3, b: Vec3) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 1e-4 * b[i].abs().max(1.0))
}

/// One particle's collision step against the triangle.
fn one(flags: u32, c: Collision, p: Particle) -> (usize, Particle) {
    let world = triangle_world();
    let mut ps = [p];
    let n = step(&mut ps, 1, flags, &c, Some(&world), None);
    (n, ps[0])
}

#[test]
fn s1_bounces_and_moves_a_full_step_from_the_previous_position() {
    let (n, p) = one(
        0x10000,
        values(1.0, 0.5, 0.0),
        particle([0.0, 0.5, 0.0], [3.0, -30.0, 0.0]),
    );
    assert_eq!(n, 1);
    assert!(close(p.velocity, [3.0, 15.0, 0.0]), "{p:?}");
    assert!(close(p.position, [0.099, 0.995, 0.0]), "{p:?}");
    assert_eq!(p.previous, [0.0, 0.5, 0.0]);
}

#[test]
fn s2_scales_the_tangential_part_only_above_the_cutoff() {
    for (cutoff, w) in [
        (0.0, [1.5, 15.0, 0.0]),
        (10.0, [3.0, 15.0, 0.0]),
        (9.0, [3.0, 15.0, 0.0]),
    ] {
        let (_, p) = one(
            0x10000,
            values(0.5, 0.5, cutoff),
            particle([0.0, 0.5, 0.0], [3.0, -30.0, 0.0]),
        );
        assert!(close(p.velocity, w), "cutoff {cutoff}: {p:?}");
    }
}

#[test]
fn s2d_without_resilience_the_particle_slides() {
    let (_, p) = one(
        0x10000,
        values(1.0, 0.0, 0.0),
        particle([0.0, 0.5, 0.0], [3.0, -30.0, 0.0]),
    );
    assert!(close(p.velocity, [3.0, 0.0, 0.0]), "{p:?}");
    assert!(close(p.position, [0.099, 0.5, 0.0]), "{p:?}");
}

#[test]
fn s3_a_short_segment_puts_the_particle_back() {
    let (n, p) = one(
        0x10000,
        values(1.0, 0.5, 0.0),
        particle([0.0, 0.5, 0.0], [0.0, -0.0001, 0.0]),
    );
    assert_eq!(n, 1);
    assert_eq!(p.position, [0.0, 0.5, 0.0]);
    assert_eq!(p.velocity, [0.0, -0.0001, 0.0]);
}

#[test]
fn a_segment_that_is_not_a_number_puts_the_particle_back() {
    let mut q = particle([0.0, 0.5, 0.0], [0.0, -30.0, 0.0]);
    q.position[1] = f32::NAN;
    let (_, p) = one(0x10000, values(1.0, 0.5, 0.0), q);
    assert_eq!(p.position, [0.0, 0.5, 0.0]);
}

#[test]
fn s4_a_triangle_is_not_hit_from_its_back() {
    let q = particle([0.0, -0.5, 0.0], [0.0, 30.0, 0.0]);
    let (_, p) = one(0x10000, values(1.0, 0.5, 0.0), q);
    assert_eq!(p, q);
    assert!(close(p.position, [0.0, 0.49, 0.0]));
}

#[test]
fn s5_a_removal_moves_the_last_particle_into_place() {
    let world = triangle_world();
    let a = particle([1.0, 2.0, 1.0], [0.0, 1.0, 0.0]);
    let b = particle([0.0, 0.5, 0.0], [0.0, -30.0, 0.0]);
    let c = particle([-1.0, 2.0, -1.0], [0.0, 2.0, 0.0]);
    let mut ps = [a, b, c];
    let mut notices = Vec::new();
    let n = step(
        &mut ps,
        3,
        0x30000,
        &values(1.0, 0.5, 0.0),
        Some(&world),
        Some(&mut notices),
    );
    assert_eq!(n, 2);
    assert_eq!(ps, [a, c, c]);
    // The death notice carries the removed particle's values unchanged.
    assert_eq!(
        notices,
        [Notice::Death {
            index: 1,
            position: b.position,
            previous: b.previous,
        }]
    );
}

#[test]
fn s6_the_last_particle_removed_stays_in_its_slot() {
    let world = triangle_world();
    let a = particle([1.0, 2.0, 1.0], [0.0, 1.0, 0.0]);
    let b = particle([0.0, 0.5, 0.0], [0.0, -30.0, 0.0]);
    let mut ps = [a, b];
    let n = step(
        &mut ps,
        2,
        0x30000,
        &values(1.0, 0.5, 0.0),
        Some(&world),
        None,
    );
    assert_eq!(n, 1);
    assert_eq!(ps, [a, b]);
    assert!(close(ps[1].position, [0.0, -0.49, 0.0]));
}

#[test]
fn s7_an_object_hit_bounces_off_up_and_the_bit_sets_the_mask() {
    // The object part of the ray is #388; its normal is (0, 1, 0).
    let w = bounce([4.0, -2.0, 3.0], [0.0, 1.0, 0.0], &values(1.0, 0.5, 0.0));
    assert_eq!(w, [4.0, 1.0, 3.0]);
    assert_eq!(mask(0x90000), MASK_OBJECTS);
    assert_eq!(mask(0x10000), MASK_STATIC);
}

#[test]
fn a_bounced_particle_is_tested_at_most_twice() {
    // A floor at y = 0 and a ceiling at y = 1: the first bounce sends the
    // particle up through the ceiling's front, the second down again, past
    // the floor's front; no third test.
    let mut b = Builder::default();
    b.floor([-10.0, 10.0], [-10.0, 10.0], 0.0, true);
    b.floor([-10.0, 10.0], [-10.0, 10.0], 1.0, false);
    let world = b.world();
    let mut ps = [particle([0.0, 0.5, 0.0], [0.0, -100.0, 0.0])];
    step(
        &mut ps,
        1,
        0x10000,
        &values(1.0, 1.0, 0.0),
        Some(&world),
        None,
    );
    let p = ps[0];
    assert!(close(p.velocity, [0.0, -100.0, 0.0]), "{p:?}");
    assert!(close(p.position, [0.0, -2.8, 0.0]), "{p:?}");
}

#[test]
fn the_nearest_hit_is_taken_across_cells() {
    // Two walls facing −x at x = 3 and x = 7, crossed by a long ray along
    // +x through several grid cells.
    let mut b = Builder::default();
    b.wall_x(7.0, [-1.0, 1.0], [-1.0, 1.0], true);
    b.wall_x(3.0, [-1.0, 1.0], [-1.0, 1.0], true);
    b.floor([-10.0, 10.0], [-10.0, 10.0], -5.0, true);
    let world = b.world();
    let hit = ray(&world, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 9.0, MASK_STATIC).unwrap();
    let Hit::Triangle { index, t } = hit else {
        panic!()
    };
    assert!((t - 3.0).abs() < 1e-5);
    assert_eq!(world.corners(index)[0][0], 3.0);
    // A hit at the segment's end is not a hit.
    assert_eq!(
        ray(&world, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 3.0, MASK_STATIC),
        None
    );
    // From behind the walls, along −x: their backs.
    assert_eq!(
        ray(&world, [9.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 9.0, MASK_STATIC),
        None
    );
}

#[test]
fn every_material_is_hit() {
    // Material id 2 is pass-through in the test library.
    let cform = Cform {
        bbox_min: [-50.0, 0.0, -50.0],
        bbox_max: [50.0, 0.0, 50.0],
        vertices: vec![[-50.0, 0.0, -50.0], [50.0, 0.0, 50.0], [50.0, 0.0, -50.0]],
        triangles: vec![CformTriangle {
            vertices: [0, 1, 2],
            attributes: 2,
        }],
    };
    let world = CollisionWorld::new(cform, materials()).unwrap();
    let mut ps = [particle([0.0, 0.5, 0.0], [3.0, -30.0, 0.0])];
    step(
        &mut ps,
        1,
        0x10000,
        &values(1.0, 0.5, 0.0),
        Some(&world),
        None,
    );
    assert!(close(ps[0].velocity, [3.0, 15.0, 0.0]));
}

/// An effect instance whose one particle is removed on a hit: the box
/// still spans the removed particle's slot, and a deferred stop ends one
/// step later (P5 5).
#[test]
fn removals_count_in_the_box_and_a_stop_ends_a_step_later() {
    let world = triangle_world();
    let mut e = EffectInstance::new(0, &effect("collide", 4, 0x30000, 0.0, Vec::new()));
    e.play();
    let mut p = particle([0.0, 0.5, 0.0], [0.0, -30.0, 0.0]);
    p.size = [0.0; 3];
    e.particles_mut().push(p);
    e.stop_deferred();
    let mut random = Random::default();
    e.update_in(33, &mut random, Some(&world));
    assert!(e.particles().is_empty());
    assert!(e.is_playing());
    assert!(close(e.bounds().min, [0.0, -0.49, 0.0]));
    assert!(close(e.bounds().max, [0.0, -0.49, 0.0]));
    e.update_in(33, &mut random, Some(&world));
    assert!(!e.is_playing());
}

#[test]
fn without_bit_0x10000_nothing_collides() {
    let world = triangle_world();
    let mut e = EffectInstance::new(0, &effect("plain", 4, 0xA0000, 0.0, Vec::new()));
    e.play();
    let p = particle([0.0, 0.5, 0.0], [0.0, -30.0, 0.0]);
    e.particles_mut().push(p);
    e.update_in(33, &mut Random::default(), Some(&world));
    assert_eq!(e.particles(), [p]);
}
