// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The map window on layouts with the spec's values (pda-map.md 1.1, 1.3,
//! 1.4) and the shipped global map with Escape's level map: the set-up,
//! the first showing's flight, the tools, the wheel, the drag, the keys,
//! the scroll bars, the player's mark, the label and the hints; the
//! gamepad's paths (our design).

use super::map::*;
use super::tests::{PDA, Run};
use super::*;
use crate::core::input::{Event, GamepadAxis, GamepadButton, MouseButton};
use crate::level_maps::LevelMaps;
use volya_engine_formats::ui::ColorDefs;
use volya_engine_formats::xml;

/// `pda_map.xml`'s map window (1.1).
pub(crate) const PDA_MAP: &str = r#"<window><map_wnd>
<main_wnd x="110" y="90" width="761" height="475">
<main_map_frame x="0" y="0" width="761" height="475" a="90"><texture>ui_pda_frame_sub</texture>
<level_frame x="8" y="41" width="725" height="406"/></main_map_frame>
<map_header_frame_line x="8" y="17" width="747" height="21"><texture>ui_string_01</texture><title/>
<tool_bar x="0" y="6" width="100" height="16">
<global_map_btn x="5" y="1" width="21" height="15" check_mode="0" hint="global_map_btn_hint"><texture>ui_icons_newPDA_compas</texture></global_map_btn>
<actor_btn x="27" y="1" width="21" height="15" hint="actor_btn_hint"><texture>ui_icons_newPDA_showpers</texture></actor_btn>
<zoom_in_btn x="120" y="2" width="21" height="15" check_mode="1" hint="zoom_in_btn_hint"><texture>ui_icons_newPDA_zoom</texture></zoom_in_btn>
<zoom_out_btn x="142" y="2" width="21" height="15" check_mode="1" hint="zoom_out_btn_hint"><texture>ui_icons_newPDA_ZoomMin</texture></zoom_out_btn>
<notused_add_spot_btn x="200" y="2" width="21" height="15"/>
</tool_bar></map_header_frame_line>
<text_hint x="735" y="450" width="100" height="22" complex_mode="1"><text font="graffiti22"/></text_hint>
</main_wnd></map_wnd></window>"#;

/// The player's spots of `map_spots.xml` (1.3).
pub(crate) const MAP_SPOTS: &str = r#"<window>
<actor_location hint="st_map_actor"><level_map spot="actor_level_spot"/></actor_location>
<actor_location_p hint="actor"><level_map spot="actor_level_spot_p"/></actor_location_p>
<actor_level_spot width="60" height="60" alignment="c"><texture x="730" y="394" width="60" height="60">ui\ui_common</texture></actor_level_spot>
<actor_level_spot_p width="60" height="60" alignment="c" heading="1"><texture x="794" y="394" width="60" height="60">ui\ui_common</texture></actor_level_spot_p>
</window>"#;

/// `hint_item.xml`: `button_hint` (pda-controls.md 6) and `hint_item`
/// (1.4).
pub(crate) const HINTS: &str = r#"<window><button_hint x="0" y="0" width="200" height="30">
<frame_line x="0" y="0" width="200" height="30"><texture>ui_icons_PDA_string_tooltips</texture></frame_line>
<description x="10" y="2" width="192" height="20" light_anim="ui_btn_hint" la_cyclic="0" la_texture="0" la_text="1" la_alpha="1">
<text font="letterica18" r="255" g="255" b="255"/></description></button_hint>
<hint_item x="0" y="0" width="210" height="200"><frame x="0" y="0" width="210" height="200"><texture>ui_icons_PDA_dialog</texture></frame>
<description x="10" y="4" width="192" height="192" complex_mode="1"><text font="letterica18" align="l"/></description></hint_item>
</window>"#;

