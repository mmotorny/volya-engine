// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's `.ltx` / `.ini` configuration files (docs/formats/ltx-config.md):
//! one file's text split into lines ([`lines`]), and a [`Config`] built from a
//! root file and its includes with [`Builder`]. Finding included files is the
//! caller's job (`volya-engine-core::config` does it over a VFS).
//!
//! Text is handled as bytes, as the spec does; names, keys and values are
//! decoded from Windows-1251 ([`crate::cp1251`], a bijection) only when stored,
//! and ordering uses the original bytes.

pub mod fsgame;
pub mod value;

use std::collections::BTreeMap;
use std::io;

use crate::cp1251;

/// Longest allowed line (section 1).
pub const MAX_LINE: usize = 4094;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Trim bytes: `0x00`–`0x20`.
fn is_trim(b: u8) -> bool {
    b <= 0x20
}

/// Value whitespace: tab, LF, VT, FF, CR, space.
fn is_value_space(b: u8) -> bool {
    matches!(b, 0x09..=0x0D | 0x20)
}

pub(crate) fn trim(mut s: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = s
        && is_trim(*first)
    {
        s = rest;
    }
    while let [rest @ .., last] = s
        && is_trim(*last)
    {
        s = rest;
    }
    s
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Physical lines of `text` with their 1-based line numbers: split at CR/LF
/// runs and cut at a NUL (section 1). Lines longer than [`MAX_LINE`] are an
/// error. Empty lines are dropped.
pub(crate) fn raw_lines(text: &[u8]) -> impl Iterator<Item = io::Result<(usize, &[u8])>> {
    let mut rest = text;
    let mut number = 1;
    std::iter::from_fn(move || {
        loop {
            // Skip a run of CR/LF, counting each LF and each CR not followed
            // by LF as one line end (for messages only).
            while let [b @ (b'\r' | b'\n'), tail @ ..] = rest {
                if *b == b'\n' || tail.first() != Some(&b'\n') {
                    number += 1;
                }
                rest = tail;
            }
            if rest.is_empty() {
                return None;
            }
            let end = rest
                .iter()
                .position(|&b| b == b'\r' || b == b'\n')
                .unwrap_or(rest.len());
            let (line, tail) = rest.split_at(end);
            rest = tail;
            // The spec does not say whether the length limit counts bytes after
            // a NUL; we measure the whole physical line (the stricter reading).
            if line.len() > MAX_LINE {
                return Some(Err(invalid(format!(
                    "line {number}: {} bytes, longer than {MAX_LINE}",
                    line.len()
                ))));
            }
            let line = match line.iter().position(|&b| b == 0) {
                Some(nul) => &line[..nul],
                None => line,
            };
            if !line.is_empty() {
                return Some(Ok((number, line)));
            }
        }
    })
}

/// A classified line of a configuration file (section 1, step 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// `#include "path"`: the path as written (trimmed, Windows-1251 bytes).
    Include(Vec<u8>),
    /// `[name]` or `[name]:parents`: the lowercased name and the trimmed
    /// parent names (not lowercased; lookups lowercase them).
    Header {
        name: Vec<u8>,
        parents: Vec<Vec<u8>>,
    },
    /// `key = value`: the trimmed key (never empty) and the normalized value
    /// (`None` when empty).
    Entry {
        key: Vec<u8>,
        value: Option<Vec<u8>>,
    },
}

/// A classified line with its 1-based line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberedLine {
    pub number: usize,
    pub line: Line,
}

/// The meaningful lines of one file's text, in order (sections 1–4).
pub fn lines(text: &[u8]) -> impl Iterator<Item = io::Result<NumberedLine>> + '_ {
    raw_lines(text).filter_map(|raw| {
        let (number, raw) = match raw {
            Ok(raw) => raw,
            Err(e) => return Some(Err(e)),
        };
        classify(raw)
            .map_err(|e| invalid(format!("line {number}: {e}")))
            .transpose()
            .map(|line| line.map(|line| NumberedLine { number, line }))
    })
}

