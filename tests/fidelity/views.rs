// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The views the `fidelity` suite compares (#521):
// `volya_engine::selftest::fidelity` draws ours (`include!`), and the
// original's frames of them (`//tests/fidelity:frames`) are captured with
// the same views and settings; a change here needs new frames.
//
// Each view has its own user configuration file, `<view>.user.ltx` ([`user_ltx`]):
// the shipped `user.ltx` with [`SETTINGS`] and [`VIEW_SETTINGS`]
// written into it. The original runs with it (`-ltx`), our check starts
// from the same bytes, and each engine's file as it rewrites it on quit
// (options-screens.md 12.3) must match the other's byte for byte before
// any pixel is compared (#561).

/// The video mode both engines draw at: the original's `vid_mode`
/// (steering-the-original.md section 3.1), full screen on a display of the
/// same size, so the display is its frame. The largest the user chose to
/// compare at (#561); the virtual display offers it, and in full screen the
/// original keeps a `vid_mode` only when its resolution list has it
/// (display-mode.md A.5), which its rewritten file shows.
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;

/// The vertical field of view in degrees: the player's, which a camera
/// track does not change (steering-the-original.md section 4.2).
pub const FOV_DEGREES: f32 = 67.5;

/// The settings at their best-quality end (the user's decision, #561), each
/// written as its status (options-screens.md 7.1), so as the original
/// rewrites it. Every control of the options screen's video tab and
/// advanced page (options-screens.md, here OS, 6.1 and 6.2) and the first
/// renderer's own settings (r1-settings-scope.md, here RS, section 1):
///
/// - `renderer`: kept the shipped `renderer_r1`: a choice of renderer, not
///   a quality, and Volya Engine draws the first renderer's picture (RS).
/// - `_preset`: `Extreme`, the last and highest of the list (OS 6.1, 7.2:
///   "максимальные"; `rspec_maximum.ltx` has no name, OS 8). The shipped
///   file's first line; its script runs first and the lines below set the
///   rest over it (RS 2.10).
/// - `vid_mode`, `rs_fullscreen`: [`WIDTH`] × [`HEIGHT`], full screen.
/// - `rs_c_gamma`, `rs_c_contrast`, `rs_c_brightness`: no quality end;
///   the shipped 1, the identity ramp (display-gamma.md).
/// - `rs_vis_distance` 1.5: the range's top (OS 7.2); the far plane is the
///   environment's times the value (RS 2.2).
/// - `r__geometry_lod` 1.2: the top; raising it lowers every size threshold,
///   so smaller and farther parts are drawn, at higher detail
///   (object-detail-level.md 1).
/// - `r__detail_density` 0.2: the bottom, the inverted bar's right end
///   (OS 6.2 row 2); a slot gets 2 / value placement positions (RS 2.4),
///   the most at 0.2. The shipped value.
/// - `texture_lod` 0: the bottom, the inverted bar's right end (OS 6.2 row
///   3); no mip level is dropped (texture-quality-filtering.md 1.3). The
///   shipped value.
/// - `r__tf_aniso` 16: the top (OS 6.2 row 4). The shipped value.
/// - `r__supersample` 4: the top (OS 6.2 row 5); the target is
///   floor(N · 2 + 0.5) clamped to 2048 per axis, 2048 × 2048 at
///   1920 × 1080 (frame-presentation.md 1.1).
/// - `r2_sun`, `r2_sun_details` on; `r2_slight_fade` 2 (light distance)
///   and `r2_ls_squality` 1 (shadow quality), their tops (OS 6.2 rows 6 – 8
///   and 10): the second renderer's; the first reads none (RS 1).
/// - `ai_use_torch_dynamic_lights` on (OS 6.2 row 9): characters' torches
///   give their lights (RS 2.9). The shipped value.
/// - `rs_v_sync`, `rs_refresh_60hz`: no effect on a frame (display-mode.md
///   A.7, A.8); the shipped values.
/// - `r1_dlights` on: dynamic lights are listed (dynamic-lights.md 4 rule
///   4). The shipped value.
/// - `r1_dlights_clip` 150: the top (dynamic-lights.md 8); a light fades
///   over the longest distance, g = 1 − (|E − C| − R) / clip (5.2).
/// - `r1_glows_per_frame` 32: the top (glows.md 5); the most glows tested
///   for occlusion each frame.
///
/// Left at the shipped values, with no quality end the specs establish:
/// `r1_tf_mipbias` (in a level the engine's −0.5 is in force whatever it
/// holds, texture-quality-filtering.md 8.2), `r1_ssa_lod_a`, `r1_ssa_lod_b`
/// and `r1_lmodel_lerp` (no control offers them), `r1_pps_u`, `r1_pps_v`
/// (an offset of the presentation, frame-presentation.md 3.2) and
/// `r2_sun_lumscale`.
pub const SETTINGS: &[(&str, &str)] = &[
    ("_preset", "Extreme"),
    ("ai_use_torch_dynamic_lights", "on"),
    ("r1_dlights", "on"),
    ("r1_dlights_clip", "150."),
    ("r1_glows_per_frame", "32"),
    ("r2_ls_squality", "1."),
    ("r2_slight_fade", "2."),
    ("r2_sun", "on"),
    ("r2_sun_details", "on"),
    ("r__detail_density", "0.2"),
    ("r__geometry_lod", "1.2"),
    ("r__supersample", "4"),
    ("r__tf_aniso", "16"),
    ("renderer", "renderer_r1"),
    ("rs_fullscreen", "on"),
    ("rs_vis_distance", "1.5"),
    ("texture_lod", "0"),
    ("vid_mode", "1920x1080"),
];

