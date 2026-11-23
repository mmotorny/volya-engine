// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Rules of docs/formats/ltx-config.md, sections 1–6, on synthetic inputs.

use std::collections::HashMap;

use super::*;

fn one_line(line: &[u8]) -> Option<Line> {
    let mut all: Vec<Line> = lines(line).map(|l| l.unwrap().line).collect();
    assert!(all.len() <= 1);
    all.pop()
}

fn entry(key: &str, value: Option<&str>) -> Option<Line> {
    Some(Line::Entry {
        key: key.as_bytes().to_vec(),
        value: value.map(|v| v.as_bytes().to_vec()),
    })
}

/// Loads `root` from in-memory `files`, resolving includes by name.
fn load(files: &[(&str, &[u8])], root: &str) -> io::Result<Config> {
    fn add(files: &HashMap<&str, &[u8]>, name: &str, builder: &mut Builder) -> io::Result<()> {
        let text = files
            .get(name)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, name.to_owned()))?;
        builder.add_file(text, &mut |path, b| {
            add(files, std::str::from_utf8(path).unwrap(), b)
        })
    }
    let files: HashMap<&str, &[u8]> = files.iter().copied().collect();
    let mut builder = Builder::new();
    add(&files, root, &mut builder)?;
    Ok(builder.finish())
}

#[test]
fn splits_lines_at_any_terminator() {
    let text = b"[a]\r\nx=1\n\n\ny=2\rz=3\r\n\r\n";
    let config = parse(text).unwrap();
    let a = config.section("a").unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(a.get("z"), Some(Some("3")));
    let numbers: Vec<usize> = lines(text).map(|l| l.unwrap().number).collect();
    assert_eq!(numbers, [1, 2, 5, 6]);
}

#[test]
fn nul_ends_the_line() {
    let config = parse(b"[a]\r\nx=1\0junk=2\r\ny=2\r\n\0").unwrap();
    let a = config.section("a").unwrap();
    assert_eq!(a.get("x"), Some(Some("1")));
    assert_eq!(a.get("junk"), None);
    assert_eq!(a.get("y"), Some(Some("2")));
}

#[test]
fn rejects_long_lines() {
    let mut text = b"[a]\nx=".to_vec();
    text.resize(4 + MAX_LINE, b'1');
    assert!(parse(&text).is_ok());
    text.push(b'1');
    assert!(parse(&text).is_err());
}

#[test]
fn removes_comments() {
    assert_eq!(
        one_line(b"max_belt\t\t= 5 //16"),
        entry("max_belt", Some("5"))
    );
    assert_eq!(
        one_line(b"slot_persistent_1 = true ;knife"),
        entry("slot_persistent_1", Some("true"))
    );
    // Only the first `/` of the line is examined.
    assert_eq!(one_line(b"a/b // c"), entry("a/b // c", None));
    assert_eq!(one_line(b"k = a/b // c"), entry("k", Some("a/b//c")));
    assert_eq!(one_line(b"k = //x"), entry("k", None));
    // Quotes do not protect comments.
    assert_eq!(one_line(b"k = \"a;b\""), entry("k", Some("\"a")));
    assert_eq!(one_line(b"; whole line"), None);
    assert_eq!(one_line(b"   // whole line"), None);
    assert_eq!(
        one_line(b"script_func = ;trader_job.artefacts_task_list_text"),
        entry("script_func", None)
    );
}

#[test]
fn normalizes_values() {
    for (line, key, value) in [
        (
            &b"$spawn\t\t\t= \"weapons\\ammo\\ammo_9x18_fmj\" ; option"[..],
            "$spawn",
            Some("\"weapons\\ammo\\ammo_9x18_fmj\""),
        ),
        (
            b"inv_name\t\t= \"9x18 PBP\"",
            "inv_name",
            Some("\"9x18 PBP\""),
        ),
        (b"$spawn\t\t\t= \"\" ; option", "$spawn", Some("\"\"")),
        (b"discovery_dependency = ", "discovery_dependency", None),
        (
            b"ph_box0_center \t\t\t\t\t\t= 0.0,  0.9,  0.0",
            "ph_box0_center",
            Some("0.0,0.9,0.0"),
        ),
        (
            b"on_info = {=hit_by_actor} %+agr_krot_skirmish_start%",
            "on_info",
            Some("{=hit_by_actor}%+agr_krot_skirmish_start%"),
        ),
        (
            b"caption = \"l04u_labx18_test",
            "caption",
            Some("\"l04u_labx18_test"),
        ),
        (
            b"caption = \"a b \" c d \" e f",
            "caption",
            Some("\"a b \"cd\" e f"),
        ),
        (
            b"physics\\Dead_Body\\skelet_crash",
            "physics\\Dead_Body\\skelet_crash",
            None,
        ),
        (
            b"ignore_ distance_sound = 1",
            "ignore_ distance_sound",
            Some("1"),
        ),
        (b"GroupControlSection = x", "GroupControlSection", Some("x")),
        (b"#notinclude = 1", "#notinclude", Some("1")),
        // Control bytes other than value whitespace are kept.
        (b"k = a\x01b\x0bc", "k", Some("a\x01bc")),
        // Cyrillic bytes are never whitespace.
        (
            b"k = \xcf\xf0\xe8 \xe2\xe5\xf2",
            "k",
            Some("\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"),
        ),
    ] {
        let expected = Some(Line::Entry {
            key: key.as_bytes().to_vec(),
            value: value.map(|v| cp1251::encode(v).unwrap()),
        });
        assert_eq!(
            one_line(line),
            expected,
            "{}",
            String::from_utf8_lossy(line)
        );
    }
    assert_eq!(one_line(b"= x"), None);
    assert_eq!(one_line(b"  =  "), None);
}

