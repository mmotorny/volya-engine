// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The talk screen's showing, input and closing
//! (docs/behavior/talk-window-input.md) on the layout of the spec's values.

use super::screen::*;
use super::tests::{TALK, hero, window_of, wolf};
use crate::core::input::{Event, Input, Key, MouseButton};
use crate::data::GameData;
use crate::fixture;

struct Run {
    data: GameData,
    screen: TalkScreen,
    input: Input,
    now: u64,
    paused: bool,
    use_pressed: bool,
}

fn keys() -> Keys {
    Keys {
        use_keys: vec![Key::F],
        quit: vec![Key::Escape],
    }
}

impl Run {
    /// A screen shown for a talk with Wolf, `special` its special mode,
    /// with `choices` listed.
    fn shown(special: bool, choices: &[&str]) -> Self {
        let tree = fixture::tree();
        let mut data = fixture::data(&tree);
        data.quit_keys = vec![Key::Escape];
        let mut screen = TalkScreen::new(window_of(TALK));
        let start = TalkStart {
            player: hero(),
            partner: wolf(),
            special_mode: special,
            said: vec![
                Said {
                    speaker: "Wolf".into(),
                    text: String::new(),
                },
                Said {
                    speaker: "Wolf".into(),
                    text: "Hello".into(),
                },
            ],
        };
        screen.show(&data, Some(start), 7, [0.0, 0.0]).unwrap();
        assert_eq!(screen.take_portions(), [TALK_SHOWN, TRADE_HIDDEN]);
        assert!(screen.take_refill(), "1.3: marked at the talk start");
        screen.window.set_choices(&data, choices).unwrap();
        Self {
            data,
            screen,
            input: Input::new(),
            now: 0,
            paused: false,
            use_pressed: false,
        }
    }

    fn frame(&mut self) -> Asked {
        self.now += 16;
        let asked = self.screen.input(
            &self.data,
            &self.input,
            &keys(),
            std::mem::take(&mut self.use_pressed),
            self.paused,
            self.now,
        );
        self.input.end_frame();
        asked
    }

    fn key(&mut self, key: Key) {
        for pressed in [true, false] {
            self.input.handle(Event::Key { key, pressed });
        }
    }

    fn button(&mut self, at: [f32; 2], pressed: bool) {
        self.input.handle(Event::PointerMoved { at });
        self.input.handle(Event::PointerButton {
            at: Some(at),
            button: MouseButton::Left,
            pressed,
            time: self.now as f64,
        });
    }

    fn click(&mut self, at: [f32; 2]) -> Asked {
        self.button(at, true);
        self.frame();
        self.button(at, false);
        self.frame()
    }

    fn question(&self, i: usize) -> [f32; 2] {
        let r = self.screen.window.question_rect(i).unwrap();
        [r.x + 5.0, r.y + r.height / 2.0]
    }
}

/// 1.3, talk-window.md 6.1: the phrases said are logged, an empty text
/// adds nothing.
#[test]
fn showing_sets_up_the_talk() {
    let r = Run::shown(false, &["a"]);
    let s = r.screen.state();
    assert!(s.open && s.window.shown && s.window.button_shown);
    assert_eq!(s.partner, Some(7));
    let answers: Vec<_> = (s.window.answers.iter())
        .map(|e| (e.name.as_str(), e.text.as_str()))
        .collect();
    assert_eq!(answers, [("Wolf", "Hello")]);
}

/// 3.4: a click on a question takes its choice and marks the refill; a
/// second before the refill is ignored; the press alone does nothing.
#[test]
fn a_click_on_a_question_takes_its_choice_once_per_refill() {
    let mut r = Run::shown(false, &["first", "second"]);
    let at = r.question(1);
    r.button(at, true);
    assert_eq!(r.frame(), Asked::default(), "a press alone");
    assert_eq!(r.screen.state().window.questions.len(), 2);
    r.button(at, false);
    let asked = r.frame();
    assert_eq!(asked.choice, Some(1));
    assert!(r.screen.state().refill);
    assert_eq!(r.click(r.question(0)).choice, None, "pending: ignored");
    assert!(r.screen.take_refill());
    assert_eq!(r.click(r.question(0)).choice, Some(0));
    // A release elsewhere clicks nothing.
    r.screen.take_refill();
    let at = r.question(0);
    r.button(at, true);
    r.frame();
    r.button([10.0, 10.0], false);
    assert_eq!(r.frame().choice, None);
}

