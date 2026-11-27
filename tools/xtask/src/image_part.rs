// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Wine image's parts as Bazel actions (build/wine_image.bzl), so the
//! disk cache keeps them: `image-part PART OUT INPUT...` builds `PART`
//! (`wine` or `box64`) on this machine from `INPUT`s with cross toolchains
//! whose compilers run natively, and writes `OUT`, a gzipped tarball of the
//! files it installs, relative to `/`. The image (`wine.rs`) only adds the
//! tarballs.
//!
//! The toolchains come from the action's environment, as paths into its
//! inputs: `VOLYA_CLANG` (llvm-mingw's clang, beside lld and LLVM's tools),
//! `VOLYA_SYSROOT` (a Debian sysroot's marker; the tree is `unpacked/`
//! beside it), `VOLYA_MAKE`, and per part the rest.
//!
//! - Wine (LGPL): the fork's archive, `wine.tar.gz`, cross-built as Wine's
//!   own documentation says (`--host`, `--with-wine-tools`): first its
//!   build tools for this machine (Xcode's clang; `VOLYA_BISON`,
//!   `VOLYA_FLEX`, and FreeType, `VOLYA_HOST_FREETYPE`, for `sfnt2fon`),
//!   then Wine for x86_64 Linux in WoW64 mode, its Unix side with clang and
//!   the x86_64 sysroot, its PE side with llvm-mingw (`--with-mingw`), into
//!   `/opt/wine`.
//! - box64 (MIT): its release with our patch, applied by Bazel
//!   (third_party/box64/), the archive's files as `INPUT`s; built for arm64
//!   Linux with CMake (`VOLYA_CMAKE`, `VOLYA_PYTHON` for its generators)
//!   into `/usr/bin/box64` and the x86_64 libraries it runs emulated rather
//!   than wraps (Wine needs its libgcc_s) where its install puts them.

#![allow(dead_code)] // The modules' other tasks are xtask's.

mod common;
mod machine;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use common::{Result, cmd, output, root, run, runfile, step};

