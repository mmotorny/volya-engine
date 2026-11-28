// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The areas of the tests that run by area (tests/areas.txt;
//! docs/architecture/build.md, Test triggers): each target's globs, and the
//! targets a diff's paths trigger.

use crate::Result;
use std::collections::HashMap;

/// The file, relative to the repository's root.
pub const FILE: &str = "tests/areas.txt";

/// The targets of tests/areas.txt, in its order, each with its globs (sets
/// expanded).
#[derive(Debug)]
pub struct Areas {
    pub targets: Vec<(String, Vec<String>)>,
}

impl Areas {
    /// Parses the format described at the top of tests/areas.txt.
    pub fn parse(text: &str) -> Result<Self> {
        let mut sets: HashMap<String, Vec<String>> = HashMap::new();
        let mut targets: Vec<(String, Vec<String>)> = Vec::new();
        // The entry being read: a set's name or a target's index.
        enum Current {
            None,
            Set(String),
            Target(usize),
        }
        let mut current = Current::None;
        for (number, line) in text.lines().enumerate() {
            let at = |message: String| format!("{FILE}:{}: {message}", number + 1);
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if !line.starts_with(char::is_whitespace) {
                if let Some(name) = trimmed.strip_prefix('@') {
                    if sets.contains_key(name) {
                        return Err(at(format!("set @{name} defined twice")));
                    }
                    sets.insert(name.to_owned(), Vec::new());
                    current = Current::Set(name.to_owned());
                } else if trimmed.starts_with("//") && trimmed.contains(':') {
                    if targets.iter().any(|(t, _)| t == trimmed) {
                        return Err(at(format!("{trimmed} listed twice")));
                    }
                    targets.push((trimmed.to_owned(), Vec::new()));
                    current = Current::Target(targets.len() - 1);
                } else {
                    return Err(at(format!(
                        "expected `//package:target` or `@set`, found `{trimmed}`"
                    )));
                }
                continue;
            }
            let globs = if let Some(name) = trimmed.strip_prefix('@') {
                sets.get(name)
                    .cloned()
                    .ok_or_else(|| at(format!("set @{name} used before it is defined")))?
            } else {
                check_glob(trimmed).map_err(at)?;
                vec![trimmed.to_owned()]
            };
            let entry = match &current {
                Current::None => return Err(at("a path before any entry".to_owned())),
                Current::Set(name) => sets.get_mut(name).expect("the set is defined"),
                Current::Target(index) => &mut targets[*index].1,
            };
            for glob in globs {
                if !entry.contains(&glob) {
                    entry.push(glob);
                }
            }
        }
        if let Some((target, _)) = targets.iter().find(|(_, globs)| globs.is_empty()) {
            return Err(format!("{FILE}: {target} has no paths"));
        }
        Ok(Self { targets })
    }

    /// The targets any of `paths` (relative to the repository's root)
    /// triggers, in the file's order.
    pub fn triggered<'a>(&'a self, paths: &[&str]) -> Vec<&'a str> {
        self.targets
            .iter()
            .filter(|(_, globs)| {
                paths
                    .iter()
                    .any(|path| globs.iter().any(|glob| glob_matches(glob, path)))
            })
            .map(|(target, _)| target.as_str())
            .collect()
    }
}

/// A glob's syntax: relative, `**` only as a whole segment, no braces.
fn check_glob(glob: &str) -> std::result::Result<(), String> {
    if glob.starts_with('/') || glob.ends_with('/') {
        return Err(format!("`{glob}`: a glob is relative, of files"));
    }
    if glob.contains(['{', '}', '?', '[', ']']) {
        return Err(format!("`{glob}`: only `*` and `**` are supported"));
    }
    for segment in glob.split('/') {
        if segment.is_empty() || (segment.contains("**") && segment != "**") {
            return Err(format!("`{glob}`: `**` stands for whole segments"));
        }
    }
    Ok(())
}

/// Whether `path` matches `glob`: `*` matches within one path segment,
/// a `**` segment any number of segments.
pub fn glob_matches(glob: &str, path: &str) -> bool {
    let glob: Vec<&str> = glob.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    segments_match(&glob, &path)
}

fn segments_match(glob: &[&str], path: &[&str]) -> bool {
    match glob.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
        Some((first, rest)) => match path.split_first() {
            Some((segment, path)) => segment_matches(first, segment) && segments_match(rest, path),
            None => false,
        },
    }
}

