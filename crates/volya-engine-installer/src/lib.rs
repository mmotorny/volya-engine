// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Reads and extracts the game from GOG offline installers (Inno Setup with
//! GOG Galaxy parts); see docs/formats/gog-installer.md.
//!
//! The `.exe` holds the Inno Setup headers; parsing them (with `inno`) gives
//! every Inno file entry and where its bytes are. GOG stores each game file as
//! a chain of zlib *parts*, one per Inno entry, and records how to reassemble
//! them in each entry's install conditions:
//!
//! - `before_install('<file md5>', '<path>', <part count>)` on a file's first
//!   part (`before_install_dependency` for redistributables);
//! - `after_install('<part md5>', <compressed size>, <size>)` on every part;
//! - `check_if_install('<lang>#…', …)` with the languages.
//!
//! Game parts are stored (uncompressed, unencrypted) Inno chunks in the first
//! `.bin` slice, so extraction reads them directly from recorded offsets,
//! inflates them and verifies every MD5.

use std::io::{self, Read, Seek, SeekFrom, Write};

use inno::entry::FileLocationFlags;
use md5::{Digest, Md5};

/// Magic that starts every Inno chunk in a slice.
const CHUNK_MAGIC: &[u8; 4] = b"zlb\x1a";

/// A file the installer installs, assembled from GOG parts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameFile {
    /// Install path relative to the game directory, `/`-separated.
    pub path: String,
    /// Size in bytes once assembled.
    pub size: u64,
    pub md5: [u8; 16],
    /// Languages the file is installed for; empty means all.
    pub languages: Vec<String>,
    /// A redistributable (DirectX, ...) rather than part of the game.
    pub dependency: bool,
    pub parts: Vec<Part>,
}

/// One zlib part of a [`GameFile`] inside the `.bin` slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// Offset of the compressed bytes in the `.bin` file.
    pub offset: u64,
    pub compressed_size: u64,
    /// Size once inflated.
    pub size: u64,
    /// MD5 of the inflated bytes.
    pub md5: [u8; 16],
}

impl GameFile {
    /// Whether this file is installed for `language` (e.g. `ru-RU`).
    pub fn is_for(&self, language: &str) -> bool {
        self.languages.is_empty() || self.languages.iter().any(|l| l == language)
    }
}

/// The game files of a GOG installer, parsed from its `.exe`.
#[derive(Clone, Debug)]
pub struct Installer {
    pub files: Vec<GameFile>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

impl Installer {
    /// Parses the installer's `.exe`.
    pub fn parse(exe: impl Read + Seek) -> io::Result<Self> {
        let inno = inno::Inno::new(exe).map_err(|e| invalid(format!("Inno Setup headers: {e}")))?;
        if inno.setup_loader().data_offset() != 0 {
            return Err(invalid(
                "data embedded in the .exe; only .bin slices are supported",
            ));
        }
        let locations = inno.file_locations();
        let mut files = Vec::new();
        let mut current: Option<(GameFile, usize)> = None;

        for entry in inno.file_entries() {
            let condition = entry.condition();
            if let Some(before) = condition.before_install()
                && let Some(call) = Call::parse(before)
                && let ("before_install" | "before_install_dependency", [md5, path, count]) =
                    (call.name, call.args.as_slice())
            {
                if let Some((file, _)) = current.take() {
                    return Err(invalid(format!("{}: missing parts", file.path)));
                }
                let count = count.number().ok_or_else(|| invalid("part count"))? as usize;
                current = Some((
                    GameFile {
                        path: path
                            .string()
                            .ok_or_else(|| invalid("path"))?
                            .replace('\\', "/"),
                        size: 0,
                        md5: md5.md5().ok_or_else(|| invalid("file md5"))?,
                        languages: condition.check().map(languages).unwrap_or_default(),
                        dependency: call.name.ends_with("_dependency"),
                        parts: Vec::with_capacity(count),
                    },
                    count,
                ));
            }

            let Some(after) = condition.after_install().and_then(Call::parse) else {
                continue; // Not a GOG part (installer UI files and the like).
            };
            let ("after_install" | "after_install_dependency", [md5, compressed, size]) =
                (after.name, after.args.as_slice())
            else {
                continue;
            };
            let Some((file, count)) = current.as_mut() else {
                return Err(invalid("GOG part outside any file"));
            };
            let location = locations
                .get(entry.location() as usize)
                .ok_or_else(|| invalid("file location index out of range"))?;
            let flags = location.file_option_flags();
            if flags.intersects(
                FileLocationFlags::CHUNK_COMPRESSED | FileLocationFlags::CHUNK_ENCRYPTED,
            ) {
                return Err(invalid(format!(
                    "{}: compressed or encrypted chunk",
                    file.path
                )));
            }
            let chunk = location.chunk();
            if chunk.first_slice() != 0 || chunk.last_slice() != 0 {
                return Err(invalid(format!(
                    "{}: only one .bin slice is supported",
                    file.path
                )));
            }
            let part = Part {
                offset: chunk.start_offset() + CHUNK_MAGIC.len() as u64 + location.file().offset(),
                compressed_size: compressed
                    .number()
                    .ok_or_else(|| invalid("compressed size"))?,
                size: size.number().ok_or_else(|| invalid("size"))?,
                md5: md5.md5().ok_or_else(|| invalid("part md5"))?,
            };
            if location.file().size() != part.compressed_size {
                return Err(invalid(format!("{}: part size mismatch", file.path)));
            }
            file.size += part.size;
            file.parts.push(part);
            if file.parts.len() == *count {
                files.push(current.take().expect("current file").0);
            }
        }
        if let Some((file, _)) = current {
            return Err(invalid(format!("{}: missing parts", file.path)));
        }
        Ok(Self { files })
    }

