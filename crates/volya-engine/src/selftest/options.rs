// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The options screen (once `tests/options.rs`'s drawing;
//! docs/behavior/options-screens.md), drawn without errors: the video page
//! as it opens, the advanced page after the low preset is chosen and
//! scrolled to the bottom, the controls tab (with the cost of its frames,
//! section 8.8) and the renderer list open with its disabled rows (#364).
//! Frames `options`, `options-advanced`, `options-controls` and
//! `options-renderers`.
//!
//! The browser's tree holds the archives only, not the installer's
//! `user.ltx` the native test reads, so the console starts as the game
//! does without a user configuration file (options-screens.md 12.2).

use super::{Context, Outcome, Run, capture, ensure, ensure_eq, now_ms};
use crate::core::console::Console;
use crate::core::input::{Event, Input, Key, MouseButton};
use crate::data::{Assets, GameData};
use crate::draw::Ctx;
use crate::options::{ControlState, OptionsData, OptionsScreen, OptionsState, Page};
use crate::present::Presenter;
use crate::render::Renderer;
use volya_engine_formats::ui::Resolution;

pub const CHECKS: &[(&str, Run)] = &[
    ("options screen draws", |c| Box::pin(video_page_draws(c))),
    ("advanced options draw", |c| {
        Box::pin(advanced_page_draws(c))
    }),
    ("controls tab draws", |c| Box::pin(controls_tab_draws(c))),
    ("renderer list draws", |c| Box::pin(renderer_list_draws(c))),
];

const XGA: Resolution = Resolution::new(1024, 768);

struct Walk<'a> {
    cx: &'a Context<'a>,
    data: &'a GameData,
    screen: OptionsScreen,
    console: Console,
    input: Input,
    now: u64,
    r: Renderer,
    presenter: Presenter,
    assets: Assets,
}

impl<'a> Walk<'a> {
    /// The screen opened at 1000 ms on the video page.
    fn open(cx: &'a Context<'a>, data: &'a GameData) -> Result<Self, String> {
        let opt = OptionsData::load(cx.tree, data, XGA).map_err(|e| format!("options: {e}"))?;
        let screen = OptionsScreen::new(data, &opt).map_err(|e| format!("screen: {e}"))?;
        let mut walk = Self {
            cx,
            data,
            screen,
            console: data.console(),
            input: Input::new(),
            now: 1000,
            r: cx.renderer(),
            presenter: Presenter::new(),
            assets: Assets::default(),
        };
        walk.screen
            .open(data, &mut walk.console, XGA, walk.now, [512.0, 384.0]);
        Ok(walk)
    }

    fn frame(&mut self) {
        self.now += 16;
        self.screen
            .update(self.data, &mut self.console, XGA, self.now, &self.input);
        self.input.end_frame();
    }

    fn click(&mut self, [x, y]: [f32; 2]) {
        self.input.handle(Event::PointerMoved { at: [x, y] });
        for pressed in [true, false] {
            self.input.handle(Event::PointerButton {
                at: None,
                button: MouseButton::Left,
                pressed,
                time: self.now as f64 * 10.0,
            });
            self.frame();
        }
    }

    fn state(&self) -> OptionsState {
        self.screen.state(self.data)
    }

    fn control(&self, name: &str) -> Result<ControlState, String> {
        self.state()
            .controls
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| format!("no {name}"))
    }

    fn button(&self, name: &str) -> Result<[f32; 2], String> {
        let b = self
            .state()
            .buttons
            .into_iter()
            .find(|b| b.name == name)
            .ok_or_else(|| format!("no {name}"))?;
        Ok([b.rect[0] + b.rect[2] / 2.0, b.rect[1] + b.rect[3] / 2.0])
    }

    /// The screen's draw list, without errors.
    fn draw_list(&mut self) -> Result<crate::render::DrawList, String> {
        let mut ctx = Ctx {
            tree: self.cx.tree,
            data: self.data,
            assets: &mut self.assets,
            resolution: XGA,
            clock_ms: self.now,
        };
        let scene = self
            .screen
            .scene(&mut ctx)
            .map_err(|e| format!("scene: {e}"))?;
        let list = self
            .presenter
            .draw_list(&mut self.r, self.cx.tree, &scene, self.now);
        let errors = self.presenter.take_errors();
        if !errors.is_empty() {
            return Err(format!("{errors:?}"));
        }
        Ok(list)
    }

    /// The screen drawn without errors, kept as frame `name`.
    async fn draw(&mut self, name: &str) -> Outcome {
        let list = self.draw_list()?;
        let pixels = capture(&mut self.r, &list, XGA.width, XGA.height).await?;
        self.cx.dump(name, XGA.width, XGA.height, &pixels);
        Ok(String::new())
    }
}

