// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The site's licences (`third-party-licenses.txt`): Volya Engine's own
//! LICENSE, then the third-party notices from the manifest
//! `build/notices.bzl` writes of what the wasm links and the page serves
//! (docs/architecture/build.md, Third-party notices). Like
//! cargo-about, a crate's licence is its manifest's SPDX expression and its
//! notice the licence files in its root that the chosen licences' texts are
//! in; an alternative is chosen in deny.toml's allow-list order. Fails on
//! anything undeclared, a licence not on that list, or a chosen licence
//! without its text.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::Result;

/// The standard library's in-tree notices besides the project's own
/// (`third_party/rust`): the in-tree parts under licence files of their own
/// (compiler-builtins and its libm port, `core::arch` and `std_detect`,
/// `core::simd`, `std::backtrace`), within rust-src's `library/`.
const STD_IN_TREE_NOTICES: &[&str] = &[
    "compiler-builtins/LICENSE.txt",
    "compiler-builtins/libm/LICENSE.txt",
    "stdarch/LICENSE-MIT",
    "portable-simd/LICENSE-MIT",
    "backtrace/LICENSE-MIT",
];

/// Between one component's notice files.
const SEPARATOR: &str = "\n\n* * *\n\n";

/// The rule between components.
const RULE: &str = "----------------------------------------------------------------------";

/// The rule between Volya Engine's licence and the third-party notices.
const PART_RULE: &str = "======================================================================";

/// Volya Engine's own licence, `license` its LICENSE's text, as the file
/// begins: its heading, the licence it holds, and the text whole.
fn own_license(license: &str) -> std::result::Result<String, String> {
    let ids = classify(license);
    let [id] = ids.iter().collect::<Vec<_>>()[..] else {
        return Err(format!("LICENSE: not one licence's text ({ids:?})"));
    };
    Ok(format!(
        "Volya Engine, under {id}:\n\n{}\n",
        license.trim_end()
    ))
}

/// A licence expression (SPDX).
#[derive(Debug, PartialEq)]
enum Expr {
    /// A licence, with its exception (`Apache-2.0 WITH LLVM-exception`).
    License(String),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

fn tokens(text: &str) -> Vec<String> {
    let spaced = text.replace('(', " ( ").replace(')', " ) ");
    spaced
        .split_whitespace()
        // Cargo's legacy `MIT/Apache-2.0`.
        .flat_map(|t| {
            if t.contains('/') {
                let parts: Vec<&str> = t.split('/').collect();
                let mut out = Vec::new();
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        out.push("OR".to_owned());
                    }
                    out.push((*p).to_owned());
                }
                out
            } else {
                vec![t.to_owned()]
            }
        })
        .collect()
}

/// Parses an SPDX expression: `OR` binds loosest, then `AND`, then `WITH`.
fn parse(text: &str) -> std::result::Result<Expr, String> {
    let tokens = tokens(text);
    let mut at = 0;
    let expr = parse_or(&tokens, &mut at)?;
    if at != tokens.len() {
        return Err(format!(
            "licence expression {text:?}: unexpected {:?}",
            tokens[at]
        ));
    }
    Ok(expr)
}

fn parse_or(tokens: &[String], at: &mut usize) -> std::result::Result<Expr, String> {
    let mut terms = vec![parse_and(tokens, at)?];
    while tokens
        .get(*at)
        .is_some_and(|t| t.eq_ignore_ascii_case("OR"))
    {
        *at += 1;
        terms.push(parse_and(tokens, at)?);
    }
    Ok(if terms.len() == 1 {
        terms.remove(0)
    } else {
        Expr::Or(terms)
    })
}

fn parse_and(tokens: &[String], at: &mut usize) -> std::result::Result<Expr, String> {
    let mut terms = vec![parse_term(tokens, at)?];
    while tokens
        .get(*at)
        .is_some_and(|t| t.eq_ignore_ascii_case("AND"))
    {
        *at += 1;
        terms.push(parse_term(tokens, at)?);
    }
    Ok(if terms.len() == 1 {
        terms.remove(0)
    } else {
        Expr::And(terms)
    })
}

