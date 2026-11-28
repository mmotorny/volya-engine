// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The window on layouts with the spec's values (talk-window.md 2–10),
//! texts measured with the line heights of 1024 × 768 (18 for
//! `letterica18`, 16 for `letterica16`, 22 for `graffiti22`) and 10 units
//! per character: the rectangles, the worked bands and heights, the drawing
//! order, the scrolling, the templates, the panels and the special mode.

use super::*;
use crate::fixture;
use volya_engine_formats::ui::ColorDefs;
use volya_engine_formats::xml;

/// `talk.xml` with the values of talk-window.md 2–8 (c1 D1, D3, D5, D7, D8).
pub(crate) const TALK: &str = r#"<window>
<top_background x="0" y="0" width="1024" height="128"><texture>ui\ui_top_background</texture></top_background>
<bottom_background x="0" y="736" width="1024" height="32"><texture>ui\ui_bottom_background</texture></bottom_background>
<left_character_icon x="15" y="32" width="200" height="218"><texture>ui_charinfo_left</texture><texture_offset x="-11" y="0"/></left_character_icon>
<right_character_icon x="820" y="32" width="200" height="218"><texture>ui_charinfo_right</texture></right_character_icon>
<frame_line_window x="218" y="42" width="600" height="470" vertical="1"><texture>ui_talk_dialogue</texture>
<title x="25" y="3" width="190" height="22" complex_mode="0"><text font="graffiti22" r="238" g="153" b="26" align="l" vert_align="t"/></title></frame_line_window>
<frame_line_window x="218" y="485" width="600" height="250" vertical="1"><texture>ui_talk_dialogue2</texture>
<title x="25" y="60" width="190" height="22" complex_mode="0"><text font="graffiti22" r="238" g="153" b="26" align="l" vert_align="t"/></title></frame_line_window>
<answers_list x="33" y="46" width="544" height="377" right_ident="0" always_show_scroll="0"/>
<questions_list x="30" y="100" width="547" height="120" right_ident="5" always_show_scroll="0"/>
<button x="437" y="726" width="157" height="48" highlight_text="1" hR="255" hG="255" hB="255"><texture>ui_button_main01</texture>
<text font="graffiti22" r="238" g="153" b="26" align="c">ui_st_trade</text></button>
<question_item x="0" y="0" width="527" height="20" min_height="13"><content_text x="0" y="0" width="520" height="20">
<text font="letterica18" r="255" g="232" b="208" align="l" complex_mode="1"/>
<text_color><e r="255" g="232" b="208"/><h r="255" g="255" b="255"/><t r="231" g="168" b="12"/></text_color></content_text></question_item>
<other_answer_item x="0" y="0" width="527" height="50" min_height="13" bottom_footer="5">
<name_caption x="0" y="0" width="520" height="10"><text font="graffiti22" r="238" g="153" b="26"/></name_caption>
<content_text x="20" y="22" width="510" height="20" complex_mode="1"><text font="letterica18" r="216" g="186" b="140"/></content_text></other_answer_item>
<actor_answer_item x="0" y="0" width="527" height="50" min_height="13" bottom_footer="10">
<name_caption x="0" y="0" width="520" height="10"><text font="graffiti22" r="135" g="183" b="116"/></name_caption>
<content_text x="20" y="22" width="510" height="20" complex_mode="1"><text font="letterica18" r="255" g="232" b="208"/></content_text></actor_answer_item>
<iconed_answer_item x="0" y="0" width="527" height="30" min_height="30" bottom_footer="5">
<name_caption x="0" y="0" width="0" height="0"/>
<content_text x="75" y="0" width="510" height="20" complex_mode="1"><text font="letterica16" r="255" g="255" b="255"/></content_text>
<msg_icon x="20" y="3" width="49" height="29"/></iconed_answer_item>
<iconed_trade_info x="0" y="0" width="527" height="20" min_height="20" bottom_footer="5">
<name_caption x="0" y="0" width="0" height="0"/>
<content_text x="65" y="2" width="510" height="20" complex_mode="1"><text font="letterica18" r="216" g="186" b="140"/></content_text>
<msg_icon x="20" y="0" width="34" height="20"/></iconed_trade_info>
</window>"#;

