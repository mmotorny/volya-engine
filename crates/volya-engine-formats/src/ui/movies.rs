// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Movie sequences of `config\ui\ui_movies.xml` (reached through
//! `config\ui\game_tutorials.xml`), such as the logo sequence `intro_logo`
//! (docs/behavior/startup.md, section 6.1): a background window, then items
//! played in document order, each a video with its sound or a set of timed
//! pictures.

use std::io;

use super::layout::{StaticDesc, TextureRef};
use super::{ColorDefs, flag_attr, float_attr};
use crate::ltx::value;
use crate::xml::Element;

/// A picture of an image item, visible for part of the item
/// (startup.md section 6.4).
#[derive(Clone, Debug, PartialEq)]
pub struct TimedPicture {
    pub desc: StaticDesc,
    /// `start_time`, seconds from the item's start (default 0).
    pub start_time: f32,
    /// `length_sec`, seconds (default 0: never visible).
    pub length_sec: f32,
    /// `light_anim`: the colour animation applied while visible.
    pub light_anim: Option<String>,
    /// `la_alpha` (default 0): only the opacity comes from the animation.
    pub la_alpha: bool,
    /// `la_texture` (default 1): the animation applies to the picture.
    pub la_texture: bool,
}

impl TimedPicture {
    fn read(e: &Element, defs: &ColorDefs) -> io::Result<Self> {
        Ok(Self {
            desc: StaticDesc::read(e, defs)?,
            start_time: float_attr(e, "start_time", 0.0),
            length_sec: float_attr(e, "length_sec", 0.0),
            light_anim: e
                .attr("light_anim")
                .filter(|a| !a.is_empty())
                .map(str::to_owned),
            la_alpha: flag_attr(e, "la_alpha", false),
            la_texture: flag_attr(e, "la_texture", true),
        })
    }

    /// Whether the picture shows at `t` seconds into its item: strictly
    /// after `start_time` and strictly before its end (section 6.4).
    pub fn visible_at(&self, t: f32) -> bool {
        t > self.start_time && t < self.start_time + self.length_sec
    }
}

/// What an item shows.
#[derive(Clone, Debug, PartialEq)]
pub enum MovieItemKind {
    /// `type="video"` (section 6.3).
    Video {
        /// `delay` in seconds; absent, zero or negative means none.
        delay: f32,
        /// `sound` *S*: the pair `sounds\S_l.ogg`, `sounds\S_r.ogg`.
        sound: Option<String>,
        /// `video_wnd`, whose `texture` child names the video (a texture
        /// path) and the part of the decoded frame shown.
        window: Box<StaticDesc>,
        texture: TextureRef,
        /// `can_be_stopped` (default on).
        can_be_stopped: bool,
    },
    /// `type="image"` (section 6.4).
    Image {
        /// `length_sec`: the item's duration in seconds.
        length_sec: f32,
        /// `main_wnd`'s `auto_static` children, in document order.
        pictures: Vec<TimedPicture>,
        /// `guard_key`, when present.
        guard_key: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MovieItem {
    pub kind: MovieItemKind,
    /// `pause_state` value (no visible effect at launch was identified).
    pub pause_state: Option<String>,
}

/// A movie sequence such as `<intro_logo>`.
#[derive(Clone, Debug, PartialEq)]
pub struct MovieSequence {
    /// `global_wnd`'s `auto_static` children: the background behind every
    /// item, relative to `global_wnd`.
    pub global_wnd: Option<StaticDesc>,
    /// `play_each_item` (the spec describes only 1).
    pub play_each_item: bool,
    pub items: Vec<MovieItem>,
}

fn child_value<'a>(e: &'a Element, name: &str) -> Option<&'a str> {
    e.child(name).and_then(Element::value)
}

