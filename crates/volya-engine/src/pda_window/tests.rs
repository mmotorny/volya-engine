// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The window on a layout with the spec's values (pda-window.md 2.1, 3.2),
//! captions 10 units per character: the tab bar's layout, switching tabs
//! and its portions, the marks, the clock and the gamepad (our design).

use super::*;
use crate::core::input::{Event, GamepadAxis, GamepadButton, MouseButton};
use crate::fixture::{self, MemVfs};
use volya_engine_formats::ui::ColorDefs;
use volya_engine_formats::xml;

/// `pda.xml` with the values of pda-window.md 1, 2.1, 3.2 (`off_button`
/// first, never read).
pub(crate) const PDA: &str = r#"<window>
<off_button x="0" y="0" width="1" height="1"><texture>ui\ui_pda_button</texture></off_button>
<main x="0" y="0" width="1024" height="768"/>
<background_static x="3" y="20" width="891" height="643"><texture>ui_PDA</texture></background_static>
<mbbackground_frame_line x="110" y="574" width="591" height="32"><texture>ui_string_01</texture></mbbackground_frame_line>
<timer_frame_line x="710" y="573" width="161" height="32"><texture>ui_string_01</texture>
<title x="10" y="7" width="140" height="18" complex_mode="0"><text font="letterica18" r="255" g="255" b="255"/></title>
</timer_frame_line>
<tab x="129" y="573" width="550" height="30">
<button x="0" y="7" width="50" height="20" hint="pda_btn_quests_hint"><text font="letterica18" align="l">pda_btn_quests</text>
<text_color><e r="127" g="127" b="127"/><t r="231" g="168" b="12"/><d r="181" g="170" b="148"/><h r="255" g="255" b="255"/></text_color></button>
<button x="65" y="7" width="40" height="20" hint="pda_btn_map_hint"><text font="letterica18" align="l">pda_btn_map</text>
<text_color><e r="127" g="127" b="127"/><t r="231" g="168" b="12"/><d r="181" g="170" b="148"/><h r="255" g="255" b="255"/></text_color></button>
<button x="120" y="7" width="60" height="20" hint="pda_btn_diary_hint"><text font="letterica18" align="l">pda_btn_diary</text>
<text_color><e r="127" g="127" b="127"/><t r="231" g="168" b="12"/><d r="181" g="170" b="148"/><h r="255" g="255" b="255"/></text_color></button>
<button x="195" y="7" width="60" height="20" hint="pda_btn_contacts_hint"><text font="letterica18" align="l">pda_btn_contacts</text>
<text_color><e r="127" g="127" b="127"/><t r="231" g="168" b="12"/><d r="181" g="170" b="148"/><h r="255" g="255" b="255"/></text_color></button>
<button x="275" y="7" width="53" height="20" hint="pda_btn_ranking_hint"><text font="letterica18" align="l">pda_btn_ranking</text>
<text_color><e r="127" g="127" b="127"/><t r="231" g="168" b="12"/><d r="181" g="170" b="148"/><h r="255" g="255" b="255"/></text_color></button>
<button x="345" y="7" width="78" height="20" hint="pda_btn_statistics_hint"><text font="letterica18" align="l">pda_btn_statistics</text>
<text_color><e r="127" g="127" b="127"/><t r="231" g="168" b="12"/><d r="181" g="170" b="148"/><h r="255" g="255" b="255"/></text_color></button>
<button x="440" y="7" width="100" height="20" hint="pda_btn_encyclopedia_hint"><text font="letterica18" align="l">pda_btn_encyclopedia</text>
<text_color><e r="127" g="127" b="127"/><t r="231" g="168" b="12"/><d r="181" g="170" b="148"/><h r="255" g="255" b="255"/></text_color></button>
</tab>
<updated_section_static x="0" y="0" width="9" height="11"><texture>ui_pda_m_noupdate</texture></updated_section_static>
<old_section_static x="0" y="0" width="9" height="11"><texture>ui_pda_m_update</texture></old_section_static>
</window>"#;

/// The captions' texts: the keys' lengths stand in for the strings.
fn text(key: &str) -> Vec<u8> {
    key.trim_start_matches("pda_btn_").as_bytes().to_vec()
}

/// Ten units per character.
fn width(_: volya_engine_formats::text::font::UiFont, t: &[u8]) -> f32 {
    10.0 * t.len() as f32
}

fn layout() -> Layout {
    let doc = xml::parse(PDA.as_bytes()).unwrap();
    Layout::read(&doc, &ColorDefs::default(), &text, &width).unwrap()
}

