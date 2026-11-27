// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Wine image's parts as Bazel actions (build/wine_image.bzl), so the
//! disk cache keeps them: `image-part PART OUT INPUT...` builds `PART`
//! (`wine` or `box64`) in a fresh Debian container from `INPUT`s (copied
//! to `/in` under their file names) and writes `OUT`, a gzipped tarball of
//! the files it installs, relative to `/`. The image (`wine.rs`) only adds
//! the tarballs.
//!
//! - Wine (LGPL): the fork's archive, `wine.tar.gz`, built as x86_64 under
//!   Rosetta (an amd64 container) in WoW64 mode, into `/opt/wine`.
//! - box64 (MIT): its release, `box64.tar.gz`, with our patch,
//!   `frame-clock.patch` (third_party/box64/), into `/usr/bin/box64` and
//!   the x86_64 libraries it runs emulated rather than wraps (Wine needs
//!   its libgcc_s) where its install puts them.

#![allow(dead_code)] // The modules' other tasks are xtask's.

mod common;
mod machine;

use std::fs;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{ChildStdin, Command, ExitCode, Stdio};

pub use common::{Result, cmd, output, root, run, runfile, step};

/// A part's build: the container's platform, size and script (`/in` its
/// inputs, `/out/part.tar.gz` its tarball).
struct Part {
    platform: &'static str,
    cpus: &'static str,
    memory: &'static str,
    script: &'static str,
}

/// The build's output beyond its last lines goes to `/out/build.log`, so
/// the action's log stays short.
const WINE: Part = Part {
    platform: "linux/amd64",
    cpus: "8",
    memory: "8G",
    script: r#"set -e
log=/out/build.log
apt-get update >$log 2>&1
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
  build-essential flex bison gcc-mingw-w64-i686 gcc-mingw-w64-x86-64 \
  libx11-dev libxext-dev libxrender-dev libxrandr-dev libxi-dev libxcursor-dev \
  libxcomposite-dev libxinerama-dev libxfixes-dev libxxf86vm-dev libxkbcommon-dev \
  libfreetype-dev libfontconfig-dev libgl-dev libegl-dev libvulkan-dev >>$log 2>&1
mkdir /src && tar -xzf /in/wine.tar.gz -C /src --strip-components=1 && cd /src
{
./configure --prefix=/opt/wine --enable-archs=i386,x86_64 --disable-tests \
  CFLAGS=-O2 CROSSCFLAGS=-O2 \
  --without-gstreamer --without-pulse --without-alsa --without-oss --without-cups \
  --without-sane --without-gphoto --without-v4l2 --without-pcap --without-netapi \
  --without-krb5 --without-gssapi --without-capi --without-usb --without-udev \
  --without-dbus --without-wayland --without-ffmpeg --without-sdl --without-unwind
make -j"$(nproc)"
make install DESTDIR=/stage
} >>$log 2>&1 || { tail -40 $log; exit 1; }
tar -czf /out/part.tar.gz -C /stage .
"#,
};

const BOX64: Part = Part {
    platform: "linux/arm64",
    cpus: "4",
    memory: "4G",
    script: r#"set -e
log=/out/build.log
apt-get update >$log 2>&1
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
  build-essential cmake python3 patch >>$log 2>&1
mkdir /src && tar -xzf /in/box64.tar.gz -C /src --strip-components=1 && cd /src
{
patch -p1 < /in/frame-clock.patch
cmake -S . -B build -DARM_DYNAREC=ON -DCMAKE_BUILD_TYPE=RelWithDebInfo
cmake --build build -j"$(nproc)"
} >>$log 2>&1 || { tail -40 $log; exit 1; }
mkdir -p /stage/usr/bin /stage/usr/lib
cp build/box64 /stage/usr/bin/box64
cp -R x64lib /stage/usr/lib/box64-x86_64-linux-gnu
tar -czf /out/part.tar.gz -C /stage .
"#,
};

/// The image the parts build in.
const BASE: &str = "debian:trixie-slim";

/// The hidden task that stops a part's container if the action dies first:
/// `<REAPER> <container> <stop-service>`.
const REAPER: &str = "image-part-reaper";

