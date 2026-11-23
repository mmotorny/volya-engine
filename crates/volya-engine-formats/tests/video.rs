// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Every video of the Russian edition decodes exactly like libtheora: the MD5
//! of each video's frames (picture region, planes in order) was recorded once
//! with libtheora 1.2.0. Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Cursor};

use md5::{Digest, Md5};
use volya_engine_formats::db::{Archive, Scrambling};
use volya_engine_formats::video::{Chroma, TheoraVideo};

/// (lowercase stored path, MD5 of all frames as libtheora decodes them)
const LIBTHEORA: &[(&str, &str)] = &[
    (
        "textures\\car\\trabantnoise#alpha.ogm",
        "469e99ab3ad584c2b39fbce4dc204cb9",
    ),
    (
        "textures\\car\\trabantnoise.ogm",
        "469e99ab3ad584c2b39fbce4dc204cb9",
    ),
    (
        "textures\\intro\\dream_doctor.ogm",
        "7a677c5b7cac742771ed0fd99fcf2a7d",
    ),
    (
        "textures\\intro\\dream_doctor_2.ogm",
        "0ed373fdcfbffdf990b4cdac6cce0560",
    ),
    (
        "textures\\intro\\dream_rats.ogm",
        "1ae516c9bd72b723bcc978612bafd66d",
    ),
    (
        "textures\\intro\\dream_sarcofag.ogm",
        "611d4d7b5537698c9dd909e891d4b414",
    ),
    (
        "textures\\intro\\final_apocal.ogm",
        "11eeb576315bb4cc67579c412c9d258f",
    ),
    (
        "textures\\intro\\final_blind.ogm",
        "0cdd8fc8164baef4686d985b200d557b",
    ),
    (
        "textures\\intro\\final_gold.ogm",
        "03a108f0cd82f72e1e596044c2a13d09",
    ),
    (
        "textures\\intro\\final_immortal.ogm",
        "e9ebe0fa4db33208263f152feb94b20e",
    ),
    (
        "textures\\intro\\final_peace.ogm",
        "ecc6cb27cc8a16ff8fb347d8520b5884",
    ),
    (
        "textures\\intro\\final_to_monolith.ogm",
        "7b11cb1f366eb87f3c2f7c633ef4f71e",
    ),
    (
        "textures\\intro\\intro_gsc-wp.ogm",
        "74dbbac740130e241d6594351ac0286a",
    ),
    (
        "textures\\intro\\intro_gsc.ogm",
        "8cb5d35586c223d76f54d1324dc5a093",
    ),
    (
        "textures\\intro\\intro_kill#alpha.ogm",
        "d1e8e708b3b61470a396288dbc7af9f2",
    ),
    (
        "textures\\intro\\intro_kill.ogm",
        "d7aef470be95326f82413bcc367761f2",
    ),
    (
        "textures\\intro\\intro_mission.ogm",
        "f06920283edcf94e93b1bbf3f4a0bdeb",
    ),
    (
        "textures\\intro\\intro_movie.ogm",
        "d9ccb11b322ba4e4063d5110b3021b24",
    ),
    (
        "textures\\intro\\intro_stalker.ogm",
        "47c1112f2fd3d366ffa6791df2448b31",
    ),
    (
        "textures\\intro\\intro_thq.ogm",
        "b7d1f3d5b83d3c167cecd8f57dac13fb",
    ),
    (
        "textures\\intro\\thq_logo_hyuv.ogm",
        "b062ec393d3e1531c7f0d197413ca044",
    ),
    (
        "textures\\intro\\yantar_dream.ogm",
        "db6ce8bbbee0667cc178363dc8b89399",
    ),
    (
        "textures\\sleep\\aes_sky_red.ogm",
        "d14c6b4aa51800e0807d5c4a01806232",
    ),
    (
        "textures\\sleep\\esc_sky_01.ogm",
        "cc6dfc0c2946f2b3a08cec7abc97875a",
    ),
    (
        "textures\\sleep\\movie-003_rats_output-010.ogm",
        "f1af258ef5168cad1e67cc4dd8563200",
    ),
    (
        "textures\\ui\\1_hyuv_sq.ogm",
        "60791a42f0c38ff11618d17097c62e98",
    ),
    (
        "textures\\ui\\credits_back_512_v10.ogm",
        "53e33124a5bc1d7aecf35044f1d06fa9",
    ),
    (
        "textures\\ui\\ui_mm_coal.ogm",
        "da119b88319477c29f1cbe8286e9ed33",
    ),
    (
        "textures\\ui\\ui_mm_credits_back.ogm",
        "a661037937a812e6f36327a106a5a925",
    ),
    (
        "textures\\ui\\ui_mm_fire.ogm",
        "02fa3eed9d669bb64bce569e6212c5b2",
    ),
    (
        "textures\\ui\\ui_mm_main_back_crop.ogm",
        "bdec991d9317a0535cefab3f654a5905",
    ),
    (
        "textures\\ui\\ui_mm_newspaper.ogm",
        "f355ab0ecf1d6e23ad8c755df76746cf",
    ),
    (
        "textures\\ui\\ui_mm_rug02.ogm",
        "fa37acf25f708ef700f68de29a1adb8f",
    ),
    (
        "textures\\ui\\ui_mm_window_back_crop.ogm",
        "cfa4ab40a65da6afb25c7bf36265932c",
    ),
    (
        "textures\\ui\\ui_vid_back_01.ogm",
        "481774c40c5f0de06dbb7768e9f06a14",
    ),
    (
        "textures\\ui\\ui_vid_back_02.ogm",
        "0097ac918993df01005f4be2cee87cee",
    ),
    (
        "textures\\ui\\ui_vid_back_03.ogm",
        "3be5f2bccf507b2f5db37dc15bbb7771",
    ),
    (
        "textures\\ui\\ui_vid_back_04.ogm",
        "333dbd875d042edcec132b7c83c0cfff",
    ),
];

