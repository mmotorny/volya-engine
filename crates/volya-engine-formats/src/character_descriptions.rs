// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The character descriptions (docs/formats/character-descriptions.md,
//! cited by section): the lists of `[profiles]` (section 1), the entries of
//! their documents and their id spaces (sections 2, 3, [`Registry`]), a
//! profile (section 4, [`Profile`]) and a specific character (section 5,
//! [`SpecificCharacter`]) as read from their entry, and the fatal errors the
//! format names ([`FailedCheck`]). Which files are opened and when is the
//! caller's (`volya_engine::characters`); how a profile gives a specific
//! character is CH-2's.

use std::fmt;
use std::io;

use crate::ltx::value;
use crate::text::string_table::{StringTable, normalize_whitespace};
use crate::xml::{Document, Element};

/// An integer that is not set (conventions): −2,147,483,647.
pub const NOT_SET: i32 = -2_147_483_647;
/// The entries of a profile file (section 2).
pub const PROFILE: &str = "character";
/// The entries of a specific-character file (section 2).
pub const SPECIFIC_CHARACTER: &str = "specific_character";
/// `icon`'s default (5.2).
pub const DEFAULT_ICON: &str = "ui_npc_u_barman";
/// `critical_wound_weights`' default (5.2).
pub const DEFAULT_CRITICAL_WOUND_WEIGHTS: &str = "1";

/// A failed check (conventions): the fatal error's description, its
/// argument, and the log lines written before it (3.2). It travels inside
/// an [`io::Error`] ([`Self::of`]).
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

fn failed(description: impl Into<String>, argument: Option<&str>) -> io::Error {
    logged(description, argument, Vec::new())
}

fn logged(description: impl Into<String>, argument: Option<&str>, log: Vec<String>) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        FailedCheck {
            description: description.into(),
            argument: argument.map(str::to_owned),
            log,
        },
    )
}

/// The items of a `[profiles]` list (section 1): comma-separated, each
/// trimmed of surrounding white space. No value gives no item.
pub fn list(value: Option<&str>) -> Vec<String> {
    match value {
        Some(v) => v.split(',').map(|i| i.trim().to_owned()).collect(),
        None => Vec::new(),
    }
}

/// List item `item`'s file under `$game_config$` (section 1).
pub fn file_path(item: &str) -> String {
    format!("gameplay\\{item}.xml")
}

/// The fatal error of a listed file that cannot be opened (section 1).
pub fn cannot_open(item: &str) -> io::Error {
    failed("error while parsing XML file", Some(&format!("{item}.xml")))
}

/// Where a registered entry is (3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryRef {
    /// Its registration index: 0, 1, 2, … across the list.
    pub index: usize,
    /// Its file's place in the list.
    pub file: usize,
    /// Its position among that file's entries.
    pub position: usize,
}

/// A registered id: a range of [`Registry::text`], and where its entry is.
#[derive(Clone, Copy, Debug)]
struct Registered {
    at: u32,
    len: u32,
    file: u32,
    position: u32,
}

