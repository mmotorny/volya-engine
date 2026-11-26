// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Deploys to GitHub Pages without GitHub Actions: builds the release site
//! with Bazel (`//platforms/web:site`), stamps it with the commit
//! and force-pushes it as a single commit to the `gh-pages` branch. The branch
//! is a build artifact; its history carries no value. Tests are never
//! deployed.

use std::fs;

use volya_engine_site::{Stamp, copy_dir, restamp};

use super::{Result, cmd, git_stamp, output, root, run, step};

const BRANCH: &str = "gh-pages";

pub fn run_deploy() -> Result {
    step("Preconditions");
    if !output(&mut cmd("git", &["status", "--porcelain"]))?
        .trim()
        .is_empty()
    {
        return Err("working tree is dirty; commit first".into());
    }
    // Never build output in the tree (#547; //build:gitignore keeps it
    // ignored).
    let tracked = output(&mut cmd(
        "git",
        &[
            "ls-files",
            "--",
            "dist",
            "dist-next",
            ".cache",
            "target",
            ":(glob)bazel-*",
        ],
    ))?;
    if !tracked.trim().is_empty() {
        return Err(format!("build output is tracked:\n{tracked}"));
    }
    // Deploy only what main holds, whatever branch HEAD is on (worktree
    // branches have no upstream).
    run(&mut cmd("git", &["fetch", "-q", "origin", "main"]))?;
    if !cmd(
        "git",
        &["merge-base", "--is-ancestor", "HEAD", "origin/main"],
    )
    .status()
    .map_err(|e| format!("git merge-base: {e}"))?
    .success()
    {
        return Err("HEAD is not in origin/main; merge and push first".into());
    }
    let sha = output(&mut cmd("git", &["rev-parse", "--short", "HEAD"]))?;
    let sha = sha.trim();

    step("Checks");
    run(&mut cmd("bazel", &["test", "//:checks"]))?;
    step("Release build");
    let built = crate::bazel::build_output(&root(), "//platforms/web:site")?;

    step("Stage site");
    let site = root().join("target/gh-pages");
    if site.exists() {
        fs::remove_dir_all(&site).map_err(|e| format!("remove {}: {e}", site.display()))?;
    }
    copy_dir(&built, &site).map_err(|e| format!("copy {}: {e}", built.display()))?;
    // Bazel's site is stamped for tests; this one carries the commit.
    let (commit, date) = git_stamp("")?;
    restamp(
        &site,
        &root().join("platforms/web/static"),
        &Stamp {
            commit: &commit,
            date: &date,
        },
    )?;
    // Serve files as-is, without Jekyll processing.
    fs::write(site.join(".nojekyll"), "").map_err(|e| format!(".nojekyll: {e}"))?;

    step("Publish");
    let git = |args: &[&str]| {
        let mut c = cmd("git", args);
        c.current_dir(&site);
        c
    };
    run(&mut git(&["init", "-q", "-b", BRANCH]))?;
    run(&mut git(&["add", "-A"]))?;
    run(&mut git(&[
        "commit",
        "-q",
        "-m",
        &format!("Deploy {sha}\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"),
    ]))?;
    let remote = output(&mut cmd("git", &["remote", "get-url", "origin"]))?;
    run(&mut git(&[
        "push",
        "-q",
        "--force",
        remote.trim(),
        &format!("{BRANCH}:{BRANCH}"),
    ]))?;

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
    let pages = format!("repos/{repo}/pages");
    if output(&mut cmd("gh", &["api", &pages])).is_err() {
        step("Enable GitHub Pages");
        run(cmd(
            "gh",
            &[
                "api",
                "-X",
                "POST",
                &pages,
                "-f",
                &format!("source[branch]={BRANCH}"),
                "-f",
                "source[path]=/",
            ],
        )
        .stdout(std::process::Stdio::null()))?;
    }
    let url = output(&mut cmd("gh", &["api", &pages, "--jq", ".html_url"]))?;
    step(&format!(
        "Published {sha}; GitHub Pages serves it in a minute or two at {}",
        url.trim()
    ));
    Ok(())
}