fn main() -> std::process::ExitCode {
    common::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        [part @ ("wine" | "box64"), out, inputs @ ..] => build(part, Path::new(out), inputs),
        _ => Err("usage: image-part wine|box64 OUT INPUT...".into()),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("image-part: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// A build's directory and log: everything a step prints goes to the log,
/// so the action's own output stays short.
struct Build {
    dir: PathBuf,
    log: PathBuf,
    jobs: String,
    /// What every step runs with: `PATH`, and bison's data and m4 as
    /// absolute paths (the action gives them relative to its directory).
    env: Vec<(String, String)>,
}

impl Build {
    /// Runs `command` in `dir`, its output appended to the log.
    fn run(&self, dir: &Path, command: &mut Command) -> Result {
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .map_err(|e| format!("{}: {e}", self.log.display()))?;
        let err = log.try_clone().map_err(|e| e.to_string())?;
        command
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(err);
        run(command)
    }

    fn make(&self, dir: &Path, targets: &[&str]) -> Result {
        let mut make = Command::new(tool("VOLYA_MAKE")?);
        make.arg(format!("-j{}", self.jobs)).args(targets);
        self.run(dir, &mut make)
    }
}

fn build(part: &str, out: &Path, inputs: &[&str]) -> Result {
    machine::wait_memory()?;
    let dir = std::env::temp_dir().join(format!("volya-image-part-{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dir = fs::canonicalize(&dir).map_err(|e| e.to_string())?;
    let jobs = std::thread::available_parallelism().map_or(8, |n| n.get());
    let mut env = Vec::new();
    for var in ["BISON_PKGDATADIR", "M4"] {
        if std::env::var_os(var).is_some() {
            env.push((var.to_owned(), tool(var)?.display().to_string()));
        }
    }
    let mut build = Build {
        log: dir.join("build.log"),
        dir,
        jobs: jobs.to_string(),
        env,
    };
    let stage = build.dir.join("stage");
    let result = match part {
        "wine" => wine(&mut build, inputs, &stage),
        _ => box64(&build, inputs, &stage),
    }
    .and_then(|()| tarball(&stage, out));
    if result.is_err() {
        let kept = std::env::temp_dir().join(format!("volya-image-part-{part}.log"));
        if fs::copy(&build.log, &kept).is_ok() {
            if let Ok(text) = fs::read_to_string(&kept) {
                let lines: Vec<&str> = text.lines().collect();
                eprintln!("{}", lines[lines.len().saturating_sub(40)..].join("\n"));
            }
            eprintln!("image-part: the build's log is kept in {}", kept.display());
        }
    }
    let _ = fs::remove_dir_all(&build.dir);
    result
}

/// An input the action's environment names (`var`), as an absolute path
/// with links resolved, so it holds wherever a build step runs.
fn tool(var: &str) -> Result<PathBuf> {
    let path = std::env::var_os(var).ok_or(format!("{var} is not set"))?;
    let path = runfile(Path::new(&path));
    fs::canonicalize(&path).map_err(|e| format!("{var}: {}: {e}", path.display()))
}

/// The LLVM directory (`bin/clang`'s) and the sysroot's tree.
fn toolchain() -> Result<(PathBuf, PathBuf)> {
    let clang = tool("VOLYA_CLANG")?;
    let bin = clang
        .parent()
        .ok_or("clang has no directory")?
        .to_path_buf();
    let marker = tool("VOLYA_SYSROOT")?;
    let sysroot = marker.with_file_name("unpacked");
    Ok((bin, sysroot))
}

/// A directory of links for `PATH`: `name` to `target`.
fn links(dir: &Path, links: &[(String, PathBuf)]) -> Result {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, target) in links {
        std::os::unix::fs::symlink(target, dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(())
}

/// Configure's options shared by both of Wine's builds: what the image
/// has no use for (sound, printing, scanners, networking services) is off.
const WINE_WITHOUT: &[&str] = &[
    "--disable-tests",
    "--without-alsa",
    "--without-capi",
    "--without-cups",
    "--without-dbus",
    "--without-ffmpeg",
    "--without-gphoto",
    "--without-gssapi",
    "--without-gstreamer",
    "--without-krb5",
    "--without-netapi",
    "--without-oss",
    "--without-pcap",
    "--without-pulse",
    "--without-sane",
    "--without-sdl",
    "--without-udev",
    "--without-unwind",
    "--without-usb",
    "--without-v4l2",
    "--without-wayland",
];

fn wine(build: &mut Build, inputs: &[&str], stage: &Path) -> Result {
    let (llvm, sysroot) = toolchain()?;
    let [archive] = inputs else {
        return Err("Wine takes its archive".into());
    };
    let src = build.dir.join("src");
    fs::create_dir_all(&src).map_err(|e| e.to_string())?;
    let archive = fs::canonicalize(runfile(Path::new(archive))).map_err(|e| e.to_string())?;
    build.run(
        &src,
        Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("--strip-components=1"),
    )?;

    // What configure finds on PATH: the build tools, the Unix side's
    // binutils (LLVM's, by their GNU names) and llvm-mingw's compilers.
    let bin = build.dir.join("bin");
    let mut names = vec![
        ("bison".to_owned(), tool("VOLYA_BISON")?),
        ("flex".to_owned(), tool("VOLYA_FLEX")?),
        ("make".to_owned(), tool("VOLYA_MAKE")?),
    ];
    for t in [
        "ar", "nm", "objcopy", "objdump", "ranlib", "readelf", "strip",
    ] {
        names.push((
            format!("x86_64-linux-gnu-{t}"),
            llvm.join(format!("llvm-{t}")),
        ));
    }
    names.push(("x86_64-linux-gnu-ld".to_owned(), llvm.join("ld.lld")));
    links(&bin, &names)?;
    let path = format!(
        "{}:{}:/usr/bin:/bin:/usr/sbin:/sbin",
        bin.display(),
        llvm.display()
    );
    build.env.push(("PATH".to_owned(), path));
    let configure = src.join("configure");
    let configure = |dir: &Path| {
        let mut c = Command::new(&configure);
        c.args(WINE_WITHOUT);
        let _ = fs::create_dir_all(dir);
        c
    };

    // Wine's tools for this machine (Xcode's clang), FreeType's static
    // library standing for the shared one configure looks for: `sfnt2fon`
    // only needs it linked.
    let tools = build.dir.join("tools");
    let freetype = tool("VOLYA_HOST_FREETYPE")?;
    let freetype_include = tool("VOLYA_HOST_FREETYPE_INCLUDE")?;
    build.run(
        &tools,
        configure(&tools)
            .args([
                "--with-mingw=llvm-mingw",
                "--without-x",
                "--without-fontconfig",
                "--without-gnutls",
                "--without-opencl",
                "--without-vulkan",
                "--without-opengl",
                "--without-coreaudio",
                "--without-inotify",
            ])
            .arg(format!("FREETYPE_CFLAGS=-I{}", freetype_include.display()))
            .arg(format!("FREETYPE_LIBS={}", freetype.display()))
            .arg("ac_cv_lib_soname_freetype=libfreetype.6.dylib"),
    )?;
    build.make(&tools, &["__tooldeps__"])?;

    // Wine for x86_64 Linux. pkg-config is not used: the packages' flags
    // are given (`-I=` is the sysroot's).
    let cross = build.dir.join("build");
    let cc = format!(
        "{} --target=x86_64-linux-gnu --sysroot={} --start-no-unused-arguments -fuse-ld=lld --end-no-unused-arguments",
        llvm.join("clang").display(),
        sysroot.display()
    );
    build.run(
        &cross,
        configure(&cross)
            .args([
                "--host=x86_64-linux-gnu",
                "--with-mingw=llvm-mingw",
                "--prefix=/opt/wine",
                "--enable-archs=i386,x86_64",
                // Its tools for Linux too, as a native build installs them:
                // `bin/wine`, the launcher, is one.
                "--enable-tools",
            ])
            .arg(format!("--with-wine-tools={}", tools.display()))
            .arg(format!("CC={cc}"))
            .args([
                "CFLAGS=-O2",
                "CROSSCFLAGS=-O2",
                "FREETYPE_CFLAGS=-I=/usr/include/freetype2",
                "FONTCONFIG_CFLAGS=-I=/usr/include",
            ]),
    )?;
    build.make(&cross, &[])?;
    build.make(
        &cross,
        &["install", &format!("DESTDIR={}", stage.display())],
    )
}

fn box64(build: &Build, inputs: &[&str], stage: &Path) -> Result {
    let (llvm, sysroot) = toolchain()?;
    // The archive's files, by their paths in its repository: copied, as
    // its generators write into the tree.
    let src = build.dir.join("src");
    let mut copied = 0;
    for input in inputs {
        let Some((_, relative)) = input.split_once("box64_src/") else {
            continue;
        };
        let to = src.join(relative);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(runfile(Path::new(input)), &to).map_err(|e| format!("{input}: {e}"))?;
        copied += 1;
    }
    if copied == 0 {
        return Err("box64 takes its archive's files".into());
    }
    let out = build.dir.join("build");
    let clang = llvm.join("clang");
    let tool = |name: &str| llvm.join(name).display().to_string();
    let mut cmake = Command::new(self::tool("VOLYA_CMAKE")?);
    cmake
        .arg("-S")
        .arg(&src)
        .arg("-B")
        .arg(&out)
        .args(["-G", "Unix Makefiles"])
        .arg(format!(
            "-DCMAKE_MAKE_PROGRAM={}",
            self::tool("VOLYA_MAKE")?.display()
        ))
        .args([
            "-DCMAKE_SYSTEM_NAME=Linux",
            "-DCMAKE_SYSTEM_PROCESSOR=aarch64",
            "-DCMAKE_C_COMPILER_TARGET=aarch64-linux-gnu",
            "-DCMAKE_ASM_COMPILER_TARGET=aarch64-linux-gnu",
            "-DCMAKE_LINKER_TYPE=LLD",
            "-DARM_DYNAREC=ON",
            "-DCMAKE_BUILD_TYPE=RelWithDebInfo",
        ])
        .arg(format!("-DCMAKE_SYSROOT={}", sysroot.display()))
        .arg(format!("-DCMAKE_C_COMPILER={}", clang.display()))
        .arg(format!("-DCMAKE_ASM_COMPILER={}", clang.display()))
        .arg(format!("-DCMAKE_AR={}", tool("llvm-ar")))
        .arg(format!("-DCMAKE_RANLIB={}", tool("llvm-ranlib")))
        .arg(format!("-DCMAKE_STRIP={}", tool("llvm-strip")))
        .arg(format!("-DCMAKE_OBJCOPY={}", tool("llvm-objcopy")))
        .arg(format!(
            "-DPython3_EXECUTABLE={}",
            self::tool("VOLYA_PYTHON")?.display()
        ))
        // The tree's paths in its debug information, as the old builds'.
        .arg(format!(
            "-DCMAKE_C_FLAGS=-ffile-prefix-map={}=/src",
            src.display()
        ));
    build.run(&build.dir, &mut cmake)?;
    build.make(&out, &[])?;
    let bin = stage.join("usr/bin");
    let lib = stage.join("usr/lib/box64-x86_64-linux-gnu");
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    fs::copy(out.join("box64"), bin.join("box64")).map_err(|e| format!("box64: {e}"))?;
    copy_tree(&src.join("x64lib"), &lib)
}

fn copy_tree(from: &Path, to: &Path) -> Result {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &to.join(entry.file_name()))?;
        } else {
            fs::copy(&path, to.join(entry.file_name()))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// `stage` as a gzipped tarball, owned by root, without macOS's metadata
/// (`._` files, extended attributes).
fn tarball(stage: &Path, out: &Path) -> Result {
    let out = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(out);
    let mut tar = Command::new("tar");
    tar.env("COPYFILE_DISABLE", "1")
        .args([
            "--no-mac-metadata",
            "--no-xattrs",
            "--uid",
            "0",
            "--gid",
            "0",
        ])
        .args(["--uname", "root", "--gname", "root", "-czf"])
        .arg(&out)
        .arg("-C")
        .arg(stage)
        .arg(".");
    run(&mut tar)
}
