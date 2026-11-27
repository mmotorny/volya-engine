// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game from launch to the title menu (docs/behavior/startup.md): the
//! logo sequence, then the title menu, a New Game's level and the menu
//! during it (docs/behavior/save-load-screens.md section 2), driven by the
//! platform's clock and input, drawing [`Scene`]s and playing sounds through
//! [`AudioOut`].
//!
//! The platform shows the splash ([`splash`]) while it loads the data
//! ([`GameData::load`]), then creates an [`App`]; the sequence starts on the
//! first [`App::update`]. When the player quits, [`App::quit_requested`]
//! turns true and the platform returns to its own interface.

use std::io;
use std::sync::Arc;

use serde::Serialize;

use crate::audio::AudioOut;
use crate::console_window::ConsoleWindow;
use crate::core::audio::Category;
use crate::core::console::{Console, FatalReport};
use crate::core::diagnostics;
use crate::core::display::DisplayColour;
use crate::core::file_cache;
use crate::core::input::{Action, Control, Input, Key};
use crate::core::lights::LightSettings;
use crate::core::pause::{GlobalClock, Pause};
use crate::core::sound::Fatal;
use crate::core::sound::system::Listener;
use crate::core::texture_quality::{SamplerStates, TextureQuality};
use crate::core::vfs::{self, Vfs};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::inventory_screen::{self, InventoryScreen, InventoryState};
use crate::level::{LevelScreen, LevelState, LevelWorld};
use crate::level_sound::{LevelSounds, SoundRun};
use crate::loading_screen::{
    GAME_TYPE, Loading, LoadingScreen, LoadingScreenState, OVERLAY_FRAMES, SERVER_NEW_GAME,
    intro_due,
};
use crate::menu_screens::{Frame, MenuFrame, MenuScreens, MenuState};
use crate::pda_window::{self, PdaState, PdaWindow};
use crate::render::present::Presentation;
use crate::render::world::Filtering;
use crate::save_load::FieldFont;
use crate::saved_games::SavedGames;
use crate::scene::{Scene, Sprite, TextureId};
use crate::startup::{MoviePlayer, MovieSequenceState, Startup};
use crate::statistics_overlay::{FrameTimes, StatisticsOverlay};
use crate::title_menu::{Opening, TitleMenu};
use volya_engine_formats::pe::{self, Rgba};
use volya_engine_formats::ui::Resolution;
use volya_engine_formats::ui::picture::Quad;

mod console_actions;

/// The executable holding the splash picture, relative to the game root,
/// and the picture's bitmap resource (startup.md section 3).
pub const SPLASH_EXE: &str = "bin/XR_3DA.exe";
pub const SPLASH_BITMAP: u32 = 116;

/// The splash picture from the game root, `None` when the executable or
/// the resource is absent (nothing is shown then).
pub fn splash(root: &dyn Vfs) -> io::Result<Option<Rgba>> {
    let Ok(exe) = vfs::read(root, SPLASH_EXE) else {
        return Ok(None);
    };
    pe::resource(&exe, pe::RT_BITMAP, SPLASH_BITMAP)?
        .map(pe::decode_dib)
        .transpose()
}

/// The splash window on a black screen: the picture 1:1 in output pixels,
/// centred (section 3).
pub fn splash_scene(resolution: Resolution, size: (u32, u32)) -> Scene {
    let mut scene = Scene::new(resolution);
    let left = (resolution.width.saturating_sub(size.0) / 2) as f32 - 0.5;
    let top = (resolution.height.saturating_sub(size.1) / 2) as f32 - 0.5;
    scene.sprites.push(Sprite::picture(
        "splash",
        TextureId::Splash,
        &Quad {
            min: (left, top),
            max: (left + size.0 as f32, top + size.1 as f32),
            uv_min: (0.0, 0.0),
            uv_max: (1.0, 1.0),
            color: volya_engine_formats::ui::Color::WHITE,
        },
    ));
    scene
}

enum Phase {
    Startup(Startup),
    Menu(TitleMenu),
    /// A New Game loading (docs/behavior/loading-screen.md section 2).
    Loading {
        difficulty: &'static str,
        stages: NewGame,
    },
    /// The level; for its first frames with the loading image over it,
    /// then with the intro over it.
    Level {
        level: Box<LevelScreen>,
        overlay: Option<Overlay>,
        intro: Intro,
        /// The menu opened during the game: the level stays loaded, paused
        /// and not drawn (save-load-screens.md section 2).
        menu: Option<Box<TitleMenu>>,
        /// The level's inventory screen (docs/behavior/inventory-screen.md;
        /// `None` only in unit tests of other screens).
        inventory: Option<Box<InventoryScreen>>,
        /// The level's PDA window (docs/behavior/pda-window.md; `None`
        /// only in unit tests of other screens).
        pda: Option<Box<PdaWindow>>,
        /// Items the player dropped, released from the inventory: kept
        /// until the world can place them (inventory-screen.md 5.7).
        /// TODO(#111): placed into the level near the character.
        dropped: Vec<crate::core::character::inventory::InventoryItem>,
    },
}

enum NewGame {
    /// The frame the difficulty was chosen: the last menu frame stays on
    /// screen until the first caption begins (section 2, step 2).
    Closing(Box<TitleMenu>),
    Stages(Box<Loading>),
}

/// The loading image over the running level, sound muted (section 2,
/// step 4).
struct Overlay {
    screen: LoadingScreen,
    /// Overlay frames still to draw, this one included.
    frames_left: u32,
}

impl Overlay {
    /// The first overlay frame: sound muted (section 6).
    fn new(screen: LoadingScreen, audio: &mut dyn AudioOut) -> Self {
        audio.set_master_gain(0.0);
        Self {
            screen,
            frames_left: OVERLAY_FRAMES,
        }
    }

    /// The next frame's overlay; after the last, none: the image is gone
    /// and sound un-muted (section 2, step 6).
    fn next_frame(mut self, audio: &mut dyn AudioOut) -> Option<Self> {
        self.frames_left -= 1;
        if self.frames_left == 0 {
            self.end(audio);
            return None;
        }
        Some(self)
    }

    /// Ends the overlay early (leaving the level), un-muting sound.
    fn end(self, audio: &mut dyn AudioOut) {
        audio.set_master_gain(1.0);
    }
}

/// The New Game intro `intro_game` (loading-screen.md section 7): checked
/// once per started game during the overlay frames, then played over the
/// level.
#[derive(Default)]
struct Intro {
    checked: bool,
    player: Option<MoviePlayer>,
    /// Where its image items put the cursor, until the app takes it.
    cursor: Option<[f32; 2]>,
}

impl Intro {
    /// One frame at `now`: the intro starts when due ([`intro_due`]; the
    /// level is loaded whenever this runs), then plays, each of
    /// `quit_presses` skipping a stoppable item (startup.md section 6.5).
    /// Returns whether game time is paused: the current item's
    /// `pause_state` is on. Data without `intro_game` fails when the intro
    /// is due: the original starts the sequence then (loading-screen.md 7),
    /// and a missing element is fatal.
    fn frame(
        &mut self,
        data: &GameData,
        now: u64,
        overlay_frames_left: Option<u32>,
        quit_presses: u32,
        audio: &mut dyn AudioOut,
    ) -> io::Result<bool> {
        if !self.checked
            && let Some(left) = overlay_frames_left
            && intro_due(true, left, SERVER_NEW_GAME)
        {
            self.checked = true;
            if data.intro_game.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "game_tutorials.xml: no intro_game",
                ));
            }
            // Unmeasured (data read without its media) there is none to
            // play.
            if data.intro_game().is_some() {
                self.player = Some(MoviePlayer::new(now));
            }
        }
        let (Some(player), Some(seq)) = (self.player.as_mut(), data.intro_game()) else {
            return Ok(false);
        };
        player.update(seq, data, now, quit_presses, audio);
        self.cursor = player.take_cursor().or(self.cursor);
        if player.finished(seq) {
            // Section 2.3, step 4: then the level is visible.
            self.player = None;
            return Ok(false);
        }
        Ok(player.pauses(seq))
    }

    fn stop(&mut self, audio: &mut dyn AudioOut) {
        if let Some(p) = self.player.as_mut() {
            p.stop(audio);
        }
    }

    fn draw(&self, ctx: &mut Ctx, scene: &mut Scene) -> io::Result<()> {
        match (&self.player, ctx.data.intro_game()) {
            (Some(p), Some(seq)) => p.draw(seq, ctx, scene),
            _ => Ok(()),
        }
    }

    fn state(&self, data: &GameData, now: u64) -> Option<MovieSequenceState> {
        Some(self.player.as_ref()?.state(data.intro_game()?, now))
    }

    /// See [`MoviePlayer::current`].
    fn current<'a>(&self, data: &'a GameData) -> Option<(usize, Option<&'a str>)> {
        self.player.as_ref()?.current(data.intro_game()?)
    }
}

/// The interface sequences that play (sound-weather-ambient.md section 3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
enum SequenceId {
    /// `intro_logo`.
    Logo,
    /// `intro_game`.
    Intro,
}

/// A level frame: the level, then the intro, then the loading image
/// (the intro's first frames are under the image, loading-screen.md
/// section 7). The world is drawn under the opaque loading image (which
/// brings it to the GPU and warms it up, section 2 step 4) but not under
/// the intro, whose black background hides it (our choice, to save GPU
/// time).
fn compose(
    mut scene: Scene,
    intro: &Intro,
    overlay: Option<&Overlay>,
    ctx: &mut Ctx,
) -> io::Result<Scene> {
    if let Some(world) = &mut scene.world {
        world.covered = intro.player.is_some() && overlay.is_none();
    }
    intro.draw(ctx, &mut scene)?;
    if let Some(o) = overlay {
        o.screen.draw(ctx, &mut scene)?;
    }
    Ok(scene)
}

/// Presses that skip a movie item this frame (startup.md section 6.5):
/// each press of a key bound to quit (a gamepad's east button arrives as
/// Escape); a touch counts as one (our design for touch).
fn quit_presses(data: &GameData, input: &Input) -> u32 {
    input
        .actions()
        .iter()
        .filter(|a| {
            matches!(a, Action::Press { control: Control::Key(k), .. }
                if data.quit_keys.contains(k))
        })
        .count() as u32
        + input.touch_starts()
}

/// Whether the frame has a press of a keyboard key bound to `quit`
/// (save-load-screens.md section 2).
fn quit_key_pressed(data: &GameData, input: &Input) -> bool {
    input
        .device_changes()
        .iter()
        .any(|&(c, pressed)| pressed && matches!(c, Control::Key(k) if data.quit_keys.contains(&k)))
}

/// The actions handled before a key's console line, which they never
/// execute (console-key-bindings.md section 5, rules 2 and 4).
const BEFORE_CONSOLE_LINES: [&str; 6] = [
    "pause",
    "screenshot",
    "quit",
    "console",
    "quick_save",
    "quick_load",
];

/// The actions the single-player interface takes a press of, with no
/// in-game window open, while a living player character exists
/// (console-key-bindings.md section 5, rule 3): `inventory` opens the
/// inventory screen, the others the PDA window (pda-window.md 4.1).
const INTERFACE_TAKES: [&str; 4] = ["inventory", "active_jobs", "map", "contacts"];

/// The keyboard's and mouse's presses reaching the level this frame
/// (console-key-bindings.md section 5), the caller having checked rule 3
/// but for the interface's actions, which take a press while `alive` (a
/// living player character exists): each press of a key with a
/// console-line binding, unless the key's action is handled before it or
/// taken, executes its line and is used up, so the player character does
/// not get it. Mouse buttons press the keys `mouse1` … `mouse8`. Returns
/// whether a line was executed.
fn run_bound_lines(console: &mut Console, input: &mut Input, alive: bool) -> bool {
    use crate::core::console::{ACTIONS, KEYS, key_index};
    let presses: Vec<Control> = input
        .device_changes()
        .iter()
        .filter_map(|&(c, pressed)| pressed.then_some(c))
        .collect();
    let mut ran = false;
    for control in presses {
        let code = match control {
            Control::Key(k) => k.code(),
            Control::Mouse(b) => KEYS[key_index(b.key_name()).expect("mouse key")].1,
            Control::Pad(_) | Control::Touch(_) => continue,
        };
        let action = console.bound_action(code).map(|a| ACTIONS[a].0);
        if action.is_some_and(|a| {
            BEFORE_CONSOLE_LINES.contains(&a) || (alive && INTERFACE_TAKES.contains(&a))
        }) {
            continue;
        }
        let Some(line) = console.console_line(code).map(str::to_owned) else {
            continue;
        };
        // As a line the programs execute: not echoed (CC 2).
        console.exec(&line);
        input.use_up_press(control);
        ran = true;
    }
    ran
}

/// One frame of `menu` and its screens.
#[allow(clippy::too_many_arguments)]
fn menu_frame(
    screens: &mut MenuScreens,
    menu: &mut TitleMenu,
    tree: &dyn Vfs,
    data: &GameData,
    assets: &mut Assets,
    console: &mut Console,
    (native, video): (Resolution, Resolution),
    now: u64,
    input: &Input,
    audio: &mut dyn AudioOut,
) -> MenuFrame {
    // The name field measures its text in its font at the size drawn at;
    // a font that cannot be created is fatal (text-and-fonts.md 2.5, 2.6).
    let field = match screens.field_font() {
        Some(font) => match assets.fonts(tree, data, video.height) {
            Ok(fonts) => Some(FieldFont {
                font: fonts.get(font).clone(),
                scale_x: video.scale().0,
            }),
            Err(e) => {
                return MenuFrame {
                    fatal: Some(e.to_string()),
                    ..Default::default()
                };
            }
        },
        None => None,
    };
    screens.update(
        menu,
        Frame {
            tree,
            data,
            console,
            native,
            video,
            now,
            input,
            audio,
            field_font: field.as_ref(),
        },
    )
}

/// The keys and buttons bound to the actions whose held keys the level
/// receives again as presses when the console closes (console-window.md
/// 3.4; `llookout` and `rlookout` are not acted on here).
fn resumed_controls(controls: &crate::controls::Controls) -> Vec<Control> {
    use crate::controls::MoveAction as A;
    [
        A::Left,
        A::Right,
        A::Up,
        A::Down,
        A::Crouch,
        A::Accel,
        A::Forward,
        A::Back,
        A::LStrafe,
        A::RStrafe,
        A::WpnFire,
    ]
    .into_iter()
    .flat_map(|a| controls.bindings.controls(a))
    .collect()
}

/// The keys the inventory screen reads, from the effective bindings
/// (inventory-screen.md 6.1).
fn inventory_keys(data: &GameData, controls: &crate::controls::Controls) -> inventory_screen::Keys {
    use crate::controls::MoveAction;
    inventory_screen::Keys {
        inventory: controls.keys(MoveAction::Inventory),
        drop: controls.keys(MoveAction::Drop),
        quit: data.quit_keys.clone(),
    }
}

/// The keys the PDA window reads, from the effective bindings
/// (pda-window.md 4.1, 5).
fn pda_keys(data: &GameData, controls: &crate::controls::Controls) -> pda_window::Keys {
    use crate::controls::MoveAction;
    pda_window::Keys {
        actions: [
            MoveAction::ActiveJobs,
            MoveAction::Map,
            MoveAction::Contacts,
        ]
        .map(|a| controls.keys(a)),
        quit: data.quit_keys.clone(),
    }
}

/// The inventory screen's menu font at the size drawn at (its width,
/// inventory-screen.md 5.6); one that cannot be created is fatal
/// (text-and-fonts.md 2.5, 2.6).
fn menu_font(
    tree: &dyn Vfs,
    data: &GameData,
    assets: &mut Assets,
    screen: &InventoryScreen,
    video: Resolution,
) -> io::Result<FieldFont> {
    let fonts = assets.fonts(tree, data, video.height)?;
    Ok(FieldFont {
        font: fonts.get(screen.layout().menu.font).clone(),
        scale_x: video.scale().0,
    })
}

/// See [`App::level_controls`].
/// The memory figures of the latest load (console-diagnostics.md 2.4),
/// bytes: `* phase cmem` as each caption began, by its string-table id, and
/// the figures at the load's end (`* phase cmem`, *h* and *t*), once it has
/// ended; compared with the original's (docs/architecture/memory.md).
/// Beside them the file cache's: under each caption that has ended, the
/// most it held and its budget then (`file_cache`, in the captions' order).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LoadMemory {
    pub captions: Vec<(&'static str, u64)>,
    pub file_cache: Vec<(u64, u64)>,
    pub end: Option<LoadEnd>,
}

/// The file cache's figure for the window [`file_cache::mark`] ends, in
/// which the original has `open` open: the most it held and its budget.
fn file_cache_figure(open: &[(String, u64)]) -> (u64, u64) {
    let window = file_cache::mark();
    (window.most, file_cache::budget(&window, open))
}

/// [`LoadMemory`]'s figures at the load's end, bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LoadEnd {
    pub cmem: u64,
    pub crt_heap: u64,
    pub textures: u64,
    /// The game scripts' pool in use (*g*).
    pub game_lua: u64,
    /// The file cache: held at the load's end, its budget then (the files
    /// the original has open to the level's end, and any being read), and
    /// the most it held since the load began.
    pub file_cache: u64,
    pub file_cache_budget: u64,
    pub file_cache_most: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelControls {
    pub crouch: bool,
    /// The crouch toggle engaged in the low crouch's box.
    pub low_crouch: bool,
    pub sprint: bool,
    /// The touch stick's sprint lock holds it forward.
    pub sprint_lock: bool,
    /// The touch Torch button: `None` hidden, else whether the torch is on
    /// ([`crate::torch::PlayerTorches::buttons`]).
    pub torch: Option<bool>,
    /// The touch Night vision button: `None` hidden, else whether night
    /// vision is active.
    pub night_vision: Option<bool>,
}

/// [`App::device_destroyed`], with the fields it takes.
fn device_destroyed(release: &mut Option<Box<dyn FnMut()>>, console: &mut Console) {
    if let Some(release) = release {
        release();
    }
    console.device_destroyed();
}

