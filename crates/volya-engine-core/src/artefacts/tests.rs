// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use volya_engine_formats::particles::Library;

use super::*;
use crate::particles::fixture::muhi;

const SECTIONS: &str = "[af_medusa]\nclass = ARTEFACT\nparticles = tests\\muhi\n\
     lights_enabled = false\njump_height = .5\n\
     [af_night_star]\nclass = ARTEFACT\nparticles = tests\\muhi\nlights_enabled = true\n\
     trail_light_color = 0.6, 0.7, 0.5\ntrail_light_range = 3.0\njump_height = .2\n\
     [af_electra_flash]\nclass = SCRPTART\nparticles = tests\\muhi\nlights_enabled = true\n\
     trail_light_color = 0.6,0.7,0.9\ntrail_light_range = 2.5\njump_height = .1\n\
     [af_dummy_spring]\nclass = ARTEFACT\nlights_enabled = false\n\
     [af_fireball]\nclass = ARTEFACT\nparticles = tests\\muhi\nlights_enabled = true\n\
     trail_light_color = 0.9, 0.5, 0.5\ntrail_light_range = 3.0\n\
     [af_broken]\nclass = ARTEFACT\nparticles = tests\\muhi\n";

fn section(name: &str) -> io::Result<ArtefactSection> {
    let config = volya_engine_formats::ltx::parse(SECTIONS.as_bytes()).unwrap();
    let s = config.section(name).unwrap();
    let class = s.read("class").unwrap();
    ArtefactSection::read(s, Behaviour::of(class).unwrap())
}

fn effects() -> PlacedEffects {
    let library = Arc::new(Library::new(vec![muhi()], Vec::new()));
    PlacedEffects::new(library, &[], 0).unwrap()
}

/// `esc_af_medusa` as P10 8 gives it: *X* with row 3 at its position,
/// bone 0 of `artefact_blackdrip` at rest, its model's sphere.
fn medusa(parent: bool) -> ArtefactRecord {
    let mut transform = crate::particles::IDENTITY;
    transform[3] = [-244.8165, -13.1585, -33.6452, 1.0];
    ArtefactRecord {
        id: 3,
        name: "esc_af_medusa".into(),
        transform,
        parent,
        bone: [0.0005, 0.0390, -0.0001],
        sphere: ([0.0062, 0.1543, -0.0049], 0.23),
    }
}

#[test]
fn sections_read_their_keys() {
    // P10 1.2, 1.3.
    let m = section("af_medusa").unwrap();
    assert_eq!(m.behaviour, Behaviour::Jumping);
    assert_eq!(m.particles.as_deref(), Some("tests\\muhi"));
    assert_eq!(m.light, None);
    assert_eq!(m.jump_height, 0.5);
    let n = section("af_night_star").unwrap();
    assert_eq!(
        n.light,
        Some(TrailLight {
            rgb: [0.6, 0.7, 0.5],
            range: 3.0
        })
    );
    // `SCRPTART` does not read `jump_height`.
    let e = section("af_electra_flash").unwrap();
    assert_eq!(e.behaviour, Behaviour::Plain);
    assert_eq!(e.jump_height, 0.0);
    assert_eq!(e.light.unwrap().rgb, [0.6, 0.7, 0.9]);
    // Without `particles` no effect; without `jump_height` 0.
    assert_eq!(section("af_dummy_spring").unwrap().particles, None);
    assert_eq!(section("af_fireball").unwrap().jump_height, 0.0);
    // `lights_enabled` missing is fatal.
    assert!(section("af_broken").is_err());
    assert_eq!(Behaviour::of("Z_MBALD"), None);
}

#[test]
fn the_idle_start_matrix_angles_and_update_rotation() {
    // P10 4.2, 4.3 (M1).
    let s = start_matrix(UP);
    assert_eq!(
        s,
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    );
    let [h, p, b] = kept_angles(&s);
    assert_eq!((h, b), (0.0, 0.0));
    assert_eq!(p.to_bits(), 0x3FC9_0FDB);
    let u = update_rotation([h, p, b]);
    let c = f32::from_bits(0xB33B_BD2E);
    assert_eq!(p.cos().to_bits(), c.to_bits());
    assert_eq!(
        u,
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c, -1.0, 0.0],
            [0.0, 1.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    );
    // A source region's local point (0.1, 0.2, 0.3) lies at q + (0.1, 0.3,
    // −0.2).
    let q = [0.5, 1.0, 2.0];
    let mut m = s;
    m[3] = [q[0], q[1], q[2], 1.0];
    let at = place_point([0.1, 0.2, 0.3], &m);
    for (k, d) in [0.1, 0.3, -0.2].into_iter().enumerate() {
        assert!((at[k] - (q[k] + d)).abs() < 1e-6, "{at:?}");
    }
}

#[test]
fn scheduled_periods_grow_with_distance() {
    // P10 6.2 (M3).
    let got: Vec<u32> = [0.0, 10.0, 50.0, 100.0, 200.0, 400.0]
        .into_iter()
        .map(period)
        .collect();
    assert_eq!(got, [30, 54, 153, 277, 525, 525]);
}

