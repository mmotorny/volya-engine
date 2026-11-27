// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Sounds by name: finding a sound's file, reading its parameters once per
//! name, and decoding it on demand (docs/formats/sound-ogg-params.md
//! sections 2 – 5). How the parameters are heard is S2's
//! (docs/behavior/level-sound-scope.md); this exposes them only.
//!
//! Only the parameters are kept per sound: its file is read again and
//! decoded block by block each time it is opened ([`Sound::open`]), so no
//! PCM is held.

use std::collections::HashMap;
use std::io::{self, Cursor};
use std::rc::Rc;

use volya_engine_formats::audio::{VorbisStream, ogg_end_granule};
use volya_engine_formats::sound_params::{self, SoundParams};

use crate::config::{LEVEL, PathAliases};
use crate::vfs::{self, Vfs};

pub mod ambient;
pub mod music;
pub mod occlusion;
pub mod player_body;
pub mod rain_thunder;
pub mod random;
pub mod reverb;
pub mod static_sources;
pub mod system;

/// The alias of the sound directory (section 2).
pub const GAME_SOUNDS: &str = "$game_sounds$";

/// The only channel count and sample rate a sound file may have (section 1).
pub const CHANNELS: u16 = 1;
pub const SAMPLE_RATE: u32 = 44_100;

/// The smallest max distance and max AI distance (section 4).
pub const MIN_MAX_DISTANCE: f32 = 0.1;

/// A fatal error (section 5): the program shows it and stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fatal(pub String);

impl std::fmt::Display for Fatal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Fatal {}

impl From<Fatal> for io::Error {
    fn from(fatal: Fatal) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, fatal)
    }
}

/// `name` with the part from its last `.` onwards removed, if it has one.
fn without_suffix(name: &str) -> &str {
    name.rfind('.').map_or(name, |dot| &name[..dot])
}

/// The name a request for `name` is searched by among loaded sounds: lowered
/// and two `.`-suffixes removed (Gaps found in review).
fn search_name(name: &str) -> String {
    let lower = name.to_lowercase();
    without_suffix(without_suffix(&lower)).to_owned()
}

/// The name a sound requested as `name` is loaded and kept under: lowered and
/// three `.`-suffixes removed (section 2, rule 1).
pub fn resolved_name(name: &str) -> String {
    without_suffix(&search_name(name)).to_owned()
}

/// The path of the file of the sound requested as `name` (section 2): under
/// the loaded level's directory if it exists there, else under the sound
/// directory. Fails when the aliases lack either directory.
pub fn file_path(tree: &dyn Vfs, aliases: &PathAliases, name: &str) -> io::Result<String> {
    let file = format!("{}.ogg", resolved_name(name));
    let level = aliases.path(LEVEL, &file)?;
    if tree.exists(&level) {
        return Ok(level);
    }
    aliases.path(GAME_SOUNDS, &file)
}

/// A loaded sound: its file and parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// The resolved name (section 2, rule 1).
    pub name: String,
    /// The file's path in the game tree.
    pub path: String,
    pub params: SoundParams,
    /// The number of samples *N* of the decoded data: the granule position
    /// of the last Ogg page (docs/behavior/sound-positional.md section 2).
    pub samples: u64,
}

impl Sound {
    /// The duration *D* in milliseconds: floor(*N* · 1000 / 44,100)
    /// (docs/behavior/sound-positional.md section 2).
    pub fn duration_ms(&self) -> u64 {
        self.samples * 1000 / u64::from(SAMPLE_RATE)
    }

    /// Reads the file at `path` for the sound `name`: checks the stream and
    /// reads its parameters (sections 3 – 5). A missing or unknown record
    /// gives the defaults and a warning.
    pub fn load(tree: &dyn Vfs, name: &str, path: &str) -> Result<(Self, Option<String>), Fatal> {
        let bytes = read(tree, path)?;
        check_stream(path, &bytes)?;
        let comment = sound_params::first_user_comment(Cursor::new(&bytes))
            .map_err(|e| Fatal(format!("{path}: not a valid Ogg Vorbis stream ({e})")))?;
        let (params, problem) = sound_params::params(comment.as_deref());
        let warning = problem.map(|p| format!("{path}: {p}; using defaults"));
        if params.max_distance < MIN_MAX_DISTANCE {
            return Err(Fatal(format!(
                "{path}: max distance {} below {MIN_MAX_DISTANCE}",
                params.max_distance
            )));
        }
        if params.max_ai_distance < MIN_MAX_DISTANCE {
            return Err(Fatal(format!(
                "{path}: max AI distance {} below {MIN_MAX_DISTANCE}",
                params.max_ai_distance
            )));
        }
        let sound = Sound {
            name: name.to_owned(),
            path: path.to_owned(),
            params,
            samples: ogg_end_granule(&bytes).unwrap_or(0),
        };
        Ok((sound, warning))
    }