pub struct App {
    data: GameData,
    assets: Assets,
    /// Platform time of engine clock 0, set by the first update.
    origin: Option<f64>,
    /// The engine clock at the latest update, milliseconds.
    clock: u64,
    /// `None` until the first update.
    phase: Option<Phase>,
    /// New Game loads started, the latest one's number ([`App::load`]).
    loads: u64,
    /// Why the latest new game failed to load or ended.
    error: Option<String>,
    /// A fatal error met during this update, taken by [`App::fatal`] at
    /// its end.
    failed: Option<String>,
    /// The game type of the latest game started in this run
    /// (loading-screen.md section 5).
    game_type: Option<&'static str>,
    /// The game type's prefetched models, made under its first New Game's
    /// `st_prefetching_objects` and kept while the game type stays
    /// (docs/architecture/memory.md, Lifetimes).
    prefetched: Option<Arc<crate::objects::Prefetched>>,
    /// Platform time of the update that began the first caption, while
    /// loading.
    load_started: Option<f64>,
    /// Platform time the phase timer last restarted (the load or its
    /// latest caption began, console-diagnostics.md 2.4), until the
    /// loading screen finishes.
    phase_started: Option<f64>,
    /// Set with the level: when its loading began, until the next update.
    loaded_since: Option<f64>,
    /// Platform milliseconds from the first caption to the update after
    /// the level was made: every stage (one frame each at least) and the
    /// first frame drawn (which uploads the world).
    load_ms: Option<f64>,
    /// The latest load's memory figures.
    load_memory: LoadMemory,
    /// The files the original has open under the load's last caption, to
    /// the level's end: the file cache's budget at the load's end.
    load_open: Vec<(String, u64)>,
    /// The settings and bindings (docs/behavior/options-screens.md).
    console: Console,
    /// The menu's sub-screens and boxes, and the saved games.
    screens: MenuScreens,
    /// The screen's own size in pixels, which the resolution list offers.
    native: Resolution,
    /// The resolution drawn at: `vid_mode` as the last video restart
    /// applied it (sections 5.4, 9).
    video: Resolution,
    /// `r__supersample` as the last video restart applied it (the target is
    /// built at device creation and every reset, frame-presentation.md 1.2).
    supersample: u32,
    /// The user configuration file to store, once something asked for it.
    save: Option<Vec<u8>>,
    /// The console's generation as last applied to the game data.
    applied: u64,
    /// The sound volumes as last set.
    volumes: Option<[f32; 2]>,
    /// The sound system's voices were set from `snd_targets`.
    sound_voices_set: bool,
    /// The last frame was in a level, for the sound system.
    sound_in_level: bool,
    /// The sound system is paused for the menu over the level.
    sound_paused: bool,
    /// The running level's sounds (crate::level_sound).
    level_sounds: Option<LevelSounds>,
    /// The sound rules' state kept for the whole run.
    sound_run: SoundRun,
    /// The pause and the global clock (sound-weather-ambient.md section
    /// 3.1; [`App::sync_pause`]).
    pause: Pause,
    global_clock: GlobalClock,
    /// The interface sequence item playing at the last [`App::sync_pause`].
    pause_item: Option<(SequenceId, usize)>,
    /// The engine clock's advance since the previous update, milliseconds.
    frame_ms: u64,
    /// The console may have changed this frame.
    console_touched: bool,
    /// Alt+F4 ended the game (console-input.md section 1).
    alt_f4: bool,
    /// A console line's `quit` ended the game (steering-the-original.md 7).
    quit: bool,
    /// Releases the platform's graphics resources at exit
    /// ([`App::set_device_release`]).
    device_release: Option<Box<dyn FnMut()>>,
    /// Updates so far.
    frames: u64,
    /// The update in which a lost pointer lock opened the menu.
    unlock_opened: Option<u64>,
    /// Whether the platform prepares a level's textures while it loads
    /// ([`App::set_prepare_textures`]).
    prepare_textures: bool,
    /// The menu was asked for (the `quit` key, a gamepad's east button,
    /// the touch Menu button, a lost pointer lock) while the loading image
    /// covered the level: it opens once the player's controls are live
    /// (our design, #96).
    menu_asked: bool,
    /// The page is hidden: nothing runs ([`App::set_hidden`]).
    hidden: bool,
    /// The page showed again: the next update takes the engine clock on
    /// from where it stopped.
    shown: bool,
    /// The inventory screen's menu font at the size drawn at.
    menu_font: Option<(Resolution, FieldFont)>,
    /// The display colour settings and their ramp as last computed.
    ramp: Option<(DisplayColour, Option<[u16; 256]>)>,
    /// `[reduce_lod_texture_list]` (texture-quality-filtering.md 1.4).
    texture_list: Arc<[String]>,
    /// The maximum anisotropy and mip bias in force (sections 5 – 7).
    samplers: SamplerStates,
    /// Whether the page is in the browser's full screen, as it last said;
    /// `None` where it can't be ([`App::set_full_screen`]).
    page_full_screen: Option<bool>,
    /// Full screen to enter (true) or leave, asked of the page by a video
    /// restart ([`App::take_full_screen_request`]).
    full_screen_request: Option<bool>,
    /// The statistics overlay of `rs_stats` (display-mode.md part B).
    stats: StatisticsOverlay,
    /// The console window (docs/behavior/console-window.md).
    window: ConsoleWindow,
    /// The main menu was active after the previous update.
    menu_was_active: bool,
}

/// The layout surface of the main menu while it asks for the
/// post-process (text-and-fonts.md 4.4, frame-presentation.md 2.5, 5.1):
/// the title menu, its New Game list and its message boxes draw into the
/// off-screen target at every `r__supersample`, so they lay out on its
/// size (the screen's at 1, unless an axis exceeds 2048). Its other screens
/// lay out on the screen (5.2).
pub fn main_menu_surface(screen: Resolution, supersample: u32) -> crate::draw::Surface {
    let (width, height) =
        crate::render::present::target_size(screen.width, screen.height, supersample);
    crate::draw::Surface {
        size: Resolution::new(width, height),
        screen,
    }
}

/// `r__supersample`, 1 … 4.
pub fn supersample(console: &Console) -> u32 {
    (console.number("r__supersample") as u32).clamp(1, 4)
}

/// At a video restart, the full screen to ask of the page: `rs_fullscreen`
/// where the page, in full screen `page` (`None`: it can't be), differs
/// (display-mode.md A.9 step 1).
fn display_mode_request(console: &Console, page: Option<bool>) -> Option<bool> {
    let wanted = console.on("rs_fullscreen");
    page.is_some_and(|on| on != wanted).then_some(wanted)
}

/// `r__geometry_lod`, `r1_ssa_lod_a` and `r1_ssa_lod_b`
/// (object-detail-level.md 1).
fn geometry_lod(console: &Console) -> [f32; 3] {
    ["r__geometry_lod", "r1_ssa_lod_a", "r1_ssa_lod_b"].map(|name| console.number(name))
}

/// What a level takes from the console before each update: the renderer's
/// light settings (dynamic-lights.md 8, glows.md 5), the size its frames
/// are drawn at (`video`, the video mode) and the target's supersampling
/// (`supersample`, as the device was last made or reset with), the view
/// distance, the geometry detail level (object-detail-level.md 1) and the
/// grass density (detail-objects.md 7), with `wind` the wind's strength.
pub fn level_settings(
    level: &mut LevelScreen,
    console: &Console,
    (video, supersample): (Resolution, u32),
    wind: f32,
) {
    let status = |name| console.status(name);
    let settings = LightSettings::from_statuses(
        status("r1_dlights").as_deref(),
        status("r1_dlights_clip").as_deref(),
        status("r2_sun_lumscale").as_deref(),
    );
    let glows = console.number("r1_glows_per_frame") as u32;
    level.set_light_settings(settings, glows);
    level.set_view_size(video);
    level.set_supersample(supersample);
    level.set_vis_distance(console.number("rs_vis_distance"));
    let [lod, a, b] = geometry_lod(console);
    level.set_geometry_lod(lod, a, b);
    level.set_detail_settings(console.number("r__detail_density"), wind);
}

/// What a frame takes from the console besides the display colour's ramp
/// (display-gamma.md, [`DisplayColour`]): the presentation of the target
/// (`supersample`, frame-presentation.md), the samplers' values in force
/// (texture-quality-filtering.md 5), the texture quality `quality` and the
/// geometry detail level; `frame_ms` is the frame's time.
pub fn frame_settings(
    scene: &mut Scene,
    console: &Console,
    supersample: u32,
    samplers: &SamplerStates,
    quality: TextureQuality,
    frame_ms: u64,
) {
    scene.presentation = Presentation {
        supersample,
        pps: pps(console),
        frame_time: frame_ms as f32 / 1000.0,
        resets: samplers.resets(),
    };
    let s = samplers.in_force();
    scene.filtering = Filtering {
        anisotropy: s.anisotropy,
        mip_bias: s.bias,
    };
    scene.texture_quality = quality;
    scene.geometry_lod = console.number("r__geometry_lod");
    scene.lmodel_lerp = lmodel_lerp(console);
}

/// `r1_lmodel_lerp` (object-projected-light.md 6).
fn lmodel_lerp(console: &Console) -> f32 {
    console.number("r1_lmodel_lerp")
}

/// `r1_pps_u` and `r1_pps_v` (frame-presentation.md 3.2).
fn pps(console: &Console) -> [f32; 2] {
    ["r1_pps_u", "r1_pps_v"].map(|name| console.number(name))
}

/// The resolution of `vid_mode`.
fn video_mode(console: &Console) -> Resolution {
    let (w, h) = console.video_mode();
    Resolution::new(w, h)
}

/// What the engine interface reports.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum AppState {
    Loaded {
        clock: u64,
    },
    Startup {
        clock: u64,
        startup: MovieSequenceState,
    },
    Menu {
        clock: u64,
        #[serde(flatten)]
        menu: Box<MenuState>,
    },
    Loading {
        clock: u64,
        difficulty: &'static str,
        /// `None` while the last menu frame shows.
        #[serde(skip_serializing_if = "Option::is_none")]
        loading: Option<LoadingScreenState>,
    },
    Level {
        clock: u64,
        level: Box<LevelState>,
        load_ms: Option<f64>,
        /// Frames the loading image still covers, this one included.
        overlay_frames: u32,
        /// The menu was asked for under the loading image and opens once
        /// the controls are live (#96).
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        menu_asked: bool,
        /// The New Game intro while it plays.
        #[serde(skip_serializing_if = "Option::is_none")]
        intro: Option<MovieSequenceState>,
        /// The menu while it is open.
        #[serde(skip_serializing_if = "Option::is_none")]
        menu: Option<Box<MenuState>>,
        /// The inventory screen while it is open.
        #[serde(skip_serializing_if = "Option::is_none")]
        inventory: Option<Box<InventoryState>>,
        /// The PDA window while it is open.
        #[serde(skip_serializing_if = "Option::is_none")]
        pda: Option<Box<PdaState>>,
    },
}

/// Whole milliseconds since the phase timer last restarted, restarting it
/// at `time_ms` (console-diagnostics.md 2.4).
fn phase_ms(started: &mut Option<f64>, time_ms: f64) -> u64 {
    let since = started.replace(time_ms).unwrap_or(time_ms);
    (time_ms - since).max(0.0) as u64
}

impl App {
    pub fn new(data: GameData) -> io::Result<Self> {
        TitleMenu::check(&data)?;
        let mut console = data.console();
        // The original ends at startup (console-commands.md section 5).
        if let Some(why) = console.fatal() {
            return Err(io::Error::other(why.to_owned()));
        }
        // TODO(#654): the user configuration file's actions at startup.
        console.take_actions();
        let video = video_mode(&console);
        let supersample = supersample(&console);
        let texture_list = TextureQuality::read_list(&data.system);
        // Its `[evaluation]` lines are read at device creation (B.3).
        let stats = StatisticsOverlay::new(&data.system)?;
        // The device is made once the user configuration has run (7.2).
        let samplers = SamplerStates::at_creation(&console);
        Ok(Self {
            texture_list,
            samplers,
            page_full_screen: None,
            full_screen_request: None,
            stats,
            window: ConsoleWindow::default(),
            menu_was_active: false,
            applied: console.generation(),
            console,
            screens: MenuScreens::default(),
            native: video,
            video,
            supersample,
            save: None,
            volumes: None,
            sound_voices_set: false,
            sound_in_level: false,
            sound_paused: false,
            level_sounds: None,
            sound_run: SoundRun::default(),
            pause: Pause::default(),
            global_clock: GlobalClock::default(),
            pause_item: None,
            frame_ms: 0,
            console_touched: false,
            alt_f4: false,
            quit: false,
            device_release: None,
            frames: 0,
            unlock_opened: None,
            prepare_textures: false,
            menu_asked: false,
            hidden: false,
            shown: false,
            menu_font: None,
            ramp: None,
            data,
            assets: Assets::default(),
            origin: None,
            clock: 0,
            phase: None,
            loads: 0,
            error: None,
            failed: None,
            game_type: None,
            prefetched: None,
            load_started: None,
            phase_started: None,
            loaded_since: None,
            load_ms: None,
            load_memory: LoadMemory::default(),
            load_open: Vec::new(),
        })
    }

    pub fn data(&self) -> &GameData {
        &self.data
    }

    /// Whether a New Game's loading waits under `st_loading_textures`
    /// while the platform prepares the level's textures
    /// ([`App::textures_to_prepare`]; our addition, #113: a platform whose
    /// GPU lacks the textures' format transcodes them there).
    pub fn set_prepare_textures(&mut self, on: bool) {
        self.prepare_textures = on;
    }

    /// The level's texture files, while the loading waits for the platform
    /// to prepare them.
    pub fn textures_to_prepare(&self) -> Option<&[String]> {
        match &self.phase {
            Some(Phase::Loading {
                stages: NewGame::Stages(loading),
                ..
            }) => loading.textures_to_prepare(),
            _ => None,
        }
    }

    /// Whether the loading waits under `st_prefetching_objects` for the
    /// platform to make the prefetched models ([`App::prefetch_models`];
    /// with [`App::set_prepare_textures`]).
    pub fn models_to_prefetch(&self) -> bool {
        matches!(
            &self.phase,
            Some(Phase::Loading {
                stages: NewGame::Stages(loading),
                ..
            }) if loading.models_to_prefetch()
        )
    }

    /// Makes the next prefetched models from `tree`, handing each chunk of
    /// their geometry to `keep` (the platform's renderer keeps it) as it is
    /// made; returns whether the loading waits for more
    /// ([`Loading::prefetch_models`]).
    pub fn prefetch_models(
        &mut self,
        tree: &dyn Vfs,
        keep: &mut dyn FnMut(crate::objects::GeometryChunk),
    ) -> bool {
        match &mut self.phase {
            Some(Phase::Loading {
                stages: NewGame::Stages(loading),
                ..
            }) => loading.prefetch_models(tree, &self.data, keep),
            _ => false,
        }
    }

    /// Whether [`App::textures_to_prepare`] are the prefetched models'
    /// (under `st_prefetching_objects`), kept while the game type stays,
    /// rather than the level's.
    pub fn prefetching_textures(&self) -> bool {
        matches!(
            &self.phase,
            Some(Phase::Loading {
                stages: NewGame::Stages(loading),
                ..
            }) if loading.prefetching_textures()
        )
    }

    /// The number of the New Game load in progress, or of the one that
    /// made the level shown; `None` without either. What the platform
    /// prepares for a load belongs to it: once this changes the load was
    /// abandoned (#262).
    pub fn load(&self) -> Option<u64> {
        match &self.phase {
            Some(Phase::Loading { .. } | Phase::Level { .. }) => Some(self.loads),
            _ => None,
        }
    }

    /// The platform has prepared [`App::textures_to_prepare`]: the loading
    /// goes on at the next update.
    pub fn textures_prepared(&mut self) {
        if let Some(Phase::Loading {
            stages: NewGame::Stages(loading),
            ..
        }) = &mut self.phase
        {
            loading.textures_prepared();
        }
    }

    /// The level's world, while the loading waits for the platform to
    /// prepare it for the GPU (with [`App::set_prepare_textures`], #184).
    pub fn world_to_prepare(&self) -> Option<&Arc<LevelWorld>> {
        match &self.phase {
            Some(Phase::Loading {
                stages: NewGame::Stages(loading),
                ..
            }) => loading.world_to_prepare(),
            _ => None,
        }
    }

    /// The level's start view at `resolution`, while the loading waits for
    /// the platform to prepare its world ([`App::world_to_prepare`]): the
    /// platform draws it once, unseen, so the GPU's first-draw work is done
    /// under the loading image (#355, #376).
    pub fn level_to_prepare_scene(&self, resolution: Resolution) -> Option<Scene> {
        match &self.phase {
            Some(Phase::Loading {
                stages: NewGame::Stages(loading),
                ..
            }) => Some(loading.level_to_prepare()?.scene(resolution, self.clock)),
            _ => None,
        }
    }

    /// The platform has prepared [`App::world_to_prepare`]: the level shows
    /// at the next update.
    pub fn world_prepared(&mut self) {
        if let Some(Phase::Loading {
            stages: NewGame::Stages(loading),
            ..
        }) = &mut self.phase
        {
            loading.world_prepared();
        }
    }

    /// The saved-games folder as the platform found it.
    pub fn set_saved_games(&mut self, saves: SavedGames) {
        self.screens.set_saves(saves);
    }

    /// Files of the saved-games folder the player deleted since the last
    /// call, relative to the folder, for the platform to remove.
    pub fn take_deleted_saves(&mut self) -> Vec<String> {
        self.screens.take_deleted()
    }

    /// How the menu opens now (title-menu.md section 4.1).
    fn opening(&self, in_game: bool) -> Opening {
        let last = self.console.last_save();
        Opening {
            in_game,
            last_save: self.screens.saves().last_save_valid(last),
        }
    }

    /// The engine clock, milliseconds.
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// One frame at platform time `time_ms` with this frame's `input`,
    /// whose cursor the UI sequences' image items place (title-menu.md
    /// section 7). Loading a level reads from `tree`.
    pub fn update(
        &mut self,
        tree: &dyn Vfs,
        time_ms: f64,
        input: &mut Input,
        audio: &mut dyn AudioOut,
    ) {
        if self.hidden {
            return;
        }
        self.update_phase(tree, time_ms, input, audio);
        self.follow_menu();
        self.alt_f4(input);
        // A line the console executed ended the program (an unknown key
        // name, console-key-bindings.md section 2).
        if let Some(why) = self.console.take_fatal() {
            self.failed.get_or_insert(why);
        }
        self.run_console_actions(input, audio);
        // A fatal error the level met, or a placed effect's group
        // (particle-groups.md 2.3): the game ends with it.
        if let Some(Phase::Level { level, .. }) = &mut self.phase {
            let effects = level.effects().and_then(|e| e.fatal()).map(str::to_owned);
            if let Some(error) = level.take_fatal().or(effects) {
                self.failed.get_or_insert(error);
            }
        }
        self.sync_pause();
        let failed = match self.failed.take() {
            Some(error) => Some(error),
            None => self.sound_frame(tree, audio).err().map(|f| f.to_string()),
        };
        if let Some(error) = failed {
            self.fatal(tree, error, input, audio);
        }
    }

    /// The console as the main menu opens and closes (console-window.md
    /// sections 1, 2): the menu remembers it and closes it, and opens it
    /// again as it closes; a level starting closes it.
    fn follow_menu(&mut self) {
        let active = matches!(
            self.phase,
            Some(Phase::Menu(_)) | Some(Phase::Level { menu: Some(_), .. })
        );
        if matches!(self.phase, Some(Phase::Loading { .. })) {
            self.window.level_started();
        } else if active && !self.menu_was_active {
            self.window.menu_opened();
        } else if !active && self.menu_was_active {
            self.window.menu_closed(&mut self.console);
        }
        self.menu_was_active = active;
    }

