// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A record's string ([`Text`]), shared by the records that hold the same
//! text once [`Strings`] has seen them (docs/architecture/memory.md, Leaner
//! structures, #787).

use std::borrow::Borrow;
use std::collections::HashSet;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

/// An immutable string of a record: changing one replaces it. Clones share
/// it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Text(Arc<str>);

impl Text {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for Text {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Text {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for Text {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Self(s.into())
    }
}

impl From<&String> for Text {
    fn from(s: &String) -> Self {
        Self(s.as_str().into())
    }
}

impl From<Text> for String {
    fn from(t: Text) -> Self {
        t.0.as_ref().to_owned()
    }
}

impl PartialEq<str> for Text {
    fn eq(&self, other: &str) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<&str> for Text {
    fn eq(&self, other: &&str) -> bool {
        &*self.0 == *other
    }
}

impl PartialEq<String> for Text {
    fn eq(&self, other: &String) -> bool {
        *self.0 == **other
    }
}

impl PartialEq<Text> for str {
    fn eq(&self, other: &Text) -> bool {
        self == &*other.0
    }
}

impl PartialEq<Text> for &str {
    fn eq(&self, other: &Text) -> bool {
        *self == &*other.0
    }
}

impl PartialEq<Text> for String {
    fn eq(&self, other: &Text) -> bool {
        **self == *other.0
    }
}

/// The texts read so far: each distinct text once, which [`Self::text`]
/// gives every record holding it. Kept while records are read in bulk (a
/// New Game's, a load's); the texts outlive it in their records.
#[derive(Debug, Default)]
pub struct Strings(HashSet<Text>);

impl Strings {
    pub fn new() -> Self {
        Self::default()
    }

    /// The text with contents `text`: the one read before, or a new one
    /// kept for those that follow.
    pub fn text(&mut self, text: &str) -> Text {
        if let Some(seen) = self.0.get(text) {
            return seen.clone();
        }
        let t = Text::from(text);
        self.0.insert(t.clone());
        t
    }
}