/// `trade_character.xml` with the values of talk-window.md 5 (c1 D4).
pub(crate) const TRADE_CHARACTER: &str = r#"<window>
<icon_static x="18" y="35" width="165" height="108"/>
<name_static x="23" y="7" width="153" height="22" complex_mode="0"><text font="graffiti22" r="238" g="155" b="23"/></name_static>
<rank_static x="23" y="150" width="153" height="15"><text font="letterica16" r="240" g="217" b="182" align="r"/></rank_static>
<rank_caption x="23" y="150" width="153" height="15"><text font="letterica16" r="216" g="186" b="140" align="l">ui_st_rank</text></rank_caption>
<community_static x="23" y="164" width="153" height="15"><text font="letterica16" r="240" g="217" b="182" align="r"/></community_static>
<community_caption x="23" y="164" width="153" height="15"><text font="letterica16" r="216" g="186" b="140" align="l">ui_st_community</text></community_caption>
<relation_static x="23" y="177" width="153" height="15"><text font="letterica16" r="240" g="217" b="182" align="r"/></relation_static>
<relation_caption x="23" y="177" width="153" height="15"><text font="letterica16" r="216" g="186" b="140" align="l">ui_st_relation</text></relation_caption>
</window>"#;

/// The object section with the special mode (9), for the fixture's
/// `system.ltx`.
pub(crate) const SECTIONS: &str = "[m_osoznanie]\nneed_osoznanie_mode = true\n[stalker]\n";