    /// The input system's own rule, whichever part has input
    /// (console-input.md section 1): with F4 and either Alt key down after
    /// the frame's events, the program quits as after `quit` (a normal
    /// exit: the log file and the configuration file are written), once.
    fn alt_f4(&mut self, input: &Input) {
        let down = |k| input.device_held(Control::Key(k));
        if self.alt_f4 || !down(Key::F4) || !(down(Key::LeftAlt) || down(Key::RightAlt)) {
            return;
        }
        self.alt_f4 = true;
        self.device_destroyed();
        self.console.log_mut().save();
        self.save = Some(self.console.save());
    }

    /// A fatal error during play (as sound-ogg-params.md section 5 makes a
    /// sound file that cannot be opened, or inventory-screen.md 3.2 an item
    /// with no room in its list): the game ends, as when loading fails, and
    /// the title menu opens saying why. Never recovered from.
    fn fatal(&mut self, tree: &dyn Vfs, error: String, input: &Input, audio: &mut dyn AudioOut) {
        // The title menu that already says why fails again (drawing it):
        // it stays, not opened anew each frame.
        if matches!(self.phase, Some(Phase::Menu(_))) && self.error.as_ref() == Some(&error) {
            return;
        }
        // The game's log carries the report: the trace only says how the
        // game ended (docs/architecture/diagnostics.md).
        diagnostics::end("fatal", "");
        // The report, a fatal message's, is written to the log, then the
        // log file (console-log.md 3.3, 3.8); a browser has no stack trace
        // of the game to add (#646). The end through the unhandled
        // exception (3.10) adds no stack trace or system error after a
        // report, and writes the file once more.
        let log = self.console.log_mut();
        let argument = [error.as_str()];
        log.fatal(&FatalReport::message(
            &argument,
            "App::fatal",
            file!(),
            line!() as i32,
        ));
        log.save();
        self.error = Some(error);
        let now = self.clock;
        let title = self.opening(false);
        if let Some(Phase::Level {
            menu,
            intro,
            overlay,
            ..
        }) = &mut self.phase
        {
            if let Some(m) = menu {
                m.stop(audio);
            }
            intro.stop(audio);
            if let Some(o) = overlay.take() {
                o.end(audio);
            }
        }
        self.phase = Some(Phase::Menu(TitleMenu::open(
            &self.data, now, input, audio, title,
        )));
        self.screens.menu_opened();
        self.sync_pause();
        // The level's sounds go as when the level is left.
        let _ = self.sound_frame(tree, audio);
    }

    /// The events of the pause's table (sound-weather-ambient.md section
    /// 3.1) that this frame's state shows: an interface sequence's item
    /// ending or starting (the logo sequence, the New Game intro), then the
    /// main menu opening or closing (on the title screen, or over a level;
    /// it closes as a New Game starts, before the level is loaded). In this
    /// order, so the menu that opens as the logos end finds the pause off.
    /// Our logo player moves from one item to the next, and from the last
    /// to the menu, within one update, so no unpaused frame passes between
    /// them (TODO(#335): the spec leaves those frames open). The input
    /// focus is [`App::set_hidden`]'s; the table's other events are
    /// TODO(#334).
    fn sync_pause(&mut self) {
        let data = &self.data;
        let (item, menu) = match &self.phase {
            Some(Phase::Startup(s)) => (s.current(data).map(|c| (SequenceId::Logo, c)), false),
            Some(Phase::Menu(_)) => (None, true),
            Some(Phase::Level { intro, menu, .. }) => (
                intro.current(data).map(|c| (SequenceId::Intro, c)),
                menu.is_some(),
            ),
            Some(Phase::Loading { .. }) | None => (None, false),
        };
        let key = item.map(|(id, (i, _))| (id, i));
        if key != self.pause_item {
            if self.pause_item.is_some() {
                self.pause.item_ended();
            }
            if let Some((_, (_, state))) = item {
                self.pause.item_started(state);
            }
            self.pause_item = key;
        }
        self.pause.main_menu(menu);
    }

    /// What the sound system takes from this frame
    /// (docs/behavior/sound-positional.md): its voices (`snd_targets`) once
    /// the settings are read; the listener, the level's camera; a pause
    /// while the menu is open over the level (section 4.5); every sound
    /// stopped when the level is left (section 4.4). The level's own sounds
    /// (crate::level_sound) are created once the level runs and the sound
    /// system exists, and their rules run on the level's frames while the
    /// pause is off, on the global clock (sound-weather-ambient.md section
    /// 3.1). The platform runs the sound system's update. A fatal error of
    /// theirs is returned.
    fn sound_frame(&mut self, tree: &dyn Vfs, audio: &mut dyn AudioOut) -> Result<(), Fatal> {
        let paused = self.pause.is_on();
        self.sound_run
            .frame_time(self.frame_ms as f32 / 1000.0, paused);
        // The player's body sound events of this frame (S17 3, 6), taken
        // with or without audio output.
        let body_events = match &mut self.phase {
            Some(Phase::Level { level, .. }) => level.take_body_events(),
            _ => Vec::new(),
        };
        if audio.sounds().is_none() {
            return Ok(());
        }
        if !self.sound_voices_set {
            // The sound cache, made with the device (console-diagnostics.md
            // 2.1.1).
            if let Some(size) = self
                .console
                .status("snd_cache_size")
                .and_then(|v| v.trim().parse::<u32>().ok())
            {
                audio.set_sound_cache(size);
            }
        }
        let Some(sounds) = audio.sounds() else {
            return Ok(());
        };
        if !self.sound_voices_set {
            self.sound_voices_set = true;
            if let Some(voices) = self
                .console
                .status("snd_targets")
                .and_then(|v| v.trim().parse::<usize>().ok())
            {
                sounds.restart(voices);
            }
        }
        let mut fatal = None;
        let (in_level, menu_open) = match &mut self.phase {
            Some(Phase::Level {
                level,
                menu,
                overlay,
                ..
            }) => {
                let v = level.player().view();
                sounds.set_listener(Listener {
                    position: v.eye,
                    direction: v.direction,
                    up: v.up,
                });
                let mut created_now = false;
                if let Some(data) = level.take_sound_data() {
                    match LevelSounds::create(sounds, tree, &self.data.aliases, &data) {
                        Ok(created) => {
                            sounds.occlusion_mut().level = Some(level.shared_collision());
                            self.level_sounds = Some(created);
                            created_now = true;
                        }
                        Err(e) => fatal = Some(e),
                    }
                }
                // The zones' idle sounds (S15 3): those of zones that came
                // online since are created; created this frame, the sounds
                // play as the zones stand, else as this frame changed them.
                if let Some(level_sounds) = &mut self.level_sounds {
                    let added = if level.zones().zones().len() > level_sounds.zones.len() {
                        let names = level.zone_sound_names();
                        let dir = level.world().dir.clone();
                        level_sounds.add_zones(sounds, tree, &self.data.aliases, &dir, &names)
                    } else {
                        Ok(())
                    };
                    if let Err(e) = added {
                        fatal = Some(e);
                    } else if created_now {
                        level_sounds.zone_events(sounds, &level.zone_sounds_playing());
                    } else {
                        level_sounds.zone_events(sounds, level.zone_sound_events());
                    }
                }
                if !paused {
                    let now = self.global_clock.now();
                    let (inputs, mut particles) = level.sound_frame_parts();
                    if let Some(level_sounds) = &mut self.level_sounds
                        && let Err(e) = self.sound_run.frame(
                            sounds,
                            level_sounds,
                            &inputs,
                            now,
                            overlay.is_some(),
                            &mut particles,
                        )
                    {
                        fatal = Some(e);
                    }
                    // The player's steps of this frame (S11) on the global
                    // clock, which is the step clock (S11 3.1), played
                    // independently (4.3).
                    if let Err(e) = level.footsteps_frame(now as u32) {
                        fatal = Some(e);
                    } else if let (Some(level_sounds), Some(f)) =
                        (&self.level_sounds, level.footsteps())
                    {
                        for step in f.sounds() {
                            level_sounds.pairs.play(sounds, step);
                        }
                    }
                }
                // The torches' night-vision sounds of this frame (torch.md
                // 14), made by its update.
                for sound in level.take_torch_sounds() {
                    if let Some(level_sounds) = &mut self.level_sounds
                        && let Err(e) = level_sounds.torch(
                            sounds,
                            tree,
                            &self.data.aliases,
                            sound,
                            level.torches().night_vision(sound.torch),
                        )
                    {
                        fatal = Some(e);
                    }
                }
                // The detectors' ticks and stops of this frame
                // (sound-detector.md 3, 6), made by its update.
                for sound in level.take_detector_sounds() {
                    if let Some(level_sounds) = &mut self.level_sounds
                        && let Err(e) = level_sounds.detector(
                            sounds,
                            tree,
                            &self.data.aliases,
                            sound,
                            level.detectors().section(sound.detector),
                        )
                    {
                        fatal = Some(e);
                    }
                }
                // The player's body sounds (S17) on the global clock: the
                // hits' and death's sounds as they came; the per-update
                // sounds and the shock's per-frame update while the pause
                // is off; the shock's camera effects every frame.
                if let Some(body) = (self.level_sounds.as_mut()).and_then(|l| l.body.as_mut()) {
                    let now = self.global_clock.now();
                    let seconds = now as f32 / 1000.0;
                    for event in body_events {
                        body.event(sounds, event, seconds);
                    }
                    if !paused {
                        let state = level.body_state();
                        body.update(sounds, state);
                        body.frame_at(sounds, now, state.alive);
                    }
                    body.camera_effects(seconds, self.sound_run.thunder.frame_time);
                }
                (true, menu.is_some())
            }
            _ => (false, false),
        };
        if self.sound_in_level && !in_level {
            sounds.stop_all();
            if let Some(level_sounds) = self.level_sounds.take() {
                level_sounds.discard(sounds);
            }
            self.sound_run.level_left();
        }
        self.sound_in_level = in_level;
        if menu_open != self.sound_paused {
            if menu_open {
                sounds.pause();
            } else {
                sounds.resume();
            }
            self.sound_paused = menu_open;
        }
        fatal.map_or(Ok(()), Err)
    }

