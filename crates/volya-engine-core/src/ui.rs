// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! UI data loaded from the game tree (docs/formats/ui-layout.md): layout
//! files of `config\ui\` with the wide-screen `_16` rule and `#include`
//! lines (sections 1.1–1.2), colour definitions, texture descriptions
//! (section 6), texture kinds and frame sequences (sections 6.3–6.4).
//! Parsing is [`volya_engine_formats::ui`].

use std::io;

use volya_engine_formats::cp1251;
use volya_engine_formats::ui::layout::check_fonts;
use volya_engine_formats::ui::seq::Sequence;
use volya_engine_formats::ui::texture_desc::{self, TextureDescriptions};
use volya_engine_formats::ui::title_menu::TitleMenuLayout;
use volya_engine_formats::ui::{ColorDefs, Resolution, still_image_stem, ui_file_names};
use volya_engine_formats::xml::{self, Document};

use crate::config::{Config, PathAliases};
use crate::vfs::{self, Vfs};

const GAME_CONFIG: &str = "$game_config$";
const GAME_TEXTURES: &str = "$game_textures$";

fn context(path: &str, err: io::Error) -> io::Error {
    io::Error::new(err.kind(), format!("{path}: {err}"))
}

fn exists(vfs: &dyn Vfs, path: &str) -> bool {
    vfs.open(path).is_ok()
}

/// The path of the `ui` file `name` to load at `resolution`: the `_16`
/// variant when wide and present, else the requested name (section 1.1).
/// `None` when that file does not exist.
pub fn find_ui_file(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    name: &str,
    resolution: Resolution,
) -> io::Result<Option<String>> {
    for candidate in ui_file_names(name, resolution) {
        let path = aliases.path(GAME_CONFIG, &format!("ui\\{candidate}"))?;
        if exists(vfs, &path) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Reads the `ui` file `name` with its includes expanded (sections 1.1 and
/// 1.2), returning its path and text. An include path starting with `ui\`
/// (or `UI\`, the directory names the rule accepts) is looked up as a `ui`
/// file first; otherwise, or if that file is missing, it is opened as
/// written relative to `config\`.
pub fn read_ui_file(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    name: &str,
    resolution: Resolution,
) -> io::Result<(String, Vec<u8>)> {
    let path = find_ui_file(vfs, aliases, name, resolution)?.ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("UI file {name} not found"))
    })?;
    let text = vfs::read(vfs, &path).map_err(|e| context(&path, e))?;
    let text = xml::preprocess(&text, &mut |include| {
        let include = cp1251::decode(include);
        let ui = include
            .strip_prefix("ui\\")
            .or_else(|| include.strip_prefix("UI\\"));
        let found = match ui {
            Some(rest) => find_ui_file(vfs, aliases, rest, resolution)?,
            None => None,
        };
        let path = match found {
            Some(path) => path,
            None => aliases.path(GAME_CONFIG, &include)?,
        };
        vfs::read(vfs, &path).map_err(|e| context(&format!("include {path}"), e))
    })
    .map_err(|e| context(&path, e))?;
    Ok((path, text))
}

/// Loads and parses the `ui` file `name` (e.g. `ui_mm_main.xml`); an
/// unknown `font` name anywhere in it is the fatal "unknown font" error
/// (section 7.2).
pub fn load_ui_file(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    name: &str,
    resolution: Resolution,
) -> io::Result<Document> {
    let (path, text) = read_ui_file(vfs, aliases, name, resolution)?;
    let doc = xml::parse(&text).map_err(|e| context(&path, e))?;
    check_fonts(&doc).map_err(|e| context(&path, e))?;
    Ok(doc)
}

/// Loads `config\ui\color_defs.xml` (section 2).
pub fn load_color_defs(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    resolution: Resolution,
) -> io::Result<ColorDefs> {
    Ok(ColorDefs::read(&load_ui_file(
        vfs,
        aliases,
        "color_defs.xml",
        resolution,
    )?))
}

