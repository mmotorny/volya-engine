// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What the tests of the shipped scripts share: the aliases, the engine
//! bases the scripts' classes derive from, the game's trees, Lua's tokens
//! of a script, and which script names the game can load.

// Each test uses some of them.
#![allow(dead_code)]

use std::io;

use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, VfsFile};
use volya_engine_script::loader::Trees;

/// The aliases scripts use (as in tests/gog.rs).
pub const FSGAME: &[u8] = b"$game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
    $game_scripts$ =  true | false| $game_data$    | scripts\\\r\n\
    $game_config$ =   true | false| $game_data$    | config\\\r\n";

/// The 33 engine bases of script API A (its 1.3) and their constructors' parameter counts
/// (3.2; one count for a base the spec gives one for).
pub const ENGINE_BASES: &[(&str, &[usize])] = &[
    ("property_evaluator", &[2, 0]),
    ("action_base", &[2, 0]),
    ("CUIScriptWnd", &[0]),
    ("object_binder", &[1]),
    ("effector", &[2]),
    ("CUIWindow", &[0]),
    ("CUIListItemEx", &[0]),
    ("cse_alife_item", &[1]),
    ("cse_alife_smart_zone", &[1]),
    ("game_sv_mp_script", &[0]),
    ("CGameObject", &[0]),
    ("CSE_AbstractVisual", &[1]),
    ("UIGame_custom_script", &[0]),
    ("cse_alife_car", &[1]),
    ("cse_alife_helicopter", &[1]),
    ("cse_alife_human_stalker", &[1]),
    ("cse_alife_item_artefact", &[1]),
    ("cse_alife_item_custom_outfit", &[1]),
    ("cse_alife_item_torch", &[1]),
    ("cse_alife_item_weapon", &[1]),
    ("cse_alife_item_weapon_magazined", &[1]),
    ("cse_alife_item_weapon_magazined_w_gl", &[1]),
    ("cse_alife_item_weapon_shotgun", &[1]),
    ("cse_alife_monster_base", &[1]),
    ("cse_alife_object_physic", &[1]),
    ("cse_alife_space_restrictor", &[1]),
    ("cse_alife_trader", &[1]),
    ("cse_anomalous_zone", &[1]),
    ("cse_zone_visual", &[1]),
    ("game_PlayerState", &[0]),
    ("game_cl_mp_script", &[0]),
    ("action_planner", &[0]),
    ("game_sv_Deathmatch", &[0]),
];

/// The game's trees, kept for the test's lifetime.
pub struct GameTrees {
    pub tree: DbVfs<Box<dyn VfsFile + 'static>>,
    pub root: &'static StdVfs,
}

impl Trees for GameTrees {
    fn with_trees(&self, f: &mut dyn FnMut(&dyn Vfs, &dyn Vfs)) -> io::Result<()> {
        f(&self.tree, self.root);
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum Token {
    Name(String),
    String(Vec<u8>),
    Punct(u8),
    Other,
}

/// The end of a long bracket `[==[` ... `]==]` whose opening is at `i`, or
/// `None` if there is none at `i`.
pub fn long_bracket(text: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i + 1;
    while text.get(j) == Some(&b'=') {
        j += 1;
    }
    if text.get(i) != Some(&b'[') || text.get(j) != Some(&b'[') {
        return None;
    }
    let close = [b"]".as_slice(), &vec![b'='; j - i - 1], b"]"].concat();
    let body = j + 1;
    let end = text[body..]
        .windows(close.len())
        .position(|w| w == close)
        .map_or(text.len(), |p| body + p + close.len());
    Some((body, end))
}

/// Lua's tokens of a script, after its `--` and C-style comments
/// (docs/behavior/script-loading.md 1); numbers and other symbols count
/// only as tokens.
pub fn tokens(text: &[u8]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let b = text[i];
        let next = text.get(i + 1).copied();
        if b.is_ascii_whitespace() {
            i += 1;
        } else if b == b'-' && next == Some(b'-') {
            match long_bracket(text, i + 2) {
                Some((_, end)) => i = end,
                None => {
                    while i < text.len() && text[i] != b'\n' && text[i] != b'\r' {
                        i += 1;
                    }
                }
            }
        } else if b == b'/' && next == Some(b'/') {
            while i < text.len() && text[i] != b'\n' {
                i += 1;
            }
        } else if b == b'/' && next == Some(b'*') {
            i += 2;
            while i < text.len() {
                let star = text[i] == b'*';
                i += 1;
                if star && i < text.len() {
                    i += 1;
                    if text[i - 1] == b'/' {
                        break;
                    }
                }
            }
        } else if b == b'"' || b == b'\'' {
            let mut s = Vec::new();
            i += 1;
            while i < text.len() && text[i] != b && text[i] != b'\n' {
                if text[i] == b'\\' && i + 1 < text.len() {
                    i += 1;
                }
                s.push(text[i]);
                i += 1;
            }
            i += 1;
            out.push(Token::String(s));
        } else if let Some((body, end)) = long_bracket(text, i) {
            out.push(Token::String(text[body..end.min(text.len())].to_vec()));
            i = end;
        } else if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < text.len() && (text[i].is_ascii_alphanumeric() || text[i] == b'_') {
                i += 1;
            }
            out.push(Token::Name(
                String::from_utf8_lossy(&text[start..i]).into_owned(),
            ));
        } else if b.is_ascii_digit() {
            while i < text.len() && (text[i].is_ascii_alphanumeric() || text[i] == b'.') {
                i += 1;
            }
            out.push(Token::Other);
        } else {
            out.push(Token::Punct(b));
            i += 1;
        }
    }
    out
}

/// Whether the game can load a script of this name: a Lua name
/// (docs/behavior/script-loading.md 2.3).
pub fn loadable_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}