#[test]
fn reads_headers() {
    let header = |name: &str, parents: &[&str]| {
        Some(Line::Header {
            name: name.as_bytes().to_vec(),
            parents: parents.iter().map(|p| p.as_bytes().to_vec()).collect(),
        })
    };
    assert_eq!(
        one_line(b"[PH_Skeleton_Object]"),
        header("ph_skeleton_object", &[])
    );
    assert_eq!(one_line(b"[ a ]"), header(" a ", &[]));
    assert_eq!(
        one_line(b"[m_dog_step_params]\tfwdleftleg fwdrightleg"),
        header("m_dog_step_params", &[])
    );
    assert_eq!(
        one_line(b"[x]:a, B ,c ; comment"),
        header("x", &["a", "B", "c"])
    );
    assert_eq!(one_line(b"[x]:a,"), header("x", &["a"]));
    assert_eq!(one_line(b"[x]:a,,b"), header("x", &["a", "", "b"]));
    assert_eq!(one_line(b"[x]:a,,"), header("x", &["a", ""]));
    assert_eq!(one_line(b"[x]:"), header("x", &[]));
    assert_eq!(one_line(b"[x] :a"), header("x", &[]));
    assert!(lines(b"[x").next().unwrap().is_err());
}

#[test]
fn reads_include_paths() {
    let include = |p: &[u8]| Some(Line::Include(p.to_vec()));
    assert_eq!(
        one_line(b"#include \"creatures\\monsters.ltx\""),
        include(b"creatures\\monsters.ltx")
    );
    assert_eq!(
        one_line(b"#include \" a.ltx \" trailing"),
        include(b"a.ltx")
    );
    assert_eq!(one_line(b"#include \"a.ltx"), include(b"a.ltx"));
    assert_eq!(one_line(b"## see #include \"a.ltx\""), include(b"a.ltx"));
    assert!(lines(b"#include a.ltx").next().unwrap().is_err());
}

#[test]
fn replaces_repeated_keys() {
    let config = parse(
        b"[helicopter]\nlight_color = 0.9,0.9,0.5\nlight_color = 0.9,0.8,0.4\nKey=1\nkey=2\n",
    )
    .unwrap();
    let s = config.section("helicopter").unwrap();
    assert_eq!(s.len(), 3);
    assert_eq!(s.get("light_color"), Some(Some("0.9,0.8,0.4")));
    assert_eq!(s.get("Key"), Some(Some("1")));
    assert_eq!(s.get("key"), Some(Some("2")));
    assert_eq!(s.get("KEY"), None);
}

#[test]
fn ignores_text_before_the_first_header() {
    let config = parse(b"orphan = 1\n[a]\nx\n").unwrap();
    assert_eq!(config.len(), 1);
    assert_eq!(config.section("a").unwrap().get("x"), Some(None));
    assert!(parse(b"orphan\n").unwrap().is_empty());
    assert!(parse(b"").unwrap().is_empty());
}

#[test]
fn registers_empty_sections_and_rejects_duplicates() {
    let config = parse(b"[a]\n[b]\n").unwrap();
    assert_eq!(config.len(), 2);
    assert!(config.section("a").unwrap().is_empty());
    let err = parse(b"[a]\nx=1\n[A]\n").unwrap_err();
    assert!(err.to_string().contains("duplicate"), "{err}");
}

