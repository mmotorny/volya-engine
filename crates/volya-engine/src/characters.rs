// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The character descriptions as the program reads them
//! (docs/formats/character-descriptions.md, cited by section;
//! [`volya_engine_formats::character_descriptions`] holds the format): the
//! files of `[profiles]`, opened and their ids registered when the game data
//! is read at startup ([`CharacterFiles`], sections 1, 3.1), and the entries
//! loaded by id ([`Characters`], 3.2).
//!
//! The original keeps each file's document open for the run; we keep only
//! the ids and where their entries are, and a [`Characters`] parses a file
//! again when it first loads one of its entries (a leaner structure with the
//! same behaviour, docs/architecture/memory.md rule 5: the files do not
//! change while the program runs). An entry's contents are read once per
//! [`Characters`]; reading the same entry again gives the same value, so
//! the original's sharing of the first read is not observable.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io;

use crate::core::character;
use crate::core::character_identity::Descriptions;
use crate::core::config::{self, Config, PathAliases};
use crate::core::vfs::{self, Vfs};
use crate::data::GameData;
use volya_engine_formats::character_descriptions::{
    self as format, PROFILE, Profile, Registry, SPECIFIC_CHARACTER, SpecificCharacter,
};
use volya_engine_formats::cp1251;
use volya_engine_formats::object::TraderLike;
use volya_engine_formats::xml::{self, Document};

/// The registered ids of both lists (3.1).
#[derive(Clone, Debug, Default)]
pub struct CharacterFiles {
    /// From `[profiles] files`.
    pub profiles: Registry,
    /// From `[profiles] specific_characters_files`.
    pub specific_characters: Registry,
}

/// List item `item`'s document (sections 1, 2): its include lines expanded
/// and parsed. A file that cannot be opened is a fatal error.
fn document(tree: &dyn Vfs, aliases: &PathAliases, item: &str) -> io::Result<Document> {
    let path = aliases.path("$game_config$", &format::file_path(item))?;
    let text = match vfs::read(tree, &path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(format::cannot_open(item)),
        Err(e) => return Err(e),
    };
    let text = xml::preprocess(&text, &mut |include| {
        let path = aliases.path("$game_config$", &cp1251::decode(include))?;
        vfs::read(tree, &path)
    })?;
    xml::parse(&text).map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))
}

/// The items of `[profiles] key`. A missing section or key gives none (our
/// choice: the spec reads both from every edition's `system.ltx`).
fn items(system: &Config, key: &str) -> Vec<String> {
    format::list(system.get("profiles", key).flatten())
}

impl CharacterFiles {
    /// Opens the files of both lists and registers their ids, the profiles
    /// first (sections 1, 3.1): the fatal errors of a file that cannot be
    /// opened, an entry without id and a duplicate id.
    pub fn register(tree: &dyn Vfs, aliases: &PathAliases, system: &Config) -> io::Result<Self> {
        let mut files = Self::default();
        for (registry, key, entry) in [
            (&mut files.profiles, "files", PROFILE),
            (
                &mut files.specific_characters,
                "specific_characters_files",
                SPECIFIC_CHARACTER,
            ),
        ] {
            for item in items(system, key) {
                registry.register(&item, &document(tree, aliases, &item)?, entry)?;
            }
        }
        Ok(files)
    }
}

/// `[game_relations] communities` (5.5; player-state.md 2.2): from the
/// system configuration, or `config\creatures\game_relations.ltx` when it
/// does not hold the section.
pub fn communities(tree: &dyn Vfs, data: &GameData) -> io::Result<Vec<(String, i32)>> {
    let relations = match data.system.section("game_relations") {
        Some(_) => data.system.clone(),
        None => config::load(
            tree,
            &data
                .aliases
                .path("$game_config$", "creatures\\game_relations.ltx")?,
        )?,
    };
    Ok(character::communities(
        relations
            .get("game_relations", "communities")
            .flatten()
            .unwrap_or(""),
    ))
}

/// The two lists, as the documents' keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum List {
    Profiles,
    SpecificCharacters,
}

/// Entries loaded by id or registration index (3.2), each read once.
pub struct Characters<'a> {
    tree: &'a dyn Vfs,
    data: &'a GameData,
    documents: HashMap<(List, usize), Document>,
    communities: Option<Vec<String>>,
    profiles: HashMap<usize, Profile>,
    specific_characters: HashMap<usize, SpecificCharacter>,
}

impl<'a> Characters<'a> {
    pub fn new(tree: &'a dyn Vfs, data: &'a GameData) -> Self {
        Self {
            tree,
            data,
            documents: HashMap::new(),
            communities: None,
            profiles: HashMap::new(),
            specific_characters: HashMap::new(),
        }
    }

