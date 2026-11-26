// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! 32-bit Windows programs, headless, in Apple's `container`: Wine in an
//! arm64 Debian container, its i386 side under QEMU user mode (Rosetta runs
//! no 32-bit x86 in Linux VMs, measured). The container has no display;
//! Wine draws into Xvfb and its audio driver is set to none.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};

use crate::{Result, cmd, root, run, step};

const IMAGE: &str = "volya-engine-wine:latest";

/// Memory of each run's VM.
const MEMORY: &str = "4G";

/// The hidden task [`Session`] starts: `<REAPER> <container> <stop-service> <stop-builder>`.
pub const REAPER: &str = "wine-reaper";

/// Run in the container before the program: registers QEMU for i386 ELF
/// binaries, creates the Wine prefix with no audio driver (an empty `Audio`
/// value: documented in Wine's mmdevapi as "explicitly chose no driver"),
/// and waits for the prefix's processes to end, so the program's own start
/// under Xvfb brings up its desktop.
const SETUP: &str = r#"set -e
mount -t binfmt_misc binfmt_misc /proc/sys/fs/binfmt_misc
cat /usr/lib/binfmt.d/qemu-i386.conf > /proc/sys/fs/binfmt_misc/register
wineboot -i >/dev/null 2>&1
wine reg add 'HKCU\Software\Wine\Drivers' /v Audio /t REG_SZ /d '' /f >/dev/null 2>&1
wineserver -w
"#;

const USAGE: &str = "\
Usage: cargo xtask wine image
       cargo xtask wine run DIR -- EXE [ARGS...]
       cargo xtask wine check";

/// `cargo xtask wine ...`; returns the exit code to pass on.
pub fn main(args: &[&str]) -> Result<u8> {
    match args {
        ["image"] => {
            let _session = Session::open(None)?;
            build_image()?;
            Ok(0)
        }
        ["run", dir, "--", exe, rest @ ..] => {
            let dir = fs::canonicalize(dir).map_err(|e| format!("{dir}: {e}"))?;
            let tail = r#"xvfb-run -a wine "$@""#;
            let mut program = vec![*exe];
            program.extend_from_slice(rest);
            in_container(&dir, tail, &program)
        }
        ["check"] => check(),
        _ => Err(USAGE.into()),
    }
}

/// Builds the D3D9 smoke test with MinGW in the container, runs it and
/// checks the pixel it saved.
fn check() -> Result<u8> {
    let dir = root().join("target/wine-check");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::copy(
        root().join("tools/xtask/wine/d3d9smoke.c"),
        dir.join("d3d9smoke.c"),
    )
    .map_err(|e| format!("d3d9smoke.c: {e}"))?;
    let tail = "i686-w64-mingw32-gcc -O2 -static -o d3d9smoke.exe d3d9smoke.c -ld3d9\n\
                xvfb-run -a wine d3d9smoke.exe";
    let code = in_container(&dir, tail, &[])?;
    if code != 0 {
        return Err(format!("the smoke test exited with {code}"));
    }
    let bmp = dir.join("d3d9smoke.bmp");
    let bytes = fs::read(&bmp).map_err(|e| format!("{}: {e}", bmp.display()))?;
    let pixel = centre_pixel(&bytes).ok_or("d3d9smoke.bmp: not a 32-bit BMP")?;
    if pixel != 0x2080e0 {
        return Err(format!("centre pixel 0x{pixel:06x}, expected 0x2080e0"));
    }
    step("Wine check passed: D3D9 cleared to 0x2080e0");
    Ok(0)
}

/// The centre pixel (0xRRGGBB) of a 32-bit BMP.
fn centre_pixel(bmp: &[u8]) -> Option<u32> {
    let u32_at = |at: usize| Some(u32::from_le_bytes(bmp.get(at..at + 4)?.try_into().ok()?));
    let offset = u32_at(10)? as usize;
    let width = u32_at(18)? as i32;
    let height = u32_at(22)? as i32;
    let bits = u16::from_le_bytes(bmp.get(28..30)?.try_into().ok()?);
    if bits != 32 || width <= 0 || height == 0 {
        return None;
    }
    let (width, rows) = (width as usize, height.unsigned_abs() as usize);
    let at = offset + (rows / 2 * width + width / 2) * 4;
    Some(u32_at(at)? & 0xff_ffff)
}

