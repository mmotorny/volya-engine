// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Every `.dds` texture of the Russian edition parses, and the format census
//! matches #26. Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::BufReader;

use volya_engine_formats::db::{Archive, Entry, Scrambling};
use volya_engine_formats::dds::{Dds, Format, Masks};

#[test]
fn every_texture_parses() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    // The game's own archives, as counted in #26 (without `mods/`).
    let mut opened: Vec<_> = archives
        .iter()
        .filter(|a| !a.path.starts_with("mods/"))
        .map(|a| {
            let file = BufReader::new(File::open(&a.file).unwrap());
            Archive::open(file, Scrambling::for_path(&a.path)).unwrap()
        })
        .collect();
    // Later archives override earlier ones; the last copy of a path wins.
    let mut textures: BTreeMap<String, (usize, Entry)> = BTreeMap::new();
    for (i, archive) in opened.iter().enumerate() {
        for entry in archive.entries() {
            let path = entry.path.to_ascii_lowercase();
            if path.ends_with(".dds") {
                textures.insert(path, (i, entry.clone()));
            }
        }
    }
    assert_eq!(textures.len(), 5306, "distinct textures");

    let mut formats: HashMap<&str, usize> = HashMap::new();
    let (mut cubes, mut single_level, mut trailing) = (0, 0, 0);
    let mut largest = 0;
    for (path, (i, entry)) in &textures {
        let bytes = opened[*i].read(entry).unwrap();
        let dds = Dds::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        let name = match dds.format {
            Format::Bc1 => "BC1",
            Format::Bc2 => "BC2",
            Format::Bc3 => "BC3",
            Format::Masked(Masks::BGRA8) => "BGRA8",
            Format::Masked(Masks { bits: 16, .. }) => "16-bit",
            Format::Masked(Masks { bits: 8, .. }) => "8-bit",
            Format::Masked(m) => panic!("{path}: unexpected masks {m:?}"),
        };
        *formats.entry(name).or_default() += 1;
        assert!(
            dds.width.is_power_of_two() && dds.height.is_power_of_two(),
            "{path}"
        );
        if dds.cube {
            assert_eq!(dds.faces, 6, "{path}");
            cubes += 1;
        }
        single_level += usize::from(dds.mip_levels == 1);
        trailing += usize::from(bytes.len() != 128 + dds.data_len());
        largest = largest.max(dds.width.max(dds.height));
    }
    let count = |name| formats.get(name).copied().unwrap_or(0);
    assert_eq!(
        [
            count("BC3"),
            count("BC1"),
            count("BC2"),
            count("BGRA8"),
            count("16-bit"),
            count("8-bit")
        ],
        [2614, 2046, 416, 225, 4, 1],
        "{formats:?}"
    );
    assert_eq!((cubes, single_level, largest), (168, 1321, 2048));
    eprintln!("{trailing} textures have bytes after the data");
}
