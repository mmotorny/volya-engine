// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The original's frame of each view of the fidelity suite
//! (`tests/fidelity/views.rs`), steered by
//! docs/behavior/steering-the-original.md: [`capture`], which Bazel's
//! capture action runs (`//tests/fidelity:frames`, build/original.bzl), so
//! the frames are captured again only when their inputs change: one launch
//! of the game, every movie skipped, the title menu first, then a New
//! Game started from it. The `fidelity` browser suite compares ours with
//! them.
//!
//! A New Game view's frame is the game's own screenshot taken by the camera
//! recorder's F12, which leaves the whole HUD out of the frame it saves
//! (section 5.1; the displayed frames keep it): [`without_hud`]. The
//! display at that moment, HUD included, is kept beside it as
//! `display/<view>.png` (not served to the suite).
//!
//! Besides each view's frame, `<view>.png`, it keeps the configuration file
//! the run started from, `<view>.user.ltx` (`user_ltx`, which our check
//! starts from too), and the file as the game rewrote it when it quit,
//! `<view>.saved.ltx` (options-screens.md 12.3), which the suite compares
//! with ours before any pixel (#561).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::thread;
use std::time::Duration;

use super::{Install, Run, png};
use crate::Result;

#[allow(dead_code)]
mod list {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fidelity/views.rs"
    ));
}
pub use list::{Camera, HEIGHT, Place, VIEWS, View, WIDTH, user_ltx};

/// Seconds between presses of the quit key while the logo sequence may
/// play, and the looks in a row at the title menu that tell it has opened
/// (see [`skip_logos`]); then seconds the menu is given before its capture
/// (its bar's opening slide takes 556 ms, title-menu.md 4.3).
const SKIP_EVERY: u64 = 2;
const MENU_LOOKS: u32 = 3;
const MENU_SETTLE: u64 = 4;

/// Seconds the logo sequence may take, skipped, to the title menu.
const MENU_TIMEOUT: u64 = 300;

/// Seconds after the intro's quit key that it is pressed again, if the
/// intro still shows (the key was not read).
const INTRO_SKIP_RETRY: u64 = 30;

/// Keys of each camera track: one per second of track time, all equal, so
/// it holds one view (steering-the-original.md section 4.2, "A track whose
/// keys are all equal holds one fixed view").
const TRACK_KEYS: u32 = 15;

/// Cycles each track plays: it holds its view for cycles × keys seconds of
/// track time (section 4.2.2), which runs no faster than real time (section
/// 4.2), so at least [`PRECACHE`] + [`SETTLE`] seconds after its precache.
const TRACK_CYCLES: u32 = 4;

/// Seconds a track's precache is given (section 4.2.1: 50 black frames;
/// measured at the views' settings, 1920 × 1080 with supersampling 4: 80 to
/// 95 s): its `demo_play` runs this long and [`SETTLE`] before the view's
/// moment.
const PRECACHE: u32 = 110;

/// Seconds between the precache's end (the display no longer black,
/// section 4.2.2 step 3) and the capture, at the least: the view's
/// textures, glows and effects settle (glows fade in, glows.md).
const SETTLE: u32 = 10;

/// Seconds a precache may take before the view is given up: 50 frames at
/// 0.2 frames per second.
const PRECACHE_TIMEOUT: u64 = 250;

/// Seconds the level gets to load before its intro must show, and the
/// intro, skipped, to end. Measured: the loading takes about 55 s.
const LOAD_TIMEOUT: u64 = 900;
const INTRO_TIMEOUT: u64 = 120;

/// Seconds between looks at the display while waiting.
const POLL: u64 = 2;

/// Seconds the camera recorder gets to start before its F12 (section 5.1
/// step 3: two frames or more; a frame takes up to about a second at the
/// views' settings, measured), and the screenshot to be written after it.
const RECORDER_START: u64 = 10;
const SCREENSHOT_TIMEOUT: u64 = 120;

