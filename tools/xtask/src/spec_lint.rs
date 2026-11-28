// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `xtask spec-lint <draft.md>…`: flags content a clean spec may not
//! contain (docs/clean-room.md) before a draft goes to review. It catches
//! mechanical slips only (decompiler output, code blocks, mangled symbols,
//! address-shaped values); the reviewer still judges everything else.

use std::fs;

use crate::Result;

/// Words a decompiler or disassembler prints and a functional spec never
/// needs, matched as whole words.
const DECOMPILER_WORDS: &[&str] = &[
    "undefined1",
    "undefined2",
    "undefined4",
    "undefined8",
    "param_1",
    "param_2",
    "param_3",
    "this_00",
    "in_EAX",
    "in_ECX",
    "in_EDX",
    "unaff_EBX",
    "unaff_ESI",
    "unaff_EDI",
    "extraout_EAX",
    "CONCAT11",
    "CONCAT22",
    "CONCAT31",
    "CONCAT44",
    "SUB41",
    "SUB42",
    "ZEXT14",
    "SEXT14",
    "__thiscall",
    "__fastcall",
    "__stdcall",
    "__cdecl",
    "vftable",
    "vtable",
];

/// Prefixes of names Ghidra and similar tools invent for code and data.
const DECOMPILER_PREFIXES: &[&str] = &[
    "FUN_", "DAT_", "LAB_", "PTR_", "local_", "uVar", "iVar", "pcVar",
];

/// x86 mnemonics that start a disassembly line.
const MNEMONICS: &[&str] = &[
    "mov", "push", "pop", "call", "jmp", "jnz", "jz", "lea", "cmp", "xor", "ret", "fld", "fstp",
];

/// Words that make a number right after them a position in a file.
const OFFSET_WORDS: &[&str] = &["offset", "offsets", "address", "addresses", "rva", "va"];

/// Words that make a number right after them a place in code or memory.
const MEMORY_WORDS: &[&str] = &["address", "addresses", "rva", "va"];

/// Words that make a hexadecimal number after them a hash or checksum
/// (matched as prefixes: `CRC-32`, `SHA-256`, `prefixes`).
const HASH_WORDS: &[&str] = &["crc", "md5", "sha", "hash", "checksum", "prefix"];

/// Markers that declare a line's address-shaped values a constant or a
/// position in a game data file (docs/clean-room.md).
const MARKERS: &[&str] = &[
    "<!-- spec-lint: constant -->",
    "<!-- spec-lint: data position -->",
];

/// Code fence languages that would hold code rather than data.
const CODE_FENCES: &[&str] = &["c", "cpp", "c++", "asm", "nasm", "x86", "lua"];

pub fn run(paths: &[&str]) -> Result {
    if paths.is_empty() {
        return Err("usage: xtask spec-lint <draft.md>…".into());
    }
    let mut findings = 0;
    for path in paths {
        let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        for (line, problem) in lint(&text) {
            println!("{path}:{line}: {problem}");
            findings += 1;
        }
    }
    if findings > 0 {
        return Err(format!(
            "{findings} finding(s); fix them or justify each to the reviewer"
        ));
    }
    println!("spec-lint: no findings");
    Ok(())
}

/// Returns (1-based line, problem) for every finding.
fn lint(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let names_binary = |j: usize| {
        lines
            .get(j)
            .is_some_and(|l| tokens(l).iter().any(|t| binary(t)))
    };
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        let n = i + 1;
        if let Some(lang) = line.strip_prefix("```") {
            let lang = lang.trim().to_ascii_lowercase();
            if CODE_FENCES.contains(&lang.as_str()) {
                out.push((
                    n,
                    format!("code block in `{lang}`: describe behaviour in prose or as data"),
                ));
            }
        }
        for word in words(line) {
            if DECOMPILER_WORDS.contains(&word) {
                out.push((n, format!("decompiler term `{word}`")));
            }
            if DECOMPILER_PREFIXES
                .iter()
                .any(|p| word.starts_with(p) && word.len() > p.len())
            {
                out.push((n, format!("decompiler-style name `{word}`")));
            }
            if mangled(word) {
                out.push((n, format!("mangled symbol `{word}`")));
            }
        }
        if let Some(first) = line.split_whitespace().next() {
            let disassembly = MNEMONICS.contains(&first.to_ascii_lowercase().as_str())
                && line
                    .split_whitespace()
                    .nth(1)
                    .is_some_and(|op| op.contains(',') || op.contains('['));
            if disassembly {
                out.push((n, "looks like a disassembly line".into()));
            }
        }
        if line.contains("::") && !line.contains("://") {
            out.push((
                n,
                "`::` scope: likely an internal class or function name".into(),
            ));
        }
        if !MARKERS.iter().any(|m| line.contains(m)) {
            // A binary named on the line or next to it in its paragraph.
            let named = (i.saturating_sub(1)..=i + 1)
                .any(|j| names_binary(j) && (j == i || !lines[j].trim().is_empty()));
            for problem in addresses(line, named) {
                out.push((n, problem));
            }
        }
    }
    out
}