    /// Opens the sound for decoding: mono `f32` PCM at [`SAMPLE_RATE`], block
    /// by block. The file is read again; the stream was checked on load, so
    /// errors here mean the file changed or cannot be read.
    pub fn open(&self, tree: &dyn Vfs) -> io::Result<VorbisStream<Cursor<Vec<u8>>>> {
        VorbisStream::open(Cursor::new(self.file(tree)?))
    }

    /// The sound's file, read again (compressed), for a mixer to decode
    /// ([`crate::audio::MixerControl::play_file`]).
    pub fn file(&self, tree: &dyn Vfs) -> io::Result<Vec<u8>> {
        vfs::read(tree, &self.path)
    }
}

/// The file at `path`; missing, unreadable or empty is fatal (section 2,
/// rule 4).
fn read(tree: &dyn Vfs, path: &str) -> Result<Vec<u8>, Fatal> {
    match vfs::read(tree, path) {
        Ok(bytes) if !bytes.is_empty() => Ok(bytes),
        Ok(_) => Err(Fatal(format!("{path}: sound file is empty"))),
        Err(e) => Err(Fatal(format!("{path}: cannot open sound file ({e})"))),
    }
}

/// Fatal unless `bytes` is an Ogg Vorbis stream of [`CHANNELS`] channel at
/// [`SAMPLE_RATE`] (sections 1, 5 and Gaps found in review).
fn check_stream(path: &str, bytes: &[u8]) -> Result<(), Fatal> {
    let stream = VorbisStream::open(Cursor::new(bytes))
        .map_err(|e| Fatal(format!("{path}: not a valid Ogg Vorbis stream ({e})")))?;
    let (channels, rate) = (stream.channels(), stream.sample_rate());
    if channels != CHANNELS || rate != SAMPLE_RATE {
        return Err(Fatal(format!(
            "{path}: {channels} channels at {rate} Hz; sounds must be {CHANNELS} channel at \
             {SAMPLE_RATE} Hz"
        )));
    }
    Ok(())
}

/// The loaded sounds, by resolved name: a name's first request loads its
/// file, later ones reuse it (section 2).
#[derive(Default)]
pub struct Sounds {
    loaded: HashMap<String, Rc<Sound>>,
    warnings: Vec<String>,
}

impl Sounds {
    pub fn new() -> Self {
        Self::default()
    }

    /// The sound requested as `name`, loaded on first request. A request is
    /// matched against loaded sounds after two of the three suffix removals
    /// (Gaps found in review), so a name with three or more `.` is loaded
    /// again on each request.
    pub fn get(
        &mut self,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        name: &str,
    ) -> Result<Rc<Sound>, Fatal> {
        if let Some(sound) = self.loaded.get(&search_name(name)) {
            return Ok(sound.clone());
        }
        let resolved = resolved_name(name);
        let path =
            file_path(tree, aliases, name).map_err(|e| Fatal(format!("sound {name}: {e}")))?;
        let (sound, warning) = Sound::load(tree, &resolved, &path)?;
        self.warnings.extend(warning);
        let sound = Rc::new(sound);
        self.loaded.insert(resolved, sound.clone());
        Ok(sound)
    }