/// `scroll_bar.xml`: the `default` and `pda` profiles (pda-controls.md
/// 11).
pub(crate) const SCROLL: &str = r#"<window><default height="16"><up_arrow><texture_e>ui_scroll_btn_up</texture_e></up_arrow>
<down_arrow><texture_e>ui_scroll_btn_down</texture_e></down_arrow><back_v><texture>ui_scroll_back</texture></back_v>
<box_v><texture>ui_scroll_box</texture></box_v></default>
<pda height="17"><up_arrow width="17" height="21"><texture_e>ui_scroll_PDA_btn_up_e</texture_e></up_arrow>
<down_arrow width="17" height="30"><texture_e>ui_scroll_PDA_btn_down_e</texture_e></down_arrow>
<back_v><texture>ui_scroll_PDA_back</texture></back_v><box_v><texture>ui_scroll_PDA_box_e</texture></box_v></pda></window>"#;

/// `game.ltx`'s maps (pda-level-maps.md 4, 5.2): the global map and
/// Escape.
pub(crate) const GAME: &str = "[level_maps_single]\nL01_escape\n\
    [global_map]\ntexture = ui\\ui_global_map\nbound_rect = 0.0,0.0, 1024, 2634.0\nmax_zoom = 6.0\n\
    [l01_escape]\ntexture = map\\map_escape\nbound_rect = -317.164, -633.718, 404.880, 811.627\n\
    global_rect = 355, 2011, 576, 2453\n";

/// The New Game's start (start-position.md).
const START: [f32; 2] = [-246.7263, -134.43387];
const START_YAW: f32 = 1.3962544;

fn map_window() -> MapWindow {
    let doc = |s: &str| xml::parse(s.as_bytes()).unwrap();
    let colors = ColorDefs::default();
    let layout = MapLayout::read(
        &doc(PDA_MAP),
        ELEMENT,
        &doc(MAP_SPOTS),
        &doc(HINTS),
        &doc(SCROLL),
        &colors,
    )
    .unwrap();
    let game = volya_engine_formats::ltx::parse(GAME.as_bytes()).unwrap();
    let maps = LevelMaps::read(&game).unwrap();
    // 20 units a line, 20 characters a line.
    MapWindow::new(
        layout,
        maps,
        Box::new(|t: &[u8]| 20.0 * t.len().div_ceil(20).max(1) as f32),
    )
}

impl Run {
    /// An open window with the map, the player at the start, on tab 0.
    fn with_map() -> Self {
        let mut r = Self::new();
        r.window = super::tests::window().with_map(map_window(), &|k| k.to_owned(), &|_, t| {
            10.0 * t.len() as f32
        });
        r.window.switch_to(0);
        r.window.open([512.0, 384.0]);
        r.player = Some(("l01_escape".into(), START, START_YAW));
        r
    }

    fn map(&self) -> MapState {
        self.window.state().map.unwrap()
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(0);
        }
    }

    fn move_to(&mut self, p: [f32; 2]) {
        self.input.handle(Event::PointerMoved { at: p });
    }

    fn button(&mut self, pressed: bool) {
        // Apart enough not to count as a double click.
        self.now += 300;
        self.input.handle(Event::PointerButton {
            at: None,
            button: MouseButton::Left,
            pressed,
            time: self.now as f64,
        });
    }

    /// Moves there, presses and releases a frame apart.
    fn press_at(&mut self, p: [f32; 2]) {
        self.move_to(p);
        self.frame(0);
        self.button(true);
        self.frame(0);
        self.button(false);
        self.frame(0);
    }

    fn tool(&self, i: usize) -> [f32; 2] {
        let r = self.window.map().unwrap().layout().tools[i].rect;
        [r.x + r.width / 2.0, r.y + r.height / 2.0]
    }

    /// Runs frames until the transition is reached.
    fn settle(&mut self) {
        for _ in 0..2000 {
            if !self.map().moving {
                return;
            }
            self.frame(0);
        }
        panic!("no end: {:?}", self.map());
    }
}

fn close(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() < eps
}