fn parse_term(tokens: &[String], at: &mut usize) -> std::result::Result<Expr, String> {
    let token = tokens.get(*at).ok_or("licence expression ends early")?;
    *at += 1;
    if token == "(" {
        let expr = parse_or(tokens, at)?;
        if tokens.get(*at).map(String::as_str) != Some(")") {
            return Err("licence expression: unclosed (".into());
        }
        *at += 1;
        return Ok(expr);
    }
    if tokens
        .get(*at)
        .is_some_and(|t| t.eq_ignore_ascii_case("WITH"))
    {
        let exception = tokens
            .get(*at + 1)
            .ok_or("licence expression ends after WITH")?;
        *at += 2;
        return Ok(Expr::License(format!("{token} WITH {exception}")));
    }
    Ok(Expr::License(token.clone()))
}

/// The licence whose text a licence (with an exception) needs.
fn base(license: &str) -> &str {
    license.split(" WITH ").next().unwrap_or(license)
}

/// The licences whose texts `text` holds, recognised by their wording.
fn classify(text: &str) -> BTreeSet<&'static str> {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let has = |s: &str| flat.contains(s);
    let mut ids = BTreeSet::new();
    if has("Permission is hereby granted, free of charge, to any person obtaining") {
        ids.insert("MIT");
    }
    if has("Apache License") && has("Version 2.0") {
        ids.insert("Apache-2.0");
    }
    if has("Redistribution and use in source and binary forms") {
        if has("Neither the name") || has("Neither the names") || has("endorse or promote") {
            ids.insert("BSD-3-Clause");
        } else {
            ids.insert("BSD-2-Clause");
        }
    }
    if has("Boost Software License") {
        ids.insert("BSL-1.0");
    }
    if has("Altered source versions must be plainly marked") {
        ids.insert("Zlib");
    }
    if has("UNICODE LICENSE V3") {
        ids.insert("Unicode-3.0");
    }
    if has(
        "Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee",
    ) {
        ids.insert("ISC");
    }
    if has("CC0 1.0 Universal") {
        ids.insert("CC0-1.0");
    }
    if has("This is free and unencumbered software released into the public domain") {
        ids.insert("Unlicense");
    }
    ids
}

/// A notice file: its text and the licences it holds.
struct Notice {
    text: String,
    ids: BTreeSet<&'static str>,
}

impl Notice {
    fn read(path: &str) -> Result<Notice> {
        let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let ids = classify(&text);
        Ok(Notice { text, ids })
    }
}

/// The licences chosen from `expr`: all of an `AND`, of an `OR` the
/// alternative first on the allow-list, each allowed and with its text in
/// one of `notices` (an exception's licence's text). `None` if none can be.
fn choose(expr: &Expr, allow: &[String], notices: &[Notice]) -> Option<Vec<String>> {
    match expr {
        Expr::License(license) => {
            let allowed = allow.iter().any(|a| a == license);
            let has_text = notices.iter().any(|n| n.ids.contains(base(license)));
            (allowed && has_text).then(|| vec![license.clone()])
        }
        Expr::And(terms) => {
            let mut all = Vec::new();
            for term in terms {
                all.extend(choose(term, allow, notices)?);
            }
            Some(all)
        }
        Expr::Or(terms) => terms
            .iter()
            .filter_map(|t| choose(t, allow, notices))
            .min_by_key(|chosen| {
                chosen
                    .iter()
                    .map(|l| allow.iter().position(|a| a == l).unwrap_or(usize::MAX))
                    .max()
            }),
    }
}

/// The allow-list of deny.toml's `[licenses]`, in its order.
fn allow_list(deny: &str) -> Result<Vec<String>> {
    let mut in_licenses = false;
    let mut in_allow = false;
    let mut allow = Vec::new();
    for line in deny.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && !in_allow {
            in_licenses = line == "[licenses]";
            continue;
        }
        if in_licenses && line.starts_with("allow") && line.contains('[') {
            in_allow = true;
        }
        if in_allow {
            for part in line.split('"').skip(1).step_by(2) {
                allow.push(part.to_owned());
            }
            if line.contains(']') {
                break;
            }
        }
    }
    if allow.is_empty() {
        return Err("deny.toml: no [licenses] allow-list".into());
    }
    Ok(allow)
}