/// `talk_16.xml`: `talk.xml` with the differences of talk-window.md 10.
fn talk_16() -> String {
    TALK.replace(
        r#"<left_character_icon x="15" y="32" width="200" height="218">"#,
        r#"<left_character_icon x="15" y="32" width="150" height="220" stretch="1">"#,
    )
    .replace(
        r#"<right_character_icon x="820" y="32" width="200" height="218">"#,
        r#"<right_character_icon x="850" y="32" width="150" height="220" stretch="1">"#,
    )
    .replace(
        r#"<answers_list x="33" y="46" width="544""#,
        r#"<answers_list x="20" y="46" width="557""#,
    )
    .replace(
        r#"<questions_list x="30" y="100" width="547" height="120""#,
        r#"<questions_list x="20" y="100" width="557" height="125""#,
    )
    .replace(r#"width="527""#, r#"width="557""#)
    .replace(
        r#"<content_text x="0" y="0" width="520" height="20">
<text font="letterica18" r="255" g="232" b="208" align="l" complex_mode="1"/>"#,
        r#"<content_text x="0" y="0" width="550" height="20">
<text font="letterica18" r="255" g="232" b="208" align="l"/>"#,
    )
    .replace(r#"width="510""#, r#"width="520""#)
    .replace(
        r#"<name_caption x="0" y="0" width="520""#,
        r#"<name_caption x="0" y="0" width="550""#,
    )
}

/// The line height *t* of 1024 × 768; complex texts wrapped at 10 units
/// per character, `\n` ending a line.
fn measure() -> Measure {
    Box::new(|font, text, width, complex| {
        let t = match font {
            UiFont::Letterica16 => 16.0,
            UiFont::Graffiti22 => 22.0,
            _ => 18.0,
        };
        if !complex {
            return t;
        }
        let text = String::from_utf8_lossy(text).into_owned();
        let lines: usize = (text.split("\\n"))
            .map(|l| ((l.len() as f32 * 10.0 / width).ceil() as usize).max(1))
            .sum();
        lines as f32 * t
    })
}

fn doc(text: &str) -> xml::Document {
    xml::parse(text.as_bytes()).unwrap()
}

fn layout_of(talk: &str) -> Layout {
    Layout::read(
        doc(talk),
        &doc(TRADE_CHARACTER),
        &doc(crate::pda_window::map_tests::SCROLL),
        &ColorDefs::default(),
    )
    .unwrap()
}

pub(crate) fn window_of(talk: &str) -> TalkWindow {
    TalkWindow::new(layout_of(talk), measure())
}

pub(crate) fn hero() -> Character {
    Character {
        id: 0,
        name: "Hero".into(),
        rank: "novice".into(),
        community: "stalker".into(),
        reputation: "good".into(),
        icon: Some(Icon {
            texture: "ui\\ui_inventory2".into(),
            rect: Some([0.0, 0.0, 128.0, 128.0]),
        }),
    }
}

pub(crate) fn wolf() -> Character {
    Character {
        id: 7,
        name: "Wolf".into(),
        rank: "veteran".into(),
        community: "stalker".into(),
        reputation: "good".into(),
        icon: None,
    }
}

fn rect4(r: Rect) -> [f32; 4] {
    [r.x, r.y, r.width, r.height]
}

/// 2, 4, 5: every element at the table's absolute rectangle; the lists
/// relative to their frame lines.
#[test]
fn the_elements_stand_at_the_tables_rectangles() {
    let l = layout_of(TALK);
    assert_eq!(rect4(l.top_background.rect), [0.0, 0.0, 1024.0, 128.0]);
    assert_eq!(rect4(l.bottom_background.rect), [0.0, 736.0, 1024.0, 32.0]);
    assert_eq!(rect4(l.left_icon.rect), [15.0, 32.0, 200.0, 218.0]);
    assert_eq!(rect4(l.right_icon.rect), [820.0, 32.0, 200.0, 218.0]);
    assert_eq!(rect4(l.left_panel.rect), [15.0, 32.0, 200.0, 218.0]);
    assert_eq!(rect4(l.right_panel.rect), [820.0, 32.0, 200.0, 218.0]);
    assert_eq!(rect4(l.frame_lines[0].rect), [218.0, 42.0, 600.0, 470.0]);
    assert_eq!(rect4(l.frame_lines[1].rect), [218.0, 485.0, 600.0, 250.0]);
    assert!(l.frame_lines.iter().all(|f| f.vertical));
    assert_eq!(rect4(l.answers.rect), [251.0, 88.0, 544.0, 377.0]);
    assert_eq!(rect4(l.questions.rect), [248.0, 585.0, 547.0, 120.0]);
    assert_eq!(rect4(l.button.rect), [437.0, 726.0, 157.0, 48.0]);
    let title = |i: usize| rect4(l.frame_lines[i].title.as_ref().unwrap().rect);
    assert_eq!(title(0), [243.0, 45.0, 190.0, 22.0]);
    assert_eq!(title(1), [243.0, 545.0, 190.0, 22.0]);
    let at = |p: &Option<Placed>| {
        let r = p.as_ref().unwrap().rect;
        [r.x, r.y]
    };
    for (p, x) in [(&l.left_panel, 15.0), (&l.right_panel, 820.0)] {
        assert_eq!(at(&p.icon), [x + 18.0, 67.0]);
        assert_eq!(at(&p.name), [x + 23.0, 39.0]);
        assert_eq!(at(&p.rank_caption), [x + 23.0, 182.0]);
        assert_eq!(at(&p.community), [x + 23.0, 196.0]);
        assert_eq!(at(&p.relation), [x + 23.0, 209.0]);
        assert!(p.reputation.is_none() && p.biography.is_none());
    }
    let (_, strings) = l.references();
    assert_eq!(
        strings,
        [
            "ui_st_community",
            "ui_st_rank",
            "ui_st_relation",
            "ui_st_trade"
        ]
    );
}

/// 10: the wide-screen layout's lists and pictures; a choice is one line.
#[test]
fn the_wide_layout_moves_the_lists_and_does_not_wrap_choices() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let mut w = window_of(&talk_16());
    let l = w.layout();
    assert_eq!(rect4(l.answers.rect), [238.0, 88.0, 557.0, 377.0]);
    assert_eq!(rect4(l.questions.rect), [238.0, 585.0, 557.0, 125.0]);
    assert_eq!(rect4(l.right_icon.rect), [850.0, 32.0, 150.0, 220.0]);
    assert_eq!(rect4(l.left_panel.rect), [15.0, 32.0, 150.0, 220.0]);
    assert!(l.left_icon.desc.stretch);
    w.set_choices(&data, &["one\\ntwo", &"x".repeat(80)])
        .unwrap();
    let s = w.state();
    assert_eq!(s.questions[0].height, 18.0);
    assert_eq!(s.questions[1].height, 18.0);
    w.add_phrase(&data, "Wolf", "a\\nb").unwrap();
    // The answers still wrap: two lines.
    assert_eq!(w.state().answers[0].height, 63.0);
}

/// 6.3, 7: the worked heights at 1024 × 768.
#[test]
fn entries_take_the_worked_heights() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let mut w = window_of(TALK);
    w.start(&data, &hero(), &wolf(), false);
    w.set_shown(true);
    for (speaker, text) in [
        ("Wolf", "Hello"),
        ("Wolf", "Hello\\nagain"),
        ("Hero", "Hello"),
        ("Hero", "Hello\\nagain"),
    ] {
        w.add_phrase(&data, speaker, text).unwrap();
    }
    let icon = || Icon {
        texture: "ui\\ui_iconstotal".into(),
        rect: Some([0.0, 0.0, 83.0, 47.0]),
    };
    w.add_iconed_line(&data, "Got it", icon(), None).unwrap();
    w.add_iconed_line(&data, "Sold", icon(), Some("iconed_trade_info"))
        .unwrap();
    w.set_choices(&data, &["Bye", "Tell me\\nmore"]).unwrap();
    let s = w.state();
    let heights: Vec<f32> = s.answers.iter().map(|e| e.height).collect();
    assert_eq!(heights, [45.0, 63.0, 50.0, 68.0, 35.0, 25.0]);
    let templates: Vec<&str> = s.answers.iter().map(|e| e.template.as_str()).collect();
    assert_eq!(
        templates,
        [
            OTHER_ANSWER,
            OTHER_ANSWER,
            ACTOR_ANSWER,
            ACTOR_ANSWER,
            ICONED_ANSWER,
            "iconed_trade_info"
        ]
    );
    let heights: Vec<f32> = s.questions.iter().map(|e| e.height).collect();
    assert_eq!(heights, [18.0, 36.0]);
    // 7: stacked from the view's top at `right_ident`.
    assert_eq!(s.questions[0].at, [253.0, 585.0]);
    assert_eq!(s.questions[1].at, [253.0, 603.0]);
}

