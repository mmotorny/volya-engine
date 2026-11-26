// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The original's frame of each view of the fidelity suite
//! (`tests/fidelity/views.rs`), steered by
//! docs/behavior/steering-the-original.md: [`capture`], which Bazel's
//! capture actions run (`//tests/fidelity:frames`, build/original.bzl), so
//! a frame is captured again only when its inputs change. The `fidelity`
//! browser suite compares ours with them.
//!
//! Besides each view's frame, `<view>.png`, it keeps the configuration file
//! the run started from, `<view>.user.ltx` (`user_ltx`, which our check
//! starts from too), and the file as the game rewrote it when it quit,
//! `<view>.saved.ltx` (options-screens.md 12.3), which the suite compares
//! with ours before any pixel (#561).

use std::fs;
use std::path::Path;
use std::process::ExitStatus;
use std::thread;
use std::time::Duration;

use super::{Install, NEW_GAME, Run, png};
use crate::Result;

#[allow(dead_code)]
mod list {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fidelity/views.rs"
    ));
}
pub use list::{Camera, HEIGHT, Place, VIEWS, View, WIDTH, user_ltx};

/// Seconds after the container's start at which the title menu is captured:
/// measured, the menu shows by about 65 s (#519); it waits twice that.
const TITLE_AT: u64 = 130;

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
/// intro to end. Measured: the intro starts about 60 s after the container
/// and ends about 180 s later.
const LOAD_TIMEOUT: u64 = 900;
const INTRO_TIMEOUT: u64 = 400;

/// Seconds between looks at the display while waiting.
const POLL: u64 = 2;

/// Captures the original's frames of the views at `place` (`title`: the
/// title menu's; `level`: a New Game's) into `out/fidelity/<view>.png`, one
/// run of the game in the temporary directory, removed after.
pub fn capture(place: &str, out: &Path) -> Result {
    let views: Vec<&View> = VIEWS
        .iter()
        .filter(|v| {
            matches!(
                (place, v.place),
                ("title", Place::TitleMenu) | ("level", Place::NewGame { .. })
            )
        })
        .collect();
    if !["title", "level"].contains(&place) {
        return Err(format!("no place {place:?}: title or level"));
    }
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
    let result = if views.is_empty() {
        Ok(())
    } else if place == "title" {
        capture_title(&install, &dir, &views)
    } else {
        capture_level(&install, &dir, &views)
    };
    // The run's clone of the install, its Wine prefix and captures: scratch.
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

/// The title menu: the game started without a level shows its logo videos
/// and then the menu (#519, measured); captured at [`TITLE_AT`], then the
/// game quits (steering-the-original.md section 7).
fn capture_title(install: &Install, out: &Path, views: &[&View]) -> Result {
    let mut run = install.start(&[], &[], "to the title menu")?;
    wait_until(&mut run, Duration::from_secs(TITLE_AT))?;
    let frame = run.capture().ok_or("no display")?;
    for view in views {
        keep(&run, out, view, frame.clone())?;
    }
    end(run, out, views)
}

/// The New Game views, in one run:
///
/// - each track view's camera track written to the saves folder (section
///   4.2);
/// - a New Game started with `-start` (sections 1.3, 3.2, 3.3);
/// - the New Game intro waited out (section 3.4: it plays once the level is
///   loaded; whether it can be skipped is unestablished, so nothing is
///   pressed): after the loading screen (full frame; the splash before it
///   is a picture on black, measured) it shows letterboxed, and the first
///   frame after it that isn't letterboxed is the start of play;
/// - the start views ([`Camera::Start`]) captured at their `seconds` after
///   that, before any track: a track's precache blacks the display
///   (section 4.2.1);
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
fn capture_level(install: &Install, out: &Path, views: &[&View]) -> Result {
    let (mut starts, mut tracks) = (Vec::new(), Vec::new());
    let mut demos = Vec::new();
    for view in views {
        let Place::NewGame { camera, seconds } = view.place else {
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
    let mut run = install.start(&NEW_GAME, &demos, "into a New Game")?;
    poll(&mut run, LOAD_TIMEOUT, "the loading screen", |w, h, rgb| {
        !letterboxed(w, h, rgb)
    })?;
    poll(&mut run, LOAD_TIMEOUT, "the intro", |w, h, rgb| {
        letterboxed(w, h, rgb) && !black(w, h, rgb)
    })?;
    let play = poll(&mut run, INTRO_TIMEOUT, "play", |w, h, rgb| {
        !letterboxed(w, h, rgb)
    })?;
    let at = |s: u32| play + Duration::from_secs(u64::from(s));
    for (view, seconds) in starts {
        wait_until(&mut run, at(seconds))?;
        let frame = run.capture().ok_or("no display")?;
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
        let frame = run.capture().ok_or("no display")?;
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
        assert_eq!(lines[5], "hud_crosshair off", "inserted after `g_…`");
        assert!(lines.contains(&"r__supersample 4"));
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
