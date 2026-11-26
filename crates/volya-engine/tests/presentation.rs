// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Frame presentation from the Russian edition (BYOA): the verification
//! facts of docs/behavior/frame-presentation.md section 5. Its drawing, the
//! start view at `r__supersample` 2 against 1, is the render-level suite's
//! (`selftest::presentation`). Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::fs::File;
use std::io::BufReader;

use volya_engine::core::console::Console;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{self, StdVfs};
use volya_engine::data::{DEFAULT_USER_LTX, GameData};
use volya_engine::level::LevelScreen;
use volya_engine_formats::dds::{Dds, Format, Masks};
use volya_engine_formats::ui::Resolution;
use volya_engine_installer::{Installer, extract};

const SIZE: Resolution = Resolution::new(1024, 768);

/// The installer's `__support/userdocs/user.ltx`.
fn shipped_user_ltx() -> Vec<u8> {
    let exe = std::fs::read_dir(std::env::var_os("VOLYA_GOG_INSTALLER").unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.contains("(russian)") && n.ends_with(".exe")
        })
        .unwrap();
    let bin = exe.with_file_name(format!(
        "{}-1.bin",
        exe.file_stem().unwrap().to_string_lossy()
    ));
    let installer = Installer::parse(BufReader::new(File::open(&exe).unwrap())).unwrap();
    let file = installer
        .game_files("ru-RU")
        .find(|f| f.path == DEFAULT_USER_LTX)
        .unwrap();
    let mut data = Vec::new();
    extract(
        file,
        &mut BufReader::new(File::open(&bin).unwrap()),
        &mut data,
    )
    .unwrap();
    data
}

#[test]
fn presentation_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, SIZE).unwrap();

    // Section 5: the shipped user.ltx.
    let user = String::from_utf8_lossy(&shipped_user_ltx()).into_owned();
    for line in ["r__supersample 2", "r1_pps_u 0.", "r1_pps_v 0."] {
        assert!(user.lines().any(|l| l.trim() == line), "{line}");
    }
    let console = Console::startup(Default::default(), user.as_bytes());
    assert_eq!(console.number("r__supersample"), 2.0);
    // `shaders\r1\.s`'s texture names.
    let s = vfs::read(
        &tree,
        &data.aliases.path("$game_shaders$", "r1\\.s").unwrap(),
    )
    .unwrap();
    // Whitespace collapsed (the file aligns with tabs).
    let s: String = String::from_utf8_lossy(&s)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for name in [
        "t_rt = \"$user$rendertarget\"",
        "t_distort = \"$user$distort\"",
        "t_noise = \"fx\\\\fx_noise2\"",
    ] {
        assert!(s.contains(name), "{name}");
    }
    // `fx_noise2.dds`: 174,890 bytes, 256 × 256, 9 levels, 5:6:5.
    let noise = data
        .aliases
        .path("$game_textures$", "fx\\fx_noise2.dds")
        .unwrap();
    let bytes = vfs::read(&tree, &noise).unwrap();
    assert_eq!(bytes.len(), 174_890);
    let dds = Dds::parse(&bytes).unwrap();
    assert_eq!((dds.width, dds.height, dds.mip_levels), (256, 256, 9));
    assert_eq!(
        dds.format,
        Format::Masked(Masks {
            bits: 16,
            r: 0xF800,
            g: 0x07E0,
            b: 0x001F,
            a: 0,
        })
    );
    // The level presents with it.
    let screen = LevelScreen::load(&tree, &data, 0).unwrap();
    assert_eq!(
        screen.world().presentation_noise.as_deref(),
        Some(noise.as_str())
    );
}
