// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game from launch to the title menu (docs/behavior/startup.md): the
//! logo sequence, then the title menu, driven by the platform's clock and
//! input, drawing [`Scene`]s and playing sounds through [`AudioOut`].
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
use crate::core::vfs::{self, Vfs};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::level::{LevelScreen, LevelState};
use crate::loading_screen::{
    GAME_TYPE, Loading, LoadingScreen, LoadingScreenState, OVERLAY_FRAMES, SERVER_NEW_GAME,
    intro_due,
};
use crate::options::{OptionsData, OptionsScreen, OptionsState};
use crate::scene::{Scene, Sprite, TextureId};
use crate::startup::{MoviePlayer, MovieSequenceState, Startup};
use crate::title_menu::{TitleMenu, TitleMenuState};
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
    /// `pause_state` is on.
    fn frame(
        &mut self,
        data: &GameData,
        now: u64,
        overlay_frames_left: Option<u32>,
        quit_presses: u32,
        audio: &mut dyn AudioOut,
    ) -> bool {
        if !self.checked
            && let Some(left) = overlay_frames_left
            && intro_due(true, left, SERVER_NEW_GAME)
        {
            self.checked = true;
            // Without a measured `intro_game` there is none to play (our
            // choice).
            if data.intro_game().is_some() {
                self.player = Some(MoviePlayer::new(now));
            }
        }
        let (Some(player), Some(seq)) = (self.player.as_mut(), data.intro_game()) else {
            return false;
        };
        player.update(seq, data, now, quit_presses, audio);
        if player.finished(seq) {
            // Section 2.3, step 4: then the level is visible.
            self.player = None;
            return false;
        }
        player.pauses(seq)
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

/// See [`App::level_controls`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelControls {
    pub crouch: bool,
    pub sprint: bool,
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
    /// Why the latest new game failed to load.
    error: Option<String>,
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
    /// The options screen's layout, loaded when it first opens.
    options_data: Option<OptionsData>,
    /// The options screen of the current menu screen: made on its first
    /// opening after each opening of the menu (section 2.1).
    options: Option<OptionsScreen>,
    options_open: bool,
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
    /// The console may have changed this frame.
    console_touched: bool,
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
        menu: TitleMenuState,
        /// The options screen while it shows.
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<Box<OptionsState>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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
        level: LevelState,
        load_ms: Option<f64>,
        /// Frames the loading image still covers, this one included.
        overlay_frames: u32,
        /// The New Game intro while it plays.
        #[serde(skip_serializing_if = "Option::is_none")]
        intro: Option<MovieSequenceState>,
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
            options_data: None,
            options: None,
            options_open: false,
            native: video,
            video,
            save: None,
            volumes: None,
            console_touched: false,
            data,
            assets: Assets::default(),
            origin: None,
            clock: 0,
            phase: None,
            error: None,
            game_type: None,
            load_started: None,
            loaded_since: None,
            load_ms: None,
        })
    }

    pub fn data(&self) -> &GameData {
        &self.data
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
        if let Some(at) = self.loaded_since.take() {
            self.load_ms = Some(time_ms - at);
        }
        let origin = *self.origin.get_or_insert(time_ms);
        self.clock = self.clock.max((time_ms - origin).max(0.0) as u64);
        let now = self.clock;
        let data = &self.data;
        let phase = self
            .phase
            .get_or_insert_with(|| Phase::Startup(Startup::new(now)));
        match phase {
            Phase::Startup(startup) => {
                startup.update(data, now, quit_presses(data, input), audio);
                if startup.finished(data) {
                    // Shown within one frame of the sequence's end.
                    *phase = Phase::Menu(TitleMenu::open(data, now, input, audio));
                    self.options = None;
                }
            }
            Phase::Menu(menu) if self.options_open => {
                // The title menu is hidden meanwhile (title-menu.md 4.5);
                // its music plays on.
                let screen = self.options.as_mut().expect("open options");
                screen.update(data, &mut self.console, self.native, now, input);
                self.console_touched = true;
                menu.update(data, now, &Input::new(), audio);
                if let Some(closed) = screen.take_closed() {
                    self.options_open = false;
                    if closed.video_restart {
                        // `vid_restart` (section 5.4).
                        self.video = video_mode(&self.console);
                        menu.resized(data, self.video);
                    }
                    // Stored now, not only at exit (our choice: a browser
                    // tab is usually closed without the game's shutdown).
                    self.save = Some(self.console.save());
                }
            }
            Phase::Menu(menu) => {
                menu.update(data, now, input, audio);
                if menu.take_options() {
                    let opened = self.open_options(tree, now, input);
                    if let Err(e) = opened {
                        // Back on the menu, saying why (our choice).
                        self.error = Some(format!("options: {e}"));
                    }
                    self.after_update(audio);
                    return;
                }
                if menu.quit_requested() {
                    self.save = Some(self.console.save());
                }
                if let Some(difficulty) = menu.take_new_game() {
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
                        *stages = NewGame::Stages(Box::new(Loading::start(data, changed)));
                        Ok(None)
                    }
                    NewGame::Stages(loading) => loading
                        .step(tree, data, now)
                        .map(|level| level.map(|l| (l, loading.screen().clone()))),
                };
                match step {
                    Ok(None) => {}
                    Ok(Some((level, screen))) => {
                        self.error = None;
                        self.loaded_since = self.load_started.take();
                        *phase = Phase::Level {
                            level: Box::new(level),
                            overlay: Some(Overlay::new(screen, audio)),
                            intro: Intro::default(),
                        };
                    }
                    Err(e) => {
                        // Back to the menu, saying why (our choice).
                        self.error = Some(e.to_string());
                        *phase = Phase::Menu(TitleMenu::open(data, now, input, audio));
                        self.options = None;
                    }
                }
            }
            Phase::Level {
                level,
                overlay,
                intro,
            } => {
                if let Some(o) = overlay.take() {
                    *overlay = o.next_frame(audio);
                }
                let left = overlay.as_ref().map(|o| o.frames_left);
                // While the intro plays, including the frame that ends it,
                // input goes to it alone (our choice; the level is paused
                // under it anyway).
                let playing = intro.player.is_some();
                if intro.frame(data, now, left, quit_presses(data, input), audio) {
                    level.paused(now);
                    return;
                }
                if playing || intro.player.is_some() {
                    // An item without `pause_state`, or the intro's end: the
                    // level runs without this frame's input.
                    level.update(now, &Input::new());
                    return;
                }
                level.update(now, input);
                if level.back_requested() {
                    if let Some(o) = overlay.take() {
                        o.end(audio);
                    }
                    *phase = Phase::Menu(TitleMenu::open(data, now, input, audio));
                    self.options = None;
                }
            }
        }
        self.after_update(audio);
    }

    /// Opens the options screen (title-menu.md 4.5; options-screens.md 2.1),
    /// loading its layout the first time.
    fn open_options(&mut self, tree: &dyn Vfs, now: u64, input: &Input) -> io::Result<()> {
        if self.options_data.is_none() {
            self.options_data = Some(OptionsData::load(tree, &self.data, self.video)?);
        }
        if self.options.is_none() {
            let opt = self.options_data.as_ref().expect("loaded");
            self.options = Some(OptionsScreen::new(&self.data, opt)?);
        }
        let screen = self.options.as_mut().expect("made");
        screen.open(
            &self.data,
            &mut self.console,
            self.native,
            now,
            input.cursor_ui(),
        );
        self.options_open = true;
        self.error = None;
        Ok(())
    }

    /// What the settings changed this frame do: the bindings and movement
    /// settings the next level uses, and the sound volumes at once.
    fn after_update(&mut self, audio: &mut dyn AudioOut) {
        if std::mem::take(&mut self.console_touched) || self.options_open {
            let generation = self.console.generation();
            if generation != self.applied {
                self.data.apply(&self.console);
                self.applied = generation;
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

    /// The frame at the current clock for `resolution`.
    pub fn scene(&mut self, tree: &dyn Vfs, resolution: Resolution) -> io::Result<Scene> {
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
            Some(Phase::Menu(_)) if self.options_open => {
                self.options.as_ref().expect("open options").scene(&mut ctx)
            }
            Some(Phase::Menu(m)) => m.scene(&mut ctx),
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
                overlay,
                intro,
            }) => compose(
                level.scene(resolution, self.clock),
                intro,
                overlay.as_ref(),
                &mut ctx,
            ),
        }
    }

    /// While the player's controls are live (the level shows with no
    /// loading image or intro over it): whether the crouch toggle is engaged
    /// and a sprint is requested, which the touch controls show.
    pub fn level_controls(&self) -> Option<LevelControls> {
        match &self.phase {
            Some(Phase::Level {
                level,
                overlay: None,
                intro,
            }) if intro.player.is_none() => {
                let p = level.player().state();
                Some(LevelControls {
                    crouch: p.crouch_toggled,
                    sprint: p.sprint_requested,
                })
            }
            _ => None,
        }
    }

    pub fn quit_requested(&self) -> bool {
        matches!(&self.phase, Some(Phase::Menu(m)) if m.quit_requested())
    }

    /// Stops every sound the app started.
    pub fn stop(&mut self, audio: &mut dyn AudioOut) {
        match &mut self.phase {
            Some(Phase::Startup(s)) => s.stop(audio),
            Some(Phase::Menu(m)) => m.stop(audio),
            Some(Phase::Level { overlay, intro, .. }) => {
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
            Some(Phase::Menu(m)) => AppState::Menu {
                clock,
                menu: m.state(&self.data, clock),
                options: self
                    .options
                    .as_ref()
                    .filter(|_| self.options_open)
                    .map(|o| Box::new(o.state(&self.data))),
                error: self.error.clone(),
            },
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
            }) => AppState::Level {
                clock,
                level: level.state(),
                load_ms: self.load_ms,
                overlay_frames: overlay.as_ref().map_or(0, |o| o.frames_left),
                intro: intro.state(&self.data, clock),
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
        // Escape ends the last item: the menu shows in the same frame.
        press(&mut input, Key::Escape);
        app.update(&tree, 5300.0, &input, &mut audio);
        input.end_frame();
        let AppState::Menu { clock, menu, .. } = app.state() else {
            panic!()
        };
        assert_eq!((clock, menu.list, menu.music.starts), (300, "menu_main", 1));
        let scene = app.scene(&tree, Resolution::new(1024, 768)).unwrap();
        assert!(scene.first("background").is_some());
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
        let AppState::Menu { options, error, .. } = app.state() else {
            panic!("not in the menu")
        };
        assert!(options.is_none());
        assert!(error.unwrap().contains("ui_mm_opt.xml"));
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
        let AppState::Menu { error, menu, .. } = app.state() else {
            panic!("not in the menu: {:?}", app.state())
        };
        assert!(error.unwrap().contains("spawn"), "error");
        assert_eq!(menu.list, "menu_main");
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
            let paused = intro.frame(&data, frame as u64 * 16, Some(left), 0, &mut audio);
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
        assert!(intro.frame(&data, start + 2999, None, 0, &mut audio));
        assert!(!intro.frame(&data, start + 3000, None, 0, &mut audio));
        assert!(intro.player.is_none());
        assert!(audio.playing.is_empty(), "its sounds ended");
        assert!(!intro.frame(&data, 4000, Some(1), 0, &mut audio));
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
        intro.frame(&data, 0, Some(2), 0, &mut audio);
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
        assert!(intro.frame(&data, 0, Some(2), 0, &mut audio));
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
        assert!(!intro.frame(&data, 32, Some(1), 1, &mut audio));
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

    #[test]
    fn splash_is_centred_one_to_one() {
        let s = splash_scene(Resolution::new(1920, 1080), (500, 268));
        assert_eq!(s.sprites[0].bounds(), [709.5, 405.5, 1209.5, 673.5]);
        assert_eq!(s.sprites[0].texture, TextureId::Splash);
    }
}
