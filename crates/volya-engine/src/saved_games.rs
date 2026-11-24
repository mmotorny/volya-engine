// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The saved-games folder (`$game_saves$`, docs/formats/sav.md section 1.1)
//! as the menu and its save and load screens see it
//! (docs/behavior/save-load-screens.md sections 3.4, 7 and 11): every file
//! with its last-modification time, and the `.sav` files' contents.
//!
//! The platform lists the folder when the game starts
//! ([`SavedGames::new`]); on the web it is the `savedgames` directory of
//! Volya Engine's OPFS namespace, beside `user.ltx` (`$app_data_root$`).
//! Nothing writes saves yet (#75), so the list changes only by deletions,
//! which the app hands the platform ([`crate::app::App::take_deleted_saves`]).

use volya_engine_formats::sav;

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
    /// Relative to the folder, `\` between folders.
    pub path: String,
    /// The last-modification time: milliseconds since the Unix epoch, for
    /// ordering, and as local time, for showing.
    pub modified_ms: f64,
    pub modified: LocalTime,
    /// The file's bytes, for `.sav` files directly in the folder (the only
    /// ones a screen reads); `None` for others.
    pub contents: Option<Vec<u8>>,
}

/// A row of a save list (section 3.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRow {
    /// The file's name without `.sav`, lower case.
    pub name: String,
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedGames {
    files: Vec<SavedFile>,
}

const SAV: &str = ".sav";

fn is_sav(path: &str) -> bool {
    path.len() > SAV.len()
        && path.is_char_boundary(path.len() - SAV.len())
        && path[path.len() - SAV.len()..].eq_ignore_ascii_case(SAV)
}

impl SavedGames {
    pub fn new(files: Vec<SavedFile>) -> Self {
        Self { files }
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
            .filter(|f| is_sav(&f.path) && (sub_folders || !f.path.contains(['\\', '/'])))
            .collect();
        files.sort_by(|a, b| {
            b.modified_ms
                .total_cmp(&a.modified_ms)
                .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
        });
        files
            .into_iter()
            .map(|f| SaveRow {
                name: f.path[..f.path.len() - SAV.len()]
                    .replace('/', "\\")
                    .to_lowercase(),
                modified: f.modified,
            })
            .collect()
    }

    /// The file of save `name` (`<name>.sav`, names compared
    /// case-insensitively).
    pub fn get(&self, name: &str) -> Option<&SavedFile> {
        let file = format!("{name}{SAV}").to_lowercase();
        self.files
            .iter()
            .find(|f| f.path.replace('/', "\\").to_lowercase() == file)
    }

    /// Whether `<name>.sav` exists.
    pub fn exists(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Whether save `name` exists and is valid (sav.md section 2).
    pub fn is_valid(&self, name: &str) -> bool {
        self.get(name)
            .and_then(|f| f.contents.as_deref())
            .is_some_and(sav::is_valid)
    }

    /// Deletes `<name>.sav` and `<name>.dds` (section 3.6); returns the
    /// paths removed.
    pub fn delete(&mut self, name: &str) -> Vec<String> {
        let name = name.to_lowercase();
        let doomed = |f: &SavedFile| {
            let p = f.path.replace('/', "\\").to_lowercase();
            p == format!("{name}.sav") || p == format!("{name}.dds")
        };
        let removed = self
            .files
            .iter()
            .filter(|f| doomed(f))
            .map(|f| f.path.clone())
            .collect();
        self.files.retain(|f| !doomed(f));
        removed
    }

    /// The remembered last save is valid (title-menu.md section 4.1): the
    /// name `last` (`load_last_save <name>` of the user configuration file)
    /// is non-empty and its file exists and is valid.
    pub fn last_save_valid(&self, last: Option<&str>) -> bool {
        last.is_some_and(|n| !n.is_empty() && self.is_valid(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, ms: f64, contents: Option<&[u8]>) -> SavedFile {
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
            contents: contents.map(<[u8]>::to_vec),
        }
    }

    #[test]
    fn rows_are_sav_files_newest_first_lower_case() {
        let valid = [0xff, 0xff, 0xff, 0xff, 3, 0, 0, 0];
        let saves = SavedGames::new(vec![
            file("Old.sav", 60_000.0, Some(&valid)),
            file("new.SAV", 300_000.0, Some(&[1, 2])),
            file("new.dds", 400_000.0, None),
            file("sub\\deep.sav", 200_000.0, None),
            file("demo.xrdemo", 500_000.0, None),
        ]);
        let names = |rows: Vec<SaveRow>| rows.into_iter().map(|r| r.name).collect::<Vec<_>>();
        assert_eq!(names(saves.rows(false)), ["new", "old"]);
        assert_eq!(names(saves.rows(true)), ["new", "sub\\deep", "old"]);
        // Section 13 fact 7.
        let row = SaveRow {
            name: "x".into(),
            modified: LocalTime {
                year: 2026,
                month: 11,
                day: 24,
                hour: 14,
                minute: 5,
            },
        };
        assert_eq!(row.time_text(), "[24:11:2026 14:05]");
        assert!(saves.exists("OLD") && saves.is_valid("old"));
        assert!(!saves.is_valid("new"), "too short");
        assert!(saves.last_save_valid(Some("Old")));
        assert!(!saves.last_save_valid(Some("new")));
        assert!(!saves.last_save_valid(Some("")) && !saves.last_save_valid(None));
    }

    #[test]
    fn deleting_removes_the_save_and_its_picture() {
        let mut saves = SavedGames::new(vec![
            file("a.sav", 1.0, None),
            file("A.dds", 1.0, None),
            file("b.sav", 1.0, None),
        ]);
        assert_eq!(saves.delete("a"), ["a.sav", "A.dds"]);
        assert_eq!(saves.files().len(), 1);
    }
}