/// Captures the original's frames of the views into
/// `out/fidelity/<view>.png`, one run of the game in the temporary
/// directory, removed after (kept when the capture fails).
pub fn capture(out: &Path) -> Result {
    let views: Vec<&View> = VIEWS.iter().collect();
    let dir = out.join("fidelity");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let runs = std::env::temp_dir().join(format!("volya-capture-{}", std::process::id()));
    let install = Install::assemble_into(runs.clone())?;
    // The file every view of the run starts from, so each view's.
    let shipped = install.folder.join("__support/userdocs/user.ltx");
    let shipped = fs::read(&shipped).map_err(|e| format!("{}: {e}", shipped.display()))?;
    let config = user_ltx(&shipped);
    for view in &views {
        let path = dir.join(format!("{}.user.ltx", view.name));
        fs::write(&path, &config).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let result = capture_views(&install, &dir, &views);
    // The run's clone of the install, its Wine prefix and captures: scratch,
    // kept when the capture failed, for a look (game assets: local only).
    if let Err(e) = &result {
        return Err(format!("{e}\nThe run is kept in {}", runs.display()));
    }
    let _ = crate::run(
        std::process::Command::new("chmod")
            .arg("-R")
            .arg("u+w")
            .arg(&runs),
    );
    let _ = fs::remove_dir_all(&runs);
    result
}

/// Keeps `rgb` as `view`'s frame in `out` and in the run's captures.
fn keep(run: &Run, out: &Path, view: &View, (w, h, rgb): (u32, u32, Vec<u8>)) -> Result {
    if (w, h) != (WIDTH, HEIGHT) {
        return Err(format!("{}: the display is {w}x{h}", view.name));
    }
    let bytes = png(w, h, &rgb);
    if black(w, h, &rgb) {
        let copy = run.frames.join(format!("{}.png", view.name));
        fs::write(&copy, &bytes).map_err(|e| format!("{}: {e}", copy.display()))?;
        println!(
            "{:>5} s  {}: the display is black, not kept",
            run.elapsed().as_secs(),
            view.name
        );
        return Ok(());
    }
    let path = out.join(format!("{}.png", view.name));
    fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let copy = run.frames.join(format!("{}.png", view.name));
    fs::write(&copy, &bytes).map_err(|e| format!("{}: {e}", copy.display()))?;
    println!(
        "{:>5} s  {}: {}",
        run.elapsed().as_secs(),
        view.name,
        path.display()
    );
    Ok(())
}

/// Sleeps until `at` seconds after the container's start; fails if the
/// game ended first.
fn wait_until(run: &mut Run, at: Duration) -> Result {
    while run.elapsed() < at {
        thread::sleep((at - run.elapsed()).min(Duration::from_secs(1)));
        if let Some(status) = run.container.try_wait()? {
            return Err(format!("the game ended ({status})"));
        }
    }
    Ok(())
}

/// Looks at the display every [`POLL`] seconds until `done` holds for it;
/// the time it did. Every tenth look is kept in the run's captures.
fn poll(
    run: &mut Run,
    timeout: u64,
    what: &str,
    done: impl Fn(u32, u32, &[u8]) -> bool,
) -> Result<Duration> {
    let until = run.elapsed() + Duration::from_secs(timeout);
    let mut looks = 0u32;
    while run.elapsed() < until {
        thread::sleep(Duration::from_secs(POLL));
        if let Some(status) = run.container.try_wait()? {
            return Err(format!(
                "the game ended ({status}) while waiting for {what}"
            ));
        }
        let Some((w, h, rgb)) = run.capture() else {
            continue;
        };
        let at = run.elapsed();
        if looks.is_multiple_of(10) {
            let path = run.frames.join(format!("{:04}s.png", at.as_secs()));
            let _ = fs::write(path, png(w, h, &rgb));
        }
        looks += 1;
        if done(w, h, &rgb) {
            println!("{:>5} s  {what}", at.as_secs());
            return Ok(at);
        }
    }
    Err(format!("no {what} within {timeout} s"))
}

/// Skips the logo sequence of a start without a level and returns when
/// the title menu shows. The quit key (Esc, startup.md section 4) ends the
/// current item at once, whatever it is (6.5: every item of `intro_logo`
/// is stoppable), and on the title screen's menu it does nothing
/// (title-menu.md 4.4); before the sequence the splash takes no input
/// (steering-the-original.md 3.1). So it is pressed every [`SKIP_EVERY`]
/// seconds until the menu shows: the display's four corners not black for
/// [`MENU_LOOKS`] looks in a row, which no item outlasts under the presses
/// (the items draw on a black background, startup.md 6.1, measured: the
/// menu's corners are not black).
fn skip_logos(run: &mut Run) -> Result<Duration> {
    let until = run.elapsed() + Duration::from_secs(MENU_TIMEOUT);
    let mut lit = 0;
    while run.elapsed() < until {
        thread::sleep(Duration::from_secs(SKIP_EVERY));
        if let Some(status) = run.container.try_wait()? {
            return Err(format!("the game ended ({status}) before the title menu"));
        }
        // No display yet: nothing to press into.
        let Some((w, h, rgb)) = run.capture() else {
            continue;
        };
        lit = if corners_lit(w, h, &rgb) { lit + 1 } else { 0 };
        if lit >= MENU_LOOKS {
            let at = run.elapsed();
            println!("{:>5} s  the title menu", at.as_secs());
            return Ok(at);
        }
        // Before the game's window exists the key has nowhere to go and
        // `xdotool` fails; the next look presses again.
        let _ = run.xdotool("xdotool key Escape");
    }
    Err(format!("no title menu within {MENU_TIMEOUT} s"))
}

/// Whether each corner's 32 × 32 pixels have a mean channel value above 5.
fn corners_lit(w: u32, h: u32, rgb: &[u8]) -> bool {
    let (w, h) = (w as usize, h as usize);
    if w < 32 || h < 32 {
        return false;
    }
    [(0, 0), (w - 32, 0), (0, h - 32), (w - 32, h - 32)]
        .iter()
        .all(|&(x0, y0)| {
            let sum: u32 = (y0..y0 + 32)
                .flat_map(|y| rgb[(y * w + x0) * 3..(y * w + x0 + 32) * 3].iter())
                .map(|&c| u32::from(c))
                .sum();
            sum > 5 * 32 * 32 * 3
        })
}

/// The views, in one run:
///
/// - each track view's camera track written to the saves folder (section
///   4.2);
/// - the game started without a level, its logo sequence skipped
///   ([`skip_logos`]) and the title menu's views captured from the display
///   [`MENU_SETTLE`] seconds after the menu shows;
/// - a New Game started from the menu as its New Game item does: `start
///   server(all/single/alife/new) client(localhost)`, then `main_menu off`
///   (section 3.2), typed into the console;
/// - the New Game intro skipped: it plays once the level is loaded, shown
///   letterboxed (measured), and is skippable as movie sequences are
///   (loading-screen.md 7, 11; startup.md 6.5: the quit key ends its one
///   item), with game time paused while it plays (level-loading.md 2.4
///   step 3), so skipping it moves no view's moment. The key is pressed
///   once it shows, again after [`INTRO_SKIP_RETRY`] seconds if it still
///   does (only then: in a level the key opens the menu); the first frame
///   that isn't letterboxed is the start of play;
/// - the start views ([`Camera::Start`]) captured at their `seconds` after
///   that ([`without_hud`], as every New Game view), before any track: a
///   track's precache blacks the display (section 4.2.1);
/// - each track view's track played from the console (`demo_play
///   <name>,<cycles>`, sections 4.2 and 4.2.2) [`PRECACHE`] + [`SETTLE`]
///   seconds before its `seconds`; once the display has gone black and is
///   no longer black (the precache has ended, section 4.2.2 step 3), the
///   display captured at its `seconds`, and at least [`SETTLE`] seconds
///   after the precache,
///   while the track holds the view. A later `demo_play` replaces the
///   track (section 4.2), so each is captured before the next plays. A
///   black capture is not kept;
/// - `quit` (section 7) with no precache pending, so exit code 0.
fn capture_views(install: &Install, out: &Path, views: &[&View]) -> Result {
    let (mut titles, mut starts, mut tracks) = (Vec::new(), Vec::new(), Vec::new());
    let mut demos = Vec::new();
    for view in views {
        let Place::NewGame { camera, seconds } = view.place else {
            titles.push(*view);
            continue;
        };
        match camera {
            Camera::Start => starts.push((*view, seconds)),
            Camera::Track { eye, yaw, pitch } => {
                tracks.push((*view, seconds));
                demos.push((view.name, track(eye, yaw, pitch)));
            }
        }
    }
    starts.sort_by_key(|(_, s)| *s);
    tracks.sort_by_key(|(_, s)| *s);
    // Each track plays `lead` seconds before its view's moment.
    let lead = PRECACHE + SETTLE;
    for pair in tracks.windows(2) {
        let (a, b) = (pair[0].1, pair[1].1);
        if b < a + lead + 2 {
            return Err(format!(
                "views {} and {}: their tracks would overlap ({a} s and {b} s, \
                 each played {lead} s before)",
                pair[0].0.name, pair[1].0.name
            ));
        }
    }
    if let Some((view, s)) = tracks.first() {
        if *s < lead {
            return Err(format!("{}: at least {lead} s into the game", view.name));
        }
        if let Some((start, t)) = starts.last()
            && *t >= *s - lead
        {
            return Err(format!(
                "{}: a start view comes before every track ({t} s, {} played at {} s)",
                start.name,
                view.name,
                *s - lead
            ));
        }
    }
    let mut run = install.start(&[], &demos, "to the title menu, then into a New Game")?;
    let menu = skip_logos(&mut run)?;
    wait_until(&mut run, menu + Duration::from_secs(MENU_SETTLE))?;
    let frame = run.capture().ok_or("no display")?;
    for view in &titles {
        keep(&run, out, view, frame.clone())?;
    }
    run.console_lines(&[
        "start server(all/single/alife/new) client(localhost)",
        "main_menu off",
    ])?;
    poll(&mut run, LOAD_TIMEOUT, "the intro", |w, h, rgb| {
        letterboxed(w, h, rgb) && !black(w, h, rgb)
    })?;
    let play = loop {
        run.xdotool("xdotool key Escape")?;
        match poll(&mut run, INTRO_SKIP_RETRY, "play", |w, h, rgb| {
            !letterboxed(w, h, rgb)
        }) {
            Ok(at) => break at,
            Err(_) if run.elapsed() < Duration::from_secs(LOAD_TIMEOUT + INTRO_TIMEOUT) => {}
            Err(e) => return Err(e),
        }
    };
    let at = |s: u32| play + Duration::from_secs(u64::from(s));
    for (view, seconds) in starts {
        wait_until(&mut run, at(seconds))?;
        let frame = without_hud(&mut run, out, view)?;
        keep(&run, out, view, frame)?;
    }
    for (view, seconds) in tracks {
        let played = at(seconds - lead);
        wait_until(&mut run, played)?;
        run.console(&format!("demo_play {},{TRACK_CYCLES}", view.name))?;
        // The precache blacks the display, which may still show the level
        // for the first looks (measured).
        let what = format!("{}'s precache", view.name);
        poll(&mut run, PRECACHE_TIMEOUT, &what, black)?;
        let what = format!("{}'s precache over", view.name);
        let shown = poll(&mut run, PRECACHE_TIMEOUT, &what, |w, h, rgb| {
            !black(w, h, rgb)
        })?;
        let capture = at(seconds).max(shown + Duration::from_secs(SETTLE.into()));
        // The display while the view settles, kept in the run's captures.
        while run.elapsed() + Duration::from_secs(POLL) < capture {
            thread::sleep(Duration::from_secs(POLL));
            if let Some((w, h, rgb)) = run.capture() {
                let s = run.elapsed().saturating_sub(play).as_secs();
                let path = run.frames.join(format!("{}-{s:03}s.png", view.name));
                let _ = fs::write(path, png(w, h, &rgb));
            }
        }
        wait_until(&mut run, capture)?;
        let frame = without_hud(&mut run, out, view)?;
        println!(
            "{}: precache {} s, captured {} s after play",
            view.name,
            (shown - played).as_secs(),
            (capture - play).as_secs()
        );
        keep(&run, out, view, frame)?;
    }
    end(run, out, views)
}

/// The frame of the view the game shows now, without its HUD: the camera
/// recorder's screenshot (section 5.1). `demo_record` starts the recorder
/// from the current view (a held track's, which it replaces, or the
/// player's; roll 0, and without input it stays there); the F12 key (the
/// key itself, not the `screenshot` command, which keeps the HUD) has it
/// draw one frame with the HUD's flags cleared and save it (section 5:
/// full screen, the front buffer, here also as TGA, `-ss_tga`); Esc ends
/// the recorder and the player's camera returns. The display at the F12,
/// HUD included, is kept as `display/<view>.png` in `out`.
fn without_hud(run: &mut Run, out: &Path, view: &View) -> Result<(u32, u32, Vec<u8>)> {
    let before = screenshots(&run.screenshots);
    // A scratch name, not a track's: the recorder replaces its file.
    run.console("demo_record recorder")?;
    thread::sleep(Duration::from_secs(RECORDER_START));
    let display = run.capture().ok_or("no display")?;
    let path = run.frames.join(format!("{}-recorder.png", view.name));
    let _ = fs::write(path, png(display.0, display.1, &display.2));
    run.xdotool("xdotool key F12")?;
    let until = run.elapsed() + Duration::from_secs(SCREENSHOT_TIMEOUT);
    let mut looks = 0;
    let tga = loop {
        thread::sleep(Duration::from_secs(POLL));
        // The display while waiting, in the run's captures.
        if let Some((w, h, rgb)) = run.capture() {
            let path = run.frames.join(format!("{}-f12-{looks:02}.png", view.name));
            let _ = fs::write(path, png(w, h, &rgb));
            looks += 1;
        }
        if let Some(status) = run.container.try_wait()? {
            return Err(format!(
                "the game ended ({status}) before {}'s screenshot",
                view.name
            ));
        }
        // Decoded once written whole: the file holds all its pixels.
        let frame = screenshots(&run.screenshots)
            .into_iter()
            .filter(|p| !before.contains(p))
            .find_map(|p| fs::read(p).ok().and_then(|b| tga_rgb(&b)));
        if let Some(frame) = frame {
            break frame;
        }
        if run.elapsed() > until {
            let names: Vec<String> = fs::read_dir(&run.screenshots)
                .map(|d| {
                    d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into()))
                        .collect()
                })
                .unwrap_or_default();
            return Err(format!(
                "{}: no screenshot within {SCREENSHOT_TIMEOUT} s of the recorder's F12 \
                 (the screenshots folder holds {names:?})",
                view.name
            ));
        }
    };
    run.xdotool("xdotool key Escape")?;
    let dir = out.join("display");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.png", view.name));
    fs::write(&path, png(display.0, display.1, &display.2))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "{:>5} s  {}: the recorder's screenshot",
        run.elapsed().as_secs(),
        view.name
    );
    Ok(tga)
}

