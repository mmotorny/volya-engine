// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Tasks of the Bazel build (docs/architecture/build.md): keeping it at head
//! (`update`) and removing what deleted worktrees left (`gc`).

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{Result, cmd, locked_version, output, root, run, step};

/// Moves everything to its latest release: Rust, the crates, Bazel, its
/// modules, the wasm LLVM, the Windows cross LLVM (llvm-mingw), DynamoRIO, MojoAL, SDL 3 and the
/// wasm-bindgen CLI's pin; then the checks and the advisories.
pub fn update() -> Result {
    let module_path = root().join("MODULE.bazel");
    let read = |p: &Path| fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let mut module = read(&module_path)?;

    step("Rust toolchain");
    let channel = output(&mut cmd(
        "curl",
        &[
            "-sfL",
            "https://static.rust-lang.org/dist/channel-rust-stable.toml",
        ],
    ))?;
    let rust = stable_rust(&channel).ok_or("no Rust version in the stable channel")?;
    module = replace_quoted_after(&module, "versions = [", &rust)?;
    println!("Rust {rust}");

    step("Bazel modules");
    for name in bazel_deps(&module) {
        let url = format!("https://bcr.bazel.build/modules/{name}/metadata.json");
        let metadata = output(&mut cmd("curl", &["-sfL", &url]))?;
        let latest = latest_module_version(&metadata).ok_or(format!("{name}: no versions"))?;
        module = set_bazel_dep(&module, &name, &latest)?;
        println!("{name} {latest}");
    }

    step("wasm LLVM (wasi-sdk)");
    module = update_wasm_llvm(&module)?;
    step("The Windows cross LLVM (llvm-mingw)");
    module = update_llvm_mingw(&module)?;
    step("DynamoRIO (the Windows capture's counter client)");
    module = update_dynamorio(&module)?;
    step("MojoAL and SDL 3 (the Windows capture's audio device)");
    module = update_mojoal(&module)?;
    module = update_sdl3(&module)?;
    fs::write(&module_path, &module).map_err(|e| format!("MODULE.bazel: {e}"))?;

    step("Bazel");
    let bazel = output(&mut cmd(
        "gh",
        &[
            "api",
            "repos/bazelbuild/bazel/releases/latest",
            "--jq",
            ".tag_name",
        ],
    ))?;
    fs::write(root().join(".bazelversion"), format!("{}\n", bazel.trim()))
        .map_err(|e| format!(".bazelversion: {e}"))?;
    println!("Bazel {}", bazel.trim());

    step("Dependencies");
    run(&mut cmd(
        "bazel",
        &[
            "run",
            "@rules_rust//tools/upstream_wrapper:cargo",
            "--",
            "update",
        ],
    ))?;
    let bindgen = locked_version("wasm-bindgen")?;
    let module = read(&module_path)?;
    let start = module
        .find("package = \"wasm-bindgen-cli\"")
        .ok_or("MODULE.bazel: no wasm-bindgen-cli")?;
    let (head, spec) = module.split_at(start);
    let module = format!(
        "{head}{}",
        replace_quoted_after(spec, "version = ", &format!("={bindgen}"))?
    );
    fs::write(&module_path, module).map_err(|e| format!("MODULE.bazel: {e}"))?;

    step("Editors' rust-project.json");
    run(&mut cmd(
        "bazel",
        &["run", "@rules_rust//tools/rust_analyzer:gen_rust_project"],
    ))?;

    step("Checks");
    run(&mut cmd("bazel", &["test", "//:checks"]))?;
    step("Advisories");
    run(&mut cmd(
        "cargo-deny",
        &["--log-level", "error", "check", "advisories"],
    ))
}

/// Builds `target` in the workspace `workspace` (with the shared caches,
/// mostly hits); the path of its one output.
pub fn build_output(workspace: &Path, target: &str) -> Result<PathBuf> {
    run(cmd("bazel", &["build", target]).current_dir(workspace))?;
    let files = output(cmd("bazel", &["cquery", "--output=files", target]).current_dir(workspace))?;
    let file = files.lines().last().ok_or(format!("{target}: no output"))?;
    // Relative to the execroot, which the workspace's `bazel-out` link
    // reaches.
    Ok(workspace.join(file.trim()))
}