/// 6: while the entries fit they stand from the view's top; then the last
/// ends one unit above its bottom, the bar shown.
#[test]
fn the_log_scrolls_to_its_end() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let mut w = window_of(TALK);
    w.start(&data, &hero(), &wolf(), false);
    for _ in 0..8 {
        w.add_phrase(&data, "Wolf", "Hello").unwrap();
    }
    let s = w.state();
    assert_eq!((s.answers_scroll, s.answers_bar), (0, false));
    assert_eq!(s.answers[7].at, [251.0, 88.0 + 7.0 * 45.0]);
    w.add_phrase(&data, "Wolf", "Hello").unwrap();
    let s = w.state();
    // 9 · 45 = 405: 405 − 377 + 1.
    assert_eq!((s.answers_scroll, s.answers_bar), (29, true));
    let bottom = s.answers[8].at[1] + 45.0 - s.answers_scroll as f32;
    assert_eq!(bottom, 88.0 + 377.0 - 1.0);
    // 7: adding choices does not scroll; the next talk start empties both.
    let keys: Vec<String> = (0..8).map(|i| format!("choice {i}")).collect();
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    w.set_choices(&data, &keys).unwrap();
    let s = w.state();
    assert_eq!((s.questions_scroll, s.questions_bar), (0, true));
    w.start(&data, &hero(), &wolf(), false);
    let s = w.state();
    assert!(s.answers.is_empty() && s.questions.is_empty());
    assert_eq!((s.answers_scroll, s.answers_bar), (0, false));
}

