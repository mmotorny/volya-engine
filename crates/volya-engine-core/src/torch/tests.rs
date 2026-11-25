// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The examples and derived values of docs/behavior/torch.md.

use super::*;
use volya_engine_formats::ltx;

/// `torch_definition` as shipped (1.2).
const DEFINITION_TEXT: &[u8] = b"[torch_definition]\n\
range = 13\nrange_r2 = 13\ncolor = 1.0, 1.0, 1.0, 0.2\ncolor_r2 = 1.2, 1.2, 1.2, 0.2\n\
omni_range = 1.5\nomni_range_r2 = 0.75\nomni_color = 1.0, 1.0, 1.0, 0.1\n\
omni_color_r2 = 1.0, 1.0, 1.0, 0.1\ncolor_animator = empty\n\
spot_texture = internal\\internal_light_torch_r2\nspot_angle = 60\n\
glow_texture = glow\\glow_torch_r2\nglow_radius = 0.3\nguide_bone = lights_bone\n";

/// The keys of `device_torch` the torch reads (1.1).
const SECTION_TEXT: &[u8] = b"[device_torch]\nclass = TORCH_S\n\
light_trace_bone = cone_bone\nattach_bone_name = bip01_head\n\
attach_angle_offset = 0, -3.14, 1.6\nattach_position_offset = 0.115, 0.00, 0.07\n\
night_vision = on\nsnd_night_vision_on = weapons\\nv_start\n\
snd_night_vision_off = weapons\\nv_off\nsnd_night_vision_idle = weapons\\nv_loop\n\
snd_night_vision_broken = weapons\\nv_off\ndisabled_maps = ai_test,ai_test2, andy_test_\n";

fn definition() -> TorchDefinition {
    let c = ltx::parse(DEFINITION_TEXT).unwrap();
    TorchDefinition::read(c.section(DEFINITION).unwrap()).unwrap()
}