/// The views' own lines (steering-the-original.md section 3.1): silent.
/// No HUD line: none hides the whole HUD (section 5.1), and the capture
/// leaves it out of the original's New Game frames by taking them with the
/// camera recorder's screenshot, which draws its frame with every HUD
/// switch off (section 5.1), as our views draw no HUD.
pub const VIEW_SETTINGS: &[(&str, &str)] = &[("snd_volume_eff", "0."), ("snd_volume_music", "0.")];

/// A view's user configuration file: the shipped `user.ltx` `shipped`
/// (options-screens.md 12.4: CR LF, one command per line in ascending byte
/// order of the names, the bindings at `bind`) with each of [`SETTINGS`]
/// and [`VIEW_SETTINGS`] written into its line, or, where the file has
/// none, into a new line at its name's place.
pub fn user_ltx(shipped: &[u8]) -> Vec<u8> {
    let mut lines: Vec<Vec<u8>> = shipped
        .split(|&b| b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l).to_vec())
        .filter(|l| !l.is_empty())
        .collect();
    let name_of = |line: &[u8]| line.split(|&b| b == b' ').next().unwrap_or(b"").to_vec();
    // A line's place: its command's name; the bindings sit at `bind`.
    let place = |line: &[u8]| match name_of(line) {
        n if n == b"unbindall" => b"bind".to_vec(),
        n => n,
    };
    for (name, value) in SETTINGS.iter().chain(VIEW_SETTINGS) {
        let line = format!("{name} {value}").into_bytes();
        match lines.iter().position(|l| name_of(l) == name.as_bytes()) {
            Some(at) => lines[at] = line,
            None => {
                let at = lines
                    .iter()
                    .position(|l| place(l).as_slice() > name.as_bytes())
                    .unwrap_or(lines.len());
                lines.insert(at, line);
            }
        }
    }
    lines
        .into_iter()
        .flat_map(|mut l| {
            l.extend_from_slice(b"\r\n");
            l
        })
        .collect()
}

/// Where a view is.
#[derive(Clone, Copy, Debug)]
pub enum Place {
    /// The title menu, once its startup sequence has ended.
    TitleMenu,
    /// A New Game's level (`l01_escape`, start-position.md) seen by
    /// `camera`, `seconds` after play started (the original: after its New
    /// Game intro; ours: of the level's updates). The time of day and the
    /// weather are the New Game's (05:30 on its first day, game time at ×10,
    /// steering-the-original.md section 4.4; the level's own weather cycle,
    /// section 4.5): neither can be set in the original.
    NewGame { camera: Camera, seconds: u32 },
}

/// Where a New Game's view is seen from.
#[derive(Clone, Copy, Debug)]
pub enum Camera {
    /// The player's own view where the New Game puts it (start-position.md):
    /// no camera track. The original draws its HUD over every New Game view,
    /// a track's too (measured, #549); its frames are the camera recorder's
    /// screenshots, which leave it out (steering-the-original.md 5.1).
    Start,
    /// A camera track's (steering-the-original.md section 4.2): from `eye`
    /// (metres, level space) at `yaw` and `pitch` (radians,
    /// start-position.md section 5: positive yaw turns toward +x, positive
    /// pitch looks down).
    Track { eye: [f32; 3], yaw: f32, pitch: f32 },
}

/// A view: its name (lower case, digits and `-`: a file name in both
/// engines), what it shows, and where.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub name: &'static str,
    pub shows: &'static str,
    pub place: Place,
}

/// The views, in the order they are captured.
pub const VIEWS: &[View] = &[
    View {
        name: "title-menu",
        shows: "the title menu",
        place: Place::TitleMenu,
    },
    View {
        name: "bunker-start",
        shows: "a New Game's start view in the trader's bunker (start-position.md)",
        place: Place::NewGame {
            camera: Camera::Start,
            seconds: 30,
        },
    },
    View {
        name: "caged-lamp",
        shows: "the caged lamp under the vault by the bunker's stairs, from below \
                (glows.md 7: level glow 5 at (-255.72, -18.48, -127.23); #523)",
        place: Place::NewGame {
            camera: Camera::Track {
                eye: [-254.42, -19.48, -127.23],
                yaw: -1.5707964,
                pitch: -0.6556956,
            },
            seconds: 160,
        },
    },
    View {
        // Not the artefact: by then the original's esc_af_medusa, a jumping
        // artefact, has left its record position for a place not
        // established, and it shows no artefact here (measured;
        // artefact-idle-effects.md 6.4, 8; #556). Ours still draws it at
        // its record (#572).
        name: "leaves",
        shows: "the ground below esc_af_medusa's record position, from 1.5 m above, with the \
                level's leaves (detail-objects.md 8.3, its worked example's view)",
        place: Place::NewGame {
            camera: Camera::Track {
                eye: [-244.8057, -11.6224, -34.1349],
                yaw: 0.0,
                pitch: 1.25,
            },
            seconds: 290,
        },
    },
    View {
        name: "outdoor",
        shows: "outdoors above the start: terrain, trees, fog and the sky",
        place: Place::NewGame {
            camera: Camera::Track {
                eye: [-180.0, 5.0, -120.0],
                yaw: 1.2,
                pitch: 0.15,
            },
            seconds: 420,
        },
    },
];
