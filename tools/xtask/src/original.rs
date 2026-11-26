// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `cargo xtask original`: the shipped game run as a black box
//! (docs/clean-room.md, Sources): its install folder assembled from the
//! user's GOG installer, its executable started unmodified under
//! `cargo xtask wine`, and the virtual display captured to PNG from outside.
//! Everything it writes is game assets and stays in the local cache.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use volya_engine_installer::{Installer, extract};

use crate::wine::{Container, Emulator};
use crate::{Result, step};

/// The edition the game-data tests use.
const LANGUAGE: &str = "ru-RU";

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
/// [`STEERING`], whose lines run after it and so override it.
const STEER_LTX: &str = "steer.ltx";

/// Settings for the run (steering-the-original.md section 3.1): full screen
/// at the display's size, so the whole display is the game's frame; effects
/// and music silent.
const STEERING: &str =
    "rs_fullscreen on\nvid_mode 1024x768\nsnd_volume_eff 0\nsnd_volume_music 0\n";

/// The Wine user (the container runs as root; measured: the prefix's only
/// user folder is `users/root`).
const WINE_USER: &str = "root";

/// The current user's Documents folder, `{userdocs}`, as Windows sees it
/// (gog-install-additions.md section 2: an absolute path without trailing
/// `\`).
fn userdocs() -> String {
    format!("C:\\users\\{WINE_USER}\\Documents")
}

/// The game's user folder below `{userdocs}`, as the support-data copies
/// name it (gog-install-additions.md section 3; the same folder as
/// `fsgame.ltx`'s `Stalker-SHOC`).
const USER_FOLDER: &str = "STALKER-SHOC";

/// In the container the run folder is `/work` and the install folder
/// `/work/game`; Wine's default `Z:` drive maps `/`.
const INSTALL_IN_CONTAINER: &str = "/work/game";
const INSTALL_IN_WINDOWS: &str = "Z:\\work\\game";

/// The virtual display, the size the game runs at ([`STEERING`]).
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

/// Ends the game with its own `quit` console command, typed with `xdotool`
/// into the virtual display: the shipped bindings open the console with the
/// grave key (steering-the-original.md section 2.2).
const QUIT: &str = "xdotool key grave && sleep 1 && xdotool type quit && xdotool key Return";

/// Seconds the game gets to end after [`QUIT`].
const QUIT_WAIT: u64 = 60;

const USAGE: &str = "Usage: cargo xtask original [--emulator qemu|box64] [--quit] [SECONDS]";