/// One id space (3.1): the ids of one list's files, in list order and,
/// within a file, document order, every id once in one buffer.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// The list items registered, in order.
    files: Vec<String>,
    text: String,
    entries: Vec<Registered>,
    /// Registration indices, sorted by id (byte by byte).
    sorted: Vec<u32>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the entries named `entry` of list item `item`'s document
    /// `doc` (the root's children of that name, in document order; section
    /// 2), after the earlier files'. An entry without `id`, or with an id
    /// an earlier entry has, is a fatal error (3.1).
    pub fn register(&mut self, item: &str, doc: &Document, entry: &str) -> io::Result<()> {
        let file = self.files.len() as u32;
        self.files.push(item.to_owned());
        let Some(root) = doc.root() else {
            return Ok(());
        };
        for (position, e) in root.children_named(entry).enumerate() {
            let Some(id) = e.attr("id") else {
                return Err(failed(
                    format!("id for item don't set, number {position} in {item}"),
                    None,
                ));
            };
            let at = match self.search(id) {
                Ok(_) => return Err(failed("duplicate item id", Some(id))),
                Err(at) => at,
            };
            let index = self.entries.len() as u32;
            self.entries.push(Registered {
                at: self.text.len() as u32,
                len: id.len() as u32,
                file,
                position: position as u32,
            });
            self.text.push_str(id);
            self.sorted.insert(at, index);
        }
        Ok(())
    }

    fn search(&self, id: &str) -> Result<usize, usize> {
        self.sorted
            .binary_search_by(|&i| self.id(i as usize).as_bytes().cmp(id.as_bytes()))
    }

    /// The number of registered ids; the last index is one less (3.1).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The list items registered, in order.
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// The id of registration index `index`.
    pub fn id(&self, index: usize) -> &str {
        let r = self.entries[index];
        &self.text[r.at as usize..(r.at + r.len) as usize]
    }

    /// Where the entry of registration index `index` is.
    pub fn entry(&self, index: usize) -> EntryRef {
        let r = self.entries[index];
        EntryRef {
            index,
            file: r.file as usize,
            position: r.position as usize,
        }
    }

    /// The entry of `id`, looked up exactly (3.2). An empty id is a failed
    /// check; an id not registered is one after a log line `[n]=[id]` for
    /// every registered id, in registration order.
    pub fn find(&self, id: &str) -> io::Result<EntryRef> {
        if id.is_empty() {
            // The spec names no description: ours.
            return Err(failed("empty id", None));
        }
        match self.search(id) {
            Ok(at) => Ok(self.entry(self.sorted[at] as usize)),
            Err(_) => {
                let log = (0..self.len())
                    .map(|n| format!("[{n}]=[{}]", self.id(n)))
                    .collect();
                Err(logged("item not found, id", Some(id), log))
            }
        }
    }
}

/// The `position`-th entry named `entry` of `doc`'s root (3.2).
pub fn entry_element<'a>(
    doc: &'a Document,
    entry: &'a str,
    position: usize,
) -> Option<&'a Element> {
    doc.root()?.children_named(entry).nth(position)
}

/// An element's value (section 2): its text when its first child is text,
/// condensed (white space at its ends dropped, every inner run one space).
fn text(e: &Element) -> Option<String> {
    e.raw_value().map(normalize_whitespace)
}

/// The value of `parent`'s first child named `name`.
fn first_text(parent: &Element, name: &str) -> Option<String> {
    parent.child(name).and_then(text)
}

/// [`first_text`] or the empty text.
fn first_or_empty(parent: &Element, name: &str) -> String {
    first_text(parent, name).unwrap_or_default()
}

/// The first child `name`'s value as C `atoi` reads it, or `default`.
fn first_int(parent: &Element, name: &str, default: i32) -> i32 {
    first_text(parent, name).map_or(default, |t| value::int(&t) as i32)
}

/// The first child `name`'s value as C `atof` reads it, or `default`.
fn first_float(parent: &Element, name: &str, default: f32) -> f32 {
    first_text(parent, name).map_or(default, |t| value::float(&t))
}

/// Lower-casing of `class` and `community` texts (4, 5.5).
// TODO(#1006): which letters lower-casing changes is a review gap; the
// simplest reading, ASCII `A`–`Z` only, is taken.
fn lower(text: &str) -> String {
    text.to_ascii_lowercase()
}

/// A profile (section 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    /// The specific character this profile always gives; when it has a
    /// value nothing else is read.
    pub specific_character: Option<String>,
    /// Lower-cased; read only without `specific_character`.
    pub class: Option<String>,
    /// Read only without `specific_character`; [`NOT_SET`] by default.
    pub rank: i32,
    /// Read only without `specific_character`; [`NOT_SET`] by default.
    pub reputation: i32,
}