#[test]
fn reads_the_layout_at_the_spec_s_places() {
    let w = map_window();
    let l = w.layout();
    // 1.1: the frame on screen 118, 131 – 843, 537.
    assert_eq!(l.frame, Rect::new(118.0, 131.0, 725.0, 406.0));
    assert_eq!(l.header, Rect::new(118.0, 107.0, 747.0, 21.0));
    assert_eq!(l.tools[0].rect, Rect::new(123.0, 114.0, 21.0, 15.0));
    assert_eq!(l.tools[2].rect, Rect::new(238.0, 115.0, 21.0, 15.0));
    assert_eq!(
        l.tools.iter().map(|t| t.check).collect::<Vec<_>>(),
        [false, false, true, true]
    );
    assert_eq!(
        l.tools[0].looks[0].as_deref(),
        Some("ui_icons_newPDA_compas_e")
    );
    assert_eq!(l.spots[0].hint, "st_map_actor");
    assert_eq!(l.spots[1].hint, "actor");
    assert!(l.spots[1].desc.heading && !l.spots[0].desc.heading);
    assert_eq!((l.up_arrow, l.down_arrow), ((17.0, 21.0), (17.0, 30.0)));
    // 2.1: the set-up covers the frame: 725 × 1864.89, s 0.7080078.
    let s = w.state();
    assert!(close(s.min_zoom, 0.708_008, 1e-6), "{}", s.min_zoom);
    assert!(close(s.picture[3], 1864.89, 0.01), "{:?}", s.picture);
    assert_eq!(s.picture[..3], [0.0, 0.0, 725.0]);
    // 5.6: the bars' values follow the picture.
    assert_eq!(s.bars, [[0, 725, 725, 72], [0, 1864, 406, 40]]);
}

#[test]
fn the_first_showing_flies_to_the_player_at_maximum_zoom() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    assert!(r.window.take_portions().contains(&MAP_SHOWN));
    r.frame(0);
    let m = r.map();
    // 3: the actor button's action: T the start point, Z the maximum.
    assert_eq!(m.zoom, 6.0);
    assert!(close(m.target[0], 376.559, 0.01) && close(m.target[1], 2300.566, 0.01));
    // The start point lies outside the grown frame: the approach (4.2).
    assert_eq!(m.phase, Some(map::Phase::Approach));
    r.settle();
    let m = r.map();
    // 3's worked example: (−1896.86, −13600.40), 6144 × 15804.
    assert!(close(m.picture[0], -1896.86, 0.05), "{:?}", m.picture);
    assert!(close(m.picture[1], -13600.4, 0.05), "{:?}", m.picture);
    assert!(close(m.picture[2] - m.picture[0], 6144.0, 0.05));
    assert!(close(m.scale, 6.0, 1e-4));
    // The approach, planned for 6.539 s (3), gives way to the settle once
    // the start point is near (4.2), which ends sooner.
    assert!(r.global_ms > 2000 && r.global_ms < 6539, "{}", r.global_ms);
    // 6.7: the player's two marks, centred in the frame, the second turned
    // by −yaw (6.3).
    assert_eq!(m.marks.len(), 2);
    let c = |k: &map::MarkState| [k.rect[0] + 30.0, k.rect[1] + 30.0];
    assert!(
        close(c(&m.marks[0])[0], 118.0 + 362.5, 0.1),
        "{:?}",
        m.marks
    );
    assert!(
        close(c(&m.marks[0])[1], 131.0 + 203.0, 0.1),
        "{:?}",
        m.marks
    );
    assert!(close(m.marks[1].angle, -START_YAW, 1e-5));
    // Hidden and shown again: no second flight (3).
    r.window.switch_to(0);
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    assert!(!r.map().moving);
}

#[test]
fn heading_follows_the_spec_s_rule() {
    use std::f32::consts::{FRAC_PI_2, PI};
    assert_eq!(heading([0.0, 0.0]), 0.0);
    assert_eq!(heading([1.0, 0.0]), -FRAC_PI_2);
    assert_eq!(heading([-1.0, 0.0]), FRAC_PI_2);
    assert!(close(heading([1.0, 1.0]), -PI / 4.0, 1e-6));
    // z < 0: less π.
    assert!(close(heading([1.0, -1.0]), PI / 4.0 - PI, 1e-6));
}