fn centre(r: [f32; 4]) -> [f32; 2] {
    [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]
}

fn game_data(cx: &Context<'_>) -> Result<GameData, String> {
    GameData::read(cx.tree, None, XGA).map_err(|e| format!("game data: {e}"))
}

/// The video page as the screen opens: every texture loads.
pub async fn video_page_draws(cx: &Context<'_>) -> Outcome {
    let data = game_data(cx)?;
    let mut walk = Walk::open(cx, &data)?;
    ensure_eq!(walk.state().page, Page::Video);
    walk.draw("options").await
}

/// The preset list: "низкие" chosen (section 8), then the advanced page
/// scrolled to the bottom (78 units, section 4.8).
pub async fn advanced_page_draws(cx: &Context<'_>) -> Outcome {
    let data = game_data(cx)?;
    let mut walk = Walk::open(cx, &data)?;
    walk.click(centre(walk.control("list_presets")?.rect));
    let open = walk.state().open_list;
    ensure_eq!(open.as_deref(), Some("list_presets"), "the preset list");
    let r = walk.control("list_presets")?.rect;
    walk.click([r[0] + 20.0, r[1] + 23.0 + 18.0 + 9.0]);
    let preset = walk.console.status("_preset");
    ensure_eq!(preset.as_deref(), Some("Low"));
    walk.click(walk.button("btn_advanced")?);
    ensure_eq!(walk.state().page, Page::Advanced);
    for _ in 0..3 {
        walk.input
            .handle(Event::PointerMoved { at: [700.0, 400.0] });
        walk.input.handle(Event::Wheel { steps: 1 });
        walk.frame();
    }
    ensure_eq!(walk.state().scroll[0], 78, "scrolled");
    walk.draw("options-advanced").await
}

/// The controls tab (the key table), and the cost of its frames
/// (section 8.8): update, scene and draw list.
pub async fn controls_tab_draws(cx: &Context<'_>) -> Outcome {
    let data = game_data(cx)?;
    let mut walk = Walk::open(cx, &data)?;
    walk.click(walk.button("tab 3")?);
    ensure!(
        walk.state()
            .cells
            .iter()
            .any(|c| c.action == "forward" && c.slot == 0),
        "the key cells"
    );
    walk.draw("options-controls").await?;
    let start = now_ms();
    let frames = 100;
    for _ in 0..frames {
        walk.input
            .handle(Event::PointerMoved { at: [700.0, 450.0] });
        walk.frame();
        walk.draw_list()?;
    }
    let ms = (now_ms() - start) / f64::from(frames);
    walk.input.handle(Event::Key {
        key: Key::Escape,
        pressed: true,
    });
    walk.frame();
    Ok(format!(
        "options frame (controls tab: update, scene, draw list): {ms:.3} ms"
    ))
}

/// The renderer list open on the video page: its dynamic entries are
/// disabled (#364, section 4.6's disabled rows).
pub async fn renderer_list_draws(cx: &Context<'_>) -> Outcome {
    let data = game_data(cx)?;
    let mut walk = Walk::open(cx, &data)?;
    walk.click(walk.button("tab 0")?);
    walk.click(centre(walk.control("list_renderer")?.rect));
    let open = walk.state().open_list;
    ensure_eq!(open.as_deref(), Some("list_renderer"), "the renderer list");
    walk.draw("options-renderers").await
}
