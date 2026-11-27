// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Direct3D 9 probes (`tests/d3d9probes/`, docs/testing.md): built with
//! MinGW and run under Wine in the Wine image (Bazel's actions,
//! build/d3d9probes.bzl, through `d3d9probes_action.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::Result;
use crate::wine::Container;

/// The executable's name, in the zip and in the container's working folder.
const EXE: &str = "d3d9probes.exe";

/// The build in the container: the flags the probes were written against,
/// and no link timestamp, so the same sources give the same executable.
const BUILD: &str = "i686-w64-mingw32-gcc -O2 -static -Wall -Wextra \
                     -Wl,--no-insert-timestamp -o d3d9probes.exe *.c -ld3d9";

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