#[test]
fn the_global_button_zooms_out_and_marks_go_at_minimum_zoom() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    r.settle();
    let p = r.tool(GLOBAL_BUTTON);
    r.press_at(p);
    let m = r.map();
    assert!(close(m.zoom, m.min_zoom, 1e-6));
    r.settle();
    let m = r.map();
    assert!(close(m.scale, m.min_zoom, 1e-4));
    // 6.1: at minimum zoom no marks.
    assert!(m.marks.is_empty());
    // 5.2: the actor button flies back, zoomed up.
    let p = r.tool(ACTOR_BUTTON);
    r.press_at(p);
    assert_eq!(r.map().zoom, 6.0);
}

#[test]
fn zoom_tools_toggle_and_zoom_at_the_press() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    r.settle();
    // To the global map, then the zoom-in tool on.
    let p = r.tool(GLOBAL_BUTTON);
    r.press_at(p);
    r.settle();
    let p = r.tool(ZOOM_IN);
    r.press_at(p);
    assert_eq!(r.map().tool, Some(ZOOM_IN));
    assert_eq!(r.map().looks[ZOOM_IN], "t");
    // The other tool: this one off (5.4).
    let p = r.tool(ZOOM_OUT);
    r.press_at(p);
    assert_eq!(r.map().tool, Some(ZOOM_OUT));
    r.press_at(p);
    assert_eq!(r.map().tool, None);
    let p = r.tool(ZOOM_IN);
    r.press_at(p);
    // A press in the frame: Z × 1.5, T the point under the cursor.
    let at = [118.0 + 300.0, 131.0 + 200.0];
    let before = r.map();
    r.press_at(at);
    let m = r.map();
    assert!(close(m.zoom, before.min_zoom * 1.5, 1e-5), "{}", m.zoom);
    let s = before.scale;
    assert!(
        close(m.target[0], 300.0 / s, 0.01)
            && close(m.target[1], (200.0 - before.picture[1]) / s, 0.01)
    );
}

#[test]
fn the_wheel_zooms_backward_in_forward_out() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    r.settle();
    let p = r.tool(GLOBAL_BUTTON);
    r.press_at(p);
    r.settle();
    r.move_to([400.0, 300.0]);
    r.frame(0);
    let z = r.map().zoom;
    r.input.handle(Event::Wheel { steps: 1 });
    r.frame(0);
    assert!(close(r.map().zoom, z * 1.5, 1e-5), "wheel down zooms in");
    r.input.handle(Event::Wheel { steps: -1 });
    r.frame(0);
    assert!(close(r.map().zoom, z, 1e-5), "wheel up zooms out");
}

#[test]
fn the_keypad_zooms_on_press_and_release_and_arrows_move() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    r.settle();
    let p = r.tool(GLOBAL_BUTTON);
    r.press_at(p);
    r.settle();
    let z = r.map().zoom;
    r.input.handle(Event::Key {
        key: Key::NumpadAdd,
        pressed: true,
    });
    r.frame(0);
    r.input.handle(Event::Key {
        key: Key::NumpadAdd,
        pressed: false,
    });
    r.frame(0);
    // 5.7: twice for one press and release.
    assert!(close(r.map().zoom, z * 2.25, 1e-4), "{}", r.map().zoom);
    // The keypad keys are kept from the character while the map shows.
    let kept = r.window.kept(&super::tests::keys());
    assert!(kept.contains(&Control::Key(Key::NumpadAdd)));
    r.settle();
    // 5.6: Down held moves the picture up a unit a frame.
    let y = r.map().picture[1];
    r.input.handle(Event::Key {
        key: Key::Down,
        pressed: true,
    });
    r.frames(3);
    assert!(
        close(r.map().picture[1], y - 3.0, 1e-3),
        "{:?}",
        r.map().picture
    );
}