/// A crate's `[package]` fields: name, version, license.
fn package(manifest: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            if let Some(value) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
                fields.insert(key.trim().to_owned(), value.to_owned());
            }
        }
    }
    fields
}

/// One entry of the notices: a component, its chosen licences, its text.
struct Entry {
    name: String,
    licenses: Vec<String>,
    text: String,
}

/// A crate's entry from its manifest, its root module (whose SPDX header's
/// copyright, as REUSE states it, precedes its notices) and licence files.
fn crate_entry(
    manifest: &str,
    root: Option<&str>,
    files: &[&str],
    allow: &[String],
) -> Result<Entry> {
    let text = fs::read_to_string(manifest).map_err(|e| format!("{manifest}: {e}"))?;
    let fields = package(&text);
    let name = fields.get("name").ok_or(format!("{manifest}: no name"))?;
    let version = fields
        .get("version")
        .ok_or(format!("{manifest}: no version"))?;
    let license = fields.get("license").ok_or(format!(
        "{name} {version}: no licence expression in its manifest"
    ))?;
    let candidates: Vec<&str> = files
        .iter()
        .copied()
        .filter(|f| {
            let base = Path::new(f)
                .file_name()
                .map(|n| n.to_string_lossy().to_uppercase())
                .unwrap_or_default();
            f.contains("/LICENSES/")
                || ["LICENSE", "LICENCE", "COPYING", "UNLICENSE", "NOTICE"]
                    .iter()
                    .any(|p| base.starts_with(p))
        })
        .collect();
    let mut notices = Vec::new();
    let mut names = Vec::new();
    for file in &candidates {
        notices.push(Notice::read(file)?);
        names.push(*file);
    }
    let mut entry = entry(
        &format!("{name} {version}"),
        license,
        notices,
        &names,
        allow,
    )?;
    if let Some(root) = root {
        let text = fs::read_to_string(root).map_err(|e| format!("{root}: {e}"))?;
        let holders: Vec<String> = text
            .lines()
            .take(10)
            .filter_map(|l| l.split_once("SPDX-FileCopyrightText:"))
            .map(|(_, holder)| holder.trim())
            .filter(|holder| !entry.text.contains(holder))
            .map(|holder| format!("Copyright {holder}"))
            .collect();
        if !holders.is_empty() {
            entry.text = format!("{}\n\n{}", holders.join("\n"), entry.text);
        }
    }
    Ok(entry)
}

/// An entry from a licence expression and candidate notices: the chosen
/// licences' texts, and any NOTICE file (Apache-2.0's).
fn entry(
    name: &str,
    license: &str,
    notices: Vec<Notice>,
    paths: &[&str],
    allow: &[String],
) -> Result<Entry> {
    let expr = parse(license).map_err(|e| format!("{name}: {e}"))?;
    let chosen = choose(&expr, allow, &notices).ok_or(format!(
        "{name}: no licence of {license:?} is on deny.toml's allow-list with its text among {paths:?}"
    ))?;
    let mut text = Vec::new();
    for (notice, path) in notices.iter().zip(paths) {
        let is_notice = Path::new(path)
            .file_name()
            .is_some_and(|n| n.to_string_lossy().to_uppercase().starts_with("NOTICE"));
        if is_notice || chosen.iter().any(|l| notice.ids.contains(base(l))) {
            text.push(notice.text.trim_end().to_owned());
        }
    }
    Ok(Entry {
        name: name.to_owned(),
        licenses: chosen,
        text: text.join(SEPARATOR),
    })
}

/// A file's leading comment (`/* ... */`).
fn header_comment(path: &str) -> Result<String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let start = text.find("/*").ok_or(format!("{path}: no comment"))?;
    let end = text[start..]
        .find("*/")
        .ok_or(format!("{path}: unclosed comment"))?;
    Ok(text[start..start + end + 2].to_owned())
}

fn split_paths(field: &str) -> Vec<&str> {
    field.split(';').filter(|p| !p.is_empty()).collect()
}