fn main() -> ExitCode {
    common::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        [REAPER, container, stop_service] => reap(container, stop_service),
        ["wine", out, inputs @ ..] => build(&WINE, Path::new(out), inputs),
        ["box64", out, inputs @ ..] => build(&BOX64, Path::new(out), inputs),
        _ => Err("usage: image-part wine|box64 OUT INPUT...".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("image-part: {error}");
            ExitCode::FAILURE
        }
    }
}

fn build(part: &Part, out: &Path, inputs: &[&str]) -> Result {
    let dir = std::env::temp_dir().join(format!("volya-image-part-{}", std::process::id()));
    let (into, from) = (dir.join("in"), dir.join("out"));
    for d in [&into, &from] {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    for input in inputs {
        let input = runfile(Path::new(input));
        let name = input.file_name().ok_or("an input without a name")?;
        fs::copy(&input, into.join(name)).map_err(|e| format!("{}: {e}", input.display()))?;
    }
    let result = in_container(part, &dir).and_then(|()| {
        let tar = from.join("part.tar.gz");
        fs::copy(&tar, out)
            .map(drop)
            .map_err(|e| format!("{}: {e}", tar.display()))
    });
    if result.is_err() {
        let log = from.join("build.log");
        if log.is_file() {
            let kept =
                std::env::temp_dir().join(format!("volya-image-part-{}.log", std::process::id()));
            if fs::copy(&log, &kept).is_ok() {
                eprintln!("image-part: the build's log is kept in {}", kept.display());
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
    result
}

/// Runs the part's script in a container with `dir/in` and `dir/out`
/// mounted, the `container` service started first if it wasn't running
/// (and left as found), after waiting for memory.
fn in_container(part: &Part, dir: &Path) -> Result {
    machine::wait_memory()?;
    let stop_service = !quiet(&mut cmd("container", &["system", "status"]));
    if stop_service {
        run(&mut cmd("container", &["system", "start"]))?;
    }
    let name = format!("volya-engine-image-part-{}", std::process::id());
    let _guard = Guard::new(&name, stop_service)?;
    let mount = |sub: &str| format!("type=bind,source={},target=/{sub}", dir.join(sub).display());
    let mut command = cmd("container", &["run", "--rm", "--progress", "none"]);
    command
        .args(["--name", &name, "--platform", part.platform])
        .args(["--cpus", part.cpus, "--memory", part.memory])
        .args(["--mount", &mount("in"), "--mount", &mount("out")]);
    if part.platform == "linux/amd64" {
        command.arg("--rosetta");
    }
    command.args([BASE, "sh", "-c", part.script]);
    run(command.stdin(Stdio::null()))
}

/// Stops the part's container, and the service if this task started it,
/// on drop; a reaper process does the same if this one dies first.
struct Guard {
    container: String,
    stop_service: bool,
    reaper: Option<ChildStdin>,
}

impl Guard {
    fn new(container: &str, stop_service: bool) -> Result<Self> {
        let exe = std::env::current_exe().map_err(|e| format!("image-part: {e}"))?;
        let mut reaper = Command::new(exe)
            .args([REAPER, container, if stop_service { "1" } else { "0" }])
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("image-part reaper: {e}"))?;
        Ok(Self {
            container: container.to_string(),
            stop_service,
            reaper: reaper.stdin.take(),
        })
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        finish(&self.container, self.stop_service);
        if let Some(mut reaper) = self.reaper.take() {
            let _ = reaper.write_all(b"done");
        }
    }
}

/// Waits for its parent to close stdin; unless it wrote "done" first,
/// cleans up after it.
fn reap(container: &str, stop_service: &str) -> Result {
    let mut said = String::new();
    let _ = std::io::stdin().read_to_string(&mut said);
    if said != "done" {
        finish(container, stop_service == "1");
    }
    Ok(())
}

fn finish(container: &str, stop_service: bool) {
    quiet(&mut cmd("container", &["stop", container]));
    quiet(&mut cmd("container", &["delete", "--force", container]));
    // Another task may have started using the service meanwhile.
    let idle = cmd("container", &["list", "--quiet"])
        .output()
        .is_ok_and(|o| o.status.success() && o.stdout.trim_ascii().is_empty());
    if stop_service && idle {
        quiet(&mut cmd("container", &["system", "stop"]));
    }
}

/// Runs `command` without output; whether it succeeded.
fn quiet(command: &mut Command) -> bool {
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
