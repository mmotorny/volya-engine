// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's torch on a New Game (BYOA, docs/behavior/torch.md): the
//! verification facts of section 17, the torch at the first level's start
//! (12, 16), switched on with its key (its light records and glow, 3, 4,
//! 7.4), night vision with the starting outfit (silent) and with one that
//! has an effect (its sounds, 14, 15.2), the torch leaving its slot. The
//! level lit by it is the render-level suite's (`selftest::torch`). Skips
//! when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::character::inventory::{InventoryItem, ItemConfig, ItemState};
use volya_engine::core::input::{Event, Input, Key};
use volya_engine::core::lights::LightKind;
use volya_engine::core::sound::system::{NoDevice, Phase, SoundSystem};
use volya_engine::core::torch::{self, TorchSection};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{self, StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine::level_sound::LevelSounds;
use volya_engine::torch::read_definition;
use volya_engine_formats::lanims::ColourAnimations;
use volya_engine_formats::model::{self, Model};
use volya_engine_formats::object::Kind;
use volya_engine_formats::ui::Resolution;

const SIZE: Resolution = Resolution::new(1024, 768);

fn with_game(f: impl FnOnce(&dyn Vfs, &GameData)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();
    f(&tree, &data);
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
// The data's −3.14 is not π.
#[allow(clippy::approx_constant)]
fn the_torch_data_from_game_data() {
    with_game(|tree, data| {
        // 1.1 [T-D1].
        let s = data.system.section("device_torch").unwrap();
        assert_eq!(s.read("class").unwrap().trim(), "TORCH_S");
        assert_eq!(s.read("slot").unwrap().trim(), "9");
        let t = TorchSection::read(s).unwrap();
        assert_eq!(t.light_trace_bone, "cone_bone");
        assert_eq!(t.attach_bone_name, "bip01_head");
        assert_eq!(t.attach_angle_offset, [0.0, -3.14, 1.6]);
        assert_eq!(t.attach_position_offset, [0.115, 0.0, 0.07]);
        let nv = t.night_vision.as_ref().unwrap();
        let names: Vec<_> = (torch::NightVisionSound::ALL.iter())
            .map(|&k| nv.variants(k)[0].name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "weapons\\nv_start",
                "weapons\\nv_off",
                "weapons\\nv_loop",
                "weapons\\nv_off"
            ]
        );
        assert_eq!(nv.disabled_maps, ["ai_test", "ai_test2", "andy_test_"]);
        // 1.2 [T-M1 … T-M4, T-D4].
        let path = data
            .aliases
            .path("$game_meshes$", "equipments\\light_night.ogf")
            .unwrap();
        let file = vfs::read(tree, &path).unwrap();
        assert_eq!(file.len(), 73_740);
        let user = model::user_data(&file).unwrap().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&user).trim(),
            "#include \"models\\objects\\light_night.ltx\""
        );
        let d = read_definition(tree, data, &path, &file).unwrap();
        assert_eq!((d.range, d.colour), (13.0, [1.0, 1.0, 1.0, 0.2]));
        assert_eq!((d.omni_range, d.omni_colour), (1.5, [1.0, 1.0, 1.0, 0.1]));
        assert_eq!(d.colour_animator, "empty");
        assert_eq!(d.spot_texture, "internal\\internal_light_torch_r2");
        assert_eq!((d.spot_angle, d.glow_radius), (60.0, 0.3));
        assert_eq!(d.glow_texture, "glow\\glow_torch_r2");
        assert_eq!(d.guide_bone, "lights_bone");
        let m = Model::parse(&file).unwrap();
        let bones: Vec<_> = m.bones.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            bones,
            ["main_bone", "bone_nigth", "lights_bone", "cone_bone"]
        );
        assert!((1..4).all(|i| m.bones[i].parent == Some(i - 1)));
        // The skeleton alone reads the same bones and header.
        assert_eq!(model::skeleton(&file).unwrap(), (m.header, m.bones.clone()));
        let meshes: Vec<_> = (m.meshes.iter())
            .map(|c| (c.texture.as_str(), c.shader.as_str()))
            .collect();
        assert_eq!(
            meshes,
            [
                ("wpn\\wpn_upgrade", "models\\model"),
                ("wpn\\wpn_upgrade", "models\\selflight"),
                ("lights\\lights_cone_handing_lamp", "models\\lightplanes"),
                ("glow\\glow_Yellow", "models\\lightplanes"),
                ("wpn\\wpn_upgrade", "models\\weapons"),
            ]
        );
        // 7: G, the guide bone at rest [T-N3].
        let g = m.bind_pose().unwrap()[m.bone("lights_bone").unwrap()];
        let want = [
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.0563, 0.0162, -0.1480],
        ];
        for (r, w) in want.iter().enumerate() {
            assert!(
                (0..3).all(|k| close(g[r][k], w[k], 6e-5)),
                "G row {r}: {:?}",
                g[r]
            );
        }
        // 3 step 2 [T-D5]: no `empty` entry.
        let lanims = vfs::read(
            tree,
            &data.aliases.path("$game_data$", "lanims.xr").unwrap(),
        );
        let lanims = ColourAnimations::parse(&lanims.unwrap()).unwrap();
        assert!(lanims.find(b"empty").is_none());
        // 15.1 [T-D8]: 20 outfit sections with an effect, the starting
        // outfit none.
        let (mut bad, mut good) = (0, 0);
        for s in data.system.sections() {
            let Ok(c) = ItemConfig::read(&data.system, s.name()) else {
                continue;
            };
            if c.class_id.kind() != Kind::Outfit {
                continue;
            }
            match c.nightvision_sect.as_deref() {
                Some("effector_nightvision_bad") => bad += 1,
                Some("effector_nightvision_good") => good += 1,
                Some(other) => panic!("{}: {other}", s.name()),
                None => {}
            }
        }
        assert_eq!((bad, good), (14, 6));
        let novice = ItemConfig::read(&data.system, "novice_outfit").unwrap();
        assert!(novice.class_id.kind() == Kind::Outfit && novice.nightvision_sect.is_none());
        // 14 [T-D11].
        for f in ["nv_start", "nv_off", "nv_loop"] {
            let p = data
                .aliases
                .path("$game_sounds$", &format!("weapons\\{f}.ogg"))
                .unwrap();
            assert!(tree.exists(&p), "{f}");
        }
    });
}

