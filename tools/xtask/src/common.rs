// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What xtask's tasks and the capture tool (`capture.rs`) share: the source
//! tree, runfiles, and running commands.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub type Result<T = ()> = std::result::Result<T, String>;

/// The directory this process started in: under `bazel run`, its runfiles'
/// tree, which runfiles paths (`$(rootpath)`) are relative to.
pub static START_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Records the start directory, then, under `bazel run`, moves to the
/// caller's (`BUILD_WORKING_DIRECTORY`), so paths given are theirs
/// (commands still run in the workspace, [`cmd`]).
pub fn start() {
    let _ = START_DIR.set(std::env::current_dir().unwrap_or_default());
    if let Some(dir) = std::env::var_os("BUILD_WORKING_DIRECTORY") {
        let _ = std::env::set_current_dir(dir);
    }
}

/// A runfiles path (`$(rootpath)`, as Bazel passes them) as a path.
pub fn runfile(path: &Path) -> PathBuf {
    START_DIR
        .get()
        .map_or_else(|| path.to_path_buf(), |d| d.join(path))
}

/// The source tree: the workspace under `bazel run`, else Cargo's
/// workspace, else (a Bazel test or action) the directory this process
/// started in, whose relative paths are the tree's.
pub fn root() -> PathBuf {
    if let Some(dir) = std::env::var_os("BUILD_WORKSPACE_DIRECTORY") {
        return PathBuf::from(dir);
    }
    let cargo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask lives at tools/xtask")
        .to_path_buf();
    if std::env::var_os("TEST_SRCDIR").is_none() && cargo.join("Cargo.toml").is_file() {
        return cargo;
    }
    START_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().expect("a directory"))
}

pub fn step(name: &str) {
    println!("\n==> {name}");
}

pub fn cmd(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.args(args).current_dir(root());
    command
}

pub fn run(command: &mut Command) -> Result {
    let status = command
        .status()
        .map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed with {status}"))
    }
}

pub fn output(command: &mut Command) -> Result<String> {
    let out = command
        .output()
        .map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if !out.status.success() {
        return Err(format!(
            "{command:?} failed with {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