    fn update_phase(
        &mut self,
        tree: &dyn Vfs,
        time_ms: f64,
        input: &mut Input,
        audio: &mut dyn AudioOut,
    ) {
        if self.hidden {
            return;
        }
        self.frames += 1;
        // The filtering variables executed since (6.1).
        self.samplers.follow(&self.console);
        if let Some(at) = self.loaded_since.take() {
            self.load_ms = Some(time_ms - at);
            let level = match &self.phase {
                Some(Phase::Level { level, .. }) => level.world().name.as_str(),
                _ => "",
            };
            tracing::info!(level, load_ms = time_ms - at, "level made");
        }
        if std::mem::take(&mut self.shown) {
            // The hidden time never reaches the engine clock: this frame's
            // delta is zero (our design, #93).
            self.origin = Some(time_ms - self.clock as f64);
        }
        let origin = *self.origin.get_or_insert(time_ms);
        let previous = self.clock;
        self.clock = self.clock.max((time_ms - origin).max(0.0) as u64);
        self.frame_ms = self.clock - previous;
        // The global clock at the frame's start, the pause as the previous
        // frame left it (sound-weather-ambient.md section 3.1). The engine
        // clock starts at the first update and leaves out the hidden page's
        // time, during which the pause is on anyway (focus loss).
        self.global_clock.frame(self.clock, self.pause.is_on());
        if let Some(Phase::Level { level, .. }) = &mut self.phase {
            // The lamps' clock (hanging-lamps.md 4.3).
            level.set_global_clock(self.global_clock.now());
            // weather-ambient-effects.md 4.3: the wind as the sound rules
            // last updated it (without audio output they do not run, #352).
            level_settings(
                level,
                &self.console,
                (self.video, self.supersample),
                self.sound_run.wind.strength,
            );
            // player-hits.md 3: the immunities follow `g_game_difficulty`.
            if let Some(difficulty) = self.console.status("g_game_difficulty") {
                level.set_difficulty(&difficulty);
            }
        }
        let now = self.clock;
        let title = self.opening(false);
        let in_game = self.opening(true);
        let data = &self.data;
        let phase = self
            .phase
            .get_or_insert_with(|| Phase::Startup(Startup::new(now)));
        let resolutions = (self.native, self.video);
        match phase {
            Phase::Startup(startup) => {
                startup.update(data, now, quit_presses(data, input), audio);
                if let Some(at) = startup.take_cursor() {
                    input.place_cursor(at);
                }
                if startup.finished(data) {
                    // Shown within one frame of the sequence's end.
                    *phase = Phase::Menu(TitleMenu::open(data, now, input, audio, title));
                    self.screens.menu_opened();
                }
            }
            Phase::Menu(menu) => {
                let below = (self.window).receiver_frame(&mut self.console, input, now, true, &[]);
                let input: &Input = below.as_ref().unwrap_or(input);
                let out = menu_frame(
                    &mut self.screens,
                    menu,
                    tree,
                    data,
                    &mut self.assets,
                    &mut self.console,
                    resolutions,
                    now,
                    input,
                    audio,
                );
                self.console_touched = true;
                if let Some(error) = &out.fatal {
                    self.failed.get_or_insert(error.clone());
                }
                if out.video_restart {
                    // `vid_restart` (options-screens.md section 5.4).
                    self.video = video_mode(&self.console);
                    self.supersample = supersample(&self.console);
                    self.full_screen_request =
                        display_mode_request(&self.console, self.page_full_screen);
                    // A device reset (texture-quality-filtering.md 7.3).
                    self.samplers.reset(&self.console);
                    menu.resized(data, self.video);
                }
                if out.sound_restart {
                    // `snd_restart`, after `vid_restart` (options-screens.md
                    // 5.4): the next sound frame restarts the sound system.
                    self.sound_voices_set = false;
                }
                if menu.quit_requested() {
                    // The program's normal end (console-log.md 3.3), the
                    // graphics device destroyed (console-diagnostics.md
                    // 2.5).
                    device_destroyed(&mut self.device_release, &mut self.console);
                    self.console.log_mut().save();
                }
                if out.store_config || menu.quit_requested() {
                    // Stored when the options screen closes, not only at exit
                    // (our choice: a browser tab is usually closed without
                    // the game's shutdown).
                    self.save = Some(self.console.save());
                }
                if let Some(difficulty) = out.new_game {
                    // level-loading.md section 2.1.
                    self.console
                        .exec(&format!("g_game_difficulty {difficulty}"));
                    // What the menu music does while loading is not
                    // established (loading-screen.md section 6); it stops.
                    menu.stop(audio);
                    let Phase::Menu(menu) =
                        std::mem::replace(phase, Phase::Startup(Startup::new(now)))
                    else {
                        unreachable!()
                    };
                    *phase = Phase::Loading {
                        difficulty,
                        stages: NewGame::Closing(Box::new(menu)),
                    };
                    // The device is reset as the menu goes, before the
                    // level is made (texture-quality-filtering.md 8.1).
                    self.samplers.reset(&self.console);
                    self.loads += 1;
                }
            }
            Phase::Loading { stages, .. } => {
                let step = match stages {
                    NewGame::Closing(_) => {
                        // Section 5: the first caption only when the game
                        // type changes. It is remembered from now on, before
                        // the first caption's work (our choice; the spec sets
                        // it after).
                        let changed = self.game_type != Some(GAME_TYPE);
                        self.game_type = Some(GAME_TYPE);
                        self.load_started = Some(time_ms);
                        // A new game type's models replace the old's: those
                        // go before the new are made.
                        if changed {
                            self.prefetched = None;
                        }
                        let mut loading = Loading::start(data, changed, self.prefetched.clone());
                        // The first caption begins as the load does.
                        self.phase_started = Some(time_ms);
                        let memory = self
                            .console
                            .phase_caption(0, &loading.screen().state().caption);
                        file_cache::begin_load();
                        self.load_memory = LoadMemory {
                            captions: vec![(loading.stage().caption_id(), memory.cmem)],
                            file_cache: Vec::new(),
                            end: None,
                        };
                        if self.prepare_textures {
                            loading.prepare_textures();
                        }
                        *stages = NewGame::Stages(Box::new(loading));
                        Ok(None)
                    }
                    NewGame::Stages(loading) => {
                        let captions = loading.screen().state().captions;
                        let step = loading.step(tree, data, now);
                        // Kept for the game type once made, whatever
                        // becomes of this load.
                        if self.prefetched.is_none() {
                            self.prefetched = loading.prefetched().cloned();
                        }
                        let screen = loading.screen().state();
                        if screen.captions != captions {
                            // The phase timer restarts whenever the global
                            // timer runs; here it always does (#619).
                            let ms = phase_ms(&mut self.phase_started, time_ms);
                            let memory = self.console.phase_caption(ms, &screen.caption);
                            (self.load_memory.captions)
                                .push((loading.stage().caption_id(), memory.cmem));
                            (self.load_memory.file_cache)
                                .push(file_cache_figure(loading.original_open()));
                        }
                        if matches!(step, Ok(Some(_))) {
                            self.load_open = loading.original_open().to_vec();
                        }
                        step.map(|level| level.map(|l| (l, loading.screen().clone())))
                    }
                };
                match step {
                    Ok(None) => {}
                    Ok(Some((level, screen))) => {
                        // The level's in-game interface: the inventory
                        // screen's layout at the `_16` rule of now
                        // (inventory-screen.md section 2), then the PDA
                        // window, its tab bar laid out in the fonts of now
                        // (pda-window.md 1, 3.2).
                        let video = self.video;
                        let interface = InventoryScreen::load(tree, data, video).and_then(|i| {
                            let fonts = self.assets.fonts(tree, data, video.height)?;
                            Ok((i, PdaWindow::load(tree, data, video, fonts)?))
                        });
                        match interface {
                            Ok((inventory, pda)) => {
                                self.error = None;
                                self.loaded_since = self.load_started.take();
                                self.menu_asked = false;
                                *phase = Phase::Level {
                                    level: Box::new(level),
                                    overlay: Some(Overlay::new(screen, audio)),
                                    intro: Intro::default(),
                                    menu: None,
                                    inventory: Some(Box::new(inventory)),
                                    pda: Some(Box::new(pda)),
                                    dropped: Vec::new(),
                                };
                            }
                            Err(e) => {
                                diagnostics::end("load_failed", &e.to_string());
                                self.error = Some(e.to_string());
                                *phase =
                                    Phase::Menu(TitleMenu::open(data, now, input, audio, title));
                                self.screens.menu_opened();
                            }
                        }
                    }
                    Err(e) => {
                        // Back to the menu, saying why (our choice).
                        diagnostics::end("load_failed", &e.to_string());
                        self.error = Some(e.to_string());
                        *phase = Phase::Menu(TitleMenu::open(data, now, input, audio, title));
                        self.screens.menu_opened();
                    }
                }
            }
            Phase::Level {
                level,
                overlay,
                intro,
                menu: Some(menu),
                ..
            } => {
                // The menu pauses the game for its whole duration, its
                // screens and boxes included (save-load-screens.md section 2),
                // and covers the world.
                level.covered_paused(now);
                let below = (self.window).receiver_frame(&mut self.console, input, now, true, &[]);
                let input: &Input = below.as_ref().unwrap_or(input);
                let out = menu_frame(
                    &mut self.screens,
                    menu,
                    tree,
                    data,
                    &mut self.assets,
                    &mut self.console,
                    resolutions,
                    now,
                    input,
                    audio,
                );
                self.console_touched = true;
                if let Some(error) = &out.fatal {
                    self.failed.get_or_insert(error.clone());
                }
                if out.video_restart {
                    self.video = video_mode(&self.console);
                    self.supersample = supersample(&self.console);
                    self.full_screen_request =
                        display_mode_request(&self.console, self.page_full_screen);
                    // A device reset (texture-quality-filtering.md 7.3).
                    self.samplers.reset(&self.console);
                    menu.resized(data, self.video);
                }
                if out.sound_restart {
                    // `snd_restart`, after `vid_restart` (options-screens.md
                    // 5.4): the next sound frame restarts the sound system.
                    self.sound_voices_set = false;
                }
                if menu.quit_requested() {
                    device_destroyed(&mut self.device_release, &mut self.console);
                    self.console.log_mut().save();
                }
                if out.store_config || menu.quit_requested() {
                    self.save = Some(self.console.save());
                }
                // An Escape that follows a lost pointer lock in the same
                // frame or the next is the one the browser used to leave the
                // lock (when it lets the press through as well): it does not
                // close the menu the unlock opened (our choice).
                let guarded = self.unlock_opened.is_some_and(|at| self.frames <= at + 1);
                if out.return_to_game && !guarded {
                    // `main_menu off`: the menu screen is discarded, which
                    // stops its music; the pause is released.
                    menu.stop(audio);
                    let Phase::Level { menu, .. } = phase else {
                        unreachable!()
                    };
                    *menu = None;
                } else if out.disconnect {
                    // `disconnect`: the game ends; with no level the title
                    // screen's menu opens.
                    diagnostics::end("quit", "quit to the title menu");
                    menu.stop(audio);
                    intro.stop(audio);
                    if let Some(o) = overlay.take() {
                        o.end(audio);
                    }
                    *phase = Phase::Menu(TitleMenu::open(data, now, input, audio, title));
                    self.screens.menu_opened();
                }
            }
            Phase::Level {
                level,
                overlay,
                intro,
                menu,
                inventory,
                pda,
                dropped,
            } => {
                if let Some(o) = overlay.take() {
                    *overlay = o.next_frame(audio);
                    if overlay.is_none() {
                        // The loading screen finishes
                        // (console-diagnostics.md 2.4).
                        let ms = phase_ms(&mut self.phase_started, time_ms);
                        self.phase_started = None;
                        let memory = self.console.load_ended(ms);
                        let open = std::mem::take(&mut self.load_open);
                        (self.load_memory.file_cache).push(file_cache_figure(&open));
                        let held = file_cache::Window {
                            most: file_cache::bytes(),
                            files: file_cache::held(),
                        };
                        self.load_memory.end = Some(LoadEnd {
                            cmem: memory.cmem,
                            crt_heap: memory.crt_heap,
                            textures: memory.textures,
                            game_lua: memory.game_lua,
                            file_cache: held.most,
                            file_cache_budget: file_cache::budget(&held, &open),
                            file_cache_most: file_cache::most(),
                        });
                    }
                }
                let left = overlay.as_ref().map(|o| o.frames_left);
                // While the intro plays, including the frame that ends it,
                // input goes to it alone (our choice; the level is paused
                // under it anyway).
                let playing = intro.player.is_some();
                let frame = intro.frame(data, now, left, quit_presses(data, input), audio);
                if let Some(at) = intro.cursor.take() {
                    input.place_cursor(at);
                }
                match frame {
                    Ok(true) => {
                        level.paused(now);
                        return;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        self.failed.get_or_insert(e.to_string());
                        return;
                    }
                }
                if playing || intro.player.is_some() {
                    // An item without `pause_state`, or the intro's end: the
                    // level runs without this frame's input.
                    level.update(now, &Input::new());
                    // The intro's end makes the controls live: a menu asked
                    // for under the loading image opens now (#96).
                    if intro.player.is_none() && overlay.is_none() && self.menu_asked {
                        self.menu_asked = false;
                        *menu = Some(Box::new(TitleMenu::open(data, now, input, audio, in_game)));
                        self.screens.menu_opened();
                        self.unlock_opened = None;
                    }
                    return;
                }
                // The console opens over the level whose controls are live
                // (our choice: not under the loading image), the inventory
                // screen included (console-window.md section 1).
                let resume = resumed_controls(level.controls());
                let mut below = (self.window).receiver_frame(
                    &mut self.console,
                    input,
                    now,
                    overlay.is_none(),
                    &resume,
                );
                if self.window.is_open() || below.is_some() {
                    self.console_touched = true;
                }
                let input: &mut Input = match below.as_mut() {
                    Some(below) => below,
                    None => input,
                };
                if let Some(screen) = inventory.as_deref_mut().filter(|s| s.is_open()) {
                    // inventory-screen.md 1.3: the game runs on; keys the
                    // screen does not use reach the character.
                    let keys = inventory_keys(data, level.controls());
                    level.controls_mut().keys_only = Some(keys.used());
                    // Measured with once per size drawn at.
                    if self.menu_font.as_ref().map(|(r, _)| *r) != Some(self.video) {
                        match menu_font(tree, data, &mut self.assets, screen, self.video) {
                            Ok(f) => self.menu_font = Some((self.video, f)),
                            Err(e) => {
                                self.failed.get_or_insert(e.to_string());
                                return;
                            }
                        }
                    }
                    let font = self.menu_font.as_ref().map(|(_, f)| f);
                    let time_of_day = level.clock().time_of_day();
                    let out = match level.character_mut() {
                        // An error is fatal, as in the original.
                        Some(character) => screen.update(inventory_screen::Frame {
                            data,
                            character,
                            keys: &keys,
                            time_of_day,
                            now,
                            input,
                            audio,
                            menu_font: font,
                        }),
                        None => {
                            screen.close(data, audio);
                            Ok(Default::default())
                        }
                    };
                    match out {
                        Ok(out) => dropped.extend(out.dropped),
                        Err(e) => {
                            self.failed.get_or_insert(e.to_string());
                            return;
                        }
                    }
                    level.update(now, input);
                    // The screen has the quit key (1.2), and the page holds
                    // no pointer lock while it shows: a back request or an
                    // unlock opens no menu.
                    level.take_back_requested();
                    if !screen.is_open() {
                        level.controls_mut().keys_only = None;
                    }
                    self.after_update(audio);
                    return;
                }
                if let Some(pda) = pda.as_deref_mut().filter(|p| p.is_open()) {
                    // pda-window.md 6: the game runs on; keys the window
                    // does not use reach the character.
                    let keys = pda_keys(data, level.controls());
                    level.controls_mut().keys_only = Some(pda.kept(&keys));
                    let p = level.player();
                    let [x, _, z] = p.state().position;
                    let player = pda_window::map::MapPlayer {
                        level: &level.world().name,
                        position: [x, z],
                        yaw: p.angles().yaw,
                    };
                    pda.update(pda_window::Frame {
                        data,
                        keys: &keys,
                        game_time: level.clock().time(),
                        now,
                        global_ms: self.global_clock.now(),
                        input,
                        paused: self.pause.is_on(),
                        alive: level.character().is_some_and(|c| c.conditions.alive()),
                        player: Some(player),
                    });
                    for line in pda.take_log() {
                        self.console.log_mut().message(&line);
                    }
                    level.controls_mut().keys_only = Some(pda.kept(&keys));
                    // TODO(#738): the character receives them.
                    pda.take_portions();
                    level.update(now, input);
                    // The window has the quit key (5), and the page holds
                    // no pointer lock while it shows: a back request or an
                    // unlock opens no menu.
                    level.take_back_requested();
                    if !pda.is_open() {
                        level.controls_mut().keys_only = None;
                    }
                    self.after_update(audio);
                    return;
                }
                // The level takes key presses while it runs with the
                // player's controls live, the pause off and no window open
                // (console-key-bindings.md section 5, rule 3; under the
                // loading image the game is not running yet, our reading).
                if overlay.is_none() && !self.pause.is_on() {
                    let alive = level.character().is_some_and(|c| c.conditions.alive());
                    self.console_touched |= run_bound_lines(&mut self.console, input, alive);
                }
                level.update(now, input);
                // Pointer lock lost while the controls are live opens the menu
                // too (our choice). The browser takes Escape to leave pointer
                // lock: documented by the Pointer Lock specification (the
                // user agent's exit gesture), believed for Chrome, so the key
                // need not reach the game.
                let unlocked = input.pointer_unlocked();
                let asked =
                    level.take_back_requested() || quit_key_pressed(data, input) || unlocked;
                // The menu opens while the player's controls are live, not
                // under the loading image (our choice). Asked for under the
                // image, it is not dropped: it opens at the first frame the
                // controls are live, after the intro if one plays (our
                // design, #96; presses during the intro skip it instead).
                if overlay.is_some() {
                    self.menu_asked |= asked;
                } else if asked || std::mem::take(&mut self.menu_asked) {
                    *menu = Some(Box::new(TitleMenu::open(data, now, input, audio, in_game)));
                    self.screens.menu_opened();
                    self.unlock_opened = unlocked.then_some(self.frames);
                } else if let Some(screen) = inventory.as_deref_mut()
                    && level.controls().inventory_pressed(input)
                    && let Some(character) = level.character()
                    && character.conditions.alive()
                {
                    // inventory-screen.md 1.1: the level runs, the game is
                    // not paused, the character is alive, no other window
                    // is open.
                    let time_of_day = level.clock().time_of_day();
                    match screen.open(data, character, time_of_day, now, input.cursor_ui(), audio) {
                        Ok(()) => {
                            let keys = inventory_keys(data, level.controls());
                            level.controls_mut().keys_only = Some(keys.used());
                        }
                        // An error is fatal, as in the original.
                        Err(e) => {
                            self.failed.get_or_insert(e.to_string());
                        }
                    }
                } else if let Some(pda) = pda.as_deref_mut()
                    && !self.pause.is_on()
                    && level.character().is_some_and(|c| c.conditions.alive())
                    && let Some(tab) = level.controls().pda_pressed(input)
                {
                    // pda-window.md 4.1: not paused, the character alive,
                    // no other window open: switch to the action's tab,
                    // then open.
                    pda.act(tab, input.cursor_ui());
                    let keys = pda_keys(data, level.controls());
                    level.controls_mut().keys_only = Some(pda.kept(&keys));
                    // TODO(#738): the character receives them.
                    pda.take_portions();
                }
            }
        }
        self.after_update(audio);
    }

    /// The page was hidden (`hidden`) or shown again. Our design (#93),
    /// not the original's: hidden pages are throttled or suspended by every
    /// browser, so while hidden the engine stops. No update runs, the
    /// engine clock (and with it game time, movement and movies) stands
    /// still, and on return it takes on from where it stopped. Hiding opens
    /// the menu in a level whose controls are live, as a lost pointer lock
    /// does, with the same guard against an Escape closing it at once; the
    /// menu or a screen of it already open stays as it is, and nothing opens
    /// during the loading or the intro. The platform suspends its audio
    /// output and idles its frame loop meanwhile.
    pub fn set_hidden(&mut self, hidden: bool, input: &Input, audio: &mut dyn AudioOut) {
        if hidden == self.hidden {
            return;
        }
        self.hidden = hidden;
        if !hidden {
            self.shown = true;
            // Regaining the input focus (sound-weather-ambient.md section
            // 3.1): off unless it was on when the focus was lost.
            self.pause.focus(true);
            return;
        }
        let in_game = self.opening(true);
        if let Some(Phase::Level {
            overlay: None,
            intro,
            menu: menu @ None,
            ..
        }) = &mut self.phase
            && intro.player.is_none()
        {
            *menu = Some(Box::new(TitleMenu::open(
                &self.data, self.clock, input, audio, in_game,
            )));
            self.screens.menu_opened();
            // The first two updates after the return, like the update a
            // lost pointer lock opened the menu in and the next.
            self.unlock_opened = Some(self.frames + 1);
        }
        // Hiding is our platform's loss of the input focus, which turns the
        // pause on (sound-weather-ambient.md section 3.1); after the menu
        // it opens, so the menu remembers the pause off and the return
        // leaves it on until the menu closes.
        self.sync_pause();
        self.pause.focus(false);
    }

    /// Whether the page is hidden ([`App::set_hidden`]).
    pub fn hidden(&self) -> bool {
        self.hidden
    }

    /// What the settings changed this frame do: the bindings and movement
    /// settings the next level uses, and the sound volumes at once.
    fn after_update(&mut self, audio: &mut dyn AudioOut) {
        if std::mem::take(&mut self.console_touched) {
            let generation = self.console.generation();
            if generation != self.applied {
                self.data.apply(&self.console);
                self.applied = generation;
                // Settings changed in the menu during a game reach the
                // running level: mouse_sens, mouse_invert, g_always_run and
                // the bindings.
                if let Some(Phase::Level { level, .. }) = &mut self.phase {
                    level.set_controls(&self.data.controls);
                }
            }
        }
        let volumes = [
            self.console.number("snd_volume_eff"),
            self.console.number("snd_volume_music"),
        ];
        if self.volumes != Some(volumes) {
            audio.set_volume(Category::Effects, volumes[0]);
            audio.set_volume(Category::Music, volumes[1]);
            self.volumes = Some(volumes);
        }
    }

    /// The screen's own size in pixels changed (the resolution list offers
    /// it, section 9).
    pub fn resized(&mut self, native: Resolution) {
        self.native = native;
    }

    /// The resolution to draw at: the video mode the last video restart
    /// applied (#49: shown scaled to fit the screen, aspect kept).
    pub fn video_mode(&self) -> Resolution {
        self.video
    }

    /// Sets `vid_mode` to `mode` and applies it, as a video restart would
    /// (tests and measurements: the frame's cost at a given size).
    /// Sets `r__supersample` to `value` and applies it, as a video restart
    /// would (tests).
    pub fn set_supersample(&mut self, value: u32) {
        self.console.exec(&format!("r__supersample {value}"));
        self.supersample = supersample(&self.console);
    }

    pub fn set_video_mode(&mut self, mode: Resolution) {
        self.console
            .exec(&format!("vid_mode {}x{}", mode.width, mode.height));
        self.video = video_mode(&self.console);
        if let Some(Phase::Menu(menu)) = &mut self.phase {
            menu.resized(&self.data, self.video);
        }
    }

    /// Gives the platform's release of its graphics resources (the
    /// renderer's), which a normal exit runs before the shutdown's lines
    /// (console-diagnostics.md 2.5): the original destroys its graphics
    /// device first, so the lines count what was left after it.
    pub fn set_device_release(&mut self, release: Box<dyn FnMut()>) {
        self.device_release = Some(release);
    }

    /// At a normal exit: the graphics resources released, then the
    /// resource manager's lines (console-diagnostics.md 2.5).
    pub(crate) fn device_destroyed(&mut self) {
        device_destroyed(&mut self.device_release, &mut self.console);
    }

    /// The user configuration file to store (section 12.3), once asked for:
    /// when the options screen closes and when the player quits.
    pub fn take_save(&mut self) -> Option<Vec<u8>> {
        self.save.take()
    }

    /// The log file's contents (console-log.md 3.2), once written: as the
    /// console is set up, by `flush` and `clear_log`, when the player quits
    /// and at a fatal error (3.3), for the platform to store.
    pub fn take_log_file(&mut self) -> Option<Vec<u8>> {
        self.console.log_mut().take_file()
    }

    /// A write of the log file at `path` (its full path in lower case)
    /// could not open it, for `error` (console-log.md 3.9): the log says
    /// so, and the game goes on.
    pub fn log_file_not_written(&mut self, path: &str, error: &str) {
        self.console.log_mut().cannot_write(path, error);
    }

    /// The page is in the browser's full screen (`on`) or not, or can't be
    /// (`supported` false). `rs_fullscreen` is the page's full screen
    /// (a deviation decided by the user, docs/architecture/web-platform.md):
    /// it follows the page, and a video restart asks the page for it
    /// ([`App::take_full_screen_request`]). Where the page can't be, the
    /// options screen's check box is disabled and the value kept.
    pub fn set_full_screen(&mut self, on: bool, supported: bool) {
        self.screens.set_full_screen_supported(supported);
        self.page_full_screen = supported.then_some(on);
        if supported && self.console.on("rs_fullscreen") != on {
            let value = if on { "on" } else { "off" };
            self.console.exec(&format!("rs_fullscreen {value}"));
        }
    }

    /// Full screen to enter (true) or leave, once asked by a video restart.
    pub fn take_full_screen_request(&mut self) -> Option<bool> {
        self.full_screen_request.take()
    }

    /// `texture_lod` and its list as a texture loaded now uses them
    /// (texture-quality-filtering.md 1).
    pub fn texture_quality(&self) -> TextureQuality {
        let lod = self.console.number("texture_lod").max(0.0) as u32;
        TextureQuality::new(lod, self.texture_list.clone())
    }

    /// Executes a console command, as a program's line (tests); an action
    /// runs at the next update.
    pub fn exec(&mut self, line: &str) {
        self.console.exec(line);
    }

    /// The latest load's memory figures.
    pub fn load_memory(&self) -> &LoadMemory {
        &self.load_memory
    }

    /// The console's settings.
    pub fn console(&self) -> &Console {
        &self.console
    }

