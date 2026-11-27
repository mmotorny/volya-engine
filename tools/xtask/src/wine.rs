// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! 32-bit Windows programs, headless, in Apple's `container`: Wine in an
//! arm64 Debian container, its x86 code under box64 (Rosetta runs no 32-bit
//! x86 in Linux VMs, measured), a 64-bit Wine in WoW64 mode (32-bit Windows
//! code in a 64-bit process). The container has no display;
//! Wine draws into Xvfb and its audio driver is set to none.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};

use crate::{Result, cmd, root, run, step};

/// The file the image is built from (`tools/xtask/wine/`).
const CONTAINERFILE: &str = "Containerfile";

/// The image, tagged with a hash of its Containerfile and its parts
/// ([`parts`]): a changed file names an image not built yet, which
/// [`Container::spawn`] builds, so a run (and a Bazel action, which takes
/// them as inputs) never uses an image of other files.
fn image() -> Result<String> {
    use sha2::{Digest, Sha256};
    let file = root().join("tools/xtask/wine").join(CONTAINERFILE);
    let text = fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut hash = Sha256::new();
    hash.update(&text);
    for (_, path) in parts()? {
        let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    let hash: String = hash.finalize()[..6]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(format!("volya-engine-wine-box64:{hash}"))
}

/// The image's parts (build/wine_image.bzl), by the names the Containerfile
/// adds them as: Wine's and box64's builds, tarballs of what they install.
/// A Bazel action is given them (`VOLYA_WINE_PART`, `VOLYA_BOX64_PART`);
/// under `bazel run` they are built (from the disk cache, unless their
/// inputs changed: Wine's build takes about an hour).
fn parts() -> Result<&'static [(&'static str, PathBuf); 2]> {
    static PARTS: std::sync::OnceLock<[(&str, PathBuf); 2]> = std::sync::OnceLock::new();
    if let Some(parts) = PARTS.get() {
        return Ok(parts);
    }
    let names = [
        ("wine.tar.gz", "VOLYA_WINE_PART"),
        ("box64.tar.gz", "VOLYA_BOX64_PART"),
    ];
    let given: Option<Vec<PathBuf>> = names
        .iter()
        .map(|(_, var)| std::env::var_os(var).map(|p| crate::runfile(Path::new(&p))))
        .collect();
    let paths = match given {
        Some(paths) => paths,
        None => {
            let targets = ["//tools/xtask:wine-part", "//tools/xtask:box64-part"];
            step("Building the Wine image's parts (from the disk cache unless changed)");
            run(cmd("bazel", &["build"]).args(targets).current_dir(root()))?;
            // One query a target: a query's files come in no set order.
            let file = |target: &str| {
                let files = crate::output(
                    cmd("bazel", &["cquery", "--output=files", target]).current_dir(root()),
                )?;
                let file = files.lines().last().ok_or(format!("{target}: no output"))?;
                Ok::<_, String>(root().join(file.trim()))
            };
            targets.iter().map(|t| file(t)).collect::<Result<_>>()?
        }
    };
    let [wine, box64] = <[PathBuf; 2]>::try_from(paths).map_err(|_| "two parts expected")?;
    Ok(PARTS.get_or_init(|| [(names[0].0, wine), (names[1].0, box64)]))
}

/// Registers box64 for x86_64 ELF executables and shared objects
/// (e_machine 0x3e), Wine's Unix side.
const BINFMT: &str = concat!(
    r"printf '%s\n' ':box64:M::\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x3e\x00:",
    r"\xff\xff\xff\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff\xff\xfe\xff\xff\xff:/usr/bin/box64:'",
    " > /proc/sys/fs/binfmt_misc/register\n"
);

/// The 32-bit `reg`: in a WoW64 prefix it writes the registry view 32-bit
/// programs read.
pub const REG32: &str = "C:\\windows\\syswow64\\reg.exe";

/// Memory of each run's VM.
const MEMORY: &str = "4G";

/// The hidden task [`Session`] starts: `<REAPER> <container> <stop-service> <stop-builder>`.
pub const REAPER: &str = "wine-reaper";

/// Run in the container before the program, after box64's binfmt
/// registration ([`BINFMT`]): creates the Wine prefix with no
/// audio driver (an empty `Audio` value: documented in Wine's mmdevapi as
/// "explicitly chose no driver"),
/// and waits for the prefix's processes to end, so the program's own start
/// under Xvfb brings up its desktop.
const SETUP: &str = r#"wineboot -i >/dev/null 2>&1
wine reg add 'HKCU\Software\Wine\Drivers' /v Audio /t REG_SZ /d '' /f >/dev/null 2>&1
wineserver -w
"#;

const USAGE: &str = "\
Usage: xtask wine image
       xtask wine run DIR -- EXE [ARGS...]
       xtask wine check";

/// `xtask wine ...`; returns the exit code to pass on.
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

