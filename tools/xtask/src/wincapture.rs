// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The original's capture on Windows (`tools/wincapture/`, docs/testing.md,
//! The original on Windows): the frame clock's proxy `winmm.dll` and the
//! driver `volyacapture.exe`, built with llvm-mingw, and the driver's plan
//! from the fidelity suite's views (Bazel's actions, build/wincapture.bzl,
//! through `wincapture_action.rs`); the runner zip published as a GitHub
//! release (`xtask wincapture release`).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::{Result, run};

#[allow(dead_code)]
mod list {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fidelity/views.rs"
    ));
}
use list::{Camera, FRAME_MS, HEIGHT, Place, VIEWS, WIDTH};

/// The runner zip's Bazel target and the release tags' prefix.
pub const RUNNER: &str = "//tools/wincapture:runner";
const TAG_PREFIX: &str = "wincapture-";

// The views' `FRAME_MS`, what a frame advances the original's clock (as the
// Wine frame clock's runs, #575): a whole number of milliseconds, so the
// millisecond and 100 ns clocks advance alike.

/// Main-thread clock reads with no frame presented that add 1 ms, so a
/// wait for time to pass ends (measured under Wine: the original's startup
/// waits so, and ended after 1002 ms of them at 1000, #591).
const SPIN: u32 = 1000;

// The plan's `sleep_advances 1`: each main-thread `Sleep(n)` adds n ms to
// the clock, so a wait that sleeps between its reads ends too (a run on
// the PC stuck before its first frame, #762).

/// Before the first frame, the least a main-thread sleep advances the clock:
/// 0, a sleep advancing it by what it asks for alone. The startup's
/// `Sleep(0)` loop before the first frame waits for the game itself, not for
/// time, and its count follows real time (1,282 and 1,292 ms of the clock at
/// 10 ms each, measured on the PC, #762).
const START_QUANTUM_MS: u32 = 0;

// The plan's `prestart_sleeps 0`: before the first frame sleeps advance the
// clock not at all, as the startup's Sleep(5) waits on its other threads
// took 282 and 283 calls in two runs (measured on the PC, #762); only its
// busy waits advance it, by their reads.

/// The clock at the first frame, whatever the startup took: how long that
/// was depends on how often it waited, so from the first frame on every
/// run reads the same times (the driver warns when the startup took longer).
const FIRST_FRAME_MS: u32 = 120_000;

/// The system time at the clock's tick 0, seconds since 1970 (2026-01-01
/// 00:00 UTC), so every run reads the same dates.
const EPOCH: u64 = 1_767_225_600;

/// Keys of each camera track: one per second of track time, all equal, so
/// it holds one view (steering-the-original.md section 4.2).
const TRACK_KEYS: u32 = 15;

/// The compile flags: optimised, warnings, no link timestamp and stripped,
/// so the same sources give the same files; everything but Windows' own
/// libraries linked in.
const FLAGS: &[&str] = &[
    "-O2",
    "-Wall",
    "-Wextra",
    "-Werror",
    "-static",
    "-s",
    "-Wl,--no-insert-timestamp",
];

/// `build CLANG OUT KIND SRC...`: the C sources compiled by `clang`
/// (llvm-mingw's `i686-w64-mingw32-clang`) into `OUT`, the proxy `winmm.dll`
/// (KIND `dll`), the driver (KIND `exe`) or the counter's DynamoRIO client
/// (KIND `client`: with DynamoRIO's headers, the folder of its `dr_api.h`,
/// and its import library `dynamorio.lib` among the sources).
pub fn build(clang: &Path, out: &Path, kind: &str, sources: &[&str]) -> Result {
    let mut command = Command::new(clang);
    command
        .args(FLAGS)
        .arg("-o")
        .arg(out)
        .args(sources.iter().filter(|s| s.ends_with(".c")));
    match kind {
        // Exports undecorated, as winmm's.
        "dll" => command.args(["-shared", "-Wl,--kill-at", "-luser32"]),
        "exe" => command.args(["-lbcrypt", "-lwinmm", "-luser32"]),
        // No C runtime: DynamoRIO loads a client privately and its own API
        // replaces the runtime's (counter.c).
        "client" => {
            let api = sources
                .iter()
                .find(|s| s.ends_with("/dr_api.h"))
                .ok_or("client: no dr_api.h")?;
            let include = Path::new(api).parent().ok_or("dr_api.h: no folder")?;
            command
                .arg("-isystem")
                .arg(include)
                .args(["-shared", "-nostdlib"])
                .args(sources.iter().filter(|s| s.ends_with(".lib")))
                .arg("-lkernel32")
        }
        _ => return Err(format!("{kind}: not dll, exe or client")),
    };
    run(&mut command)
}