    /// The frame at the current clock for `resolution`. An error is fatal
    /// (as ui-layout.md 6.3 makes a missing texture): the next update ends
    /// the game ([`App::fatal`]); except what this engine does not support
    /// (`ErrorKind::Unsupported`: AVI, a DDS or Theora layout our decoders
    /// lack), which the original plays, so it is returned, not fatal.
    // TODO(#339): a storage read failing while drawing (a platform failure,
    // not a missing file) is fatal too; no spec covers it.
    pub fn scene(&mut self, tree: &dyn Vfs, resolution: Resolution) -> io::Result<Scene> {
        let mut scene = self.compose_scene(tree, resolution);
        // display-gamma.md: the ramp of the display colour settings acts
        // on everything presented.
        let colour = DisplayColour::of(&self.console);
        if self.ramp.is_none_or(|(c, _)| c != colour) {
            self.ramp = Some((colour, colour.ramp()));
        }
        if let Ok(scene) = &mut scene {
            frame_settings(
                scene,
                &self.console,
                self.supersample,
                &self.samplers,
                self.texture_quality(),
                self.frame_ms,
            );
            scene.ramp = self.ramp.and_then(|(_, r)| r);
        }
        // console-window.md 4.7: after everything else, before the
        // statistics; at half height over a level or the main menu (4.1).
        if let Ok(scene) = &mut scene {
            let half = matches!(self.phase, Some(Phase::Level { .. }) | Some(Phase::Menu(_)));
            let drawn = (self.window).draw(
                tree,
                &self.data,
                &mut self.assets,
                &self.console,
                scene,
                half,
            );
            if let Err(e) = drawn {
                self.failed.get_or_insert(e.to_string());
            }
        }
        // display-mode.md B.1: over everything of the frame, measured only
        // while on.
        if self.console.on("rs_stats")
            && let Ok(scene) = &mut scene
        {
            let camera = match &self.phase {
                Some(Phase::Level { level, .. }) => level.state().eye,
                _ => [0.0; 3],
            };
            self.stats.frame(self.frame_ms as f64 / 1000.0);
            if let Err(e) = (self.stats).draw(tree, &self.data, &mut self.assets, scene, camera) {
                self.failed.get_or_insert(e.to_string());
            }
        }
        if let Err(e) = &scene
            && e.kind() != io::ErrorKind::Unsupported
        {
            self.failed.get_or_insert(e.to_string());
        }
        scene
    }

    /// The platform's measurements of the frame just drawn, for the
    /// statistics overlay (display-mode.md B.4.1).
    pub fn set_frame_times(&mut self, times: FrameTimes) {
        self.stats.measured(times);
    }

    fn compose_scene(&mut self, tree: &dyn Vfs, resolution: Resolution) -> io::Result<Scene> {
        let mut ctx = Ctx {
            tree,
            data: &self.data,
            assets: &mut self.assets,
            resolution,
            surface: crate::draw::Surface::screen(resolution),
            clock_ms: self.clock,
        };
        let menu_surface = main_menu_surface(resolution, self.supersample);
        match &self.phase {
            None => Ok(Scene::new(resolution)),
            Some(Phase::Startup(s)) => s.scene(&mut ctx),
            Some(Phase::Menu(m)) => {
                if self.screens.sub().is_none() {
                    ctx.surface = menu_surface;
                }
                self.screens.scene(m, &mut ctx)
            }
            Some(Phase::Loading {
                stages: NewGame::Closing(menu),
                ..
            }) => {
                ctx.surface = menu_surface;
                menu.scene(&mut ctx)
            }
            Some(Phase::Loading {
                stages: NewGame::Stages(loading),
                ..
            }) => {
                let mut scene = Scene::new(resolution);
                loading.screen().draw(&mut ctx, &mut scene)?;
                Ok(scene)
            }
            Some(Phase::Level {
                level,
                menu: Some(menu),
                ..
            }) => {
                // Nothing of the game is drawn behind the menu; the world
                // stays in the scene, covered, so it stays on the GPU.
                let mut scene = level.covered_scene(resolution, self.clock);
                if self.screens.sub().is_none() {
                    ctx.surface = menu_surface;
                }
                let menu = self.screens.scene(menu, &mut ctx)?;
                let under = scene.sprites.len();
                scene.lens = menu.lens.map(|mut lens| {
                    lens.drawn += under;
                    lens
                });
                scene.sprites.extend(menu.sprites);
                Ok(scene)
            }
            Some(Phase::Level {
                level,
                overlay,
                intro,
                menu: None,
                inventory,
                pda,
                ..
            }) => {
                let mut scene = compose(
                    level.scene(resolution, self.clock),
                    intro,
                    overlay.as_ref(),
                    &mut ctx,
                )?;
                // inventory-screen.md 3.1: over the rendered level.
                if let (Some(screen), Some(character)) = (inventory, level.character()) {
                    screen.draw(&mut ctx, &mut scene, character)?;
                }
                // pda-window.md 2.2: over the rendered level.
                if let Some(pda) = pda {
                    pda.draw(&mut ctx, &mut scene)?;
                }
                Ok(scene)
            }
        }
    }

    /// The texture files of the level shown, if any (tests: transcoding
    /// cost).
    pub fn level_texture_paths(&self) -> Option<Vec<String>> {
        match &self.phase {
            Some(Phase::Level { level, .. }) => Some(level.world().texture_paths()),
            _ => None,
        }
    }

    /// While the player's controls are live (the level shows with no
    /// loading image or intro over it): whether the crouch toggle is engaged
    /// (and in the low crouch), a sprint is requested and the sprint lock
    /// holds, which the touch controls show.
    pub fn level_controls(&self) -> Option<LevelControls> {
        match &self.phase {
            Some(Phase::Level {
                level,
                overlay: None,
                intro,
                menu: None,
                inventory,
                ..
            }) if intro.player.is_none()
                && !inventory.as_ref().is_some_and(|i| i.is_open())
                && !self.pda_open()
                && !self.window.is_open() =>
            {
                let p = level.player().state();
                let (torch, night_vision) = level.torch_buttons();
                Some(LevelControls {
                    crouch: p.crouch_toggled,
                    low_crouch: p.crouch_toggled && p.posture == 2,
                    sprint: p.sprint_requested,
                    sprint_lock: level.controls().sprint_lock.locked(),
                    torch,
                    night_vision,
                })
            }
            _ => None,
        }
    }

    /// Whether the frame shows the game's mouse cursor: the title menu and
    /// its screens, before a game and during one, the inventory screen and
    /// the PDA window (their scenes draw it). Platforms turn touch into a touchpad for the
    /// cursor then (a Volya Engine addition, decided by the user, #108).
    pub fn cursor_shown(&self) -> bool {
        matches!(
            &self.phase,
            Some(Phase::Menu(_))
                | Some(Phase::Loading {
                    stages: NewGame::Closing(_),
                    ..
                })
                | Some(Phase::Level { menu: Some(_), .. })
        ) || self.inventory_open()
            || self.pda_open()
    }

    /// The save screen's name field (UI units) and whether it has focus,
    /// while it shows: a platform puts its text entry over it (the
    /// device's keyboard on touch screens; our design, #94).
    pub fn text_field(&self) -> Option<(volya_engine_formats::ui::layout::Rect, bool)> {
        match &self.phase {
            Some(Phase::Menu(_)) | Some(Phase::Level { menu: Some(_), .. }) => {
                self.screens.text_field()
            }
            _ => None,
        }
    }

    /// Whether the player character takes keyboard keys: its controls are
    /// live, or the inventory screen or the PDA window over the running
    /// level passes on the keys it doesn't use (inventory-screen.md 1.3,
    /// pda-window.md 6). Platforms forward
    /// keys pressed with Control then (it crouches).
    pub fn character_takes_keys(&self) -> bool {
        self.level_controls().is_some()
            || !self.window.is_open()
                && (matches!(&self.phase, Some(Phase::Level { inventory: Some(i), menu: None, .. }) if i.is_open())
                    || self.pda_over_level())
    }

    /// Whether the console is open (docs/behavior/console-window.md): it
    /// takes every key, and the platform reads the clipboard for an Insert
    /// press ([`Input::clipboard_read`]).
    pub fn console_open(&self) -> bool {
        self.window.is_open()
    }

    /// Whether a key bound to `console` would open the console over the
    /// main menu now: the menu has input (after the logo sequence, or over
    /// a level) and the console is closed (console-window.md section 1).
    /// The touch screen's Console button shows over the menu then (#642);
    /// in a level it shows with the level's controls.
    pub fn console_opens_over_menu(&self) -> bool {
        !self.window.is_open()
            && matches!(
                &self.phase,
                Some(Phase::Menu(_) | Phase::Level { menu: Some(_), .. })
            )
    }

    /// Whether the PDA window is open (pda-window.md).
    pub fn pda_open(&self) -> bool {
        matches!(&self.phase, Some(Phase::Level { pda: Some(p), .. }) if p.is_open())
    }

    /// Whether the PDA window is open over the running level with nothing
    /// over it (no menu, no console): the touch PDA button shows over it to
    /// close it (our design, #730).
    pub fn pda_over_level(&self) -> bool {
        !self.window.is_open()
            && matches!(&self.phase, Some(Phase::Level { pda: Some(p), menu: None, .. }) if p.is_open())
    }

    /// Whether the inventory screen is open.
    pub fn inventory_open(&self) -> bool {
        matches!(&self.phase, Some(Phase::Level { inventory: Some(i), .. }) if i.is_open())
    }

    /// Test interface: puts an item of section `section` into the player's
    /// inventory, taken by the rules of player-state.md 3.3, with an id
    /// from the level (the simulation's pool), held by the character only:
    /// the game takes items from the world, which does not exist yet; this
    /// op is not a game action. Returns its id.
    pub fn give_item(&mut self, section: &str) -> io::Result<u16> {
        use crate::core::character::inventory::{InventoryItem, ItemConfig};
        let Some(Phase::Level { level, .. }) = &mut self.phase else {
            return Err(io::Error::other("no level"));
        };
        let config = ItemConfig::read(&self.data.system, section)?;
        // A detector reads its section as it is made (sound-detector.md 2.1).
        if crate::detector::is_detector(config.class_id.as_str()) {
            let detector =
                crate::core::detector::DetectorSection::read(&self.data.system, section)?;
            level.add_detector_section(detector);
        }
        if level.character().is_none() {
            return Err(io::Error::other("no player character"));
        }
        let id = level.take_item_id()?;
        let character = level
            .character_mut()
            .ok_or_else(|| io::Error::other("no player character"))?;
        let item = InventoryItem::create(
            config,
            id,
            None,
            character.owner.id,
            &character.profile.specific_character,
        );
        character.inventory.take(item);
        Ok(id)
    }

    /// Test interface: a hit on the player character (player-hits.md 2) of
    /// the type named `kind` with power `power`, on the player's model's bone
    /// `bone` (no bone without one), with armour piercing `armour_piercing`.
    /// Nothing in the game hits the player yet: zones' contact is #350,
    /// falls and radiation zones PS-2 (#246).
    pub fn hit_player(
        &mut self,
        kind: &str,
        power: f32,
        bone: Option<&str>,
        armour_piercing: f32,
    ) -> io::Result<()> {
        use crate::core::character::hits::{Hit, HitType};
        let Some(Phase::Level { level, .. }) = &mut self.phase else {
            return Err(io::Error::other("no level"));
        };
        let character = level
            .character()
            .ok_or_else(|| io::Error::other("no player character"))?;
        let bone = match bone {
            Some(name) => Some(
                character
                    .bone(name)
                    .ok_or_else(|| io::Error::other(format!("the model has no bone {name}")))?,
            ),
            None => None,
        };
        level.hit(Hit {
            kind: HitType::named(kind)?,
            power,
            bone,
            armour_piercing,
        })
    }

    /// Test interface: `wear` puts on an outfit with night vision (the
    /// first section of the configuration that is an outfit with a
    /// `nightvision_sect`, torch.md 15.1), given as [`App::give_item`] and
    /// moved to its slot; otherwise the worn outfit, if it has night
    /// vision, leaves the inventory. Returns the outfit's section.
    pub fn nightvision_outfit(&mut self, wear: bool) -> io::Result<Option<String>> {
        use crate::core::character::inventory::ItemConfig;
        use volya_engine_formats::object::Kind;
        if wear {
            let section = (self.data.system.sections())
                .filter_map(|s| ItemConfig::read(&self.data.system, s.name()).ok())
                .find(|c| c.class_id.kind() == Kind::Outfit && c.nightvision_sect.is_some())
                .map(|c| c.section)
                .ok_or_else(|| io::Error::other("no outfit with night vision"))?;
            let id = self.give_item(&section)?;
            let character = self.character_mut()?;
            if !character.inventory.move_to_slot_with_swap(id).succeeded() {
                return Err(io::Error::other(format!("{section} not worn")));
            }
            return Ok(Some(section));
        }
        let character = self.character_mut()?;
        let worn = (character.inventory.worn_outfit())
            .filter(|i| i.config.nightvision_sect.is_some())
            .map(|i| (i.id, i.config.section.clone()));
        Ok(worn.map(|(id, section)| {
            character.drop_item(id);
            section
        }))
    }

    fn character_mut(&mut self) -> io::Result<&mut crate::core::character::CharacterState> {
        match &mut self.phase {
            Some(Phase::Level { level, .. }) => level.character_mut(),
            _ => None,
        }
        .ok_or_else(|| io::Error::other("no player character"))
    }

    /// The level while one runs.
    pub fn level(&self) -> Option<&LevelScreen> {
        match &self.phase {
            Some(Phase::Level { level, .. }) => Some(level),
            _ => None,
        }
    }

    /// The game ends with this app (it is dropped next): the level's
    /// sounds leave `sounds`, which outlives it, as when the level is left,
    /// and with them what they share (the collision world the occlusion
    /// reads, #451: a game left from its level kept its 28 MB, and the next
    /// game's collision world couldn't reuse its arrays).
    pub fn end_sounds(&mut self, sounds: &mut crate::core::sound::system::SoundSystem) {
        if let Some(level_sounds) = self.level_sounds.take() {
            sounds.stop_all();
            level_sounds.discard(sounds);
        }
        sounds.occlusion_mut().level = None;
        self.sound_in_level = false;
    }

    pub fn quit_requested(&self) -> bool {
        if self.alt_f4 || self.quit {
            return true;
        }
        match &self.phase {
            Some(Phase::Menu(m)) => m.quit_requested(),
            Some(Phase::Level { menu: Some(m), .. }) => m.quit_requested(),
            _ => false,
        }
    }

    /// Stops every sound the app started.
    pub fn stop(&mut self, audio: &mut dyn AudioOut) {
        match &mut self.phase {
            Some(Phase::Startup(s)) => s.stop(audio),
            Some(Phase::Menu(m)) => m.stop(audio),
            Some(Phase::Level {
                overlay,
                intro,
                menu,
                ..
            }) => {
                if let Some(m) = menu {
                    m.stop(audio);
                }
                intro.stop(audio);
                if let Some(o) = overlay.take() {
                    o.end(audio);
                }
            }
            Some(Phase::Loading { .. }) | None => {}
        }
    }

    pub fn state(&self) -> AppState {
        let clock = self.clock;
        match &self.phase {
            None => AppState::Loaded { clock },
            Some(Phase::Startup(s)) => AppState::Startup {
                clock,
                startup: s.state(&self.data, clock),
            },
            Some(Phase::Menu(m)) => {
                let mut menu = self.screens.state(m, &self.data, clock);
                menu.error = self.error.clone().or(menu.error);
                AppState::Menu {
                    clock,
                    menu: Box::new(menu),
                }
            }
            Some(Phase::Loading { difficulty, stages }) => AppState::Loading {
                clock,
                difficulty,
                loading: match stages {
                    NewGame::Closing(_) => None,
                    NewGame::Stages(l) => Some(l.screen().state()),
                },
            },
            Some(Phase::Level {
                level,
                overlay,
                intro,
                menu,
                inventory,
                pda,
                ..
            }) => AppState::Level {
                clock,
                level: Box::new(level.state()),
                load_ms: self.load_ms,
                overlay_frames: overlay.as_ref().map_or(0, |o| o.frames_left),
                menu_asked: self.menu_asked,
                intro: intro.state(&self.data, clock),
                menu: menu
                    .as_ref()
                    .map(|m| Box::new(self.screens.state(m, &self.data, clock))),
                inventory: inventory
                    .as_ref()
                    .filter(|i| i.is_open())
                    .zip(level.character())
                    .map(|(i, c)| Box::new(i.state(c))),
                pda: (pda.as_ref())
                    .filter(|p| p.is_open())
                    .map(|p| Box::new(p.state())),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::input::{Event, TouchPhase};
    use crate::fixture::{self, FakeAudio};

    fn press(input: &mut Input, key: Key) {
        input.handle(Event::Key { key, pressed: true });
        input.handle(Event::Key {
            key,
            pressed: false,
        });
    }

    #[test]
    fn alt_f4_quits_whatever_has_input() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 5000.0, &mut input, &mut audio);
        // A press and release within one frame does not count.
        input.handle(Event::Key {
            key: Key::LeftAlt,
            pressed: true,
        });
        press(&mut input, Key::F4);
        app.update(&tree, 5100.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!app.quit_requested());
        app.take_log_file();
        // Both down after the frame's events: a normal exit, during the
        // logos too.
        input.handle(Event::Key {
            key: Key::F4,
            pressed: true,
        });
        app.update(&tree, 5200.0, &mut input, &mut audio);
        assert!(app.quit_requested());
        assert!(matches!(app.state(), AppState::Startup { .. }));
        assert!(app.take_log_file().is_some());
        assert!(app.take_save().is_some());
    }

    #[test]
    fn startup_then_menu_then_quit() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        assert!(matches!(app.state(), AppState::Loaded { .. }));
        // The clock starts at the first update.
        app.update(&tree, 5000.0, &mut input, &mut audio);
        assert!(matches!(app.state(), AppState::Startup { clock: 0, .. }));
        // Keys other than quit do nothing; a tap counts as quit.
        press(&mut input, Key::Enter);
        app.update(&tree, 5100.0, &mut input, &mut audio);
        input.end_frame();
        let AppState::Startup { startup, .. } = app.state() else {
            panic!()
        };
        assert_eq!(startup.item, 0);
        input.handle(Event::Touch {
            at: [1.0, 1.0],
            phase: TouchPhase::Start,
            time: 0.0,
        });
        app.update(&tree, 5200.0, &mut input, &mut audio);
        input.end_frame();
        let AppState::Startup { startup, .. } = app.state() else {
            panic!()
        };
        assert_eq!((startup.item, startup.skipped), (1, 1));
        // No cursor during the logos: touch stays direct (#108).
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert!(!app.cursor_shown() && scene.first("cursor").is_none());
        // Escape ends the last item: the menu shows in the same frame.
        press(&mut input, Key::Escape);
        app.update(&tree, 5300.0, &mut input, &mut audio);
        input.end_frame();
        let AppState::Menu { clock, menu, .. } = app.state() else {
            panic!()
        };
        assert_eq!(
            (clock, menu.menu.list, menu.menu.music.starts),
            (300, "menu_main", 1)
        );
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert!(scene.first("background").is_some());
        // The menu draws its cursor: touch is a touchpad (#108).
        assert!(app.cursor_shown() && scene.first("cursor").is_some());
        app.take_log_file();
        press(&mut input, Key::Q);
        app.update(&tree, 5400.0, &mut input, &mut audio);
        assert!(app.quit_requested());
        assert!(audio.playing.is_empty());
        // The log file is written at the end (console-log.md 3.3).
        assert!(app.take_log_file().is_some());
    }

