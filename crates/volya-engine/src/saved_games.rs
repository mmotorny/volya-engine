// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The saved-games folder (`$game_saves$`, docs/formats/sav.md section 1.1)
//! as the menu and its save and load screens see it
//! (docs/behavior/save-load-screens.md sections 3.4, 7 and 11): every file
//! with its last-modification time, the first bytes of the `.sav` files
//! (all validity needs, sav.md section 2), and the files' contents read on
//! demand ([`SaveFiles`]).
//!
//! The platform lists the folder when the game starts
//! ([`SavedGames::new`]); on the web it is the `savedgames` directory of
//! Volya Engine's OPFS namespace, beside `user.ltx` (`$app_data_root$`).
//! Nothing writes saves yet (#75), so the list changes only by deletions,
//! which the app hands the platform ([`crate::app::App::take_deleted_saves`]).

use std::fmt;
use std::io;
use std::rc::Rc;

use volya_engine_formats::sav;

/// Bytes of a `.sav` file kept from the listing: enough for the validity
/// test (sav.md section 2).
pub const HEAD: usize = 8;

/// A local date and time to the minute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalTime {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

/// One file of the saved-games folder.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedFile {
    /// Relative to the folder, `\` between folders, as stored.
    pub path: String,
    /// The last-modification time: milliseconds since the Unix epoch, for
    /// ordering, and as local time, for showing.
    pub modified_ms: f64,
    pub modified: LocalTime,
    /// The first [`HEAD`] bytes (fewer for a shorter file) of a `.sav` file
    /// directly in the folder; empty for others.
    pub head: Vec<u8>,
}

/// Reads a file of the folder (relative path as listed).
pub trait SaveFiles {
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
}

/// A row of a save list (section 3.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRow {
    /// The file's name without `.sav`, lower case.
    pub name: String,
    /// The file's path as stored.
    pub path: String,
    pub modified: LocalTime,
}

impl SaveRow {
    /// The row's time text: `[DD:MM:YYYY HH:MM]` (section 3.4).
    pub fn time_text(&self) -> String {
        let t = self.modified;
        format!(
            "[{:02}:{:02}:{:04} {:02}:{:02}]",
            t.day, t.month, t.year, t.hour, t.minute
        )
    }
}

/// The folder's files.
#[derive(Clone, Default)]
pub struct SavedGames {
    files: Vec<SavedFile>,
    source: Option<Rc<dyn SaveFiles>>,
}

impl fmt::Debug for SavedGames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedGames")
            .field("files", &self.files)
            .finish()
    }
}

const SAV: &str = ".sav";
const DDS: &str = ".dds";

fn has_ext(path: &str, ext: &str) -> bool {
    path.len() > ext.len()
        && path.is_char_boundary(path.len() - ext.len())
        && path[path.len() - ext.len()..].eq_ignore_ascii_case(ext)
}