/// The version in the stable channel's `[pkg.rust]`.
fn stable_rust(channel: &str) -> Option<String> {
    let section = channel.split("\n[pkg.rust]\n").nth(1)?;
    let line = section.lines().find(|l| l.starts_with("version = \""))?;
    let version = line
        .strip_prefix("version = \"")?
        .split([' ', '"'])
        .next()?;
    Some(version.to_owned())
}

/// Replaces the first quoted string after `marker` with `value`.
fn replace_quoted_after(text: &str, marker: &str, value: &str) -> Result<String> {
    let at = text
        .find(marker)
        .ok_or(format!("MODULE.bazel: no {marker:?}"))?
        + marker.len();
    let open = at + text[at..].find('"').ok_or("no quote")? + 1;
    let close = open + text[open..].find('"').ok_or("no closing quote")?;
    Ok(format!("{}{value}{}", &text[..open], &text[close..]))
}

/// The modules MODULE.bazel depends on.
fn bazel_deps(module: &str) -> Vec<String> {
    module
        .lines()
        .filter_map(|l| l.strip_prefix("bazel_dep(name = \""))
        .filter_map(|l| l.split('"').next())
        .map(str::to_owned)
        .collect()
}

fn set_bazel_dep(module: &str, name: &str, version: &str) -> Result<String> {
    replace_quoted_after(
        module,
        &format!("bazel_dep(name = \"{name}\", version = "),
        version,
    )
}

/// The last of a registry module's versions (they are listed oldest first),
/// leaving out pre-releases.
fn latest_module_version(metadata: &str) -> Option<String> {
    let list = metadata.split("\"versions\"").nth(1)?;
    let list = &list[list.find('[')? + 1..list.find(']')?];
    list.split(',')
        .map(|v| v.trim().trim_matches('"'))
        .rfind(|v| !v.is_empty() && !v.contains('-'))
        .map(str::to_owned)
}

/// Points `@wasm_llvm` at wasi-sdk's latest release for this machine, with
/// its checksum.
fn update_wasm_llvm(module: &str) -> Result<String> {
    let tag = output(&mut cmd(
        "gh",
        &[
            "api",
            "repos/WebAssembly/wasi-sdk/releases/latest",
            "--jq",
            ".tag_name",
        ],
    ))?;
    let tag = tag.trim();
    let number = tag.strip_prefix("wasi-sdk-").ok_or(format!("tag {tag}"))?;
    let name = format!("wasi-sdk-{number}.0-arm64-macos");
    let url =
        format!("https://github.com/WebAssembly/wasi-sdk/releases/download/{tag}/{name}.tar.gz");
    if module.contains(&url) {
        println!("{name} (unchanged)");
        return Ok(module.to_owned());
    }
    let file = std::env::temp_dir().join(format!("{name}.tar.gz"));
    run(&mut cmd(
        "curl",
        &["-sfL", "-o", &file.to_string_lossy(), &url],
    ))?;
    let sha = sha256_file(&file)?;
    let _ = fs::remove_file(&file);
    let start = module
        .find("name = \"wasm_llvm\"")
        .ok_or("MODULE.bazel: no wasm_llvm")?;
    let (head, block) = module.split_at(start);
    let block = replace_quoted_after(block, "sha256 = ", &sha)?;
    let block = replace_quoted_after(&block, "strip_prefix = ", &name)?;
    let block = replace_quoted_after(&block, "url = ", &url)?;
    let module = format!("{head}{block}");
    println!("{name}");
    Ok(module)
}

/// Points `@llvm_mingw` at llvm-mingw's latest release for macOS, with its
/// checksum.
fn update_llvm_mingw(module: &str) -> Result<String> {
    let tag = output(&mut cmd(
        "gh",
        &[
            "api",
            "repos/mstorsjo/llvm-mingw/releases/latest",
            "--jq",
            ".tag_name",
        ],
    ))?;
    let tag = tag.trim();
    let name = format!("llvm-mingw-{tag}-ucrt-macos-universal");
    let url =
        format!("https://github.com/mstorsjo/llvm-mingw/releases/download/{tag}/{name}.tar.xz");
    if module.contains(&url) {
        println!("{name} (unchanged)");
        return Ok(module.to_owned());
    }
    let file = std::env::temp_dir().join(format!("{name}.tar.xz"));
    run(&mut cmd(
        "curl",
        &["-sfL", "-o", &file.to_string_lossy(), &url],
    ))?;
    let sha = sha256_file(&file)?;
    let _ = fs::remove_file(&file);
    let start = module
        .find("name = \"llvm_mingw\"")
        .ok_or("MODULE.bazel: no llvm_mingw")?;
    let (head, block) = module.split_at(start);
    let block = replace_quoted_after(block, "sha256 = ", &sha)?;
    let block = replace_quoted_after(&block, "strip_prefix = ", &name)?;
    let block = replace_quoted_after(&block, "url = ", &url)?;
    println!("{name}");
    Ok(format!("{head}{block}"))
}

