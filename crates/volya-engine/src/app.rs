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

use serde::Serialize;

use crate::audio::AudioOut;
use crate::core::audio::Category;
use crate::core::console::Console;
use crate::core::input::{Action, Control, Input};
use crate::core::sound::Fatal;
use crate::core::sound::system::Listener;
use crate::core::vfs::{self, Vfs};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::inventory_screen::{self, InventoryScreen, InventoryState};
use crate::level::{LevelScreen, LevelState};
use crate::level_sound::{LevelSounds, SoundRun};
use crate::loading_screen::{
    GAME_TYPE, Loading, LoadingScreen, LoadingScreenState, OVERLAY_FRAMES, SERVER_NEW_GAME,
    intro_due,
};
use crate::menu_screens::{Frame, MenuFrame, MenuScreens, MenuState};
use crate::save_load::FieldFont;
use crate::saved_games::SavedGames;
use crate::scene::{Scene, Sprite, TextureId};
use crate::startup::{MoviePlayer, MovieSequenceState, Startup};
use crate::title_menu::{Opening, TitleMenu};
use volya_engine_formats::pe::{self, Rgba};
use volya_engine_formats::ui::Resolution;
use volya_engine_formats::ui::picture::Quad;

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelControls {
    pub crouch: bool,
    /// The crouch toggle engaged in the low crouch's box.
    pub low_crouch: bool,
    pub sprint: bool,
    /// The touch stick's sprint lock holds it forward.
    pub sprint_lock: bool,
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
    /// Why the latest new game failed to load or ended.
    error: Option<String>,
    /// A fatal error met during this update, taken by [`App::fatal`] at
    /// its end.
    failed: Option<String>,
    /// The game type of the latest game started in this run
    /// (loading-screen.md section 5).
    game_type: Option<&'static str>,
    /// Platform time of the update that began the first caption, while
    /// loading.
    load_started: Option<f64>,
    /// Set with the level: when its loading began, until the next update.
    loaded_since: Option<f64>,
    /// Platform milliseconds from the first caption to the update after
    /// the level was made: every stage (one frame each at least) and the
    /// first frame drawn (which uploads the world).
    load_ms: Option<f64>,
    /// The settings and bindings (docs/behavior/options-screens.md).
    console: Console,
    /// The menu's sub-screens and boxes, and the saved games.
    screens: MenuScreens,
    /// The screen's own size in pixels, which the resolution list offers.
    native: Resolution,
    /// The resolution drawn at: `vid_mode` as the last video restart
    /// applied it (sections 5.4, 9).
    video: Resolution,
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
    /// The sound rules' state kept for the whole run, with their clock.
    sound_run: SoundRun,
    /// The console may have changed this frame.
    console_touched: bool,
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
    },
}

