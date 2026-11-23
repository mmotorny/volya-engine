// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The path configuration file `fsgame.ltx` (docs/formats/ltx-config.md,
//! section 10): alias definitions, one per line. Resolving the aliases to
//! directories is `volya-engine-core::config`'s job.

use std::io;

use super::{invalid, raw_lines, trim};
use crate::cp1251;

/// One `alias = recurse | flag | root [| subdirectory [| extension [| caption]]]`
/// line. Text is decoded from Windows-1251; alias, root and subdirectory are
/// lowercased (ASCII).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub alias: String,
    /// Whether loose files under the directory are indexed recursively.
    pub recurse: bool,
    /// The second flag; it does not affect path resolution.
    pub flag: bool,
    pub root: String,
    pub subdirectory: Option<String>,
    pub extension: Option<String>,
    pub caption: Option<String>,
}

/// Booleans of this file: exactly `true`, `on`, `yes` or `1`.
fn flag(field: &[u8]) -> bool {
    matches!(field, b"true" | b"on" | b"yes" | b"1")
}

/// The definitions of `text`, in file order. Duplicates are not checked here.
pub fn parse(text: &[u8]) -> io::Result<Vec<Definition>> {
    let mut definitions = Vec::new();
    for line in raw_lines(text) {
        let (number, line) = line?;
        if line.first() == Some(&b';') {
            continue;
        }
        let (alias, rest) = match line.iter().position(|&b| b == b'=') {
            Some(eq) => (&line[..eq], &line[eq + 1..]),
            None => (line, &[][..]),
        };
        let rest = match rest.iter().position(|&b| b == b'=') {
            Some(eq) => &rest[..eq],
            None => rest,
        };
        let fields: Vec<&[u8]> = rest.split(|&b| b == b'|').map(trim).collect();
        if fields.len() < 3 {
            return Err(invalid(format!(
                "fsgame line {number}: fewer than three fields"
            )));
        }
        let lower = |field: &[u8]| cp1251::decode(&field.to_ascii_lowercase());
        let optional = |i: usize, lowercase: bool| {
            fields.get(i).map(|f| {
                if lowercase {
                    lower(f)
                } else {
                    cp1251::decode(f)
                }
            })
        };
        definitions.push(Definition {
            alias: lower(trim(alias)),
            recurse: flag(fields[0]),
            flag: flag(fields[1]),
            root: lower(fields[2]),
            subdirectory: optional(3, true),
            extension: optional(4, false),
            caption: optional(5, false),
        });
    }
    Ok(definitions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_definitions() {
        let text = b"; comment line\r\n\
            $game_data$ =     false| true | $fs_root$      | GameData\\\r\n\
            $level$ =         false| false| $game_levels$\r\n\
            $x$ = on|1|root|sub|*.ltx|Caption = ignored|y\r\n";
        let defs = parse(text).unwrap();
        assert_eq!(defs.len(), 3);
        assert_eq!(
            defs[0],
            Definition {
                alias: "$game_data$".into(),
                recurse: false,
                flag: true,
                root: "$fs_root$".into(),
                subdirectory: Some("gamedata\\".into()),
                extension: None,
                caption: None,
            }
        );
        assert_eq!(defs[1].subdirectory, None);
        assert!(defs[2].recurse && defs[2].flag);
        assert_eq!(defs[2].extension.as_deref(), Some("*.ltx"));
        assert_eq!(defs[2].caption.as_deref(), Some("Caption"));
    }

    #[test]
    fn flags_are_case_sensitive() {
        let defs = parse(b"$a$ = TRUE | yes | r\n").unwrap();
        assert!(!defs[0].recurse);
        assert!(defs[0].flag);
    }

    #[test]
    fn needs_three_fields() {
        assert!(parse(b"$a$ = true | false\n").is_err());
        assert!(parse(b"no definition\n").is_err());
        // Only a line's first byte marks a comment.
        assert!(parse(b" ; not a comment\n").is_err());
    }
}