/// `*` matches any run of characters within one segment.
fn segment_matches(glob: &str, segment: &str) -> bool {
    let mut parts = glob.split('*');
    let first = parts.next().expect("split yields one part");
    let Some(mut rest) = segment.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        // No `*`: the whole segment.
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_stays_within_a_segment() {
        assert!(glob_matches(
            "tests/browser/smoke.*",
            "tests/browser/smoke.js"
        ));
        assert!(glob_matches(
            "tests/browser/smoke.*",
            "tests/browser/smoke."
        ));
        assert!(!glob_matches(
            "tests/browser/smoke.*",
            "tests/browser/smoke.d/x.js"
        ));
        assert!(glob_matches(
            "platforms/web/static/*",
            "platforms/web/static/main.js"
        ));
        assert!(!glob_matches(
            "platforms/web/static/*",
            "platforms/web/static/ux-audit/states.js"
        ));
        assert!(glob_matches(
            "crates/volya-engine/src/level*.rs",
            "crates/volya-engine/src/level_sound.rs"
        ));
        assert!(glob_matches(
            "crates/volya-engine/src/level*.rs",
            "crates/volya-engine/src/level.rs"
        ));
        assert!(!glob_matches(
            "crates/volya-engine/src/level*.rs",
            "crates/volya-engine/src/level/x.rs"
        ));
        assert!(glob_matches("a/*b*c*/d", "a/xbycz/d"));
        assert!(!glob_matches("a/*b*c/d", "a/xbcy/d"));
        assert!(!glob_matches("a/*ab", "a/b"));
        assert!(!glob_matches("Cargo.lock", "crates/x/Cargo.lock"));
    }

    #[test]
    fn a_double_star_spans_segments() {
        assert!(glob_matches(
            "tools/devserver/**",
            "tools/devserver/src/main.rs"
        ));
        assert!(glob_matches(
            "tools/devserver/**",
            "tools/devserver/BUILD.bazel"
        ));
        assert!(!glob_matches(
            "tools/devserver/**",
            "tools/devserver2/BUILD.bazel"
        ));
        assert!(glob_matches("**", "MODULE.bazel"));
        assert!(glob_matches("**", "a/b/c"));
        assert!(glob_matches("a/**/c.rs", "a/c.rs"));
        assert!(glob_matches("a/**/c.rs", "a/x/y/c.rs"));
        assert!(!glob_matches("a/**/c.rs", "a/x/y/d.rs"));
        assert!(glob_matches("**/BUILD.bazel", "x/y/BUILD.bazel"));
    }

    #[test]
    fn globs_outside_the_format_are_refused() {
        for glob in ["/a", "a/", "a//b", "a/{b,c}", "a/b**", "a?"] {
            assert!(check_glob(glob).is_err(), "{glob}");
        }
    }

    const SAMPLE: &str = "\
# A comment.
@base
    a/**

@more
    @base
    b/*.rs
    # An indented comment.

//x:one
    @more
    c.txt

//x:two
    @base
    a/**
";

    #[test]
    fn sets_expand_where_they_are_used() {
        let areas = Areas::parse(SAMPLE).unwrap();
        assert_eq!(
            areas.targets,
            [
                (
                    "//x:one".to_owned(),
                    vec!["a/**".into(), "b/*.rs".into(), "c.txt".into()]
                ),
                ("//x:two".to_owned(), vec!["a/**".into()]),
            ]
        );
    }

    #[test]
    fn malformed_files_are_refused() {
        for text in [
            "//x:a\n    @later\n@later\n    a\n",
            "    a\n",
            "//x:a\n    a\n//x:a\n    b\n",
            "@s\n    a\n@s\n    b\n",
            "//x:a\n",
            "x:a\n    a\n",
            "//x:a\n    a/{b,c}\n",
        ] {
            assert!(Areas::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn a_diff_triggers_the_targets_whose_paths_it_touches() {
        let areas = Areas::parse(SAMPLE).unwrap();
        assert_eq!(areas.triggered(&["a/z/y"]), ["//x:one", "//x:two"]);
        assert_eq!(areas.triggered(&["b/m.rs"]), ["//x:one"]);
        assert_eq!(areas.triggered(&["b/m/n.rs", "d"]), Vec::<&str>::new());
        assert_eq!(areas.triggered(&["d", "c.txt"]), ["//x:one"]);
        assert_eq!(areas.triggered(&[]), Vec::<&str>::new());
    }

    #[test]
    fn the_repositorys_file_parses() {
        let text = include_str!("../../../tests/areas.txt");
        let areas = Areas::parse(text).unwrap();
        assert_eq!(areas.triggered(&["docs/roadmap.md"]), ["//:checks"]);
        let render = areas.triggered(&["crates/volya-engine-render/src/gl.rs"]);
        assert!(
            render.contains(&"//tests/browser:render_chrome"),
            "{render:?}"
        );
        assert!(
            render.contains(&"//tests/browser:render_iphone"),
            "{render:?}"
        );
    }
}
