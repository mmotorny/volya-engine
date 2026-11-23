// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Localized text and fonts (docs/formats/text-and-fonts.md): string tables
//! ([`string_table`]), font definitions and glyph metrics ([`font`]) and text
//! layout to glyph quads ([`layout`]). Finding files is the caller's job
//! (`volya-engine-core::text`).

pub mod font;
pub mod layout;
pub mod string_table;