    #[test]
    fn the_display_colour_settings_give_every_frame_its_ramp() {
        // display-gamma.md 3.1, 3.2: the stored values from the first frame;
        // a variable executed later at once.
        let tree = fixture::tree();
        let mut data = fixture::data(&tree);
        data.set_user_config(b"rs_c_brightness 1.5\r\n".to_vec());
        let mut app = App::new(data).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 5000.0, &mut input, &mut audio);
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert_eq!(scene.ramp.map(|r| [r[0], r[128]]), Some([8192, 41088]));
        app.console.exec("rs_c_brightness 1");
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert_eq!(scene.ramp, None);
        app.console.exec("rs_c_gamma 0.5");
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert_eq!(scene.ramp.map(|r| r[64]), Some(4128));
    }

    #[test]
    fn settings_apply_volumes_and_options_open_from_the_menu() {
        let tree = fixture::tree();
        let mut data = fixture::data(&tree);
        data.set_user_config(
            b"snd_volume_music 0.5\r\nvid_mode 800x600\r\nbind forward kT\r\n".to_vec(),
        );
        assert_eq!(
            data.controls
                .bindings
                .controls(crate::controls::MoveAction::Forward)
                .collect::<Vec<_>>(),
            [Control::Key(Key::T)]
        );
        let mut app = App::new(data).unwrap();
        app.assets = fixture::assets();
        assert_eq!(app.video_mode(), Resolution::new(800, 600));
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 0.0, &mut input, &mut audio);
        // Section 6.4: the volumes at once (built-in effects volume 1).
        assert_eq!(
            audio.volumes,
            [(Category::Effects, 1.0), (Category::Music, 0.5)]
        );
        for _ in 0..5 {
            press(&mut input, Key::Escape);
            app.update(&tree, 10.0, &mut input, &mut audio);
            input.end_frame();
        }
        // Options (item 3): the fixture has no options layout, so the menu
        // stays, saying why.
        for _ in 0..3 {
            press(&mut input, Key::Down);
            app.update(&tree, 20.0, &mut input, &mut audio);
            input.end_frame();
        }
        press(&mut input, Key::Enter);
        app.update(&tree, 30.0, &mut input, &mut audio);
        input.end_frame();
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("not in the menu")
        };
        assert!(menu.options.is_none());
        assert!(menu.error.unwrap().contains("ui_mm_opt.xml"));
        // Quitting stores the settings.
        press(&mut input, Key::Q);
        app.update(&tree, 40.0, &mut input, &mut audio);
        let saved = String::from_utf8(app.take_save().unwrap()).unwrap();
        assert!(
            saved.contains("snd_volume_music 0.5\r\n") && saved.contains("bind forward kT\r\n")
        );
    }

    #[test]
    fn a_difficulty_loads_the_new_game_or_returns_saying_why() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 0.0, &mut input, &mut audio);
        for _ in 0..5 {
            press(&mut input, Key::Escape);
            app.update(&tree, 10.0, &mut input, &mut audio);
            input.end_frame();
        }
        assert!(matches!(app.state(), AppState::Menu { .. }));
        // New game, then the first difficulty.
        for t in [20.0, 30.0] {
            press(&mut input, Key::Enter);
            app.update(&tree, t, &mut input, &mut audio);
            input.end_frame();
        }
        let AppState::Loading {
            difficulty,
            loading: None,
            ..
        } = app.state()
        else {
            panic!("not closing the menu: {:?}", app.state())
        };
        assert_eq!(difficulty, "gd_novice");
        assert!(audio.playing.is_empty(), "the menu music stopped");
        // The last menu frame stays until the first caption begins.
        let res = Resolution::new(1024, 768);
        assert!(app.scene(&tree, res).unwrap().first("background").is_some());

        // One caption per frame: a first New Game starts with prefetching.
        let caption = |app: &App| match app.state() {
            AppState::Loading {
                loading: Some(l), ..
            } => (l.captions, l.caption),
            s => panic!("not loading: {s:?}"),
        };
        app.update(&tree, 40.0, &mut input, &mut audio);
        assert_eq!(caption(&app), (1, "Кэширование объектов...".into()));
        // console-diagnostics.md 2.4: each caption's phase lines.
        let last = |app: &App| {
            app.console()
                .log()
                .iter()
                .rev()
                .take(3)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            last(&app),
            [
                "* phase time: 0 ms",
                "* phase cmem: 0 K",
                "Кэширование объектов..."
            ]
        );
        let scene = app.scene(&tree, res).unwrap();
        assert!(scene.first("background").is_none(), "menu gone");
        assert_eq!(scene.layer("loading strip").count(), 40);
        app.update(&tree, 50.0, &mut input, &mut audio);
        assert_eq!(caption(&app), (2, "Сервер: Старт...".into()));
        assert_eq!(
            last(&app),
            [
                "* phase time: 10 ms",
                "* phase cmem: 0 K",
                "Сервер: Старт..."
            ]
        );
        // Each caption's figure kept by its id, for the suites' comparison
        // with the original's (docs/architecture/memory.md).
        assert_eq!(
            app.load_memory().captions,
            [("st_prefetching_objects", 0), ("st_server_starting", 0)]
        );
        app.update(&tree, 60.0, &mut input, &mut audio);
        app.update(&tree, 70.0, &mut input, &mut audio);
        assert_eq!(caption(&app), (4, "st_creating_new_game...".into()));
        // The fixture has no spawn file: back to the menu, saying why.
        app.update(&tree, 80.0, &mut input, &mut audio);
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("not in the menu: {:?}", app.state())
        };
        assert!(menu.error.clone().unwrap().contains("spawn"), "error");
        assert_eq!(menu.menu.list, "menu_main");
        assert!(audio.master_gains.is_empty(), "never muted");

        // A later New Game of the same type skips prefetching.
        for t in [90.0, 100.0, 110.0] {
            press(&mut input, Key::Enter);
            app.update(&tree, t, &mut input, &mut audio);
            input.end_frame();
        }
        assert_eq!(caption(&app), (1, "Сервер: Старт...".into()));
    }

    #[test]
    fn the_intro_starts_under_the_last_two_overlay_frames_once() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut audio = FakeAudio::default();
        let mut intro = Intro::default();
        let mut started = None;
        for (frame, left) in (1..OVERLAY_FRAMES).rev().enumerate() {
            let paused = intro
                .frame(&data, frame as u64 * 16, Some(left), 0, &mut audio)
                .unwrap();
            if started.is_none() && intro.player.is_some() {
                started = Some(left);
            }
            // `pause_state on`: game time paused from its first frame.
            assert_eq!(paused, intro.player.is_some(), "{left} left");
        }
        assert_eq!(started, Some(2));
        // Its sound pair starts with the video (no delay), under the
        // muting the overlay set.
        let files: Vec<&str> = audio.started.iter().map(|(_, f, _)| f.as_str()).collect();
        assert_eq!(
            files,
            [
                "gamedata\\sounds\\v\\intro_l.ogg",
                "gamedata\\sounds\\v\\intro_r.ogg"
            ]
        );
        // After the overlay: plays its 3 s, then the level resumes; it is
        // not started again.
        let start = u64::from(OVERLAY_FRAMES - 3) * 16;
        assert!(
            intro
                .frame(&data, start + 2999, None, 0, &mut audio)
                .unwrap()
        );
        assert!(
            !intro
                .frame(&data, start + 3000, None, 0, &mut audio)
                .unwrap()
        );
        assert!(intro.player.is_none());
        assert!(audio.playing.is_empty(), "its sounds ended");
        assert!(!intro.frame(&data, 4000, Some(1), 0, &mut audio).unwrap());
        assert!(intro.player.is_none());
    }

    #[test]
    fn a_level_frame_is_level_then_intro_then_loading_image() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut audio = FakeAudio::default();
        let mut intro = Intro::default();
        let overlay = Overlay::new(LoadingScreen::default(), &mut audio);
        let mut assets = fixture::assets();
        let resolution = Resolution::new(1024, 768);
        let mut ctx = Ctx {
            tree: &tree,
            data: &data,
            assets: &mut assets,
            resolution,
            surface: crate::draw::Surface::screen(resolution),
            clock_ms: 16,
        };
        let level = || {
            let mut s = Scene::new(resolution);
            s.sprites
                .push(splash_scene(resolution, (1, 1)).sprites[0].clone());
            s.sprites[0].layer = "level".into();
            s
        };
        let layers = |s: &Scene| {
            let mut l: Vec<String> = s.sprites.iter().map(|s| s.layer.clone()).collect();
            l.dedup();
            l
        };
        let s = compose(level(), &intro, Some(&overlay), &mut ctx).unwrap();
        assert_eq!(layers(&s)[..2], ["level", "loading background"]);
        // The intro starts under the last two overlay frames.
        intro.frame(&data, 0, Some(2), 0, &mut audio).unwrap();
        let s = compose(level(), &intro, Some(&overlay), &mut ctx).unwrap();
        assert_eq!(
            layers(&s)[..5],
            [
                "level",
                "background",
                "video",
                "loading background",
                "loading strip"
            ]
        );
        let s = compose(level(), &intro, None, &mut ctx).unwrap();
        assert_eq!(layers(&s), ["level", "background", "video"]);
    }

    #[test]
    fn the_intro_draws_over_the_level_and_a_quit_press_skips_it() {
        let tree = fixture::tree();
        let data = fixture::data(&tree);
        let mut audio = FakeAudio::default();
        let mut intro = Intro::default();
        assert!(intro.frame(&data, 0, Some(2), 0, &mut audio).unwrap());
        let mut assets = fixture::assets();
        let mut ctx = Ctx {
            tree: &tree,
            data: &data,
            assets: &mut assets,
            resolution: Resolution::new(1024, 768),
            surface: crate::draw::Surface::screen(Resolution::new(1024, 768)),
            clock_ms: 16,
        };
        let mut scene = Scene::new(ctx.resolution);
        intro.draw(&mut ctx, &mut scene).unwrap();
        let layers: Vec<&str> = scene.sprites.iter().map(|s| s.layer.as_str()).collect();
        assert_eq!(layers, ["background", "video"]);
        assert_eq!(
            scene.first("video").unwrap().texture,
            TextureId::Video {
                path: "gamedata\\textures\\intro\\intro_stalker.ogm".into(),
                start_ms: 0,
                looping: false
            }
        );
        assert_eq!(intro.state(&data, 16).unwrap().items, 1);
        // Escape (a gamepad's east button too) or a touch: one press ends
        // the only item, its sounds stop and game time runs again.
        let mut input = Input::new();
        input.handle(Event::GamepadButton {
            button: crate::core::input::GamepadButton::East,
            pressed: true,
        });
        assert_eq!(quit_presses(&data, &input), 1);
        assert!(!intro.frame(&data, 32, Some(1), 1, &mut audio).unwrap());
        assert!(intro.player.is_none());
        assert!(audio.playing.is_empty());
        let mut input = Input::new();
        press(&mut input, Key::Escape);
        assert_eq!(quit_presses(&data, &input), 1);
        let mut input = Input::new();
        input.handle(Event::Touch {
            at: [1.0, 1.0],
            phase: TouchPhase::Start,
            time: 0.0,
        });
        assert_eq!(quit_presses(&data, &input), 1);
        let mut input = Input::new();
        press(&mut input, Key::Enter);
        assert_eq!(quit_presses(&data, &input), 0, "other keys do nothing");
    }

    #[test]
    fn the_overlay_mutes_for_thirty_frames_or_until_left() {
        let mut audio = FakeAudio::default();
        let mut overlay = Some(Overlay::new(LoadingScreen::default(), &mut audio));
        let mut frames = 0;
        while let Some(o) = overlay {
            frames += 1;
            assert_eq!(audio.master_gains, [0.0], "muted on frame {frames}");
            overlay = o.next_frame(&mut audio);
        }
        assert_eq!(frames, 30);
        assert_eq!(audio.master_gains, [0.0, 1.0]);
        // Leaving the level during the overlay un-mutes at once.
        let mut audio = FakeAudio::default();
        let o = Overlay::new(LoadingScreen::default(), &mut audio);
        o.next_frame(&mut audio).unwrap().end(&mut audio);
        assert_eq!(audio.master_gains, [0.0, 1.0]);
    }

    /// An app in an empty level whose controls are live.
    fn in_level() -> (fixture::MemVfs, App, Input, FakeAudio) {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 0.0, &mut input, &mut audio);
        let controls = app.data.controls.clone();
        app.phase = Some(Phase::Level {
            level: Box::new(LevelScreen::empty(controls, 0)),
            overlay: None,
            intro: Intro::default(),
            menu: None,
            inventory: None,
            pda: None,
            dropped: Vec::new(),
        });
        app.update(&tree, 16.0, &mut input, &mut audio);
        assert!(app.level_controls().is_some(), "the controls are live");
        assert!(!app.cursor_shown(), "no cursor in the level");
        (tree, app, input, audio)
    }

    /// r1-settings-scope.md 2.2: `rs_vis_distance` scales the far plane
    /// from the next frame (start-position.md 7.1; the empty level's
    /// weather has 100 m).
    #[test]
    fn the_view_distance_scales_the_far_plane() {
        let (tree, mut app, mut input, mut audio) = in_level();
        let far = |app: &App| match &app.phase {
            Some(Phase::Level { level, .. }) => level.far_plane(),
            _ => panic!("not in a level"),
        };
        assert_eq!(far(&app), 100.0);
        app.console.exec("rs_vis_distance 0.5");
        app.update(&tree, 32.0, &mut input, &mut audio);
        assert_eq!(far(&app), 50.0);
    }

    /// #566: the camera pass before a level's frame is drawn takes the
    /// frame's size, the video mode, not a size of its own: its size
    /// thresholds (object-detail-level.md 1) are the 1920 × 1080 frame's
    /// from the first update after a video restart to it.
    #[test]
    fn the_camera_pass_takes_the_video_mode() {
        let (tree, mut app, mut input, mut audio) = in_level();
        app.set_supersample(1);
        app.set_video_mode(Resolution::new(1920, 1080));
        app.update(&tree, 32.0, &mut input, &mut audio);
        let Some(Phase::Level { level, .. }) = &app.phase else {
            panic!("not in a level");
        };
        let [lod, a, b] = geometry_lod(&app.console);
        let fov = crate::core::camera::FOV_Y_DEGREES;
        let expected = volya_engine_formats::level::Thresholds::new(1920, 1080, fov, lod, a, b);
        assert_eq!(level.visibility().thresholds(), Some(&expected));
    }

    /// `r1_lmodel_lerp`: 0.1 built in, the stored value; one out of
    /// 0 … 0.333 is rejected.
    #[test]
    fn r1_lmodel_lerp_is_read() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        assert_eq!(lmodel_lerp(&app.console), 0.1);
        app.console.exec("r1_lmodel_lerp 0.25");
        assert_eq!(lmodel_lerp(&app.console), 0.25);
        app.console.exec("r1_lmodel_lerp 1");
        assert_eq!(lmodel_lerp(&app.console), 0.25);
    }

    /// `rs_fullscreen` is the page's full screen (our deviation): it
    /// follows the page, a video restart asks for it where it differs, and
    /// where the page can't be in full screen it is kept and nothing asked.
    #[test]
    fn rs_fullscreen_is_the_pages_full_screen() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.set_full_screen(false, true);
        assert!(!app.console.on("rs_fullscreen"));
        app.console.exec("rs_fullscreen on");
        let request = |app: &App| display_mode_request(&app.console, app.page_full_screen);
        assert_eq!(request(&app), Some(true));
        app.set_full_screen(true, true);
        assert_eq!(request(&app), None);
        app.set_full_screen(false, false);
        assert!(app.console.on("rs_fullscreen"), "kept");
        assert_eq!(request(&app), None);
    }

    fn menu_open(app: &App) -> bool {
        matches!(app.state(), AppState::Level { menu: Some(_), .. })
    }

    /// An app in an empty level with a player character holding bread and
    /// an inventory screen, its controls bound as the game's defaults are
    /// (`inventory` I, `forward` W, `wpn_fire` the left button).
    fn in_level_with_inventory() -> (fixture::MemVfs, App, Input, FakeAudio) {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 0.0, &mut input, &mut audio);
        let mut controls = app.data.controls.clone();
        controls.bindings = crate::controls::Bindings::read(&[
            b"bind inventory kI\nbind forward kW\nbind wpn_fire mouse1\nbind drop kG\n\
              bind active_jobs kP\nbind map kM\nbind contacts kH\n",
        ]);
        let character = crate::inventory_screen::tests::character(&[("bread", 5, 3)]);
        let xga = Resolution::new(1024, 768);
        let inventory = InventoryScreen::load(&tree, &app.data, xga).unwrap();
        let fonts = app.assets.fonts(&tree, &app.data, xga.height).unwrap();
        let pda = PdaWindow::load(&tree, &app.data, xga, fonts).unwrap();
        app.phase = Some(Phase::Level {
            level: Box::new(LevelScreen::empty(controls, 0).with_character(character)),
            overlay: None,
            intro: Intro::default(),
            menu: None,
            inventory: Some(Box::new(inventory)),
            pda: Some(Box::new(pda)),
            dropped: Vec::new(),
        });
        app.update(&tree, 16.0, &mut input, &mut audio);
        (tree, app, input, audio)
    }

    /// How many log lines are `line`.
    fn logged(app: &App, line: &str) -> usize {
        app.console().log().iter().filter(|l| *l == line).count()
    }

    /// Console actions (console_actions.rs): a bound key's `main_menu`
    /// opens the in-game menu, `main_menu off` closes it, `disconnect`
    /// leaves the game for the title menu.
    #[test]
    fn a_bound_line_opens_the_menu_and_disconnect_leaves_the_game() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        app.exec("bind_console main_menu kK");
        press(&mut input, Key::K);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app));
        app.exec("main_menu off");
        app.update(&tree, 48.0, &mut input, &mut audio);
        assert!(!menu_open(&app));
        app.exec("main_menu 1");
        app.update(&tree, 64.0, &mut input, &mut audio);
        assert!(menu_open(&app));
        app.exec("disconnect");
        app.update(&tree, 80.0, &mut input, &mut audio);
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("{:?}", app.state())
        };
        assert_eq!(menu.menu.list, "menu_main");
        // Nothing to leave on the title screen.
        app.exec("disconnect");
        app.update(&tree, 96.0, &mut input, &mut audio);
        assert!(matches!(app.state(), AppState::Menu { .. }));
        assert!(!app.quit_requested());
    }

    /// Steering-the-original.md section 7: a normal exit.
    #[test]
    fn a_bound_quit_line_ends_the_game() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        app.exec("bind_console quit kK");
        app.take_log_file();
        app.take_save();
        press(&mut input, Key::K);
        app.update(&tree, 32.0, &mut input, &mut audio);
        assert!(app.quit_requested());
        assert!(app.take_log_file().is_some());
        assert!(app.take_save().is_some());
    }

    /// `snd_restart` restarts the sound system with `snd_targets`
    /// (sound-positional.md section 2), and only then.
    #[test]
    fn snd_restart_takes_snd_targets() {
        use crate::core::sound::system::SoundSystem;
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        audio.sounds = Some(SoundSystem::new(4));
        let voices = |audio: &FakeAudio| audio.sounds.as_ref().unwrap().voice_count();
        // The first frame with sound takes it.
        app.update(&tree, 32.0, &mut input, &mut audio);
        assert_eq!(voices(&audio), 16);
        app.exec("snd_targets 8");
        app.update(&tree, 48.0, &mut input, &mut audio);
        assert_eq!(voices(&audio), 16);
        app.exec("snd_restart");
        app.update(&tree, 64.0, &mut input, &mut audio);
        assert_eq!(voices(&audio), 8);
    }

    /// Steering-the-original.md 3.2: `start` of the New Game loads it at
    /// the difficulty set; without a client it says so.
    #[test]
    fn start_begins_the_new_game() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 0.0, &mut input, &mut audio);
        for _ in 0..5 {
            press(&mut input, Key::Escape);
            app.update(&tree, 10.0, &mut input, &mut audio);
            input.end_frame();
        }
        assert!(matches!(app.state(), AppState::Menu { .. }));
        app.exec("start server(all/alife/new)");
        app.update(&tree, 20.0, &mut input, &mut audio);
        assert_eq!(
            logged(
                &app,
                "! Can't start game without client. Arguments: 'server(all/alife/new)'."
            ),
            1
        );
        app.exec("g_game_difficulty gd_veteran");
        app.exec("start server(all/single/alife/new) client(localhost)");
        app.update(&tree, 30.0, &mut input, &mut audio);
        let AppState::Loading { difficulty, .. } = app.state() else {
            panic!("{:?}", app.state())
        };
        assert_eq!(difficulty, "gd_veteran");
        assert!(audio.playing.is_empty(), "the menu music stopped");
    }

    /// Console-misc-settings.md 8: `check_for_updates` prints the
    /// original's failure to send the query, whatever its argument.
    #[test]
    fn check_for_updates_cannot_send_the_query() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 0.0, &mut input, &mut audio);
        app.exec("check_for_updates");
        app.exec("check_for_updates 1");
        app.update(&tree, 10.0, &mut input, &mut audio);
        assert_eq!(logged(&app, "! Unable to send query for patch!"), 2);
    }

    /// console-key-bindings.md section 5.
    #[test]
    fn a_press_reaching_the_level_executes_its_bound_line() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        let start = "- --- Action list start ---";
        app.exec("bind_console list_actions kK");
        app.exec("bind_console list_actions mouse1");
        app.exec("bind wpn_fire mouse1");
        // An executed line applies the console's bindings to the level.
        app.exec("bind inventory kI");
        press(&mut input, Key::K);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert_eq!(logged(&app, start), 1);
        assert!(!app.console().log().iter().any(|l| l.starts_with("~ ")));
        // The mouse's left button presses mouse1; the press is used up,
        // its release kept.
        let left = Control::Mouse(crate::core::input::MouseButton::Left);
        for pressed in [true, false] {
            input.handle(Event::PointerButton {
                at: None,
                button: crate::core::input::MouseButton::Left,
                pressed,
                time: 40.0,
            });
        }
        app.update(&tree, 48.0, &mut input, &mut audio);
        assert_eq!(logged(&app, start), 2);
        assert_eq!(input.device_changes(), [(left, false)]);
        input.end_frame();
        // Not while a window is open.
        press(&mut input, Key::I);
        app.update(&tree, 64.0, &mut input, &mut audio);
        input.end_frame();
        assert!(app.inventory_open());
        press(&mut input, Key::K);
        app.update(&tree, 80.0, &mut input, &mut audio);
        input.end_frame();
        assert_eq!(logged(&app, start), 2);
    }

    /// console-key-bindings.md section 5: mouse buttons 4 to 8 press the
    /// keys `mouse4` … `mouse8`.
    #[test]
    fn mouse_buttons_four_to_eight_execute_their_lines() {
        use crate::core::input::MouseButton;
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        let start = "- --- Action list start ---";
        let further = &MouseButton::ALL[3..];
        for b in further {
            app.exec(&format!("bind_console list_actions {}", b.key_name()));
        }
        for (i, &button) in further.iter().enumerate() {
            for pressed in [true, false] {
                input.handle(Event::PointerButton {
                    at: None,
                    button,
                    pressed,
                    time: 40.0 + i as f64,
                });
            }
            app.update(&tree, 48.0 + 16.0 * i as f64, &mut input, &mut audio);
            input.end_frame();
            assert_eq!(logged(&app, start), i + 1, "{button:?}");
        }
    }

    /// console-key-bindings.md section 5, rule 3: the interface takes a
    /// press of the inventory key or a PDA key while the character lives,
    /// so its line is not executed; dead, the line is.
    #[test]
    fn the_inventory_key_opens_the_screen_instead_of_executing_its_line() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        let start = "- --- Action list start ---";
        app.exec("bind inventory kI");
        app.exec("bind_console list_actions kI");
        app.exec("bind map kM");
        app.exec("bind_console list_actions kM");
        press(&mut input, Key::M);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert!(app.pda_open(), "the map key opens the PDA");
        assert_eq!(logged(&app, start), 0);
        press(&mut input, Key::M);
        app.update(&tree, 40.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!app.pda_open(), "and closes it");
        assert_eq!(logged(&app, start), 0);
        press(&mut input, Key::I);
        app.update(&tree, 48.0, &mut input, &mut audio);
        input.end_frame();
        assert!(app.inventory_open());
        assert_eq!(logged(&app, start), 0);
        // The press closing it goes to the open window.
        press(&mut input, Key::I);
        app.update(&tree, 64.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!app.inventory_open());
        assert_eq!(logged(&app, start), 0);
        let Some(Phase::Level { level, .. }) = &mut app.phase else {
            panic!()
        };
        level.character_mut().unwrap().conditions.health = 0.0;
        press(&mut input, Key::I);
        app.update(&tree, 80.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!app.inventory_open());
        assert_eq!(logged(&app, start), 1);
    }

    /// pda-window.md 4–6: P opens the PDA on the tasks tab over the
    /// running level, the cursor shown and keys passed on; the inventory
    /// key does nothing under it; Escape closes it without the menu. The
    /// touch PDA button and a gamepad's d-pad left open it, east closes it
    /// (our design).
    #[test]
    fn the_pda_keys_open_and_close_the_window() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        let mut t = 32.0;
        let mut frame = |app: &mut App, input: &mut Input| {
            app.update(&tree, t, input, &mut audio);
            input.end_frame();
            t += 16.0;
        };
        press(&mut input, Key::P);
        frame(&mut app, &mut input);
        assert!(app.pda_open());
        assert!(app.cursor_shown() && app.character_takes_keys());
        assert!(app.level_controls().is_none() && app.pda_over_level());
        let AppState::Level { pda: Some(pda), .. } = app.state() else {
            panic!("no PDA state")
        };
        assert_eq!(pda.tab, Some(0));
        // A click on the diary's caption switches to it (3.3).
        let [x, y, w, h] = pda.tabs[2].rect;
        input.handle(Event::PointerMoved {
            at: [x + w / 2.0, y + h / 2.0],
        });
        for pressed in [true, false] {
            input.handle(Event::PointerButton {
                at: None,
                button: crate::core::input::MouseButton::Left,
                pressed,
                time: 5000.0,
            });
        }
        frame(&mut app, &mut input);
        let AppState::Level { pda: Some(pda), .. } = app.state() else {
            panic!("no PDA state")
        };
        assert_eq!(pda.tab, Some(2), "the click switched tabs");
        press(&mut input, Key::I);
        frame(&mut app, &mut input);
        assert!(app.pda_open() && !app.inventory_open());
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input);
        assert!(!app.pda_open());
        assert!(
            matches!(app.phase, Some(Phase::Level { menu: None, .. })),
            "no menu"
        );
        assert!(app.level_controls().is_some());
        for pressed in [true, false] {
            input.handle(Event::TouchButton {
                button: crate::core::input::TouchButton::Pda,
                pressed,
            });
        }
        frame(&mut app, &mut input);
        assert!(app.pda_open(), "the touch button opens it");
        for pressed in [true, false] {
            input.handle(Event::TouchButton {
                button: crate::core::input::TouchButton::Pda,
                pressed,
            });
        }
        frame(&mut app, &mut input);
        assert!(!app.pda_open(), "and closes it");
        for (button, open) in [
            (crate::core::input::GamepadButton::DpadLeft, true),
            (crate::core::input::GamepadButton::East, false),
        ] {
            for pressed in [true, false] {
                input.handle(Event::GamepadButton { button, pressed });
            }
            frame(&mut app, &mut input);
            assert_eq!(app.pda_open(), open, "{button:?}");
        }
        assert!(
            matches!(app.phase, Some(Phase::Level { menu: None, .. })),
            "east closed the PDA only"
        );
    }

    #[test]
    fn the_quit_key_opens_the_menu_and_never_executes_its_line() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        app.exec("bind quit kESCAPE");
        app.exec("bind_console list_actions kESCAPE");
        press(&mut input, Key::Escape);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert!(matches!(app.state(), AppState::Level { menu: Some(_), .. }));
        assert_eq!(logged(&app, "- --- Action list start ---"), 0);
        // Nor with the menu open.
        app.exec("bind_console list_actions kK");
        press(&mut input, Key::K);
        app.update(&tree, 48.0, &mut input, &mut audio);
        assert_eq!(logged(&app, "- --- Action list start ---"), 0);
    }

    #[test]
    fn an_unknown_key_name_ends_the_game() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        app.exec("bind_console flush kNOSUCH");
        app.update(&tree, 32.0, &mut input, &mut audio);
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("{:?}", app.state())
        };
        assert!(menu.error.unwrap().contains("knosuch"));
        assert_eq!(
            logged(
                &app,
                "! cant find corresponding [_keyboard*] for keyname knosuch"
            ),
            1
        );
    }

    #[test]
    fn the_inventory_key_opens_the_screen_over_the_running_level() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        let game_time = |app: &App| match app.state() {
            AppState::Level { level, .. } => level.game_time,
            _ => panic!(),
        };
        press(&mut input, Key::I);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert!(app.inventory_open());
        // inventory-screen.md 1.3: the cursor shows, the page's touch
        // controls and pointer lock go (our design), the game runs on.
        assert!(app.level_controls().is_none());
        // Touch is a touchpad for its cursor (#108).
        assert!(app.cursor_shown());
        // Keys still reach the character: the page forwards Control.
        assert!(app.character_takes_keys());
        assert!(audio.started.last().unwrap().1.ends_with("inv_open.ogg"));
        let t = game_time(&app);
        // A key the screen does not use reaches the character: W walks.
        input.handle(Event::Key {
            key: Key::W,
            pressed: true,
        });
        app.update(&tree, 132.0, &mut input, &mut audio);
        input.end_frame();
        let AppState::Level {
            level, inventory, ..
        } = app.state()
        else {
            panic!()
        };
        assert!(level.game_time > t, "not paused");
        assert!(level.player.moving, "W walks under the screen");
        assert!(inventory.is_some());
        input.handle(Event::Key {
            key: Key::W,
            pressed: false,
        });
        // Mouse buttons are the screen's: no fire, and a press on the bread
        // selects it.
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert!(scene.first("cursor").is_some());
        assert!(
            scene.world.as_ref().is_some_and(|w| !w.covered),
            "the level shows"
        );
        // Escape closes the screen, not opening the menu (1.2 item 2).
        press(&mut input, Key::Escape);
        app.update(&tree, 148.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!app.inventory_open() && !menu_open(&app) && !app.cursor_shown());
        assert!(app.level_controls().is_some());
        assert!(audio.started.last().unwrap().1.ends_with("inv_close.ogg"));
        // Escape again opens the menu as usual.
        press(&mut input, Key::Escape);
        app.update(&tree, 164.0, &mut input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app));
    }

    /// inventory-screen.md 4.2: an item without its grid keys is a fatal
    /// error, opening the screen or while it is open: the game ends and
    /// the title menu says why.
    #[test]
    fn the_inventory_screen_errors_are_fatal() {
        let without_grid = |app: &mut App| {
            let Some(Phase::Level { level, .. }) = &mut app.phase else {
                panic!()
            };
            let inv = &mut level.character_mut().unwrap().inventory;
            for item in &mut inv.items {
                item.config.inv_grid = None;
            }
        };
        let ends_saying_why = |app: &App| {
            let AppState::Menu { menu, .. } = app.state() else {
                panic!("{:?}", app.state())
            };
            assert!(menu.error.unwrap().contains("[bread] lacks"));
        };
        // Opening.
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        without_grid(&mut app);
        press(&mut input, Key::I);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        ends_saying_why(&app);
        // While open: an item without them enters the inventory, which
        // rebuilds the screen (4.3).
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        press(&mut input, Key::I);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert!(app.inventory_open());
        let Some(Phase::Level { level, .. }) = &mut app.phase else {
            panic!()
        };
        let inv = &mut level.character_mut().unwrap().inventory;
        let mut item = inv.item(5).unwrap().clone();
        item.id = 6;
        item.config.inv_grid = None;
        inv.take(item);
        app.update(&tree, 48.0, &mut input, &mut audio);
        ends_saying_why(&app);
    }

    #[test]
    fn touch_and_gamepad_open_the_inventory_and_it_eats() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        // The touch Inventory button (our design).
        for pressed in [true, false] {
            input.handle(Event::TouchButton {
                button: crate::core::input::TouchButton::Inventory,
                pressed,
            });
        }
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert!(app.inventory_open());
        // The inventory key closes it.
        press(&mut input, Key::I);
        app.update(&tree, 48.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!app.inventory_open());
        // A gamepad's north button opens it (our design).
        for pressed in [true, false] {
            input.handle(Event::GamepadButton {
                button: crate::core::input::GamepadButton::North,
                pressed,
            });
        }
        app.update(&tree, 64.0, &mut input, &mut audio);
        input.end_frame();
        assert!(app.inventory_open());
        // A double click on the bread eats it.
        let AppState::Level {
            inventory: Some(inv),
            ..
        } = app.state()
        else {
            panic!()
        };
        let [x, y, w, h] = inv.lists[0].cells[0].rect;
        let at = [x + w / 2.0, y + h / 2.0];
        input.handle(Event::PointerMoved { at });
        for time in [1000.0, 1100.0] {
            for pressed in [true, false] {
                input.handle(Event::PointerButton {
                    at: None,
                    button: crate::core::input::MouseButton::Left,
                    pressed,
                    time,
                });
            }
        }
        app.update(&tree, 80.0, &mut input, &mut audio);
        input.end_frame();
        let AppState::Level { level, .. } = app.state() else {
            panic!()
        };
        let c = level.character.unwrap();
        assert!(c.items.is_empty(), "the bread was eaten");
        assert!((c.satiety - 0.7).abs() < 1e-6);
        // Death closes it (1.2 item 4).
        let Some(Phase::Level { level, .. }) = &mut app.phase else {
            panic!()
        };
        level.character_mut().unwrap().conditions.health = 0.0;
        app.update(&tree, 96.0, &mut input, &mut audio);
        assert!(!app.inventory_open());
        // Dead, the key opens nothing.
        press(&mut input, Key::I);
        app.update(&tree, 112.0, &mut input, &mut audio);
        assert!(!app.inventory_open());
    }

    #[test]
    fn the_test_interface_gives_items() {
        let (_tree, mut app, _input, _audio) = in_level_with_inventory();
        app.data.system = crate::inventory_screen::tests::items();
        let id = app.give_item("medkit").unwrap();
        assert_eq!(id, 6);
        let AppState::Level { level, .. } = app.state() else {
            panic!()
        };
        let c = level.character.unwrap();
        assert_eq!(
            c.items.last().map(|i| (i.id, i.place)),
            Some((6, "backpack"))
        );
        assert!(app.give_item("no_such_section").is_err());
    }

    fn hold(input: &mut Input, key: Key, pressed: bool) {
        input.handle(Event::Key { key, pressed });
    }

    /// console-window.md sections 1 – 4 over a level: the console key
    /// opens it, the level loses its held keys and the mouse, keys type,
    /// Esc closes it without opening the menu, held movement resumes and
    /// a held Shift is released (3.4).
    #[test]
    fn the_console_opens_over_the_level_takes_the_keys_and_closes() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        // The console's bindings are the level's once applied.
        app.exec("bind console kGRAVE");
        app.exec("bind forward kW");
        let mut t = 32.0;
        let mut frame = |app: &mut App, input: &mut Input| {
            app.update(&tree, t, input, &mut audio);
            input.end_frame();
            t += 16.0;
        };
        let forward = |app: &App| app.level().unwrap().player().state().flags.forward;
        hold(&mut input, Key::W, true);
        frame(&mut app, &mut input);
        frame(&mut app, &mut input);
        assert!(forward(&app));
        press(&mut input, Key::Grave);
        frame(&mut app, &mut input);
        assert!(app.console_open() && app.level_controls().is_none());
        assert!(!app.character_takes_keys());
        frame(&mut app, &mut input);
        assert!(!forward(&app), "the level lost the held W");
        // Keys type; Shift held is the console's from its press.
        hold(&mut input, Key::LeftShift, true);
        press(&mut input, Key::Digit1);
        frame(&mut app, &mut input);
        press(&mut input, Key::Q);
        frame(&mut app, &mut input);
        assert_eq!(app.console().line(), "!q");
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        let area = scene.first(crate::console_window::LAYER).unwrap();
        assert_eq!(area.bounds(), [-0.5, -0.5, 1023.5, 383.5], "half height");
        assert!(scene.layer(crate::console_window::LAYER).count() > 3);
        // The quit key (Esc) closes the console and opens no menu; W still
        // held moves again; Shift is off for the next opening.
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input);
        assert!(!app.console_open() && !menu_open(&app));
        assert!(forward(&app), "the held W resumes");
        assert!(app.level_controls().is_some());
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert!(scene.first(crate::console_window::LAYER).is_none());
        // Opened again: the line is emptied, and Shift (still held from
        // before) types nothing shifted until a repeat comes due.
        press(&mut input, Key::Grave);
        frame(&mut app, &mut input);
        assert_eq!(app.console().line(), "");
        press(&mut input, Key::Digit1);
        frame(&mut app, &mut input);
        assert_eq!(app.console().line(), "1");
        // Key 41 without Shift closes it too.
        hold(&mut input, Key::LeftShift, false);
        press(&mut input, Key::Grave);
        frame(&mut app, &mut input);
        assert!(!app.console_open());
    }

    /// console-window.md sections 1, 2: the console over the main menu,
    /// the menu's keys and mouse lost to it; the menu remembers it as it
    /// opens over the level and opens it again as it closes.
    #[test]
    fn the_console_over_the_menu_and_the_menu_remembering_it() {
        let (tree, mut app, mut input, mut audio) = in_level();
        app.exec("bind console kGRAVE");
        let mut t = 32.0;
        let mut frame = |app: &mut App, input: &mut Input| {
            app.update(&tree, t, input, &mut audio);
            input.end_frame();
            t += 16.0;
        };
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input);
        assert!(menu_open(&app));
        let cursor = input.cursor();
        press(&mut input, Key::Grave);
        frame(&mut app, &mut input);
        assert!(app.console_open());
        // The mouse is lost: the cursor stays; Esc closes the console, not
        // the menu.
        input.handle(Event::PointerMotion {
            delta: [50.0, 50.0],
        });
        frame(&mut app, &mut input);
        assert_eq!(input.cursor(), cursor);
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input);
        assert!(!app.console_open() && menu_open(&app));
        // Open over the menu, then the menu closes: it opens again over the
        // level. Typed `main_menu off` is an action (#647), so the menu
        // closes here as the console reopens through the menu's own close.
        press(&mut input, Key::Grave);
        frame(&mut app, &mut input);
        let Some(Phase::Level { menu, .. }) = &mut app.phase else {
            panic!()
        };
        *menu = None;
        frame(&mut app, &mut input);
        assert!(app.console_open(), "the menu's close opened it again");
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input);
        assert!(!app.console_open() && !menu_open(&app));
        // The menu opening closes it, remembered.
        press(&mut input, Key::Grave);
        frame(&mut app, &mut input);
        let in_game = app.opening(true);
        let Some(Phase::Level { menu, .. }) = &mut app.phase else {
            panic!()
        };
        *menu = Some(Box::new(TitleMenu::open(
            &app.data,
            0,
            &input,
            &mut FakeAudio::default(),
            in_game,
        )));
        frame(&mut app, &mut input);
        assert!(!app.console_open() && menu_open(&app));
    }

    /// The touch screen's Console button (#642) opens the console as the
    /// key bound to `console` does, over the level and over the menu, and
    /// does nothing while it is open; the page shows it over the menu
    /// while the console is closed.
    #[test]
    fn the_touch_console_button_opens_the_console_as_its_key_does() {
        let (tree, mut app, mut input, mut audio) = in_level();
        let mut t = 32.0;
        let mut frame = |app: &mut App, input: &mut Input| {
            app.update(&tree, t, input, &mut audio);
            input.end_frame();
            t += 16.0;
        };
        let tap = |input: &mut Input| {
            for pressed in [true, false] {
                input.handle(Event::TouchButton {
                    button: crate::core::input::TouchButton::Console,
                    pressed,
                });
            }
        };
        assert!(app.level_controls().is_some() && !app.console_opens_over_menu());
        tap(&mut input);
        frame(&mut app, &mut input);
        assert!(app.console_open() && app.level_controls().is_none());
        // Open, it types nothing and stays open.
        tap(&mut input);
        frame(&mut app, &mut input);
        assert!(app.console_open() && app.console().line().is_empty());
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input);
        assert!(!app.console_open() && !menu_open(&app));
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input);
        assert!(menu_open(&app) && app.console_opens_over_menu());
        tap(&mut input);
        frame(&mut app, &mut input);
        assert!(app.console_open() && !app.console_opens_over_menu());
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input);
        assert!(!app.console_open() && menu_open(&app));
    }

    #[test]
    fn a_lost_pointer_lock_opens_the_menu_and_its_escape_does_not_close_it() {
        let (tree, mut app, mut input, mut audio) = in_level();
        input.handle(Event::PointerUnlocked);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app), "the unlock opened the menu");
        assert!(app.level_controls().is_none());
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert!(app.cursor_shown() && scene.first("cursor").is_some());
        // The Escape the browser used to leave the lock, arriving a frame
        // later, leaves it open.
        press(&mut input, Key::Escape);
        app.update(&tree, 48.0, &mut input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app), "the following Escape is ignored");
        // A later Escape closes it.
        app.update(&tree, 64.0, &mut input, &mut audio);
        press(&mut input, Key::Escape);
        app.update(&tree, 80.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!menu_open(&app), "a later Escape returns to the game");
        // Escape opens it too, and closes it the next frame (no unlock).
        press(&mut input, Key::Escape);
        app.update(&tree, 96.0, &mut input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app));
        press(&mut input, Key::Escape);
        app.update(&tree, 112.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!menu_open(&app));
    }

    fn game_time(app: &App) -> u64 {
        match app.state() {
            AppState::Level { level, .. } => level.game_time,
            s => panic!("not in the level: {s:?}"),
        }
    }

    #[test]
    fn hiding_in_a_level_opens_the_menu_and_hidden_time_never_runs() {
        let (tree, mut app, mut input, mut audio) = in_level();
        app.update(&tree, 1016.0, &mut input, &mut audio);
        let (clock, time) = (app.clock(), game_time(&app));
        assert!(time > 0, "the game clock runs");
        app.set_hidden(true, &input, &mut audio);
        assert!(menu_open(&app), "hiding opened the menu");
        assert!(app.level_controls().is_none());
        // Updates while hidden (a platform still calling) do nothing.
        app.update(&tree, 30_000.0, &mut input, &mut audio);
        assert_eq!((app.clock(), game_time(&app)), (clock, time));
        // The first frame back has a zero delta, then the clock runs on;
        // the menu's pause holds the game clock. An Escape in the first two
        // frames back (one the browser used to leave pointer lock) leaves
        // the menu open.
        app.set_hidden(false, &input, &mut audio);
        press(&mut input, Key::Escape);
        app.update(&tree, 600_000.0, &mut input, &mut audio);
        input.end_frame();
        assert_eq!(app.clock(), clock, "the hidden time is not counted");
        assert!(menu_open(&app), "the guarded Escape is ignored");
        app.update(&tree, 600_016.0, &mut input, &mut audio);
        assert_eq!(app.clock(), clock + 16);
        assert_eq!(game_time(&app), time, "paused under the menu");
        press(&mut input, Key::Escape);
        app.update(&tree, 600_032.0, &mut input, &mut audio);
        input.end_frame();
        assert!(!menu_open(&app), "a later Escape returns to the game");
        app.update(&tree, 601_064.0, &mut input, &mut audio);
        // About a second shown ran, at the game's time factor; the ~10
        // minutes hidden did not.
        let ran = game_time(&app) - time;
        assert!(ran < 60_000, "only shown time ran: {ran} ms of game time");
    }

    #[test]
    fn the_global_clock_stands_still_under_the_title_menu() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 1000.0, &mut input, &mut audio);
        // The fixture's logo items have no `pause_state`: the clock runs.
        app.update(&tree, 1100.0, &mut input, &mut audio);
        assert_eq!(app.global_clock.now(), 100);
        assert!(!app.pause.is_on());
        for at in [1200.0, 1300.0] {
            press(&mut input, Key::Escape);
            app.update(&tree, at, &mut input, &mut audio);
            input.end_frame();
        }
        assert!(matches!(app.state(), AppState::Menu { .. }));
        assert!(app.pause.is_on(), "the main menu");
        app.update(&tree, 9300.0, &mut input, &mut audio);
        assert_eq!(app.global_clock.now(), 300);
    }

    #[test]
    fn the_pause_in_a_level_the_menu_the_intro_and_the_hidden_page() {
        let (tree, mut app, mut input, mut audio) = in_level();
        let (mut at, start) = (16.0, app.global_clock.now());
        let mut frame = |app: &mut App, input: &mut Input, ms: f64| {
            at += ms;
            app.update(&tree, at, input, &mut audio);
            input.end_frame();
            app.global_clock.now()
        };
        assert_eq!(frame(&mut app, &mut input, 1000.0), start + 1000);
        // The menu: on from its opening to its closing.
        press(&mut input, Key::Escape);
        assert_eq!(frame(&mut app, &mut input, 16.0), start + 1016);
        assert!(menu_open(&app) && app.pause.is_on());
        assert_eq!(frame(&mut app, &mut input, 5000.0), start + 1016);
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input, 16.0);
        assert!(!menu_open(&app) && !app.pause.is_on());
        assert_eq!(frame(&mut app, &mut input, 16.0), start + 1032);
        // Hidden (focus lost): the menu opens and the pause is on; shown
        // again (focus back), the menu holds it until it closes.
        app.set_hidden(true, &input, &mut FakeAudio::default());
        assert!(app.pause.is_on());
        app.set_hidden(false, &input, &mut FakeAudio::default());
        assert!(app.pause.is_on());
        for _ in 0..3 {
            frame(&mut app, &mut input, 16.0);
        }
        assert_eq!(app.global_clock.now(), start + 1032);
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input, 16.0);
        assert!(!app.pause.is_on());
        let now = frame(&mut app, &mut input, 16.0);
        // The intro's `pause_state` on item: on while it plays.
        if let Some(Phase::Level { intro, .. }) = &mut app.phase {
            intro.checked = true;
            intro.player = Some(MoviePlayer::new(app.clock));
        }
        assert_eq!(frame(&mut app, &mut input, 16.0), now + 16);
        assert!(app.pause.is_on());
        assert_eq!(frame(&mut app, &mut input, 1000.0), now + 16);
        press(&mut input, Key::Escape);
        frame(&mut app, &mut input, 16.0);
        assert!(!app.pause.is_on(), "the intro skipped");
        assert_eq!(frame(&mut app, &mut input, 16.0), now + 32);
    }

    #[test]
    fn hiding_leaves_an_open_menu_and_opens_nothing_outside_the_level() {
        // The menu already open, its selection moved: left as it is.
        let (tree, mut app, mut input, mut audio) = in_level();
        press(&mut input, Key::Escape);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        press(&mut input, Key::Down);
        app.update(&tree, 48.0, &mut input, &mut audio);
        input.end_frame();
        let menu = |app: &App| match app.state() {
            AppState::Level { menu: Some(m), .. } => (m.menu.list, m.menu.selected),
            s => panic!("no menu: {s:?}"),
        };
        let before = menu(&app);
        app.set_hidden(true, &input, &mut audio);
        assert_eq!(menu(&app), before);

        // The logo sequence: nothing opens and its clock stands still.
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        app.update(&tree, 0.0, &mut input, &mut audio);
        app.update(&tree, 100.0, &mut input, &mut audio);
        app.set_hidden(true, &input, &mut audio);
        app.update(&tree, 5000.0, &mut input, &mut audio);
        app.set_hidden(false, &input, &mut audio);
        app.update(&tree, 9000.0, &mut input, &mut audio);
        assert!(matches!(app.state(), AppState::Startup { clock: 100, .. }));

        // Under the loading image and during the intro: nothing opens.
        let (tree, mut app, input, mut audio) = in_level();
        let Some(Phase::Level { overlay, .. }) = &mut app.phase else {
            panic!()
        };
        *overlay = Some(Overlay::new(LoadingScreen::default(), &mut audio));
        app.set_hidden(true, &input, &mut audio);
        assert!(!menu_open(&app), "not under the loading image");
        let (tree2, mut app, mut input, mut audio) = in_level();
        let Some(Phase::Level { intro, .. }) = &mut app.phase else {
            panic!()
        };
        intro.checked = true;
        intro.player = Some(MoviePlayer::new(16));
        app.set_hidden(true, &input, &mut audio);
        assert!(!menu_open(&app), "not during the intro");
        app.set_hidden(false, &input, &mut audio);
        app.update(&tree2, 50_000.0, &mut input, &mut audio);
        // The intro takes on where it stopped.
        let AppState::Level {
            intro: Some(intro), ..
        } = app.state()
        else {
            panic!("the intro plays: {:?}", app.state())
        };
        assert_eq!(intro.elapsed_ms, 0);
        drop(tree);
    }

    #[test]
    fn the_menu_asked_for_under_the_loading_image_opens_once_the_controls_are_live() {
        for gamepad in [false, true] {
            let (tree, mut app, mut input, mut audio) = in_level();
            let Some(Phase::Level { overlay, .. }) = &mut app.phase else {
                panic!()
            };
            let mut o = Overlay::new(LoadingScreen::default(), &mut audio);
            o.frames_left = 4;
            *overlay = Some(o);
            let mut t = 32.0;
            let mut step = |app: &mut App, input: &mut Input| {
                app.update(&tree, t, input, &mut audio);
                input.end_frame();
                t += 16.0;
            };
            if gamepad {
                // A gamepad's east button.
                for pressed in [true, false] {
                    input.handle(Event::GamepadButton {
                        button: crate::core::input::GamepadButton::East,
                        pressed,
                    });
                }
            } else {
                press(&mut input, Key::Escape);
            }
            step(&mut app, &mut input);
            assert!(!menu_open(&app), "not under the loading image");
            let asked = |app: &App| {
                matches!(
                    app.state(),
                    AppState::Level {
                        menu_asked: true,
                        ..
                    }
                )
            };
            assert!(asked(&app));
            // The image's last frames, with the intro under the last two,
            // then the intro: still nothing.
            for _ in 0..4 {
                step(&mut app, &mut input);
            }
            assert!(!menu_open(&app));
            assert!(app.level_controls().is_none());
            assert!(matches!(
                app.state(),
                AppState::Level {
                    intro: Some(_),
                    overlay_frames: 0,
                    ..
                }
            ));
            // An Escape during the intro skips it; the controls go live
            // and the menu opens in that frame.
            press(&mut input, Key::Escape);
            step(&mut app, &mut input);
            assert!(
                menu_open(&app),
                "the menu opens once the controls are live: {:?}",
                app.state()
            );
            assert!(!asked(&app));
            // Escape on it returns to the game, which then runs as usual.
            press(&mut input, Key::Escape);
            step(&mut app, &mut input);
            assert!(!menu_open(&app));
            step(&mut app, &mut input);
            assert!(!menu_open(&app), "asked for once");
            assert!(app.level_controls().is_some());
        }
    }

    #[test]
    fn without_an_intro_the_menu_asked_for_opens_as_the_loading_image_goes() {
        let (tree, mut app, mut input, mut audio) = in_level();
        let Some(Phase::Level { overlay, intro, .. }) = &mut app.phase else {
            panic!()
        };
        // The intro checked already (none plays).
        intro.checked = true;
        let mut o = Overlay::new(LoadingScreen::default(), &mut audio);
        o.frames_left = 3;
        *overlay = Some(o);
        press(&mut input, Key::Escape);
        let mut t = 32.0;
        for _ in 0..2 {
            app.update(&tree, t, &mut input, &mut audio);
            input.end_frame();
            t += 16.0;
            assert!(!menu_open(&app), "not under the loading image");
        }
        // The image's last frame ends: the controls are live, the menu opens.
        app.update(&tree, t, &mut input, &mut audio);
        assert!(matches!(
            app.state(),
            AppState::Level {
                intro: None,
                overlay_frames: 0,
                ..
            }
        ));
        assert!(menu_open(&app), "the menu opens in the first live frame");
    }

    /// A fatal error the level met while running (the simulation's switch
    /// pass, offline-simulation.md 6.2) ends the game at the end of the
    /// update: the title menu opens saying why.
    #[test]
    fn a_fatal_error_of_the_running_level_ends_the_game() {
        let (tree, mut app, mut input, mut audio) = in_level();
        let Some(Phase::Level { level, .. }) = &mut app.phase else {
            panic!()
        };
        level.record_fatal("lamp: no renderer type set for hanging-lamp".into());
        app.update(&tree, 32.0, &mut input, &mut audio);
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("{:?}", app.state())
        };
        assert!(menu.error.unwrap().contains("hanging-lamp"));
    }

    /// `App::fatal`: the title menu that already says why, failing again
    /// with the same error, stays as it is; another error opens it anew.
    #[test]
    fn the_menu_saying_why_is_not_opened_again_for_the_same_error() {
        let (tree, mut app, mut input, mut audio) = in_level();
        let error = "x: fatal".to_owned();
        app.take_log_file();
        app.failed = Some(error.clone());
        app.update(&tree, 32.0, &mut input, &mut audio);
        assert!(matches!(app.state(), AppState::Menu { .. }));
        // The report, then the log file (console-log.md 3.3, 3.8).
        let log = app.console().log();
        assert_eq!(log[log.len() - 3], "[error]Arguments     : x: fatal");
        assert!(
            app.take_log_file()
                .unwrap()
                .ends_with(b"[error]Arguments     : x: fatal\r\n \r\n\r\n")
        );
        let opened = audio.started.len();
        assert!(opened > 0, "the menu's music started");
        for t in [48.0, 64.0] {
            app.failed = Some(error.clone());
            app.update(&tree, t, &mut input, &mut audio);
        }
        assert_eq!(audio.started.len(), opened, "not opened again");
        app.failed = Some("y: fatal".into());
        app.update(&tree, 80.0, &mut input, &mut audio);
        assert!(audio.started.len() > opened, "opened anew");
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("{:?}", app.state())
        };
        assert!(menu.error.unwrap().contains("y: fatal"));
    }

    /// loading-screen.md 7: data without `intro_game` loads; the New Game
    /// fails when it reaches the intro, under the last overlay frames.
    #[test]
    fn a_new_game_without_its_intro_ends_when_the_intro_is_due() {
        let (tree, mut app, mut input, mut audio) = in_level();
        app.data.intro_game = None;
        let Some(Phase::Level { overlay, .. }) = &mut app.phase else {
            panic!()
        };
        let mut o = Overlay::new(LoadingScreen::default(), &mut audio);
        o.frames_left = 4;
        *overlay = Some(o);
        // Three and two frames left after this one: not yet due.
        app.update(&tree, 32.0, &mut input, &mut audio);
        assert!(matches!(app.state(), AppState::Level { .. }));
        app.update(&tree, 48.0, &mut input, &mut audio);
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("{:?}", app.state())
        };
        assert!(menu.error.unwrap().contains("intro_game"));
    }

    #[test]
    fn a_setting_changed_in_the_menu_reaches_the_running_level() {
        let (tree, mut app, mut input, mut audio) = in_level();
        press(&mut input, Key::Escape);
        app.update(&tree, 32.0, &mut input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app));
        // What the options screen's controls set (options-screens.md 5.3).
        for line in [
            "mouse_sens 0.5",
            "mouse_invert on",
            "g_always_run off",
            "bind forward kT",
        ] {
            app.console.exec(line);
        }
        app.update(&tree, 48.0, &mut input, &mut audio);
        let Some(Phase::Level { level, .. }) = &app.phase else {
            panic!("not in the level")
        };
        let settings = level.controls().settings;
        assert_eq!(
            (
                settings.mouse_sens,
                settings.mouse_invert,
                settings.always_run
            ),
            (0.5, true, false)
        );
        assert_eq!(level.player().settings.mouse_sens, 0.5);
        assert_eq!(
            level
                .controls()
                .bindings
                .controls(crate::controls::MoveAction::Forward)
                .collect::<Vec<_>>(),
            [Control::Key(Key::T)]
        );
    }

    #[test]
    fn splash_is_centred_one_to_one() {
        let s = splash_scene(Resolution::new(1920, 1080), (500, 268));
        assert_eq!(s.sprites[0].bounds(), [709.5, 405.5, 1209.5, 673.5]);
        assert_eq!(s.sprites[0].texture, TextureId::Splash);
    }
}
