// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! `xtask spec-lint <draft.md>…`: flags content a clean spec may not
//! contain (docs/clean-room.md) before a draft goes to review. It catches
//! mechanical slips only (decompiler output, code blocks, mangled symbols);
//! the reviewer still judges everything else.

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
    for (i, raw) in text.lines().enumerate() {
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
    }
    out
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
}
