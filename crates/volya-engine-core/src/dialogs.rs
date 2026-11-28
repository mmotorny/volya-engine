// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Dialogs loaded by id (docs/formats/dialog-files.md 3.2;
//! [`volya_engine_formats::dialog_files`] holds the format). The ids are
//! registered with the start step's declarations
//! ([`Declarations::dialogs`], sections 1, 3.1); [`Dialogs`] reads a
//! dialog's contents from its file the first time its id is loaded.
//!
//! The original keeps each file's document open for the run; we keep only
//! the ids and where their dialogs are, and a [`Dialogs`] parses a file again
//! when it first loads one of its dialogs (as `volya_engine::characters`:
//! the files do not change while the program runs). A dialog's contents
//! are read once per [`Dialogs`] and then shared ([`Arc`]); reading them
//! again gives the same value.
//!
//! [`conditions`] tests a dialog's and a phrase's conditions and runs a
//! phrase's effects (DG-2); [`flow`] proceeds through a dialog in a talk
//! (DG-4).

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io;
use std::sync::Arc;

use volya_engine_formats::dialog_files::{DIALOG, Dialog};
use volya_engine_formats::failed_check::logged;
use volya_engine_formats::xml::Document;

use crate::config::PathAliases;
use crate::info_portions::{Declarations, read_gameplay};
use crate::vfs::Vfs;

pub mod conditions;
pub mod flow;

/// The dialogs loaded so far, by dialog index, and the documents read for
/// them, by list item.
pub struct Dialogs {
    declarations: Arc<Declarations>,
    documents: HashMap<usize, Document>,
    loaded: HashMap<usize, Arc<Dialog>>,
}

impl Dialogs {
    pub fn new(declarations: Arc<Declarations>) -> Self {
        Self {
            declarations,
            documents: HashMap::new(),
            loaded: HashMap::new(),
        }
    }

    pub fn declarations(&self) -> &Declarations {
        &self.declarations
    }

    /// The dialog `id` (3.2), looked up exactly, its contents read from its
    /// file in `vfs` the first time. An id no listed file defines is a
    /// fatal error after a log line `[n]=[id]` per registered dialog; a
    /// broken reference of its phrases is one when it is first read
    /// (section 8).
    // TODO(#907): a dialog with an `init_func` has its phrases built by
    // that function, called once when the dialog is first loaded (DG-9);
    // here it has none.
    pub fn load(
        &mut self,
        vfs: &dyn Vfs,
        aliases: &PathAliases,
        id: &str,
    ) -> io::Result<Arc<Dialog>> {
        let mut log = Vec::new();
        let Ok(index) = self.declarations.dialogs.look_up(id, &mut log) else {
            return Err(logged("item not found, id", Some(id), log));
        };
        if let Some(dialog) = self.loaded.get(&index) {
            return Ok(Arc::clone(dialog));
        }
        let place = self.declarations.dialog_place(index);
        let item = &self.declarations.dialog_files()[place.file];
        let doc = match self.documents.entry(place.file) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(read_gameplay(vfs, aliases, item)?),
        };
        let element = (doc.root().into_iter())
            .flat_map(|root| root.children_named(DIALOG))
            .nth(place.position)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{item}: dialog {} changed since startup", place.position),
                )
            })?;
        let dialog = Arc::new(Dialog::read(element, item)?);
        self.loaded.insert(index, Arc::clone(&dialog));
        Ok(dialog)
    }
}

#[cfg(test)]
mod tests;
