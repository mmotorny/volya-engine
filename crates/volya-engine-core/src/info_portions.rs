// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Information portions (docs/behavior/info-portions.md, cited by section):
//! the story flags the game keeps per holder. [`Declarations`] reads the
//! files of `[info_portions]` (1), with the indexes of articles and dialogs
//! the same start step reads and the task ids a portion's `task` children
//! name; [`InfoPortions`] is the engine's state: each holder's list of
//! portions with the times it received them (2, `sav` 9 part 1), the
//! player character's articles and tasks the portions give (3.4), giving and
//! disabling by transfers (3, 4), the messages that return from the server
//! (3.1), documents' portions (7.4) and the player character's dialog list
//! (6).
//!
//! Holders are named by object id; the caller gives and disables only on
//! holders (5: on any other object the script members do nothing). The
//! script part is reported, in order, as [`Event`]s: the `action`
//! functions (3.3), the callbacks `article_info`, `task_state` and
//! `inventory_info` (3.4) and the dialogue window's rebuild mark (3.4 step
//! 4, 4.2); script API P's story state (`volya-engine-script`'s `story`)
//! calls the functions and binds the members of 5 to this API.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::sync::Arc;

use volya_engine_formats::cp1251;
use volya_engine_formats::dialog_files::DIALOG;
use volya_engine_formats::ltx::Config;
use volya_engine_formats::ui::int_attr;
use volya_engine_formats::xml::{self, Document, Element};

use crate::config::PathAliases;
use crate::vfs::{self, Vfs};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

const GAME_CONFIG: &str = "$game_config$";

/// The file the tasks are created from (3.4 step 2): `gameplay\<name>.xml`.
pub const TASKS_FILE: &str = "game_tasks";

/// An index of ids (1.2): every declared id in file order and position
/// order, compared exactly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    ids: Vec<String>,
    positions: HashMap<String, usize>,
}

impl Index {
    /// Adds `id`; an id declared twice stops the game (1.2).
    fn insert(&mut self, id: &str) -> io::Result<usize> {
        if self.positions.contains_key(id) {
            return Err(invalid(format!("duplicate item id {id}")));
        }
        let position = self.ids.len();
        self.ids.push(id.to_owned());
        self.positions.insert(id.to_owned(), position);
        Ok(position)
    }

    /// The ids in index order.
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Whether `id` is declared.
    pub fn contains(&self, id: &str) -> bool {
        self.positions.contains_key(id)
    }

    /// The position of the declared `id`, without the failure of
    /// [`Self::look_up`].
    pub fn position(&self, id: &str) -> Option<usize> {
        self.positions.get(id).copied()
    }

    /// Looks `id` up (1.2): its position in the index. An id that is not
    /// declared stops the game: first every declared id goes to `log` as
    /// one line `[<index>]=[<id>]`, then the error "item not found, id"
    /// with the id.
    pub fn look_up(&self, id: &str, log: &mut Vec<String>) -> io::Result<usize> {
        match self.position(id) {
            Some(position) => Ok(position),
            None => {
                log.extend(
                    (self.ids.iter().enumerate()).map(|(index, id)| format!("[{index}]=[{id}]")),
                );
                Err(invalid(format!("item not found, id {id}")))
            }
        }
    }

    /// The `element` children of each file's root, in file and document
    /// order, registered by their attribute `id` (1.1, 1.2): one without it
    /// stops the game, naming its 0-based position among the file's
    /// elements and the file.
    fn read<'a>(
        files: &'a [(String, Document)],
        element: &'static str,
    ) -> io::Result<(Self, Vec<Entry<'a>>)> {
        let mut index = Self::default();
        let mut elements = Vec::new();
        for (file, (name, doc)) in files.iter().enumerate() {
            let Some(root) = doc.root() else {
                continue;
            };
            for (number, e) in root.children_named(element).enumerate() {
                let id = e.attr("id").ok_or_else(|| {
                    invalid(format!("id for item don't set, number {number} in {name}"))
                })?;
                index.insert(id)?;
                elements.push(Entry {
                    place: Place {
                        file,
                        position: number,
                    },
                    element: e,
                });
            }
        }
        Ok((index, elements))
    }
}