/// A declared component's entry: its notices' texts whole, its header
/// notices' leading comments, under the licences chosen from its expression.
/// None for a component without notices under licences that waive them for
/// compiled code (the LLVM exception).
fn component_entry(
    name: &str,
    license: &str,
    notices: &[&str],
    headers: &[&str],
    allow: &[String],
) -> Result<Option<Entry>> {
    if notices.is_empty() && headers.is_empty() {
        let expr = parse(license).map_err(|e| format!("{name}: {e}"))?;
        let mut ids = Vec::new();
        leaves(&expr, &mut ids);
        let waived = |id: &String| allow.contains(id) && id.ends_with(" WITH LLVM-exception");
        if matches!(expr, Expr::License(_) | Expr::And(_)) && ids.iter().all(waived) {
            return Ok(None);
        }
        return Err(format!("{name}: no notices for {license:?}"));
    }
    let mut texts = Vec::new();
    for path in notices {
        texts.push(fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?);
    }
    for path in headers {
        texts.push(header_comment(path)?);
    }
    let all: Vec<Notice> = texts
        .iter()
        .map(|t| Notice {
            ids: classify(t),
            text: t.clone(),
        })
        .collect();
    let expr = parse(license).map_err(|e| format!("{name}: {e}"))?;
    let chosen = choose(&expr, allow, &all).ok_or(format!(
        "{name}: no licence of {license:?} is on deny.toml's allow-list with its text among its notices"
    ))?;
    let text = texts
        .iter()
        .map(|t| t.trim_end().to_owned())
        .collect::<Vec<_>>()
        .join(SEPARATOR);
    Ok(Some(Entry {
        name: name.to_owned(),
        licenses: chosen,
        text,
    }))
}

/// The licence identifiers a served file's SPDX header names, if it has one.
fn spdx_licenses(path: &str) -> Option<String> {
    let text = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&text);
    text.lines()
        .take(5)
        .find_map(|l| l.split_once("SPDX-License-Identifier:"))
        .map(|(_, expr)| {
            expr.trim()
                .trim_end_matches("*/")
                .trim_end_matches("-->")
                .trim()
                .to_owned()
        })
}

/// Every licence an expression names.
fn leaves(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::License(l) => out.push(l.clone()),
        Expr::And(t) | Expr::Or(t) => t.iter().for_each(|e| leaves(e, out)),
    }
}

