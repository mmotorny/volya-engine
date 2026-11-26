// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Deploys to GitHub Pages without GitHub Actions: builds a release locally
//! and force-pushes it as a single commit to the `gh-pages` branch. The branch
//! is a build artifact; its history carries no value. Tests are never
//! deployed.

use std::fs;

use super::{Profile, Result, check, cmd, copy_dir, output, root, run, step, web};

const BRANCH: &str = "gh-pages";

pub fn run_deploy() -> Result {
    step("Preconditions");
    if !output(&mut cmd("git", &["status", "--porcelain"]))?
        .trim()
        .is_empty()
    {
        return Err("working tree is dirty; commit first".into());
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

    check()?;
    web(Profile::Release)?;

    step("Stage site");
    let site = root().join("target/gh-pages");
    if site.exists() {
        fs::remove_dir_all(&site).map_err(|e| format!("remove {}: {e}", site.display()))?;
    }
    copy_dir(&root().join("dist"), &site).map_err(|e| format!("copy dist: {e}"))?;
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
