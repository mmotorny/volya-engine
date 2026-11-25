// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Project tasks: `cargo xtask <task>`.

mod deploy;
mod e2e;
mod spec_lint;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const USAGE: &str = "\
Usage: cargo xtask <task>

Tasks:
  check       Fast static checks (SPDX, licenses, fmt, clippy). Gates every push.
  e2e [TARGET...] [SUITE...]
              Native tests, then browser suites on TARGETs (chrome, iphone;
              default: all). Naming SUITEs runs only those, without the
              native tests. On demand only.
  sim STEP...  Real touches in the iOS Simulator (tools/simdriver): runs the
              steps, e.g. 'open URL' 'tap-text Play' 'screenshot out.png'.
  simshot [PATH]
              A screenshot of the test simulator (PNG; default: a new file in
              the system's temporary directory). Prints its path.
  simrec PATH SECONDS
              Records the test simulator's screen for SECONDS (H.264).
              VOLYA_SIM_MEDIA=DIR records every iphone suite of `e2e` and
              screenshots its end into DIR.
  web [--dev] Build the web app into dist/ (release unless --dev).
  deploy      Check, build and publish to GitHub Pages (gh-pages branch).
  spec-lint   Flag decompiler output, code and mangled names in clean-room spec drafts.
  update      Move to the latest stable Rust and dependencies, then check.";

/// Crates that never build for the web target.
const NATIVE_ONLY: &[&str] = &["volya-engine-devserver", "xtask"];

const WASM_TARGET: &str = "wasm32-unknown-unknown";

type Result<T = ()> = std::result::Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["check"] => check(),
        ["e2e", targets @ ..] => e2e::run(targets),
        ["sim", steps @ ..] if !steps.is_empty() => e2e::sim(steps),
        ["simshot"] => e2e::screenshot(None).map(drop),
        ["simshot", path] => e2e::screenshot(Some(Path::new(path))).map(drop),
        ["simrec", path, seconds] => seconds
            .parse::<f64>()
            .map_err(|e| format!("seconds: {e}"))
            .and_then(|s| e2e::record_for(Path::new(path), Duration::from_secs_f64(s))),
        ["web"] => web(Profile::Release),
        ["web", "--dev"] => web(Profile::Dev),
        ["deploy"] => deploy::run_deploy(),
        ["spec-lint", paths @ ..] => spec_lint::run(paths),
        ["update"] => update(),
        ["-h" | "--help"] | [] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(format!("unknown arguments: {}\n\n{USAGE}", args.join(" "))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("\nxtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask lives at tools/xtask")
        .to_path_buf()
}

fn step(name: &str) {
    println!("\n==> {name}");
}

fn cmd(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.args(args).current_dir(root());
    command
}

fn run(command: &mut Command) -> Result {
    let status = command
        .status()
        .map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed with {status}"))
    }
}

fn output(command: &mut Command) -> Result<String> {
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

/// `--exclude` arguments for crates that don't build for wasm.
fn exclude_native_only() -> Vec<&'static str> {
    NATIVE_ONLY.iter().flat_map(|c| ["--exclude", c]).collect()
}

fn check() -> Result {
    step("SPDX headers");
    check_spdx()?;
    step("Dependency licenses");
    run(&mut cmd(
        "cargo",
        &["deny", "--log-level", "error", "check"],
    ))?;
    step("Format");
    run(&mut cmd("cargo", &["fmt", "--all", "--check"]))?;
    step("Clippy");
    let lint = ["--all-targets", "--", "-D", "warnings"];
    run(cmd("cargo", &["clippy", "--workspace"]).args(lint))?;
    run(
        cmd("cargo", &["clippy", "--workspace", "--target", WASM_TARGET])
            .args(exclude_native_only())
            .args(lint),
    )?;
    // The web build with WebGL 2 too (WASM_BUILDS).
    run(cmd(
        "cargo",
        &[
            "clippy",
            "-p",
            "volya-engine-web",
            "--target",
            WASM_TARGET,
            "--features",
            "webgl",
        ],
    )
    .args(lint))?;
    step("Static checks passed");
    Ok(())
}

/// Keeps the project at head: latest stable Rust, latest compatible crates.
fn update() -> Result {
    step("Rust toolchain");
    run(&mut cmd("rustup", &["update", "stable"]))?;
    step("Dependencies");
    run(&mut cmd("cargo", &["update"]))?;
    check()
}

/// Version of `package` recorded in Cargo.lock.
fn locked_version(package: &str) -> Result<String> {
    let lock = fs::read_to_string(root().join("Cargo.lock"))
        .map_err(|e| format!("read Cargo.lock: {e}"))?;
    locked_version_in(&lock, package).ok_or_else(|| format!("{package} not in Cargo.lock"))
}