/// Runs `SETUP` then `tail` (a shell snippet given `args` as `$@`) in a new
/// container with `dir` at /work; returns the container's exit code. The
/// shell stays PID 1: `xvfb-run` as PID 1 never sees Xvfb's ready signal
/// (measured: it hangs).
fn in_container(dir: &Path, tail: &str, args: &[&str]) -> Result<u8> {
    let name = format!("volya-engine-wine-{}", std::process::id());
    let session = Session::open(Some(&name))?;
    if !quiet(&mut cmd("container", &["image", "inspect", IMAGE])) {
        build_image()?;
    }
    let mount = format!("type=bind,source={},target=/work", dir.display());
    let mut run = cmd("container", &["run", "--rm", "--progress", "none"]);
    run.args(["--name", &name, "--cap-add", "ALL", "--memory", MEMORY])
        .args(["--mount", &mount, "--workdir", "/work"]);
    if std::env::var_os("WINEDEBUG").is_some() {
        run.args(["--env", "WINEDEBUG"]);
    }
    let script = format!("{SETUP}{tail}");
    run.args([IMAGE, "sh", "-c", &script, "sh"]).args(args);
    let status = run.status().map_err(|e| format!("container: {e}"))?;
    drop(session);
    status
        .code()
        .and_then(|c| u8::try_from(c).ok())
        .ok_or_else(|| format!("container run ended with {status}"))
}

fn build_image() -> Result {
    step(&format!("Building {IMAGE}"));
    let context = root().join("tools/xtask/wine");
    let context = context.to_str().ok_or("non-UTF-8 path")?;
    let file = format!("{context}/Containerfile");
    run(&mut cmd(
        "container",
        &["build", "--tag", IMAGE, "--file", &file, context],
    ))
}

/// The `container` service for one task: started if it wasn't running (after
/// waiting for memory), and on drop left as it was found, with the task's
/// container and the image builder stopped. A reaper process does the same
/// if this process dies first (e.g. on Ctrl-C).
struct Session {
    container: String,
    stop_service: bool,
    stop_builder: bool,
    reaper: Option<ChildStdin>,
}

impl Session {
    fn open(container: Option<&str>) -> Result<Self> {
        crate::machine::wait_memory()?;
        let stop_service = !quiet(&mut cmd("container", &["system", "status"]));
        if stop_service {
            step("Starting the container service");
            run(&mut cmd("container", &["system", "start"]))?;
        }
        let stop_builder = !builder_running();
        let container = container.unwrap_or("").to_string();
        let flag = |b: bool| if b { "1" } else { "0" };
        let reaper = std::env::current_exe()
            .map_err(|e| format!("xtask: {e}"))
            .and_then(|exe| {
                Command::new(exe)
                    .args([REAPER, &container, flag(stop_service), flag(stop_builder)])
                    .process_group(0)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| format!("wine reaper: {e}"))
            });
        let mut session = Self {
            container,
            stop_service,
            stop_builder,
            reaper: None,
        };
        // Without a reaper, cleanup still runs on drop.
        session.reaper = reaper?.stdin.take();
        Ok(session)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        finish(&self.container, self.stop_service, self.stop_builder);
        if let Some(mut reaper) = self.reaper.take() {
            let _ = reaper.write_all(b"done");
        }
    }
}

/// Waits for its parent to close stdin; unless it wrote "done" first,
/// cleans up after it.
pub fn reap(container: &str, stop_service: &str, stop_builder: &str) -> Result {
    let mut said = String::new();
    let _ = std::io::stdin().read_to_string(&mut said);
    if said != "done" {
        finish(container, stop_service == "1", stop_builder == "1");
    }
    Ok(())
}

fn finish(container: &str, stop_service: bool, stop_builder: bool) {
    if !container.is_empty() {
        quiet(&mut cmd("container", &["stop", container]));
        quiet(&mut cmd("container", &["delete", "--force", container]));
    }
    if stop_builder {
        quiet(&mut cmd("container", &["builder", "stop"]));
    }
    // Another task may have started using the service meanwhile.
    if stop_service && running_containers().is_some_and(|c| c.is_empty()) {
        quiet(&mut cmd("container", &["system", "stop"]));
    }
}

fn builder_running() -> bool {
    running_containers().is_some_and(|c| c.iter().any(|id| id == "buildkit"))
}

/// IDs of running containers, the image builder's included.
fn running_containers() -> Option<Vec<String>> {
    let out = cmd("container", &["list", "--quiet"]).output().ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(str::to_string)
            .collect()
    })
}

/// Runs `command` without output; whether it succeeded.
fn quiet(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_centre_pixel() {
        let (w, h) = (4usize, 2usize);
        let mut bmp = vec![0u8; 54];
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&(w as u32).to_le_bytes());
        bmp[22..26].copy_from_slice(&(-(h as i32)).to_le_bytes());
        bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
        let mut pixels = vec![0u8; w * h * 4];
        let at = (w + 2) * 4;
        pixels[at..at + 4].copy_from_slice(&[0xe0, 0x80, 0x20, 0xff]);
        bmp.extend(pixels);
        assert_eq!(centre_pixel(&bmp), Some(0x2080e0));
        assert_eq!(centre_pixel(&bmp[..20]), None);
    }
}