impl Profile {
    /// The profile of entry `e` (a registered `character`).
    pub fn read(e: &Element) -> Self {
        let id = e.attr("id").unwrap_or_default().to_owned();
        match first_text(e, "specific_character") {
            Some(specific) => Self {
                id,
                specific_character: Some(specific),
                class: None,
                rank: NOT_SET,
                reputation: NOT_SET,
            },
            // TODO(#1006): without `class` either, what the program keeps is
            // a review gap; the simplest reading, no class and no error, is
            // taken.
            None => Self {
                id,
                specific_character: None,
                class: first_text(e, "class").map(|c| lower(&c)),
                rank: first_int(e, "rank", NOT_SET),
                reputation: first_int(e, "reputation", NOT_SET),
            },
        }
    }
}

/// `money` (5.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Money {
    pub min: u32,
    /// Never lower than `min` (compared unsigned).
    pub max: u32,
    pub infinitive: bool,
}

impl Money {
    /// The first `money` child of `parent`, or min 0, max 0, infinitive off.
    fn read(parent: &Element) -> Self {
        let Some(e) = parent.child("money") else {
            return Self::default();
        };
        let int = |name| e.attr(name).map_or(0, |v| value::int(v) as i32);
        let min = int("min") as u32;
        let max = int("max") as u32;
        Self {
            min,
            max: max.max(min),
            infinitive: int("infinitive") != 0,
        }
    }
}

/// A specific character (section 5). [`Default`] is the empty value, not
/// the format's defaults (which [`Self::read`] applies).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpecificCharacter {
    pub id: String,
    /// On only when the attribute equals 1 (5.1).
    pub no_random: bool,
    /// On only when the attribute equals 1 (5.1).
    pub team_default: bool,
    /// The start dialog id, empty for none (section 6).
    pub start_dialog: String,
    /// The actor dialog ids, in document order (section 6).
    pub actor_dialogs: Vec<String>,
    pub icon: String,
    /// Kept as written: a string id or text (5.4).
    pub name: String,
    /// Translated when read (5.4).
    pub bio: String,
    pub panic_threshold: f32,
    pub hit_probability_factor: f32,
    pub crouch_type: i32,
    pub critical_wound_weights: String,
    pub visual: String,
    /// Each `\n` replaced by a line feed (5.3).
    pub supplies: String,
    pub npc_config: String,
    pub snd_config: String,
    pub terrain_sect: String,
    /// Lower-cased, one per `class` child (5.2).
    pub classes: Vec<String>,
    /// Lower-cased, one of the communities (5.5).
    pub community: String,
    pub rank: i32,
    pub reputation: i32,
    pub money: Money,
}

