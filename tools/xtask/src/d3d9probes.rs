// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Direct3D 9 probes (`tests/d3d9probes/`, docs/testing.md): built with
//! MinGW and run under Wine in the Wine image (Bazel's actions,
//! build/d3d9probes.bzl, through `d3d9probes_action.rs`), packed into the
//! Windows runner zip, and that zip published as a GitHub release
//! (`xtask d3d9probes release`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::wine::Container;
use crate::{Result, cmd, output, run, step};

/// The executable's name, in the zip and in the container's working folder.
const EXE: &str = "d3d9probes.exe";

/// The build in the container: the flags the probes were written against,
/// and no link timestamp, so the same sources give the same executable.
const BUILD: &str = "i686-w64-mingw32-gcc -O2 -static -Wall -Wextra \
                     -Wl,--no-insert-timestamp -o d3d9probes.exe *.c -ld3d9";

/// The runner zip's Bazel target and the release tags' prefix.
pub const RUNNER: &str = "//tests/d3d9probes:runner";
const TAG_PREFIX: &str = "d3d9probes-";

/// A fresh folder `<out>.work` beside `out`, for the container's mount.
fn work_dir(out: &Path) -> Result<PathBuf> {
    let mut name = out.file_name().ok_or("no file name")?.to_os_string();
    name.push(".work");
    let dir = out.with_file_name(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::canonicalize(&dir).map_err(|e| format!("{}: {e}", dir.display()))
}

fn copy(from: &Path, to: &Path) -> Result {
    fs::copy(from, to)
        .map(drop)
        .map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

/// Runs `tail` in a Wine container with `dir` at /work, its output to
/// `log` (and the container's own errors to ours); fails unless it exits
/// with 0.
fn in_container(dir: &Path, tail: &str, log: &Path) -> Result {
    let file = fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = file.try_clone().map_err(|e| e.to_string())?;
    let mut container = Container::spawn(dir, tail, &[], Stdio::from(file), Stdio::from(err))?;
    let status = container.wait()?;
    drop(container);
    if status.success() {
        Ok(())
    } else {
        let text = fs::read_to_string(log).unwrap_or_default();
        Err(format!("{status}:\n{text}"))
    }
}

/// `build OUT SRC...`: the probes' sources compiled into `OUT`.
pub fn build(out: &Path, sources: &[&str]) -> Result {
    let dir = work_dir(out)?;
    for source in sources {
        let source = Path::new(source);
        copy(source, &dir.join(source.file_name().ok_or("no file name")?))?;
    }
    in_container(&dir, BUILD, &dir.join("build.log"))?;
    copy(&dir.join(EXE), out)?;
    fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// `run EXE OUT`: the probes run under Wine in `OUT`, leaving
/// `OUT/results/` and the program's output, `OUT/log.txt`.
pub fn run_probes(exe: &Path, out: &Path) -> Result {
    let _ = fs::remove_dir_all(out);
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let dir = fs::canonicalize(out).map_err(|e| format!("{}: {e}", out.display()))?;
    copy(exe, &dir.join(EXE))?;
    let result = in_container(
        &dir,
        "xvfb-run -a wine d3d9probes.exe",
        &dir.join("log.txt"),
    );
    fs::remove_file(dir.join(EXE)).map_err(|e| format!("{EXE}: {e}"))?;
    result
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

/// The release tag of a runner zip: the prefix and the start of its SHA-256,
/// so the same runner is published once.
fn tag(zip: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(zip)[..6]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{TAG_PREFIX}{hash}")
}

/// `xtask d3d9probes release`: publishes the runner zip `zip` (built by
/// [`RUNNER`]) as the asset `d3d9probes.zip` of a prerelease tagged by its
/// hash ([`tag`]), at the default branch's head, unless that release
/// exists; prints the tag and the zip's URL.
pub fn release(zip: &Path) -> Result {
    let bytes = fs::read(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    let tag = tag(&bytes);
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
        let asset = format!("{}#d3d9probes.zip", zip.display());
        let notes = format!(
            "The Direct3D 9 probes' Windows runner (tests/d3d9probes/, built by \
             `{RUNNER}`). Steps: docs/testing.md, Direct3D 9 probes on real hardware."
        );
        run(&mut cmd(
            "gh",
            &[
                "release",
                "create",
                &tag,
                &asset,
                "--repo",
                repo,
                "--prerelease",
                "--title",
                &tag,
                "--notes",
                &notes,
            ],
        ))?;
    }
    println!("{tag}\nhttps://github.com/{repo}/releases/download/{tag}/d3d9probes.zip");
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
