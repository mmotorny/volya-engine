// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `cargo xtask original`: the shipped game run as a black box
//! (docs/clean-room.md, Sources): its install folder assembled from the
//! user's GOG installer, its executable started unmodified under
//! `cargo xtask wine`, and the virtual display captured to PNG from outside.
//! Everything it writes is game assets and stays in the local cache.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::wine::{Container, REG32};
use crate::{Result, step};

mod views;
pub use views::capture;

/// The executable and its working folder, relative to the install folder,
/// and its arguments: the launch task (gog-install-additions.md section 5:
/// `bin\XR_3DA.exe`, working folder `bin`, no arguments) plus the options
/// for an unattended start (steering-the-original.md sections 1.2 and 3.1:
/// no memory-check box, no crash dialog, sound off, the run's configuration
/// file instead of `user.ltx`).
const EXE: &str = "bin/XR_3DA.exe";
const WORKING_DIR: &str = "bin";
const ARGS: [&str; 5] = [
    "--skipmemcheck",
    "-silent_error_mode",
    "-nosound",
    "-ltx",
    STEER_LTX,
];

/// The run's configuration file, in the user-data folder
/// (steering-the-original.md section 2.2): the shipped `user.ltx` followed by
/// the views' settings (`views::SETTINGS`, section 3.1), whose lines run
/// after it and so override it.
const STEER_LTX: &str = "steer.ltx";

/// The Wine user (the container runs as root; measured: the prefix's only
/// user folder is `users/root`; tools/gog-extract names the same in
/// `fsgame.ltx`).
const WINE_USER: &str = "root";

/// The game's user folder below `{userdocs}`, as the support-data copies
/// name it (gog-install-additions.md section 3; the same folder as
/// `fsgame.ltx`'s `Stalker-SHOC`).
const USER_FOLDER: &str = "STALKER-SHOC";

/// In the container the run folder is `/work` and the install folder
/// `/work/game`; Wine's default `Z:` drive maps `/`.
const INSTALL_IN_CONTAINER: &str = "/work/game";
const INSTALL_IN_WINDOWS: &str = "Z:\\work\\game";

/// The virtual display, the size the game runs at (`views::WIDTH` × `views::HEIGHT`).
const SCREEN: &str = "1024x768x24";

/// Seconds between captures.
const EVERY: u64 = 5;

/// Seconds the game runs unless given.
const DEFAULT_SECONDS: u64 = 300;

/// Xvfb keeps its screen in this file (`-fbdir`, an XWD image).
const FRAMEBUFFER: &str = "/tmp/fb/Xvfb_screen0";

/// The virtual display and its authority file (`xvfb-run -n`, `-f`).
const DISPLAY: &str = "99";
const XAUTHORITY: &str = "/tmp/xauthority";

/// Seconds the game gets to end after its `quit` command.
const QUIT_WAIT: u64 = 60;

const USAGE: &str = "\
Usage: cargo xtask original [--new-game] [--quit] [SECONDS]
       cargo xtask original views [--again] [NAME...]";

/// A New Game, as the menu's New Game starts it (steering-the-original.md
/// section 3.2); `-start` takes the rest of the command line, so it comes
/// last (section 1.3).
const NEW_GAME: [&str; 3] = [
    "-start",
    "server(all/single/alife/new)",
    "client(localhost)",
];

/// `cargo xtask original ...`.
pub fn main(args: &[&str]) -> Result {
    if let ["capture", place, out] = args {
        return capture(place, Path::new(out));
    }
    let (new_game, args) = match args {
        ["--new-game", rest @ ..] => (true, rest),
        _ => (false, args),
    };
    let (quit, args) = match args {
        ["--quit", rest @ ..] => (true, rest),
        _ => (false, args),
    };
    let seconds = match args {
        [] => DEFAULT_SECONDS,
        [s] => s.parse().map_err(|e| format!("SECONDS: {e}\n{USAGE}"))?,
        _ => return Err(USAGE.into()),
    };
    let install = Install::assemble()?;
    let extra: &[&str] = if new_game { &NEW_GAME } else { &[] };
    let mut run = install.start(extra, &[], &format!("for {seconds} s"))?;
    let mut last: Option<Vec<u8>> = None;
    let mut ended = None;
    while run.elapsed() < Duration::from_secs(seconds) {
        thread::sleep(Duration::from_secs(EVERY));
        if let Some(status) = run.container.try_wait()? {
            ended = Some(status);
            break;
        }
        let at = run.elapsed().as_secs();
        if let Some(png) = run.capture().map(|(w, h, rgb)| png(w, h, &rgb)) {
            let changed = last.as_ref() != Some(&png);
            let path = run.frames.join(format!("{at:04}s.png"));
            fs::write(&path, &png).map_err(|e| format!("{}: {e}", path.display()))?;
            println!(
                "{at:>5} s  {}{}",
                path.display(),
                if changed { "" } else { " (unchanged)" }
            );
            last = Some(png);
        } else {
            println!("{at:>5} s  no display yet");
        }
    }
    if ended.is_none() {
        if let Some((w, h, rgb)) = run.capture() {
            let path = run.frames.join("final.png");
            fs::write(&path, png(w, h, &rgb)).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("final  {}", path.display());
        }
        save_user_files(&run.container, &run.dir.join("prefix"));
        if quit {
            ended = run.quit()?;
        }
    }
    run.finish(ended);
    Ok(())
}