/// `hint_item.xml`'s `button_hint` (pda-controls.md 6).
const HINT: &str = r#"<window><button_hint x="0" y="0" width="200" height="30">
<frame_line x="0" y="0" width="200" height="30"><texture>ui_icons_PDA_string_tooltips</texture></frame_line>
<description x="10" y="2" width="192" height="20" light_anim="ui_btn_hint" la_cyclic="0" la_texture="0" la_text="1" la_alpha="1">
<text font="letterica18" r="255" g="255" b="255"/></description></button_hint></window>"#;

/// A synthetic `ui_btn_hint`: transparent to opaque over 10 frames at 10
/// per second (1 s), keys 0 and 10.
fn hint_desc() -> HintDesc {
    let animations = volya_engine_formats::lanims::ColourAnimations {
        version: 1,
        entries: vec![volya_engine_formats::lanims::Entry {
            name: b"ui_btn_hint".to_vec(),
            frame_rate: 10.0,
            frame_count: 10,
            keys: vec![(0, 0x00FF_0000), (10, 0xFF00_00FF)],
        }],
    };
    let doc = xml::parse(HINT.as_bytes()).unwrap();
    HintDesc::read(&doc, &ColorDefs::default(), &animations).unwrap()
}

pub(crate) fn window() -> PdaWindow {
    PdaWindow::new(layout(), hint_desc(), &|k| k.to_owned(), &width)
}

/// The keys of the game's default bindings (pda-window.md 4.1).
pub(crate) fn keys() -> Keys {
    Keys {
        actions: [vec![Key::P], vec![Key::M], vec![Key::H]],
        quit: vec![Key::Escape],
    }
}

pub(crate) struct Run {
    pub(crate) tree: MemVfs,
    pub(crate) window: PdaWindow,
    pub(crate) input: Input,
    pub(crate) now: u64,
    pub(crate) global_ms: u64,
    pub(crate) paused: bool,
    pub(crate) alive: bool,
    /// The player's level, position and yaw.
    pub(crate) player: Option<(String, [f32; 2], f32)>,
}

impl Run {
    pub(crate) fn new() -> Self {
        Self {
            tree: fixture::tree(),
            window: window(),
            input: Input::new(),
            now: 0,
            global_ms: 0,
            paused: false,
            alive: true,
            player: None,
        }
    }

    /// An open window on tab 0.
    fn open() -> Self {
        let mut r = Self::new();
        r.window.switch_to(0);
        r.window.open([0.0, 0.0]);
        r.window.take_portions();
        r
    }

    pub(crate) fn frame(&mut self, game_time: u64) -> Outcome {
        let data = fixture::data(&self.tree);
        self.now += 16;
        if !self.paused {
            self.global_ms += 16;
        }
        // The gamepad drives the cursor, as the app has it (#835).
        let targets = crate::dialog::cursor_targets(&self.window.cursor_targets());
        self.input.set_pad_cursor(true);
        let scrolls = !self.window.map_shown();
        (self.input).pad_cursor_frame(self.now as f64, &targets, scrolls);
        let out = self.window.update(Frame {
            data: &data,
            keys: &keys(),
            game_time,
            now: self.now,
            global_ms: self.global_ms,
            input: &self.input,
            paused: self.paused,
            alive: self.alive,
            player: self
                .player
                .as_ref()
                .map(|(level, position, yaw)| map::MapPlayer {
                    level,
                    position: *position,
                    yaw: *yaw,
                }),
        });
        self.input.end_frame();
        out
    }

    fn key(&mut self, key: Key) {
        for pressed in [true, false] {
            self.input.handle(Event::Key { key, pressed });
        }
    }

    fn click(&mut self, at: [f32; 2], time: f64) {
        self.input.handle(Event::PointerMoved { at });
        for pressed in [true, false] {
            self.input.handle(Event::PointerButton {
                at: Some(at),
                button: MouseButton::Left,
                pressed,
                time,
            });
        }
    }
}

fn centre(w: &PdaWindow, tab: usize) -> [f32; 2] {
    let r = w.layout().buttons[tab].rect;
    [r.x + r.width / 2.0, r.y + r.height / 2.0]
}

