# Clean room

How knowledge of the original game's formats and behavior enters Volya Engine. The goal is to stay defensively clean: no leaked or derived code, and nothing whose provenance we can't show.

## Roles

| Role | May read | Produces |
|---|---|---|
| **Investigator** (dirty) | Game data and the shipped x86 binaries the user owns; debugs and executes them | Notes and a draft spec in the dirty workspace |
| **Reviewer** (dirty) | The draft spec (and the dirty workspace if needed) | Verdict: approve (copies the spec into `docs/formats/`) or reject (the spec dies) |
| **Implementer** (clean) | Approved specs and the repository only | Code and tests |
| **Code reviewer** (clean) | Approved specs and the implementation | Verdict: every behavior in the code traces to a fact in the spec |

The orchestrating session is clean. Nothing dirty reaches it.

## Handoff

- **Dirty workspace:** `~/Sources/mmotorny/volya-dirty/`, outside the repository, never committed. Only dirty roles read or write it.
- **Investigator → reviewer:** files only, `specs/<format>.md` in the dirty workspace.
- **Reviewer → repository:** an approved spec is copied verbatim to `docs/formats/<format>.md` (formats) or `docs/behavior/<name>.md` (behavior); a rejected one never reaches the clean side in any form.
- **Scope comes from the dirty side:** an area starts with a scoping spec that splits it into small specs, one format or one behaviour each; each is then investigated in its full scope. Scope is never narrowed to pass review; anything an investigator can't establish is marked unestablished, not cut.
- **Runnable evidence:** every claim in a draft carries a check (a script in the dirty workspace that reproduces it from the data or binaries). The investigator runs all checks and `cargo xtask spec-lint <draft>` before reporting `SPEC READY`; the reviewer runs the same checks first, then looks for what they miss.
- **Review outcome:** only a clean-criteria violation blocks a spec. Otherwise the reviewer approves and copies it, having marked in the copy every wrong or unbacked claim inline (`Disputed in review: …`, with the correct value where established) and listed every gap under "Gaps found in review"; wording is fixed or noted, never blocking. These notes are clean text, held to the same criteria. Implementers treat a disputed statement as unestablished. The orchestrator reads the marks and decides: accept, send back for a targeted fix of named items, or split a gap into its own spec; anything that changes scope or looks critical goes to the user.
- **Rejection → revision:** a rejected draft (clean-criteria violations only) is revised in place by the investigator, from the reviewer's reasons in the dirty workspace; a fresh investigator takes over once the previous one's context is large. Every review round is done by a fresh reviewer that reviews the whole spec.
- **Dirty roles report to the orchestrator only a verdict, a path and counts** (`SPEC READY <path>`, `APPROVED <path>: <n> disputed, <n> gaps`, `REJECTED: <n> clean`, `BLOCKED`), never findings or reasons that carry content.
- **Implementers start fresh** with only the approved spec's path; they never see dirty prompts, reports or files.

## What a clean spec may contain

Allowed:

- Functional facts needed for interoperability: data layouts, field meanings, sizes, constants and magic values, keys, algorithms described in prose.
- Evidence from game data that anyone owning the game can reproduce (e.g. "all 14 archives of the Russian edition end with …").
- A provenance section: date, game edition and data examined, kinds of analysis performed.

Rejected:

- Disassembly listings, decompiled code, pseudo-code, or prose that transliterates code step by step.
- Binary addresses, internal symbol or function names, class or source file names.
- Structure mirroring the original program's code organization rather than the data.
- Anything taken from the leaked X-Ray source, OpenXRay or other derivatives, or documents of unclear provenance.

## Sources

| Source | Who |
|---|---|
| The user's own game data | everyone may use evidence from it; dirty roles may study it freely |
| The shipped binaries: static analysis, debugging, execution | investigator, reviewer |
| Public documentation of generic algorithms (e.g. standard compression schemes) | everyone |
| Leaked X-Ray source, OpenXRay and forks, anything derived from them; X-Ray documentation of unclear provenance | nobody |

Model memory is not a source. Agents may have absorbed X-Ray-derived code in training; every fact must be established from data or binaries, never from recollection.

## Tooling

- Static analysis: [Ghidra](https://ghidra-sre.org/) (Apache-2.0), headless mode for scripted analysis.
- Execution and debugging of the 32-bit Windows binaries: Wine or a Windows VM, set up when behavior questions need it.
