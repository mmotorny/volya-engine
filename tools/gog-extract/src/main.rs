// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game data's repositories (docs/architecture/build.md, Game data):
//!
//! - `gog-extract EXE OUT`: the installer's `ru-RU` game files into
//!   `OUT/files/` in the game-root layout, each checked against the MD5 the
//!   installer records, and `OUT/manifest.tsv` (path, size, MD5; installer
//!   order). What `@gog_game` holds.
//! - `gog-extract install FILES OUT`: the original's install folder,
//!   `OUT/game/`: the game files `FILES` cloned (APFS copy-on-write), plus
//!   what the install adds to its folder (docs/formats/
//!   gog-install-additions.md: `fsgame.ltx`, the empty `userdocs` folders),
//!   and `OUT/manifest.tsv` (the files' manifest, then the additions'
//!   paths and sizes). What `@gog_original` holds; each run adds the user
//!   folder (`xtask original`).

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

use volya_engine_installer::{Installer, extract, hex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [mode, files, out] if mode == "install" => install(Path::new(files), Path::new(out)),
        [exe, out] => run(Path::new(exe), Path::new(out)),
        _ => Err("usage: gog-extract EXE OUT | gog-extract install FILES OUT".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gog-extract: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The Wine user (the container runs as root; measured: the prefix's only
/// user folder is `users/root`).
const WINE_USER: &str = "root";

/// The current user's Documents folder, `{userdocs}`, as Windows sees it
/// (gog-install-additions.md section 2: an absolute path without trailing
/// `\`).
fn userdocs() -> String {
    format!("C:\\users\\{WINE_USER}\\Documents")
}

/// The generated `fsgame.ltx` (gog-install-additions.md section 2): 17
/// LF-terminated ASCII lines, `{userdocs}` the only placeholder.
const FSGAME_LTX: &str = "\
$app_data_root$ = true | false| {userdocs}\\Stalker-SHOC
$game_data$ =     false| true | $fs_root$      | gamedata\\
$game_ai$ =       true | false| $game_data$    | ai\\
$game_spawn$ =    true | false| $game_data$    | spawns\\
$game_anims$ =    true | true | $game_data$    | anims\\
$game_levels$ =   true | false| $game_data$    | levels\\
$game_meshes$ =   true | true | $game_data$    | meshes\\
$game_dm$ =       true | true | $game_data$    | meshes\\
$game_shaders$ =  true | true | $game_data$    | shaders\\
$game_sounds$ =   true | true | $game_data$    | sounds\\
$game_textures$ = true | true | $game_data$    | textures\\
$game_scripts$ =  true | false| $game_data$    | scripts\\
$game_config$ =   true | false| $game_data$    | config\\
$level$ =         false| false| $game_levels$
$game_saves$ =    true | false| $app_data_root$| savedgames\\
$logs$ =          true | false| $app_data_root$| logs\\
$screenshots$ =   true | false| $app_data_root$| screenshots\\
";

/// An I/O error at `path`, as a message.
fn at(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

fn run(exe: &Path, out: &Path) -> Result<(), String> {
    let installer =
        Installer::parse(BufReader::new(File::open(exe).map_err(at(exe))?)).map_err(at(exe))?;
    let bin = exe.with_file_name(format!(
        "{}-1.bin",
        exe.file_stem().unwrap_or_default().to_string_lossy()
    ));
    let mut bin_reader = BufReader::new(File::open(&bin).map_err(at(&bin))?);
    let files = out.join("files");
    let start = Instant::now();
    let mut manifest = String::new();
    let mut total = 0;
    for file in installer.game_files("ru-RU") {
        let path = files.join(&file.path);
        fs::create_dir_all(path.parent().expect("a file's directory")).map_err(at(&path))?;
        let mut writer = BufWriter::new(File::create(&path).map_err(at(&path))?);
        extract(file, &mut bin_reader, &mut writer).map_err(at(&path))?;
        writer.flush().map_err(at(&path))?;
        manifest.push_str(&format!(
            "{}\t{}\t{}\n",
            file.path,
            file.size,
            hex(&file.md5)
        ));
        total += file.size;
    }
    let path = out.join("manifest.tsv");
    fs::write(&path, manifest).map_err(at(&path))?;
    eprintln!(
        "gog-extract: {total} bytes into {} in {:.1?}",
        files.display(),
        start.elapsed()
    );
    Ok(())
}

/// The original's install folder from the game files `files` (see the
/// crate's documentation).
fn install(files: &Path, out: &Path) -> Result<(), String> {
    let game = out.join("game");
    let status = Command::new("cp")
        .arg("-cR")
        .arg(files)
        .arg(&game)
        .status()
        .map_err(|e| format!("cp: {e}"))?;
    if !status.success() {
        return Err(format!(
            "cp -cR {} {}: {status}",
            files.display(),
            game.display()
        ));
    }
    let manifest_in = files.parent().unwrap_or(files).join("manifest.tsv");
    let mut manifest = fs::read_to_string(&manifest_in).map_err(at(&manifest_in))?;
    // Section 2, then `attrib +r`: Wine shows a file without write
    // permission as read-only.
    let fsgame = game.join("fsgame.ltx");
    let text = FSGAME_LTX.replace("{userdocs}", &userdocs());
    fs::write(&fsgame, &text).map_err(at(&fsgame))?;
    let mut permissions = fs::metadata(&fsgame).map_err(at(&fsgame))?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&fsgame, permissions).map_err(at(&fsgame))?;
    manifest.push_str(&format!("fsgame.ltx\t{}\t\n", text.len()));
    // Section 6: empty folders.
    for dir in ["downloads", "logs", "savedgames"] {
        let dir = game.join("userdocs").join(dir);
        fs::create_dir_all(&dir).map_err(at(&dir))?;
        manifest.push_str(&format!(
            "userdocs/{}/\t0\t\n",
            dir.file_name().unwrap().to_string_lossy()
        ));
    }
    let path = out.join("manifest.tsv");
    fs::write(&path, manifest).map_err(at(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fsgame_ltx_is_the_specified_file() {
        let text = FSGAME_LTX.replace("{userdocs}", &userdocs());
        assert!(text.is_ascii() && !text.contains('\r'));
        assert_eq!(FSGAME_LTX.len(), 965);
        assert_eq!(text.lines().count(), 17);
        assert!(text.starts_with(
            "$app_data_root$ = true | false| C:\\users\\root\\Documents\\Stalker-SHOC\n"
        ));
        assert!(text.ends_with("| screenshots\\\n"));
    }
}
