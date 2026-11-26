// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The views the `fidelity` suite compares (#521), shared by both engines:
// Bazel's capture actions (`//tests/fidelity:frames`) steers the original to each and caches its
// frame, `volya_engine::selftest::fidelity` draws ours. Both include this
// file with `include!`; a change to a view here (or to the settings) changes
// its cache key, so the original's frame is captured again.

/// The video mode both engines draw at: the original's `vid_mode`
/// (steering-the-original.md section 3.1), full screen on a display of the
/// same size, so the display is its frame.
pub const WIDTH: u32 = 1024;
pub const HEIGHT: u32 = 768;

/// The vertical field of view in degrees: the player's, which a camera
/// track does not change (steering-the-original.md section 4.2).
pub const FOV_DEGREES: f32 = 67.5;

/// Render settings: both engines read the shipped `user.ltx`; the
/// original's configuration file adds these console lines after it
/// (steering-the-original.md sections 2.2 and 3.1): full screen at
/// [`WIDTH`] × [`HEIGHT`], silent, and the HUD's crosshair and object info
/// off, as our views draw no HUD.
pub const SETTINGS: &[&str] = &[
    "rs_fullscreen on",
    "vid_mode 1024x768",
    "snd_volume_eff 0",
    "snd_volume_music 0",
    "hud_crosshair off",
    "hud_info off",
];

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
    /// a track's too (measured, #549).
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
            seconds: 90,
        },
    },
    View {
        name: "artefact",
        shows: "esc_af_medusa and its idle effect, from 1.5 m above (#514)",
        place: Place::NewGame {
            camera: Camera::Track {
                eye: [-244.8057, -11.6224, -34.1349],
                yaw: 0.0,
                pitch: 1.25,
            },
            seconds: 150,
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
            seconds: 210,
        },
    },
];