/// Where an indexed element is: its file's place in the list and its
/// position among that file's elements of its name (`dialog-files` 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub file: usize,
    pub position: usize,
}

/// An indexed element and its place.
struct Entry<'a> {
    place: Place,
    element: &'a Element,
}

/// A portion's children (1.3), each name's in file order. The children
/// `precondition`, `has_info` and `dont_has_info` are read by the original
/// and never used (3.3, believed: no effect), so they are not kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Portion {
    /// Dialog ids offered while the player character holds it (6).
    pub dialogs: Vec<String>,
    /// Portions disabled on the same holder when it is given (3.2 step 5).
    pub disables: Vec<String>,
    /// Script functions called when it is given (3.3 step 1).
    pub actions: Vec<String>,
    /// Portions given to and disabled on the player character (3.3 step 2).
    pub give_info: Vec<String>,
    pub disable_info: Vec<String>,
    /// Articles given to and removed from the player character (3.4 step 1).
    pub articles: Vec<String>,
    pub article_disables: Vec<String>,
    /// Tasks given to the player character (3.4 step 2).
    pub tasks: Vec<String>,
}

impl Portion {
    /// The children of `e` (1.3), each its text. What an empty child does
    /// is unestablished (1.3); it is taken as the empty id (the reader's
    /// default value, `layout` 1.4).
    fn read(e: &Element) -> Self {
        let texts = |name| {
            e.children_named(name)
                .map(|c| c.value().unwrap_or("").to_owned())
                .collect()
        };
        Self {
            dialogs: texts("dialog"),
            disables: texts("disable"),
            actions: texts("action"),
            give_info: texts("give_info"),
            disable_info: texts("disable_info"),
            articles: texts("article"),
            article_disables: texts("article_disable"),
            tasks: texts("task"),
        }
    }

    /// Whether it has none of the children that act (1.3: giving it only
    /// adds it to the list).
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

/// An article's declaration, as receiving it needs (3.4 step 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Article {
    /// The attribute `article_type`. TODO(#913): its meaning and the
    /// number `sav` 9 part 4 saves are PD-9's.
    pub article_type: Option<String>,
}

/// What the start step reads (1, `dialog-files` 1): the information
/// portions with their children, the articles (`[encyclopedia]`) and the
/// dialogs (`[dialogs]`), and the task ids of [`TASKS_FILE`].
///
/// The original reads a portion's children the first time its data is used
/// (1.2) and keeps them; here they are read with the index, which nothing
/// observes otherwise (every child is text, 1.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declarations {
    pub portions: Index,
    portion_data: Vec<Portion>,
    pub articles: Index,
    article_data: Vec<Article>,
    pub dialogs: Index,
    /// Each dialog's `priority` (`dialog-files` 4), by dialog index.
    dialog_priorities: Vec<i64>,
    /// The `[dialogs]` list items, in list order (`dialog-files` 1).
    dialog_files: Vec<String>,
    /// Where each dialog is (`dialog-files` 3.1), by dialog index.
    dialog_places: Vec<Place>,
    /// The `id`s of [`TASKS_FILE`]'s root's `game_task` children.
    tasks: HashSet<String>,
}

impl Declarations {
    /// Reads the lists `[info_portions]`, `[encyclopedia]` and `[dialogs]`
    /// `files` of `system` (1.1) and [`TASKS_FILE`] from `vfs`. A missing
    /// section or key stops the game (our reading: the original reads them
    /// unconditionally).
    // TODO(#995): for `[dialogs]`, what a missing section or key and an
    // empty item do are review gaps; an empty item is read as the file
    // `gameplay\.xml`, which cannot be opened.
    pub fn read(vfs: &dyn Vfs, aliases: &PathAliases, system: &Config) -> io::Result<Self> {
        let list = |section: &str| -> io::Result<Vec<(String, Document)>> {
            let files = system.read(section, "files")?;
            files
                .split(',')
                .map(str::trim)
                .map(|name| Ok((name.to_owned(), read_gameplay(vfs, aliases, name)?)))
                .collect()
        };
        let portions = list("info_portions")?;
        let articles = list("encyclopedia")?;
        let dialogs = list("dialogs")?;
        let tasks = read_gameplay(vfs, aliases, TASKS_FILE)?;
        Self::from_documents(&portions, &articles, &dialogs, &tasks)
    }

