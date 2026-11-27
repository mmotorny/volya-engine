// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The log (docs/behavior/console-log.md): the one list of text lines every
//! part of the program writes to and the console shows, how a written text
//! becomes lines (section 1), and the log file's contents and when they are
//! written (section 3). Storing the file is the platform's
//! ([`Log::take_file`]).

use volya_engine_formats::cp1251;

/// A formatted message keeps at most this many characters (1.3).
const MESSAGE_LENGTH: usize = 1023;

/// The log's lines, and the log file's contents to store.
#[derive(Clone, Debug, Default)]
pub struct Log {
    /// Oldest first.
    lines: Vec<String>,
    /// Whether the log file has been set up (3.3: before, writing it does
    /// nothing).
    file_set_up: bool,
    /// The file's contents written since the last [`Log::take_file`]: each
    /// write replaces the whole file, so only the last one matters.
    file: Option<Vec<u8>>,
}

impl Log {
    /// Writes `text` (1.2): every piece ending in a line feed is a line, an
    /// empty one stored as a single space; the piece after the last line
    /// feed is one more line, stored as it is.
    pub fn write(&mut self, text: &str) {
        let mut pieces = text.split('\n').peekable();
        while let Some(piece) = pieces.next() {
            let last = pieces.peek().is_none();
            self.lines.push(match piece {
                "" if !last => " ".to_owned(),
                piece => piece.to_owned(),
            });
        }
    }

    /// A formatted message (1.3): cut to its first 1023 characters; an empty
    /// one writes nothing.
    pub fn message(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        match text.char_indices().nth(MESSAGE_LENGTH) {
            Some((at, _)) => self.write(&text[..at]),
            None => self.write(text),
        }
    }

    /// Text and a word (1.3): `<text> <word>`, `<text>` alone without one.
    pub fn word(&mut self, text: &str, word: Option<&str>) {
        match word {
            Some(word) => self.write(&format!("{text} {word}")),
            None => self.write(text),
        }
    }

    /// Text and an integer (1.3), in signed decimal: an unsigned value is
    /// given as its bits (`as i32`), so values from 2³¹ up appear negative.
    pub fn integer(&mut self, text: &str, value: i32) {
        self.write(&format!("{text} {value}"));
    }

    /// Text and a real (1.3), six digits after the point (finite values).
    pub fn real(&mut self, text: &str, value: f32) {
        self.write(&format!("{text} {}", real(value)));
    }

    /// Text and a vector (1.3): `<text> (<x>,<y>,<z>)`.
    pub fn vector(&mut self, text: &str, [x, y, z]: [f32; 3]) {
        let (x, y, z) = (real(x), real(y), real(z));
        self.write(&format!("{text} ({x},{y},{z})"));
    }

    /// Text and a 4 × 4 matrix (1.3): `<text>:`, its four rows, an empty
    /// line.
    pub fn matrix(&mut self, text: &str, rows: [[f32; 4]; 4]) {
        let mut out = format!("{text}:\n");
        for row in rows {
            let row: Vec<String> = row.into_iter().map(real).collect();
            out.push_str(&row.join(","));
            out.push('\n');
        }
        self.write(&out);
    }

    /// The lines, oldest first.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Empties the log (`clear_log`, 3.6).
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Sets the log file up, at the end of the file system's setup (3.3):
    /// it is created empty, replacing an earlier one.
    pub fn set_up_file(&mut self) {
        self.file_set_up = true;
        self.file = Some(Vec::new());
    }

    /// Writes the log file (3.2), once it has been set up: the whole log,
    /// every line followed by CR LF.
    pub fn save(&mut self) {
        if !self.file_set_up {
            return;
        }
        let mut file = Vec::new();
        for line in &self.lines {
            match cp1251::encode(line) {
                Some(bytes) => file.extend(bytes),
                None => file.extend(line.as_bytes()),
            }
            file.extend(b"\r\n");
        }
        self.file = Some(file);
    }

    /// The log file's contents written since the last call, for the
    /// platform to store.
    pub fn take_file(&mut self) -> Option<Vec<u8>> {
        self.file.take()
    }
}

/// C's `%f`.
fn real(value: f32) -> String {
    format!("{value:.6}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(f: impl FnOnce(&mut Log)) -> Vec<String> {
        let mut log = Log::default();
        f(&mut log);
        log.lines
    }

    #[test]
    fn a_text_is_split_at_line_feeds() {
        // 1.2: an empty piece before a line feed is a space, the last
        // piece is kept as it is.
        assert_eq!(written(|l| l.write("a\n\nb")), ["a", " ", "b"]);
        assert_eq!(written(|l| l.write("a\n")), ["a", ""]);
        assert_eq!(written(|l| l.write("")), [""]);
        assert_eq!(written(|l| l.write("\n")), [" ", ""]);
        assert_eq!(written(|l| l.write("\ta\r")), ["\ta\r"]);
    }

    #[test]
    fn a_message_is_cut_and_an_empty_one_writes_nothing() {
        assert!(written(|l| l.message("")).is_empty());
        let long = "я".repeat(1100);
        let lines = written(|l| l.message(&long));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].chars().count(), 1023);
        // Cut before it is split.
        let text = format!("{}\nb", "a".repeat(1023));
        assert_eq!(written(|l| l.message(&text)), ["a".repeat(1023)]);
    }

    #[test]
    fn the_forms_with_a_value() {
        assert_eq!(
            written(|l| l.word("! Unknown command: ", Some("x"))),
            ["! Unknown command:  x"]
        );
        assert_eq!(written(|l| l.word("a", None)), ["a"]);
        assert_eq!(
            written(|l| l.integer("n", 3_000_000_000u32 as i32)),
            ["n -1294967296"]
        );
        assert_eq!(written(|l| l.real("r", 0.5)), ["r 0.500000"]);
        assert_eq!(
            written(|l| l.vector("v", [1.0, -2.0, 0.25])),
            ["v (1.000000,-2.000000,0.250000)"]
        );
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let lines = written(|l| l.matrix("m", identity));
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[0], "m:");
        assert_eq!(lines[1], "1.000000,0.000000,0.000000,0.000000");
        assert_eq!(lines[5], "");
    }

    #[test]
    fn the_file_is_every_line_with_cr_lf() {
        let mut log = Log::default();
        log.write("before");
        log.save();
        assert_eq!(log.take_file(), None, "not set up yet");
        log.set_up_file();
        assert_eq!(log.take_file(), Some(Vec::new()), "created empty");
        log.write("я\n");
        log.save();
        assert_eq!(log.take_file().unwrap(), b"before\r\n\xff\r\n\r\n");
        assert_eq!(log.take_file(), None);
        log.clear();
        log.save();
        assert_eq!(log.take_file(), Some(Vec::new()));
    }
}
