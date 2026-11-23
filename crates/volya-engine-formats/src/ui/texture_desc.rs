// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Texture descriptions: texture ids → texture file and pixel rectangle
//! (docs/formats/ui-layout.md, section 6).

use std::collections::HashMap;
use std::io;

use super::float_attr;
use super::layout::Rect;
use crate::ltx::Config;
use crate::xml::Document;

/// One texture id's texture path (as written in `file_name`) and rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureDesc {
    pub file: String,
    pub rect: Rect,
}

/// The names of the description files: `[texture_desc] files` of
/// `config\system.ltx`, comma-separated, items trimmed (section 6.1). Each
/// gets `.xml` appended by the loader (as a `ui` file). No section: none.
pub fn file_names(system: &Config) -> Vec<String> {
    match system.get("texture_desc", "files") {
        Some(Some(files)) => files
            .split(',')
            .map(|f| crate::xml::trim(f).to_owned())
            .filter(|f| !f.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// All loaded texture descriptions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureDescriptions {
    ids: HashMap<String, TextureDesc>,
}

impl TextureDescriptions {
    /// Adds the entries of one description file (section 6.2): the root's
    /// `file_name` child names the texture, each `texture` child of the root
    /// gives an `id` and a rectangle. An id already defined keeps its first
    /// definition. The spec does not cover a file without `file_name` or an
    /// entry without `id`; both are rejected.
    pub fn add_file(&mut self, doc: &Document) -> io::Result<()> {
        let invalid = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_owned());
        let root = doc
            .root()
            .ok_or_else(|| invalid("empty texture description"))?;
        let file = root
            .child("file_name")
            .and_then(|f| f.value())
            .ok_or_else(|| invalid("texture description without file_name"))?;
        for e in root.children_named("texture") {
            let id = e
                .attr("id")
                .ok_or_else(|| invalid("texture description entry without id"))?;
            self.ids
                .entry(id.to_owned())
                .or_insert_with(|| TextureDesc {
                    file: file.to_owned(),
                    rect: Rect::new(
                        float_attr(e, "x", 0.0),
                        float_attr(e, "y", 0.0),
                        float_attr(e, "width", 0.0),
                        float_attr(e, "height", 0.0),
                    ),
                });
        }
        Ok(())
    }

    /// The description of texture id `id` (exact, case-sensitive).
    pub fn get(&self, id: &str) -> Option<&TextureDesc> {
        self.ids.get(id)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Every id with its description, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &TextureDesc)> {
        self.ids.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ltx;
    use crate::xml::parse;

    #[test]
    fn first_definition_wins() {
        let mut d = TextureDescriptions::default();
        d.add_file(
            &parse(
                b"<ui_texture><file_name>ui\\ui_mainMenu</file_name>\
                <texture id=\"ui_menu_move\" x=\"7\" y=\"951\" width=\"392\" height=\"73\" />\
                </ui_texture>",
            )
            .unwrap(),
        )
        .unwrap();
        d.add_file(
            &parse(
                b"<ui_texture><file_name>other</file_name>\
                <texture id=\"ui_menu_move\"/><texture id=\"Ui_menu_move\" x=\"1.5\"/></ui_texture>",
            )
            .unwrap(),
        )
        .unwrap();
        let m = d.get("ui_menu_move").unwrap();
        assert_eq!(m.file, "ui\\ui_mainMenu");
        assert_eq!(m.rect, Rect::new(7.0, 951.0, 392.0, 73.0));
        let other = d.get("Ui_menu_move").unwrap();
        assert_eq!((other.file.as_str(), other.rect.x), ("other", 1.5));
        assert_eq!(d.len(), 2);
        assert!(d.add_file(&parse(b"<ui_texture/>").unwrap()).is_err());
    }

    #[test]
    fn lists_files() {
        let system =
            ltx::parse(b"[texture_desc]\nfiles = ui_mp_icon_rank, ui_icons_map ,ui_common\n")
                .unwrap();
        assert_eq!(
            file_names(&system),
            ["ui_mp_icon_rank", "ui_icons_map", "ui_common"]
        );
        assert!(file_names(&ltx::parse(b"[x]\n").unwrap()).is_empty());
    }
}
