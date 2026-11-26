// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `bazel run //tools/xtask -- parity [REV]`: the test app (Bazel's
//! `//platforms/web:site_test`) built at a git revision (a detached worktree
//! under `target/parity/`, its own Bazel workspace sharing the caches) and
//! from the working tree, and
//! the `parity` suite run on each in headless Chrome, one after the other,
//! on the same origin and kept profile: the first run keeps its views'
//! frames, the second compares its own with them pixel by pixel
//! (`render-game.js`) and reports the pixels that differ
//! (docs/architecture/rendering.md, The parity test).

use std::fs;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use volya_engine_devserver::{BASE, Server};

use volya_engine_site::copy_dir;

use super::{Result, cmd, root, step};
use crate::e2e;

const SUITE: &str = "parity";
const TIMEOUT: Duration = Duration::from_secs(1500);

/// Runs the parity test of the working tree against the revision in `args`
/// (`HEAD` by default: with no changes, a check that the views repeat).
pub fn run(args: &[&str]) -> Result {
    let rev = match args {
        [] => "HEAD",
        [rev] => rev,
        _ => return Err(format!("unknown arguments {args:?}; expected a revision")),
    };
    // Resolved in the working tree: inside the parity worktree `HEAD`
    // would name its own last checkout.
    let rev = super::output(&mut cmd("git", &["rev-parse", "--verify", rev]))?;
    let rev = rev.trim();
    crate::machine::wait_memory()?;
    let dir = root().join("target/parity");
    let src = dir.join("src");
    let first = dir.join("build-base");
    let second = dir.join("build-tree");
    step(&format!("Parity: the build at {rev}"));
    let src_arg = src.to_string_lossy();
    if src.exists() {
        super::run(&mut cmd(
            "git",
            &[
                "-C", &src_arg, "checkout", "--quiet", "--force", "--detach", rev,
            ],
        ))?;
    } else {
        super::run(&mut cmd(
            "git",
            &["worktree", "add", "--force", "--detach", &src_arg, rev],
        ))?;
    }
    copy_site(&src, &first)?;
    step("Parity: the working tree's build");
    copy_site(&root(), &second)?;
    let frames_first = dir.join("frames-base");
    let frames_second = dir.join("frames-tree");
    for frames in [&frames_first, &frames_second] {
        if frames.exists() {
            fs::remove_dir_all(frames).map_err(|e| format!("{}: {e}", frames.display()))?;
        }
    }

    let (tx, reports) = mpsc::channel();
    let server = Server::new(&first)
        .and_then(|s| s.with_next(&second))
        .and_then(|s| {
            s.with_tests(
                &root().join("tests/browser"),
                Arc::new(move |report| {
                    let _ = tx.send(report);
                }),
            )
        })
        .and_then(|s| match std::env::var_os("VOLYA_GOG_INSTALLER") {
            Some(dir) => s.with_installer(std::path::Path::new(&dir)),
            None => Ok(s),
        })
        .map_err(|e| format!("dev server: {e}"))?;
    let server = Arc::new(server);
    let profile = e2e::KeptProfile::take()?;
    let (port, listeners) = e2e::listen(true)?;
    for listener in listeners {
        let running = Arc::clone(&server);
        thread::spawn(move || running.run_shared(listener));
    }

    let suite = |label: &str| -> Result<String> {
        step(&format!("Parity suite on the {label} build"));
        let id = e2e::run_id("chrome", SUITE);
        let url = format!("http://localhost:{port}{BASE}__test/run/{id}/{SUITE}.html");
        let report = e2e::chrome(&url, &id, true, &reports, TIMEOUT)?;
        println!("{report}");
        Ok(report)
    };
    let io = |e: std::io::Error| format!("dev server: {e}");
    server.set_dumps(Some(&frames_first)).map_err(io)?;
    let before = suite(rev)?;
    if !e2e::passed(&before) {
        return Err(format!("the parity suite failed on the build at {rev}"));
    }
    server.set_dumps(Some(&frames_second)).map_err(io)?;
    server.set_references(Some(&frames_first)).map_err(io)?;
    server.set_next(true);
    let after = suite("working tree's")?;
    profile.check_size()?;
    if e2e::passed(&after) {
        step(&format!("The working tree draws the views as {rev} does"));
        Ok(())
    } else {
        Err(format!(
            "the working tree's views differ from {rev}'s (frames in {} and {})",
            frames_first.display(),
            frames_second.display()
        ))
    }
}

/// The test site built in the workspace `workspace`, copied to `to`.
fn copy_site(workspace: &std::path::Path, to: &std::path::Path) -> Result {
    let built = crate::bazel::build_output(workspace, "//platforms/web:site_test")?;
    if to.exists() {
        fs::remove_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    copy_dir(&built, to).map_err(|e| format!("{}: {e}", built.display()))
}