#[test]
fn a_drag_moves_the_picture_and_the_bars_follow() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    r.settle();
    let before = r.map().picture;
    r.move_to([400.0, 300.0]);
    r.frame(0);
    r.button(true);
    r.frame(0);
    r.move_to([380.0, 290.0]);
    r.frame(0);
    let m = r.map();
    assert!(
        close(m.picture[0], before[0] - 20.0, 1e-3) && close(m.picture[1], before[1] - 10.0, 1e-3)
    );
    assert_eq!(m.bars[0][0], (-m.picture[0]) as i32);
    assert_eq!(m.bars[1][0], (-m.picture[1]) as i32);
}

#[test]
fn the_label_names_the_level_under_the_cursor() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    r.settle();
    r.move_to([118.0 + 362.5, 131.0 + 203.0]);
    r.frame(0);
    assert_eq!(r.map().label.as_deref(), Some("l01_escape"));
    // The mark's hint after more than 500 ms (7.2): the top one's.
    r.frames(40);
    let h = r.map().hint.expect("a hint");
    assert_eq!(h.text, "actor");
    // 7.2: above and to the right of the cursor, 210 wide.
    assert_eq!(h.rect[2], 210.0);
    assert!(
        close(h.rect[0], 480.5, 1e-3) && close(h.rect[1], 334.0 - 64.0, 1e-3),
        "{:?}",
        h.rect
    );
    // Moved off the cursor by the arrow key, the mark's hint closes (5.6).
    r.input.handle(Event::Key {
        key: Key::Left,
        pressed: true,
    });
    r.frames(40);
    assert!(r.map().hint.is_none());
}

#[test]
fn a_gamepad_pans_with_the_right_stick_and_zooms_with_the_shoulders() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    r.settle();
    let x = r.map().picture[0];
    r.input.handle(Event::GamepadAxis {
        axis: GamepadAxis::RightX,
        value: 1.0,
    });
    r.frames(10);
    assert!(r.map().picture[0] < x - 50.0, "{:?}", r.map().picture);
    r.input.handle(Event::GamepadAxis {
        axis: GamepadAxis::RightX,
        value: 0.0,
    });
    r.input.handle(Event::GamepadButton {
        button: GamepadButton::LeftShoulder,
        pressed: true,
    });
    r.frame(0);
    assert!(close(r.map().zoom, 4.0, 1e-4), "{}", r.map().zoom);
    // The d-pad reaches the tool buttons.
    r.move_to(r.tool(ACTOR_BUTTON));
    r.frame(0);
    r.input.handle(Event::GamepadButton {
        button: GamepadButton::DpadRight,
        pressed: true,
    });
    r.frame(0);
    assert_eq!(r.window.state().cursor, r.tool(ZOOM_IN));
}

#[test]
fn an_unlisted_level_logs_and_shows_the_global_map() {
    let mut r = Run::with_map();
    r.player = Some(("l10u_bunker".into(), [0.0, 0.0], 0.0));
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    assert_eq!(
        r.window.take_log(),
        ["~ Level Map 'l10u_bunker' not registered"]
    );
    let m = r.map();
    assert!(close(m.zoom, m.min_zoom, 1e-6));
    r.settle();
    assert!(r.map().marks.is_empty());
}

#[test]
fn the_tool_buttons_hints_show_in_the_button_hint_window() {
    let mut r = Run::with_map();
    r.window.switch_to(MAP_TAB);
    r.frame(0);
    r.move_to(r.tool(ZOOM_OUT));
    r.frames(40);
    let h = r.window.state().hint.expect("a hint");
    assert_eq!(h.tab, TABS + ZOOM_OUT);
    assert_eq!(h.text, "zoom_out_btn_hint");
    // Not while another tab shows.
    r.window.switch_to(0);
    r.frame(0);
    assert!(r.window.state().hint.is_none());
}

#[test]
fn the_fixture_s_window_loads_with_its_map() {
    let tree = crate::fixture::tree();
    let data = crate::fixture::data(&tree);
    let mut assets = crate::data::Assets::default();
    let fonts = assets.fonts(&tree, &data, 768).unwrap();
    let w = PdaWindow::load(&tree, &data, Resolution::new(1024, 768), fonts).unwrap();
    assert_eq!(w.map().unwrap().maps().levels.len(), 1);
    // The PDA layout is the window's own fixture.
    let _ = PDA;
}
