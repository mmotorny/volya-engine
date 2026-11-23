# AGENTS.md

- Agent-first project: act autonomously, including commits and pushes. Block on a human only for expensive or hard-to-undo operations.
- Don't use harness memory systems; record durable knowledge here.
- Every instruction here must earn its tokens: cut prose that doesn't change agent behavior.
- AGENTS.md is not README.md: project descriptions go in README.md; only agent instructions go here.
- Every source file starts with SPDX headers: `SPDX-FileCopyrightText: 2026 Maksym Motorny` (current year for new files) and `SPDX-License-Identifier: BSD-3-Clause`, in the file's comment syntax.
- Avoid GitHub CI whenever possible (cost); all testing runs locally.
- Dependencies must be permissively licensed (no copyleft, including LGPL/MPL).
- Never use or derive from leaked X-Ray source or its forks (e.g. OpenXRay).
- Never commit game assets, including test fixtures; tests needing game data skip when it's absent.