/// Points `@dynamorio` at DynamoRIO's latest release for Windows, with the
/// checksum GitHub records for the asset (its archive is about 450 MB).
fn update_dynamorio(module: &str) -> Result<String> {
    let release = output(&mut cmd(
        "gh",
        &[
            "api",
            "repos/DynamoRIO/dynamorio/releases/latest",
            "--jq",
            r#".tag_name + " " + ([.assets[] | select(.name | test("^DynamoRIO-Windows-.*\\.zip$")) | .name + " " + .digest][0])"#,
        ],
    ))?;
    let words: Vec<&str> = release.split_whitespace().collect();
    let [tag, asset, digest] = words[..] else {
        return Err(format!("DynamoRIO's latest release: {release}"));
    };
    let sha = digest
        .strip_prefix("sha256:")
        .ok_or(format!("{asset}: digest {digest}"))?;
    let name = asset.strip_suffix(".zip").ok_or("no .zip")?;
    let url = format!("https://github.com/DynamoRIO/dynamorio/releases/download/{tag}/{asset}");
    if module.contains(&url) {
        println!("{name} (unchanged)");
        return Ok(module.to_owned());
    }
    let start = module
        .find("name = \"dynamorio\"")
        .ok_or("MODULE.bazel: no dynamorio")?;
    let (head, block) = module.split_at(start);
    let block = replace_quoted_after(block, "sha256 = ", sha)?;
    let block = replace_quoted_after(&block, "strip_prefix = ", name)?;
    let block = replace_quoted_after(&block, "url = ", &url)?;
    println!("{name}");
    Ok(format!("{head}{block}"))
}

/// Points the http_archive `repo` at `url` with `strip_prefix`, the file's
/// checksum `sha` (downloaded when None).
fn set_archive(
    module: &str,
    repo: &str,
    url: &str,
    strip_prefix: &str,
    sha: Option<&str>,
) -> Result<String> {
    if module.contains(url) {
        println!("{strip_prefix} (unchanged)");
        return Ok(module.to_owned());
    }
    let sha = match sha {
        Some(sha) => sha.to_owned(),
        None => {
            let file = std::env::temp_dir().join(format!("{repo}-update"));
            run(&mut cmd(
                "curl",
                &["-sfL", "-o", &file.to_string_lossy(), url],
            ))?;
            let sha = sha256_file(&file)?;
            let _ = fs::remove_file(&file);
            sha
        }
    };
    let start = module
        .find(&format!("name = \"{repo}\""))
        .ok_or(format!("MODULE.bazel: no {repo}"))?;
    let (head, block) = module.split_at(start);
    let block = replace_quoted_after(block, "sha256 = ", &sha)?;
    let block = replace_quoted_after(&block, "strip_prefix = ", strip_prefix)?;
    let block = replace_quoted_after(&block, "url = ", url)?;
    println!("{strip_prefix}");
    Ok(format!("{head}{block}"))
}

/// Points `@mojoal` at the head of MojoAL's main branch.
fn update_mojoal(module: &str) -> Result<String> {
    let sha = output(&mut cmd(
        "gh",
        &["api", "repos/icculus/mojoAL/commits/main", "--jq", ".sha"],
    ))?;
    let sha = sha.trim();
    let url = format!("https://github.com/icculus/mojoAL/archive/{sha}.tar.gz");
    set_archive(module, "mojoal", &url, &format!("mojoAL-{sha}"), None)
}