/// 2.1: positions relative to the background; 3.2: the tab bar laid out
/// from the captions' widths, the file's `x` and `width` overridden.
#[test]
fn the_tab_bar_is_laid_out_from_its_captions() {
    let l = layout();
    assert_eq!(l.background.rect, Rect::new(3.0, 20.0, 891.0, 643.0));
    assert_eq!((l.mb_line.rect.x, l.mb_line.rect.y), (113.0, 594.0));
    assert_eq!((l.timer_line.rect.x, l.timer_line.rect.y), (713.0, 593.0));
    assert_eq!((l.timer_title.rect.x, l.timer_title.rect.y), (723.0, 600.0));
    assert_eq!((l.tab_bar.x, l.tab_bar.y), (132.0, 593.0));
    // Tab 0: its mark at x 0, its caption 12 later, "quests" 60 wide.
    let b = &l.buttons;
    assert_eq!(b[0].mark, (132.0, 604.0));
    assert_eq!(b[0].rect, Rect::new(144.0, 600.0, 60.0, 20.0));
    // Then 3 units, the separator (20 wide), the next mark.
    assert_eq!(l.separators[0].rect, Rect::new(207.0, 600.0, 20.0, 20.0));
    assert_eq!(b[1].mark, (227.0, 604.0));
    assert_eq!(b[1].rect, Rect::new(239.0, 600.0, 30.0, 20.0));
    assert_eq!(l.separators.len(), 6);
    assert_eq!(b[6].hint.as_deref(), Some("pda_btn_encyclopedia_hint"));
}

/// 3.3: a switch hides the old tab and shows the new, selects its button,
/// clears its flag and gives its portion; nothing for the active tab.
#[test]
fn switching_selects_clears_and_gives_the_portion() {
    let mut w = window();
    assert_eq!(w.active_tab(), None);
    w.set_new_content(2);
    w.switch_to(2);
    assert_eq!(w.active_tab(), Some(2));
    assert_eq!(w.take_portions(), ["ui_pda_events"]);
    assert!(!w.state().tabs[2].new_content);
    w.set_new_content(2);
    w.switch_to(2);
    assert!(w.take_portions().is_empty(), "no portion again");
    assert!(w.state().tabs[2].new_content, "the flag stays");
    w.switch_to(0);
    assert!(w.take_portions().is_empty(), "the tasks tab gives none");
    w.switch_to(MAP_TAB);
    assert_eq!(w.take_portions(), ["ui_pda_map_local"]);
}

/// 4.2, 5: `ui_pda` on opening, `ui_pda_hide` on closing; the active tab
/// kept.
#[test]
fn opening_and_closing_give_their_portions() {
    let mut w = window();
    w.switch_to(3);
    w.open([10.0, 10.0]);
    assert_eq!(w.take_portions(), ["ui_pda_contacts", "ui_pda"]);
    w.close();
    assert_eq!(w.take_portions(), ["ui_pda_hide"]);
    assert_eq!(w.active_tab(), Some(3));
    w.close();
    assert!(w.take_portions().is_empty());
}

/// 3.3: a left press on a caption switches at once; one counted as a
/// double click does not; the looks follow selection and the cursor.
#[test]
fn a_press_on_a_caption_switches_tabs() {
    let mut r = Run::new();
    r.window.switch_to(0);
    r.window.open([0.0, 0.0]);
    r.window.take_portions();
    let at = centre(&r.window, 3);
    r.input.handle(Event::PointerMoved { at });
    r.input.handle(Event::PointerButton {
        at: Some(at),
        button: MouseButton::Left,
        pressed: true,
        time: 1000.0,
    });
    r.frame(0);
    assert_eq!(r.window.active_tab(), Some(3), "on the press");
    assert_eq!(r.window.take_portions(), ["ui_pda_contacts"]);
    let s = r.window.state();
    assert_eq!((s.tabs[3].look, s.tabs[0].look), ("t", "e"));
    // A press on tab 4 within 250 ms of the last is a double click.
    r.click(centre(&r.window, 4), 1100.0);
    r.frame(0);
    assert_eq!(
        r.window.active_tab(),
        Some(3),
        "a double click switches nothing"
    );
    assert_eq!(r.window.state().tabs[4].look, "h", "under the cursor");
    r.click(centre(&r.window, 4), 2000.0);
    r.frame(0);
    assert_eq!(r.window.active_tab(), Some(4));
}