/// Loads the texture descriptions listed in `[texture_desc] files` of
/// `system` (`config\system.ltx`), in list order (section 6.1).
pub fn load_texture_descriptions(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    system: &Config,
    resolution: Resolution,
) -> io::Result<TextureDescriptions> {
    let mut descriptions = TextureDescriptions::default();
    for name in texture_desc::file_names(system) {
        let doc = load_ui_file(vfs, aliases, &format!("{name}.xml"), resolution)?;
        descriptions.add_file(&doc).map_err(|e| context(&name, e))?;
    }
    Ok(descriptions)
}

/// What a texture path names (section 6.3), with the file to read (a full
/// VFS path).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureSource {
    /// `textures\<path>.ogm`: Ogg Theora video, drawn with `hud\movie`.
    Video(String),
    /// `textures\<path>.avi`: video.
    Avi(String),
    /// `textures\<path>.seq`: a frame sequence of still images.
    Sequence(String, Sequence),
    /// A still image `<stem>.dds`.
    Image(String),
}

/// Resolves texture path `path` (relative to `textures\`, no extension):
/// `.ogm`, then `.avi`, then `.seq`, otherwise a still image (section 6.3).
/// Still images are searched in `textures\` only: the level and saved-games
/// directories the spec searches first matter only once a level exists.
/// A missing image is an error.
pub fn texture_source(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    path: &str,
) -> io::Result<TextureSource> {
    let file = |ext: &str| aliases.path(GAME_TEXTURES, &format!("{path}{ext}"));
    let ogm = file(".ogm")?;
    if exists(vfs, &ogm) {
        return Ok(TextureSource::Video(ogm));
    }
    let avi = file(".avi")?;
    if exists(vfs, &avi) {
        return Ok(TextureSource::Avi(avi));
    }
    let seq = file(".seq")?;
    if exists(vfs, &seq) {
        let sequence = Sequence::parse(&vfs::read(vfs, &seq)?).map_err(|e| context(&seq, e))?;
        return Ok(TextureSource::Sequence(seq, sequence));
    }
    still_image(vfs, aliases, path).map(TextureSource::Image)
}

/// The `.dds` file of still image `path` (section 6.3), which must exist.
pub fn still_image(vfs: &dyn Vfs, aliases: &PathAliases, path: &str) -> io::Result<String> {
    let dds = aliases.path(GAME_TEXTURES, &format!("{}.dds", still_image_stem(path)))?;
    if exists(vfs, &dds) {
        Ok(dds)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("texture {path}: {dds} not found"),
        ))
    }
}

