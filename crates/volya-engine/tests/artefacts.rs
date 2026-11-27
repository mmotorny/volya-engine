// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Artefacts' idle effects and lights from the Russian edition's data
//! (BYOA; docs/behavior/artefact-idle-effects.md): every artefact
//! section's keys (sections 1.3, 9.1), the first level's artefacts at a New
//! Game's start (section 8). `esc_af_medusa`'s effect drawn is the
//! render-level suite's (`selftest::artefacts`). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::artefacts::{ArtefactSection, Behaviour, State};
use volya_engine::core::camera::Angles;
use volya_engine::core::input::Input;
use volya_engine::core::particles::placed::{Instance, Origin};
use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn root() -> Option<StdVfs> {
    let archives = gog_archives::archives()?;
    Some(StdVfs::new(archives[0].file.parent().unwrap()))
}

fn near(a: [f32; 3], b: [f32; 3], tolerance: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tolerance)
}

#[test]
fn artefact_sections_from_game_data() {
    let Some(root) = root() else {
        return;
    };
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    // P10 1.3: every section, its effect, light and `jump_height`.
    type Row = (&'static str, Option<&'static str>, Option<[f32; 4]>, f32);
    let table: [Row; 29] = [
        ("af_medusa", Some("gravi"), None, 0.5),
        (
            "af_cristall_flower",
            Some("gravi"),
            Some([0.6, 0.7, 0.5, 0.5]),
            0.4,
        ),
        (
            "af_night_star",
            Some("gravi"),
            Some([0.6, 0.7, 0.5, 3.0]),
            0.2,
        ),
        ("af_vyvert", Some("gravi"), None, 0.3),
        ("af_gravi", Some("gravi"), Some([0.6, 0.7, 0.5, 0.5]), 0.3),
        (
            "af_gold_fish",
            Some("gravi"),
            Some([0.6, 0.7, 0.5, 2.0]),
            0.2,
        ),
        ("af_drops", Some("zharka"), Some([0.6, 0.7, 0.5, 0.5]), 0.1),
        (
            "af_fireball",
            Some("zharka"),
            Some([0.9, 0.5, 0.5, 3.0]),
            0.0,
        ),
        (
            "af_cristall",
            Some("zharka"),
            Some([0.9, 0.5, 0.5, 3.0]),
            0.1,
        ),
        ("af_blood", Some("mincer"), None, 0.01),
        ("af_mincer_meat", Some("mincer"), None, 0.1),
        ("af_soul", Some("mincer"), None, 0.1),
        (
            "af_electra_sparkler",
            Some("electra"),
            Some([0.5, 0.5, 0.9, 0.5]),
            0.0,
        ),
        (
            "af_electra_flash",
            Some("electra"),
            Some([0.6, 0.7, 0.9, 2.5]),
            0.0,
        ),
        (
            "af_electra_moonlight",
            Some("electra"),
            Some([0.6, 0.7, 0.9, 1.0]),
            0.0,
        ),
        ("af_ameba_slime", Some("ameba"), None, 0.0),
        (
            "af_ameba_slug",
            Some("ameba"),
            Some([0.6, 0.6, 0.5, 1.0]),
            0.1,
        ),
        (
            "af_ameba_mica",
            Some("ameba"),
            Some([0.6, 0.7, 0.6, 1.0]),
            0.1,
        ),
        ("af_dummy_spring", None, None, 0.2),
        ("af_dummy_dummy", None, None, 0.0),
        ("af_dummy_battery", Some("gravi"), None, 0.2),
        ("af_dummy_pellicle", Some("ameba"), None, 0.2),
        ("af_dummy_glassbeads", Some("mincer"), None, 0.2),
        ("af_rusty_thorn", None, None, 0.3),
        ("af_rusty_kristall", None, None, 0.4),
        ("af_rusty_sea-urchin", None, None, 0.5),
        ("af_fuzz_kolobok", Some("puxx"), None, 0.5),
        ("af_blood_tutorial", Some("mincer"), None, 0.01),
        (
            "mp_af_electra_flash",
            Some("electra"),
            Some([0.6, 0.7, 0.9, 2.5]),
            0.0,
        ),
    ];
    let mut found = 0;
    for s in data.system.sections() {
        let Some(behaviour) = s
            .get("class")
            .flatten()
            .and_then(|c| Behaviour::of(c.trim()))
        else {
            continue;
        };
        found += 1;
        let a = ArtefactSection::read(s, behaviour).unwrap();
        let (_, effect, light, jump) = table
            .iter()
            .find(|r| r.0 == a.name)
            .unwrap_or_else(|| panic!("{} not in the table", a.name));
        assert_eq!(
            a.particles.as_deref(),
            effect
                .map(|e| format!("anomaly2\\artefact\\artefact_{e}"))
                .as_deref(),
            "{}",
            a.name
        );
        assert_eq!(
            a.light.map(|l| [l.rgb[0], l.rgb[1], l.rgb[2], l.range]),
            *light,
            "{}",
            a.name
        );
        assert_eq!(a.jump_height, *jump, "{}", a.name);
        let plain = a.name.contains("electra");
        assert_eq!(behaviour == Behaviour::Plain, plain, "{}", a.name);
    }
    assert_eq!(found, 29);
}

