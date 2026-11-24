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
use crate::core::input::{Action, Control, Input};
use crate::core::vfs::{self, Vfs};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::level::{LevelScreen, LevelState};
use crate::scene::{Scene, Sprite, TextureId};
use crate::startup::{Startup, StartupState};
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
    /// A new game's level loads on the next update, after one frame of
    /// this plain loading state (our own; the game's loading screen is
    /// level-loading.md section 2.4).
    Loading {
        difficulty: &'static str,
    },
    Level(Box<LevelScreen>),
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
    /// Platform time of the update that loaded the level, until the next.
    loaded_at: Option<f64>,
    /// Platform milliseconds from the update that loaded the level to the
    /// next one: loading and the first frame drawn.
    load_ms: Option<f64>,
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
        startup: StartupState,
    },
    Menu {
        clock: u64,
        menu: TitleMenuState,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Loading {
        clock: u64,
        difficulty: &'static str,
    },
    Level {
        clock: u64,
        level: LevelState,
        load_ms: Option<f64>,
    },
}

impl App {
    pub fn new(data: GameData) -> io::Result<Self> {
        TitleMenu::check(&data)?;
        Ok(Self {
            data,
            assets: Assets::default(),
            origin: None,
            clock: 0,
            phase: None,
            error: None,
            loaded_at: None,
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
        if let Some(at) = self.loaded_at.take() {
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
                // Section 6.5: each press of a key bound to quit skips one
                // item; a touch counts as one (our design for touch).
                let presses = input
                    .actions()
                    .iter()
                    .filter(|a| {
                        matches!(a, Action::Press { control: Control::Key(k), .. }
                            if data.quit_keys.contains(k))
                    })
                    .count() as u32
                    + input.touch_starts();
                startup.update(data, now, presses, audio);
                if startup.finished(data) {
                    // Shown within one frame of the sequence's end.
                    *phase = Phase::Menu(TitleMenu::open(data, now, input, audio));
                }
            }
            Phase::Menu(menu) => {
                menu.update(data, now, input, audio);
                if let Some(difficulty) = menu.take_new_game() {
                    menu.stop(audio);
                    *phase = Phase::Loading { difficulty };
                }
            }
            Phase::Loading { .. } => match LevelScreen::load(tree, data, now) {
                Ok(level) => {
                    self.error = None;
                    self.loaded_at = Some(time_ms);
                    *phase = Phase::Level(Box::new(level));
                }
                Err(e) => {
                    // Back to the menu, saying why (our choice).
                    self.error = Some(e.to_string());
                    *phase = Phase::Menu(TitleMenu::open(data, now, input, audio));
                }
            },
            Phase::Level(level) => {
                level.update(now, input);
                if level.back_requested() {
                    *phase = Phase::Menu(TitleMenu::open(data, now, input, audio));
                }
            }
        }
    }

    /// The output was resized (a video device reset).
    pub fn resized(&mut self, resolution: Resolution) {
        if let Some(Phase::Menu(menu)) = &mut self.phase {
            menu.resized(&self.data, resolution);
        }
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
            Some(Phase::Menu(m)) => m.scene(&mut ctx),
            Some(Phase::Loading { .. }) => Ok(Scene::new(resolution)),
            Some(Phase::Level(l)) => Ok(l.scene(resolution, self.clock)),
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
            Some(Phase::Loading { .. } | Phase::Level(_)) | None => {}
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
                error: self.error.clone(),
            },
            Some(Phase::Loading { difficulty }) => AppState::Loading { clock, difficulty },
            Some(Phase::Level(l)) => AppState::Level {
                clock,
                level: l.state(),
                load_ms: self.load_ms,
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
        let AppState::Loading { difficulty, .. } = app.state() else {
            panic!("not loading: {:?}", app.state())
        };
        assert_eq!(difficulty, "gd_novice");
        assert!(audio.playing.is_empty(), "the menu music stopped");
        assert!(
            app.scene(&tree, Resolution::new(64, 48))
                .unwrap()
                .sprites
                .is_empty()
        );
        // The fixture has no spawn file: back to the menu, saying why.
        app.update(&tree, 40.0, &input, &mut audio);
        let AppState::Menu { error, menu, .. } = app.state() else {
            panic!("not in the menu: {:?}", app.state())
        };
        assert!(error.unwrap().contains("spawn"), "error");
        assert_eq!(menu.list, "menu_main");
    }

    #[test]
    fn splash_is_centred_one_to_one() {
        let s = splash_scene(Resolution::new(1920, 1080), (500, 268));
        assert_eq!(s.sprites[0].bounds(), [709.5, 405.5, 1209.5, 673.5]);
        assert_eq!(s.sprites[0].texture, TextureId::Splash);
    }
}
