// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Localized text and fonts loaded from the game tree
//! (docs/formats/text-and-fonts.md): the string table of
//! `config\localization.ltx` (section 1) and the UI fonts of
//! `config\fonts.ltx` with their metric files (section 2). Parsing and
//! layout are [`volya_engine_formats::text`].

use std::collections::HashMap;
use std::io;

use volya_engine_formats::text::font::{
    Font, FontDefinition, GlyphMetrics, UiFont, metric_path, with_prefix,
};
use volya_engine_formats::text::string_table;
pub use volya_engine_formats::text::string_table::{Localization, StringTable};
use volya_engine_formats::xml;

use crate::config::{self, Config, PathAliases};
use crate::vfs::{self, Vfs};

const GAME_CONFIG: &str = "$game_config$";
const GAME_TEXTURES: &str = "$game_textures$";

fn context(path: &str, err: io::Error) -> io::Error {
    io::Error::new(err.kind(), format!("{path}: {err}"))
}

/// Loads `[string_table]` of `config\localization.ltx` (section 1.1).
pub fn load_localization(vfs: &dyn Vfs, aliases: &PathAliases) -> io::Result<Localization> {
    let config = config::load_aliased(vfs, aliases, GAME_CONFIG, "localization.ltx")?;
    Localization::read(&config)
}

/// Loads the string table (sections 1.2–1.4): every listed table
/// `config\text\<language>\<name>.xml` in list order, includes (relative to
/// `config\`) expanded in place. A missing table or include is an error.
pub fn load_string_table(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    localization: &Localization,
) -> io::Result<StringTable> {
    let mut table = StringTable::default();
    for name in &localization.files {
        let path = aliases.path(GAME_CONFIG, &localization.table_path(name))?;
        let text = vfs::read(vfs, &path).map_err(|e| {
            context(
                &path,
                io::Error::new(e.kind(), format!("string table xml file not found: {e}")),
            )
        })?;
        let text = xml::preprocess(&text, &mut |include| {
            let include =
                aliases.path(GAME_CONFIG, &volya_engine_formats::cp1251::decode(include))?;
            vfs::read(vfs, &include).map_err(|e| context(&format!("include {include}"), e))
        })
        .map_err(|e| context(&path, e))?;
        let doc = xml::parse(&text).map_err(|e| context(&path, e))?;
        table.extend(string_table::entries(&doc).map_err(|e| context(&path, e))?);
    }
    Ok(table)
}

/// The UI fonts created for one output height.
#[derive(Clone, Debug, Default)]
pub struct Fonts {
    fonts: HashMap<UiFont, Font>,
}

impl Fonts {
    /// Creates all [`UiFont::ALL`] from `config\fonts.ltx` for output height
    /// `height` (sections 2.1–2.6): the resolution variant, the language
    /// prefix of `localization`, then the metric file. Recreate them when the
    /// resolution changes.
    pub fn load(
        vfs: &dyn Vfs,
        aliases: &PathAliases,
        localization: &Localization,
        height: u32,
    ) -> io::Result<Self> {
        let fonts_ltx = config::load_aliased(vfs, aliases, GAME_CONFIG, "fonts.ltx")?;
        let mut fonts = HashMap::new();
        for ui_font in UiFont::ALL {
            let font = load_font(vfs, aliases, &fonts_ltx, ui_font, localization, height)?;
            fonts.insert(ui_font, font);
        }
        Ok(Self { fonts })
    }

    /// The font `font`.
    pub fn get(&self, font: UiFont) -> &Font {
        &self.fonts[&font]
    }
}

/// Creates one font from `fonts_ltx` for output height `height`.
pub fn load_font(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    fonts_ltx: &Config,
    ui_font: UiFont,
    localization: &Localization,
    height: u32,
) -> io::Result<Font> {
    let section = fonts_ltx.section(ui_font.section()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("fonts.ltx: no section [{}]", ui_font.section()),
        )
    })?;
    let definition = FontDefinition::from_section(section)?;
    let texture = with_prefix(
        definition.texture_for_height(height),
        localization.font_prefix.as_deref(),
    );
    let path = aliases.path(GAME_TEXTURES, &metric_path(&texture))?;
    // The metric file must exist (section 2.5), unlike a missing root LTX
    // file, which loads as empty.
    vfs.open(&path).map_err(|e| context(&path, e))?;
    let metrics = GlyphMetrics::parse(&config::load(vfs, &path)?).map_err(|e| context(&path, e))?;
    Ok(Font::new(ui_font, &definition, texture, metrics, height))
}