fn classify(raw: &[u8]) -> Result<Option<Line>, String> {
    let text = strip_comment(trim(raw));
    match text.first() {
        None => Ok(None),
        Some(b'#') if find(text, b"#include").is_some() => {
            Ok(Some(Line::Include(include_path(text)?)))
        }
        Some(b'[') => header(text).map(Some),
        Some(_) => Ok(entry(text)),
    }
}

/// Removes a comment: from the first `;`, or from the first `/` of the line if
/// it is immediately followed by another `/` (section 1, step 2).
fn strip_comment(text: &[u8]) -> &[u8] {
    let semicolon = text.iter().position(|&b| b == b';');
    let slashes = text
        .iter()
        .position(|&b| b == b'/')
        .filter(|&i| text.get(i + 1) == Some(&b'/'));
    match semicolon.into_iter().chain(slashes).min() {
        Some(cut) => &text[..cut],
        None => text,
    }
}

/// The text between the first and second `"`, or to the end of the line,
/// trimmed (section 4).
fn include_path(text: &[u8]) -> Result<Vec<u8>, String> {
    // The spec does not cover an include line without any `"`; we reject it.
    let Some(open) = text.iter().position(|&b| b == b'"') else {
        return Err("include line without a quoted path".into());
    };
    let rest = &text[open + 1..];
    let close = rest.iter().position(|&b| b == b'"').unwrap_or(rest.len());
    Ok(trim(&rest[..close]).to_vec())
}

fn header(text: &[u8]) -> Result<Line, String> {
    let Some(close) = text.iter().position(|&b| b == b']') else {
        return Err("section header without `]`".into());
    };
    let name = text[1..close].to_ascii_lowercase();
    let mut parents = Vec::new();
    if let Some(colon) = find(text, b"]:") {
        // A single comma at the end of the list is ignored. The spec does not
        // say whether trim bytes may follow it; we trim the list first, so
        // `[x]:a, ` and `[x]:a,` both have the one parent `a`.
        let list = trim(&text[colon + 2..]);
        let list = list.strip_suffix(b",").unwrap_or(list);
        if !list.is_empty() {
            parents = list
                .split(|&b| b == b',')
                .map(|p| trim(p).to_vec())
                .collect();
        }
    }
    Ok(Line::Header { name, parents })
}

fn entry(text: &[u8]) -> Option<Line> {
    let (key, value) = match text.iter().position(|&b| b == b'=') {
        Some(eq) => (&text[..eq], normalize_value(&text[eq + 1..])),
        None => (text, Vec::new()),
    };
    let key = trim(key);
    if key.is_empty() {
        return None;
    }
    Some(Line::Entry {
        key: key.to_vec(),
        value: (!value.is_empty()).then_some(value),
    })
}

/// Removes value whitespace outside double quotes; quotes are kept
/// (section 3).
fn normalize_value(raw: &[u8]) -> Vec<u8> {
    let mut quoted = false;
    let mut value = Vec::with_capacity(raw.len());
    for &b in raw {
        if b == b'"' {
            quoted = !quoted;
        } else if !quoted && is_value_space(b) {
            continue;
        }
        value.push(b);
    }
    value
}

/// An entry of a section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    key: String,
    value: Option<String>,
}

impl Entry {
    /// The key, case preserved.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The normalized value (section 3), or `None` for "no value".
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

/// A named set of entries, ordered by key bytes (section 6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Section {
    name: String,
    entries: BTreeMap<Vec<u8>, Entry>,
}

impl Section {
    /// The lowercased name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The number of distinct keys.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entries in key byte order.
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &Entry> {
        self.entries.values()
    }

    /// The entry with exactly this key (keys are case-sensitive).
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.get(&cp1251::encode(key)?)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entry(key).is_some()
    }

    /// The value of `key`: `None` if the key is missing, `Some(None)` if it
    /// has no value.
    pub fn get(&self, key: &str) -> Option<Option<&str>> {
        self.entry(key).map(Entry::value)
    }

    /// Reads `key` as the original's reader does: a missing key is fatal
    /// (section 11); a key without a value reads as empty.
    pub fn read(&self, key: &str) -> io::Result<&str> {
        match self.get(key) {
            Some(value) => Ok(value.unwrap_or("")),
            None => Err(invalid(format!("[{}] has no key {key}", self.name))),
        }
    }

    fn insert(&mut self, key: &[u8], value: Option<&[u8]>) {
        let entry = Entry {
            key: cp1251::decode(key),
            value: value.map(cp1251::decode),
        };
        self.entries.insert(key.to_vec(), entry);
    }
}

