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

/// A string of [`Config::text`]: its start and length in bytes.
#[derive(Clone, Copy, Debug)]
struct Span {
    start: u32,
    len: u32,
}

impl Span {
    /// "No value".
    const NONE: Self = Self {
        start: u32::MAX,
        len: 0,
    };
}

#[derive(Clone, Copy, Debug)]
struct EntryRecord {
    key: Span,
    /// [`Span::NONE`] for "no value".
    value: Span,
}

#[derive(Clone, Copy, Debug)]
struct SectionRecord {
    name: Span,
    /// The section's entries: `len` of [`Config::slots`] from `first`.
    first: u32,
    len: u32,
}

/// An entry of a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    key: &'a str,
    value: Option<&'a str>,
}

impl<'a> Entry<'a> {
    /// The key, case preserved.
    pub fn key(self) -> &'a str {
        self.key
    }

    /// The normalized value (section 3), or `None` for "no value".
    pub fn value(self) -> Option<&'a str> {
        self.value
    }
}

/// Compares `stored` (decoded) with `query` (Windows-1251 bytes) by the
/// bytes `stored` was decoded from.
fn cmp_bytes(stored: &str, query: &[u8]) -> std::cmp::Ordering {
    if stored.is_ascii() {
        return stored.as_bytes().cmp(query);
    }
    let bytes = stored
        .chars()
        .map(|c| cp1251::encode_char(c).expect("decoded from bytes"));
    bytes.cmp(query.iter().copied())
}

/// A named set of entries, ordered by key bytes (section 6): a view of its
/// [`Config`].
#[derive(Clone, Copy)]
pub struct Section<'a> {
    config: &'a Config,
    record: SectionRecord,
}

impl<'a> Section<'a> {
    /// The lowercased name.
    pub fn name(self) -> &'a str {
        self.config.text(self.record.name)
    }

    /// The number of distinct keys.
    pub fn len(self) -> usize {
        self.record.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.record.len == 0
    }

    fn slots(self) -> &'a [u32] {
        let first = self.record.first as usize;
        &self.config.slots[first..first + self.record.len as usize]
    }

    /// The entries in key byte order.
    pub fn entries(self) -> impl ExactSizeIterator<Item = Entry<'a>> + Clone {
        let config = self.config;
        self.slots().iter().map(move |&i| config.entry(i))
    }

    /// The entry with exactly this key (keys are case-sensitive).
    pub fn entry(self, key: &str) -> Option<Entry<'a>> {
        let key = cp1251::encode(key)?;
        let slots = self.slots();
        let i = slots
            .binary_search_by(|&i| cmp_bytes(self.config.entry(i).key, &key))
            .ok()?;
        Some(self.config.entry(slots[i]))
    }

    pub fn contains(self, key: &str) -> bool {
        self.entry(key).is_some()
    }

    /// The value of `key`: `None` if the key is missing, `Some(None)` if it
    /// has no value.
    pub fn get(self, key: &str) -> Option<Option<&'a str>> {
        self.entry(key).map(Entry::value)
    }

    /// Reads `key` as the original's reader does: a missing key is fatal
    /// (section 11); a key without a value reads as empty.
    pub fn read(self, key: &str) -> io::Result<&'a str> {
        match self.get(key) {
            Some(value) => Ok(value.unwrap_or("")),
            None => Err(invalid(format!("[{}] has no key {key}", self.name()))),
        }
    }
}

impl std::fmt::Debug for Section<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Section")
            .field("name", &self.name())
            .field("entries", &self.entries().collect::<Vec<_>>())
            .finish()
    }
}

impl PartialEq for Section<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name() && self.entries().eq(other.entries())
    }
}

impl Eq for Section<'_> {}

/// A loaded configuration: sections ordered by name bytes (section 6).
///
/// A region (docs/architecture/memory.md): every name, key and value in one
/// text buffer, the sections and entries in tables of ranges into it. An
/// inherited entry is stored once, however many sections copy it (#357).
#[derive(Clone, Default)]
pub struct Config {
    /// Every section name, key and value, decoded.
    text: String,
    /// In name byte order.
    sections: Vec<SectionRecord>,
    /// Each section's entries, a run per section in key byte order: indices
    /// into `entries`.
    slots: Vec<u32>,
    entries: Vec<EntryRecord>,
}

impl Config {
    fn text(&self, span: Span) -> &str {
        &self.text[span.start as usize..(span.start + span.len) as usize]
    }