    /// The declarations of parsed files: each list's `(name, document)` in
    /// list order (1.1).
    pub fn from_documents(
        portions: &[(String, Document)],
        articles: &[(String, Document)],
        dialogs: &[(String, Document)],
        tasks: &Document,
    ) -> io::Result<Self> {
        let (portion_index, elements) = Index::read(portions, "info_portion")?;
        let portion_data = (elements.into_iter())
            .map(|e| Portion::read(e.element))
            .collect();
        let (article_index, elements) = Index::read(articles, "article")?;
        let article_data = (elements.into_iter())
            .map(|e| Article {
                article_type: e.element.attr("article_type").map(str::to_owned),
            })
            .collect();
        let (dialog_index, elements) = Index::read(dialogs, DIALOG)?;
        let dialog_priorities = (elements.iter())
            .map(|e| int_attr(e.element, "priority", 0))
            .collect();
        let dialog_places = elements.iter().map(|e| e.place).collect();
        let dialog_files = dialogs.iter().map(|(name, _)| name.clone()).collect();
        let tasks = (tasks.root().into_iter())
            .flat_map(|root| root.children_named("game_task"))
            .filter_map(|t| t.attr("id"))
            .map(str::to_owned)
            .collect();
        Ok(Self {
            portions: portion_index,
            portion_data,
            articles: article_index,
            article_data,
            dialogs: dialog_index,
            dialog_priorities,
            dialog_files,
            dialog_places,
            tasks,
        })
    }

    /// The `[dialogs]` list items, in list order (`dialog-files` 1).
    pub fn dialog_files(&self) -> &[String] {
        &self.dialog_files
    }

    /// Where the dialog of index `index` is (`dialog-files` 3.1).
    pub fn dialog_place(&self, index: usize) -> Place {
        self.dialog_places[index]
    }

    /// The declared portion `id`'s children.
    pub fn portion(&self, id: &str) -> Option<&Portion> {
        Some(&self.portion_data[self.portions.position(id)?])
    }

    /// Every declared portion with its children, in index order.
    pub fn each_portion(&self) -> impl Iterator<Item = (&str, &Portion)> {
        (self.portions.ids().iter().map(String::as_str)).zip(&self.portion_data)
    }

    /// The declared article `id`.
    pub fn article(&self, id: &str) -> Option<&Article> {
        Some(&self.article_data[self.articles.position(id)?])
    }

    /// Whether [`TASKS_FILE`] has a `game_task` with this `id`.
    pub fn has_task(&self, id: &str) -> bool {
        self.tasks.contains(id)
    }
}

/// The document `gameplay\<name>.xml` of `$game_config$`, read with
/// `layout` 1's reader, `#include` lines included (1.1). A file that cannot
/// be opened stops the game with "error while parsing XML file" and its
/// name; an include path is relative to `$game_config$`, as `layout` 1.2
/// reads it outside `ui\`.
pub(crate) fn read_gameplay(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    name: &str,
) -> io::Result<Document> {
    let file = format!("{name}.xml");
    let path = aliases.path(GAME_CONFIG, &format!("gameplay\\{file}"))?;
    let text = vfs::read(vfs, &path)
        .map_err(|_| invalid(format!("error while parsing XML file {file}")))?;
    let text = xml::preprocess(&text, &mut |include| {
        let include = aliases.path(GAME_CONFIG, &cp1251::decode(include))?;
        vfs::read(vfs, &include).map_err(|e| invalid(format!("include {include}: {e}")))
    })
    .map_err(|e| invalid(format!("{path}: {e}")))?;
    xml::parse(&text).map_err(|e| invalid(format!("{path}: {e}")))
}

