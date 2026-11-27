// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The web app's site from its parts: the static files, the wasm-bindgen
//! output, the engine's build stamp, and what the page and its service worker
//! are told about the build (docs/architecture/build.md, The web app). Used by
//! the Bazel rule (`build/web.bzl`, through `main.rs`) and by xtask.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

pub mod notices;

pub type Result<T = ()> = std::result::Result<T, String>;

/// Where a build came from: shown as the version (`update.js`) and naming
/// the service worker's build.
pub struct Stamp<'a> {
    /// The commit, `+` when the working tree was dirty, and a suffix (the
    /// next build's).
    pub commit: &'a str,
    /// The commit's date (ISO 8601).
    pub date: &'a str,
}

/// The stamp of sites tests use (docs/architecture/build.md): fixed, so a
/// commit that changes no input keeps cached results.
pub const TEST_STAMP: Stamp<'static> = Stamp {
    commit: "0000000",
    date: "2026-01-01T00:00:00Z",
};

/// The third-party notices' name in a site ([`notices`]).
pub const NOTICES: &str = "third-party-licenses.txt";

/// Builds a site into `out` (created afresh): the static files of
/// `static_dir`, the third-party notices `notices` as [`NOTICES`], `wasm`
/// through the `wasm_bindgen` CLI into `pkg/`, the engine stamped, and
/// `stamp` injected.
pub fn build(
    static_dir: &Path,
    notices: &Path,
    wasm: &Path,
    wasm_bindgen: &Path,
    out: &Path,
    stamp: &Stamp,
) -> Result {
    if out.exists() {
        fs::remove_dir_all(out).map_err(|e| format!("remove {}: {e}", out.display()))?;
    }
    copy_dir(static_dir, out).map_err(|e| format!("copy {}: {e}", static_dir.display()))?;
    if out.join(NOTICES).exists() {
        return Err(format!("{}: {NOTICES} is generated", static_dir.display()));
    }
    fs::copy(notices, out.join(NOTICES)).map_err(|e| format!("{}: {e}", notices.display()))?;
    let status = Command::new(wasm_bindgen)
        .args([
            "--target",
            "web",
            "--no-typescript",
            "--out-name",
            "volya_engine_web",
            "--out-dir",
        ])
        .arg(out.join("pkg"))
        .arg(wasm)
        .status()
        .map_err(|e| format!("{}: {e}", wasm_bindgen.display()))?;
    if !status.success() {
        return Err(format!("wasm-bindgen failed with {status}"));
    }
    stamp_engine(out)?;
    inject_build(out, stamp)
}