/// `cargo xtask original [--emulator qemu|box64] [--quit] [SECONDS]`.
pub fn main(args: &[&str]) -> Result {
    let (emulator, args) = Emulator::parse(args)?;
    let (quit, args) = match args {
        ["--quit", rest @ ..] => (true, rest),
        _ => (false, args),
    };
    let seconds = match args {
        [] => DEFAULT_SECONDS,
        [s] => s.parse().map_err(|e| format!("SECONDS: {e}\n{USAGE}"))?,
        _ => return Err(USAGE.into()),
    };
    let dir = std::env::var_os("VOLYA_GOG_INSTALLER")
        .ok_or("VOLYA_GOG_INSTALLER must name the folder holding the GOG installer")?;
    let (exe, bin) = installer_parts(Path::new(&dir))?;
    let cache = cache_dir(&exe, &bin)?;
    let install = assemble(&exe, &bin, &cache)?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let run = cache.join("original-runs").join(stamp.to_string());
    let game = run.join("game");
    let documents = run.join("documents");
    let frames = run.join("frames");
    fs::create_dir_all(&frames).map_err(|e| format!("{}: {e}", frames.display()))?;
    // A copy-on-write clone (APFS), so what the game writes stays in this run.
    crate::run(Command::new("cp").arg("-cR").arg(&install).arg(&game))?;
    add_install(&game, &documents)?;
    let reg = run.join("install.reg");
    fs::write(&reg, registry(INSTALL_IN_WINDOWS)).map_err(|e| format!("{}: {e}", reg.display()))?;

    let log_path = run.join("wine.log");
    let log = File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    // The prefix's Documents folder is the run's `documents/`, so what the
    // game writes there (logs, settings) lands on the Mac.
    let tail = format!(
        "mkdir -p /tmp/fb\n\
         docs=\"$WINEPREFIX/drive_c/users/{WINE_USER}/Documents\"\n\
         rm -rf \"$docs\" && ln -s /work/documents \"$docs\"\n\
         wine '{reg}' import 'Z:\\work\\install.reg'\n\
         wineserver -w\n\
         for view in 32 64; do\n\
         echo \"Install key, $view-bit view:\"\n\
         wine reg query '{INSTALL_KEY}' /reg:$view || true\n\
         done\n\
         cd {INSTALL_IN_CONTAINER}/{WORKING_DIR}\n\
         xvfb-run -n {DISPLAY} -f {XAUTHORITY} -s '-screen 0 {SCREEN} -fbdir /tmp/fb' wine \"$@\"\n\
         echo \"exit code $?\"",
        reg = emulator.reg()
    );
    let exe = format!("{INSTALL_IN_CONTAINER}/{EXE}");
    let mut program = vec![exe.as_str()];
    program.extend(ARGS);
    step(&format!(
        "Running {EXE} {} for {seconds} s, captures in {}",
        ARGS.join(" "),
        frames.display()
    ));
    let mut container = Container::spawn(emulator, &run, &tail, &program, log.into(), err.into())?;
    let start = Instant::now();
    let mut last: Option<Vec<u8>> = None;
    let mut ended = None;
    while start.elapsed() < Duration::from_secs(seconds) {
        thread::sleep(Duration::from_secs(EVERY));
        if let Some(status) = container.try_wait()? {
            ended = Some(status);
            break;
        }
        let at = start.elapsed().as_secs();
        if let Some(png) = capture(&container) {
            let changed = last.as_ref() != Some(&png);
            let path = frames.join(format!("{at:04}s.png"));
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
        if let Some(png) = capture(&container) {
            let path = frames.join("final.png");
            fs::write(&path, &png).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("final  {}", path.display());
        }
        save_user_files(&container, &run.join("prefix"));
        if quit {
            ended = quit_game(&mut container)?;
        }
    }
    let elapsed = start.elapsed().as_secs();
    drop(container);
    match ended {
        Some(status) => step(&format!(
            "The game ended after about {elapsed} s ({status})"
        )),
        None => step(&format!("Stopped the game after {elapsed} s")),
    }
    println!(
        "Captures: {}\nWine's log: {}",
        frames.display(),
        log_path.display()
    );
    Ok(())
}

/// The generated `fsgame.ltx` (gog-install-additions.md section 2): 17
/// LF-terminated ASCII lines, `{userdocs}` the only placeholder.
const FSGAME_LTX: &str = "\
$app_data_root$ = true | false| {userdocs}\\Stalker-SHOC
$game_data$ =     false| true | $fs_root$      | gamedata\\
$game_ai$ =       true | false| $game_data$    | ai\\
$game_spawn$ =    true | false| $game_data$    | spawns\\
$game_anims$ =    true | true | $game_data$    | anims\\
$game_levels$ =   true | false| $game_data$    | levels\\
$game_meshes$ =   true | true | $game_data$    | meshes\\
$game_dm$ =       true | true | $game_data$    | meshes\\
$game_shaders$ =  true | true | $game_data$    | shaders\\
$game_sounds$ =   true | true | $game_data$    | sounds\\
$game_textures$ = true | true | $game_data$    | textures\\
$game_scripts$ =  true | false| $game_data$    | scripts\\
$game_config$ =   true | false| $game_data$    | config\\
$level$ =         false| false| $game_levels$
$game_saves$ =    true | false| $app_data_root$| savedgames\\
$logs$ =          true | false| $app_data_root$| logs\\
$screenshots$ =   true | false| $app_data_root$| screenshots\\
";

/// The install script's key (gog-install-additions.md section 4.1); each
/// run's `wine.log` lists it in both registry views, so where the values
/// landed is on record (measured on box64: the 32-bit view only, as
/// `HKLM\SOFTWARE\Wow6432Node\…` to 64-bit programs).
const INSTALL_KEY: &str = "HKLM\\SOFTWARE\\GSC Game World\\STALKER-SHOC";

/// The registry entries of the install, as a file for `reg import`:
/// - the install script's five values (gog-install-additions.md section
///   4.1; `$1` believed the DWORD 1), imported into the 32-bit view the
///   game reads (section 4.1, [`Emulator::reg`]);
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

/// Adds to the run's install folder `game` and Documents folder `documents`
/// what the install creates beyond the game files, and the run's
/// configuration file. Not created: shortcuts (not needed by the game,
/// gog-install-additions.md section 5), the installer's tools and caches
/// (section 6), the DirectX redistributable (Wine has Direct3D 9).
fn add_install(game: &Path, documents: &Path) -> Result {
    let write = |path: &Path, bytes: &[u8]| {
        mkdir_parent(path)?;
        fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    // Section 2, then `attrib +r`: Wine shows a file without write
    // permission as read-only.
    let fsgame = game.join("fsgame.ltx");
    write(
        &fsgame,
        FSGAME_LTX.replace("{userdocs}", &userdocs()).as_bytes(),
    )?;
    fs::set_permissions(&fsgame, fs::Permissions::from_mode(0o444))
        .map_err(|e| format!("{}: {e}", fsgame.display()))?;
    // Section 6: empty folders.
    for dir in ["downloads", "logs", "savedgames"] {
        let dir = game.join("userdocs").join(dir);
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
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
    steer.extend_from_slice(STEERING.as_bytes());
    write(&user.join(STEER_LTX), &steer)
}

/// Types [`QUIT`]; the container's exit status once the game has ended, or
/// `None` if it is still running after [`QUIT_WAIT`] seconds.
fn quit_game(container: &mut Container) -> Result<Option<ExitStatus>> {
    step("Quitting the game from its console");
    let script = format!("export DISPLAY=:{DISPLAY} XAUTHORITY={XAUTHORITY}\n{QUIT}");
    container.exec(&["sh", "-c", &script])?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(QUIT_WAIT) {
        thread::sleep(Duration::from_secs(1));
        if let Some(status) = container.try_wait()? {
            return Ok(Some(status));
        }
    }
    println!("The game was still running {QUIT_WAIT} s after quit");
    Ok(None)
}

/// The display as PNG, or `None` while there is none.
fn capture(container: &Container) -> Option<Vec<u8>> {
    let xwd = container.exec(&["cat", FRAMEBUFFER]).ok()?;
    let (width, height, rgb) = xwd_rgb(&xwd)?;
    Some(png(width, height, &rgb))
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

/// The install folder, `<cache>/original/`: every game file of the edition
/// in the installer's layout. Archives are cloned (APFS copy-on-write) from
/// the game-data tests' shared `<cache>/archives/`, extracted there first if
/// missing; the rest is extracted directly. Files already there with the
/// installer's size are kept. Locked against concurrent runs and tests.
fn assemble(exe: &Path, bin: &Path, cache: &Path) -> Result<PathBuf> {
    let install = cache.join("original");
    let archives = cache.join("archives");
    let installer = Installer::parse(BufReader::new(
        File::open(exe).map_err(|e| format!("{}: {e}", exe.display()))?,
    ))
    .map_err(|e| e.to_string())?;
    let mut bin = BufReader::new(File::open(bin).map_err(|e| format!("{}: {e}", bin.display()))?);
    let _lock = lock(&install)?;
    for file in installer.game_files(LANGUAGE) {
        let to = install.join(&file.path);
        if size_is(&to, file.size) {
            continue;
        }
        if is_archive(&file.path) {
            let _lock = lock(&archives)?;
            let from = archives.join(&file.path);
            if !size_is(&from, file.size) {
                extract_to(file, &mut bin, &archives)?;
            }
            mkdir_parent(&to)?;
            let _ = fs::remove_file(&to);
            crate::run(Command::new("cp").arg("-c").arg(&from).arg(&to))?;
        } else {
            extract_to(file, &mut bin, &install)?;
        }
    }
    Ok(install)
}

/// Extracts `file` into `root`, renamed into place once complete.
fn extract_to(
    file: &volya_engine_installer::GameFile,
    bin: &mut BufReader<File>,
    root: &Path,
) -> Result {
    let to = root.join(&file.path);
    println!("extracting {} ({} bytes)", file.path, file.size);
    mkdir_parent(&to)?;
    let partial = root.join(format!("{}.partial", file.path));
    let mut out =
        BufWriter::new(File::create(&partial).map_err(|e| format!("{}: {e}", partial.display()))?);
    extract(file, bin, &mut out).map_err(|e| format!("{}: {e}", file.path))?;
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    fs::rename(&partial, &to).map_err(|e| format!("{}: {e}", to.display()))
}

fn size_is(path: &Path, size: u64) -> bool {
    fs::metadata(path).is_ok_and(|m| m.len() == size)
}

fn mkdir_parent(path: &Path) -> Result {
    let parent = path.parent().ok_or("no parent")?;
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))
}

/// An exclusive lock on `<dir>.lock`, the same the game-data tests take.
fn lock(dir: &Path) -> Result<File> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.with_extension("lock");
    let file = File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.lock()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(file)
}

/// Whether `path` is one of the archives the game-data tests cache (the
/// rule of `crates/volya-engine-formats/tests/support/gog_archives.rs`).
fn is_archive(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let root_db = !path.contains('/')
        && name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.starts_with("db"));
    root_db || (path.starts_with("mods/") && name.contains(".xdb"))
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
    fn fsgame_ltx_is_the_specified_file() {
        let text = FSGAME_LTX.replace("{userdocs}", &userdocs());
        assert!(text.is_ascii() && !text.contains('\r'));
        assert_eq!(FSGAME_LTX.len(), 965);
        assert_eq!(text.lines().count(), 17);
        assert!(text.starts_with(
            "$app_data_root$ = true | false| C:\\users\\root\\Documents\\Stalker-SHOC\n"
        ));
        assert!(text.ends_with("| screenshots\\\n"));
    }

    #[test]
    fn registry_holds_the_install_path() {
        let reg = registry(INSTALL_IN_WINDOWS);
        assert!(reg.contains("\"InstallPath\"=\"Z:\\\\work\\\\game\"\r\n"));
        assert!(reg.contains("\"InstallVers\"=\"2.1.0.10\"\r\n"));
    }

    #[test]
    fn archives_are_the_tests_archives() {
        assert!(is_archive("gamedata.db0"));
        assert!(is_archive("mods/mp_military_2.xdb0"));
        assert!(!is_archive("bin/XR_3DA.exe"));
    }
}
