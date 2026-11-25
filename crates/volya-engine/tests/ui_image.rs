// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The pictures the web page's touch controls take from the Russian
//! edition (BYOA, #105): sizes, the posture icons' colour and the
//! crosshair's, with decode times. Skips when `VOLYA_GOG_INSTALLER` is
//! unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::time::Instant;

use volya_engine::core::vfs::StdVfs;
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::data::GameData;
use volya_engine::ui_image::{self, Source};
use volya_engine_formats::ui::Resolution;

#[test]
fn touch_control_pictures_from_game_data() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    let path = |p: &str| Source::Path(p.into());
    let id = |i: &str| Source::Id(i.into());
    let wanted = [
        (path("ui\\ui_hud_frame_back"), None, (128, 128)),
        (id("ui_hud_soldier_crouch"), None, (65, 75)),
        (id("ui_hud_soldier_creep"), None, (65, 75)),
        (id("ui_hud_soldier_sprint"), None, (65, 75)),
        (id("ui_hud_grenadetarget_d"), None, (91, 92)),
        (path("ui\\ui_common"), Some([728, 577, 50, 50]), (50, 50)),
    ];
    let mut bytes = 0;
    let start = Instant::now();
    for (source, region, size) in &wanted {
        let begun = Instant::now();
        let image =
            ui_image::load(&tree, &data.aliases, &data.descriptions, source, *region).unwrap();
        assert_eq!((image.width, image.height), *size, "{source:?}");
        bytes += image.pixels.len();
        if let Source::Id(name) = source
            && name.starts_with("ui_hud_soldier")
        {
            // Teal: green and blue well above red.
            let (shape, mean) = ui_image::silhouette(&image);
            let [r, g, b] = mean.unwrap();
            assert!(g > r + 60 && b > r + 60, "{name}: {r},{g},{b}");
            assert!(shape.pixels.chunks(4).all(|p| p[..3] == [r, g, b]));
            println!("{name}: mean {r},{g},{b}");
        }
        println!("{source:?}: {:.2} ms", begun.elapsed().as_secs_f64() * 1e3);
    }
    println!(
        "all: {:.2} ms, {bytes} bytes of RGBA",
        start.elapsed().as_secs_f64() * 1e3
    );
    // Decoding only the rectangle's rows of blocks matches cutting it
    // from the whole image.
    let whole = ui_image::load(
        &tree,
        &data.aliases,
        &data.descriptions,
        &path("ui\\ui_hud"),
        None,
    )
    .unwrap();
    let crouch = id("ui_hud_soldier_crouch");
    let part = ui_image::load(&tree, &data.aliases, &data.descriptions, &crouch, None).unwrap();
    assert_eq!(
        part.pixels,
        ui_image::crop(&whole, [65, 354, 65, 75]).pixels
    );
    let cross = ui_image::cross_color(&data.system).unwrap();
    println!("cross_color {cross:?}");
    assert!(cross.iter().all(|c| (0.0..=1.0).contains(c)));
}