/// The install folder, and where runs go.
pub struct Install {
    /// Where each run's folder goes.
    runs: PathBuf,
    folder: PathBuf,
}

impl Install {
    /// The install folder Bazel's `@gog_original` holds
    /// (docs/architecture/build.md, Game data), its manifest named by
    /// `VOLYA_GOG_ORIGINAL` (`bazel run //tools/xtask -- original`).
    /// Runs go to `<installer cache>/original-runs/`, where the user looks
    /// at them.
    pub fn assemble() -> Result<Self> {
        let mut install = Self::assemble_into(PathBuf::new())?;
        install.runs = cache()?.join("original-runs");
        Ok(install)
    }

    /// The install, its runs going to `runs`.
    pub fn assemble_into(runs: PathBuf) -> Result<Self> {
        let manifest = std::env::var_os("VOLYA_GOG_ORIGINAL").ok_or(
            "the original's install comes from Bazel: bazel run //tools/xtask -- original",
        )?;
        let manifest = fs::canonicalize(crate::runfile(Path::new(&manifest)))
            .map_err(|e| format!("{}: {e}", Path::new(&manifest).display()))?;
        if fs::metadata(&manifest).map_err(|e| e.to_string())?.len() == 0 {
            return Err(
                "no game data: set VOLYA_GOG_INSTALLER in user.bazelrc (docs/testing.md)".into(),
            );
        }
        let folder = manifest.parent().ok_or("no folder")?.join("game");
        Ok(Self { runs, folder })
    }

    /// Starts a run (`<runs>/<unix time>/`): the install
    /// cloned with what the install adds, `demos` (name, bytes) written as
    /// camera tracks to the saves folder (steering-the-original.md section
    /// 4.2), and the launch task started with [`ARGS`] and `extra` on the
    /// virtual display. `what` tells how long it runs, for the log.
    pub fn start(&self, extra: &[&str], demos: &[(&str, Vec<u8>)], what: &str) -> Result<Run> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let dir = self.runs.join(stamp.to_string());
        let game = dir.join("game");
        let documents = dir.join("documents");
        let frames = dir.join("frames");
        fs::create_dir_all(&frames).map_err(|e| format!("{}: {e}", frames.display()))?;
        // A copy-on-write clone (APFS), so what the game writes stays in this
        // run; the folder is Bazel's, read-only.
        crate::run(Command::new("cp").arg("-cR").arg(&self.folder).arg(&game))?;
        crate::run(Command::new("chmod").arg("-R").arg("u+w").arg(&game))?;
        add_install(&game, &documents)?;
        // The saves folder: `$game_saves$`, `savedgames\` below the user
        // folder (steering-the-original.md section 2.1).
        let saves = documents.join(USER_FOLDER).join("savedgames");
        fs::create_dir_all(&saves).map_err(|e| format!("{}: {e}", saves.display()))?;
        for (name, bytes) in demos {
            let path = saves.join(format!("{name}.xrdemo"));
            fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        let reg = dir.join("install.reg");
        fs::write(&reg, registry(INSTALL_IN_WINDOWS))
            .map_err(|e| format!("{}: {e}", reg.display()))?;

        let log_path = dir.join("wine.log");
        let log = File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
        let err = log.try_clone().map_err(|e| e.to_string())?;
        // The prefix's Documents folder is the run's `documents/`, so what the
        // game writes there (logs, settings) lands on the Mac.
        let tail = format!(
            "mkdir -p /tmp/fb\n\
             docs=\"$WINEPREFIX/drive_c/users/{WINE_USER}/Documents\"\n\
             rm -rf \"$docs\" && ln -s /work/documents \"$docs\"\n\
             wine '{REG32}' import 'Z:\\work\\install.reg'\n\
             wineserver -w\n\
             for view in 32 64; do\n\
             echo \"Install key, $view-bit view:\"\n\
             wine reg query '{INSTALL_KEY}' /reg:$view || true\n\
             done\n\
             cd {INSTALL_IN_CONTAINER}/{WORKING_DIR}\n\
             xvfb-run -n {DISPLAY} -f {XAUTHORITY} -s '-screen 0 {SCREEN} -fbdir /tmp/fb' wine \"$@\"\n\
             echo \"exit code $?\""
        );
        let exe = format!("{INSTALL_IN_CONTAINER}/{EXE}");
        let mut program = vec![exe.as_str()];
        program.extend(ARGS);
        program.extend(extra);
        step(&format!(
            "Running {EXE} {} {what}, captures in {}",
            program[1..].join(" "),
            frames.display()
        ));
        let container = Container::spawn(&dir, &tail, &program, log.into(), err.into())?;
        Ok(Run {
            dir,
            frames,
            log_path,
            container,
            start: Instant::now(),
        })
    }
}