    fn registry(&self, list: List) -> &'a Registry {
        let files = &self.data.characters;
        match list {
            List::Profiles => &files.profiles,
            List::SpecificCharacters => &files.specific_characters,
        }
    }

    /// The entry element of registration index `index` of `list` (3.2),
    /// its file parsed into `documents` once.
    fn element<'d>(
        &self,
        documents: &'d mut HashMap<(List, usize), Document>,
        list: List,
        index: usize,
    ) -> io::Result<&'d xml::Element> {
        let registry = self.registry(list);
        let at = registry.entry(index);
        let item = &registry.files()[at.file];
        let doc = match documents.entry((list, at.file)) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(document(self.tree, &self.data.aliases, item)?),
        };
        let entry = match list {
            List::Profiles => PROFILE,
            List::SpecificCharacters => SPECIFIC_CHARACTER,
        };
        format::entry_element(doc, entry, at.position).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{item}: entry {} changed since startup", at.position),
            )
        })
    }

    /// The profile `id` (sections 3.2, 4).
    pub fn profile(&mut self, id: &str) -> io::Result<&Profile> {
        let index = self.registry(List::Profiles).find(id)?.index;
        if !self.profiles.contains_key(&index) {
            let mut documents = std::mem::take(&mut self.documents);
            let read = self
                .element(&mut documents, List::Profiles, index)
                .map(Profile::read);
            self.documents = documents;
            self.profiles.insert(index, read?);
        }
        Ok(&self.profiles[&index])
    }

    /// The number of specific characters; the last registration index is
    /// one less (3.1).
    pub fn specific_character_count(&self) -> usize {
        self.registry(List::SpecificCharacters).len()
    }

    /// The specific character `id` (sections 3.2, 5).
    pub fn specific_character(&mut self, id: &str) -> io::Result<&SpecificCharacter> {
        let index = self.registry(List::SpecificCharacters).find(id)?.index;
        self.specific_character_at(index)
    }

    /// The specific character of registration index `index` (3.1, 5).
    pub fn specific_character_at(&mut self, index: usize) -> io::Result<&SpecificCharacter> {
        if !self.specific_characters.contains_key(&index) {
            if self.communities.is_none() {
                let names = communities(self.tree, self.data)?;
                self.communities = Some(names.into_iter().map(|(n, _)| n).collect());
            }
            let mut documents = std::mem::take(&mut self.documents);
            let names = self.communities.as_deref().unwrap_or_default();
            let read = self
                .element(&mut documents, List::SpecificCharacters, index)
                .and_then(|e| {
                    SpecificCharacter::read(e, &self.data.strings, &|c| {
                        names.iter().any(|n| n == c)
                    })
                });
            self.documents = documents;
            self.specific_characters.insert(index, read?);
        }
        Ok(&self.specific_characters[&index])
    }

    /// The player's Trader-like block's specific character
    /// (player-state.md 2.2): the block's own if named, else its profile's;
    /// none for a profile that names none. A stalker's or trader's is
    /// chosen by docs/behavior/character-identity.md
    /// ([`crate::core::character_identity`]); the player's is outside it
    /// (section 7).
    pub fn for_trader(&mut self, trader: &TraderLike) -> io::Result<Option<&SpecificCharacter>> {
        let id = if trader.specific_character.is_empty() {
            match &self.profile(&trader.character_profile)?.specific_character {
                Some(id) => id.clone(),
                None => return Ok(None),
            }
        } else {
            trader.specific_character.to_string()
        };
        self.specific_character(&id).map(Some)
    }
}