/// What kind of number a token is.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Number {
    /// `0x` and hexadecimal digits; the field is the digit count.
    Prefixed(usize),
    /// Hexadecimal digits with a letter and a decimal digit, no prefix; the
    /// field is the digit count.
    Bare(usize),
    /// Any other number (decimal, or hexadecimal digits without a letter).
    Plain,
}

fn number(token: &str) -> Option<Number> {
    if let Some(digits) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        return (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_hexdigit()))
            .then_some(Number::Prefixed(digits.len()));
    }
    let digits = token.replace([',', '_'], "");
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let letters = digits.chars().any(|c| c.is_ascii_alphabetic());
    let decimals = digits.chars().any(|c| c.is_ascii_digit());
    match (letters, decimals) {
        (true, true) => Some(Number::Bare(digits.len())),
        (false, true) => Some(Number::Plain),
        _ => None,
    }
}

/// A shipped binary's file name (`xrGame.dll`, `bin\XR_3DA.exe`).
fn binary(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    t.len() > 4 && (t.ends_with(".dll") || t.ends_with(".exe"))
}

/// A line's tokens without the punctuation and Markdown around them.
fn tokens(line: &str) -> Vec<&str> {
    line.split(|c: char| {
        c.is_whitespace() || matches!(c, '`' | '(' | ')' | '[' | ']' | '*' | '|' | '"' | ';')
    })
    .map(|t| t.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_')))
    .filter(|t| !t.is_empty())
    .collect()
}

/// Address-shaped values in one line, for the reviewer to judge. `named`:
/// the line or a line next to it in its paragraph names a shipped binary.
fn addresses(line: &str, named: bool) -> Vec<String> {
    let tokens = tokens(line);
    let lower: Vec<String> = tokens.iter().map(|t| t.to_ascii_lowercase()).collect();
    let span = |i: usize, back: usize, ahead: usize| {
        (i.saturating_sub(back)..(i + ahead + 1).min(tokens.len())).filter(move |&j| j != i)
    };
    let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let Some(kind) = number(token) else { continue };
        let after = |words: &[&str]| span(i, 2, 0).any(|j| words.contains(&lower[j].as_str()));
        let beside_binary = |reach| span(i, reach, reach).any(|j| binary(tokens[j]));
        // Addresses in 32-bit binaries have at most 8 digits; longer values
        // are hashes, and hashes and checksums are often named before them.
        let hex = matches!(kind, Number::Prefixed(5..=8) | Number::Bare(5..=8));
        let hash = span(i, 4, 0).any(|j| HASH_WORDS.iter().any(|w| lower[j].starts_with(w)));
        let problem = if after(OFFSET_WORDS) && beside_binary(6) {
            "a position in a shipped binary"
        } else if matches!(kind, Number::Bare(5..=7)) && !hash && placed(token) {
            "a bare hexadecimal number of 5 to 7 digits"
        } else if hex && after(MEMORY_WORDS) {
            "a hexadecimal number after an address word"
        } else if hex && !hash && named && (beside_binary(3) || after(&["at", "offset"])) {
            "a hexadecimal number beside a shipped binary's name"
        } else {
            continue;
        };
        out.push(format!(
            "address-shaped `{token}` ({problem}): positions in shipped binaries are \
             rejected; mark a constant or a data-file position with `{}` or `{}`",
            MARKERS[0], MARKERS[1]
        ));
    }
    out
}