    /// Number of loaded sounds.
    pub fn len(&self) -> usize {
        self.loaded.len()
    }

    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }

    /// The warnings written since the last call (section 4), for the log.
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::vfs::{DirEntry, VfsFile};

    #[test]
    fn names_lose_up_to_three_suffixes() {
        assert_eq!(resolved_name("Ambient\\Sparks1"), "ambient\\sparks1");
        assert_eq!(resolved_name("a\\b.ogg"), "a\\b");
        assert_eq!(resolved_name("a.b.c"), "a");
        assert_eq!(resolved_name("a.b.c.d"), "a");
        assert_eq!(resolved_name("a.b.c.d.e"), "a.b");
        assert_eq!(
            resolved_name("scripts\\m_gameplay\\podoidi-blije.sfk"),
            "scripts\\m_gameplay\\podoidi-blije"
        );
        assert_eq!(search_name("a.b.c.d"), "a.b");
    }

    struct Files {
        files: HashMap<String, Vec<u8>>,
        opened: Cell<usize>,
    }

    struct MemFile(Cursor<Vec<u8>>);
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
            self.opened.set(self.opened.get() + 1);
            self.files
                .get(&path)
                .map(|b| Box::new(MemFile(Cursor::new(b.clone()))) as Box<dyn VfsFile>)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path))
        }
        fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
            Ok(Vec::new())
        }
        fn exists(&self, path: &str) -> bool {
            vfs::normalize(path).is_ok_and(|p| self.files.contains_key(&p))
        }
    }

    fn aliases() -> PathAliases {
        let mut aliases = PathAliases::parse(
            b"$game_data$ = false| true | $fs_root$ | gamedata\\\n\
              $game_levels$ = true | false| $game_data$ | levels\\\n\
              $game_sounds$ = true | true | $game_data$ | sounds\\\n\
              $level$ = false| false| $game_levels$\n",
            "",
        )
        .unwrap();
        aliases.set_dir(LEVEL, "gamedata\\levels\\l01_escape");
        aliases
    }

    fn files(paths: &[(&str, &[u8])]) -> Files {
        Files {
            files: paths
                .iter()
                .map(|(p, b)| (p.to_string(), b.to_vec()))
                .collect(),
            opened: Cell::new(0),
        }
    }

    #[test]
    fn the_level_directory_comes_first() {
        let aliases = aliases();
        let tree = files(&[
            ("gamedata/sounds/a/b.ogg", b"x"),
            ("gamedata/sounds/c.ogg", b"x"),
            ("gamedata/levels/l01_escape/a/b.ogg", b"x"),
        ]);
        assert_eq!(
            file_path(&tree, &aliases, "A\\B").unwrap(),
            "gamedata\\levels\\l01_escape\\a\\b.ogg"
        );
        assert_eq!(
            file_path(&tree, &aliases, "c").unwrap(),
            "gamedata\\sounds\\c.ogg"
        );
        // A name with no file anywhere resolves to the sound directory.
        assert_eq!(
            file_path(&tree, &aliases, "d").unwrap(),
            "gamedata\\sounds\\d.ogg"
        );
    }

    #[test]
    fn missing_empty_and_invalid_files_are_fatal() {
        let aliases = aliases();
        let tree = files(&[
            ("gamedata/sounds/empty.ogg", b""),
            ("gamedata/sounds/junk.ogg", b"not an ogg stream"),
        ]);
        let mut sounds = Sounds::new();
        for (name, says) in [
            ("missing", "cannot open"),
            ("empty", "empty"),
            ("junk", "not a valid Ogg Vorbis stream"),
        ] {
            let err = sounds.get(&tree, &aliases, name).unwrap_err();
            assert!(err.0.contains(says), "{name}: {err}");
        }
        assert!(sounds.is_empty());
    }

    #[test]
    fn a_name_loads_once() {
        // The cache is checked before the file is opened, so a sound kept
        // under a name needs no valid file to be found again.
        let aliases = aliases();
        let tree = files(&[]);
        let mut sounds = Sounds::new();
        let sound = Rc::new(Sound {
            name: "a\\b".into(),
            path: "gamedata\\sounds\\a\\b.ogg".into(),
            params: SoundParams::default(),
            samples: 0,
        });
        sounds.loaded.insert("a\\b".into(), sound.clone());
        for name in ["a\\b", "A\\B", "a\\b.ogg", "a\\b.x.y"] {
            assert!(Rc::ptr_eq(
                &sounds.get(&tree, &aliases, name).unwrap(),
                &sound
            ));
        }
        assert_eq!(tree.opened.get(), 0);
        // Three suffixes: searched as "a\b.x", so not found, and loaded again.
        assert!(sounds.get(&tree, &aliases, "a\\b.x.y.z").is_err());
    }
}