/// `next`: the site `dist` as the next deploy would be, for the update flow's
/// test (tests/browser/smoke.js): the engine stamped with another id, a
/// shell module changed, and `update.js` and `sw.js` (from `static_dir`)
/// injected anew with `stamp`.
pub fn next_build(dist: &Path, static_dir: &Path, next: &Path, stamp: &Stamp) -> Result {
    if next.exists() {
        fs::remove_dir_all(next).map_err(|e| format!("remove {}: {e}", next.display()))?;
    }
    copy_dir(dist, next).map_err(|e| format!("copy {}: {e}", dist.display()))?;
    let glue = next.join("pkg/volya_engine_web.js");
    let text = fs::read_to_string(&glue).map_err(|e| format!("{}: {e}", glue.display()))?;
    let id = text
        .rsplit_once("export const BUILD_ID = \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(id, _)| id.to_owned())
        .ok_or("no BUILD_ID in the glue")?;
    let other = short_hash(format!("{id} next").as_bytes());
    fs::write(&glue, text.replace(&id, &other)).map_err(|e| format!("{}: {e}", glue.display()))?;
    // The id is the wasm's last bytes (stamp_engine).
    let wasm_path = next.join("pkg/volya_engine_web_bg.wasm");
    let mut wasm = fs::read(&wasm_path).map_err(|e| format!("{}: {e}", wasm_path.display()))?;
    if !wasm.ends_with(id.as_bytes()) {
        return Err("the wasm doesn't end with its id".into());
    }
    let at = wasm.len() - id.len();
    wasm[at..].copy_from_slice(other.as_bytes());
    fs::write(&wasm_path, wasm).map_err(|e| format!("{}: {e}", wasm_path.display()))?;
    // A shell module of its own, which tests tell apart by `NEXT_BUILD`.
    let shell = next.join("game-ui.js");
    let mut text = fs::read_to_string(&shell).map_err(|e| format!("{}: {e}", shell.display()))?;
    text.push_str("\nexport const NEXT_BUILD = true;\n");
    fs::write(&shell, text).map_err(|e| format!("{}: {e}", shell.display()))?;
    for file in ["update.js", "sw.js"] {
        fs::copy(static_dir.join(file), next.join(file)).map_err(|e| format!("{file}: {e}"))?;
    }
    inject_build(next, stamp)
}

/// `dist` (a site stamped for tests, as Bazel builds it) stamped with
/// `stamp` instead: `update.js` and `sw.js` from `static_dir` injected anew.
/// The engine's stamp (its wasm's hash) stays.
pub fn restamp(dist: &Path, static_dir: &Path, stamp: &Stamp) -> Result {
    for file in ["update.js", "sw.js"] {
        let to = dist.join(file);
        let _ = fs::remove_file(&to);
        fs::copy(static_dir.join(file), &to).map_err(|e| format!("{file}: {e}"))?;
    }
    inject_build(dist, stamp)
}

/// The first 8 bytes of SHA-256, in hex.
fn short_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Stamps the engine's wasm and its wasm-bindgen glue with one id (a hash of
/// the wasm), so the engine worker can tell when they come from different
/// builds (engine-worker.js): a custom section `volya-engine-build` in the
/// wasm, `export const BUILD_ID` in the glue.
pub fn stamp_engine(dist: &Path) -> Result {
    let wasm_path = dist.join("pkg/volya_engine_web_bg.wasm");
    let mut wasm = fs::read(&wasm_path).map_err(|e| format!("{}: {e}", wasm_path.display()))?;
    let id = short_hash(&wasm);
    let name = b"volya-engine-build";
    let mut section = leb128(name.len());
    section.extend_from_slice(name);
    section.extend_from_slice(id.as_bytes());
    wasm.push(0);
    wasm.extend(leb128(section.len()));
    wasm.extend(section);
    fs::write(&wasm_path, wasm).map_err(|e| format!("{}: {e}", wasm_path.display()))?;
    let glue = dist.join("pkg/volya_engine_web.js");
    let mut text = fs::read_to_string(&glue).map_err(|e| format!("{}: {e}", glue.display()))?;
    text.push_str(&format!("\nexport const BUILD_ID = \"{id}\";\n"));
    fs::write(&glue, text).map_err(|e| format!("{}: {e}", glue.display()))
}

/// `n` as unsigned LEB128.
fn leb128(mut n: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            bytes.push(byte);
            return bytes;
        }
        bytes.push(byte | 0x80);
    }
}

/// Injects what the page and its service worker need to know about this
/// build: the commit and its date into `update.js` (`BUILD`, `BUILT`: the
/// version shown), and the commit plus every file with a hash of its content,
/// and its size into `sw.js` (`BUILD`, `PRECACHE`, and `CACHE`, named by
/// them; static/sw.js).
pub fn inject_build(dist: &Path, stamp: &Stamp) -> Result {
    let commit = stamp.commit;
    replace_once(
        &dist.join("update.js"),
        r#"export const BUILD = "dev";"#,
        &format!(r#"export const BUILD = "{commit}";"#),
    )?;
    replace_once(
        &dist.join("update.js"),
        r#"export const BUILT = "";"#,
        &format!(r#"export const BUILT = "{}";"#, stamp.date),
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
        let hash = short_hash(&bytes);
        let size = bytes.len();
        entries.push(format!(r#"["{file}","{hash}",{size}]"#));
        if file == "index.html" {
            entries.push(format!(r#"["./","{hash}",{size}]"#));
        }
    }
    // Each build's cache is named by its files, so two builds never share one.
    let id = short_hash(entries.join(",").as_bytes());
    replace_once(
        &dist.join("sw.js"),
        r#"const CACHE = "volya-engine-build-dev";"#,
        &format!(r#"const CACHE = "volya-engine-build-{id}";"#),
    )?;
    replace_once(
        &dist.join("sw.js"),
        r#"const BUILD = "dev";"#,
        &format!(r#"const BUILD = "{commit}";"#),
    )?;
    replace_once(
        &dist.join("sw.js"),
        "const PRECACHE = [/* injected by tools/site */];",
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

/// Copies the directory `from` to `to`, following links (Bazel's inputs are
/// links into its store), with every file writable.
pub fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::write(&target, fs::read(entry.path())?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leb128_encodes() {
        assert_eq!(leb128(0), [0]);
        assert_eq!(leb128(127), [127]);
        assert_eq!(leb128(128), [0x80, 1]);
        assert_eq!(leb128(300), [0xac, 2]);
    }
}