/// A bare hexadecimal number with a decimal digit after a letter, as
/// addresses have (`9a2b3c`), unlike a number with a word after it (`12abc`).
fn placed(token: &str) -> bool {
    token
        .chars()
        .skip_while(|c| !c.is_ascii_alphabetic())
        .any(|c| c.is_ascii_digit())
}

/// Words of identifier characters (and `?@$` for mangled names).
fn words(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '?' | '@' | '$')))
        .filter(|w| !w.is_empty())
}

/// A Microsoft-mangled C++ name (`?name@scope@@…`) or an Itanium one (`_ZN…`).
fn mangled(word: &str) -> bool {
    (word.starts_with('?') && word.contains("@@")) || (word.starts_with("_ZN") && word.len() > 4)
}

#[cfg(test)]
mod tests {
    use super::lint;

    #[test]
    fn clean_text_passes() {
        let text = "The header is 12 bytes at offset 0x37940.\n```text\nfield  size\n```\nA URL: https://example.org/a::b";
        assert!(lint(text).is_empty(), "{:?}", lint(text));
    }

    #[test]
    fn flags_decompiler_output_code_and_symbols() {
        let text = "```c\nuVar3 = FUN_10001234(param_1);\nmov eax, [ecx+4]\n?load@CScript@@QAEXXZ\nCFoo::Bar\n";
        let problems: Vec<String> = lint(text).into_iter().map(|(_, p)| p).collect();
        for needle in [
            "code block",
            "uVar3",
            "FUN_10001234",
            "param_1",
            "disassembly",
            "mangled",
            "`::`",
        ] {
            assert!(
                problems.iter().any(|p| p.contains(needle)),
                "missing {needle}: {problems:?}"
            );
        }
    }

    /// The address-shaped values `lint` reports, in order.
    fn flagged(text: &str) -> Vec<String> {
        lint(text)
            .into_iter()
            .filter_map(|(_, p)| {
                let rest = p.strip_prefix("address-shaped `")?;
                Some(rest[..rest.find('`')?].to_string())
            })
            .collect()
    }

    #[test]
    fn flags_positions_in_shipped_binaries() {
        for (text, value) in [
            ("The string at offset `9a2b3c` holds it.", "9a2b3c"),
            (
                "`xrLUA.dll` contains the format `%.14g` at `0x6a2b3`.",
                "0x6a2b3",
            ),
            ("`0x2a2b3`, `0x4a2b3`) in `xrLUA.dll`", "0x2a2b3"),
            ("It starts at offset 290816 in `xrGame.dll`.", "290816"),
            ("The handler's RVA is 0x1a2b3.", "0x1a2b3"),
            ("the table at address 0x10203040", "0x10203040"),
            ("Both (at `0x3a2b3`, `0x5a2b3`)\nin `xrLUA.dll`.", "0x3a2b3"),
        ] {
            assert_eq!(
                flagged(text).first().map(String::as_str),
                Some(value),
                "{text}"
            );
        }
    }

    #[test]
    fn passes_constants_hashes_colours_and_data_positions() {
        for text in [
            "Keys (−1, 0x00E9CE72), (0, 0x00FFC066); 6,665 bytes, CRC-32 0xBFA9BD87.",
            "`xrGame.dll` (6,246,400 bytes, `19938b4b9fa11e3b4b6462ce7e7a12ac`)",
            "`bin\\xrRender_R1.dll` (SHA-256 prefixes `c5d0008f`, `60f6b66b`)",
            "bytes, CRC-32 0x4E93C6F1); binaries `bin\\xrParticles.dll`",
            "Bit 31 (`0x80000000`) is the flag; flags 0x10000 and 0x20000.",
            "Start a fresh state at `0x0131A9D3`. The AI flags start at 0x1ff.",
            "The directory is at offset 0x37940 of `gamedata.db0`.",
            "`g_corpsenum`: `12abc` → 12; renderer c5, checks B11 in xrGame.dll",
            "| `dm_lightning-01.dm` | 42,360 | `353A41DD` |",
            "The string at offset `9a2b3c` <!-- spec-lint: data position -->",
            "Seed 9a2b3c <!-- spec-lint: constant -->",
            "The value at `0x6a2b3`.\n\nA paragraph about `xrLUA.dll`.",
        ] {
            assert!(flagged(text).is_empty(), "{text}: {:?}", flagged(text));
        }
    }
}