#[test]
fn first_level_artefacts_from_game_data() {
    let Some(root) = root() else {
        return;
    };
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    let mut level = LevelScreen::load(&tree, &data, 0).unwrap();

    // P10 8: 6 artefact records on the level, 4 of them with lights.
    let sim = level.simulation().unwrap();
    let records: Vec<(String, String)> = sim
        .level_list()
        .filter_map(|id| sim.object(id))
        .filter(|o| Behaviour::of(o.record.class_id.as_str()).is_some())
        .map(|o| {
            (
                o.record.header.name.to_string(),
                o.record.header.section.to_string(),
            )
        })
        .collect();
    assert_eq!(records.len(), 6, "{records:?}");
    for (name, _) in &records {
        assert!(
            [
                "esc_af_medusa",
                "esc_af_blood",
                "esc_af_cristall_flower",
                "esc_af_electra_flash",
                "esc_af_electra_sparkler",
                "esc_af_electra_sparkler_0000",
            ]
            .contains(&name.as_str()),
            "{name}"
        );
    }

    // Online at the start only `esc_af_medusa`, lying, its light off.
    let artefacts = level.artefacts().artefacts();
    assert_eq!(artefacts.len(), 1);
    let a = &artefacts[0];
    assert_eq!(a.name, "esc_af_medusa");
    assert_eq!(a.section.name, "af_medusa");
    assert_eq!(a.state, State::Lying);
    assert!(a.section.light.is_none() && !a.light_on);
    assert!(!level.lights().get(a.record.unwrap()).unwrap().active);
    // M2: *X*'s row 3, the effect point at bone 0, the placed sphere.
    assert!(near(a.position(), [-244.8165, -13.1585, -33.6452], 1e-4));
    assert!(
        near(a.point(), [-244.8057, -13.1224, -33.6349], 1e-4),
        "{:?}",
        a.point()
    );
    let (c, r) = a.placed_sphere();
    assert!(near(c, [-244.7696, -13.0158, -33.6089], 1e-4), "{c:?}");
    assert!((r - 0.23).abs() < 1e-4, "{r}");
    // Its effect: the group `artefact_gravi`, playing.
    assert_eq!(a.effects.len(), 1);
    let o = a.effects[0].object;
    let p = level.effects().unwrap().get(o).unwrap();
    assert_eq!(p.name, "anomaly2\\artefact\\artefact_gravi");
    assert_eq!(p.origin, Origin::Artefact);
    assert!(matches!(p.instance, Instance::Group(_)));
    assert!(p.instance.is_playing());
    // Its shaders are resolved with the level's.
    let world = level.world().clone();
    for e in p.instance.effects() {
        let effect = level.effects().unwrap().library().effect_at(e.record());
        if effect.sprite.is_some() {
            assert!(
                world.particle_shaders[e.record()].is_some(),
                "{}",
                effect.name
            );
        }
    }

    // Runs 10 s with the camera 2 m above it: fast (within 50.23 m of its
    // sphere's centre), the effect keeps playing at its point.
    let eye = [c[0], c[1] + 2.0, c[2]];
    for t in 0..625u64 {
        level.set_camera(eye, Angles::default());
        level.update(16 * (t + 1), &Input::new());
    }
    let a = &level.artefacts().artefacts()[0];
    assert!(a.fast);
    let p = level.effects().unwrap().get(a.effects[0].object).unwrap();
    assert!(p.instance.is_playing());
    let particles: usize = p.instance.effects().map(|e| e.particles().len()).sum();
    eprintln!("esc_af_medusa's effect has {particles} particles after 10 s");
    assert!(particles > 0);
    // No artefact light is on (P10 8).
    assert!(!level.lights().get(a.record.unwrap()).unwrap().active);
    assert!(level.take_fatal().is_none());
}