/// 6.1: the template by the speaker's name, a partner of the player
/// character's name included; each phrase a news record with its
/// speaker's panel's picture. 6.2: iconed lines only while shown, no
/// record.
#[test]
fn phrases_choose_their_template_by_name_and_make_news() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let mut w = window_of(TALK);
    w.start(&data, &hero(), &wolf(), false);
    w.add_phrase(&data, "Wolf", "Hello").unwrap();
    w.add_phrase(&data, "Hero", "Hi").unwrap();
    let news = w.take_news();
    assert_eq!(news.len(), 2);
    assert_eq!(news[0].text, "Wolf >> Hello");
    assert_eq!(news[0].icon, None, "the partner has no icon");
    assert_eq!(news[1].text, "Hero >> Hi");
    assert_eq!(news[1].icon, hero().icon);
    let icon = Icon {
        texture: "ui\\ui_iconstotal".into(),
        rect: None,
    };
    w.add_iconed_line(&data, "Got it", icon.clone(), None)
        .unwrap();
    assert_eq!(w.state().answers.len(), 2, "not shown: not added");
    w.set_shown(true);
    w.add_iconed_line(&data, "Got it", icon, None).unwrap();
    let s = w.state();
    assert_eq!(s.answers.len(), 3);
    assert_eq!(
        (s.answers[2].name.as_str(), s.answers[2].text.as_str()),
        ("", "Got it")
    );
    assert!(w.take_news().is_empty());
    // A partner named as the player character speaks with its template.
    let mut twin = wolf();
    twin.name = "Hero".into();
    w.start(&data, &hero(), &twin, false);
    w.add_phrase(&data, "Hero", "Hello").unwrap();
    assert_eq!(w.state().answers[0].template, ACTOR_ANSWER);
}

/// 4, 5, 8, 9: the titles and panels at talk start; the special mode
/// hides the pictures, the log, frame line 0 and the button.
#[test]
fn a_talk_start_sets_titles_panels_and_the_special_mode() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let mut w = window_of(TALK);
    w.start(&data, &hero(), &wolf(), false);
    let s = w.state();
    assert_eq!(s.titles, ["Wolf".to_owned(), "Hero".to_owned()]);
    assert_eq!(s.panels[0].name.as_deref(), Some("Hero"));
    assert_eq!(s.panels[1].name.as_deref(), Some("Wolf"));
    assert_eq!(s.panels[1].rank.as_deref(), Some("veteran"));
    assert!(s.button_shown && !s.special_mode);
    assert!(has_special_mode(&data, "m_osoznanie"));
    assert!(!has_special_mode(&data, "stalker"));
    assert!(!has_special_mode(&data, "no_such_section"));
    w.start(&data, &hero(), &wolf(), true);
    let s = w.state();
    assert!(!s.button_shown && s.special_mode);
    w.start(&data, &hero(), &wolf(), false);
    assert!(w.state().button_shown);
}

/// 5: refreshed at the first update after a fill, then every 100th
/// frame: the relation line for the partner only, its colour by kind, the
/// picture tinted at health 0 or less; a vanished character is forgotten.
#[test]
fn panels_refresh_their_relation_and_tint() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let mut w = window_of(TALK);
    w.start(&data, &hero(), &wolf(), false);
    let standing = std::cell::Cell::new((2, 1.0));
    let lookup = |_: u16| {
        let (relation, health) = standing.get();
        Some(Standing {
            relation,
            relation_name: format!("relation {relation}"),
            health,
        })
    };
    w.update(&data, 7, &lookup);
    let s = w.state();
    assert!(!s.panels[0].relation_shown);
    assert!(s.panels[1].relation_shown);
    assert_eq!(s.panels[1].relation.as_deref(), Some("relation 2"));
    assert_eq!(s.panels[1].relation_color, Some([255, 0, 0, 255]));
    standing.set((0, 0.0));
    w.update(&data, 8, &lookup);
    assert_eq!(w.state().panels[1].relation_color, Some([255, 0, 0, 255]));
    w.update(&data, 100, &lookup);
    let s = w.state();
    assert_eq!(s.panels[1].relation_color, Some([0, 255, 0, 255]));
    assert_eq!(s.panels[1].icon_color, Some([255, 160, 160, 255]));
    assert_eq!(relation_color(1), Color::rgba(192, 192, 192, 255));
    w.update(&data, 200, &|_| None);
    assert_eq!(w.state().panels[1].character, None);
}