/// The driver's plan: its clock, the views' size, the configuration file's
/// settings (`user_ltx`'s, applied by the driver to the PC's own shipped
/// `user.ltx`, a game file) and the views with their frames and camera
/// tracks (the keys' floats as hex words, so the driver writes the bytes
/// `track` would).
pub fn plan() -> String {
    let mut p = String::new();
    let _ = writeln!(
        p,
        "volya-capture-plan 1\n\
         # From tests/fidelity/views.rs (xtask wincapture). Frames of {FRAME_MS} ms.\n\
         frame_ms {FRAME_MS}\nspin {SPIN}\nsleep_advances 1\n\
         start_quantum_ms {START_QUANTUM_MS}\nprestart_sleeps 0\nfirst_frame_ms {FIRST_FRAME_MS}\n\
         epoch {EPOCH}\nsize {WIDTH} {HEIGHT}"
    );
    for line in String::from_utf8_lossy(&capture_ltx()).lines() {
        let _ = writeln!(p, "ltx {line}");
    }
    for view in VIEWS {
        let frame = view.place.frame();
        match view.place {
            Place::TitleMenu => {
                let _ = writeln!(p, "view {} title-menu {frame}", view.name);
            }
            Place::NewGame { camera, .. } => match camera {
                Camera::Start => {
                    let _ = writeln!(p, "view {} start {frame}", view.name);
                }
                Camera::Track { eye, yaw, pitch } => {
                    let words: Vec<String> = view_matrix(eye, yaw, pitch)
                        .iter()
                        .flatten()
                        .map(|v| format!("{:08x}", v.to_bits()))
                        .collect();
                    let _ = writeln!(
                        p,
                        "view {} track {frame} {TRACK_KEYS} {}",
                        view.name,
                        words.join(" ")
                    );
                }
            },
        }
    }
    p
}

/// `plan OUT`: [`plan`] into `OUT`.
pub fn write_plan(out: &Path) -> Result {
    fs::write(out, plan()).map_err(|e| format!("{}: {e}", out.display()))
}

/// The view matrix of steering-the-original.md section 4.2, `[r 0; u 0;
/// d 0; e 1]` inverted, with the direction `d` and up `u` of yaw and pitch
/// (start-position.md section 5: `R · (0, 0, 1)` and `R · (0, 1, 0)`, roll
/// 0) and `r = u × d`.
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

/// `xtask wincapture release`: publishes the runner zip `zip` (built by
/// [`RUNNER`]) as a prerelease tagged by its hash.
pub fn release(zip: &Path) -> Result {
    let notes = format!(
        "The original's capture on Windows (tools/wincapture/, built by `{RUNNER}`): \
         the frame clock's winmm.dll, volyacapture.exe and its plan. Steps: \
         docs/testing.md, The original on Windows."
    );
    crate::d3d9probes::publish(zip, TAG_PREFIX, "wincapture.zip", &notes)
}

/// The views' configuration file the capture starts the game with: the
/// views' settings alone (`user_ltx` of no lines: one line each, in
/// ascending order of their names, CR LF), nothing of the PC's install or
/// user folder. The settings it lacks keep the game's built-in values; the
/// key bindings are `config\default_controls.ltx`'s, which `unbindall`
/// executes before the file (options-screens.md 11.5, 12.2), with
/// `console` on `kGRAVE` and `quit` on `kESCAPE` (console-scope.md,
/// startup.md section 4), the keys the capture presses.
pub fn capture_ltx() -> Vec<u8> {
    list::user_ltx(&[])
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
    }

    #[test]
    fn the_plan_lists_every_view_and_setting() {
        let p = plan();
        assert!(p.starts_with("volya-capture-plan 1\n"));
        assert!(p.contains("\nsize 1920 1080\n"));
        assert!(p.contains("\nltx vid_mode 1920x1080\n"));
        assert!(p.contains("\nltx snd_volume_eff 0.\n"));
        // Every setting, nothing else.
        let ltx = p.lines().filter(|l| l.starts_with("ltx ")).count();
        assert_eq!(ltx, list::SETTINGS.len() + list::VIEW_SETTINGS.len());
        assert!(p.contains("\nview title-menu title-menu 100\n"));
        assert!(p.contains("\nview bunker-start start 750\n"));
        for view in VIEWS {
            let line = format!("\nview {} ", view.name);
            assert!(p.contains(&line), "{line}");
        }
        // A track: 16 words after its frame and key count; the last is 1.0.
        let track = p.lines().find(|l| l.contains(" track ")).unwrap();
        let words: Vec<&str> = track.split(' ').collect();
        assert_eq!(words.len(), 5 + 16);
        assert_eq!(words[4], "15");
        assert_eq!(*words.last().unwrap(), "3f800000");
    }
}
