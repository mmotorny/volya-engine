// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `gog-extract EXE OUT`: the installer's `ru-RU` game files into
//! `OUT/files/` in the game-root layout, each checked against the MD5 the
//! installer records, and `OUT/manifest.tsv` (path, size, MD5; installer
//! order). What `@gog_game` holds (docs/architecture/build.md, Game data).

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use volya_engine_installer::{Installer, extract, hex};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [exe, out] = args.as_slice() else {
        eprintln!("usage: gog-extract EXE OUT");
        return ExitCode::FAILURE;
    };
    match run(Path::new(exe), Path::new(out)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gog-extract: {error}");
            ExitCode::FAILURE
        }
    }
}

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
