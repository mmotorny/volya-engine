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
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fidelity/walks.rs"
    ));
}
use list::{Camera, FRAME_MS, HEIGHT, PREROLL, Place, VIEWS, WALKS, WIDTH};

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
        "exe" => command.args(["-lbcrypt", "-lwinmm", "-luser32", "-lole32"]),
        // No C runtime: DynamoRIO loads a client privately and its own API
        // replaces the runtime's (counter.c). No system library but ntdll,
        // whose import library comes first: DynamoRIO's also provides one
        // of its section functions (capture-recipe.md, rule 2).
        // KIND `client-kernel32`: the bisect's variant, the shared memory
        // opened through kernel32 as before rule 2 (counter.c).
        "client" | "client-kernel32" => {
            let api = sources
                .iter()
                .find(|s| s.ends_with("/dr_api.h"))
                .ok_or("client: no dr_api.h")?;
            let include = Path::new(api).parent().ok_or("dr_api.h: no folder")?;
            command
                .arg("-isystem")
                .arg(include)
                .args(["-shared", "-nostdlib", "-lntdll"])
                .args(sources.iter().filter(|s| s.ends_with(".lib")));
            if kind == "client-kernel32" {
                command.args(["-DCOUNTER_KERNEL32", "-lkernel32"])
            } else {
                &mut command
            }
        }
        _ => return Err(format!("{kind}: not dll, exe or client")),
    };
    run(&mut command)?;
    if kind == "client" {
        let file = fs::read(out).map_err(|e| format!("{}: {e}", out.display()))?;
        check_client_imports(&pe_imports(&file)?).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(())
}

/// The functions the counter client must take from ntdll (counter.c).
const CLIENT_NTDLL: &[&str] = &[
    "NtMapViewOfSection",
    "NtOpenSection",
    "NtQueryInformationProcess",
];

/// The counter client's import table names only ntdll and DynamoRIO, and
/// ntdll's section functions come from ntdll (capture-recipe.md, rule 2).
fn check_client_imports(imports: &[(String, Vec<String>)]) -> Result {
    for (dll, _) in imports {
        let dll = dll.to_ascii_lowercase();
        if dll != "ntdll.dll" && dll != "dynamorio.dll" {
            return Err(format!(
                "the client imports {dll}: only ntdll and DynamoRIO allowed"
            ));
        }
    }
    for name in CLIENT_NTDLL {
        let from: Vec<&str> = imports
            .iter()
            .filter(|(_, names)| names.iter().any(|n| n == name))
            .map(|(dll, _)| dll.as_str())
            .collect();
        if !from.iter().all(|d| d.eq_ignore_ascii_case("ntdll.dll")) || from.is_empty() {
            return Err(format!(
                "{name} imported from {from:?}, not ntdll.dll alone"
            ));
        }
    }
    Ok(())
}

/// A 32-bit PE file's imports: each library's name with its functions'
/// (`#n` for one imported by ordinal), in the import table's order.
fn pe_imports(file: &[u8]) -> Result<Vec<(String, Vec<String>)>> {
    let u16_at = |at: usize| -> Result<u16> {
        file.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| format!("PE: offset {at:#x} past the end"))
    };
    let u32_at = |at: usize| -> Result<u32> {
        file.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| format!("PE: offset {at:#x} past the end"))
    };
    if u16_at(0)? != 0x5a4d {
        return Err("PE: no MZ header".into());
    }
    let nt = u32_at(0x3c)? as usize;
    if u32_at(nt)? != 0x4550 {
        return Err("PE: no PE signature".into());
    }
    let sections = u16_at(nt + 6)? as usize;
    let optional = nt + 24;
    let optional_size = u16_at(nt + 20)? as usize;
    if u16_at(optional)? != 0x10b {
        return Err("PE: not PE32".into());
    }
    // The import directory: data directory 1, after PE32's 96 bytes.
    let import_rva = u32_at(optional + 96 + 8)?;
    let table = optional + optional_size;
    let offset = |rva: u32| -> Result<usize> {
        for i in 0..sections {
            let s = table + i * 40;
            let (size, va, raw) = (
                u32_at(s + 8)?.max(u32_at(s + 16)?),
                u32_at(s + 12)?,
                u32_at(s + 20)?,
            );
            if rva >= va && rva < va + size {
                return Ok((rva - va + raw) as usize);
            }
        }
        Err(format!("PE: RVA {rva:#x} in no section"))
    };
    let string = |rva: u32| -> Result<String> {
        let at = offset(rva)?;
        let rest = file.get(at..).ok_or("PE: a name past the end")?;
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or("PE: an unterminated name")?;
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    };
    let mut imports = Vec::new();
    if import_rva == 0 {
        return Ok(imports);
    }
    let mut descriptor = offset(import_rva)?;
    loop {
        let (names_rva, name_rva, iat_rva) = (
            u32_at(descriptor)?,
            u32_at(descriptor + 12)?,
            u32_at(descriptor + 16)?,
        );
        if name_rva == 0 {
            break;
        }
        let mut functions = Vec::new();
        let mut thunk = offset(if names_rva != 0 { names_rva } else { iat_rva })?;
        loop {
            let entry = u32_at(thunk)?;
            if entry == 0 {
                break;
            }
            functions.push(if entry & 0x8000_0000 != 0 {
                format!("#{}", entry & 0xffff)
            } else {
                string(entry + 2)?
            });
            thunk += 4;
        }
        imports.push((string(name_rva)?, functions));
        descriptor += 20;
    }
    Ok(imports)
}