/// The layers of a drawing, in order, deduplicated.
fn layers(s: &Scene) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for sp in &s.sprites {
        if out.last() != Some(&sp.layer) {
            out.push(sp.layer.clone());
        }
    }
    out
}

/// The parts of `layer` as (top, bottom) in units at 1024 × 768.
fn spans(s: &Scene, layer: &str) -> Vec<(f32, f32)> {
    let mut v: Vec<(f32, f32)> = (s.sprites.iter())
        .filter(|sp| sp.layer == layer)
        .map(|sp| {
            let b = sp.bounds();
            assert_eq!((b[0] + 0.5, b[2] + 0.5), (218.0, 818.0), "{layer}");
            (b[1] + 0.5, b[3] + 0.5)
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

/// 2, 4: the drawing order, each list after its frame line's band and
/// title; the worked bands; the special mode leaving the backgrounds and
/// frame line 1 with its list.
#[test]
fn the_window_draws_in_order_with_the_worked_bands() {
    let tree = fixture::tree();
    let data = fixture::data(&tree);
    let mut assets = fixture::assets();
    let res = Resolution::new(1024, 768);
    let fonts = assets.fonts(&tree, &data, res.height).unwrap().clone();
    let mut w = TalkWindow::load(&tree, &data, res, &fonts).unwrap();
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
    assert!(s.sprites.is_empty(), "hidden");
    w.set_shown(true);
    w.start(&data, &hero(), &wolf(), false);
    w.add_phrase(&data, "Wolf", "Hello").unwrap();
    w.set_choices(&data, &["Bye"]).unwrap();
    w.draw(&mut ctx, &mut s).unwrap();
    let order = layers(&s);
    let expected = [
        "talk top_background",
        "talk bottom_background",
        "talk left_character_icon",
        "talk left panel name text",
        "talk left panel rank_caption text",
        "talk left panel community_caption text",
        "talk right_character_icon",
        "talk right panel name text",
        "talk right panel rank_caption text",
        "talk right panel community_caption text",
        "talk right panel relation_caption text",
        "talk frame line 0",
        "talk frame line 0 title text",
        "talk answer 0 content_text",
        "talk answer 0 name_caption text",
        "talk frame line 1",
        "talk frame line 1 title text",
        "talk question 0",
        "talk button",
        "talk button text",
    ];
    let shown: Vec<&str> = (order.iter())
        .map(String::as_str)
        .filter(|l| expected.contains(l))
        .collect();
    assert_eq!(shown, expected, "{order:?}");
    // Before a refresh both panels show their relation caption; after
    // one, the player character's does not.
    assert!(
        order
            .iter()
            .any(|l| l == "talk left panel relation_caption text")
    );
    // The worked bands (4).
    assert_eq!(
        spans(&s, "talk frame line 0"),
        [
            (42.0, 117.0),
            (117.0, 213.0),
            (213.0, 309.0),
            (309.0, 405.0),
            (405.0, 472.0),
            (472.0, 512.0)
        ]
    );
    assert_eq!(
        spans(&s, "talk frame line 1"),
        [
            (485.0, 583.0),
            (583.0, 679.0),
            (679.0, 695.0),
            (695.0, 735.0)
        ]
    );
    // 3: the left picture from (4, 32) at its pixel size.
    let left = (s.sprites.iter())
        .find(|sp| sp.layer == "talk left_character_icon")
        .unwrap()
        .bounds();
    assert_eq!(left.map(|v| v + 0.5), [4.0, 32.0, 214.0, 250.0]);
    // 9: with the special mode only the backgrounds, frame line 1, its
    // title and the question list.
    w.update(&data, 1, &|_| None);
    w.start(&data, &hero(), &wolf(), true);
    w.set_choices(&data, &["Bye"]).unwrap();
    let mut s = Scene::new(res);
    w.draw(&mut ctx, &mut s).unwrap();
    assert_eq!(
        layers(&s),
        [
            "talk top_background",
            "talk bottom_background",
            "talk frame line 1",
            "talk frame line 1 title text",
            "talk question 0",
        ]
    );
}