/// An entry of a holder's list (2): the id and the game time it was given,
/// what `sav` 9 part 1 saves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    pub id: String,
    /// Game time, ms (`sav` 12).
    pub received: u64,
}

/// An article the player character knows (3.4 step 1, `sav` 9 part 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownArticle {
    pub received: u64,
    pub id: String,
    pub read: bool,
    /// TODO(#913): the type's number.
    pub article_type: Option<String>,
}

/// A task the player character was given (3.4 step 2). TODO(#910): the
/// task itself (`sav` 9.8) is PD-6's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GivenTask {
    pub id: String,
    pub received: u64,
}

/// What receiving and disabling ask of the script interface and the
/// dialogue window, in the order the original does it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Call the script function `function` (`namespace.function`) with the
    /// holder's script object and nil (3.3 step 1); an error it raises ends
    /// the program.
    Action { holder: u16, function: String },
    /// The player character's callback `article_info` (number 12) for a
    /// new article (3.4 step 1).
    ArticleInfo { article: String },
    /// The callback `task_state` (number 13) for a new task (3.4 step 2).
    TaskState { task: String },
    /// The callback `inventory_info` (number 11) with the player
    /// character's script object and the portion's id (3.4 step 3).
    InventoryInfo { portion: String },
    /// If the dialogue window is open, it is marked to rebuild its
    /// questions (3.4 step 4, 4.2, 6).
    DialogsChanged,
}

/// What a message to the server asks (3.1, 4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Give,
    Disable,
}

/// A message sent to the game's server, which sends it back (3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub holder: u16,
    pub id: String,
    pub change: Change,
}

/// The information portions of a game: every holder's list, the player
/// character's articles and tasks, the messages on their way back, and the
/// events and log lines not yet taken.
#[derive(Clone, Debug)]
pub struct InfoPortions {
    declarations: Arc<Declarations>,
    player: u16,
    /// The simulation's registry (`sav` 9 part 1), by object id.
    known: BTreeMap<u16, Vec<Known>>,
    articles: Vec<KnownArticle>,
    tasks: Vec<GivenTask>,
    sent: Vec<Message>,
    events: Vec<Event>,
    log: Vec<String>,
}

impl InfoPortions {
    /// No holder holds anything; `player` is the player character's id.
    pub fn new(declarations: Arc<Declarations>, player: u16) -> Self {
        Self {
            declarations,
            player,
            known: BTreeMap::new(),
            articles: Vec::new(),
            tasks: Vec::new(),
            sent: Vec::new(),
            events: Vec::new(),
            log: Vec::new(),
        }
    }

    pub fn declarations(&self) -> &Declarations {
        &self.declarations
    }

    /// The player character's id.
    pub fn player(&self) -> u16 {
        self.player
    }

    /// Whether `holder`'s list has `id` (2): an undeclared id, or an
    /// object without a list, gives *does not hold*, without error. This is
    /// also the simulation's question about any object id.
    pub fn holds(&self, holder: u16, id: &str) -> bool {
        self.list(holder).iter().any(|k| k.id == id)
    }

    /// The game time `holder` received `id` (2: `get_info_time`).
    /// TODO(#999): for a portion it does not hold the spec does not say;
    /// none.
    pub fn received_time(&self, holder: u16, id: &str) -> Option<u64> {
        (self.list(holder).iter())
            .find(|k| k.id == id)
            .map(|k| k.received)
    }

    /// `holder`'s list, in the order given (2).
    pub fn list(&self, holder: u16) -> &[Known] {
        self.known.get(&holder).map_or(&[], Vec::as_slice)
    }

    /// Every holder's list, by increasing object id: the registry `sav` 9
    /// part 1 saves. A holder whose last portion was disabled keeps an
    /// empty entry (unestablished, 2; our reading).
    pub fn registry(&self) -> &BTreeMap<u16, Vec<Known>> {
        &self.known
    }