/// A loaded configuration: sections ordered by name bytes (section 6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    sections: BTreeMap<Vec<u8>, Section>,
}

impl Config {
    /// The number of sections.
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// The sections in name byte order.
    pub fn sections(&self) -> impl ExactSizeIterator<Item = &Section> {
        self.sections.values()
    }

    /// The section named `name`, compared case-insensitively (section 6).
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections
            .get(&cp1251::encode(name)?.to_ascii_lowercase())
    }

    /// Whether a section named exactly `name` exists. Section 6: the
    /// existence test compares the name as given, without lowercasing, so a
    /// name with uppercase letters is never found (unlike [`Self::section`]).
    pub fn contains(&self, name: &str) -> bool {
        cp1251::encode(name).is_some_and(|name| self.sections.contains_key(&name))
    }

    /// The value of `key` in section `name`: `None` if either is missing,
    /// `Some(None)` if the entry has no value.
    pub fn get(&self, name: &str, key: &str) -> Option<Option<&str>> {
        self.section(name)?.get(key)
    }

    /// Reads `key` of section `name` as the original's reader does: a
    /// missing section or key is fatal (section 11); a key without a value
    /// reads as empty.
    pub fn read(&self, name: &str, key: &str) -> io::Result<&str> {
        self.section(name)
            .ok_or_else(|| invalid(format!("no section [{name}]")))?
            .read(key)
    }
}

/// Builds a [`Config`] from a root file and, through the caller, its includes
/// (sections 2–5).
#[derive(Debug, Default)]
pub struct Builder {
    config: Config,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes one file's `text` at the current point of the load order.
    /// Each include line calls `include` with its path (as written) and this
    /// builder; the caller finds the file and processes it with
    /// [`Builder::add_file`] in turn. The file's sections are registered when
    /// closed; a section still open at an include line stays open and is
    /// registered after the included file's sections.
    pub fn add_file(
        &mut self,
        text: &[u8],
        include: &mut dyn FnMut(&[u8], &mut Builder) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut open: Option<Section> = None;
        for line in lines(text) {
            let NumberedLine { number, line } = line?;
            match line {
                Line::Include(path) => include(&path, self)?,
                Line::Header { name, parents } => {
                    if let Some(section) = open.take() {
                        self.register(section)?;
                    }
                    let mut section = Section {
                        name: cp1251::decode(&name),
                        entries: BTreeMap::new(),
                    };
                    for parent in &parents {
                        let Some(found) = self.config.sections.get(&parent.to_ascii_lowercase())
                        else {
                            return Err(invalid(format!(
                                "line {number}: section [{}]: missing parent [{}]",
                                section.name,
                                cp1251::decode(parent)
                            )));
                        };
                        section
                            .entries
                            .extend(found.entries.iter().map(|(k, e)| (k.clone(), e.clone())));
                    }
                    open = Some(section);
                }
                Line::Entry { key, value } => {
                    if let Some(section) = &mut open {
                        section.insert(&key, value.as_deref());
                    }
                }
            }
        }
        if let Some(section) = open {
            self.register(section)?;
        }
        Ok(())
    }

    fn register(&mut self, section: Section) -> io::Result<()> {
        let key = cp1251::encode(&section.name).expect("decoded from bytes");
        if self.config.sections.contains_key(&key) {
            return Err(invalid(format!("duplicate section [{}]", section.name)));
        }
        self.config.sections.insert(key, section);
        Ok(())
    }

    pub fn finish(self) -> Config {
        self.config
    }
}

/// Parses a single file with no includes allowed.
pub fn parse(text: &[u8]) -> io::Result<Config> {
    let mut builder = Builder::new();
    builder.add_file(text, &mut |path, _| {
        Err(invalid(format!(
            "include {:?} needs a file tree",
            cp1251::decode(path)
        )))
    })?;
    Ok(builder.finish())
}

#[cfg(test)]
mod tests;