/// A running game.
pub struct Run {
    /// The run's folder.
    pub dir: PathBuf,
    /// Where its captures go.
    pub frames: PathBuf,
    log_path: PathBuf,
    pub container: Container,
    start: Instant,
}

impl Run {
    /// Time since the container started.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// The display: width, height and RGB, or `None` while there is none.
    pub fn capture(&self) -> Option<(u32, u32, Vec<u8>)> {
        let xwd = self.container.exec(&["cat", FRAMEBUFFER]).ok()?;
        xwd_rgb(&xwd)
    }

    /// Types `keys` (an `xdotool` command line) into the virtual display.
    pub fn xdotool(&self, keys: &str) -> Result {
        let script = format!("export DISPLAY=:{DISPLAY} XAUTHORITY={XAUTHORITY}\n{keys}");
        self.container.exec(&["sh", "-c", &script]).map(drop)
    }

    /// Runs `command` in the game's console: the grave key opens it (the
    /// shipped bindings, steering-the-original.md section 2.2), the line is
    /// typed and entered (section 2.3).
    pub fn console(&self, command: &str) -> Result {
        let typed = command.replace('\'', "");
        self.xdotool(&format!(
            "xdotool key grave && sleep 1 && xdotool type '{typed}' && xdotool key Return"
        ))
    }

    /// Ends the game with its `quit` console command (steering-the-original.md
    /// section 7); the container's exit status once the game has ended, or
    /// `None` if it is still running after [`QUIT_WAIT`] seconds.
    pub fn quit(&mut self) -> Result<Option<ExitStatus>> {
        step("Quitting the game from its console");
        self.console("quit")?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(QUIT_WAIT) {
            thread::sleep(Duration::from_secs(1));
            if let Some(status) = self.container.try_wait()? {
                return Ok(Some(status));
            }
        }
        println!("The game was still running {QUIT_WAIT} s after quit");
        Ok(None)
    }

    /// Stops the container and reports how the run ended.
    pub fn finish(self, ended: Option<ExitStatus>) {
        let elapsed = self.elapsed().as_secs();
        drop(self.container);
        match ended {
            Some(status) => step(&format!(
                "The game ended after about {elapsed} s ({status})"
            )),
            None => step(&format!("Stopped the game after {elapsed} s")),
        }
        println!(
            "Captures: {}\nWine's log: {}",
            self.frames.display(),
            self.log_path.display()
        );
    }
}

/// The install script's key (gog-install-additions.md section 4.1); each
/// run's `wine.log` lists it in both registry views, so where the values
/// landed is on record (measured: the 32-bit view only, as
/// `HKLM\SOFTWARE\Wow6432Node\…` to 64-bit programs).
const INSTALL_KEY: &str = "HKLM\\SOFTWARE\\GSC Game World\\STALKER-SHOC";