fn press(level: &mut LevelScreen, now: &mut u64, key: Key) {
    let mut input = Input::new();
    input.handle(Event::Key { key, pressed: true });
    *now += 16;
    level.update(*now, &input);
}

/// Frames without input until `now` + `ms`.
fn run(level: &mut LevelScreen, now: &mut u64, ms: u64) {
    let end = *now + ms;
    while *now < end {
        *now += 16;
        level.update(*now, &Input::new());
    }
}

/// The emitters playing, by name, with their 2D and looped flags.
fn playing(system: &SoundSystem) -> Vec<(String, bool, bool)> {
    (system.emitters().into_iter())
        .filter(|e| e.phase != Phase::Ended)
        .map(|e| (e.name, e.two_d, e.looped))
        .collect()
}

#[test]
fn the_torch_on_a_new_game_from_game_data() {
    with_game(|tree, data| {
        let mut level = LevelScreen::load(tree, data, 0).unwrap();
        let mut now = 0;
        run(&mut level, &mut now, 16);
        // 12: the starting torch, attached, off; its byte 0b100.
        let r = level.state().torch.unwrap();
        assert_eq!(r.section, "device_torch");
        assert!(r.attached && !r.on && !r.main_active && !r.second_active && !r.glow_on);
        assert!(!r.cone_visible && !r.night_vision_requested && r.byte == 0b100);
        let (main, second, glow) = level.torches().records().unwrap();
        let records = level.lights().len();

        // L switches it on: the main light and the glow, placed by 7.4.
        press(&mut level, &mut now, Key::L);
        run(&mut level, &mut now, 1000);
        let r = level.state().torch.unwrap();
        assert!(r.on && r.main_active && !r.second_active && r.glow_on && r.cone_visible);
        assert_eq!(r.byte, 0b101);
        assert_eq!(level.lights().len(), records, "no records made or lost");
        let m = level.lights().get(main).unwrap().clone();
        assert!(m.active && m.kind == LightKind::Spot && m.shadow);
        assert_eq!((m.range, m.colour), (13.0, [1.0, 1.0, 1.0, 0.2]));
        assert!(close(m.cone, std::f32::consts::FRAC_PI_3, 1e-6));
        assert!(close(m.radius, 8.6667, 1e-4));
        assert!(!level.lights().get(second).unwrap().active);
        let v = level.player().view();
        // Aimed where the camera looks, but for the heading offset.
        let cos: f32 = (0..3).map(|k| m.direction[k] * v.direction[k]).sum();
        assert!(cos > 0.999, "torch along the view: cos {cos}");
        // At the head: within 0.6 m of the eye.
        let d: f32 = (0..3)
            .map(|k| (m.position[k] - v.eye[k]).powi(2))
            .sum::<f32>()
            .sqrt();
        assert!(
            d < 0.6,
            "main light {d} m from the eye: {:?} {:?}",
            m.position,
            v.eye
        );
        let g = level.level_lights().glows.get(glow).unwrap();
        assert!(g.on && g.radius == 0.3 && g.direction.is_some());
        let shader = &level.world().glow_shaders[g.shader as usize];
        assert!(shader.texture.contains("glow_torch_r2") && !shader.flare);

        // Night vision with the starting outfit: the request toggles, no
        // effect, no sound (16).
        let mut system = SoundSystem::new(24);
        let sound = level.take_sound_data().unwrap();
        let mut sounds = LevelSounds::create(&mut system, tree, &data.aliases, &sound).unwrap();
        let mut sound_frame = |level: &mut LevelScreen, system: &mut SoundSystem| {
            for s in level.take_torch_sounds() {
                let section = level.torches().night_vision(s.torch);
                sounds
                    .torch(system, tree, &data.aliases, s, section)
                    .unwrap();
            }
            let t = system.now() + 16;
            system.update(t, &mut NoDevice);
        };
        press(&mut level, &mut now, Key::N);
        sound_frame(&mut level, &mut system);
        let r = level.state().torch.unwrap();
        assert!(r.night_vision_requested && r.night_vision_effect.is_none());
        assert!(playing(&system).is_empty());
        press(&mut level, &mut now, Key::N);
        sound_frame(&mut level, &mut system);
        assert!(playing(&system).is_empty());

        // An outfit with an effect section in slot 6 (15.2 step 4): the on
        // sound once and the idle sound looped, both 2D.
        let id = level.take_item_id().unwrap();
        let c = level.character_mut().unwrap();
        let config = (data.system.sections())
            .filter_map(|s| ItemConfig::read(&data.system, s.name()).ok())
            .find(|c| c.class_id.kind() == Kind::Outfit && c.nightvision_sect.is_some())
            .unwrap();
        let effect = config.nightvision_sect.clone().unwrap();
        let item =
            InventoryItem::create(config, id, None, c.owner.id, &c.profile.specific_character);
        c.inventory.take(item);
        assert!(c.inventory.move_to_slot_with_swap(id).succeeded());
        press(&mut level, &mut now, Key::N);
        sound_frame(&mut level, &mut system);
        let r = level.state().torch.unwrap();
        assert_eq!(r.night_vision_effect.as_deref(), Some(effect.as_str()));
        let p = playing(&system);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(
            p.iter()
                .any(|(n, d2, l)| n.contains("nv_start") && *d2 && !l)
        );
        assert!(
            p.iter()
                .any(|(n, d2, l)| n.contains("nv_loop") && *d2 && *l)
        );
        assert_eq!(r.byte, 0b111);
        // Again (step 5): the off sound, the idle sound stopped.
        press(&mut level, &mut now, Key::N);
        sound_frame(&mut level, &mut system);
        let p = playing(&system);
        assert!(
            p.iter().any(|(n, _, l)| n.contains("nv_off") && !l),
            "{p:?}"
        );
        assert!(!p.iter().any(|(n, _, _)| n.contains("nv_loop")));

        // The torch to the backpack: detached and off (4, 6); the command
        // finds no torch.
        press(&mut level, &mut now, Key::N);
        sound_frame(&mut level, &mut system);
        let torch_id = level.state().torch.unwrap().id;
        let c = level.character_mut().unwrap();
        assert!(c.inventory.move_to_backpack(torch_id));
        run(&mut level, &mut now, 16);
        let r = level.state().torch.unwrap();
        assert!(!r.attached && !r.on && !r.main_active && !r.glow_on);
        // The request is kept (15.3), the effect too.
        assert!(r.night_vision_requested && r.night_vision_effect.is_some());
        press(&mut level, &mut now, Key::L);
        assert!(!level.state().torch.unwrap().on);
        // Back in its slot, on again with L.
        let c = level.character_mut().unwrap();
        assert!(c.inventory.move_to_slot_with_swap(torch_id).succeeded());
        press(&mut level, &mut now, Key::L);
        assert!(level.state().torch.unwrap().on);
        // Dropped: it leaves the player, its sounds stopped at once.
        sound_frame(&mut level, &mut system);
        let c = level.character_mut().unwrap();
        let dropped = c.drop_item(torch_id).unwrap();
        assert!(matches!(dropped.state, ItemState::Torch { .. }));
        run(&mut level, &mut now, 16);
        sound_frame(&mut level, &mut system);
        assert!(level.state().torch.is_none());
        assert!(level.lights().get(main).is_none());
        assert!(!playing(&system).iter().any(|(n, _, _)| n.contains("nv_")));
    });
}
