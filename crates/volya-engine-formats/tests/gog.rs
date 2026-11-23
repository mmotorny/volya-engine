// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The archive reader against the Russian edition's archives (BYOA), checking
//! the test vectors of docs/formats/db-archive.md (section 8). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "support/gog_archives.rs"]
mod gog_archives;

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

use md5::{Digest, Md5};
use volya_engine_formats::db::{
    Archive, Scrambling, descramble, parse_table, read_directory_chunk,
};

fn md5_hex(data: &[u8]) -> String {
    Md5::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Archive, size, MD5, directory payload stored and decoded size,
/// directories, files.
const ARCHIVES: &[(&str, u64, &str, usize, usize, usize, usize)] = &[
    (
        "gamedata.db0",
        678_577_379,
        "f5d5cd5465ba6425951c95ed98363abd",
        44_920,
        219_261,
        2_379,
        1_195,
    ),
    (
        "gamedata.db1",
        708_434_331,
        "54377855a148e652c63fcb17f91ad324",
        7_264,
        21_772,
        0,
        413,
    ),
    (
        "gamedata.db2",
        671_169_415,
        "4d2b7f30fc25d206e742e142c220ca4b",
        3_651,
        10_402,
        0,
        200,
    ),
    (
        "gamedata.db3",
        684_792_231,
        "c7dee0acaf23b336b02f58b19128e61c",
        5_468,
        17_269,
        0,
        300,
    ),
    (
        "gamedata.db4",
        671_392_842,
        "f65088b47f3ff704d03c190e3c58461f",
        299_340,
        1_207_547,
        0,
        14_800,
    ),
    (
        "gamedata.db5",
        696_277_331,
        "e9bec4aceefcce9782eb18cb24806e05",
        68_441,
        233_224,
        292,
        3_520,
    ),
    (
        "gamedata.db6",
        667_613_134,
        "9b845995d1551620cc1685b024a3b969",
        3_425,
        9_340,
        7,
        170,
    ),
    (
        "gamedata.db7",
        672_770_451,
        "05b59242e2204cd30937bba3eba40a9c",
        42_196,
        135_125,
        17,
        2_570,
    ),
    (
        "gamedata.db8",
        296_000_567,
        "695aae125fdd783ba0abe6192336a840",
        16_947,
        51_611,
        8,
        1_035,
    ),
    (
        "gamedata.db9",
        79_312_307,
        "14d8ba99f592e8561dc5c40322b6d6a4",
        31_388,
        125_094,
        96,
        1_473,
    ),
    (
        "gamedata.dba",
        32_500_627,
        "6199871098b8f5c76a4964a0e76ece75",
        31_867,
        74_506,
        42,
        1_526,
    ),
    (
        "gamedata.dbb",
        55_194_918,
        "43b5002ed9224b3052c3466dbab09c64",
        31_983,
        77_466,
        65,
        1_547,
    ),
    (
        "gamedata.dbc",
        160_948,
        "f123823ace87152638e746cfed5c8d6e",
        323,
        573,
        3,
        11,
    ),
    (
        "gamedata.dbd",
        101_014,
        "9bb19579fb9e7fade0251070165540af",
        210,
        438,
        5,
        6,
    ),
    (
        "mods/mp_military_2.xdb0",
        50_350_518,
        "3c81bfe2c343c9b9a1e1c008b602df89",
        531,
        1_347,
        1,
        24,
    ),
];

/// Archive, descrambled payload MD5, decoded table MD5.
const DIRECTORIES: &[(&str, &str, &str)] = &[
    (
        "gamedata.dbd",
        "163b793c4c2d1c540bc3af1026c126be",
        "f62474358e825f9069188a2ea0631b5e",
    ),
    (
        "gamedata.db0",
        "f0a101fa1a7baeb1a0ef99a9e6b1552e",
        "50578d01b03df9ee7262f352ab80a261",
    ),
    (
        "gamedata.db4",
        "8f0fc97ced5f6e1a29e241565b6f5d56",
        "a1494e398cc20ae8a33139b32e2c2bc5",
    ),
    (
        "mods/mp_military_2.xdb0",
        "9b51991978a5e427d73d4e18f6a7a212",
        "7c38b0ab4ad2d4c88de3ccf920498c52",
    ),
];

/// Archive, path, data offset, size, CRC-32, MD5.
const FILES: &[(&str, &str, u32, u32, u32, &str)] = &[
    (
        "gamedata.dbd",
        "config\\mp\\weapons_mp\\weapons_mp.ltx",
        8,
        57_567,
        0xa5c60df5,
        "0e95eaf004e8652ca9dba98aeeee5621",
    ),
    (
        "gamedata.dbd",
        "config\\ui\\ui_mm_mp_tabclient.xml",
        57_575,
        7_311,
        0xce9244d4,
        "66503c06703e143a98aff6640a9c566f",
    ),
    (
        "gamedata.dbd",
        "config\\ui\\ui_mm_mp_taboptions.xml",
        64_886,
        10_286,
        0xf1855d14,
        "00c9b9ebd997f05538fdc291381f0a14",
    ),
    (
        "gamedata.dbd",
        "scripts\\ui_mm_mp_join.script",
        75_172,
        2_811,
        0x720eb5bd,
        "2959c6c8f154acc1be3de9dcfb16b26d",
    ),
    (
        "gamedata.dbd",
        "scripts\\ui_mm_mp_options.script",
        77_983,
        6_567,
        0x6efd782e,
        "8ac118a2e100329bc4bee724c145e8c3",
    ),
    (
        "gamedata.dbd",
        "scripts\\ui_mp_main.script",
        84_550,
        16_246,
        0x36410070,
        "1aaac179eb3bd60c639d4f35218723e8",
    ),
    (
        "gamedata.db0",
        "config\\system.ltx",
        4_130_936,
        22_829,
        0xeaff4f19,
        "c29b88beb9b146237e6d6c1ce14d0574",
    ),
    (
        "gamedata.dbb",
        "config\\system.ltx",
        3_603_573,
        22_829,
        0x7a606873,
        "31c4df515eadc2d09b456ad622a20809",
    ),
    (
        "gamedata.dbc",
        "config\\system.ltx",
        8,
        22_829,
        0x0a157555,
        "3c35ee63942d827a970e2ddf6096cb40",
    ),
    (
        "gamedata.db0",
        "config\\creatures\\actor.ltx",
        319_696,
        11_293,
        0x934bf7a4,
        "83d22db80a1bf872a1f681e2065e4499",
    ),
    (
        "gamedata.dbb",
        "config\\creatures\\actor.ltx",
        17_186,
        11_359,
        0x92537db7,
        "c35d9476c2d399f493158169168857a5",
    ),
    (
        "gamedata.db4",
        "scripts\\_g.script",
        447_148_550,
        39_542,
        0xc32cb361,
        "c7562102faf72dc95222be7f913533df",
    ),
    (
        "gamedata.db7",
        "textures\\ui\\ui_icon_equipment.dds",
        667_069_639,
        2_097_280,
        0x8dc50bd0,
        "eb05cc55d1ce2dc8cd75719422eee42b",
    ),
    (
        "gamedata.db9",
        "config\\text\\rus\\string_table_general.xml",
        840_653,
        5_741,
        0x81355eda,
        "e58ec94e298a55a0cbfef477b9bd1c3b",
    ),
    (
        "gamedata.dbb",
        "config\\text\\rus\\string_table_general.xml",
        4_449_421,
        5_733,
        0x00a6e9e7,
        "ea3d5d35bb518eefa32a520f205a49eb",
    ),
    (
        "mods/mp_military_2.xdb0",
        "levels\\mp_military_2\\level.game",
        8_108_687,
        1_960,
        0x37be42c9,
        "b3f3577f8ecd502d69f7fb93e182fb77",
    ),
];

fn open(file: &std::path::Path, path: &str) -> Archive<BufReader<File>> {
    Archive::open(
        BufReader::new(File::open(file).unwrap()),
        Scrambling::for_path(path),
    )
    .unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn reads_russian_archives() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let names: Vec<&str> = archives.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(
        names,
        ARCHIVES.iter().map(|a| a.0).collect::<Vec<_>>(),
        "archives listed by the installer"
    );

    let mut opened = HashMap::new();
    let mut signatures = HashMap::new();
    let (mut total_dirs, mut total_files, mut total_bytes) = (0, 0, 0u64);
    let start = Instant::now();
    for (archive, &(path, size, md5, stored, decoded, dirs, files)) in archives.iter().zip(ARCHIVES)
    {
        assert_eq!((archive.size, archive.md5.as_str()), (size, md5), "{path}");

        let mut reader = BufReader::new(File::open(&archive.file).unwrap());
        let chunk = read_directory_chunk(&mut reader).unwrap();
        assert!(chunk.compressed, "{path}");
        assert_eq!(
            chunk.offset + 8 + chunk.payload.len() as u64,
            size,
            "{path}: ends at EOF"
        );
        let table = chunk.decode(Scrambling::for_path(path)).unwrap();
        assert_eq!(
            (chunk.payload.len(), table.len()),
            (stored, decoded),
            "{path}"
        );
        if let Some(&(_, descrambled_md5, table_md5)) = DIRECTORIES.iter().find(|d| d.0 == path) {
            let mut descrambled = chunk.payload.clone();
            if Scrambling::for_path(path) == Scrambling::Scrambled {
                descramble(&mut descrambled);
            }
            assert_eq!(md5_hex(&descrambled), descrambled_md5, "{path}");
            assert_eq!(md5_hex(&table), table_md5, "{path}");
        }
        if path == "gamedata.dbd" {
            assert_eq!(chunk.offset, 100_796);
            assert_eq!(md5_hex(&chunk.payload), "ad998b60d8ec5ae31fd0b977a75a7a11");
            let mut descrambled = chunk.payload.clone();
            descramble(&mut descrambled);
            assert_eq!(
                descrambled[..16],
                [
                    0xb6, 0x01, 0x00, 0x00, 0xd1, 0xe3, 0x25, 0x40, 0x1e, 0xff, 0xdf, 0xeb, 0xe5,
                    0xf5, 0xf9, 0xfa
                ]
            );
            let entries = parse_table(&table).unwrap();
            let dir_paths: Vec<&str> = entries
                .iter()
                .filter(|e| e.is_dir())
                .map(|e| e.path.as_str())
                .collect();
            assert_eq!(
                dir_paths,
                [
                    "config\\",
                    "config\\mp\\",
                    "config\\mp\\weapons_mp\\",
                    "config\\ui\\",
                    "scripts\\"
                ]
            );
            assert_eq!(u16::from_le_bytes([table[0], table[1]]), 23);
        }
        if path == "mods/mp_military_2.xdb0" {
            assert_eq!(chunk.offset, 50_349_979);
            assert_eq!(chunk.payload[..4], [0x43, 0x05, 0x00, 0x00]);
        }

        let mut opened_archive = open(&archive.file, path);
        let entries = opened_archive.entries().to_vec();
        let dir_count = entries.iter().filter(|e| e.is_dir()).count();
        assert_eq!(
            (dir_count, entries.len() - dir_count),
            (dirs, files),
            "{path}"
        );
        total_dirs += dir_count;
        total_files += entries.len() - dir_count;

        // Every file reads, with its checksum, and the stored ranges cover
        // the data chunk payload exactly.
        let mut ranges = Vec::new();
        for entry in entries.iter().filter(|e| !e.is_dir()) {
            let data = opened_archive
                .read(entry)
                .unwrap_or_else(|e| panic!("{path}: {}: {e}", entry.path));
            assert_eq!(
                data.len(),
                entry.original_size as usize,
                "{path}: {}",
                entry.path
            );
            assert_eq!(
                crc32fast::hash(&data),
                entry.checksum,
                "{path}: {}",
                entry.path
            );
            total_bytes += data.len() as u64;
            let ext = entry.path.rsplit_once('.').map_or("", |(_, ext)| ext);
            let magic: &[u8] = match ext {
                "ogg" | "ogm" => b"OggS",
                "dds" => b"DDS ",
                _ => b"",
            };
            assert!(data.starts_with(magic), "{path}: {}: signature", entry.path);
            if !magic.is_empty() {
                *signatures.entry(ext.to_owned()).or_insert(0) += 1;
            }
            if entry.stored_size > 0 {
                ranges.push((entry.offset as u64, entry.stored_size as u64));
            }
        }
        ranges.sort_unstable();
        ranges.dedup();
        let mut covered = 8;
        for (offset, len) in ranges {
            assert_eq!(offset, covered, "{path}: gap or overlap");
            covered += len;
        }
        assert_eq!(covered, chunk.offset, "{path}: data chunk coverage");
        opened.insert(path, opened_archive);
    }
    assert_eq!((total_dirs, total_files), (2_915, 28_790));
    assert_eq!(
        signatures,
        HashMap::from([
            ("ogg".into(), 16_308),
            ("ogm".into(), 39),
            ("dds".into(), 5_602)
        ])
    );
    eprintln!(
        "read {total_files} files ({total_bytes} bytes) from {} archives in {:.1?}",
        archives.len(),
        start.elapsed()
    );

    for &(archive, path, offset, size, crc, md5) in FILES {
        let opened = opened.get_mut(archive).unwrap();
        let entry = opened
            .find(path)
            .unwrap_or_else(|| panic!("{archive}: {path}"))
            .clone();
        assert_eq!(
            (
                entry.offset,
                entry.original_size,
                entry.stored_size,
                entry.checksum
            ),
            (offset, size, size, crc),
            "{archive}: {path}"
        );
        assert_eq!(
            md5_hex(&opened.read(&entry).unwrap()),
            md5,
            "{archive}: {path}"
        );
    }
}
