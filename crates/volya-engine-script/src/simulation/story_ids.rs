// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The classes `story_ids` and `spawn_story_ids` (4.1): holders of
//! integer constants made from a section of the game configuration.

use volya_engine_core::config::Config;
use volya_engine_formats::ltx::value;

use super::Context;
use crate::classes::EngineClass;
use crate::{Error, ErrorKind};

/// One story-id class: its name, its section, its reserved name and the
/// descriptions of its three fatal checks.
pub struct StoryIdClass {
    pub class: &'static str,
    pub section: &'static str,
    pub reserved: &'static str,
    pub spaces: &'static str,
    pub redefinition: &'static str,
    pub duplicate: &'static str,
}

/// The two classes (4.1).
pub const CLASSES: [StoryIdClass; 2] = [
    StoryIdClass {
        class: "story_ids",
        section: "story_ids",
        reserved: "INVALID_STORY_ID",
        spaces: "Invalid story id description (contains spaces)!",
        redefinition: "INVALID_STORY_ID redifinition!",
        duplicate: "Duplicated story id description!",
    },
    StoryIdClass {
        class: "spawn_story_ids",
        section: "spawn_story_ids",
        reserved: "INVALID_SPAWN_STORY_ID",
        spaces: "Invalid spawn story id description (contains spaces)!",
        redefinition: "INVALID_SPAWN_STORY_ID redifinition!",
        duplicate: "Duplicated spawn story id description!",
    },
];

/// The constants of `class` read from `game` (4.1 steps 1 to 4), in
/// order; `Err` holds the failed check's description.
pub fn constants(class: &StoryIdClass, game: &Config) -> Result<Vec<(String, i32)>, String> {
    let Some(section) = game.section(class.section) else {
        return Err(format!("cannot find section {}", class.section));
    };
    let mut list: Vec<(String, i32)> = Vec::with_capacity(section.len() + 1);
    for entry in section.entries() {
        let name = value::unquote(entry.value().unwrap_or("").trim()).to_owned();
        if name.contains(' ') {
            return Err(class.spaces.into());
        }
        if name == class.reserved {
            return Err(class.redefinition.into());
        }
        if list.iter().any(|(n, _)| *n == name) {
            return Err(class.duplicate.into());
        }
        list.push((name, value::int(entry.key()) as i32));
    }
    list.push((class.reserved.to_owned(), -1));
    Ok(list)
}

/// The two classes made from `game`; a failed check is fatal.
pub(crate) fn classes(ctx: &Context, game: &Config) -> Result<Vec<EngineClass>, Error> {
    CLASSES
        .iter()
        .map(|c| {
            let list = constants(c, game).map_err(|why| Error {
                kind: ErrorKind::Fatal,
                message: ctx.failed_check("story_ids", &why, &[]),
            })?;
            Ok(
                (list.iter()).fold(EngineClass::new(c.class), |class, (name, value)| {
                    class.constant(name, *value)
                }),
            )
        })
        .collect()
}