/// The TGA screenshots in `dir` (`ssq_…_(<level>).tga`, section 5).
fn screenshots(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name.starts_with("ssq_") && name.ends_with(".tga")
        })
        .collect()
}

/// Decodes a true-colour TGA, uncompressed (image type 2) or run-length
/// encoded (10), at 24 or 32 bits per pixel, into width, height and RGB
/// bytes, top row first; `None` for anything else or a file shorter than
/// its pixels.
fn tga_rgb(tga: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let header = tga.get(..18)?;
    let (id, colour_map, kind) = (header[0] as usize, header[1], header[2]);
    let width = u16::from_le_bytes([header[12], header[13]]) as usize;
    let height = u16::from_le_bytes([header[14], header[15]]) as usize;
    let (bits, descriptor) = (header[16], header[17]);
    // Bit 4 of the descriptor, rows right to left: refused.
    if colour_map != 0
        || !(kind == 2 || kind == 10)
        || !(bits == 24 || bits == 32)
        || descriptor & 0x10 != 0
        || width == 0
        || height == 0
    {
        return None;
    }
    let bytes = bits as usize / 8;
    let mut data = tga.get(18 + id..)?;
    let count = width * height;
    let mut rgb: Vec<u8> = Vec::with_capacity(count * 3);
    let mut push = |p: &[u8]| rgb.extend_from_slice(&[p[2], p[1], p[0]]);
    if kind == 2 {
        data.get(..count * bytes)?.chunks(bytes).for_each(&mut push);
    } else {
        let mut done = 0;
        while done < count {
            let (&packet, rest) = data.split_first()?;
            let n = (packet & 0x7f) as usize + 1;
            let run = if packet & 0x80 != 0 { bytes } else { n * bytes };
            let pixels = rest.get(..run)?;
            if packet & 0x80 != 0 {
                (0..n).for_each(|_| push(pixels));
            } else {
                pixels.chunks(bytes).for_each(&mut push);
            }
            data = &rest[run..];
            done += n;
        }
        if done != count {
            return None;
        }
    }
    // Bit 5 of the descriptor: the first row is the top one, else the
    // bottom one.
    if descriptor & 0x20 == 0 {
        let row = width * 3;
        rgb = rgb.chunks(row).rev().flatten().copied().collect();
    }
    Some((width as u32, height as u32, rgb))
}

