// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use volya_engine_installer::{Installer, extract, hex};

const USAGE: &str = "\
Usage: volya-engine-installer list <setup.exe> [--lang LANG]
       volya-engine-installer extract <setup.exe> <out-dir> [--lang LANG]

Lists or extracts the game files of a GOG offline installer for LANG
(default: ru-RU). The data slice is read from <setup.exe stem>-1.bin.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("volya-engine-installer: {error}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut lang = "ru-RU".to_owned();
    if let Some(i) = args.iter().position(|a| a == "--lang") {
        lang = args.get(i + 1).ok_or("--lang needs a value")?.clone();
        args.drain(i..=i + 1);
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["list", exe] => {
            let installer = parse(exe)?;
            let mut total = 0;
            for file in installer.game_files(&lang) {
                println!(
                    "{:>12}  {:>3} parts  {}  {}",
                    file.size,
                    file.parts.len(),
                    hex(&file.md5),
                    file.path
                );
                total += file.size;
            }
            println!("{total:>12}  total");
            Ok(())
        }
        ["extract", exe, out] => {
            let installer = parse(exe)?;
            let mut bin =
                BufReader::new(File::open(bin_path(exe)?).map_err(|e| format!("open .bin: {e}"))?);
            let start = Instant::now();
            let mut total = 0;
            for file in installer.game_files(&lang) {
                let path = Path::new(out).join(&file.path);
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                let mut writer = BufWriter::new(
                    File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?,
                );
                extract(file, &mut bin, &mut writer).map_err(|e| e.to_string())?;
                writer.flush().map_err(|e| e.to_string())?;
                total += file.size;
                println!("{}", file.path);
            }
            let secs = start.elapsed().as_secs_f64();
            println!(
                "{} MiB in {secs:.1} s ({:.0} MiB/s), all MD5s verified",
                total >> 20,
                (total >> 20) as f64 / secs
            );
            Ok(())
        }
        _ => Err("expected a command".into()),
    }
}

fn parse(exe: &str) -> Result<Installer, String> {
    Installer::parse(BufReader::new(
        File::open(exe).map_err(|e| format!("{exe}: {e}"))?,
    ))
    .map_err(|e| e.to_string())
}

fn bin_path(exe: &str) -> Result<PathBuf, String> {
    let exe = Path::new(exe);
    let stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("bad .exe path")?;
    Ok(exe.with_file_name(format!("{stem}-1.bin")))
}