/// Writes the notices of the manifest at `manifest` to `out`.
pub fn notices(manifest: &Path, out: &Path) -> Result {
    let text = fs::read_to_string(manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let records: BTreeSet<Vec<&str>> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.split('\t').collect())
        .collect();
    let field =
        |kind: &str| -> Vec<&Vec<&str>> { records.iter().filter(|r| r[0] == kind).collect() };
    let mut problems: Vec<String> = field("problem").iter().map(|r| r[1].to_owned()).collect();

    let deny_path = field("deny").first().map(|r| r[1]).ok_or("no deny.toml")?;
    let deny = fs::read_to_string(deny_path).map_err(|e| format!("{deny_path}: {e}"))?;
    let allow = allow_list(&deny)?;
    let license_path = field("license").first().map(|r| r[1]).ok_or("no LICENSE")?;
    let license = fs::read_to_string(license_path).map_err(|e| format!("{license_path}: {e}"))?;
    let own = own_license(&license);
    if let Err(e) = &own {
        problems.push(e.clone());
    }

    let mut components = Vec::new();
    for r in field("component") {
        match component_entry(r[1], r[2], &split_paths(r[3]), &split_paths(r[4]), &allow) {
            Ok(e) => components.extend(e),
            Err(e) => problems.push(e),
        }
    }

    // The standard library: its in-tree notices, then its vendored crates.
    let mut crates = Vec::new();
    let library = field("rust_library")
        .first()
        .map(|r| r[1])
        .ok_or("no rust-src")?;
    let rust_src: BTreeSet<&str> = field("rust_src").iter().map(|r| r[1]).collect();
    let mut std_entry = None;
    if let Some(r) = field("std").first() {
        let mut notices = split_paths(r[3]);
        let in_tree: Vec<String> = STD_IN_TREE_NOTICES
            .iter()
            .map(|p| format!("{library}/{p}"))
            .collect();
        for path in &in_tree {
            if !rust_src.contains(path.as_str()) {
                problems.push(format!("rust-src lacks {path}"));
            }
        }
        notices.extend(in_tree.iter().map(String::as_str));
        match component_entry(r[1], r[2], &notices, &[], &allow) {
            Ok(e) => std_entry = e,
            Err(e) => problems.push(e),
        }
    }
    // Each of the target's rlibs is an in-tree crate (library/*/) or a
    // vendored one (library/vendor/*/), by its manifest's name.
    let mut in_tree = BTreeSet::new();
    let mut vendored: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for path in &rust_src {
        let Some(rel) = path.strip_prefix(&format!("{library}/")) else {
            continue;
        };
        if !rel.ends_with("/Cargo.toml") {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let Some(name) = package(&text).get("name").map(|n| n.replace('-', "_")) else {
            continue;
        };
        if rel.starts_with("vendor/") && rel.matches('/').count() == 2 {
            vendored.entry(name).or_default().push(path);
        } else if !rel.starts_with("vendor/") {
            in_tree.insert(name);
        }
    }
    for r in field("std_rlib") {
        let name = r[1];
        match vendored.get(name).map(Vec::as_slice) {
            Some([manifest]) => {
                let dir = manifest.trim_end_matches("Cargo.toml");
                let files: Vec<&str> = rust_src
                    .iter()
                    .copied()
                    .filter(|p| p.strip_prefix(dir).is_some_and(|f| !f.contains('/')))
                    .collect();
                match crate_entry(manifest, None, &files, &allow) {
                    Ok(e) => crates.push(e),
                    Err(e) => problems.push(format!("the standard library's {e}")),
                }
            }
            Some(_) => problems.push(format!("rust-src vendors several {name}")),
            None if in_tree.contains(name) => {}
            None => problems.push(format!("the standard library's {name}: not in rust-src")),
        }
    }

    for r in field("crate") {
        match crate_entry(r[1], Some(r[2]), &r[3..], &allow) {
            Ok(e) => crates.push(e),
            Err(e) => problems.push(e),
        }
    }

    // Served files are ours, under an allowed licence.
    for r in field("static") {
        let (path, repo) = (r[1], r[2]);
        if repo != "_main" || format!("/{path}").contains("/vendor/") {
            problems.push(format!("{path} is served but not ours (from {repo})"));
        }
        if path.ends_with("/third-party-licenses.txt") {
            problems.push(format!("{path}: the notices are generated, not static"));
        }
        if let Some(expr) = spdx_licenses(path) {
            let mut ids = Vec::new();
            match parse(&expr) {
                Ok(e) => leaves(&e, &mut ids),
                Err(e) => problems.push(format!("{path}: {e}")),
            }
            for id in ids {
                if !allow.contains(&id) {
                    problems.push(format!("{path}: {id} isn't on deny.toml's allow-list"));
                }
            }
        }
    }

    if !problems.is_empty() {
        problems.sort();
        problems.dedup();
        return Err(format!("third-party notices:\n  {}", problems.join("\n  ")));
    }

    components.sort_by(|a, b| a.name.cmp(&b.name));
    crates.sort_by_key(|a| a.name.to_lowercase());
    crates.dedup_by(|a, b| a.name == b.name);
    let own = own.unwrap_or_default();
    let mut out_text = format!(
        "{own}\n{PART_RULE}\n\nThird-party software in this app, under the licences below.\n"
    );
    let mut write = |names: &[&Entry]| {
        let mut licenses: Vec<&str> = names
            .iter()
            .flat_map(|e| e.licenses.iter().map(String::as_str))
            .collect();
        licenses.sort();
        licenses.dedup();
        let list: Vec<&str> = names.iter().map(|e| e.name.as_str()).collect();
        out_text.push_str(&format!(
            "\n{RULE}\n\n{}, under {}:\n\n{}\n",
            list.join(", "),
            licenses.join(" AND "),
            names[0].text
        ));
    };
    for entry in components.iter().chain(std_entry.iter()) {
        write(&[entry]);
    }
    // Crates whose notices are the same text share one.
    let mut groups: Vec<Vec<&Entry>> = Vec::new();
    for entry in &crates {
        match groups.iter_mut().find(|g| g[0].text == entry.text) {
            Some(group) => group.push(entry),
            None => groups.push(vec![entry]),
        }
    }
    for group in &groups {
        write(group);
    }
    fs::write(out, out_text).map_err(|e| format!("{}: {e}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lic(s: &str) -> Expr {
        Expr::License(s.into())
    }

    #[test]
    fn parses_expressions() {
        assert_eq!(parse("MIT").unwrap(), lic("MIT"));
        assert_eq!(
            parse("MIT OR Apache-2.0").unwrap(),
            Expr::Or(vec![lic("MIT"), lic("Apache-2.0")])
        );
        assert_eq!(
            parse("MIT/Apache-2.0").unwrap(),
            Expr::Or(vec![lic("MIT"), lic("Apache-2.0")])
        );
        assert_eq!(
            parse("(MIT OR Apache-2.0) AND Unicode-3.0").unwrap(),
            Expr::And(vec![
                Expr::Or(vec![lic("MIT"), lic("Apache-2.0")]),
                lic("Unicode-3.0")
            ])
        );
        assert_eq!(
            parse("Apache-2.0 WITH LLVM-exception OR MIT").unwrap(),
            Expr::Or(vec![lic("Apache-2.0 WITH LLVM-exception"), lic("MIT")])
        );
        assert!(parse("(MIT").is_err());
    }

    fn notice(ids: &[&'static str]) -> Notice {
        Notice {
            text: String::new(),
            ids: ids.iter().copied().collect(),
        }
    }

    #[test]
    fn chooses_the_first_allowed_alternative_with_its_text() {
        let allow: Vec<String> = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception"]
            .map(String::from)
            .to_vec();
        let both = [notice(&["MIT"]), notice(&["Apache-2.0"])];
        let expr = parse("Apache-2.0 OR MIT").unwrap();
        assert_eq!(choose(&expr, &allow, &both), Some(vec!["MIT".into()]));
        let apache = [notice(&["Apache-2.0"])];
        assert_eq!(
            choose(&expr, &allow, &apache),
            Some(vec!["Apache-2.0".into()])
        );
        let expr = parse("Apache-2.0 WITH LLVM-exception").unwrap();
        assert_eq!(
            choose(&expr, &allow, &apache),
            Some(vec!["Apache-2.0 WITH LLVM-exception".into()])
        );
        // Copyleft is never chosen, even with its text.
        let expr = parse("LGPL-2.1-or-later").unwrap();
        assert_eq!(choose(&expr, &allow, &[notice(&["MIT"])]), None);
        let expr = parse("MIT AND GPL-3.0").unwrap();
        assert_eq!(choose(&expr, &allow, &both), None);
    }

    #[test]
    fn classifies_licence_texts() {
        let mit = "Permission is hereby granted, free of charge, to any\nperson obtaining a copy";
        assert_eq!(classify(mit), ["MIT"].into());
        let bsd = "Redistribution and use in source and binary forms ... Neither the name of";
        assert_eq!(classify(bsd), ["BSD-3-Clause"].into());
        assert!(classify("GNU LESSER GENERAL PUBLIC LICENSE").is_empty());
    }

    #[test]
    fn heads_its_own_license() {
        let bsd = "Redistribution and use in source and binary forms ... Neither the name\n\n";
        assert_eq!(
            own_license(bsd).unwrap(),
            "Volya Engine, under BSD-3-Clause:\n\nRedistribution and use in source and binary forms ... Neither the name\n"
        );
        assert!(own_license("no licence").is_err());
    }

    #[test]
    fn reads_the_allow_list() {
        let deny = "[graph]\nallow = [\"x\"]\n[licenses]\n# c\nallow = [\n  \"MIT\", # m\n  \"Zlib\",\n]\n[bans]\n";
        assert_eq!(allow_list(deny).unwrap(), ["MIT", "Zlib"]);
    }

    #[test]
    fn reads_the_package() {
        let manifest = "[package]\nname = \"a-b\"\nversion = \"1.0.0\"\nlicense = \"MIT\"\n[lib]\nname = \"c\"\n";
        let fields = package(manifest);
        assert_eq!(fields["name"], "a-b");
        assert_eq!(fields["license"], "MIT");
    }
}