/// Quits the game (section 7) and keeps its rewritten configuration file
/// as each view's `<view>.saved.ltx`; fails when it did not end normally
/// or did not rewrite the file.
fn end(mut run: Run, out: &Path, views: &[&View]) -> Result {
    let ended = run.quit()?;
    let saved = run.rewritten_config();
    run.finish(ended);
    exited(ended)?;
    let saved = saved?;
    for view in views {
        let path = out.join(format!("{}.saved.ltx", view.name));
        fs::write(&path, &saved).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// Whether the game ended with exit code 0 after `quit`; exit code 5 is the
/// crash at exit of quitting during a precache (steering-the-original.md
/// section 7), which the captures wait out.
fn exited(ended: Option<ExitStatus>) -> Result {
    match ended.map(|s| s.code()) {
        None | Some(Some(0)) => Ok(()),
        Some(Some(5)) => Err(
            "the game crashed at exit (exit code 5: quit during a precache, \
                              steering-the-original.md section 7)"
                .into(),
        ),
        Some(code) => Err(format!("the game ended with exit code {code:?}")),
    }
}

/// Rows of the letterbox's bars, top and bottom: the intro (measured)
/// shows about 95 black rows above and below its picture at 1024 × 768,
/// and 5 above and 7 below at 1920 × 1080, the views' size.
const BAR_ROWS: u32 = 4;

/// Whether the top and bottom [`BAR_ROWS`] rows of the frame are black.
fn letterboxed(w: u32, h: u32, rgb: &[u8]) -> bool {
    let row = w as usize * 3;
    let bars = BAR_ROWS as usize * row;
    let (top, bottom) = (
        &rgb[..bars],
        &rgb[(h as usize * row).saturating_sub(bars)..],
    );
    top.iter().chain(bottom).all(|&c| c <= 2)
}

/// Whether the whole frame is black.
fn black(_w: u32, _h: u32, rgb: &[u8]) -> bool {
    rgb.iter().all(|&c| c <= 2)
}

/// A camera track holding one view (section 4.2): [`TRACK_KEYS`]
/// 64-byte keys, each the view matrix of the camera at `eye`
/// with `yaw` and `pitch`, 16 little-endian floats row by row.
fn track(eye: [f32; 3], yaw: f32, pitch: f32) -> Vec<u8> {
    let key: Vec<u8> = view_matrix(eye, yaw, pitch)
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    key.repeat(TRACK_KEYS as usize)
}

/// The view matrix of section 4.2, `[r 0; u 0; d 0; e 1]` inverted, with
/// the direction `d` and up `u` of yaw and pitch (start-position.md
/// section 5: `R · (0, 0, 1)` and `R · (0, 1, 0)`, roll 0) and `r = u × d`.
fn view_matrix(e: [f32; 3], yaw: f32, pitch: f32) -> [[f32; 4]; 4] {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let d = [cp * sy, -sp, cp * cy];
    let u = [sp * sy, cp, sp * cy];
    let r = [
        u[1] * d[2] - u[2] * d[1],
        u[2] * d[0] - u[0] * d[2],
        u[0] * d[1] - u[1] * d[0],
    ];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    [
        [r[0], u[0], d[0], 0.0],
        [r[1], u[1], d[1], 0.0],
        [r[2], u[2], d[2], 0.0],
        [-dot(e, r), -dot(e, u), -dot(e, d), 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Section 4.2's worked example: the New Game start's direction and up.
    #[test]
    fn the_start_views_basis() {
        let m = view_matrix([-246.7263, -23.263261, -134.43387], 1.3962544, 0.0043165);
        let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
        assert!(near(column(2), [0.984797, -0.0043165, 0.173655]));
        assert!(near(column(1), [0.0042509, 0.999991, 0.00074959]));
        // The eye maps to the origin.
        let e = [-246.7263f32, -23.263261, -134.43387];
        for c in 0..3 {
            let v = e[0] * m[0][c] + e[1] * m[1][c] + e[2] * m[2][c] + m[3][c];
            assert!(v.abs() < 1e-3, "{v}");
        }
    }

    #[test]
    fn a_track_is_whole_keys() {
        let t = track([1.0, 2.0, 3.0], 0.0, 0.0);
        assert_eq!(t.len(), 64 * TRACK_KEYS as usize);
        assert_eq!(&t[..64], &t[64..128]);
        // Yaw 0, pitch 0: r = x, u = y, d = z; the last row is −e.
        let f = |i: usize| f32::from_le_bytes(t[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!([f(0), f(5), f(10), f(15)], [1.0, 1.0, 1.0, 1.0]);
        assert_eq!([f(12), f(13), f(14)], [-1.0, -2.0, -3.0]);
    }

    /// Uncompressed bottom-up and run-length-encoded top-down TGAs of the
    /// same 2 × 2 picture decode alike.
    #[test]
    fn decodes_tgas() {
        let header = |kind: u8, bits: u8, descriptor: u8| {
            let mut h = vec![0u8; 18];
            h[2] = kind;
            h[12] = 2;
            h[14] = 2;
            h[16] = bits;
            h[17] = descriptor;
            h
        };
        // Top row: red, green; bottom row: blue, white (BGR in the file).
        let rgb = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let mut plain = header(2, 24, 0);
        plain.extend_from_slice(&[255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 255, 0]);
        assert_eq!(tga_rgb(&plain), Some((2, 2, rgb.clone())));
        assert_eq!(tga_rgb(&plain[..plain.len() - 1]), None);
        let mut rle = header(10, 32, 0x20);
        rle.extend_from_slice(&[0x01, 0, 0, 255, 0, 0, 255, 0, 0]);
        rle.extend_from_slice(&[0x00, 255, 0, 0, 0, 0x80, 255, 255, 255, 0]);
        assert_eq!(tga_rgb(&rle), Some((2, 2, rgb)));
    }

    /// The title menu's test: all four corners lit, not just some.
    #[test]
    fn lit_corners() {
        let (w, h) = (64u32, 64u32);
        let mut rgb = vec![40u8; (w * h * 3) as usize];
        assert!(corners_lit(w, h, &rgb));
        for y in 32..64 {
            for x in 32..64 {
                rgb[((y * w + x) * 3) as usize..((y * w + x) * 3 + 3) as usize].fill(0);
            }
        }
        assert!(!corners_lit(w, h, &rgb));
    }

    #[test]
    fn the_letterbox() {
        let (w, h) = (4u32, 200u32);
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        assert!(letterboxed(w, h, &rgb) && black(w, h, &rgb));
        rgb[(100 * w * 3) as usize] = 200;
        assert!(letterboxed(w, h, &rgb) && !black(w, h, &rgb));
        rgb[0] = 30;
        assert!(!letterboxed(w, h, &rgb));
    }

    #[test]
    fn view_names_are_unique() {
        let names: std::collections::BTreeSet<&str> = VIEWS.iter().map(|v| v.name).collect();
        assert_eq!(names.len(), VIEWS.len());
    }

    /// The settings go into their lines, or new lines at their names'
    /// places, the bindings counting as `bind`'s.
    #[test]
    fn a_views_user_ltx() {
        let shipped = b"_preset Extreme\r\nai_use_torch_dynamic_lights off\r\nunbindall\r\n\
                        bind left kLEFT\r\ng_always_run on\r\nr__supersample 2\r\nvid_mode 1024x768\r\n";
        let text = String::from_utf8(user_ltx(shipped)).unwrap();
        let lines: Vec<&str> = text.split_terminator("\r\n").collect();
        assert_eq!(lines[0], "_preset Extreme");
        assert_eq!(lines[1], "ai_use_torch_dynamic_lights on");
        assert_eq!(&lines[2..4], ["unbindall", "bind left kLEFT"]);
        assert_eq!(lines[4], "g_always_run on");
        assert!(lines.contains(&"r__supersample 4"));
        assert!(lines.contains(&"snd_volume_eff 0."), "inserted");
        assert_eq!(*lines.last().unwrap(), "vid_mode 1920x1080");
        assert!(text.ends_with("\r\n"));
        let places: Vec<&str> = lines
            .iter()
            .map(|l| match l.split(' ').next().unwrap() {
                "unbindall" => "bind",
                n => n,
            })
            .collect();
        assert!(places.is_sorted(), "{places:?}");
    }
}