    /// A loaded registry (2): only the entries are restored; none of the
    /// effects of 3 happen.
    pub fn restore(&mut self, registry: BTreeMap<u16, Vec<Known>>) {
        self.known = registry;
    }

    /// The player character's articles (3.4 step 1).
    pub fn articles(&self) -> &[KnownArticle] {
        &self.articles
    }

    /// The player character's tasks (3.4 step 2).
    pub fn tasks(&self) -> &[GivenTask] {
        &self.tasks
    }

    /// Adds article `id` to the player character's articles at game time
    /// `now`, as 3.4 step 1 adds one but without its callback, unless it
    /// knows it already: whether it was added. An undeclared article stops
    /// the game (1.2): the error. Giving a task adds its objectives'
    /// articles so (`script-api-info-portions-tasks-news-and-map-spots` 4.1
    /// step 4).
    pub fn learn_article(&mut self, id: &str, now: u64) -> io::Result<bool> {
        if self.articles.iter().any(|a| a.id == id) {
            return Ok(false);
        }
        let index = self.declarations.articles.look_up(id, &mut self.log)?;
        self.articles.push(KnownArticle {
            received: now,
            id: id.to_owned(),
            read: false,
            article_type: self.declarations.article_data[index].article_type.clone(),
        });
        Ok(true)
    }

    /// The messages sent and not yet returned (3.1).
    pub fn sent(&self) -> &[Message] {
        &self.sent
    }

    /// Whether events or log lines wait to be taken.
    pub fn has_pending(&self) -> bool {
        !self.events.is_empty() || !self.log.is_empty()
    }

    /// The events since the last call, in order.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The log lines since the last call: an undeclared id's listing of the
    /// index (1.2), written before the game stops.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    /// Gives `id` to `holder` by a transfer (3.1) at game time `now`: the
    /// message to the server, the lookup (an undeclared id stops the game:
    /// the error), then receiving at once (3.2, 3.4).
    pub fn give(&mut self, holder: u16, id: &str, now: u64) -> io::Result<()> {
        self.send(holder, id, Change::Give);
        self.declarations.portions.look_up(id, &mut self.log)?;
        self.receive(holder, id, now)
    }

    /// Disables `id` on `holder` by a transfer (4.1): the message, the
    /// lookup (an undeclared id stops the game even when it is not held),
    /// then the disabling at once (4.2).
    pub fn disable(&mut self, holder: u16, id: &str) -> io::Result<()> {
        self.send(holder, id, Change::Disable);
        self.declarations.portions.look_up(id, &mut self.log)?;
        self.disable_on(holder, id);
        Ok(())
    }

    /// A document item with portion `id` taken into `holder`'s inventory
    /// (7.4): when `id` is not empty, only the *give* message is sent; the
    /// holder receives it when the message returns ([`Self::deliver`]).
    /// A PDA item's portion gives nothing (7.4, believed).
    pub fn document_taken(&mut self, holder: u16, id: &str) {
        if !id.is_empty() {
            self.send(holder, id, Change::Give);
        }
    }

    /// The server's messages return (3.1): each message sent before this
    /// call is applied again, in order, at game time `now` (believed: on a
    /// later update than the transfer). Messages the applying sends wait
    /// for the next call.
    pub fn deliver(&mut self, now: u64) -> io::Result<()> {
        for message in std::mem::take(&mut self.sent) {
            match message.change {
                Change::Give => self.receive(message.holder, &message.id, now)?,
                Change::Disable => self.disable_on(message.holder, &message.id),
            }
        }
        Ok(())
    }