/// The gamepad (#835): the left stick moves the cursor, south presses.
#[test]
fn the_gamepad_moves_the_cursor_to_a_caption_and_presses() {
    let mut r = Run::new();
    r.window.switch_to(0);
    r.window.open(centre(&window(), 0));
    let to = centre(&r.window, 2);
    for _ in 0..600 {
        let [x, y] = r.input.cursor_ui();
        let (dx, dy) = (to[0] - x, to[1] - y);
        let d = dx.hypot(dy);
        if d < 3.0 {
            break;
        }
        for (axis, v) in [(GamepadAxis::LeftX, dx / d), (GamepadAxis::LeftY, dy / d)] {
            r.input.handle(Event::GamepadAxis {
                axis,
                value: v * 0.5,
            });
        }
        r.frame(0);
    }
    for axis in [GamepadAxis::LeftX, GamepadAxis::LeftY] {
        r.input.handle(Event::GamepadAxis { axis, value: 0.0 });
    }
    r.frame(0);
    let at = r.window.state().cursor;
    assert!((at[0] - to[0]).hypot(at[1] - to[1]) < 3.0, "{at:?}");
    r.input.handle(Event::GamepadButton {
        button: GamepadButton::South,
        pressed: true,
    });
    r.frame(0);
    assert_eq!(r.window.active_tab(), Some(2));
}

/// 2.3: the clock, empty before the first update; the New Game start.
#[test]
fn the_clock_shows_the_game_time() {
    let mut r = Run::new();
    r.window.switch_to(0);
    r.window.open([0.0, 0.0]);
    assert_eq!(r.window.state().clock, "");
    r.frame(63_471_533_400_000);
    assert_eq!(r.window.state().clock, "05:30 01/05/2012");
    let start = volya_engine_formats::sav::game_time(2012, 12, 30, 23, 59, 0, 0).unwrap();
    assert_eq!(clock_text(start), "23:59 30/12/2012");
    // The formula gives 31.12.2012 and 01.01.2013 the same game time; the
    // later date is shown (#741).
    let start = volya_engine_formats::sav::game_time(2012, 12, 31, 23, 59, 0, 0).unwrap();
    assert_eq!(clock_text(start), "23:59 01/01/2013");
    let start = volya_engine_formats::sav::game_time(2013, 3, 1, 0, 0, 0, 0).unwrap();
    assert_eq!(clock_text(start), "00:00 01/03/2013");
    let start = volya_engine_formats::sav::game_time(2012, 2, 29, 7, 5, 0, 0).unwrap();
    assert_eq!(clock_text(start), "07:05 29/02/2012");
}

