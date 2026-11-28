// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! tests/areas.txt matches the trigger table (docs/architecture/build.md,
//! Test triggers): every test the table runs by area has an entry, and
//! every entry is a test. `:manual_tests` (a genquery over `//:all_tests`)
//! lists the tests and suites, but the static checks, whose suite
//! `//:checks` this test runs in.

#[allow(dead_code)] // `triggered` is xtask's.
mod areas;

use std::collections::BTreeSet;
use std::fs;

type Result<T = ()> = std::result::Result<T, String>;

/// The static checks' suite: not in the query, which would depend on this
/// test.
const CHECKS: &str = "//:checks";

/// The browser targets the table's rows name, and their label suffixes.
const BROWSER_TARGETS: [(&str, &str); 2] = [("Chrome:", "chrome"), ("iPhone:", "iphone")];

fn read(variable: &str) -> String {
    let path = std::env::var(variable).unwrap_or_else(|_| panic!("{variable} is set"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// The tests the trigger table of `doc` runs by area. `tests` are the
/// repository's, for a row naming every suite of a target.
fn by_area(doc: &str, tests: &BTreeSet<String>) -> Result<BTreeSet<String>> {
    let section = doc
        .split("\n## ")
        .find(|s| s.starts_with("Test triggers\n"))
        .ok_or("no section `## Test triggers`")?;
    let rows: Vec<&str> = section
        .lines()
        .skip_while(|l| !l.starts_with('|'))
        .take_while(|l| l.starts_with('|'))
        .skip(2)
        .collect();
    let mut found = BTreeSet::new();
    for row in rows {
        let columns: Vec<&str> = row.split('|').map(str::trim).collect();
        let [_, tests_column, trigger, _] = columns[..] else {
            return Err(format!("not a row of two columns: {row}"));
        };
        if !trigger.starts_with("By area") {
            continue;
        }
        let before = found.len();
        let browser = BROWSER_TARGETS
            .iter()
            .find(|(prefix, _)| tests_column.starts_with(prefix));
        if let Some((prefix, suffix)) = browser {
            let names = tests_column[prefix.len()..].trim();
            if names.starts_with("every suite") {
                let ending = format!("_{suffix}");
                found.extend(
                    tests
                        .iter()
                        .filter(|t| {
                            t.strip_prefix("//tests/browser:")
                                .is_some_and(|n| n.ends_with(&ending))
                        })
                        .cloned(),
                );
            } else {
                for name in quoted(names) {
                    found.insert(format!(
                        "//tests/browser:{}_{suffix}",
                        name.replace('-', "_")
                    ));
                }
            }
        } else {
            // Labels, `:name` in the package of the label before it; other
            // quoted text (a tag) is prose.
            let mut package = None;
            for label in quoted(tests_column) {
                let label = if label.starts_with("//") {
                    let (name, _) = label.split_once(':').ok_or(format!("label {label}"))?;
                    package = Some(name);
                    label.to_owned()
                } else if let (Some(name), Some(package)) = (label.strip_prefix(':'), package) {
                    format!("{package}:{name}")
                } else {
                    continue;
                };
                found.insert(label);
            }
        }
        if found.len() == before {
            return Err(format!("no tests read from the row `{row}`"));
        }
    }
    Ok(found)
}

/// The text between backticks in `text`.
fn quoted(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2)
}

#[test]
fn the_areas_are_the_tables() {
    let areas = areas::Areas::parse(&read("AREAS")).unwrap_or_else(|e| panic!("{e}"));
    let tests: BTreeSet<String> = read("MANUAL_TESTS").lines().map(str::to_owned).collect();
    let expected = by_area(&read("TRIGGERS_DOC"), &tests).unwrap_or_else(|e| panic!("{e}"));
    let listed: BTreeSet<&str> = areas.targets.iter().map(|(t, _)| t.as_str()).collect();
    let missing: Vec<&String> = expected
        .iter()
        .filter(|t| !listed.contains(t.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "tests run by area (docs/architecture/build.md, Test triggers) without \
         an entry in {}: {missing:?}",
        areas::FILE
    );
    let unknown: Vec<&&str> = listed
        .iter()
        .filter(|t| **t != CHECKS && !tests.contains(**t))
        .collect();
    assert!(
        unknown.is_empty(),
        "entries in {} that are no test or test suite: {unknown:?}",
        areas::FILE
    );
}

#[test]
fn rows_name_tests_by_label_or_by_suite() {
    let tests: BTreeSet<String> = [
        "//tests/browser:smoke_iphone",
        "//tests/browser:render_iphone",
        "//tests/browser:smoke_chrome",
    ]
    .map(str::to_owned)
    .into();
    let doc = "\
# Build

## Test triggers

| Tests | Trigger |
|---|---|
| `//:gamedata` | Nightly |
| `//:checks`: every test tagged `check` | By area: every change (`**`) |
| Chrome: `smoke`, `import` | By area |
| iPhone: every suite | By area: some |
| The measurements: `//a/b:c`, `:d`, `//e:f` | By area |

## Next
";
    let found = by_area(doc, &tests).unwrap();
    let found: Vec<&str> = found.iter().map(String::as_str).collect();
    assert_eq!(
        found,
        [
            "//:checks",
            "//a/b:c",
            "//a/b:d",
            "//e:f",
            "//tests/browser:import_chrome",
            "//tests/browser:render_iphone",
            "//tests/browser:smoke_chrome",
            "//tests/browser:smoke_iphone",
        ]
    );
    let unread = "# B\n\n## Test triggers\n\n| T | T |\n|---|---|\n| x | By area |\n";
    assert!(by_area(unread, &tests).is_err());
}
