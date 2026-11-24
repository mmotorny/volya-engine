// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the options screen reads from its layout data
//! (docs/behavior/options-screens.md): the option controls of
//! `config\ui\ui_mm_opt.xml` with their `options_item` and control
//! attributes, the key table of `config\ui\ui_keybinding.xml`
//! (section 11.3) and the scroll-bar profile of `config\ui\scroll_bar.xml`
//! (section 4.8). Which elements the screen builds is the screen's business.

use std::io;

use super::layout::{FontDesc, StaticDesc, require};
use super::{Color, ColorDefs, flag_attr, float_attr, int_attr, read_color};
use crate::xml::{Document, Element};

/// An element's `options_item` child: the variable it edits and its group
/// (sections 5.1 and 6). The spec names the `group` attribute; that `entry`
/// names the variable is our reading of the data (each control's `entry`
/// is the variable section 6 gives it), used to bind the control to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsItem {
    pub entry: String,
    pub group: String,
}

/// An element of the options screen: a static with the attributes of the
/// option controls (sections 4.4–4.6).
#[derive(Clone, Debug, PartialEq)]
pub struct ControlDesc {
    pub desc: StaticDesc,
    pub item: Option<OptionsItem>,
    /// Drop-down: rows of the open list (default 4).
    pub list_length: usize,
    /// Drop-down: `always_show_scroll` exactly 1 (the default).
    pub always_show_scroll: bool,
    /// Drop-down: the rows' font and colour (`list_font`), the selected
    /// row's colour (`list_font_s`); colour defaults 0 (ui-layout.md 9.2).
    pub list_font: Option<FontDesc>,
    pub list_font_s: Option<Color>,
    /// Track bar: `step` (default 0.1), `is_integer`, `invert`.
    pub step: f32,
    pub is_integer: bool,
    pub invert: bool,
}

impl ControlDesc {
    pub fn read(e: &Element, defs: &ColorDefs) -> io::Result<Self> {
        Ok(Self {
            desc: StaticDesc::read(e, defs)?,
            item: e.child("options_item").map(|i| OptionsItem {
                entry: i.attr("entry").unwrap_or_default().to_owned(),
                group: i.attr("group").unwrap_or_default().to_owned(),
            }),
            list_length: int_attr(e, "list_length", 4).max(1) as usize,
            always_show_scroll: int_attr(e, "always_show_scroll", 1) == 1,
            list_font: e
                .child("list_font")
                .map(|f| {
                    Ok::<_, io::Error>(FontDesc {
                        font: super::layout::font_attr(f)?,
                        color: read_color(f, defs, 0),
                    })
                })
                .transpose()?,
            list_font_s: e.child("list_font_s").map(|f| read_color(f, defs, 0)),
            step: float_attr(e, "step", 0.1),
            is_integer: flag_attr(e, "is_integer", false),
            invert: flag_attr(e, "invert", false),
        })
    }

    /// The element at `path` of `doc`, which must exist.
    pub fn require(doc: &Document, path: &str, defs: &ColorDefs) -> io::Result<Self> {
        Self::read(require(doc, path, 0)?, defs)
    }
}

/// A command row of the key table: the caption key and the action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCommand {
    pub id: String,
    pub exe: String,
}

/// A group of the key table: its caption key and commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyGroup {
    pub name: String,
    pub commands: Vec<KeyCommand>,
}

/// The `group` elements of `ui_keybinding.xml`'s root, in document order
/// (section 11.3).
pub fn key_groups(doc: &Document) -> Vec<KeyGroup> {
    let Some(root) = doc.root() else {
        return Vec::new();
    };
    root.children_named("group")
        .map(|g| KeyGroup {
            name: g.attr("name").unwrap_or_default().to_owned(),
            commands: g
                .children_named("command")
                .map(|c| KeyCommand {
                    id: c.attr("id").unwrap_or_default().to_owned(),
                    exe: c.attr("exe").unwrap_or_default().to_owned(),
                })
                .collect(),
        })
        .collect()
}