#[test]
fn every_video_decodes_like_libtheora() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    // Later archives override earlier ones; the last copy of a path wins.
    let mut videos = BTreeMap::new();
    for cached in &archives {
        let file = BufReader::new(File::open(&cached.file).unwrap());
        let mut archive = Archive::open(file, Scrambling::for_path(&cached.path)).unwrap();
        let entries: Vec<_> = archive
            .entries()
            .iter()
            .filter(|e| e.path.to_ascii_lowercase().ends_with(".ogm"))
            .cloned()
            .collect();
        for entry in entries {
            videos.insert(
                entry.path.to_ascii_lowercase(),
                archive.read(&entry).unwrap(),
            );
        }
    }
    assert_eq!(videos.len(), LIBTHEORA.len(), "video count");

    let mut frames = 0;
    for (path, expected) in LIBTHEORA {
        let data = videos
            .get(*path)
            .unwrap_or_else(|| panic!("{path} missing"));
        let mut video = TheoraVideo::open(Cursor::new(data.as_slice())).unwrap();
        assert_eq!(video.info().chroma, Chroma::Yuv420, "{path}");
        let mut hash = Md5::new();
        let mut count = 0;
        while let Some(frame) = video.next_frame().unwrap() {
            assert_eq!(frame.number, count);
            for plane in &frame.planes {
                for r in 0..plane.height {
                    hash.update(plane.row(r));
                }
            }
            count += 1;
        }
        let actual: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(&actual, expected, "{path}: {count} frames");
        frames += count;
    }
    assert_eq!(frames, 43_564);

    // Rewinding restarts from the first frame.
    let data = &videos["textures\\intro\\intro_thq.ogm"];
    let mut video = TheoraVideo::open(Cursor::new(data.as_slice())).unwrap();
    let first: Vec<u8> = video.next_frame().unwrap().unwrap().planes[0]
        .row(0)
        .to_vec();
    while video.next_frame().unwrap().is_some() {}
    video.rewind().unwrap();
    let again = video.next_frame().unwrap().unwrap();
    assert_eq!(again.number, 0);
    assert_eq!(again.planes[0].row(0), first.as_slice());
}