fn section() -> TorchSection {
    let c = ltx::parse(SECTION_TEXT).unwrap();
    TorchSection::read(c.section("device_torch").unwrap()).unwrap()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn the_definition_and_its_derived_values() {
    let d = definition();
    // The first renderer's keys (3 step 1).
    assert_eq!((d.range, d.colour), (13.0, [1.0, 1.0, 1.0, 0.2]));
    assert_eq!((d.omni_range, d.omni_colour), (1.5, [1.0, 1.0, 1.0, 0.1]));
    assert_eq!(d.glow_texture, "glow\\glow_torch_r2");
    assert_eq!((d.glow_radius, d.guide_bone.as_str()), (0.3, "lights_bone"));
    // Section 17: brightness 1.0, heading offset 0.030759 rad, cone
    // 1.0471976 rad.
    assert_eq!(d.brightness(), 1.0);
    assert!(close(d.heading_offset(), 0.030759, 1e-6));
    assert!(close(d.cone(), std::f32::consts::FRAC_PI_3, 1e-7));
}

#[test]
fn the_attachment_offset() {
    let s = section();
    let o = s.offset();
    // Section 6, to the 4 decimals given.
    let rows = [
        [-0.0292, 0.9996, -0.0016],
        [0.9996, 0.0292, 0.0],
        [0.0, -0.0016, -1.0],
        [0.115, 0.0, 0.07],
    ];
    for (r, want) in rows.iter().enumerate() {
        for k in 0..3 {
            assert!(close(o[r][k], want[k], 5e-5), "O[{r}][{k}] = {}", o[r][k]);
        }
    }
    // The rotation of LO 3.4 at the angles (o₁, o₀, o₂).
    let [a0, a1, a2] = s.attach_angle_offset;
    let r = volya_engine_formats::model::rotation([a1, a0, a2]);
    for (i, row) in r.iter().enumerate() {
        for k in 0..3 {
            assert!(close(o[i][k], row[k], 1e-6));
        }
    }
}

#[test]
fn the_smoothing_examples() {
    // 7.4 step 2: the target stepping from 0 to 1 rad, and to 0.1 rad, at
    // dt = 0.02.
    for (target, want) in [
        (1.0, [0.4764, 0.6264, 0.7363, 0.8168, 0.8758]),
        (0.1, [0.03674, 0.06365, 0.08337, 0.09782, 0.1]),
    ] {
        let mut h = 0.0;
        for w in want {
            h = smooth(h, target, 0.02);
            assert!(close(h, w, 5e-5), "toward {target}: {h} for {w}");
        }
    }
    // Across ±π the shorter way, staying within R of the target.
    let h = smooth(3.0, -3.0, 0.0);
    assert!(close(wrap(h - -3.0).abs(), 0.0, 0.3) && h.abs() <= PI);
    assert!(close(wrap(1.5 * TAU), PI, 1e-6));
    assert!(close(wrap(4.0), 4.0 - TAU, 1e-6));
}

#[test]
fn the_sound_variants() {
    let s = section();
    let nv = s.night_vision.as_ref().unwrap();
    for (sound, name) in [
        (NightVisionSound::On, "weapons\\nv_start"),
        (NightVisionSound::Off, "weapons\\nv_off"),
        (NightVisionSound::Idle, "weapons\\nv_loop"),
        (NightVisionSound::Broken, "weapons\\nv_off"),
    ] {
        assert_eq!(
            nv.variants(sound),
            [Variant {
                name: name.into(),
                volume: 1.0,
                delay: 0.0
            }]
        );
    }
    assert!(nv.disabled("AI_TEST") && nv.disabled("andy_test_"));
    assert!(!nv.disabled("l01_escape"));
    // Numbered lines without a gap, with volume and delay; none is fatal.
    let c = ltx::parse(b"[s]\nk = a, 0.5, 2\nk1 = b, 0.25\nk2 = c\nk4 = d\n").unwrap();
    let v = sound_variants(c.section("s").unwrap(), "k").unwrap();
    assert_eq!(v.len(), 3);
    assert_eq!((v[0].volume, v[0].delay, v[1].volume), (0.5, 2.0, 0.25));
    assert!(sound_variants(c.section("s").unwrap(), "z").is_err());
    // A torch without night vision has no sounds.
    let c = ltx::parse(b"[t]\nlight_trace_bone = a\nattach_bone_name = b\nattach_angle_offset = 0,0,0\nattach_position_offset = 0,0,0\n").unwrap();
    assert!(
        TorchSection::read(c.section("t").unwrap())
            .unwrap()
            .night_vision
            .is_none()
    );
}

fn spawned(stored: u8, owner: Option<Owner>, setting: bool) -> (Torch, Lights, Glows) {
    let mut lights = Lights::default();
    let mut glows = Glows::default();
    let t = Torch::spawn(
        &definition(),
        7,
        stored,
        owner,
        setting,
        true,
        &mut lights,
        &mut glows,
    );
    (t, lights, glows)
}

#[test]
fn spawning_and_switching() {
    // 3: off from a torch byte of 0, the records as section 3 makes them.
    let (mut t, mut lights, mut glows) = spawned(0, Some(Owner::Player), true);
    assert!(!t.on && !t.cone_visible);
    let main = lights.get(t.main).unwrap();
    assert!(!main.active && main.kind == LightKind::Spot && main.shadow);
    assert_eq!((main.range, main.colour), (13.0, [1.0, 1.0, 1.0, 0.2]));
    assert!(close(main.cone, std::f32::consts::FRAC_PI_3, 1e-7));
    // DL 2.3: the bounding radius 8.6667.
    assert!(close(main.radius, 8.6667, 5e-5), "{}", main.radius);
    assert_eq!(main.texture, "internal\\internal_light_torch_r2");
    let second = lights.get(t.second).unwrap();
    assert!(!second.active && second.kind == LightKind::Point && !second.shadow);
    assert_eq!((second.range, second.colour), (1.5, [1.0, 1.0, 1.0, 0.1]));
    let g = glows.get(t.glow).unwrap();
    assert!(!g.on && g.radius == 0.3 && g.shader == 7);
    assert_eq!(t.byte(true), 0b100);
    // 4: the player's torch never lights its second light.
    t.switch(true, Some(Owner::Player), true, &mut lights, &mut glows);
    assert!(t.on && t.cone_visible && glows.get(t.glow).unwrap().on);
    assert!(lights.get(t.main).unwrap().active && !lights.get(t.second).unwrap().active);
    assert_eq!(t.byte(true), 0b101);
    t.switch(false, Some(Owner::Player), true, &mut lights, &mut glows);
    assert!(!lights.get(t.main).unwrap().active && !glows.get(t.glow).unwrap().on);

    // A character with the setting off withholds both lights; the glow and
    // the cone follow.
    let (mut t, mut lights, mut glows) = spawned(0, Some(Owner::Character), false);
    t.switch(true, Some(Owner::Character), false, &mut lights, &mut glows);
    assert!(glows.get(t.glow).unwrap().on && t.cone_visible);
    assert!(!lights.get(t.main).unwrap().active && !lights.get(t.second).unwrap().active);
    // With it on, both lights; stored on at spawn, on at once.
    let (t, lights, _) = spawned(0b11, Some(Owner::Character), true);
    assert!(t.on && lights.get(t.main).unwrap().active && lights.get(t.second).unwrap().active);
    // The saved request is not restored (3 step 9).
    assert!(!t.night_vision.as_ref().unwrap().requested);
    assert!(withholds(Some(Owner::Character), false));
    assert!(!withholds(Some(Owner::Other), false) && !withholds(None, false));
}

#[test]
fn night_vision_requests_and_sounds() {
    use SoundCommand::{Play, Stop};
    let on = Play {
        sound: NightVisionSound::On,
        looped: false,
    };
    let idle = Play {
        sound: NightVisionSound::Idle,
        looped: true,
    };
    let off = Play {
        sound: NightVisionSound::Off,
        looped: false,
    };
    let mut nv = NightVision::default();
    let mut out = Vec::new();
    // 16: no outfit effect, the request toggles silently.
    nv.switch(true, true, None, false, &mut out);
    assert!(nv.requested && nv.effect.is_none() && out.is_empty());
    nv.switch(false, true, None, false, &mut out);
    assert!(out.is_empty());
    // 15.2 steps 4 and 5.
    nv.switch(
        true,
        true,
        Some("effector_nightvision_bad"),
        false,
        &mut out,
    );
    assert_eq!(out, [on, idle]);
    assert_eq!(nv.effect.as_deref(), Some("effector_nightvision_bad"));
    out.clear();
    nv.switch(
        true,
        true,
        Some("effector_nightvision_bad"),
        false,
        &mut out,
    );
    assert!(out.is_empty(), "one effect at a time");
    nv.switch(false, true, None, false, &mut out);
    assert_eq!(out, [off, Stop(NightVisionSound::Idle)]);
    assert!(nv.effect.is_none());
    out.clear();
    // Step 3: a disabled level plays the broken sound for either request.
    nv.switch(true, true, Some("e"), true, &mut out);
    nv.switch(false, true, Some("e"), true, &mut out);
    let broken = Play {
        sound: NightVisionSound::Broken,
        looped: false,
    };
    assert_eq!(out, [broken, broken]);
    out.clear();
    // Step 2: another owner only stores the request.
    nv.switch(true, false, Some("e"), false, &mut out);
    assert!(nv.requested && out.is_empty() && nv.effect.is_none());

    // Leaving the owner (4): switched off, the request off with its sound,
    // then the on, off and idle sounds stopped.
    let (mut t, mut lights, mut glows) = spawned(1, Some(Owner::Player), true);
    t.switch_night_vision(true, Some(Owner::Player), Some("e"), false, &mut out);
    out.clear();
    t.leave(
        Some(Owner::Player),
        true,
        Some("e"),
        false,
        &mut lights,
        &mut glows,
        &mut out,
    );
    assert!(!t.on && !lights.get(t.main).unwrap().active);
    assert_eq!(
        out,
        [
            off,
            Stop(NightVisionSound::Idle),
            Stop(NightVisionSound::On),
            Stop(NightVisionSound::Off),
            Stop(NightVisionSound::Idle)
        ]
    );
    assert_eq!(t.byte(false), 0);
}

const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

#[test]
fn placement_each_frame() {
    let mut x = IDENTITY;
    x[3] = [10.0, 0.0, 20.0, 1.0];
    let mut bone = IDENTITY;
    bone[3] = [0.0, 1.6, 0.0, 1.0];
    let pose = OwnerPose {
        owner: Owner::Player,
        transform: x,
        bone,
        centre: [10.0, 0.9, 20.0],
        radius: 1.2,
    };
    let (mut t, mut lights, mut glows) = spawned(1, Some(Owner::Player), true);
    // The player looking along +x (yaw π/2) and level: heading and pitch
    // move toward −yaw and −pitch.
    let aim = Aim {
        yaw: PI / 2.0,
        pitch: 0.0,
    };
    let eye = [10.0, 1.7, 20.0];
    for _ in 0..200 {
        t.frame(
            &IDENTITY,
            &IDENTITY,
            &pose,
            eye,
            aim,
            0.02,
            true,
            None,
            &mut lights,
            &mut glows,
        );
    }
    assert!(close(t.heading, -PI / 2.0, 1e-5) && t.pitch == 0.0);
    let g = direction(t.heading, t.pitch, t.heading_offset);
    let main = lights.get(t.main).unwrap();
    let l = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
    assert!((0..3).all(|k| close(main.direction[k], g[k] / l, 1e-6)));
    assert!(close(g[0], (t.heading_offset).cos(), 1e-6) && g[0] > 0.99);
    // Row 3 − 0.2 · row 0 + 0.1 · row 1 − 0.3 · row 2 of M = O · B · X.
    let near = |a: Vec3, b: Vec3| (0..3).all(|k| close(a[k], b[k], 1e-5));
    assert!(near(main.position, [9.8, 1.7, 19.7]), "{:?}", main.position);
    let glow = glows.get(t.glow).unwrap();
    assert!(near(glow.position, [10.0, 1.6, 20.0]));
    assert!(!lights.get(t.second).unwrap().active);

    // The far form: the owner beyond 100 m of the camera.
    let far = placement(&IDENTITY, &IDENTITY, &pose, [200.0, 0.0, 0.0]);
    assert!(near([far[3][0], far[3][1], far[3][2]], [10.0, 1.7, 20.0]));

    // A character's torch: both lights at row 3 along row 2.
    let pose = OwnerPose {
        owner: Owner::Character,
        ..pose
    };
    let (mut t, mut lights, mut glows) = spawned(1, Some(Owner::Character), true);
    t.frame(
        &IDENTITY,
        &IDENTITY,
        &pose,
        eye,
        aim,
        0.02,
        true,
        None,
        &mut lights,
        &mut glows,
    );
    for id in [t.main, t.second] {
        let r = lights.get(id).unwrap();
        assert!(near(r.position, [10.0, 1.6, 20.0]) && r.direction == [0.0, 0.0, 1.0]);
    }

    // Off, it is not placed; without an owner, switched off when it lies
    // in the world with its model shown.
    let (mut t, mut lights, mut glows) = spawned(1, None, true);
    t.frame_without_owner(true, false, &mut lights, &mut glows);
    assert!(t.on);
    t.frame_without_owner(true, true, &mut lights, &mut glows);
    assert!(!t.on && t.cone_visible && !glows.get(t.glow).unwrap().on);
    assert!(!lights.get(t.second).unwrap().active);
}

#[test]
fn the_colour_animator() {
    // 7.5: (red, green, blue) × b / 255 on both lights, alpha 1.
    let entry = Entry {
        name: b"x".to_vec(),
        frame_rate: 15.0,
        frame_count: 15,
        keys: vec![(0, 0x00FF_8000)],
    };
    let pose = OwnerPose {
        owner: Owner::Character,
        transform: IDENTITY,
        bone: IDENTITY,
        centre: [0.0; 3],
        radius: 1.0,
    };
    let (mut t, mut lights, mut glows) = spawned(1, Some(Owner::Character), true);
    let aim = Aim {
        yaw: 0.0,
        pitch: 0.0,
    };
    t.frame(
        &IDENTITY,
        &IDENTITY,
        &pose,
        [0.0; 3],
        aim,
        0.02,
        true,
        Some((&entry, 0.5)),
        &mut lights,
        &mut glows,
    );
    let c = lights.get(t.second).unwrap().colour;
    assert_eq!(c, [1.0, 128.0 / 255.0, 0.0, 1.0]);
}