/// The registry entries of the install, as a file for `reg import`:
/// - the install script's five values (gog-install-additions.md section
///   4.1; `$1` believed the DWORD 1), imported into the 32-bit view the
///   game reads (section 4.1, [`REG32`]);
/// - the installer's own `GOG.com` values (section 4.2), less `lang_code`,
///   whose value is not established; `installer_language` is the `/LANG`
///   value, the Russian edition's `russian` (sections 1 and 4.2).
///
/// The game reads none but `InstallVers` (section 4.1). Not written: the
/// uninstall registration and the compatibility flag (only believed,
/// sections 4.2 and 5).
fn registry(app: &str) -> String {
    let app = app.replace('\\', "\\\\");
    format!(
        "REGEDIT4\r\n\r\n\
         [HKEY_LOCAL_MACHINE\\SOFTWARE\\GSC Game World\\STALKER-SHOC]\r\n\
         \"InstallPath\"=\"{app}\"\r\n\
         \"InstallSource\"=\"GOG\"\r\n\
         \"InstallVers\"=\"2.1.0.10\"\r\n\
         \"BonusPack1\"=dword:00000001\r\n\
         \"BonusPack2\"=dword:00000001\r\n\r\n\
         [HKEY_LOCAL_MACHINE\\SOFTWARE\\GOG.com\\Games\\1207660573]\r\n\
         \"buildId\"=\"58117267108351024\"\r\n\
         \"ver\"=\"2.1.0.10 GOG 0.13\"\r\n\
         \"osbit\"=\"neutral\"\r\n\
         \"installer_language\"=\"russian\"\r\n\r\n"
    )
}

/// Adds to the run's Documents folder `documents` what the install
/// creates in the user folder, and the run's configuration file; the install
/// folder `game` already holds its own additions (`@gog_original`,
/// tools/gog-extract). Not created: shortcuts (not needed by the game,
/// gog-install-additions.md section 5), the installer's tools and caches
/// (section 6), the DirectX redistributable (Wine has Direct3D 9).
fn add_install(game: &Path, documents: &Path) -> Result {
    let write = |path: &Path, bytes: &[u8]| {
        mkdir_parent(path)?;
        fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    // Section 3: the support data copied unchanged into the user folder.
    let user = documents.join(USER_FOLDER);
    let support = game.join("__support/userdocs");
    let mut steer = Vec::new();
    for name in ["user.ltx", "traffic.net"] {
        let from = support.join(name);
        let bytes = fs::read(&from).map_err(|e| format!("{}: {e}", from.display()))?;
        write(&user.join(name), &bytes)?;
        if name == "user.ltx" {
            steer = bytes;
        }
    }
    // The run's configuration file (steering-the-original.md section 2.2).
    if !steer.ends_with(b"\n") {
        steer.extend_from_slice(b"\r\n");
    }
    for line in views::SETTINGS {
        steer.extend_from_slice(format!("{line}\n").as_bytes());
    }
    write(&user.join(STEER_LTX), &steer)
}

/// The Wine user's files (where Windows programs keep settings and logs),
/// copied out of the container before it stops.
fn save_user_files(container: &Container, to: &Path) {
    let Ok(tar) = container.exec(&["tar", "-C", "/root/wine/drive_c", "-cf", "-", "users"]) else {
        return;
    };
    if fs::create_dir_all(to).is_err() {
        return;
    }
    let untar = Command::new("tar")
        .arg("-xf")
        .arg("-")
        .arg("-C")
        .arg(to)
        .stdin(Stdio::piped())
        .spawn();
    if let Ok(mut untar) = untar {
        if let Some(mut stdin) = untar.stdin.take() {
            let _ = stdin.write_all(&tar);
        }
        let _ = untar.wait();
        println!("Wine user's files: {}", to.display());
    }
}

fn mkdir_parent(path: &Path) -> Result {
    let parent = path.parent().ok_or("no parent")?;
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))
}

/// The Russian installer's `.exe` and `-1.bin` in `VOLYA_GOG_INSTALLER`.
fn installer() -> Result<(PathBuf, PathBuf)> {
    let dir = std::env::var_os("VOLYA_GOG_INSTALLER")
        .ok_or("VOLYA_GOG_INSTALLER must name the folder holding the GOG installer")?;
    installer_parts(Path::new(&dir))
}

/// The installer's cache (as the game-data tests name it), without
/// assembling anything.
fn cache() -> Result<PathBuf> {
    let (exe, bin) = installer()?;
    cache_dir(&exe, &bin)
}

/// The Russian installer's `.exe` and `-1.bin` in `dir`.
fn installer_parts(dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let exe = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name.starts_with("setup_s.t.a.l.k.e.r._shadow_of_chernobyl")
                && name.contains("(russian)")
                && name.ends_with(".exe")
        })
        .ok_or_else(|| format!("no Russian installer .exe in {}", dir.display()))?;
    let stem = exe.file_stem().unwrap_or_default().to_string_lossy();
    let bin = exe.with_file_name(format!("{stem}-1.bin"));
    Ok((exe, bin))
}

