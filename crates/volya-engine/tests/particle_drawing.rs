// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The drawing facts of docs/behavior/particle-drawing.md section 8.1 on
//! the Russian edition (BYOA): every sprite's shader resolves as section
//! 5.1 says and its textures exist; the orientation, culling and frame
//! cases over the library. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs, read};
use volya_engine::data::GameData;
use volya_engine::particle_drawing::resolve;
use volya_engine::render::particles::{ParticleBlend, ParticleProgram};
use volya_engine::shading::{defines_normal_or_special, texture_list};
use volya_engine_formats::particles::Library;
use volya_engine_formats::shaders_xr;
use volya_engine_formats::ui::Resolution;

#[test]
fn every_sprite_resolves_as_section_5_says() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    let library = Library::parse(&read(&tree, "gamedata/particles.xr").unwrap()).unwrap();
    let definitions: HashMap<String, shaders_xr::Definition> =
        shaders_xr::parse(&read(&tree, "gamedata/shaders.xr").unwrap())
            .unwrap()
            .into_iter()
            .map(|d| (d.name.clone(), d))
            .collect();
    let shader_dir = data.aliases.path("$game_shaders$", "r1").unwrap();
    let scripted = |module: &str| {
        read(&tree, &format!("{shader_dir}\\{module}.s"))
            .is_ok_and(|t| defines_normal_or_special(&String::from_utf8_lossy(&t)))
    };
    let sprites: Vec<_> = library
        .effects()
        .filter_map(|e| e.sprite.map(|s| (e, s)))
        .collect();
    assert_eq!(sprites.len(), 1070);
    let mut per_shader: BTreeMap<&str, usize> = BTreeMap::new();
    let mut textures = BTreeSet::new();
    let (mut one, mut two) = (0, 0);
    for (e, s) in &sprites {
        *per_shader.entry(s.shader).or_default() += 1;
        let list = texture_list(&s.texture);
        match list.len() {
            1 => one += 1,
            2 => two += 1,
            n => panic!("{}: {n} textures", e.name),
        }
        textures.extend(list);
        let shader = resolve(&definitions, &scripted, &s.shader, &s.texture)
            .unwrap_or_else(|why| panic!("{}: {why}", e.name));
        // Section 5.1's table.
        let normal = shader.normal.as_ref();
        match s.shader {
            "particles\\add" => assert_eq!(normal.unwrap().blend, ParticleBlend::Add),
            "particles\\blend" => {
                let n = normal.unwrap();
                assert_eq!((n.blend, n.strict), (ParticleBlend::Alpha, true));
            }
            "particles\\alpha_add" => assert_eq!(normal.unwrap().blend, ParticleBlend::AlphaAdd),
            "particles\\set" => {
                let n = normal.unwrap();
                assert_eq!((n.program, n.priority), (ParticleProgram::Set, 1));
            }
            "particles\\xadd" => {
                assert_eq!(normal.unwrap().program, ParticleProgram::Plain);
                assert!(shader.distortion.is_some());
            }
            "particles\\xdistort" => {
                assert!(normal.is_none() && shader.distortion.is_some());
            }
            other => panic!("{other}"),
        }
        if s.shader != "particles\\xadd" && s.shader != "particles\\xdistort" {
            assert!(normal.unwrap().clamp, "{}: Texture clamp", e.name);
        }
    }
    let want: BTreeMap<&str, usize> = [
        ("particles\\add", 383),
        ("particles\\blend", 322),
        ("particles\\alpha_add", 213),
        ("particles\\xdistort", 102),
        ("particles\\xadd", 49),
        ("particles\\set", 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(per_shader, want);
    assert_eq!((one, two), (1018, 52));
    assert_eq!(textures.len(), 136);
    for t in &textures {
        let path = data
            .aliases
            .path("$game_textures$", &format!("{t}.dds"))
            .unwrap();
        assert!(tree.exists(&path), "{path}");
    }

    // Orientation, culling and frame cases (section 8.1).
    let count = |test: &dyn Fn(u32) -> bool| sprites.iter().filter(|(e, _)| test(e.flags)).count();
    assert_eq!(count(&|f| f & 0x8000 == 0), 833);
    assert_eq!(count(&|f| f & 0x8000 != 0 && f & 0x300000 == 0), 115);
    assert_eq!(count(&|f| f & 0x8000 != 0 && f & 0x300000 == 0x200000), 88);
    assert_eq!(count(&|f| f & 0x8000 != 0 && f & 0x300000 == 0x100000), 13);
    assert_eq!(count(&|f| f & 0x8000 != 0 && f & 0x300000 == 0x300000), 21);
    assert_eq!(count(&|f| f & 0xC00000 == 0), 1057);
    assert_eq!(count(&|f| f & 0xC00000 == 0x400000), 12);
    assert_eq!(count(&|f| f & 0xC00000 == 0x800000), 1);
    assert_eq!(count(&|f| f & 0x400 != 0), 422);
    assert_eq!(count(&|f| f & 0xC00 == 0xC00), 228);
    assert_eq!(count(&|f| f & 0x1400 == 0x1400), 252);
    assert_eq!(count(&|f| f & 0x2C00 == 0x2C00), 23);
    assert_eq!(count(&|f| f & 0x40000 != 0), 113);
    let muhi = library.effect_at(library.effect("tests\\muhi").unwrap());
    let s = muhi.sprite.unwrap();
    assert_eq!(
        (s.shader, texture_list(&s.texture)),
        ("particles\\blend", vec!["pfx\\pfx_leaves_01".to_owned()])
    );
}