/// The frame clock's step in [`check`]'s clock test: 40 ms in 100 ns ticks.
const CHECK_STEP: i64 = 400_000;

/// The frame clock test's epoch, seconds since 1970, and the frame it holds
/// after for a second.
const CHECK_EPOCH: i64 = 1_767_225_600;
const CHECK_HOLD: u8 = 2;

/// The clock reads between frames that advance it by 1 ms in the clock
/// test (`WINE_FRAME_SPIN`), and the wait it spins through before its first
/// frame, in 100 ns ticks (`clocksmoke.c`).
const CHECK_SPIN: u32 = 1000;
const CHECK_SPUN: i64 = 1_000_000;

/// Builds the smoke tests with MinGW in the container and runs them: the
/// D3D9 one, checking the pixel it saved, then the clock one on the frame
/// clock (docs/testing.md), held after frame [`CHECK_HOLD`] for a second,
/// after a busy wait the clock's reads advance ([`CHECK_SPIN`]), checking
/// every time it read.
fn check() -> Result<u8> {
    let dir = root().join("target/wine-check");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for file in ["d3d9smoke.c", "clocksmoke.c"] {
        fs::copy(root().join("tools/xtask/wine").join(file), dir.join(file))
            .map_err(|e| format!("{file}: {e}"))?;
    }
    let tail = format!(
        "i686-w64-mingw32-gcc -O2 -static -o d3d9smoke.exe d3d9smoke.c -ld3d9\n\
         i686-w64-mingw32-gcc -O2 -static -o clocksmoke.exe clocksmoke.c -ld3d9 -lwinmm\n\
         xvfb-run -a wine d3d9smoke.exe\n\
         wine reg add 'HKCU\\Software\\Wine\\Direct3D' /v csmt /t REG_DWORD /d 0 /f >/dev/null 2>&1\n\
         wineserver -w\n\
         clock=/tmp/frame-clock\n\
         head -c 32 /dev/zero > $clock\n\
         printf '\\{CHECK_HOLD:03o}' | dd of=$clock bs=1 seek=8 conv=notrunc status=none\n\
         WINE_FRAME_CLOCK=$clock WINE_FRAME_STEP={CHECK_STEP} WINE_FRAME_EPOCH={CHECK_EPOCH} \
         WINE_FRAME_SPIN={CHECK_SPIN} xvfb-run -a wine clocksmoke.exe &\n\
         held=\n\
         for i in $(seq 600); do\n\
         held=$(od -An -t d8 -j 16 -N 8 $clock | tr -d ' ')\n\
         [ \"$held\" = {CHECK_HOLD} ] && break\n\
         sleep 0.1\n\
         done\n\
         echo \"$held\" > held.txt\n\
         sleep 1\n\
         head -c 8 /dev/zero | dd of=$clock bs=1 seek=8 conv=notrunc status=none\n\
         wait $!"
    );
    let code = in_container(&dir, &tail, &[])?;
    if code != 0 {
        return Err(format!("the smoke tests exited with {code}"));
    }
    let bmp = dir.join("d3d9smoke.bmp");
    let bytes = fs::read(&bmp).map_err(|e| format!("{}: {e}", bmp.display()))?;
    let pixel = centre_pixel(&bytes).ok_or("d3d9smoke.bmp: not a 32-bit BMP")?;
    if pixel != 0x2080e0 {
        return Err(format!("centre pixel 0x{pixel:06x}, expected 0x2080e0"));
    }
    step("D3D9 cleared to 0x2080e0");
    let held = fs::read_to_string(dir.join("held.txt")).map_err(|e| format!("held.txt: {e}"))?;
    if held.trim() != CHECK_HOLD.to_string() {
        return Err(format!(
            "the frame clock held after frame {:?}, expected {CHECK_HOLD}",
            held.trim()
        ));
    }
    let times = fs::read_to_string(dir.join("clocksmoke.txt"))
        .map_err(|e| format!("clocksmoke.txt: {e}"))?;
    check_times(&times)?;
    step("Wine check passed: D3D9 cleared, the frame clock steps, holds and ends a busy wait");
    Ok(0)
}