/// The installer's cache, shared with the game-data tests (docs/testing.md).
fn cache_dir(exe: &Path, bin: &Path) -> Result<PathBuf> {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is unset")
    };
    let base = if cfg!(target_os = "macos") {
        home()?.join("Library/Caches")
    } else {
        match std::env::var_os("XDG_CACHE_HOME").filter(|d| !d.is_empty()) {
            Some(d) => PathBuf::from(d),
            None => home()?.join(".cache"),
        }
    };
    let size = |p: &Path| {
        fs::metadata(p)
            .map(|m| m.len())
            .map_err(|e| format!("{}: {e}", p.display()))
    };
    let stem = exe.file_stem().unwrap_or_default().to_string_lossy();
    Ok(base
        .join("volya-engine")
        .join(format!("{stem}-{}-{}", size(exe)?, size(bin)?)))
}

/// Decodes an XWD image in Z-pixmap format at 24 or 32 bits per pixel (as
/// Xvfb's `-fbdir` writes) into width, height and RGB bytes. The header's
/// byte order is told by its version field (7); the pixels' by its own
/// field.
fn xwd_rgb(xwd: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let be = xwd.get(4..8)? == 7u32.to_be_bytes();
    let field = |i: usize| -> Option<u32> {
        let b: [u8; 4] = xwd.get(i * 4..i * 4 + 4)?.try_into().ok()?;
        Some(if be {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let header = field(0)? as usize;
    let (format, width, height) = (field(2)?, field(4)?, field(5)?);
    let pixel_lsb_first = field(7)? == 0;
    let (bits, line) = (field(11)?, field(12)? as usize);
    let masks = [field(14)?, field(15)?, field(16)?];
    let colours = field(19)? as usize;
    if format != 2 || !(bits == 24 || bits == 32) || width == 0 || height == 0 {
        return None;
    }
    let bytes = bits as usize / 8;
    let start = header + colours * 12;
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let at = start + y * line + x * bytes;
            let p = xwd.get(at..at + bytes)?;
            let value = p.iter().enumerate().fold(0u32, |v, (i, &b)| {
                let shift = if pixel_lsb_first { i } else { bytes - 1 - i } * 8;
                v | u32::from(b) << shift
            });
            for mask in masks {
                rgb.push(((value & mask) >> mask.trailing_zeros()) as u8);
            }
        }
    }
    Some((width, height, rgb))
}

/// An 8-bit RGB PNG.
fn png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let row = width as usize * 3;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgb.chunks(row) {
        raw.push(0); // No filter.
        raw.extend_from_slice(line);
    }
    let mut packed = vec![0; zlib_rs::compress_bound(raw.len())];
    let (packed, _) = zlib_rs::compress_slice(&mut packed, &raw, zlib_rs::DeflateConfig::new(6));
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [(b"IHDR", &ihdr[..]), (b"IDAT", packed), (b"IEND", &[])] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let at = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = zlib_rs::crc32::crc32(0, &out[at..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_32_bit_xwd() {
        let fields: [u32; 25] = [
            100, 7, 2, 24, 2, 1, 0, 0, 32, 0, 32, 32, 8, 4, 0xff0000, 0xff00, 0xff, 8, 0, 0, 2, 1,
            0, 0, 0,
        ];
        let mut xwd: Vec<u8> = fields.iter().flat_map(|f| f.to_be_bytes()).collect();
        xwd.extend_from_slice(&[0x30, 0x20, 0x10, 0, 0xcc, 0xbb, 0xaa, 0]);
        assert_eq!(
            xwd_rgb(&xwd),
            Some((2, 1, vec![0x10, 0x20, 0x30, 0xaa, 0xbb, 0xcc]))
        );
        assert_eq!(xwd_rgb(&xwd[..104]), None);
    }

    #[test]
    fn writes_a_png() {
        let png = png(1, 1, &[1, 2, 3]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR"));
        // IEND's chunk is constant, CRC included.
        assert!(png.ends_with(b"\0\0\0\0IEND\xae\x42\x60\x82"));
        let idat = &png[33 + 8..png.len() - 12 - 4];
        let mut raw = [0u8; 4];
        let (raw, _) = zlib_rs::decompress_slice(&mut raw, idat, zlib_rs::InflateConfig::default());
        assert_eq!(raw, [0, 1, 2, 3]);
    }

    #[test]
    fn registry_holds_the_install_path() {
        let reg = registry(INSTALL_IN_WINDOWS);
        assert!(reg.contains("\"InstallPath\"=\"Z:\\\\work\\\\game\"\r\n"));
        assert!(reg.contains("\"InstallVers\"=\"2.1.0.10\"\r\n"));
    }
}