/// Points `@sdl3_mingw` at SDL's latest release's MinGW development
/// archive, with the checksum GitHub records for it.
fn update_sdl3(module: &str) -> Result<String> {
    let release = output(&mut cmd(
        "gh",
        &[
            "api",
            "repos/libsdl-org/SDL/releases/latest",
            "--jq",
            r#".tag_name + " " + ([.assets[] | select(.name | test("^SDL3-devel-.*-mingw\\.tar\\.gz$")) | .name + " " + .digest][0])"#,
        ],
    ))?;
    let words: Vec<&str> = release.split_whitespace().collect();
    let [tag, asset, digest] = words[..] else {
        return Err(format!("SDL's latest release: {release}"));
    };
    let sha = digest
        .strip_prefix("sha256:")
        .ok_or(format!("{asset}: digest {digest}"))?;
    let version = tag
        .strip_prefix("release-")
        .ok_or(format!("SDL tag {tag}"))?;
    let url = format!("https://github.com/libsdl-org/SDL/releases/download/{tag}/{asset}");
    set_archive(
        module,
        "sdl3_mingw",
        &url,
        &format!("SDL3-{version}"),
        Some(sha),
    )
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Removes the output bases (`~/Library/Caches/bazel/_bazel_<user>/<hash>/`)
/// whose workspace no longer exists, as a deleted worktree leaves its own;
/// one whose server still runs is left for a later run.
pub fn gc() -> Result {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME")?);
    let user = std::env::var("USER").map_err(|_| "USER")?;
    let bases = home.join(format!("Library/Caches/bazel/_bazel_{user}"));
    let mut freed = 0;
    for entry in fs::read_dir(&bases).map_err(|e| format!("{}: {e}", bases.display()))? {
        let base = entry.map_err(|e| e.to_string())?.path();
        let Ok(workspace) = fs::read_to_string(base.join("DO_NOT_BUILD_HERE")) else {
            continue;
        };
        let workspace = workspace.trim();
        if Path::new(workspace).exists() {
            continue;
        }
        let pid = fs::read_to_string(base.join("server/server.pid.txt")).unwrap_or_default();
        if pid.trim().parse().is_ok_and(crate::machine::alive) {
            println!("{}: its server still runs ({workspace})", base.display());
            continue;
        }
        println!("Removing {} (was {workspace})", base.display());
        make_writable(&base).map_err(|e| format!("{}: {e}", base.display()))?;
        fs::remove_dir_all(&base).map_err(|e| format!("{}: {e}", base.display()))?;
        freed += 1;
    }
    step(&format!("Removed {freed} output bases"));
    Ok(())
}

/// Bazel leaves its outputs read-only; directories must be writable for
/// their entries to be removed. Links are left alone.
fn make_writable(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() {
        return Ok(());
    }
    let mut permissions = meta.permissions();
    permissions.set_mode(permissions.mode() | 0o700);
    fs::set_permissions(dir, permissions)?;
    for entry in fs::read_dir(dir)? {
        make_writable(&entry?.path())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_stable_channel() {
        let channel = "[pkg.cargo]\nversion = \"0.1\"\n\n[pkg.rust]\nversion = \"1.100.0 (0ef1bc386 2026-11-09)\"\n";
        assert_eq!(stable_rust(channel).as_deref(), Some("1.100.0"));
    }

    #[test]
    fn edits_module_bazel() {
        let module = "bazel_dep(name = \"platforms\", version = \"1.0.0\")\nbazel_dep(name = \"rules_rust\", version = \"0.74.0\")\n    versions = [\"1.99.0\"],\n";
        assert_eq!(bazel_deps(module), ["platforms", "rules_rust"]);
        let module = set_bazel_dep(module, "rules_rust", "0.75.0").unwrap();
        assert!(module.contains("\"rules_rust\", version = \"0.75.0\""));
        assert!(module.contains("\"platforms\", version = \"1.0.0\""));
        let module = replace_quoted_after(&module, "versions = [", "1.100.0").unwrap();
        assert!(module.contains("versions = [\"1.100.0\"]"));
    }

    #[test]
    fn picks_the_latest_release() {
        let metadata = r#"{"homepage": "x", "versions": ["0.9.0", "1.0.0", "1.1.0-rc1"], "yanked_versions": {}}"#;
        assert_eq!(latest_module_version(metadata).as_deref(), Some("1.0.0"));
    }
}