/// A vertical scroll bar's look: a profile of `scroll_bar.xml`
/// (section 4.8).
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollBarProfile {
    /// The profile's `height`: the bar's width *t*.
    pub width: f32,
    /// Texture of the up and down arrows (`up_arrow:texture_e`,
    /// `down_arrow:texture_e`), the track (`back_v:texture`) and the box
    /// (`box_v:texture`). The spec names the texture ids (section 4.8) but
    /// not these elements: they are our reading of the file's structure (in
    /// the shipped `default` profile they hold exactly those ids), used to
    /// draw the scroll bars of the scroll views and drop-down lists.
    pub up: String,
    pub down: String,
    pub track: String,
    pub thumb: String,
}

impl ScrollBarProfile {
    /// Profile `name` of `doc`.
    pub fn read(doc: &Document, name: &str) -> io::Result<Self> {
        let profile = require(doc, name, 0)?;
        let texture = |part: &str, child: &str| -> io::Result<String> {
            profile
                .child(part)
                .and_then(|p| p.child(child))
                .and_then(|t| t.value())
                .map(str::to_owned)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("scroll bar profile {name}: no {part}:{child}"),
                    )
                })
        };
        Ok(Self {
            width: float_attr(profile, "height", 0.0),
            up: texture("up_arrow", "texture_e")?,
            down: texture("down_arrow", "texture_e")?,
            track: texture("back_v", "texture")?,
            thumb: texture("box_v", "texture")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn reads_controls_key_groups_and_scroll_bars() {
        let doc = parse(
            br#"<window><tab_video>
            <list_presets x="194" y="50" width="261" height="21" list_length="5">
              <options_item entry="_preset" group="mm_opt_video_preset"/>
              <text_color><e r="216" g="186" b="140"/></text_color>
              <list_font_s r="240" g="217" b="182"/>
              <list_font r="216" g="186" b="140" font="letterica16"/>
            </list_presets>
            <track x="194" y="0" width="245" height="21" invert="1" step="1" is_integer="1"/>
            </tab_video></window>"#,
        )
        .unwrap();
        let defs = ColorDefs::default();
        let l = ControlDesc::require(&doc, "tab_video:list_presets", &defs).unwrap();
        assert_eq!(
            l.item,
            Some(OptionsItem {
                entry: "_preset".into(),
                group: "mm_opt_video_preset".into()
            })
        );
        assert_eq!((l.list_length, l.always_show_scroll), (5, true));
        assert_eq!(l.list_font_s, Some(Color::rgba(240, 217, 182, 255)));
        assert_eq!(l.step, 0.1);
        let t = ControlDesc::require(&doc, "tab_video:track", &defs).unwrap();
        assert_eq!(
            (t.step, t.is_integer, t.invert, t.item),
            (1.0, true, true, None)
        );
        assert!(ControlDesc::require(&doc, "tab_video:nothing", &defs).is_err());

        let keys = parse(
            br#"<keybingidg><group name="kb_grp_direction">
            <command id="kb_left" exe="left"/><command id="kb_right" exe="right"/>
            </group><group name="g2"/></keybingidg>"#,
        )
        .unwrap();
        let groups = key_groups(&keys);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0].commands[1],
            KeyCommand {
                id: "kb_right".into(),
                exe: "right".into()
            }
        );

        let bars = parse(
            br#"<scroll_bar><default height="16">
            <up_arrow><_texture_e>old</_texture_e><texture_e>ui_scroll_btn_up</texture_e></up_arrow>
            <down_arrow><texture_e>ui_scroll_btn_down</texture_e></down_arrow>
            <back_v><texture>ui_scroll_back</texture></back_v>
            <box_v><texture>ui_scroll_box</texture></box_v></default></scroll_bar>"#,
        )
        .unwrap();
        let p = ScrollBarProfile::read(&bars, "default").unwrap();
        assert_eq!(
            (p.width, p.up.as_str(), p.track.as_str()),
            (16.0, "ui_scroll_btn_up", "ui_scroll_back")
        );
    }
}
