// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Dev tool: lists `.db` / `.xdb` archives and dumps their entries.

use std::fs::File;
use std::io::{BufReader, Write};
use std::process::ExitCode;

use volya_engine_formats::db::{Archive, Scrambling};

const USAGE: &str = "\
Usage: volya-engine-db list <archive>
       volya-engine-db dump <archive> <path>

`list` prints every entry: original size, stored size, CRC-32, data offset
and path (directories end in `\\`). `dump` writes the contents of the entry
with stored path <path> (e.g. `config\\system.ltx`) to stdout.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("volya-engine-db: {error}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn open(path: &str) -> Result<Archive<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| format!("{path}: {e}"))?;
    Archive::open(BufReader::new(file), Scrambling::for_path(path))
        .map_err(|e| format!("{path}: {e}"))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["list", path] => {
            let archive = open(path)?;
            let mut out = std::io::stdout().lock();
            for e in archive.entries() {
                writeln!(
                    out,
                    "{:>10} {:>10} {:08x} {:>10}  {}",
                    e.original_size, e.stored_size, e.checksum, e.offset, e.path
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        ["dump", path, entry] => {
            let mut archive = open(path)?;
            let entry = archive
                .find(entry)
                .ok_or_else(|| format!("{path}: no entry {entry}"))?
                .clone();
            let data = archive.read(&entry).map_err(|e| e.to_string())?;
            std::io::stdout()
                .lock()
                .write_all(&data)
                .map_err(|e| e.to_string())
        }
        _ => Err("bad arguments".into()),
    }
}
