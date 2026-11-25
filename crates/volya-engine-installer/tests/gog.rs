// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Tests against a real GOG installer (BYOA): `VOLYA_GOG_INSTALLER` names the
//! directory holding the Russian edition's `.exe` and `-1.bin`.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use volya_engine_installer::{Installer, extract};

fn installer() -> Option<(PathBuf, PathBuf)> {
    let Some(dir) = std::env::var_os("VOLYA_GOG_INSTALLER") else {
        eprintln!("skipped: VOLYA_GOG_INSTALLER is unset");
        return None;
    };
    let exe = std::fs::read_dir(&dir)
        .expect("VOLYA_GOG_INSTALLER")
        .map(|e| e.unwrap().path())
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("setup_s.t.a.l.k.e.r._shadow_of_chernobyl") && name.ends_with(".exe")
        })
        .expect("no Shadow of Chernobyl installer .exe");
    let bin = exe.with_file_name(format!(
        "{}-1.bin",
        exe.file_stem().unwrap().to_string_lossy()
    ));
    Some((exe, bin))
}

#[test]
fn lists_and_extracts_game_files() {
    let Some((exe, bin)) = installer() else {
        return;
    };
    let parsed = Installer::parse(BufReader::new(File::open(exe).unwrap())).unwrap();
    let files: Vec<_> = parsed.game_files("ru-RU").collect();
    let names: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for archive in [
        "gamedata.db0",
        "gamedata.db9",
        "gamedata.dba",
        "gamedata.dbd",
    ] {
        assert!(names.contains(&archive), "{archive} missing from {names:?}");
    }
    // Every part but the last inflates to exactly 10 MiB; the last takes the
    // remainder (observed up to ~18 MB).
    for file in &files {
        let (last, rest) = file.parts.split_last().unwrap();
        assert!(rest.iter().all(|p| p.size == 10 << 20), "{}", file.path);
        assert!(last.size < 20 << 20, "{}", file.path);
    }

    // Small archives, end to end; extract() verifies every MD5.
    let mut bin = BufReader::new(File::open(bin).unwrap());
    for name in ["gamedata.dbc", "gamedata.dbd"] {
        let file = files.iter().find(|f| f.path == name).unwrap();
        let mut out = Vec::new();
        extract(file, &mut bin, &mut out).unwrap();
        assert_eq!(out.len() as u64, file.size);
    }
}

#[test]
fn reads_the_installer_identity() {
    let Some((exe, _)) = installer() else {
        return;
    };
    let parsed = Installer::parse(BufReader::new(File::open(exe).unwrap())).unwrap();
    assert_eq!(
        parsed.app_name.as_deref(),
        Some("S.T.A.L.K.E.R. - Shadow of Chernobyl")
    );
    // docs/formats/gog-installer.md: GOG's installer version.
    assert!(
        parsed
            .app_version
            .as_deref()
            .is_some_and(|v| v.starts_with("2.1.0.10 ")),
        "{:?}",
        parsed.app_version
    );
    // Its registry entries write no `InstallVers` under the title menu's
    // key: the menu shows the built-in version.
    assert_eq!(parsed.install_vers, None);
}
