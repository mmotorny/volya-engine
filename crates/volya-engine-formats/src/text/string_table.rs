// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! String tables: string keys → localized text
//! (docs/formats/text-and-fonts.md, section 1).

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;

use crate::ltx::Config;
use crate::xml::{self, Document, Node, is_space};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// `[string_table]` of `config\localization.ltx` (section 1.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Localization {
    /// The language directory, e.g. `rus`.
    pub language: String,
    /// Table names in load order, trimmed.
    pub files: Vec<String>,
    /// `font_prefix` (section 2.4), `None` when empty.
    pub font_prefix: Option<String>,
}

impl Localization {
    /// The spec does not cover a missing `language`; it is rejected. Empty
    /// items of `files` (a trailing comma) are skipped.
    pub fn read(config: &Config) -> io::Result<Self> {
        let get = |k| config.get("string_table", k).flatten();
        let language = get("language")
            .ok_or_else(|| invalid("localization.ltx: [string_table] without language"))?
            .to_owned();
        // TODO(#330): a missing `files` or `font_prefix`.
        let files = get("files")
            .map(|f| {
                f.split(',')
                    .map(|n| xml::trim(n).to_owned())
                    .filter(|n| !n.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            language,
            files,
            font_prefix: get("font_prefix").map(str::to_owned),
        })
    }

    /// The path of table `name` relative to `$game_config$` (section 1.2).
    pub fn table_path(&self, name: &str) -> String {
        format!("text\\{}\\{name}.xml", self.language)
    }
}

/// Collapses white space (section 1.3): leading and trailing removed, every
/// inner run becomes one space.
pub fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split(is_space).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// The entries of one parsed table in document order (section 1.3): the
/// root's direct `string` children, keyed by `id`, valued by the normalized
/// text content of their first `text` child. The spec leaves entries without
/// `id` or without `text` unestablished; both are rejected (the edition has
/// none).
pub fn entries(doc: &Document) -> io::Result<Vec<(String, String)>> {
    let Some(root) = doc.root() else {
        return Ok(Vec::new());
    };
    root.children_named("string")
        .map(|s| {
            let id = s
                .attr("id")
                .ok_or_else(|| invalid("string table entry without id"))?;
            let text = s
                .child("text")
                .ok_or_else(|| invalid(format!("string table entry {id} without text")))?;
            let content: String = text
                .children
                .iter()
                .filter_map(|n| match n {
                    Node::Text(t) => Some(t.as_str()),
                    Node::Element(_) => None,
                })
                .collect();
            Ok((id.to_owned(), normalize_whitespace(&content)))
        })
        .collect()
}

/// Key names bound to an action (lowercased name → primary and optional
/// secondary key name), as [`StringTable::apply_bindings`] uses them.
pub type Bindings = dyn Fn(&str) -> Option<(String, Option<String>)>;

/// The marker of a key-binding substitution (section 1.6).
const ACTION: &str = "$$ACTION_";

/// The loaded string table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    texts: HashMap<String, String>,
    /// Unsubstituted texts of entries containing `$$ACTION_…$$`.
    templates: HashMap<String, String>,
}

impl StringTable {
    /// Inserts entries in load order; a key seen again replaces the earlier
    /// text (section 1.4).
    pub fn extend(&mut self, entries: impl IntoIterator<Item = (String, String)>) {
        for (key, text) in entries {
            if text.contains(ACTION) {
                self.templates.insert(key.clone(), text.clone());
            } else {
                self.templates.remove(&key);
            }
            self.texts.insert(key, text);
        }
    }

    /// The text of `key` (section 1.4): exact match; a missing key returns
    /// the key itself; an empty key returns an empty string.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        if key.is_empty() {
            return "";
        }
        self.texts.get(key).map_or(key, String::as_str)
    }

    /// Whether `key` is in the table.
    pub fn contains(&self, key: &str) -> bool {
        self.texts.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.texts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.texts.is_empty()
    }

    /// The keys of entries using `$$ACTION_<name>$$`.
    pub fn templated_keys(&self) -> impl Iterator<Item = &str> {
        self.templates.keys().map(String::as_str)
    }

    /// Redoes the `$$ACTION_<name>$$` substitutions from the unsubstituted
    /// texts (section 1.6). `bound` gets the lowercased action name and
    /// returns the names of its primary and optional secondary key, shown as
    /// `<primary>` or `<primary> , <secondary>`. The key names belong to the
    /// key-binding specification; the spec does not say what an unbound
    /// action shows, so `None` leaves the sequence unsubstituted.
    pub fn apply_bindings(&mut self, bound: &Bindings) {
        for (key, template) in &self.templates {
            let text = substitute(template, bound);
            self.texts.insert(key.clone(), text.into_owned());
        }
    }
}