#[test]
fn a_lying_artefact_shows_its_effect_at_bone_0() {
    // P10 3.1, 4.2, 8 (M2).
    let mut fx = effects();
    let mut list = Artefacts::default();
    list.create(&medusa(false), section("af_medusa").unwrap(), &mut fx)
        .unwrap();
    let a = list.get(3).unwrap();
    assert_eq!(a.state, State::Lying);
    assert!(!a.fast);
    let q = a.point();
    for (k, want) in [-244.816, -13.1195, -33.6453].into_iter().enumerate() {
        assert!((q[k] - want).abs() < 1e-3, "{q:?}");
    }
    let (c, r) = a.placed_sphere();
    assert!((c[1] - -13.0042).abs() < 1e-3 && r == 0.23, "{c:?}");
    let o = a.effects[0].object;
    let p = fx.get(o).unwrap();
    assert_eq!(p.origin, Origin::Artefact);
    assert!(p.instance.is_playing());
    assert!(p.registered().is_some());
    assert_eq!(p.updates(), 1, "the immediate 33 ms update");
    // It stays playing and in the list as it moves with the artefact.
    for clock in (16..2000).step_by(16) {
        fx.frame(16, q);
        list.frame(&Frame { clock, camera: q }, &mut fx);
    }
    let a = list.get(3).unwrap();
    assert_eq!(a.effects.len(), 1);
    assert!(fx.get(o).unwrap().instance.is_playing());
    // Fast within 50 m of its placed sphere (6.2).
    assert!(a.fast);
    let far = [q[0] + 60.0, q[1], q[2]];
    for clock in (2000..3000).step_by(16) {
        fx.frame(16, far);
        list.frame(&Frame { clock, camera: far }, &mut fx);
    }
    assert!(!list.get(3).unwrap().fast);
}

#[test]
fn taken_and_released_the_effect_and_light_follow_the_state() {
    // P10 4.6, 4.7, 5.2.
    let mut fx = effects();
    let mut lights = Lights::default();
    let mut list = Artefacts::default();
    let mut r = medusa(false);
    r.id = 9;
    list.create(&r, section("af_night_star").unwrap(), &mut fx)
        .unwrap();
    list.sync_lights(&mut lights);
    let a = list.get(9).unwrap();
    let id = a.record.unwrap();
    let l = lights.get(id).unwrap();
    assert!(l.active && l.shadow);
    assert_eq!(l.colour, [0.6, 0.7, 0.5, 1.0]);
    // M4: the first range differs from the default 8 and is taken.
    assert_eq!(l.range, 3.0);
    assert_eq!(l.position, a.position(), "at p, not at bone 0");
    let first = a.effects[0].object;

    list.take(9, &mut fx);
    list.sync_lights(&mut lights);
    let a = list.get(9).unwrap();
    assert_eq!(a.state, State::Carried);
    assert!(a.effects.is_empty());
    assert!(!lights.get(id).unwrap().active);
    fx.frame(16, [0.0; 3]);
    assert!(fx.get(first).is_none(), "deleted at the next frame's start");

    let mut at = r.transform;
    at[3] = [1.0, 2.0, 3.0, 1.0];
    list.release(9, at, &mut fx).unwrap();
    list.sync_lights(&mut lights);
    let a = list.get(9).unwrap();
    assert_eq!(a.state, State::Lying);
    let o = a.effects[0].object;
    assert!(fx.get(o).unwrap().instance.is_playing());
    let l = lights.get(id).unwrap();
    assert!(l.active);
    assert_eq!(l.position, [1.0, 2.0, 3.0]);

    // Destroyed: its effects go and its record is released (3.2).
    list.destroy(9, &mut fx);
    list.sync_lights(&mut lights);
    assert!(list.get(9).is_none());
    assert!(lights.get(id).is_none());
    fx.frame(16, [0.0; 3]);
    assert!(fx.get(o).is_none());
}

#[test]
fn without_lights_enabled_or_effect_nothing_is_on() {
    // P10 4.8, 5.2.
    let mut fx = effects();
    let mut lights = Lights::default();
    let mut list = Artefacts::default();
    list.create(&medusa(false), section("af_dummy_spring").unwrap(), &mut fx)
        .unwrap();
    list.sync_lights(&mut lights);
    let a = list.get(3).unwrap();
    assert!(a.effects.is_empty());
    assert!(!lights.get(a.record.unwrap()).unwrap().active);
    assert_eq!(fx.effects().count(), 0);
}

#[test]
fn created_with_a_parent_it_is_carried() {
    // Believed (P10 10): started, then taken at once.
    let mut fx = effects();
    let mut list = Artefacts::default();
    list.create(&medusa(true), section("af_night_star").unwrap(), &mut fx)
        .unwrap();
    let a = list.get(3).unwrap();
    assert_eq!(a.state, State::Carried);
    assert!(a.effects.is_empty() && !a.light_on);
}

#[test]
fn an_unknown_effect_is_an_error() {
    let mut fx = PlacedEffects::new(Arc::new(Library::default()), &[], 0).unwrap();
    let mut list = Artefacts::default();
    assert!(
        list.create(&medusa(false), section("af_medusa").unwrap(), &mut fx)
            .is_err()
    );
}