impl App {
    pub fn new(data: GameData) -> io::Result<Self> {
        TitleMenu::check(&data)?;
        let console = data.console();
        let video = video_mode(&console);
        Ok(Self {
            applied: console.generation(),
            console,
            screens: MenuScreens::default(),
            native: video,
            video,
            save: None,
            volumes: None,
            sound_voices_set: false,
            sound_in_level: false,
            sound_paused: false,
            level_sounds: None,
            sound_run: SoundRun::default(),
            console_touched: false,
            frames: 0,
            unlock_opened: None,
            prepare_textures: false,
            menu_asked: false,
            hidden: false,
            shown: false,
            menu_font: None,
            data,
            assets: Assets::default(),
            origin: None,
            clock: 0,
            phase: None,
            error: None,
            failed: None,
            game_type: None,
            load_started: None,
            loaded_since: None,
            load_ms: None,
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
        let last = self.console.status("load_last_save");
        Opening {
            in_game,
            last_save: self.screens.saves().last_save_valid(last.as_deref()),
        }
    }

    /// The engine clock, milliseconds.
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// One frame at platform time `time_ms` with this frame's `input`.
    /// Loading a level reads from `tree`.
    pub fn update(
        &mut self,
        tree: &dyn Vfs,
        time_ms: f64,
        input: &Input,
        audio: &mut dyn AudioOut,
    ) {
        if self.hidden {
            return;
        }
        self.update_phase(tree, time_ms, input, audio);
        if let Some(Phase::Level { level, .. }) = &mut self.phase
            && let Some(error) = level.take_fatal()
        {
            self.failed.get_or_insert(error);
        }
        let failed = match self.failed.take() {
            Some(error) => Some(error),
            None => self.sound_frame(tree, audio).err().map(|f| f.to_string()),
        };
        if let Some(error) = failed {
            self.fatal(tree, error, input, audio);
        }
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
        // The level's sounds go as when the level is left.
        let _ = self.sound_frame(tree, audio);
    }

    /// What the sound system takes from this frame
    /// (docs/behavior/sound-positional.md): its voices (`snd_targets`) once
    /// the settings are read; the listener, the level's camera; a pause
    /// while the menu is open over the level (section 4.5); every sound
    /// stopped when the level is left (section 4.4). The level's own sounds
    /// (crate::level_sound) are created once the level runs and the sound
    /// system exists, and their rules run on the frames that advance game
    /// time with no menu open. The platform runs the sound system's update.
    /// A fatal error of theirs is returned.
    fn sound_frame(&mut self, tree: &dyn Vfs, audio: &mut dyn AudioOut) -> Result<(), Fatal> {
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
                if let Some(data) = level.take_sound_data() {
                    match LevelSounds::create(sounds, tree, &self.data.aliases, &data) {
                        Ok(created) => {
                            sounds.occlusion_mut().level = Some(level.shared_collision());
                            self.level_sounds = Some(created);
                        }
                        Err(e) => fatal = Some(e),
                    }
                }
                match level.advanced() {
                    Some(elapsed) => {
                        self.sound_run.advance(elapsed);
                        if let Some(level_sounds) = &mut self.level_sounds
                            && menu.is_none()
                            && let Err(e) = self.sound_run.frame(
                                sounds,
                                level_sounds,
                                &level.sound_inputs(),
                                elapsed as f32 / 1000.0,
                                overlay.is_some(),
                            )
                        {
                            fatal = Some(e);
                        }
                    }
                    None => self.sound_run.paused_frame(),
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
        input: &Input,
        audio: &mut dyn AudioOut,
    ) {
        if self.hidden {
            return;
        }
        self.frames += 1;
        if let Some(at) = self.loaded_since.take() {
            self.load_ms = Some(time_ms - at);
        }
        if std::mem::take(&mut self.shown) {
            // The hidden time never reaches the engine clock: this frame's
            // delta is zero (our design, #93).
            self.origin = Some(time_ms - self.clock as f64);
        }
        let origin = *self.origin.get_or_insert(time_ms);
        self.clock = self.clock.max((time_ms - origin).max(0.0) as u64);
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
                if startup.finished(data) {
                    // Shown within one frame of the sequence's end.
                    *phase = Phase::Menu(TitleMenu::open(data, now, input, audio, title));
                    self.screens.menu_opened();
                }
            }
            Phase::Menu(menu) => {
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
                    menu.resized(data, self.video);
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
                        let mut loading = Loading::start(data, changed);
                        if self.prepare_textures {
                            loading.prepare_textures();
                        }
                        *stages = NewGame::Stages(Box::new(loading));
                        Ok(None)
                    }
                    NewGame::Stages(loading) => loading
                        .step(tree, data, now)
                        .map(|level| level.map(|l| (l, loading.screen().clone()))),
                };
                match step {
                    Ok(None) => {}
                    Ok(Some((level, screen))) => {
                        // The level's in-game interface: the inventory
                        // screen's layout at the `_16` rule of now
                        // (inventory-screen.md section 2).
                        match InventoryScreen::load(tree, data, self.video) {
                            Ok(inventory) => {
                                self.error = None;
                                self.loaded_since = self.load_started.take();
                                self.menu_asked = false;
                                *phase = Phase::Level {
                                    level: Box::new(level),
                                    overlay: Some(Overlay::new(screen, audio)),
                                    intro: Intro::default(),
                                    menu: None,
                                    inventory: Some(Box::new(inventory)),
                                    dropped: Vec::new(),
                                };
                            }
                            Err(e) => {
                                self.error = Some(e.to_string());
                                *phase =
                                    Phase::Menu(TitleMenu::open(data, now, input, audio, title));
                                self.screens.menu_opened();
                            }
                        }
                    }
                    Err(e) => {
                        // Back to the menu, saying why (our choice).
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
                // screens and boxes included (save-load-screens.md section 2).
                level.paused(now);
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
                    menu.resized(data, self.video);
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
                dropped,
            } => {
                if let Some(o) = overlay.take() {
                    *overlay = o.next_frame(audio);
                }
                let left = overlay.as_ref().map(|o| o.frames_left);
                // While the intro plays, including the frame that ends it,
                // input goes to it alone (our choice; the level is paused
                // under it anyway).
                let playing = intro.player.is_some();
                match intro.frame(data, now, left, quit_presses(data, input), audio) {
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

    /// The user configuration file to store (section 12.3), once asked for:
    /// when the options screen closes and when the player quits.
    pub fn take_save(&mut self) -> Option<Vec<u8>> {
        self.save.take()
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
        let scene = self.compose_scene(tree, resolution);
        if let Err(e) = &scene
            && e.kind() != io::ErrorKind::Unsupported
        {
            self.failed.get_or_insert(e.to_string());
        }
        scene
    }

    fn compose_scene(&mut self, tree: &dyn Vfs, resolution: Resolution) -> io::Result<Scene> {
        let mut ctx = Ctx {
            tree,
            data: &self.data,
            assets: &mut self.assets,
            resolution,
            clock_ms: self.clock,
        };
        match &self.phase {
            None => Ok(Scene::new(resolution)),
            Some(Phase::Startup(s)) => s.scene(&mut ctx),
            Some(Phase::Menu(m)) => self.screens.scene(m, &mut ctx),
            Some(Phase::Loading {
                stages: NewGame::Closing(menu),
                ..
            }) => menu.scene(&mut ctx),
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
                let mut scene = level.scene(resolution, self.clock);
                if let Some(world) = &mut scene.world {
                    world.covered = true;
                }
                scene
                    .sprites
                    .extend(self.screens.scene(menu, &mut ctx)?.sprites);
                Ok(scene)
            }
            Some(Phase::Level {
                level,
                overlay,
                intro,
                menu: None,
                inventory,
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
            }) if intro.player.is_none() && !inventory.as_ref().is_some_and(|i| i.is_open()) => {
                let p = level.player().state();
                Some(LevelControls {
                    crouch: p.crouch_toggled,
                    low_crouch: p.crouch_toggled && p.posture == 2,
                    sprint: p.sprint_requested,
                    sprint_lock: level.controls().sprint_lock.locked(),
                })
            }
            _ => None,
        }
    }

    /// Whether the frame shows the game's mouse cursor: the title menu and
    /// its screens, before a game and during one, and the inventory screen
    /// (their scenes draw it). Platforms turn touch into a touchpad for the
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
    /// live, or the inventory screen over the running level passes on the
    /// keys it doesn't use (inventory-screen.md 1.3). Platforms forward
    /// keys pressed with Control then (it crouches).
    pub fn character_takes_keys(&self) -> bool {
        self.level_controls().is_some()
            || matches!(&self.phase, Some(Phase::Level { inventory: Some(i), menu: None, .. }) if i.is_open())
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

    pub fn quit_requested(&self) -> bool {
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
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::input::{Event, Key, TouchPhase};
    use crate::fixture::{self, FakeAudio};

    fn press(input: &mut Input, key: Key) {
        input.handle(Event::Key { key, pressed: true });
        input.handle(Event::Key {
            key,
            pressed: false,
        });
    }

    #[test]
    fn startup_then_menu_then_quit() {
        let tree = fixture::tree();
        let mut app = App::new(fixture::data(&tree)).unwrap();
        app.assets = fixture::assets();
        let (mut input, mut audio) = (Input::new(), FakeAudio::default());
        assert!(matches!(app.state(), AppState::Loaded { .. }));
        // The clock starts at the first update.
        app.update(&tree, 5000.0, &input, &mut audio);
        assert!(matches!(app.state(), AppState::Startup { clock: 0, .. }));
        // Keys other than quit do nothing; a tap counts as quit.
        press(&mut input, Key::Enter);
        app.update(&tree, 5100.0, &input, &mut audio);
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
        app.update(&tree, 5200.0, &input, &mut audio);
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
        app.update(&tree, 5300.0, &input, &mut audio);
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
        press(&mut input, Key::Q);
        app.update(&tree, 5400.0, &input, &mut audio);
        assert!(app.quit_requested());
        assert!(audio.playing.is_empty());
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
        app.update(&tree, 0.0, &input, &mut audio);
        // Section 6.4: the volumes at once (built-in effects volume 1).
        assert_eq!(
            audio.volumes,
            [(Category::Effects, 1.0), (Category::Music, 0.5)]
        );
        for _ in 0..5 {
            press(&mut input, Key::Escape);
            app.update(&tree, 10.0, &input, &mut audio);
            input.end_frame();
        }
        // Options (item 3): the fixture has no options layout, so the menu
        // stays, saying why.
        for _ in 0..3 {
            press(&mut input, Key::Down);
            app.update(&tree, 20.0, &input, &mut audio);
            input.end_frame();
        }
        press(&mut input, Key::Enter);
        app.update(&tree, 30.0, &input, &mut audio);
        input.end_frame();
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("not in the menu")
        };
        assert!(menu.options.is_none());
        assert!(menu.error.unwrap().contains("ui_mm_opt.xml"));
        // Quitting stores the settings.
        press(&mut input, Key::Q);
        app.update(&tree, 40.0, &input, &mut audio);
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
        app.update(&tree, 0.0, &input, &mut audio);
        for _ in 0..5 {
            press(&mut input, Key::Escape);
            app.update(&tree, 10.0, &input, &mut audio);
            input.end_frame();
        }
        assert!(matches!(app.state(), AppState::Menu { .. }));
        // New game, then the first difficulty.
        for t in [20.0, 30.0] {
            press(&mut input, Key::Enter);
            app.update(&tree, t, &input, &mut audio);
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
        app.update(&tree, 40.0, &input, &mut audio);
        assert_eq!(caption(&app), (1, "Кэширование объектов...".into()));
        let scene = app.scene(&tree, res).unwrap();
        assert!(scene.first("background").is_none(), "menu gone");
        assert_eq!(scene.layer("loading strip").count(), 40);
        app.update(&tree, 50.0, &input, &mut audio);
        assert_eq!(caption(&app), (2, "Сервер: Старт...".into()));
        app.update(&tree, 60.0, &input, &mut audio);
        app.update(&tree, 70.0, &input, &mut audio);
        assert_eq!(caption(&app), (4, "st_creating_new_game...".into()));
        // The fixture has no spawn file: back to the menu, saying why.
        app.update(&tree, 80.0, &input, &mut audio);
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("not in the menu: {:?}", app.state())
        };
        assert!(menu.error.clone().unwrap().contains("spawn"), "error");
        assert_eq!(menu.menu.list, "menu_main");
        assert!(audio.master_gains.is_empty(), "never muted");

        // A later New Game of the same type skips prefetching.
        for t in [90.0, 100.0, 110.0] {
            press(&mut input, Key::Enter);
            app.update(&tree, t, &input, &mut audio);
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
        let (input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 0.0, &input, &mut audio);
        let controls = app.data.controls.clone();
        app.phase = Some(Phase::Level {
            level: Box::new(LevelScreen::empty(controls, 0)),
            overlay: None,
            intro: Intro::default(),
            menu: None,
            inventory: None,
            dropped: Vec::new(),
        });
        app.update(&tree, 16.0, &input, &mut audio);
        assert!(app.level_controls().is_some(), "the controls are live");
        assert!(!app.cursor_shown(), "no cursor in the level");
        (tree, app, input, audio)
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
        let (input, mut audio) = (Input::new(), FakeAudio::default());
        app.update(&tree, 0.0, &input, &mut audio);
        let mut controls = app.data.controls.clone();
        controls.bindings = crate::controls::Bindings::read(&[
            b"bind inventory kI\nbind forward kW\nbind wpn_fire mouse1\nbind drop kG\n",
        ]);
        let character = crate::inventory_screen::tests::character(&[("bread", 5, 3)]);
        let inventory =
            InventoryScreen::load(&tree, &app.data, Resolution::new(1024, 768)).unwrap();
        app.phase = Some(Phase::Level {
            level: Box::new(LevelScreen::empty(controls, 0).with_character(character)),
            overlay: None,
            intro: Intro::default(),
            menu: None,
            inventory: Some(Box::new(inventory)),
            dropped: Vec::new(),
        });
        app.update(&tree, 16.0, &input, &mut audio);
        (tree, app, input, audio)
    }

    #[test]
    fn the_inventory_key_opens_the_screen_over_the_running_level() {
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        let game_time = |app: &App| match app.state() {
            AppState::Level { level, .. } => level.game_time,
            _ => panic!(),
        };
        press(&mut input, Key::I);
        app.update(&tree, 32.0, &input, &mut audio);
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
        app.update(&tree, 132.0, &input, &mut audio);
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
        app.update(&tree, 148.0, &input, &mut audio);
        input.end_frame();
        assert!(!app.inventory_open() && !menu_open(&app) && !app.cursor_shown());
        assert!(app.level_controls().is_some());
        assert!(audio.started.last().unwrap().1.ends_with("inv_close.ogg"));
        // Escape again opens the menu as usual.
        press(&mut input, Key::Escape);
        app.update(&tree, 164.0, &input, &mut audio);
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
        app.update(&tree, 32.0, &input, &mut audio);
        input.end_frame();
        ends_saying_why(&app);
        // While open: an item without them enters the inventory, which
        // rebuilds the screen (4.3).
        let (tree, mut app, mut input, mut audio) = in_level_with_inventory();
        press(&mut input, Key::I);
        app.update(&tree, 32.0, &input, &mut audio);
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
        app.update(&tree, 48.0, &input, &mut audio);
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
        app.update(&tree, 32.0, &input, &mut audio);
        input.end_frame();
        assert!(app.inventory_open());
        // The inventory key closes it.
        press(&mut input, Key::I);
        app.update(&tree, 48.0, &input, &mut audio);
        input.end_frame();
        assert!(!app.inventory_open());
        // A gamepad's north button opens it (our design).
        for pressed in [true, false] {
            input.handle(Event::GamepadButton {
                button: crate::core::input::GamepadButton::North,
                pressed,
            });
        }
        app.update(&tree, 64.0, &input, &mut audio);
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
        app.update(&tree, 80.0, &input, &mut audio);
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
        app.update(&tree, 96.0, &input, &mut audio);
        assert!(!app.inventory_open());
        // Dead, the key opens nothing.
        press(&mut input, Key::I);
        app.update(&tree, 112.0, &input, &mut audio);
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

    #[test]
    fn a_lost_pointer_lock_opens_the_menu_and_its_escape_does_not_close_it() {
        let (tree, mut app, mut input, mut audio) = in_level();
        input.handle(Event::PointerUnlocked);
        app.update(&tree, 32.0, &input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app), "the unlock opened the menu");
        assert!(app.level_controls().is_none());
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert!(app.cursor_shown() && scene.first("cursor").is_some());
        // The Escape the browser used to leave the lock, arriving a frame
        // later, leaves it open.
        press(&mut input, Key::Escape);
        app.update(&tree, 48.0, &input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app), "the following Escape is ignored");
        // A later Escape closes it.
        app.update(&tree, 64.0, &input, &mut audio);
        press(&mut input, Key::Escape);
        app.update(&tree, 80.0, &input, &mut audio);
        input.end_frame();
        assert!(!menu_open(&app), "a later Escape returns to the game");
        // Escape opens it too, and closes it the next frame (no unlock).
        press(&mut input, Key::Escape);
        app.update(&tree, 96.0, &input, &mut audio);
        input.end_frame();
        assert!(menu_open(&app));
        press(&mut input, Key::Escape);
        app.update(&tree, 112.0, &input, &mut audio);
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
        app.update(&tree, 1016.0, &input, &mut audio);
        let (clock, time) = (app.clock(), game_time(&app));
        assert!(time > 0, "the game clock runs");
        app.set_hidden(true, &input, &mut audio);
        assert!(menu_open(&app), "hiding opened the menu");
        assert!(app.level_controls().is_none());
        // Updates while hidden (a platform still calling) do nothing.
        app.update(&tree, 30_000.0, &input, &mut audio);
        assert_eq!((app.clock(), game_time(&app)), (clock, time));
        // The first frame back has a zero delta, then the clock runs on;
        // the menu's pause holds the game clock. An Escape in the first two
        // frames back (one the browser used to leave pointer lock) leaves
        // the menu open.
        app.set_hidden(false, &input, &mut audio);
        press(&mut input, Key::Escape);
        app.update(&tree, 600_000.0, &input, &mut audio);
        input.end_frame();
        assert_eq!(app.clock(), clock, "the hidden time is not counted");
        assert!(menu_open(&app), "the guarded Escape is ignored");
        app.update(&tree, 600_016.0, &input, &mut audio);
        assert_eq!(app.clock(), clock + 16);
        assert_eq!(game_time(&app), time, "paused under the menu");
        press(&mut input, Key::Escape);
        app.update(&tree, 600_032.0, &input, &mut audio);
        input.end_frame();
        assert!(!menu_open(&app), "a later Escape returns to the game");
        app.update(&tree, 601_064.0, &input, &mut audio);
        // About a second shown ran, at the game's time factor; the ~10
        // minutes hidden did not.
        let ran = game_time(&app) - time;
        assert!(ran < 60_000, "only shown time ran: {ran} ms of game time");
    }

    #[test]
    fn hiding_leaves_an_open_menu_and_opens_nothing_outside_the_level() {
        // The menu already open, its selection moved: left as it is.
        let (tree, mut app, mut input, mut audio) = in_level();
        press(&mut input, Key::Escape);
        app.update(&tree, 32.0, &input, &mut audio);
        input.end_frame();
        press(&mut input, Key::Down);
        app.update(&tree, 48.0, &input, &mut audio);
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
        app.update(&tree, 0.0, &input, &mut audio);
        app.update(&tree, 100.0, &input, &mut audio);
        app.set_hidden(true, &input, &mut audio);
        app.update(&tree, 5000.0, &input, &mut audio);
        app.set_hidden(false, &input, &mut audio);
        app.update(&tree, 9000.0, &input, &mut audio);
        assert!(matches!(app.state(), AppState::Startup { clock: 100, .. }));

        // Under the loading image and during the intro: nothing opens.
        let (tree, mut app, input, mut audio) = in_level();
        let Some(Phase::Level { overlay, .. }) = &mut app.phase else {
            panic!()
        };
        *overlay = Some(Overlay::new(LoadingScreen::default(), &mut audio));
        app.set_hidden(true, &input, &mut audio);
        assert!(!menu_open(&app), "not under the loading image");
        let (tree2, mut app, input, mut audio) = in_level();
        let Some(Phase::Level { intro, .. }) = &mut app.phase else {
            panic!()
        };
        intro.checked = true;
        intro.player = Some(MoviePlayer::new(16));
        app.set_hidden(true, &input, &mut audio);
        assert!(!menu_open(&app), "not during the intro");
        app.set_hidden(false, &input, &mut audio);
        app.update(&tree2, 50_000.0, &input, &mut audio);
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
            app.update(&tree, t, &input, &mut audio);
            input.end_frame();
            t += 16.0;
            assert!(!menu_open(&app), "not under the loading image");
        }
        // The image's last frame ends: the controls are live, the menu opens.
        app.update(&tree, t, &input, &mut audio);
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
        let (tree, mut app, input, mut audio) = in_level();
        let Some(Phase::Level { level, .. }) = &mut app.phase else {
            panic!()
        };
        level.record_fatal("lamp: no renderer type set for hanging-lamp".into());
        app.update(&tree, 32.0, &input, &mut audio);
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("{:?}", app.state())
        };
        assert!(menu.error.unwrap().contains("hanging-lamp"));
    }

    /// `App::fatal`: the title menu that already says why, failing again
    /// with the same error, stays as it is; another error opens it anew.
    #[test]
    fn the_menu_saying_why_is_not_opened_again_for_the_same_error() {
        let (tree, mut app, input, mut audio) = in_level();
        let error = "x: fatal".to_owned();
        app.failed = Some(error.clone());
        app.update(&tree, 32.0, &input, &mut audio);
        assert!(matches!(app.state(), AppState::Menu { .. }));
        let opened = audio.started.len();
        assert!(opened > 0, "the menu's music started");
        for t in [48.0, 64.0] {
            app.failed = Some(error.clone());
            app.update(&tree, t, &input, &mut audio);
        }
        assert_eq!(audio.started.len(), opened, "not opened again");
        app.failed = Some("y: fatal".into());
        app.update(&tree, 80.0, &input, &mut audio);
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
        let (tree, mut app, input, mut audio) = in_level();
        app.data.intro_game = None;
        let Some(Phase::Level { overlay, .. }) = &mut app.phase else {
            panic!()
        };
        let mut o = Overlay::new(LoadingScreen::default(), &mut audio);
        o.frames_left = 4;
        *overlay = Some(o);
        // Three and two frames left after this one: not yet due.
        app.update(&tree, 32.0, &input, &mut audio);
        assert!(matches!(app.state(), AppState::Level { .. }));
        app.update(&tree, 48.0, &input, &mut audio);
        let AppState::Menu { menu, .. } = app.state() else {
            panic!("{:?}", app.state())
        };
        assert!(menu.error.unwrap().contains("intro_game"));
    }

    #[test]
    fn a_setting_changed_in_the_menu_reaches_the_running_level() {
        let (tree, mut app, mut input, mut audio) = in_level();
        press(&mut input, Key::Escape);
        app.update(&tree, 32.0, &input, &mut audio);
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
        app.update(&tree, 48.0, &input, &mut audio);
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