    fn entry(&self, index: u32) -> Entry<'_> {
        let record = self.entries[index as usize];
        Entry {
            key: self.text(record.key),
            value: (record.value.start != Span::NONE.start).then(|| self.text(record.value)),
        }
    }

    fn view(&self, record: SectionRecord) -> Section<'_> {
        Section {
            config: self,
            record,
        }
    }

    /// The section named by exactly these bytes.
    fn find(&self, name: &[u8]) -> Option<Section<'_>> {
        let i = self
            .sections
            .binary_search_by(|s| cmp_bytes(self.text(s.name), name))
            .ok()?;
        Some(self.view(self.sections[i]))
    }

    /// The number of sections.
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// The sections in name byte order.
    pub fn sections(&self) -> impl ExactSizeIterator<Item = Section<'_>> + Clone {
        self.sections.iter().map(|&s| self.view(s))
    }

    /// The section named `name`, compared case-insensitively (section 6).
    pub fn section(&self, name: &str) -> Option<Section<'_>> {
        self.find(&cp1251::encode(name)?.to_ascii_lowercase())
    }

    /// Whether a section named exactly `name` exists. Section 6: the
    /// existence test compares the name as given, without lowercasing, so a
    /// name with uppercase letters is never found (unlike [`Self::section`]).
    pub fn contains(&self, name: &str) -> bool {
        cp1251::encode(name).is_some_and(|name| self.find(&name).is_some())
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

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.sections()).finish()
    }
}

impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        self.sections().eq(other.sections())
    }
}

impl Eq for Config {}

/// Builds a [`Config`] from a root file and, through the caller, its includes
/// (sections 2–5).
#[derive(Debug, Default)]
pub struct Builder {
    config: Config,
    /// Each registered section's index in `config.sections` (in
    /// registration order until [`Builder::finish`]), by name bytes.
    names: BTreeMap<Vec<u8>, u32>,
}

/// A section being read: its name and its entries by key bytes.
struct Open {
    name: Span,
    entries: BTreeMap<Vec<u8>, u32>,
}

fn index(i: usize) -> u32 {
    u32::try_from(i).expect("the configuration fits u32 indices")
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends `bytes` decoded to the text.
    fn push_text(&mut self, bytes: &[u8]) -> Span {
        let text = &mut self.config.text;
        let start = index(text.len());
        if bytes.is_ascii() {
            text.push_str(std::str::from_utf8(bytes).expect("ASCII"));
        } else {
            text.extend(bytes.iter().map(|&b| cp1251::decode_byte(b)));
        }
        Span {
            start,
            len: index(text.len()) - start,
        }
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
        let mut open: Option<Open> = None;
        for line in lines(text) {
            let NumberedLine { number, line } = line?;
            match line {
                Line::Include(path) => include(&path, self)?,
                Line::Header { name, parents } => {
                    if let Some(section) = open.take() {
                        self.register(section)?;
                    }
                    let mut entries = BTreeMap::new();
                    for parent in &parents {
                        let Some(&found) = self.names.get(&parent.to_ascii_lowercase()) else {
                            return Err(invalid(format!(
                                "line {number}: section [{}]: missing parent [{}]",
                                cp1251::decode(&name),
                                cp1251::decode(parent)
                            )));
                        };
                        let found = self.config.view(self.config.sections[found as usize]);
                        for &slot in found.slots() {
                            let key = self.config.entries[slot as usize].key;
                            let key = cp1251::encode(self.config.text(key)).expect("decoded");
                            entries.insert(key, slot);
                        }
                    }
                    let name = self.push_text(&name);
                    open = Some(Open { name, entries });
                }
                Line::Entry { key, value } => {
                    if open.is_some() {
                        let record = EntryRecord {
                            key: self.push_text(&key),
                            value: value.as_deref().map_or(Span::NONE, |v| self.push_text(v)),
                        };
                        let slot = index(self.config.entries.len());
                        self.config.entries.push(record);
                        if let Some(section) = &mut open {
                            section.entries.insert(key, slot);
                        }
                    }
                }
            }
        }
        if let Some(section) = open {
            self.register(section)?;
        }
        Ok(())
    }

    fn register(&mut self, section: Open) -> io::Result<()> {
        let name = self.config.text(section.name);
        let key = cp1251::encode(name).expect("decoded from bytes");
        if self.names.contains_key(&key) {
            return Err(invalid(format!("duplicate section [{name}]")));
        }
        let first = index(self.config.slots.len());
        self.config.slots.extend(section.entries.into_values());
        let record = SectionRecord {
            name: section.name,
            first,
            len: index(self.config.slots.len()) - first,
        };
        self.names.insert(key, index(self.config.sections.len()));
        self.config.sections.push(record);
        Ok(())
    }

    pub fn finish(self) -> Config {
        let Self { mut config, names } = self;
        // The sections in name byte order, each table sized to what it holds.
        let sorted: Vec<SectionRecord> = names
            .into_values()
            .map(|i| config.sections[i as usize])
            .collect();
        config.sections = sorted;
        config.text.shrink_to_fit();
        config.slots.shrink_to_fit();
        config.entries.shrink_to_fit();
        config
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
