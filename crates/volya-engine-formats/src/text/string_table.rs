// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! String tables: string keys → localized text
//! (docs/formats/text-and-fonts.md, section 1).

use std::borrow::Cow;
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
    /// `language` and `files` are read (sections 1.1, 1.2): a missing one
    /// is fatal (ltx-config.md 11). Empty items of `files` (a trailing comma)
    /// are skipped.
    pub fn read(config: &Config) -> io::Result<Self> {
        let get = |k| config.read("string_table", k);
        let language = get("language")?.to_owned();
        let files = get("files")?
            .split(',')
            .map(|n| xml::trim(n).to_owned())
            .filter(|n| !n.is_empty())
            .collect();
        // TODO(#330): section 2.4 says what a value does, not whether the
        // key is checked for first; a missing one is taken as empty.
        let font_prefix = config
            .get("string_table", "font_prefix")
            .flatten()
            .map(str::to_owned);
        Ok(Self {
            language,
            files,
            font_prefix,
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

/// A key's or a text's bytes in one of [`StringTable`]'s buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: u32,
    len: u32,
}

impl Span {
    fn of(buffer: &mut String, s: &str) -> Self {
        let start = buffer.len() as u32;
        buffer.push_str(s);
        Self {
            start,
            len: s.len() as u32,
        }
    }

    fn get(self, buffer: &str) -> &str {
        &buffer[self.start as usize..(self.start + self.len) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    key: Span,
    text: Span,
    /// The substituted text in `bound`, for an entry containing
    /// `$$ACTION_…$$` once bindings are applied.
    bound: Option<Span>,
}

/// The loaded string table, a region (docs/architecture/memory.md): every
/// key and text once in one buffer, the entries sorted by key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    text: String,
    entries: Vec<Entry>,
    /// The texts of entries containing `$$ACTION_…$$` with their bindings
    /// substituted, remade by [`Self::apply_bindings`].
    bound: String,
}

impl StringTable {
    /// The table of `entries` in load order; a key seen again replaces the
    /// earlier text (section 1.4).
    pub fn new(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut entries: Vec<(String, String)> = entries.into_iter().collect();
        // Stable: of equal keys the last loaded is kept.
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut kept: Vec<&(String, String)> = Vec::with_capacity(entries.len());
        for e in &entries {
            match kept.last_mut() {
                Some(last) if last.0 == e.0 => *last = e,
                _ => kept.push(e),
            }
        }
        let size = kept.iter().map(|(k, t)| k.len() + t.len()).sum();
        let mut text = String::with_capacity(size);
        let entries = (kept.iter())
            .map(|(key, t)| Entry {
                key: Span::of(&mut text, key),
                text: Span::of(&mut text, t),
                bound: None,
            })
            .collect();
        Self {
            text,
            entries,
            bound: String::new(),
        }
    }

    fn find(&self, key: &str) -> Option<&Entry> {
        let i = (self.entries)
            .binary_search_by(|e| e.key.get(&self.text).cmp(key))
            .ok()?;
        Some(&self.entries[i])
    }

    /// The text of `key` (section 1.4): exact match; a missing key returns
    /// the key itself; an empty key returns an empty string.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        if key.is_empty() {
            return "";
        }
        self.find(key).map_or(key, |e| match e.bound {
            Some(b) => b.get(&self.bound),
            None => e.text.get(&self.text),
        })
    }

    /// Whether `key` is in the table.
    pub fn contains(&self, key: &str) -> bool {
        self.find(key).is_some()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The keys of entries using `$$ACTION_<name>$$`.
    pub fn templated_keys(&self) -> impl Iterator<Item = &str> {
        (self.entries.iter())
            .filter(|e| e.text.get(&self.text).contains(ACTION))
            .map(|e| e.key.get(&self.text))
    }

    /// Redoes the `$$ACTION_<name>$$` substitutions from the unsubstituted
    /// texts (section 1.6). `bound` gets the lowercased action name and
    /// returns the names of its primary and optional secondary key, shown as
    /// `<primary>` or `<primary> , <secondary>`. The key names belong to the
    /// key-binding specification; the spec does not say what an unbound
    /// action shows, so `None` leaves the sequence unsubstituted.
    pub fn apply_bindings(&mut self, bound: &Bindings) {
        self.bound.clear();
        for entry in &mut self.entries {
            let template = entry.text.get(&self.text);
            if template.contains(ACTION) {
                let text = substitute(template, bound);
                entry.bound = Some(Span::of(&mut self.bound, &text));
            }
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
        let west = ltx::parse(b"[string_table]\nlanguage = eng\nfont_prefix = _west\nfiles = a\n")
            .unwrap();
        assert_eq!(
            Localization::read(&west).unwrap().font_prefix.as_deref(),
            Some("_west")
        );
        assert!(Localization::read(&ltx::parse(b"[string_table]\n").unwrap()).is_err());
        // ltx-config.md 11: `files` is read, so a missing one is fatal.
        let err = Localization::read(&ltx::parse(b"[string_table]\nlanguage = rus\n").unwrap())
            .unwrap_err();
        assert!(err.to_string().contains("files"), "{err}");
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
        let t = StringTable::new([
            ("a".into(), "1".into()),
            ("b".into(), "2".into()),
            ("a".into(), "3".into()),
        ]);
        assert_eq!(t.get("a"), "3");
        assert_eq!(t.get("b"), "2");
        assert_eq!(t.get("B"), "B");
        assert_eq!(t.get("missing key"), "missing key");
        assert_eq!(t.get(""), "");
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn substitutes_bindings() {
        let mut t = StringTable::new([
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