fn locked_version_in(lock: &str, package: &str) -> Option<String> {
    let name = format!("name = \"{package}\"");
    let mut lines = lock.lines();
    lines.find(|l| *l == name)?;
    let version = lines.next()?.strip_prefix("version = \"")?;
    Some(version.trim_end_matches('"').to_owned())
}

/// Path to a `wasm-bindgen` CLI matching the locked `wasm-bindgen` crate (the
/// two must match exactly), installed into `.cache/tools` on demand.
fn wasm_bindgen_cli() -> Result<PathBuf> {
    let version = locked_version("wasm-bindgen")?;
    let tools = root().join(".cache/tools");
    let bin = tools.join("bin/wasm-bindgen");
    let installed = bin.exists()
        && output(&mut cmd(&bin.to_string_lossy(), &["--version"]))
            .is_ok_and(|v| v.split_whitespace().nth(1) == Some(version.as_str()));
    if !installed {
        step(&format!("Install wasm-bindgen CLI {version}"));
        run(&mut cmd(
            "cargo",
            &[
                "install",
                "--locked",
                "--force",
                "wasm-bindgen-cli",
                "--version",
                &version,
                "--root",
                &tools.to_string_lossy(),
            ],
        ))?;
    }
    Ok(bin)
}

#[derive(Clone, Copy)]
enum Profile {
    /// Unoptimised, with the test ops.
    Dev,
    /// Optimised, as deployed.
    Release,
    /// Optimised, with the test ops (`cargo xtask e2e`).
    Test,
}

/// The web build's two wasm builds (static/wasm-build.js, #186): the
/// directory each goes to in `dist/`, the build's name and its features.
const WASM_BUILDS: &[(&str, &str, &[&str])] =
    &[("pkg", "webgpu", &[]), ("pkg-webgl", "webgl", &["webgl"])];

fn web(profile: Profile) -> Result {
    let wasm_bindgen = wasm_bindgen_cli()?;
    let (release, test_ops, dir) = match profile {
        Profile::Dev => (false, true, "debug"),
        Profile::Release => (true, false, "release"),
        Profile::Test => (true, true, "release"),
    };
    let dist = root().join("dist");
    if dist.exists() {
        fs::remove_dir_all(&dist).map_err(|e| format!("remove dist: {e}"))?;
    }
    copy_dir(&root().join("platforms/web/static"), &dist).map_err(|e| format!("copy web: {e}"))?;
    for &(out, build, build_features) in WASM_BUILDS {
        step(&format!("Web build ({build})"));
        let mut features: Vec<&str> = build_features.to_vec();
        if test_ops {
            features.push("test-ops");
        }
        let mut command = cmd(
            "cargo",
            &["build", "-p", "volya-engine-web", "--target", WASM_TARGET],
        );
        if release {
            command.arg("--release");
        }
        if !features.is_empty() {
            command.args(["--features", &features.join(",")]);
        }
        run(&mut command)?;
        // Each build writes the same file: bound before the next.
        let wasm = format!("target/{WASM_TARGET}/{dir}/volya_engine_web.wasm");
        run(&mut cmd(
            &wasm_bindgen.to_string_lossy(),
            &[
                "--target",
                "web",
                "--no-typescript",
                "--out-dir",
                &format!("dist/{out}"),
                &wasm,
            ],
        ))?;
    }
    inject_build(&dist)
}