/// pda-controls.md 4: under the cursor `h`, pressed `t`.
#[test]
fn the_captions_look_under_the_cursor() {
    let mut r = Run::shown(false, &["first"]);
    let at = r.question(0);
    r.input.handle(Event::PointerMoved { at });
    r.frame();
    assert_eq!(r.screen.window.question_look, Some((0, "h")));
    r.button(at, true);
    r.frame();
    assert_eq!(r.screen.window.question_look, Some((0, "t")));
    let b = r.screen.window.button_rect().unwrap();
    r.button([b.x + 10.0, b.y + 10.0], false);
    r.frame();
    assert_eq!(r.screen.window.question_look, None);
    assert_eq!(r.screen.window.button_look, "h");
}

/// 3.4: the trade button reports message 45; hidden with the special mode.
#[test]
fn the_trade_button() {
    let mut r = Run::shown(false, &[]);
    let b = r.screen.window.button_rect().unwrap();
    let asked = r.click([b.x + 10.0, b.y + 10.0]);
    assert!(asked.trade && !asked.closed);
    let r = Run::shown(true, &[]);
    assert_eq!(r.screen.window.button_rect(), None);
}

/// 3.1, 3.2, 5: `use` and `quit` close the screen, giving
/// `ui_trade_hide`; with a special-mode partner neither does, unless
/// paused for `quit`; `use` closes also while paused.
#[test]
fn use_and_quit_close_it() {
    for close in [Key::F, Key::Escape] {
        let mut r = Run::shown(false, &[]);
        if close == Key::F {
            r.use_pressed = true;
        } else {
            r.key(close);
        }
        assert!(r.frame().closed, "{close:?}");
        assert!(!r.screen.is_open());
        assert_eq!(r.screen.take_portions(), [TRADE_HIDDEN]);
        assert!(r.screen.window.is_shown(), "5: no ui_talk_hide");
    }
    let mut r = Run::shown(true, &[]);
    r.use_pressed = true;
    r.key(Key::Escape);
    assert!(!r.frame().closed && r.screen.is_open(), "special mode");
    r.paused = true;
    r.use_pressed = true;
    assert!(!r.frame().closed, "use: nothing with the special mode");
    r.key(Key::Escape);
    assert!(r.frame().closed, "quit closes it while paused");
    let mut r = Run::shown(false, &[]);
    r.paused = true;
    r.use_pressed = true;
    assert!(r.frame().closed, "use closes it while paused");
}

/// 3.4, 3.6: while paused no click is taken.
#[test]
fn nothing_is_taken_while_paused() {
    let mut r = Run::shown(false, &["first"]);
    r.paused = true;
    assert_eq!(r.click(r.question(0)), Asked::default());
}

/// 3.4, 3.5, options-screens.md 4.7, 4.8: the wheel over a list scrolls
/// it by a step; the arrows step it.
#[test]
fn the_lists_scroll() {
    let many: Vec<String> = (0..12).map(|i| format!("choice {i}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let mut r = Run::shown(false, &many);
    let (list, bar, _) = r.screen.window.list(1).unwrap();
    let (rect, step) = (list.rect, bar.step());
    assert!(bar.limit() > 0, "taller than the view");
    r.input.handle(Event::PointerMoved {
        at: [rect.x + 20.0, rect.y + 20.0],
    });
    r.input.handle(Event::Wheel { steps: 1 });
    r.frame();
    assert_eq!(r.screen.state().window.questions_scroll, step);
    r.input.handle(Event::Wheel { steps: -1 });
    r.frame();
    assert_eq!(r.screen.state().window.questions_scroll, 0);
    // The down arrow, at the bar's bottom.
    let x = rect.x + rect.width - 8.0;
    r.click([x, rect.y + rect.height - 4.0]);
    assert_eq!(r.screen.state().window.questions_scroll, step);
    r.click([x, rect.y + 4.0]);
    assert_eq!(r.screen.state().window.questions_scroll, 0);
}