#[test]
fn inherits_by_copy_left_to_right() {
    let config =
        parse(b"[c]\nc=1\nshared=c\n[b]:c\nb=2\n[d]\nshared=d\nd=\n[A]:b, D\nown=3\nb=override\n")
            .unwrap();
    let a = config.section("a").unwrap();
    let keys: Vec<&str> = a.entries().map(Entry::key).collect();
    assert_eq!(keys, ["b", "c", "d", "own", "shared"]);
    assert_eq!(a.get("shared"), Some(Some("d")));
    assert_eq!(a.get("b"), Some(Some("override")));
    assert_eq!(a.get("d"), Some(None));
    assert_eq!(config.get("b", "b"), Some(Some("2")));
}

#[test]
fn parents_must_be_registered() {
    for text in [
        &b"[a]:missing\n"[..],
        b"[a]:a\n",
        b"[b]\n[a]:b,,\n",
        b"[a]:b\n[b]\n",
    ] {
        let err = parse(text).unwrap_err();
        assert!(err.to_string().contains("missing parent"), "{err}");
    }
    // A section named by the empty string can be a parent.
    let config = parse(b"[]\nx=1\n[b]\n[a]:b,,\n").unwrap();
    assert_eq!(config.get("a", "x"), Some(Some("1")));
    assert_eq!(
        parse(b"[]\nx=1\n[a]:,\n")
            .unwrap()
            .section("a")
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn looks_up_sections_case_insensitively() {
    let config = parse(b"[PH_Skeleton_Object]\nk=1\n").unwrap();
    assert_eq!(
        config.sections().next().unwrap().name(),
        "ph_skeleton_object"
    );
    assert_eq!(config.get("PH_Skeleton_Object", "k"), Some(Some("1")));
    assert!(config.contains("ph_skeleton_object"));
    // Section 6: the existence test compares the name as given.
    assert!(!config.contains("PH_Skeleton_Object"));
    assert_eq!(config.get("ph_skeleton_object", "K"), None);
}

#[test]
fn orders_by_bytes() {
    let config =
        parse(b"[zeta]\n[??tg_event]\n[_x]\n[\xc0]\n[a]\nbox_size\n$spawn\nGroupControlSection\n$prefetch\nbuck_shot\n").unwrap();
    let names: Vec<&str> = config.sections().map(Section::name).collect();
    assert_eq!(names, ["??tg_event", "_x", "a", "zeta", "\u{410}"]);
    let keys: Vec<&str> = config
        .section("a")
        .unwrap()
        .entries()
        .map(Entry::key)
        .collect();
    assert_eq!(
        keys,
        [
            "$prefetch",
            "$spawn",
            "GroupControlSection",
            "box_size",
            "buck_shot"
        ]
    );
    // Cyrillic names are not lowercased.
    assert!(config.contains("\u{410}"));
    assert!(!config.contains("\u{430}"));
}

#[test]
fn includes_are_scoped() {
    let files: &[(&str, &[u8])] = &[
        (
            "root",
            b"#include \"inc\"\n[open]\nbefore=1\n#include \"inc2\"\nafter=2\n[next]:open\n",
        ),
        ("inc", b"stray=1\n[base]\nk=base\n"),
        // `open` is not registered yet, so it cannot be a parent here.
        ("inc2", b"[from_inc2]:base\n"),
    ];
    let config = load(files, "root").unwrap();
    let names: Vec<&str> = config.sections().map(Section::name).collect();
    assert_eq!(names, ["base", "from_inc2", "next", "open"]);
    assert_eq!(config.section("open").unwrap().len(), 2);
    assert_eq!(config.section("next").unwrap().len(), 2);
    assert_eq!(config.get("from_inc2", "k"), Some(Some("base")));
    assert_eq!(config.get("base", "stray"), None);

    let open_parent: &[(&str, &[u8])] = &[
        ("root", b"[open]\n#include \"inc\"\n"),
        ("inc", b"[x]:open\n"),
    ];
    assert!(load(open_parent, "root").is_err());
    let twice: &[(&str, &[u8])] = &[
        ("root", b"#include \"inc\"\n#include \"inc\"\n"),
        ("inc", b"[x]\n"),
    ];
    assert!(
        load(twice, "root")
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    // An included file's open section is closed when that file ends, before
    // the including file continues.
    let closed: &[(&str, &[u8])] = &[
        ("root", b"#include \"inc\"\n[y]:x\n"),
        ("inc", b"[x]\nk=1\n"),
    ];
    assert_eq!(load(closed, "root").unwrap().get("y", "k"), Some(Some("1")));
    assert!(parse(b"#include \"x\"\n").is_err());
}

#[test]
fn decodes_windows_1251() {
    let config =
        parse(b"[a]\nname = \"\xd1\xf2\xe0\xeb\xea\xe5\xf0\" ; \xea\xee\xec\xec\xe5\xed\xf2\n")
            .unwrap();
    assert_eq!(config.get("a", "name"), Some(Some("\"Сталкер\"")));
}