/// The descriptions of docs/behavior/character-identity.md 1.2: CH-1's
/// profiles and specific characters, by id and registration index.
impl Descriptions for Characters<'_> {
    fn profile(&mut self, id: &str) -> io::Result<&Profile> {
        Characters::profile(self, id)
    }

    fn count(&self) -> usize {
        self.specific_character_count()
    }

    fn at(&mut self, index: usize) -> io::Result<&SpecificCharacter> {
        self.specific_character_at(index)
    }

    fn find(&mut self, id: &str) -> io::Result<&SpecificCharacter> {
        self.specific_character(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use volya_engine_formats::character_descriptions::FailedCheck;
    use volya_engine_formats::ltx;

    const PROFILES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xml>\n;---\n\
        <character id=\"trader\"><class>Trader</class>\
        <specific_character>trader</specific_character></character>\n\
        <character id=\"novice\"><class>Novice</class></character>\n</xml>";
    const SPECIFIC: &str = "<xml>\n<specific_character id=\"trader\" no_random = \"1\">\n\
        <name>trader_name</name><bio>trader_bio</bio>\n\
        #include \"gameplay\\dialogs.xml\"\n\
        <community>Trader</community><rank>330</rank><reputation>23</reputation>\n\
        </specific_character>\n</xml>";
    const SYSTEM: &str = "[profiles]\nfiles =  npc_profile\nspecific_characters_files = a,b\n\
        [game_relations]\ncommunities = stalker, 0, trader, 1\n";

    /// The fixture's tree with `files` in `config\gameplay\`, and its data
    /// with configuration `system` and its character files registered.
    fn game(system: &str, files: &[(&str, &str)]) -> (fixture::MemVfs, io::Result<GameData>) {
        let mut tree = fixture::tree();
        for (name, text) in files {
            tree.insert(&format!("gamedata/config/gameplay/{name}"), *text);
        }
        let mut data = fixture::data(&tree);
        data.system = ltx::parse(system.as_bytes()).unwrap();
        let registered = CharacterFiles::register(&tree, &data.aliases, &data.system);
        let data = registered.map(|c| {
            data.characters = c;
            data
        });
        (tree, data)
    }

    /// A Trader-like block of profile `profile` and no specific character.
    fn trader(profile: &str) -> TraderLike {
        TraderLike {
            money: 0,
            specific_character: String::new().into(),
            trader_flags: 0,
            character_profile: profile.into(),
            community: -1,
            rank: format::NOT_SET,
            reputation: format::NOT_SET,
            character_name: String::new().into(),
        }
    }

    fn check(e: &io::Error) -> &FailedCheck {
        FailedCheck::of(e).expect("a failed check")
    }

    #[test]
    fn the_lists_files_are_registered_and_entries_loaded() {
        let (tree, data) = game(
            SYSTEM,
            &[
                ("npc_profile.xml", PROFILES),
                ("a.xml", SPECIFIC),
                ("b.xml", "<xml></xml>"),
                ("dialogs.xml", "<start_dialog>hello</start_dialog>"),
            ],
        );
        let data = data.unwrap();
        assert_eq!(data.characters.profiles.len(), 2);
        assert_eq!(data.characters.specific_characters.files(), ["a", "b"]);
        let mut characters = Characters::new(&tree, &data);
        assert_eq!(characters.specific_character_count(), 1);
        let novice = characters.profile("novice").unwrap();
        assert_eq!(novice.class.as_deref(), Some("novice"));
        let c = characters.for_trader(&trader("trader")).unwrap().unwrap();
        assert_eq!(
            (c.id.as_str(), c.community.as_str(), c.rank),
            ("trader", "trader", 330)
        );
        assert!(c.no_random);
        assert_eq!(c.start_dialog, "hello");
        // A profile without its own specific character gives none here.
        assert!(characters.for_trader(&trader("novice")).unwrap().is_none());
    }

    #[test]
    fn a_listed_file_that_cannot_be_opened_is_fatal() {
        let (_, data) = game(
            SYSTEM,
            &[
                ("npc_profile.xml", PROFILES),
                ("a.xml", SPECIFIC),
                ("dialogs.xml", ""),
            ],
        );
        let e = data.err().unwrap();
        assert_eq!(check(&e).description, "error while parsing XML file");
        assert_eq!(check(&e).argument.as_deref(), Some("b.xml"));
    }

    #[test]
    fn a_file_listed_twice_is_a_duplicate() {
        let system = "[profiles]\nfiles = npc_profile, npc_profile\n";
        let (_, data) = game(system, &[("npc_profile.xml", PROFILES)]);
        let e = data.err().unwrap();
        assert_eq!(check(&e).description, "duplicate item id");
        assert_eq!(check(&e).argument.as_deref(), Some("trader"));
    }

    #[test]
    fn an_undefined_id_and_an_unlisted_community_are_fatal() {
        let system = "[profiles]\nfiles = npc_profile\nspecific_characters_files = a\n\
            [game_relations]\ncommunities = stalker, 0\n";
        let (tree, data) = game(
            system,
            &[
                ("npc_profile.xml", PROFILES),
                ("a.xml", SPECIFIC),
                ("dialogs.xml", ""),
            ],
        );
        let data = data.unwrap();
        let mut characters = Characters::new(&tree, &data);
        let e = characters.profile("Trader").unwrap_err();
        assert_eq!(check(&e).log, ["[0]=[trader]", "[1]=[novice]"]);
        let e = characters.specific_character("trader").unwrap_err();
        assert_eq!(check(&e).description, "item not found, id");
        assert_eq!(check(&e).argument.as_deref(), Some("trader"));
    }

    #[test]
    fn without_the_lists_nothing_is_read() {
        let (_, data) = game("[other]\n", &[]);
        let data = data.unwrap();
        assert!(data.characters.profiles.is_empty());
        assert!(data.characters.specific_characters.files().is_empty());
    }
}
