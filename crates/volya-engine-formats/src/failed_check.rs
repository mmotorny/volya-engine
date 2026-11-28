// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A failed check: the fatal error the original reports (console-log.md
//! 3.8) with its description, its argument and the log lines written
//! before it, as the specs that name one state it
//! (character-descriptions.md and dialog-files.md, conventions).

use std::fmt;
use std::io;

/// A failed check: the fatal error's description, its argument, and the
/// log lines written before it. It travels inside an [`io::Error`]
/// ([`Self::of`]).
// TODO(#1012): the app reports it as a fatal message of its text, without
// the log lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailedCheck {
    pub description: String,
    pub argument: Option<String>,
    /// Lines written to the log before the report, in order.
    pub log: Vec<String>,
}

impl FailedCheck {
    /// The failed check `error` carries, if any.
    pub fn of(error: &io::Error) -> Option<&FailedCheck> {
        error.get_ref()?.downcast_ref()
    }
}

impl fmt::Display for FailedCheck {
    /// The description, then the argument, separated by a space.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.description)?;
        if let Some(a) = &self.argument {
            write!(f, " {a}")?;
        }
        Ok(())
    }
}

impl std::error::Error for FailedCheck {}

/// The error of a failed check with no log lines.
pub fn failed(description: impl Into<String>, argument: Option<&str>) -> io::Error {
    logged(description, argument, Vec::new())
}

/// The error of a failed check after the log lines `log`.
pub fn logged(
    description: impl Into<String>,
    argument: Option<&str>,
    log: Vec<String>,
) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        FailedCheck {
            description: description.into(),
            argument: argument.map(str::to_owned),
            log,
        },
    )
}