fn substitute<'a>(text: &'a str, bound: &Bindings) -> Cow<'a, str> {
    if !text.contains(ACTION) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(ACTION) {
        out.push_str(&rest[..start]);
        let after = &rest[start + ACTION.len()..];
        let Some(end) = after.find("$$") else {
            out.push_str(&rest[start..]);
            return Cow::Owned(out);
        };
        let name = &after[..end];
        match bound(&name.to_ascii_lowercase()) {
            Some((primary, Some(secondary))) => {
                out.push_str(&format!("{primary} , {secondary}"));
            }
            Some((primary, None)) => out.push_str(&primary),
            None => out.push_str(&rest[start..start + ACTION.len() + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cp1251, ltx};

    fn table(xml: &str) -> Vec<(String, String)> {
        entries(&xml::parse(&cp1251::encode(xml).unwrap()).unwrap()).unwrap()
    }

    #[test]
    fn reads_localization() {
        let config = ltx::parse(
            b"[string_table]\nlanguage\t= rus\nfont_prefix\t= ;_cent ;_west\n\
            files = ui_st_pda, ui_st_mm_mp,  stable_game_credits\n",
        )
        .unwrap();
        let l = Localization::read(&config).unwrap();
        assert_eq!(l.language, "rus");
        assert_eq!(l.files, ["ui_st_pda", "ui_st_mm_mp", "stable_game_credits"]);
        assert_eq!(l.font_prefix, None);
        assert_eq!(l.table_path("ui_st_mm"), "text\\rus\\ui_st_mm.xml");
        let west = ltx::parse(b"[string_table]\nlanguage = eng\nfont_prefix = _west\n").unwrap();
        assert_eq!(
            Localization::read(&west).unwrap().font_prefix.as_deref(),
            Some("_west")
        );
        assert!(Localization::read(&ltx::parse(b"[string_table]\n").unwrap()).is_err());
    }

    #[test]
    fn reads_entries() {
        let e = table(
            "<?xml version=\"1.0\" encoding=\"windows-1251\" ?>\n<string_table>\n\
            <string id=\"ui_mm_newgame\">\n\t\t<text>Новая игра</text>\n\t</string>\n\
            <!-- comment --><other id=\"x\"><text>y</text></other>\
            <string id=\"st_zaitsev\"><text>Евгений </text><text>no</text></string>\
            <string id=\"lines\"><text>\r\n a \t\r\n b\u{a0}c  </text></string>\
            </string_table>",
        );
        assert_eq!(
            e,
            [
                ("ui_mm_newgame".into(), "Новая игра".into()),
                ("st_zaitsev".into(), "Евгений".into()),
                ("lines".into(), "a b\u{a0}c".into()),
            ]
        );
        let doc = xml::parse(b"<t><string><text>x</text></string></t>").unwrap();
        assert!(entries(&doc).is_err());
        let doc = xml::parse(b"<t><string id=\"a\"/></t>").unwrap();
        assert!(entries(&doc).is_err());
    }

    #[test]
    fn resolves_keys() {
        let mut t = StringTable::default();
        t.extend([("a".into(), "1".into()), ("b".into(), "2".into())]);
        t.extend([("a".into(), "3".into())]);
        assert_eq!(t.get("a"), "3");
        assert_eq!(t.get("b"), "2");
        assert_eq!(t.get("B"), "B");
        assert_eq!(t.get("missing key"), "missing key");
        assert_eq!(t.get(""), "");
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn substitutes_bindings() {
        let mut t = StringTable::default();
        t.extend([
            (
                "crouch".into(),
                "Press $$ACTION_Crouch$$ or $$ACTION_jump$$.".into(),
            ),
            ("open".into(), "$$ACTION_none$$ $$ACTION_broken".into()),
        ]);
        let bound = |name: &str| match name {
            "crouch" => Some(("Ctrl".to_owned(), Some("C".to_owned()))),
            "jump" => Some(("Space".to_owned(), None)),
            _ => None,
        };
        t.apply_bindings(&bound);
        assert_eq!(t.get("crouch"), "Press Ctrl , C or Space.");
        assert_eq!(t.get("open"), "$$ACTION_none$$ $$ACTION_broken");
        t.apply_bindings(&|_| Some(("K".to_owned(), None)));
        assert_eq!(t.get("crouch"), "Press K or K.");
        let mut keys: Vec<&str> = t.templated_keys().collect();
        keys.sort();
        assert_eq!(keys, ["crouch", "open"]);
    }
}