/// Checks the clock test's times (`clocksmoke.c`): each frame's two reads
/// equal (the clock stands still over a real sleep and the hold), the busy
/// wait before them ended by its reads, and each time advanced by the step
/// a frame from the first read's.
fn check_times(text: &str) -> Result {
    let rows: Vec<Vec<i64>> = text
        .lines()
        .map(|l| l.split_whitespace().map(|v| v.parse::<i64>()).collect())
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| format!("clocksmoke.txt: {e}"))?;
    if rows.len() != 10 || rows.iter().any(|r| r.len() != 7) {
        return Err(format!("clocksmoke.txt: unexpected contents:\n{text}"));
    }
    // The ticks at the first read: the busy wait's, after any frames
    // presented before the test's first (a window's, a device's).
    let first = rows[0][1];
    if first < CHECK_SPUN {
        return Err(format!(
            "clocksmoke.txt: the first read at {first} ticks, before the busy wait's end"
        ));
    }
    let epoch = (CHECK_EPOCH + 11_644_473_600) * 10_000_000;
    for (i, row) in rows.iter().enumerate() {
        let ticks = first + row[0] * CHECK_STEP;
        // GetTickCount and timeGetTime are 32-bit milliseconds.
        let ms = (ticks / 10_000) & 0xffff_ffff;
        let want = [
            row[0],
            ticks,
            10_000_000,
            ms,
            ms,
            epoch + ticks,
            ticks * 100,
        ];
        if row[..] != want[..] {
            return Err(format!(
                "clocksmoke.txt line {}: {row:?}, expected {want:?} \
                 (frame qpc qpf tick time filetime tsc)",
                i + 1
            ));
        }
    }
    Ok(())
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
/// container with `dir` at /work; returns the container's exit code.
fn in_container(dir: &Path, tail: &str, args: &[&str]) -> Result<u8> {
    let mut container = Container::spawn(dir, tail, args, Stdio::inherit(), Stdio::inherit())?;
    let status = container.wait()?;
    status
        .code()
        .and_then(|c| u8::try_from(c).ok())
        .ok_or_else(|| format!("container run ended with {status}"))
}

/// A container running `SETUP` then a shell snippet; stopped and deleted
/// when dropped (or by the reaper if this process dies first).
pub struct Container {
    name: String,
    child: Child,
    session: Option<Session>,
}

impl Container {
    /// Starts `SETUP` then `tail` (a shell snippet given `args` as `$@`) in a
    /// new container with `dir` at /work, its output to `stdout` and
    /// `stderr`. `WINEDEBUG` and `WINEDLLOVERRIDES` are passed through. The
    /// shell stays PID 1: `xvfb-run` as PID 1 never sees Xvfb's ready signal
    /// (measured: it hangs).
    pub fn spawn(
        dir: &Path,
        tail: &str,
        args: &[&str],
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<Self> {
        let name = format!("volya-engine-wine-{}", std::process::id());
        let session = Session::open(Some(&name))?;
        let image = image()?;
        if !quiet(&mut cmd("container", &["image", "inspect", &image])) {
            build_image()?;
        }
        let mount = format!("type=bind,source={},target=/work", dir.display());
        let mut run = cmd("container", &["run", "--rm", "--progress", "none"]);
        run.args(["--name", &name, "--cap-add", "ALL", "--memory", MEMORY])
            .args(["--mount", &mount, "--workdir", "/work"]);
        for var in ["WINEDEBUG", "WINEDLLOVERRIDES"] {
            if std::env::var_os(var).is_some() {
                run.args(["--env", var]);
            }
        }
        let script = format!(
            "set -e\nmount -t binfmt_misc binfmt_misc /proc/sys/fs/binfmt_misc\n{BINFMT}{SETUP}{tail}"
        );
        run.args([image.as_str(), "sh", "-c", &script, "sh"])
            .args(args);
        let child = run
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .map_err(|e| format!("container: {e}"))?;
        Ok(Self {
            name,
            child,
            session: Some(session),
        })
    }

    /// The container's exit status if it has ended, without waiting.
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.child.try_wait().map_err(|e| format!("container: {e}"))
    }

    /// Waits for the container to end.
    pub fn wait(&mut self) -> Result<ExitStatus> {
        self.child.wait().map_err(|e| format!("container: {e}"))
    }

    /// Runs `args` in the running container; its standard output.
    pub fn exec(&self, args: &[&str]) -> Result<Vec<u8>> {
        let out = cmd("container", &["exec", &self.name])
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("container exec: {e}"))?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(format!("container exec {args:?}: {}", out.status))
        }
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        // Stopping the container ends the `container run` child.
        drop(self.session.take());
        let _ = self.child.wait();
    }
}

fn build_image() -> Result {
    let image = image()?;
    step(&format!("Building {image}"));
    // The build context: the Containerfile's parts, by name.
    let context = std::env::temp_dir().join(format!("volya-wine-image-{}", std::process::id()));
    fs::create_dir_all(&context).map_err(|e| format!("{}: {e}", context.display()))?;
    let copied = parts()?.iter().try_for_each(|(name, path)| {
        fs::copy(path, context.join(name))
            .map(drop)
            .map_err(|e| format!("{}: {e}", path.display()))
    });
    let file = root().join("tools/xtask/wine").join(CONTAINERFILE);
    let built = copied.and_then(|()| {
        let file = file.to_str().ok_or("non-UTF-8 path")?;
        let context = context.to_str().ok_or("non-UTF-8 path")?;
        run(&mut cmd(
            "container",
            &["build", "--tag", &image, "--file", file, context],
        ))
    });
    let _ = fs::remove_dir_all(&context);
    built
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