/// 2.4: each tab's mark at its point, by its flag; 2.2: the marks over
/// the rest; the window draws nothing while closed.
#[test]
fn the_window_draws_its_marks_last() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let mut assets = fixture::assets();
    let res = Resolution::new(1024, 768);
    let fonts = assets.fonts(&tree, &data, res.height).unwrap();
    let mut w = PdaWindow::load(&tree, &data, res, fonts).unwrap();
    let mut ctx = Ctx {
        tree: &tree,
        data: &data,
        assets: &mut assets,
        resolution: res,
        surface: crate::draw::Surface::screen(res),
        clock_ms: 0,
    };
    let mut s = Scene::new(res);
    w.draw(&mut ctx, &mut s).unwrap();
    assert!(s.sprites.is_empty(), "closed");
    w.set_new_content(0);
    w.switch_to(1);
    w.open([0.0, 0.0]);
    w.draw(&mut ctx, &mut s).unwrap();
    let layers: Vec<&str> = s.sprites.iter().map(|s| s.layer.as_str()).collect();
    assert_eq!(layers.first(), Some(&"pda background"));
    let marks: Vec<usize> = (layers.iter().enumerate())
        .filter(|(_, l)| l.starts_with("pda mark"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(marks.len(), TABS);
    assert!(
        (layers.iter().enumerate())
            .filter(|(_, l)| l.starts_with("pda tab"))
            .all(|(i, _)| i < marks[0]),
        "the captions under the marks: {layers:?}"
    );
    assert_eq!(layers.last(), Some(&"cursor"));
}

/// 4.1: with the window open, an action's key switches to its tab (its
/// flag cleared, its portion given) and then closes it; the next opening
/// by another key switches again.
#[test]
fn an_actions_key_switches_then_closes() {
    let mut r = Run::open();
    r.window.set_new_content(3);
    r.key(Key::H);
    assert!(r.frame(0).closed);
    assert!(!r.window.is_open());
    assert_eq!(r.window.active_tab(), Some(3));
    assert!(!r.window.state().tabs[3].new_content);
    assert_eq!(r.window.take_portions(), ["ui_pda_contacts", "ui_pda_hide"]);
    r.window.act(MAP_TAB, [0.0, 0.0]);
    assert!(r.window.is_open());
    assert_eq!(r.window.take_portions(), ["ui_pda_map_local", "ui_pda"]);
    // The key of the active tab closes it with no portion of its own.
    r.key(Key::M);
    assert!(r.frame(0).closed);
    assert_eq!(r.window.take_portions(), ["ui_pda_hide"]);
}

/// 5: the key bound to `quit` closes the window, also while paused; while
/// paused nothing else acts (6). A gamepad's east button is `quit` (our
/// design).
#[test]
fn quit_closes_it_even_while_paused() {
    let mut r = Run::open();
    r.paused = true;
    r.key(Key::H);
    r.click(centre(&r.window, 2), 0.0);
    assert!(!r.frame(0).closed);
    assert_eq!(r.window.active_tab(), Some(0), "no input while paused");
    r.key(Key::Escape);
    assert!(r.frame(0).closed);
    assert_eq!(r.window.active_tab(), Some(0));
    let mut r = Run::open();
    for pressed in [true, false] {
        r.input.handle(Event::GamepadButton {
            button: GamepadButton::East,
            pressed,
        });
    }
    assert!(r.frame(0).closed);
}

/// 5 item 3: the character's death closes it.
#[test]
fn death_closes_it() {
    let mut r = Run::open();
    r.alive = false;
    assert!(r.frame(0).closed);
    assert_eq!(r.window.take_portions(), ["ui_pda_hide"]);
}

/// The touch PDA button acts as the key bound to `active_jobs` (our
/// design): over the open window it switches to tab 0 and closes it.
#[test]
fn the_touch_button_acts_as_active_jobs() {
    let mut r = Run::open();
    r.window.switch_to(4);
    for pressed in [true, false] {
        r.input.handle(Event::TouchButton {
            button: TouchButton::Pda,
            pressed,
        });
    }
    assert!(r.frame(0).closed);
    assert_eq!(r.window.active_tab(), Some(0));
}

/// pda-controls.md 6: the hint shows once the cursor has rested over 500
/// ms of the global clock on a caption, above the cursor, max(80, text +
/// 30) wide; it fades in once and holds; it goes when the cursor leaves;
/// the paused global clock holds it back.
#[test]
fn a_hint_shows_after_500_ms_over_a_caption() {
    let mut r = Run::open();
    let at = centre(&r.window, 2);
    r.input.handle(Event::PointerMoved { at });
    r.frame(0);
    r.paused = true;
    for _ in 0..40 {
        r.frame(0);
    }
    assert_eq!(r.window.state().hint, None, "the global clock stood");
    r.paused = false;
    // 16 ms a frame: 500 ms are past at the 32nd frame after entering.
    for _ in 0..31 {
        r.frame(0);
    }
    assert_eq!(r.window.state().hint, None);
    r.frame(0);
    let h = r.window.state().hint.expect("the hint");
    assert_eq!((h.tab, h.text.as_str()), (2, "pda_btn_diary_hint"));
    // "pda_btn_diary_hint" is 18 characters: 180 + 30 wide, 30 high, its
    // bottom-left at the cursor.
    assert_eq!(h.rect, [at[0], at[1] - 30.0, 210.0, 30.0]);
    assert_eq!(h.alpha, 0, "the animation's first frame");
    for _ in 0..80 {
        r.frame(0);
    }
    let h = r.window.state().hint.unwrap();
    assert!(h.alpha > 200, "faded in and held: {}", h.alpha);
    r.input.handle(Event::PointerMoved {
        at: [at[0], at[1] + 100.0],
    });
    r.frame(0);
    assert_eq!(r.window.state().hint, None, "released on leaving");
}

/// pda-controls.md 6: off the screen above, it moves left, then down to
/// the cursor's height, then below the cursor.
#[test]
fn a_hint_off_the_screen_is_moved() {
    let d = hint_desc();
    let h = hint::Hint::take(&d, 0, "x".into(), 50.0, [500.0, 10.0], 0);
    assert_eq!(h.rect, Rect::new(420.0, 10.0, 80.0, 30.0));
    let h = hint::Hint::take(&d, 0, "x".into(), 50.0, [1000.0, 300.0], 0);
    assert_eq!(h.rect, Rect::new(920.0, 270.0, 80.0, 30.0));
    let h = hint::Hint::take(&d, 0, "x".into(), 50.0, [1000.0, 10.0], 0);
    assert_eq!(h.rect, Rect::new(920.0, 10.0, 80.0, 30.0));
    let h = hint::Hint::take(&d, 0, "x".into(), 50.0, [40.0, 10.0], 0);
    assert_eq!(h.rect, Rect::new(40.0, 55.0, 80.0, 30.0));
}