/// Loads the title menu's layout `ui_mm_main.xml` with `colors`.
pub fn load_title_menu(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    colors: &ColorDefs,
    resolution: Resolution,
) -> io::Result<TitleMenuLayout> {
    let doc = load_ui_file(vfs, aliases, "ui_mm_main.xml", resolution)?;
    TitleMenuLayout::read(&doc, colors).map_err(|e| context("ui_mm_main.xml", e))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Cursor;

    use super::*;
    use crate::vfs::{DirEntry, VfsFile};

    struct Files(HashMap<&'static str, &'static [u8]>);

    struct MemFile(Cursor<&'static [u8]>);
    impl io::Read for MemFile {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }
    impl io::Seek for MemFile {
        fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
            self.0.seek(pos)
        }
    }
    impl VfsFile for MemFile {
        fn size(&self) -> u64 {
            self.0.get_ref().len() as u64
        }
    }
    impl Vfs for Files {
        fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
            let path = vfs::normalize(path)?;
            match self.0.get(path.as_str()) {
                Some(data) => Ok(Box::new(MemFile(Cursor::new(data)))),
                None => Err(io::Error::new(io::ErrorKind::NotFound, path)),
            }
        }
        fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
            unimplemented!()
        }
    }

    fn aliases() -> PathAliases {
        PathAliases::parse(
            b"$game_data$ = false| true | $fs_root$ | gamedata\\\r\n\
            $game_config$ = true | false| $game_data$ | config\\\r\n\
            $game_textures$ = true | true | $game_data$ | textures\\\r\n",
            "",
        )
        .unwrap()
    }

    fn tree() -> Files {
        Files(HashMap::from([
            ("gamedata/config/ui/a.xml", &b"<w>\r\n#include \"ui\\b.xml\"\r\n</w>"[..]),
            ("gamedata/config/ui/a_16.xml", b"<w16/>"),
            ("gamedata/config/ui/b.xml", b"<b/>\n#include \"text\\c.xml\""),
            ("gamedata/config/ui/b_16.xml", b"<b16/>"),
            ("gamedata/config/text/c.xml", b"<c/>"),
            ("gamedata/config/ui/inc.xml", b"<w>\n#include \"ui\\b.xml\"\n</w>"),
            ("gamedata/config/ui/bad.xml", b"<w>\n#include \"ui\\none.xml\"\n</w>"),
            (
                "gamedata/config/ui/tex.xml",
                b"<ui_texture><file_name>ui\\t</file_name><texture id=\"i\" width=\"2\"/></ui_texture>",
            ),
            (
                "gamedata/config/ui/font.xml",
                b"<w><s><text font=\"header\"/></s><e><text font=\"\"/></e></w>",
            ),
            ("gamedata/config/ui/fine.xml", b"<w><s><text font=\"di\"/></s></w>"),
            ("gamedata/textures/ui/v.ogm", b""),
            ("gamedata/textures/ui/v.seq", b""),
            ("gamedata/textures/ui/s.seq", b"2\r\nfx\\f.tga\r\n"),
            ("gamedata/textures/fx/f.dds", b""),
        ]))
    }

    const XGA: Resolution = Resolution::new(1024, 768);
    const HD: Resolution = Resolution::new(1920, 1080);

    #[test]
    fn applies_wide_screen_rule_to_files_and_includes() {
        let (vfs, aliases) = (tree(), aliases());
        let (path, text) = read_ui_file(&vfs, &aliases, "a.xml", XGA).unwrap();
        assert_eq!(path, "gamedata\\config\\ui\\a.xml");
        assert_eq!(text, b"<w>\n<b/>\n<c/>\n</w>\n");
        let (path, _) = read_ui_file(&vfs, &aliases, "a.xml", HD).unwrap();
        assert_eq!(path, "gamedata\\config\\ui\\a_16.xml");
        let doc = load_ui_file(&vfs, &aliases, "inc.xml", HD).unwrap();
        assert_eq!(doc.root().unwrap().elements().next().unwrap().name, "b16");
        // No `inc_16.xml`: the normal file at any ratio.
        assert!(find_ui_file(&vfs, &aliases, "inc", HD).unwrap().is_some());
        assert!(load_ui_file(&vfs, &aliases, "bad.xml", XGA).is_err());
        assert!(load_ui_file(&vfs, &aliases, "missing.xml", XGA).is_err());
        let err = load_ui_file(&vfs, &aliases, "font.xml", XGA).unwrap_err();
        assert!(err.to_string().contains("unknown font \"header\""), "{err}");
        assert!(load_ui_file(&vfs, &aliases, "fine.xml", XGA).is_ok());
    }

    #[test]
    fn loads_texture_descriptions_in_list_order() {
        let system = volya_engine_formats::ltx::parse(b"[texture_desc]\nfiles = tex\n").unwrap();
        let d = load_texture_descriptions(&tree(), &aliases(), &system, XGA).unwrap();
        assert_eq!(d.get("i").unwrap().file, "ui\\t");
        let none = volya_engine_formats::ltx::parse(b"[other]\n").unwrap();
        assert!(
            load_texture_descriptions(&tree(), &aliases(), &none, XGA)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn resolves_texture_kinds() {
        let (vfs, aliases) = (tree(), aliases());
        assert_eq!(
            texture_source(&vfs, &aliases, "ui\\v").unwrap(),
            TextureSource::Video("gamedata\\textures\\ui\\v.ogm".into())
        );
        let TextureSource::Sequence(path, seq) = texture_source(&vfs, &aliases, "ui\\s").unwrap()
        else {
            panic!("not a sequence");
        };
        assert_eq!(path, "gamedata\\textures\\ui\\s.seq");
        assert_eq!(
            still_image(&vfs, &aliases, &seq.frames[0]).unwrap(),
            "gamedata\\textures\\fx\\f.dds"
        );
        assert!(texture_source(&vfs, &aliases, "ui\\none").is_err());
    }
}