fn normal(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

impl SavedGames {
    /// The listed `files`, whose contents `source` reads.
    pub fn new(files: Vec<SavedFile>, source: Option<Rc<dyn SaveFiles>>) -> Self {
        Self { files, source }
    }

    pub fn files(&self) -> &[SavedFile] {
        &self.files
    }

    /// The save list's rows (section 3.4): every `*.sav` file, only those
    /// directly in the folder unless `sub_folders` (the save screen lists
    /// sub-folders' too, with their relative path), names lower case, most
    /// recently modified first. Equal times keep the order of the names
    /// (our choice: the original's order is not established, section 14).
    pub fn rows(&self, sub_folders: bool) -> Vec<SaveRow> {
        let mut files: Vec<&SavedFile> = self
            .files
            .iter()
            .filter(|f| has_ext(&f.path, SAV) && (sub_folders || !f.path.contains(['\\', '/'])))
            .collect();
        files.sort_by(|a, b| {
            b.modified_ms
                .total_cmp(&a.modified_ms)
                .then_with(|| a.path.cmp(&b.path))
        });
        files
            .into_iter()
            .map(|f| SaveRow {
                name: normal(&f.path[..f.path.len() - SAV.len()]),
                path: f.path.clone(),
                modified: f.modified,
            })
            .collect()
    }

    fn file(&self, path: &str) -> Option<&SavedFile> {
        self.files.iter().find(|f| f.path == path)
    }

    /// Whether `<name>.sav` exists (names compared case-insensitively,
    /// section 4.3).
    pub fn exists(&self, name: &str) -> bool {
        let file = normal(&format!("{name}{SAV}"));
        self.files.iter().any(|f| normal(&f.path) == file)
    }

    /// Whether the file at `path` exists and is a valid save (sav.md
    /// section 2).
    pub fn is_valid(&self, path: &str) -> bool {
        self.file(path).is_some_and(|f| sav::is_valid(&f.head))
    }

    /// The contents of the file at `path`; `None` when it is not listed.
    pub fn read(&self, path: &str) -> Option<io::Result<Vec<u8>>> {
        self.file(path)?;
        Some(match &self.source {
            Some(s) => s.read(path),
            None => Err(io::Error::new(io::ErrorKind::NotFound, path.to_owned())),
        })
    }

    /// Deletes the save at `path` and its picture, the `.dds` of the same
    /// name (section 3.6); returns the paths removed. Only that save's
    /// files go: a picture differing in case is taken when no exact one
    /// exists, it is the only one, and no listed save has its exact name
    /// (our choice).
    pub fn delete(&mut self, path: &str) -> Vec<String> {
        let Some(stem) = path.get(..path.len().saturating_sub(SAV.len())) else {
            return Vec::new();
        };
        let exact = format!("{stem}{DDS}");
        let pictures: Vec<&SavedFile> = self
            .files
            .iter()
            .filter(|f| normal(&f.path) == normal(&exact))
            .collect();
        // A picture differing in case is another save's when a save of its
        // exact name is listed.
        let owned = |p: &SavedFile| {
            let stem = &p.path[..p.path.len() - DDS.len()];
            self.files.iter().any(|f| {
                f.path != path
                    && has_ext(&f.path, SAV)
                    && &f.path[..f.path.len() - SAV.len()] == stem
            })
        };
        let picture = pictures
            .iter()
            .find(|f| f.path == exact)
            .or_else(|| (pictures.len() == 1 && !owned(pictures[0])).then(|| &pictures[0]))
            .map(|f| f.path.clone());
        let mut removed = Vec::new();
        for doomed in std::iter::once(path.to_owned()).chain(picture) {
            if let Some(i) = self.files.iter().position(|f| f.path == doomed) {
                removed.push(self.files.remove(i).path);
            }
        }
        removed
    }

    /// The remembered last save is valid (title-menu.md section 4.1): the
    /// name `last` (`load_last_save <name>` of the user configuration file)
    /// is non-empty and its file exists and is valid.
    pub fn last_save_valid(&self, last: Option<&str>) -> bool {
        let Some(last) = last.filter(|n| !n.is_empty()) else {
            return false;
        };
        let file = normal(&format!("{last}{SAV}"));
        self.files
            .iter()
            .any(|f| normal(&f.path) == file && sav::is_valid(&f.head))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Files held in memory.
    pub struct MemFiles(pub HashMap<String, Vec<u8>>);

    impl SaveFiles for MemFiles {
        fn read(&self, path: &str) -> io::Result<Vec<u8>> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned()))
        }
    }

    pub fn file(path: &str, ms: f64, head: &[u8]) -> SavedFile {
        SavedFile {
            path: path.into(),
            modified_ms: ms,
            modified: LocalTime {
                year: 2026,
                month: 11,
                day: 24,
                hour: 14,
                minute: (ms / 60_000.0) as u32 % 60,
            },
            head: head.to_vec(),
        }
    }

    pub const VALID: [u8; 8] = [0xff, 0xff, 0xff, 0xff, 3, 0, 0, 0];

    #[test]
    fn rows_are_sav_files_newest_first_lower_case() {
        let saves = SavedGames::new(
            vec![
                file("Old.sav", 60_000.0, &VALID),
                file("new.SAV", 300_000.0, &[1, 2]),
                file("new.dds", 400_000.0, &[]),
                file("sub\\deep.sav", 200_000.0, &[]),
                file("demo.xrdemo", 500_000.0, &[]),
            ],
            None,
        );
        let names = |rows: Vec<SaveRow>| rows.into_iter().map(|r| r.name).collect::<Vec<_>>();
        assert_eq!(names(saves.rows(false)), ["new", "old"]);
        assert_eq!(names(saves.rows(true)), ["new", "sub\\deep", "old"]);
        assert_eq!(saves.rows(false)[1].path, "Old.sav");
        // Section 13 fact 7.
        let row = SaveRow {
            name: "x".into(),
            path: "x.sav".into(),
            modified: LocalTime {
                year: 2026,
                month: 11,
                day: 24,
                hour: 14,
                minute: 5,
            },
        };
        assert_eq!(row.time_text(), "[24:11:2026 14:05]");
        assert!(saves.exists("OLD") && saves.is_valid("Old.sav"));
        assert!(!saves.is_valid("new.SAV"), "too short");
        assert!(saves.last_save_valid(Some("old")));
        assert!(!saves.last_save_valid(Some("new")));
        assert!(!saves.last_save_valid(Some("")) && !saves.last_save_valid(None));
        // Contents come from the source, only for listed files.
        assert!(saves.read("Old.sav").unwrap().is_err(), "no source");
        assert!(saves.read("gone.sav").is_none());
        let source = MemFiles(HashMap::from([("Old.sav".to_owned(), VALID.to_vec())]));
        let saves = SavedGames::new(saves.files, Some(Rc::new(source)));
        assert_eq!(saves.read("Old.sav").unwrap().unwrap(), VALID);
    }

    #[test]
    fn deleting_removes_only_that_save_and_its_picture() {
        let mut saves = SavedGames::new(
            vec![
                file("a.sav", 1.0, &[]),
                file("A.sav", 1.0, &[]),
                file("A.dds", 1.0, &[]),
                file("b.sav", 1.0, &[]),
                file("b.DDS", 1.0, &[]),
            ],
            None,
        );
        // Names differing in case are distinct rows; each delete takes one.
        // A.dds is A.sav's picture, not a.sav's.
        assert_eq!(saves.rows(false).len(), 3);
        assert_eq!(saves.delete("a.sav"), ["a.sav"]);
        assert_eq!(saves.delete("A.sav"), ["A.sav", "A.dds"]);
        // A picture differing in case, with no save of its exact name.
        assert_eq!(saves.delete("b.sav"), ["b.sav", "b.DDS"]);
        assert!(saves.files().is_empty());
    }
}