impl SpecificCharacter {
    /// The specific character of entry `e` (a registered
    /// `specific_character`): `bio` translated through `strings`, the
    /// community checked with `is_community` (a name of
    /// `[game_relations] communities`). The fatal errors of 5.1 and 5.5.
    pub fn read(
        e: &Element,
        strings: &StringTable,
        is_community: &dyn Fn(&str) -> bool,
    ) -> io::Result<Self> {
        let id = e.attr("id").unwrap_or_default().to_owned();
        let flag = |name| e.attr(name).is_some_and(|v| value::int(v) == 1);
        let (no_random, team_default) = (flag("no_random"), flag("team_default"));
        if no_random && team_default {
            return Err(failed(
                "cannot set 'no_random' and 'team_default' flags simultaneously, profile id",
                Some(&id),
            ));
        }
        let start_dialog = first_or_empty(e, "start_dialog");
        let actor_dialogs = e
            .children_named("actor_dialog")
            .map(|d| text(d).unwrap_or_default())
            .collect();
        let icon = first_text(e, "icon").unwrap_or_else(|| DEFAULT_ICON.to_owned());
        let name = first_or_empty(e, "name");
        let bio = strings.get(&first_or_empty(e, "bio")).to_owned();
        let panic_threshold = first_float(e, "panic_threshold", 0.0);
        let hit_probability_factor = first_float(e, "hit_probability_factor", 1.0);
        let crouch_type = first_int(e, "crouch_type", 0);
        let critical_wound_weights = first_text(e, "critical_wound_weights")
            .unwrap_or_else(|| DEFAULT_CRITICAL_WOUND_WEIGHTS.to_owned());
        let visual = first_or_empty(e, "visual");
        let supplies = first_or_empty(e, "supplies").replace("\\n", "\n");
        let npc_config = first_or_empty(e, "npc_config");
        let snd_config = first_or_empty(e, "snd_config");
        let terrain_sect = first_or_empty(e, "terrain_sect");
        // Each `class` child adds the first one's value (5.2).
        let first_class = lower(&first_or_empty(e, "class"));
        let classes = vec![first_class; e.children_named("class").count()];
        let Some(community) = first_text(e, "community") else {
            return Err(failed(
                "'community' field not fulfiled for specific character",
                Some(&id),
            ));
        };
        let community = lower(&community);
        if !is_community(&community) {
            return Err(failed("item not found, id", Some(&community)));
        }
        let required = |name: &str| {
            let v = first_int(e, name, NOT_SET);
            if v == NOT_SET {
                return Err(failed(
                    format!("'{name}' field not fulfiled for specific character"),
                    Some(&id),
                ));
            }
            Ok(v)
        };
        let rank = required("rank")?;
        let reputation = required("reputation")?;
        let money = Money::read(e);
        Ok(Self {
            id,
            no_random,
            team_default,
            start_dialog,
            actor_dialogs,
            icon,
            name,
            bio,
            panic_threshold,
            hit_probability_factor,
            crouch_type,
            critical_wound_weights,
            visual,
            supplies,
            npc_config,
            snd_config,
            terrain_sect,
            classes,
            community,
            rank,
            reputation,
            money,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::{parse, preprocess};

    fn doc(text: &str) -> Document {
        parse(text.as_bytes()).unwrap()
    }

    fn check(e: io::Error) -> FailedCheck {
        FailedCheck::of(&e).expect("a failed check").clone()
    }

    fn strings() -> StringTable {
        StringTable::new([("escape_trader_bio".to_owned(), "Bio text.".to_owned())])
    }

    fn communities(name: &str) -> bool {
        ["trader", "stalker", "actor"].contains(&name)
    }

    /// A specific character `id` with the required fields and `body`.
    fn specific(attrs: &str, body: &str) -> io::Result<SpecificCharacter> {
        let d = doc(&format!(
            "<xml><specific_character id=\"x\" {attrs}>{body}</specific_character></xml>"
        ));
        let e = entry_element(&d, SPECIFIC_CHARACTER, 0).unwrap();
        SpecificCharacter::read(e, &strings(), &communities)
    }

    const REQUIRED: &str =
        "<community>stalker</community><rank>10</rank><reputation>-5</reputation>";

    // Section 1.

    #[test]
    fn list_items_are_trimmed_and_named_under_gameplay() {
        assert_eq!(list(Some(" npc_profile")), ["npc_profile"]);
        assert_eq!(list(Some("a, b,c")), ["a", "b", "c"]);
        assert!(list(None).is_empty());
        assert_eq!(file_path("npc_profile"), "gameplay\\npc_profile.xml");
        let c = check(cannot_open("character_desc_bar"));
        assert_eq!(c.description, "error while parsing XML file");
        assert_eq!(c.argument.as_deref(), Some("character_desc_bar.xml"));
    }

    // Section 2.

    #[test]
    fn element_text_is_condensed_and_attributes_are_not() {
        let c = specific(
            "",
            &format!("{REQUIRED}<name>\n\t a \t\n  b </name><icon x=\" 1\"></icon>"),
        )
        .unwrap();
        assert_eq!(c.name, "a b");
        // An empty element has no value: the default applies.
        assert_eq!(c.icon, DEFAULT_ICON);
    }

    #[test]
    fn an_indented_include_line_stays_text() {
        let text = b"<xml><specific_character id=\"x\">\n<supplies>[spawn] \\n\n\t#include \"f.xml\"\n</supplies>\n#include \"c.xml\"\n<community>stalker</community><rank>1</rank><reputation>1</reputation></specific_character></xml>";
        let expanded = preprocess(text, &mut |name| {
            assert_eq!(name, b"c.xml");
            Ok(b"<start_dialog>dm_hello_dialog</start_dialog>".to_vec())
        })
        .unwrap();
        let d = parse(&expanded).unwrap();
        let e = entry_element(&d, SPECIFIC_CHARACTER, 0).unwrap();
        let c = SpecificCharacter::read(e, &strings(), &communities).unwrap();
        assert_eq!(c.supplies, "[spawn] \n #include \"f.xml\"");
        assert_eq!(c.start_dialog, "dm_hello_dialog");
    }

    #[test]
    fn only_entries_of_the_root_count() {
        let d =
            doc("<xml>;--- text<other id=\"o\"/><character id=\"a\"/><character id=\"b\"/></xml>");
        let mut r = Registry::new();
        r.register("npc_profile", &d, PROFILE).unwrap();
        assert_eq!((r.len(), r.id(0), r.id(1)), (2, "a", "b"));
        assert_eq!(entry_element(&d, PROFILE, 1).unwrap().attr("id"), Some("b"));
    }

    // Section 3.

    #[test]
    fn ids_are_registered_across_a_list_in_order() {
        let mut r = Registry::new();
        r.register("f0", &doc("<xml><c id=\"b\"/><c id=\"B\"/></xml>"), "c")
            .unwrap();
        r.register("empty", &doc("<xml></xml>"), "c").unwrap();
        r.register("f1", &doc("<xml><c id=\"a\"/></xml>"), "c")
            .unwrap();
        assert_eq!(r.files(), ["f0", "empty", "f1"]);
        assert_eq!(
            r.find("a").unwrap(),
            EntryRef {
                index: 2,
                file: 2,
                position: 0
            }
        );
        assert_eq!(r.find("B").unwrap().index, 1);
        assert_eq!(r.entry(1).position, 1);
    }

    #[test]
    fn an_entry_without_id_is_fatal() {
        let mut r = Registry::new();
        let d = doc("<xml><c id=\"a\"/><c/></xml>");
        let c = check(r.register("npc_profile", &d, "c").unwrap_err());
        assert_eq!(
            c.description,
            "id for item don't set, number 1 in npc_profile"
        );
        assert_eq!(c.argument, None);
    }

    #[test]
    fn a_duplicate_id_is_fatal_across_files() {
        let mut r = Registry::new();
        let d = doc("<xml><c id=\"a\"/></xml>");
        r.register("f", &d, "c").unwrap();
        let c = check(r.register("f", &d, "c").unwrap_err());
        assert_eq!(
            (c.description.as_str(), c.argument.as_deref()),
            ("duplicate item id", Some("a"))
        );
    }

    #[test]
    fn an_unknown_id_logs_every_id_then_is_fatal() {
        let mut r = Registry::new();
        r.register("f", &doc("<xml><c id=\"b\"/><c id=\"a\"/></xml>"), "c")
            .unwrap();
        // Looked up exactly.
        let c = check(r.find("A").unwrap_err());
        assert_eq!(c.description, "item not found, id");
        assert_eq!(c.argument.as_deref(), Some("A"));
        assert_eq!(c.log, ["[0]=[b]", "[1]=[a]"]);
        assert!(FailedCheck::of(&r.find("").unwrap_err()).is_some());
    }

    // Section 4.

    #[test]
    fn a_profile_with_a_specific_character_reads_nothing_else() {
        let d = doc("<xml><character id=\"escape_trader\"><class>Trader</class>\
             <specific_character>escape_trader</specific_character><rank>5</rank></character></xml>");
        let p = Profile::read(entry_element(&d, PROFILE, 0).unwrap());
        assert_eq!(
            p,
            Profile {
                id: "escape_trader".into(),
                specific_character: Some("escape_trader".into()),
                class: None,
                rank: NOT_SET,
                reputation: NOT_SET,
            }
        );
    }

    #[test]
    fn a_profile_without_one_reads_its_class_lower_cased() {
        let d = doc(
            "<xml><character id=\"p\"><class>Esc_Novice</class><rank>3</rank>\
             <specific_character></specific_character></character>\
             <character id=\"q\"/></xml>",
        );
        let p = Profile::read(entry_element(&d, PROFILE, 0).unwrap());
        assert_eq!(p.specific_character, None);
        assert_eq!(p.class.as_deref(), Some("esc_novice"));
        assert_eq!((p.rank, p.reputation), (3, NOT_SET));
        let q = Profile::read(entry_element(&d, PROFILE, 1).unwrap());
        assert_eq!(q.class, None);
    }

    // Section 5.1.

    #[test]
    fn flags_are_on_only_at_one() {
        let c = specific("no_random = \"1\" team_default=\"3\"", REQUIRED).unwrap();
        assert!(c.no_random && !c.team_default);
        let c = specific("team_default=\"1\"", REQUIRED).unwrap();
        assert!(!c.no_random && c.team_default);
        let c = check(specific("no_random=\"1\" team_default=\"1\"", REQUIRED).unwrap_err());
        assert_eq!(
            c.description,
            "cannot set 'no_random' and 'team_default' flags simultaneously, profile id"
        );
        assert_eq!(c.argument.as_deref(), Some("x"));
    }

    // Section 5.2.

    #[test]
    fn defaults_apply_to_missing_elements() {
        let c = specific("", REQUIRED).unwrap();
        assert_eq!(c.icon, DEFAULT_ICON);
        assert_eq!(c.critical_wound_weights, "1");
        assert_eq!(
            (c.panic_threshold, c.hit_probability_factor, c.crouch_type),
            (0.0, 1.0, 0)
        );
        assert!(c.start_dialog.is_empty() && c.actor_dialogs.is_empty());
        assert!(c.name.is_empty() && c.bio.is_empty() && c.visual.is_empty());
        assert!(c.supplies.is_empty() && c.npc_config.is_empty());
        assert!(c.snd_config.is_empty() && c.terrain_sect.is_empty());
        assert!(c.classes.is_empty());
        assert_eq!(c.money, Money::default());
        assert_eq!(
            (c.community.as_str(), c.rank, c.reputation),
            ("stalker", 10, -5)
        );
    }

    #[test]
    fn first_children_are_read_and_later_ones_ignored() {
        let c = specific(
            "",
            &format!(
                "{REQUIRED}<terrain_sect>a</terrain_sect><terrain_sect>b</terrain_sect>\
                 <panic_threshold>0.5</panic_threshold><panic_treshold>9</panic_treshold>\
                 <hit_probability_factor>1.5</hit_probability_factor><crouch_type>-1</crouch_type>\
                 <critical_wound_weights>75,25,25</critical_wound_weights>\
                 <critical_wound_weights>55,30,15</critical_wound_weights>\
                 <snd_config>characters_voice\\human_01\\ military\\</snd_config>\
                 <npc_config>n</npc_config><team>t</team><map_icon x=\"1\" y=\"2\"></map_icon>"
            ),
        )
        .unwrap();
        assert_eq!(c.terrain_sect, "a");
        assert_eq!(
            (c.panic_threshold, c.hit_probability_factor, c.crouch_type),
            (0.5, 1.5, -1)
        );
        assert_eq!(c.critical_wound_weights, "75,25,25");
        assert_eq!(c.snd_config, "characters_voice\\human_01\\ military\\");
        assert_eq!(c.npc_config, "n");
    }

    #[test]
    fn each_class_child_adds_the_first_class() {
        let c = specific(
            "",
            &format!("{REQUIRED}<class>Stalker</class><class>bandit</class>"),
        )
        .unwrap();
        assert_eq!(c.classes, ["stalker", "stalker"]);
        let c = specific("", &format!("{REQUIRED}<class/>")).unwrap();
        assert_eq!(c.classes, [""]);
    }

    // Section 5.3.

    #[test]
    fn supplies_replace_every_backslash_n() {
        let c = specific(
            "",
            &format!("{REQUIRED}<supplies>\n\t\t[spawn] \\n\n\t\twpn_knife \\n\n\t</supplies>"),
        )
        .unwrap();
        assert_eq!(c.supplies, "[spawn] \n wpn_knife \n");
    }

    // Section 5.4.

    #[test]
    fn name_is_kept_and_bio_translated() {
        let c = specific(
            "",
            &format!("{REQUIRED}<name>escape_trader_name</name><bio>escape_trader_bio</bio>"),
        )
        .unwrap();
        assert_eq!(
            (c.name.as_str(), c.bio.as_str()),
            ("escape_trader_name", "Bio text.")
        );
        let c = specific("", &format!("{REQUIRED}<bio>yan_kruglov_bio</bio>")).unwrap();
        assert_eq!(c.bio, "yan_kruglov_bio");
        let c = specific("", &format!("{REQUIRED}<bio/>")).unwrap();
        assert_eq!(c.bio, "");
    }

    // Section 5.5.

    #[test]
    fn community_is_lower_cased_and_must_be_listed() {
        let c = specific(
            "",
            "<community>Trader</community><rank>00</rank><reputation>0</reputation>",
        )
        .unwrap();
        assert_eq!((c.community.as_str(), c.rank), ("trader", 0));
        let c = check(
            specific(
                "",
                "<community>Dolg</community><rank>1</rank><reputation>1</reputation>",
            )
            .unwrap_err(),
        );
        assert_eq!(
            (c.description.as_str(), c.argument.as_deref()),
            ("item not found, id", Some("dolg"))
        );
        let c = check(
            specific("", "<community/><rank>1</rank><reputation>1</reputation>").unwrap_err(),
        );
        assert_eq!(
            c.description,
            "'community' field not fulfiled for specific character"
        );
        assert_eq!(c.argument.as_deref(), Some("x"));
    }

    #[test]
    fn rank_and_reputation_are_required_in_order() {
        let c = check(specific("", "<community>actor</community>").unwrap_err());
        assert_eq!(
            c.description,
            "'rank' field not fulfiled for specific character"
        );
        let c = check(
            specific(
                "",
                "<community>actor</community><rank>1</rank><reputation>-2147483647</reputation>",
            )
            .unwrap_err(),
        );
        assert_eq!(
            c.description,
            "'reputation' field not fulfiled for specific character"
        );
        // Not a number reads as `atoi` does, and passes.
        let c = specific(
            "",
            "<community>actor</community><rank>abc</rank><reputation>7x</reputation>",
        )
        .unwrap();
        assert_eq!((c.rank, c.reputation), (0, 7));
    }

    // Section 5.6.

    #[test]
    fn money_reads_the_first_elements_attributes() {
        let c = specific(
            "",
            &format!(
                "{REQUIRED}<money min=\"100000\" max=\"100000\" infinitive=\"1\">5</money>\
                 <money min=\"1\" max=\"2\" infinitive=\"0\"/>"
            ),
        )
        .unwrap();
        assert_eq!(
            c.money,
            Money {
                min: 100_000,
                max: 100_000,
                infinitive: true
            }
        );
        let c = specific(
            "",
            &format!("{REQUIRED}<money min=\"600\" max=\"200\" infinitive=\"2\"/>"),
        )
        .unwrap();
        assert_eq!(
            (c.money.min, c.money.max, c.money.infinitive),
            (600, 600, true)
        );
        // Compared unsigned: −1 is above any other.
        let c = specific("", &format!("{REQUIRED}<money min=\"-1\" max=\"5\"/>")).unwrap();
        assert_eq!((c.money.min, c.money.max), (u32::MAX, u32::MAX));
        let c = specific("", &format!("{REQUIRED}<money max=\"5\"/>")).unwrap();
        assert_eq!(
            (c.money.min, c.money.max, c.money.infinitive),
            (0, 5, false)
        );
    }

    // Section 6.

    #[test]
    fn dialog_ids_are_kept_as_text() {
        let c = specific(
            "",
            &format!(
                "{REQUIRED}<start_dialog>gar_wounded_bandit</start_dialog><start_dialog>dm_hello_dialog</start_dialog>\
                 <actor_dialog>a</actor_dialog><actor_dialog></actor_dialog><actor_dialog>b</actor_dialog>"
            ),
        )
        .unwrap();
        assert_eq!(c.start_dialog, "gar_wounded_bandit");
        assert_eq!(c.actor_dialogs, ["a", "", "b"]);
    }
}