/// The driver's plan: its clock, the views' size, the configuration file's
/// settings (`user_ltx`'s, applied by the driver to the PC's own shipped
/// `user.ltx`, a game file) and the views with their frames and camera
/// tracks (the keys' floats as hex words, so the driver writes the bytes
/// `track` would); then the walks' configuration file (`audio_ltx` of no
/// lines) and the walks (tests/fidelity/walks.rs): `walk NAME START TAIL
/// PREROLL SEGMENT...`, each segment `KEYS:FRAMES`, the keys' scan codes in
/// hex joined by `+` (`e0__` an extended key).
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
    for line in String::from_utf8_lossy(&list::audio_ltx(&[])).lines() {
        let _ = writeln!(p, "audio_ltx {line}");
    }
    for walk in WALKS {
        let segments: Vec<String> = walk
            .segments
            .iter()
            .map(|s| {
                let keys: Vec<String> = s
                    .keys
                    .iter()
                    .map(|k| format!("{:x}", k.scan_code()))
                    .collect();
                format!("{}:{}", keys.join("+"), s.frames)
            })
            .collect();
        let _ = writeln!(
            p,
            "walk {} {} {} {PREROLL} {}",
            walk.name,
            walk.start,
            walk.tail,
            segments.join(" ")
        );
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

    /// A minimal PE32 file: one section at RVA 0x1000 (file offset 0x200)
    /// holding the import table of `imports`.
    fn pe(imports: &[(&str, &[&str])]) -> Vec<u8> {
        let mut file = vec![0u8; 0x200];
        file[0..2].copy_from_slice(b"MZ");
        file[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        file[0x40..0x44].copy_from_slice(b"PE\0\0");
        let (nt, optional) = (0x40, 0x58);
        file[nt + 6..nt + 8].copy_from_slice(&1u16.to_le_bytes());
        file[nt + 20..nt + 22].copy_from_slice(&224u16.to_le_bytes());
        file[optional..optional + 2].copy_from_slice(&0x10bu16.to_le_bytes());
        let section = optional + 224;
        // The section's contents: descriptors, then thunks and names.
        let mut body = vec![0u8; (imports.len() + 1) * 20];
        let rva = |body: &Vec<u8>| 0x1000 + body.len() as u32;
        for (i, (dll, names)) in imports.iter().enumerate() {
            let mut name_rvas = Vec::new();
            for name in *names {
                name_rvas.push(rva(&body));
                body.extend_from_slice(&[0, 0]);
                body.extend_from_slice(name.as_bytes());
                body.push(0);
            }
            let thunks = rva(&body);
            for r in name_rvas {
                body.extend_from_slice(&r.to_le_bytes());
            }
            body.extend_from_slice(&0u32.to_le_bytes());
            let dll_rva = rva(&body);
            body.extend_from_slice(dll.as_bytes());
            body.push(0);
            let d = i * 20;
            body[d..d + 4].copy_from_slice(&thunks.to_le_bytes());
            body[d + 12..d + 16].copy_from_slice(&dll_rva.to_le_bytes());
            body[d + 16..d + 20].copy_from_slice(&thunks.to_le_bytes());
        }
        file[optional + 104..optional + 108].copy_from_slice(&0x1000u32.to_le_bytes());
        let size = body.len() as u32;
        file[section + 8..section + 12].copy_from_slice(&size.to_le_bytes());
        file[section + 12..section + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        file[section + 16..section + 20].copy_from_slice(&size.to_le_bytes());
        file[section + 20..section + 24].copy_from_slice(&0x200u32.to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    const SECTION_FUNCTIONS: &[&str] = &[
        "NtQueryInformationProcess",
        "NtOpenSection",
        "NtMapViewOfSection",
    ];

    #[test]
    fn the_import_table_is_read() {
        let file = pe(&[
            ("ntdll.dll", SECTION_FUNCTIONS),
            ("dynamorio.dll", &["dr_fprintf"]),
        ]);
        let imports = pe_imports(&file).unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].0, "ntdll.dll");
        assert_eq!(imports[0].1, SECTION_FUNCTIONS);
        assert_eq!(
            imports[1],
            ("dynamorio.dll".into(), vec!["dr_fprintf".into()])
        );
    }

    #[test]
    fn the_client_imports_only_ntdll_and_dynamorio() {
        let ok = pe(&[
            ("ntdll.dll", SECTION_FUNCTIONS),
            ("dynamorio.dll", &["dr_fprintf"]),
        ]);
        assert!(check_client_imports(&pe_imports(&ok).unwrap()).is_ok());
        // kernel32 at all.
        let kernel32 = pe(&[
            ("ntdll.dll", SECTION_FUNCTIONS),
            ("dynamorio.dll", &["dr_fprintf"]),
            ("KERNEL32.dll", &["MapViewOfFile"]),
        ]);
        assert!(check_client_imports(&pe_imports(&kernel32).unwrap()).is_err());
        // A section function from DynamoRIO's import library (the link
        // order reversed).
        let order = pe(&[
            ("ntdll.dll", &["NtQueryInformationProcess", "NtOpenSection"]),
            ("dynamorio.dll", &["NtMapViewOfSection", "dr_fprintf"]),
        ]);
        assert!(check_client_imports(&pe_imports(&order).unwrap()).is_err());
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
        assert!(p.contains("\naudio_ltx snd_volume_eff 1.\n"));
        assert!(p.contains("\nwalk bunker-exit 900 50 25 e04b:23 11:66 e04d:12 "));
        // A track: 16 words after its frame and key count; the last is 1.0.
        let track = p.lines().find(|l| l.contains(" track ")).unwrap();
        let words: Vec<&str> = track.split(' ').collect();
        assert_eq!(words.len(), 5 + 16);
        assert_eq!(words[4], "15");
        assert_eq!(*words.last().unwrap(), "3f800000");
    }
}