impl MovieSequence {
    /// Reads sequence element `e` (e.g. `intro_logo`). Items of an unknown
    /// `type` are not covered by the spec and are rejected, as is a video
    /// item without `video_wnd` and `texture`.
    pub fn read(e: &Element, defs: &ColorDefs) -> io::Result<Self> {
        let invalid = |m: String| io::Error::new(io::ErrorKind::InvalidData, m);
        let mut items = Vec::new();
        for (i, item) in e.children_named("item").enumerate() {
            let kind = match item.attr("type") {
                Some("video") => {
                    let wnd = item
                        .child("video_wnd")
                        .ok_or_else(|| invalid(format!("{} item {i}: no video_wnd", e.name)))?;
                    let texture = wnd
                        .child("texture")
                        .and_then(|t| TextureRef::read(t, defs))
                        .ok_or_else(|| invalid(format!("{} item {i}: no texture", e.name)))?;
                    MovieItemKind::Video {
                        delay: child_value(item, "delay").map_or(0.0, value::float),
                        sound: child_value(item, "sound").map(str::to_owned),
                        window: Box::new(StaticDesc::read(wnd, defs)?),
                        texture,
                        can_be_stopped: child_value(item, "can_be_stopped")
                            .is_none_or(|v| value::int(v) != 0),
                    }
                }
                Some("image") => MovieItemKind::Image {
                    length_sec: child_value(item, "length_sec").map_or(0.0, value::float),
                    pictures: item
                        .child("main_wnd")
                        .map(|w| {
                            w.children_named("auto_static")
                                .map(|s| TimedPicture::read(s, defs))
                                .collect::<io::Result<_>>()
                        })
                        .transpose()?
                        .unwrap_or_default(),
                    guard_key: child_value(item, "guard_key").map(str::to_owned),
                },
                other => {
                    return Err(invalid(format!(
                        "{} item {i}: unknown type {other:?}",
                        e.name
                    )));
                }
            };
            items.push(MovieItem {
                kind,
                pause_state: child_value(item, "pause_state").map(str::to_owned),
            });
        }
        Ok(Self {
            global_wnd: e
                .child("global_wnd")
                .map(|w| StaticDesc::read(w, defs))
                .transpose()?,
            play_each_item: child_value(e, "play_each_item").is_some_and(|v| value::int(v) != 0),
            items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn reads_items() {
        let doc = parse(
            br#"<w><intro_logo>
            <global_wnd x="0" y="0" width="1024" height="768">
              <auto_static x="0" y="0" width="1024" height="768" stretch="1"><texture>intro\intro_back</texture></auto_static>
            </global_wnd>
            <play_each_item>1</play_each_item>
            <item type="video"><delay>1</delay><pause_state>on</pause_state>
              <sound>a\b</sound>
              <video_wnd x="0" y="0" width="1024" height="768" stretch="1">
                <texture x="0" y="1" width="720" height="484">intro\thq</texture></video_wnd></item>
            <item type="image"><length_sec>9</length_sec>
              <main_wnd><auto_static start_time="4" length_sec="4" x="0" y="0" width="1024"
                height="768" stretch="1" light_anim="asus_logo_01" la_alpha="1">
                <texture>ui_asus_02</texture></auto_static></main_wnd></item>
            </intro_logo></w>"#,
        )
        .unwrap();
        let s =
            MovieSequence::read(doc.find("intro_logo", 0).unwrap(), &ColorDefs::default()).unwrap();
        assert!(s.play_each_item);
        let global = s.global_wnd.unwrap();
        assert_eq!(global.window.auto_statics.len(), 1);
        assert_eq!(s.items.len(), 2);
        let MovieItemKind::Video {
            delay,
            sound,
            texture,
            can_be_stopped,
            ..
        } = &s.items[0].kind
        else {
            panic!("not a video");
        };
        assert_eq!((*delay, sound.as_deref()), (1.0, Some("a\\b")));
        assert_eq!(texture.name, "intro\\thq");
        assert_eq!(texture.rect.unwrap().height, 484.0);
        assert!(can_be_stopped);
        assert_eq!(s.items[0].pause_state.as_deref(), Some("on"));
        let MovieItemKind::Image {
            length_sec,
            pictures,
            guard_key,
        } = &s.items[1].kind
        else {
            panic!("not an image");
        };
        assert_eq!((*length_sec, guard_key), (9.0, &None));
        let p = &pictures[0];
        assert_eq!((p.start_time, p.length_sec), (4.0, 4.0));
        assert_eq!(p.light_anim.as_deref(), Some("asus_logo_01"));
        assert!(p.la_alpha && p.la_texture);
        // Strict bounds (section 6.4).
        assert!(!p.visible_at(4.0) && p.visible_at(4.001) && !p.visible_at(8.0));
    }
}