    /// Game files (not redistributables) installed for `language`.
    pub fn game_files<'a>(&'a self, language: &'a str) -> impl Iterator<Item = &'a GameFile> {
        self.files
            .iter()
            .filter(move |f| !f.dependency && f.is_for(language))
    }

    /// The languages the game can be installed in: those whose tagged game
    /// files add up to at least half the bytes of the best-covered language,
    /// most bytes first (ties by tag). A language tagging only a few small
    /// files (the Russian installer tags three, 12 KB, with eight other
    /// languages) is not offered. Empty when no file is tagged.
    pub fn offered_languages(&self) -> Vec<String> {
        let mut bytes = std::collections::BTreeMap::<&str, u64>::new();
        for f in self.files.iter().filter(|f| !f.dependency) {
            for l in &f.languages {
                *bytes.entry(l).or_default() += f.size;
            }
        }
        let most = bytes.values().copied().max().unwrap_or(0);
        let mut offered: Vec<_> = bytes.into_iter().filter(|&(_, b)| b * 2 >= most).collect();
        offered.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        offered.into_iter().map(|(l, _)| l.to_owned()).collect()
    }

    /// The language to install, chosen from the data: the first of
    /// `preferred` (e.g. the device's languages) among the offered ones, by
    /// exact tag, then by primary subtag (`en` matches `en-US`); otherwise
    /// the best-covered language. `None` when no file is tagged (every game
    /// file is installed).
    pub fn choose_language(&self, preferred: &[String]) -> Option<String> {
        let offered = self.offered_languages();
        let primary = |tag: &str| tag.split('-').next().unwrap_or(tag).to_ascii_lowercase();
        let exact = preferred
            .iter()
            .find_map(|p| offered.iter().find(|o| o.eq_ignore_ascii_case(p)));
        let by_primary = || {
            preferred
                .iter()
                .find_map(|p| offered.iter().find(|o| primary(o) == primary(p)))
        };
        exact.or_else(by_primary).or(offered.first()).cloned()
    }
}

/// Writes `file`, read from the `.bin` slice, to `out`, verifying each part's
/// and the whole file's MD5.
pub fn extract(
    file: &GameFile,
    bin: &mut (impl Read + Seek),
    out: &mut impl Write,
) -> io::Result<()> {
    let mut compressed = Vec::new();
    let mut inflated = Vec::new();
    let mut whole = Md5::new();
    for (i, part) in file.parts.iter().enumerate() {
        let what = || format!("{} part {}/{}", file.path, i + 1, file.parts.len());
        let mut magic = [0u8; 4];
        bin.seek(SeekFrom::Start(part.offset - CHUNK_MAGIC.len() as u64))?;
        bin.read_exact(&mut magic)?;
        if &magic != CHUNK_MAGIC {
            return Err(invalid(format!("{}: no chunk at offset", what())));
        }
        compressed.resize(part.compressed_size as usize, 0);
        bin.read_exact(&mut compressed)?;
        inflated.resize(part.size as usize, 0);
        let (output, status) = zlib_rs::decompress_slice(
            &mut inflated,
            &compressed,
            zlib_rs::InflateConfig::default(),
        );
        if status != zlib_rs::ReturnCode::Ok || output.len() != inflated.len() {
            return Err(invalid(format!("{}: inflate failed ({status:?})", what())));
        }
        if <[u8; 16]>::from(Md5::digest(&inflated)) != part.md5 {
            return Err(invalid(format!("{}: MD5 mismatch", what())));
        }
        whole.update(&inflated);
        out.write_all(&inflated)?;
    }
    if <[u8; 16]>::from(whole.finalize()) != file.md5 {
        return Err(invalid(format!("{}: MD5 mismatch", file.path)));
    }
    Ok(())
}

/// Hex form of an MD5, as GOG writes it.
pub fn hex(md5: &[u8; 16]) -> String {
    md5.iter().map(|b| format!("{b:02x}")).collect()
}

/// Languages from `check_if_install('ru-RU#en-US#', ...)`.
fn languages(check: &str) -> Vec<String> {
    let Some(call) = Call::parse(check) else {
        return Vec::new();
    };
    match (call.name, call.args.first().and_then(Arg::string)) {
        ("check_if_install", Some(langs)) => langs
            .split('#')
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

/// A Pascal Script call such as `name('text', 42)`, as GOG writes them.
#[derive(Debug, PartialEq)]
struct Call<'a> {
    name: &'a str,
    args: Vec<Arg<'a>>,
}

#[derive(Debug, PartialEq)]
enum Arg<'a> {
    String(&'a str),
    Number(u64),
    Other(&'a str),
}

impl<'a> Arg<'a> {
    fn string(&self) -> Option<&'a str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    fn number(&self) -> Option<u64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    fn md5(&self) -> Option<[u8; 16]> {
        let s = self.string()?;
        if s.len() != 32 {
            return None;
        }
        let mut md5 = [0u8; 16];
        for (i, byte) in md5.iter_mut().enumerate() {
            *byte = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        Some(md5)
    }
}

impl<'a> Call<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let text = text.trim();
        let (name, rest) = text.split_once('(')?;
        let mut rest = rest.strip_suffix(')')?.trim();
        let mut args = Vec::new();
        while !rest.is_empty() {
            let (arg, tail) = if let Some(quoted) = rest.strip_prefix('\'') {
                let end = quoted.find('\'')?;
                (Arg::String(&quoted[..end]), &quoted[end + 1..])
            } else {
                let end = rest.find(',').unwrap_or(rest.len());
                let raw = rest[..end].trim();
                (
                    raw.parse().map_or(Arg::Other(raw), Arg::Number),
                    &rest[end..],
                )
            };
            args.push(arg);
            rest = tail.trim_start();
            rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
        }
        Some(Self {
            name: name.trim(),
            args,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gog_calls() {
        assert_eq!(
            Call::parse("before_install('695aae125fdd783ba0abe6192336a840', 'gamedata.db8', 28)"),
            Some(Call {
                name: "before_install",
                args: vec![
                    Arg::String("695aae125fdd783ba0abe6192336a840"),
                    Arg::String("gamedata.db8"),
                    Arg::Number(28)
                ]
            })
        );
        assert_eq!(
            Call::parse("after_install('6fe800ac55281824b251ed00d620eef7', 4685830, 10485760)")
                .unwrap()
                .args[2],
            Arg::Number(10_485_760)
        );
        assert_eq!(
            Call::parse("check_if_install_dependency('9dd9', False)")
                .unwrap()
                .args[1],
            Arg::Other("False")
        );
        assert_eq!(Call::parse("nonsense"), None);
    }

    fn installer(files: &[(&str, u64, &[&str])]) -> Installer {
        Installer {
            files: files
                .iter()
                .map(|(path, size, langs)| GameFile {
                    path: (*path).into(),
                    size: *size,
                    languages: langs.iter().map(|l| (*l).into()).collect(),
                    ..GameFile::default()
                })
                .collect(),
        }
    }

    #[test]
    fn chooses_language_from_data() {
        let prefs = |p: &[&str]| p.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        // Like the Russian installer: the game in one language, a few small
        // files tagged with others.
        let single = installer(&[
            ("a", 600, &["ru-RU"]),
            ("b", 400, &["ru-RU"]),
            ("c", 5, &["en-US", "ru-RU", "uk-UA"]),
        ]);
        assert_eq!(single.offered_languages(), ["ru-RU"]);
        assert_eq!(
            single.choose_language(&prefs(&["en-US"])),
            Some("ru-RU".into())
        );
        // Like the English installer: shared archives in several languages,
        // some files in one only.
        let multi = installer(&[
            ("a", 500, &["de-DE", "en-US", "uk-UA"]),
            ("b", 400, &["de-DE", "en-US", "uk-UA"]),
            ("c", 100, &["en-US"]),
        ]);
        assert_eq!(multi.offered_languages(), ["en-US", "de-DE", "uk-UA"]);
        assert_eq!(
            multi.choose_language(&prefs(&["uk-UA", "en-US"])),
            Some("uk-UA".into())
        );
        assert_eq!(multi.choose_language(&prefs(&["uk"])), Some("uk-UA".into()));
        assert_eq!(
            multi.choose_language(&prefs(&["fr-FR"])),
            Some("en-US".into())
        );
        assert_eq!(multi.choose_language(&[]), Some("en-US".into()));
        assert_eq!(
            installer(&[("a", 1, &[])]).choose_language(&prefs(&["en"])),
            None
        );
    }

    #[test]
    fn parses_languages_and_md5() {
        assert_eq!(
            languages("check_if_install('ru-RU#','32#64#','')"),
            ["ru-RU"]
        );
        assert_eq!(
            languages("check_if_install('it-IT#es-ES#','32#64#','')"),
            ["it-IT", "es-ES"]
        );
        assert!(languages("check_if_install_dependency('x', False)").is_empty());
        let md5 = Arg::String("9bb19579fb9e7fade0251070165540af")
            .md5()
            .unwrap();
        assert_eq!(hex(&md5), "9bb19579fb9e7fade0251070165540af");
        assert_eq!(Arg::String("xyz").md5(), None);
    }
}