    /// The player character's dialog list for a talk partner (6): each
    /// `dialog` child of each portion it holds, in list order, then each of
    /// the partner's `actor_dialogs`, each id tried once and looked up (an
    /// undeclared dialog stops the game), kept when `conditions` (the
    /// dialog's own, `crate::dialogs::conditions::dialog_test`) holds for
    /// it, ordered by `priority`, highest first. Dialogs of equal priority
    /// keep the order tried (unestablished, 6; our reading). `conditions`
    /// gets this state, for the portions its test asks about; an error it
    /// returns stops the build.
    pub fn dialog_list(
        &mut self,
        actor_dialogs: &[&str],
        conditions: &mut dyn FnMut(&mut Self, &str) -> io::Result<bool>,
    ) -> io::Result<Vec<String>> {
        let declarations = Arc::clone(&self.declarations);
        let held: Vec<&str> = (self.list(self.player).iter())
            .filter_map(|k| declarations.portion(&k.id))
            .flat_map(|p| p.dialogs.iter().map(String::as_str))
            .collect();
        let mut tried = HashSet::new();
        let mut list = Vec::new();
        for id in held.into_iter().chain(actor_dialogs.iter().copied()) {
            if !tried.insert(id) {
                continue;
            }
            let index = declarations.dialogs.look_up(id, &mut self.log)?;
            if conditions(self, id)? {
                list.push((declarations.dialog_priorities[index], id.to_owned()));
            }
        }
        list.sort_by_key(|&(priority, _)| std::cmp::Reverse(priority));
        Ok(list.into_iter().map(|(_, id)| id).collect())
    }

    fn send(&mut self, holder: u16, id: &str, change: Change) {
        self.sent.push(Message {
            holder,
            id: id.to_owned(),
            change,
        });
    }

    /// Receiving (3.2), and on the player character 3.4.
    fn receive(&mut self, holder: u16, id: &str, now: u64) -> io::Result<()> {
        let list = self.known.entry(holder).or_default();
        if list.iter().any(|k| k.id == id) {
            return Ok(());
        }
        list.push(Known {
            id: id.to_owned(),
            received: now,
        });
        let declarations = Arc::clone(&self.declarations);
        let index = declarations.portions.look_up(id, &mut self.log)?;
        let portion = &declarations.portion_data[index];
        // 3.3: the script part.
        for function in &portion.actions {
            self.events.push(Event::Action {
                holder,
                function: function.clone(),
            });
        }
        for given in &portion.give_info {
            self.give(self.player, given, now)?;
        }
        for disabled in &portion.disable_info {
            self.disable(self.player, disabled)?;
        }
        // 3.2 step 5.
        for disabled in &portion.disables {
            self.disable(holder, disabled)?;
        }
        if holder == self.player {
            self.player_received(id, portion, now)?;
        }
        Ok(())
    }

    /// 3.4, after the player character received `id` it did not hold.
    fn player_received(&mut self, id: &str, portion: &Portion, now: u64) -> io::Result<()> {
        let declarations = Arc::clone(&self.declarations);
        // Step 1: the articles.
        self.articles
            .retain(|a| !portion.article_disables.contains(&a.id));
        for article in &portion.articles {
            if self.articles.iter().any(|a| a.id == *article) {
                continue;
            }
            let index = declarations.articles.look_up(article, &mut self.log)?;
            self.articles.push(KnownArticle {
                received: now,
                id: article.clone(),
                read: false,
                article_type: declarations.article_data[index].article_type.clone(),
            });
            self.events.push(Event::ArticleInfo {
                article: article.clone(),
            });
        }
        // Step 2: the tasks.
        for task in &portion.tasks {
            if self.tasks.iter().any(|t| t.id == *task) {
                continue;
            }
            if !declarations.has_task(task) {
                return Err(invalid(format!(
                    "NavigateToNode failed in XML file {TASKS_FILE}.xml"
                )));
            }
            self.tasks.push(GivenTask {
                id: task.clone(),
                received: now,
            });
            self.events.push(Event::TaskState { task: task.clone() });
        }
        // Steps 3 and 4.
        self.events.push(Event::InventoryInfo {
            portion: id.to_owned(),
        });
        self.events.push(Event::DialogsChanged);
        Ok(())
    }

    /// Disabling on a holder (4.2).
    fn disable_on(&mut self, holder: u16, id: &str) {
        if let Some(list) = self.known.get_mut(&holder) {
            list.retain(|k| k.id != id);
        }
        if holder == self.player {
            self.events.push(Event::DialogsChanged);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
