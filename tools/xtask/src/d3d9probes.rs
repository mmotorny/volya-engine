// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Direct3D 9 probes (`tests/d3d9probes/`, docs/testing.md): built with
//! llvm-mingw and packed into the Windows runner zip (Bazel's actions,
//! build/d3d9probes.bzl, through `d3d9probes_action.rs`), and that zip
//! published as a GitHub release (`xtask d3d9probes release`).

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::{Result, cmd, output, run, step};

/// The flags the probes were written against, statically linked, and no
/// link timestamp, so the same sources give the same executable.
const FLAGS: &[&str] = &[
    "-O2",
    "-static",
    "-Wall",
    "-Wextra",
    "-Wl,--no-insert-timestamp",
];

/// The runner zip's Bazel target and the release tags' prefix.
pub const RUNNER: &str = "//tests/d3d9probes:runner";
const TAG_PREFIX: &str = "d3d9probes-";

fn copy(from: &Path, to: &Path) -> Result {
    fs::copy(from, to)
        .map(drop)
        .map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

/// `build CLANG OUT SRC...`: the probes' C sources compiled by `clang`
/// (llvm-mingw's `i686-w64-mingw32-clang`) into `OUT`.
pub fn build(clang: &Path, out: &Path, sources: &[&str]) -> Result {
    let mut command = Command::new(clang);
    command
        .args(FLAGS)
        .arg("-o")
        .arg(out)
        .args(sources.iter().filter(|s| s.ends_with(".c")))
        .arg("-ld3d9");
    run(&mut command)
}

/// `references OUT NAMES DEST=SRC...`: the comparison suite's references
/// (`//tests/d3d9probes:references`, served to `d3d9probes.js`): each SRC
/// (a file, or a folder copied whole) at `OUT/d3d9probes/DEST`, and
/// `OUT/d3d9probes/references.json` listing NAMES (comma-separated), the
/// references in the order the suite compares with them.
pub fn references(out: &Path, names: &str, pairs: &[&str]) -> Result {
    let root = out.join("d3d9probes");
    for pair in pairs {
        let (dest, src) = pair
            .split_once('=')
            .ok_or_else(|| format!("{pair}: not DEST=SRC"))?;
        copy_all(Path::new(src), &root.join(dest))?;
    }
    let list: Vec<String> = names.split(',').map(|n| format!("{n:?}")).collect();
    let index = root.join("references.json");
    fs::write(&index, format!("[{}]\n", list.join(", ")))
        .map_err(|e| format!("{}: {e}", index.display()))
}

/// `from` at `to`: a file copied, a folder with everything in it.
fn copy_all(from: &Path, to: &Path) -> Result {
    let made = |dir: &Path| fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()));
    if !from.is_dir() {
        made(to.parent().ok_or("no folder")?)?;
        return copy(from, to);
    }
    made(to)?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        copy_all(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// `zip OUT FILE...`: the files, by their names, in a zip (stored, no
/// timestamps), so the same files give the same zip.
pub fn zip(out: &Path, files: &[&str]) -> Result {
    let mut entries = Vec::new();
    for file in files {
        let path = Path::new(file);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("no file name")?;
        let bytes = fs::read(path).map_err(|e| format!("{file}: {e}"))?;
        entries.push((name.to_string(), bytes));
    }
    fs::write(out, zip_bytes(&entries)).map_err(|e| format!("{}: {e}", out.display()))
}

/// A zip of `entries` (name, contents), stored uncompressed, dated
/// 1980-01-01 (DOS time 0 is not a date).
fn zip_bytes(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    const DOS_DATE: u16 = 1 << 5 | 1;
    let mut zip = Vec::new();
    let mut central = Vec::new();
    let u16le = |v: &mut Vec<u8>, x: u16| v.extend(x.to_le_bytes());
    let u32le = |v: &mut Vec<u8>, x: u32| v.extend(x.to_le_bytes());
    for (name, bytes) in entries {
        let offset = zip.len() as u32;
        let crc = crc32(bytes);
        let size = bytes.len() as u32;
        // Version needed, flags, method (stored), time, date, CRC, sizes,
        // name length, extra length.
        let common = |v: &mut Vec<u8>| {
            u16le(v, 10);
            u16le(v, 0);
            u16le(v, 0);
            u16le(v, 0);
            u16le(v, DOS_DATE);
            u32le(v, crc);
            u32le(v, size);
            u32le(v, size);
            u16le(v, name.len() as u16);
            u16le(v, 0);
        };
        u32le(&mut zip, 0x0403_4b50);
        common(&mut zip);
        zip.extend(name.as_bytes());
        zip.extend(bytes);
        u32le(&mut central, 0x0201_4b50);
        u16le(&mut central, 20); // made by: MS-DOS, 2.0
        common(&mut central);
        u16le(&mut central, 0); // comment length
        u16le(&mut central, 0); // disk
        u16le(&mut central, 0); // internal attributes
        u32le(&mut central, 0); // external attributes
        u32le(&mut central, offset);
        central.extend(name.as_bytes());
    }
    let start = zip.len() as u32;
    let size = central.len() as u32;
    zip.extend(central);
    u32le(&mut zip, 0x0605_4b50);
    u16le(&mut zip, 0);
    u16le(&mut zip, 0);
    u16le(&mut zip, entries.len() as u16);
    u16le(&mut zip, entries.len() as u16);
    u32le(&mut zip, size);
    u32le(&mut zip, start);
    u16le(&mut zip, 0);
    zip
}

/// CRC-32 (IEEE, reflected), as zip files carry.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                crc >> 1 ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The release tag of a runner zip: `prefix` and the start of its
/// SHA-256, so the same runner is published once.
fn tag(prefix: &str, zip: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(zip)[..6]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{prefix}{hash}")
}

/// `xtask d3d9probes release`: publishes the runner zip `zip` (built by
/// [`RUNNER`]) as [`publish`] does.
pub fn release(zip: &Path) -> Result {
    let notes = format!(
        "The Direct3D 9 probes' Windows runner (tests/d3d9probes/, built by \
         `{RUNNER}`). Steps: docs/testing.md, Direct3D 9 probes on real hardware."
    );
    publish(zip, TAG_PREFIX, "d3d9probes.zip", &notes)
}

/// Publishes the zip `zip` as the asset `asset` of a prerelease tagged by
/// `prefix` and its hash ([`tag`]), at the default branch's head, unless
/// that release exists; prints the tag and the asset's URL.
pub fn publish(zip: &Path, prefix: &str, asset: &str, notes: &str) -> Result {
    let bytes = fs::read(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    let tag = tag(prefix, &bytes);
    let repo = output(&mut cmd(
        "gh",
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "--jq",
            ".nameWithOwner",
        ],
    ))?;
    let repo = repo.trim();
    let exists = cmd("gh", &["release", "view", &tag, "--repo", repo])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if exists {
        step(&format!("Release {tag} exists"));
    } else {
        step(&format!("Publishing release {tag}"));
        let file = format!("{}#{asset}", zip.display());
        run(&mut cmd(
            "gh",
            &[
                "release",
                "create",
                &tag,
                &file,
                "--repo",
                repo,
                "--prerelease",
                "--title",
                &tag,
                "--notes",
                notes,
            ],
        ))?;
    }
    println!("{tag}\nhttps://github.com/{repo}/releases/download/{tag}/{asset}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_of_the_check_string() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn zip_layout() {
        let zip = zip_bytes(&[("a.txt".into(), b"hi".to_vec())]);
        // Local header, name, data; central header, name; end record.
        assert_eq!(zip.len(), 30 + 5 + 2 + 46 + 5 + 22);
        assert_eq!(&zip[..4], b"PK\x03\x04");
        assert_eq!(&zip[30..37], b"a.txthi");
        let end = &zip[zip.len() - 22..];
        assert_eq!(&end[..4], b"PK\x05\x06");
        assert_eq!(u32::from_le_bytes(end[16..20].try_into().unwrap()), 37);
    }
}