/// Injects what the page and its service worker need to know about this
/// build: the commit and its date into `update.js` (`BUILD`, `BUILT`: the
/// version shown), and the commit plus every file with a hash of its content,
/// its size and, for a wasm build's files, that build's name into `sw.js`
/// (`BUILD`, `PRECACHE`; static/sw.js).
fn inject_build(dist: &Path) -> Result {
    use sha2::{Digest, Sha256};
    let commit = output(&mut cmd("git", &["rev-parse", "--short", "HEAD"]))?;
    let dirty = !output(&mut cmd("git", &["status", "--porcelain"]))?
        .trim()
        .is_empty();
    let commit = format!("{}{}", commit.trim(), if dirty { "+" } else { "" });
    let date = output(&mut cmd("git", &["show", "-s", "--format=%cI", "HEAD"]))?;
    replace_once(
        &dist.join("update.js"),
        r#"export const BUILD = "dev";"#,
        &format!(r#"export const BUILD = "{commit}";"#),
    )?;
    replace_once(
        &dist.join("update.js"),
        r#"export const BUILT = "";"#,
        &format!(r#"export const BUILT = "{}";"#, date.trim()),
    )?;
    let mut files = Vec::new();
    list_files(dist, dist, &mut files).map_err(|e| format!("list dist: {e}"))?;
    // The UX audit page (static/ux-audit/) is for the project, not players: not
    // kept offline.
    files.retain(|f| f != "sw.js" && !f.starts_with("ux-audit/"));
    files.sort();
    let mut entries = Vec::new();
    for file in &files {
        let bytes = fs::read(dist.join(file)).map_err(|e| format!("{file}: {e}"))?;
        let hash: String = Sha256::digest(&bytes)[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let size = bytes.len();
        let build = WASM_BUILDS
            .iter()
            .find(|(out, ..)| file.starts_with(&format!("{out}/")))
            .map(|(_, build, _)| format!(r#","{build}""#))
            .unwrap_or_default();
        entries.push(format!(r#"["{file}","{hash}",{size}{build}]"#));
        if file == "index.html" {
            entries.push(format!(r#"["./","{hash}",{size}]"#));
        }
    }
    replace_once(
        &dist.join("sw.js"),
        r#"const BUILD = "dev";"#,
        &format!(r#"const BUILD = "{commit}";"#),
    )?;
    replace_once(
        &dist.join("sw.js"),
        "const PRECACHE = [/* injected by cargo xtask web */];",
        &format!("const PRECACHE = [{}];", entries.join(",")),
    )
}

/// Replaces the one occurrence of `from` in the file at `path`.
fn replace_once(path: &Path, from: &str, to: &str) -> Result {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if text.matches(from).count() != 1 {
        return Err(format!("{}: expected one {from:?}", path.display()));
    }
    fs::write(path, text.replacen(from, to, 1)).map_err(|e| format!("{}: {e}", path.display()))
}

fn list_files(root: &Path, dir: &Path, files: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            list_files(root, &path, files)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Third-party sources kept as released, under their own licences.
const VENDORED: &str = "/vendor/";

/// Source files that must carry SPDX headers.
const SPDX_PATTERNS: &[&str] = &[
    "*.rs",
    "*.c",
    "*.h",
    "*.S",
    "*.lua",
    "*.sh",
    "*.js",
    "*.mjs",
    "*.html",
    "*.css",
    "*.svg",
    "*.toml",
    "*.wgsl",
    "*.swift",
    "*.pbxproj",
    "*.xcscheme",
];

fn check_spdx() -> Result {
    let files = output(
        cmd(
            "git",
            &["ls-files", "--cached", "--others", "--exclude-standard"],
        )
        .args(SPDX_PATTERNS),
    )?;
    let missing: Vec<&str> = files
        .lines()
        .filter(|file| !file.contains(VENDORED))
        .filter(|file| {
            fs::read_to_string(root().join(file))
                .map(|text| !has_spdx_headers(&text))
                .unwrap_or(false) // Deleted but still tracked.
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("missing SPDX headers:\n  {}", missing.join("\n  ")))
    }
}

fn has_spdx_headers(text: &str) -> bool {
    let head: Vec<&str> = text.lines().take(5).collect();
    head.iter().any(|l| l.contains("SPDX-FileCopyrightText: "))
        && head
            .iter()
            .any(|l| l.contains("SPDX-License-Identifier: BSD-3-Clause"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_locked_versions() {
        let lock = "[[package]]\nname = \"wasm-bindgen\"\nversion = \"0.2.130\"\n\n[[package]]\nname = \"wasm-bindgen-shared\"\nversion = \"0.2.131\"\n";
        assert_eq!(
            locked_version_in(lock, "wasm-bindgen").as_deref(),
            Some("0.2.130")
        );
        assert_eq!(
            locked_version_in(lock, "wasm-bindgen-shared").as_deref(),
            Some("0.2.131")
        );
        assert_eq!(locked_version_in(lock, "serde"), None);
    }

    #[test]
    fn spdx_headers() {
        let ok = "// SPDX-FileCopyrightText: 2026 A\n// SPDX-License-Identifier: BSD-3-Clause\n";
        assert!(has_spdx_headers(ok));
        assert!(has_spdx_headers(&format!("#!/bin/sh\n{ok}")));
        assert!(!has_spdx_headers(
            "// SPDX-License-Identifier: BSD-3-Clause\n"
        ));
        assert!(!has_spdx_headers(
            "// SPDX-FileCopyrightText: 2026 A\n// SPDX-License-Identifier: MIT\n"
        ));
        assert!(!has_spdx_headers(&format!("\n\n\n\n\n{ok}")));
    }
}
