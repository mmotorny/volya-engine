// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Dialogs and trade as scripts reach them (script API R,
//! docs/behavior/script-api-dialogs-and-trade.md; section numbers are its):
//! the phrase classes `CPhraseDialog`, `CPhrase` and `CPhraseScript`
//! ([`phrases`]), the engine's calls of the script functions dialogs name
//! ([`Scripts::init_dialog`], [`Scripts::call_precondition`],
//! [`Scripts::call_action`] and the one-object forms; DG-2's
//! `volya_engine_core::dialogs::conditions` uses the two-speaker ones over
//! [`functions`]), the talk,
//! start-dialog, trade and trader members of `game_object` ([`talk`]), the
//! global trade-condition functions over the default trade parameters
//! ([`trade`]) and the class `game_difficulty`.
//!
//! What scripts change is the characters' parts
//! ([`crate::game_object::InventoryOwner`]: their [`talk::Talk`] and
//! trade parameters), the default trade parameters and the events for the
//! engine ([`DialogsAndTrade`]). The `game_object` members are defined by
//! the Lua side (`dialogs.lua`) over one hidden Rust function, as the
//! story's are. See docs/architecture/scripts.md, Dialogs and trade.

pub mod functions;
pub mod phrases;
pub mod talk;
pub mod trade;

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_formats::cp1251;
use volya_engine_formats::dialog_files::{Body, ConditionsAndEffects, Dialog};

use crate::classes::{EngineClass, ObjectMaker};
use crate::entry_points::{Keep, lua_text};
use crate::files::{FileSystem, IniFile};
use crate::game_object::members::Handle;
use crate::game_object::{GameObject, GameObjects, Kinds, ObjectClass, game_bytes};
use crate::loader::Scripts;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::story::Story;
use crate::{Error, ErrorKind, Value};

pub use phrases::{PhraseDialog, PhrasePart, PhraseScript, SharedDialog};
pub use talk::{Talk, offer_talk};
pub use trade::{TradeParameters, TradePart};

/// `game_difficulty`'s constants (section 7).
pub const DIFFICULTIES: [(&str, i32); 4] =
    [("novice", 0), ("stalker", 1), ("veteran", 2), ("master", 3)];

/// Which motion of a trader a member plays (section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraderMotion {
    Body,
    Head,
}

/// What the members ask of the engine, in order
/// ([`DialogsAndTrade::take_events`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `run_talk_dialog`'s partner accepted (4.2): the topmost open in-game
    /// window, if any, is toggled off and the talk window is shown.
    TalkWindowShown { partner: u16 },
    /// `switch_to_trade` (5.1): the talk window switches to trade, as its
    /// trade button does (DG-10).
    SwitchedToTrade,
    /// A trader plays its model's motion `name` (6): when it ends it is
    /// forgotten.
    TraderMotion {
        trader: u16,
        motion: TraderMotion,
        name: Vec<u8>,
    },
    /// `set_trader_sound` (6): the trader's current sound stopped, the head
    /// motion `head_motion` played and the sound file `sound` played for
    /// the trader (positioned as spec T states).
    TraderSound {
        trader: u16,
        sound: Vec<u8>,
        head_motion: Vec<u8>,
    },
    /// `external_sound_start` (script API T 6.6; scripted-motion-channels.md
    /// 2.5): the trader's current sound stopped, the sound file `sound`
    /// played for it as `set_trader_sound`'s, the head channel marked ended.
    ExternalSoundStart { trader: u16, sound: Vec<u8> },
    /// `external_sound_stop` (T 6.6; TC2 2.6): the trader's sound stopped.
    ExternalSoundStop { trader: u16 },
}

/// The dialogs and trade's own state.
#[derive(Default)]
pub struct DialogsAndTrade {
    /// The default trade parameters (5.2), made on first use; the engine
    /// destroys them with the level ([`Scripts::destroy_default_trade`]).
    pub default_trade: Option<TradeParameters>,
    events: Vec<Event>,
}

impl DialogsAndTrade {
    pub fn new() -> Self {
        Self::default()
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The events since the last call other than the trader members'
    /// ([`DialogsAndTrade::take_trader_events`]), oldest first; those are
    /// kept.
    pub fn take_other_events(&mut self) -> Vec<Event> {
        let trader = |e: &Event| {
            matches!(
                e,
                Event::TraderMotion { .. }
                    | Event::TraderSound { .. }
                    | Event::ExternalSoundStart { .. }
                    | Event::ExternalSoundStop { .. }
            )
        };
        let (kept, taken) = std::mem::take(&mut self.events)
            .into_iter()
            .partition(trader);
        self.events = kept;
        taken
    }

    /// The trader members' events ([`Event::TraderMotion`],
    /// [`Event::TraderSound`]) since the last call, oldest first: of
    /// `trader`, or of every trader; the other events are kept.
    pub fn take_trader_events(&mut self, trader: Option<u16>) -> Vec<Event> {
        let (taken, kept) = std::mem::take(&mut self.events).into_iter().partition(|e| {
            let of = match e {
                Event::TraderMotion { trader, .. }
                | Event::TraderSound { trader, .. }
                | Event::ExternalSoundStart { trader, .. }
                | Event::ExternalSoundStop { trader } => *trader,
                _ => return false,
            };
            trader.is_none_or(|t| t == of)
        });
        self.events = kept;
        taken
    }

    /// An event raised by another part of the interface (script API T's
    /// trader members).
    pub(crate) fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }
}

/// What the members share.
#[derive(Clone)]
pub(crate) struct Context {
    pub state: Rc<RefCell<DialogsAndTrade>>,
    pub objects: Rc<RefCell<GameObjects>>,
    pub services: Rc<RefCell<EngineServices>>,
    pub files: Rc<RefCell<FileSystem>>,
    pub story: Rc<RefCell<Story>>,
    pub fatal: Rc<RefCell<Option<String>>>,
}

impl Context {
    pub fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error standing for a crash the spec believes (our wording).
    /// The error to raise.
    pub fn fatal(&self, function: &str, line: u32, text: &str) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression: "fatal error",
                description: "<no expression>",
                arguments: &[text],
            },
        )
    }

    /// A failed check with description `description` (CL 3.8): fatal.
    fn failed_check(&self, function: &str, line: u32, description: &str) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression: "fatal error",
                description,
                arguments: &[],
            },
        )
    }

    /// The default trade parameters, made on first use (5.2).
    fn with_defaults<T>(&self, f: impl FnOnce(&mut TradeParameters) -> T) -> Result<T, String> {
        let mut state = self.state.borrow_mut();
        if state.default_trade.is_none() {
            let system = Rc::clone(&self.files.borrow().system);
            let made = TradeParameters::defaults(&system)
                .map_err(|key| self.failed_check("CTradeParameters", line!(), &key))?;
            state.default_trade = Some(made);
        }
        Ok(f(state.default_trade.as_mut().expect("made above")))
    }
}

type Results = Result<Vec<Value>, String>;

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// A text argument (our wording for a mismatch; depends on A).
fn text(args: &[Value], i: usize, what: &str) -> Result<Vec<u8>, String> {
    match arg(args, i) {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("{what}: text expected")),
    }
}

fn handle(maker: &ObjectMaker, args: &[Value], i: usize, what: &str) -> Result<u16, String> {
    maker
        .get::<Handle>(arg(args, i))
        .map(|h| h.id)
        .ok_or_else(|| format!("{what}: game object expected"))
}

fn boolean(b: bool) -> Results {
    Ok(vec![Value::Boolean(b)])
}

/// Registers the classes, the global functions, the raw function and the
/// Lua side.
fn register(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    for class in phrases::classes(ctx) {
        scripts.register_class(class)?;
    }
    scripts.register_class(
        DIFFICULTIES
            .into_iter()
            .fold(EngineClass::new("game_difficulty"), |c, (name, value)| {
                c.constant(name, value)
            }),
    )?;
    let lua = scripts.lua_mut();
    for name in ["sell_condition", "buy_condition", "show_condition"] {
        let c = ctx.clone();
        lua.register_with_objects(name, move |maker, args| {
            c.stopped()?;
            trade_condition(&c, maker, name, args)
        })?;
    }
    let c = ctx.clone();
    lua.register_with_objects("__volya_dialogs", move |maker, args| raw(&c, maker, args))?;
    lua.exec("=dialogs", include_bytes!("dialogs.lua"))
        .map(drop)
}

/// The global trade-condition functions (5.2), on the default trade
/// parameters: `sell_condition` and `buy_condition` with an `ini_file`
/// and a section or two factors, `show_condition` with an `ini_file` and
/// a section. Another argument list matches no form (our wording;
/// depends on A).
fn trade_condition(ctx: &Context, maker: &ObjectMaker, name: &str, args: &[Value]) -> Results {
    let no_form = || format!("{name}: no form of it takes these arguments");
    if let [Value::Number(f1), Value::Number(f2), ..] = args
        && name != "show_condition"
    {
        let default = (crate::values::to_f32(*f1), crate::values::to_f32(*f2));
        ctx.with_defaults(|d| match name {
            "sell_condition" => d.sell.default = default,
            _ => d.buy.default = default,
        })?;
        return Ok(Vec::new());
    }
    let ini = maker.get::<IniFile>(arg(args, 0)).ok_or_else(no_form)?;
    let Value::String(section) = arg(args, 1) else {
        return Err(no_form());
    };
    let section = cp1251::decode(section);
    let Some(found) = ini.0.section(&section) else {
        // Believed fatal (5.3).
        return Err(ctx.failed_check(name, line!(), &format!("cannot find section {section}")));
    };
    ctx.with_defaults(|d| match name {
        "sell_condition" => d.sell.read(found),
        "buy_condition" => d.buy.read(found),
        _ => trade::read_show(&mut d.show, found),
    })?;
    Ok(Vec::new())
}

/// Whether `object` is the player's.
fn is_player(object: &GameObject) -> bool {
    object.kinds.has(Kinds::PLAYER)
}

/// The engine part of the Lua side's members and its own operations:
/// `args[0]` names it, `args[1]` is the member's object.
fn raw(ctx: &Context, maker: &ObjectMaker, args: &[Value]) -> Results {
    ctx.stopped()?;
    let op = match arg(args, 0) {
        Value::String(op) => String::from_utf8_lossy(op).into_owned(),
        _ => return Err("dialogs: operation expected".into()),
    };
    let op = op.as_str();
    let id = handle(maker, args, 1, op)?;
    let args = args.get(2..).unwrap_or(&[]);
    let mut objects = ctx.objects.borrow_mut();
    let Some(object) = objects.get_mut(id) else {
        return Err(format!("game object {id} no longer exists"));
    };
    let class = object.class;
    let trader = matches!(class, ObjectClass::Named(talk::TRADER));
    let player = is_player(object);
    let start_dialog = talk::has_start_dialog(class);
    let owner = object.owner.as_mut();
    match op {
        // 4.1: characters only; queries false on other objects.
        "is_talking" => boolean(owner.is_some_and(|o| o.talk.talking)),
        "stop_talk" => {
            if let Some(o) = owner {
                o.talk.stop();
            }
            Ok(Vec::new())
        }
        "enable_talk" | "disable_talk" => {
            if let Some(o) = owner {
                o.talk.talk_enabled = op == "enable_talk";
            }
            Ok(Vec::new())
        }
        "is_talk_enabled" => boolean(talk::talk_enabled(object)),
        // 4.2.
        "run_talk_dialog" => {
            if !player {
                return Ok(Vec::new());
            }
            let partner = maker.get::<Handle>(arg(args, 0)).map(|h| h.id);
            let accepted = partner.and_then(|p| offer_talk(&mut objects, id, p));
            match (partner, accepted) {
                (Some(partner), Some(true)) => {
                    drop(objects);
                    (ctx.state.borrow_mut().events).push(Event::TalkWindowShown { partner });
                    Ok(Vec::new())
                }
                (_, Some(false)) => Ok(Vec::new()),
                // Believed an access fault (4.2): our wording.
                _ => {
                    drop(objects);
                    Err(ctx.fatal(
                        op,
                        line!(),
                        "run_talk_dialog: the partner is not a character",
                    ))
                }
            }
        }
        // 4.3.
        "switch_to_talk" => Ok(Vec::new()),
        // 4.4: stalkers and traders only.
        "set_start_dialog" => {
            let dialog = text(args, 0, op)?;
            if let Some(o) = owner.filter(|_| start_dialog) {
                o.talk.start_dialog = dialog;
            }
            Ok(Vec::new())
        }
        "get_start_dialog" => Ok(match owner.filter(|_| start_dialog) {
            // The shared-text kind: nil when empty (believed).
            Some(o) if !o.talk.start_dialog.is_empty() => {
                vec![Value::String(o.talk.start_dialog.clone())]
            }
            Some(_) => vec![Value::Nil],
            None => Vec::new(),
        }),
        "restore_default_start_dialog" => {
            if let Some(o) = owner.filter(|_| start_dialog) {
                o.talk.start_dialog = o.talk.default_start_dialog.clone();
            }
            Ok(Vec::new())
        }
        // 5.1.
        "enable_trade" | "disable_trade" => {
            if let Some(o) = owner {
                o.talk.trade_enabled = op == "enable_trade";
            }
            Ok(Vec::new())
        }
        "is_trade_enabled" => boolean(owner.is_some_and(|o| o.talk.trade_enabled)),
        "switch_to_trade" => {
            let partner = owner.and_then(|o| o.talk.partner);
            let trade_enabled = |objects: &GameObjects, id: u16| {
                objects
                    .get(id)
                    .and_then(|o| o.owner.as_ref())
                    .is_some_and(|o| o.talk.trade_enabled)
            };
            if player
                && ctx.story.borrow().talk_window_shown
                && let Some(partner) = partner
                && trade_enabled(&objects, id)
                && trade_enabled(&objects, partner)
            {
                drop(objects);
                ctx.state.borrow_mut().events.push(Event::SwitchedToTrade);
            }
            Ok(Vec::new())
        }
        // 6: traders only.
        "set_trader_global_anim" | "set_trader_head_anim" => {
            let name = text(args, 0, op)?;
            if trader {
                drop(objects);
                let motion = if op == "set_trader_global_anim" {
                    TraderMotion::Body
                } else {
                    TraderMotion::Head
                };
                ctx.state.borrow_mut().events.push(Event::TraderMotion {
                    trader: id,
                    motion,
                    name,
                });
            }
            Ok(Vec::new())
        }
        "set_trader_sound" => {
            let sound = text(args, 0, op)?;
            let head_motion = text(args, 1, op)?;
            if trader {
                drop(objects);
                ctx.state.borrow_mut().events.push(Event::TraderSound {
                    trader: id,
                    sound,
                    head_motion,
                });
            }
            Ok(Vec::new())
        }
        _ => Err(format!("dialogs: unknown operation {op}")),
    }
}

/// What a state with the dialogs and trade registered keeps for the
/// engine's calls.
pub(crate) struct Registered {
    ctx: Context,
}

/// The text of a list entry (`files` 6); an entry without text is the
/// empty text (TODO(#1071), as DG-2's lists).
fn entry(e: &Option<String>) -> &str {
    e.as_deref().unwrap_or("")
}

impl Scripts {
    /// Registers the dialogs and trade's names (1.1) over `state`: after
    /// the game object basics and the story (whose objects, services, files
    /// and information portions it shares), before
    /// [`Scripts::load_globals`].
    pub fn register_dialogs_and_trade(
        &mut self,
        state: Rc<RefCell<DialogsAndTrade>>,
    ) -> Result<(), Error> {
        let api = |message: &str| Error {
            kind: ErrorKind::Api,
            message: message.into(),
        };
        let basics = (self.game_object_state.as_ref())
            .ok_or_else(|| api("the game object basics aren't registered"))?;
        let story = (self.registered_story()).ok_or_else(|| api("the story isn't registered"))?;
        let ctx = Context {
            state,
            objects: Rc::clone(&basics.objects),
            services: Rc::clone(&basics.services),
            files: Rc::clone(&basics.files),
            story,
            fatal: self.fatal_flag(),
        };
        register(self, &ctx)?;
        self.dialogs_state = Some(Registered { ctx });
        Ok(())
    }

    /// The dialogs and trade's state, once registered.
    pub(crate) fn registered_dialogs(&self) -> Option<Rc<RefCell<DialogsAndTrade>>> {
        self.dialogs_state.as_ref().map(|r| Rc::clone(&r.ctx.state))
    }

    fn dialogs_context(&self) -> Result<Context, Error> {
        self.dialogs_state
            .as_ref()
            .map(|r| r.ctx.clone())
            .ok_or_else(|| Error {
                kind: ErrorKind::Api,
                message: "the dialogs and trade aren't registered".into(),
            })
    }

    /// The dialog's init function called once, when it is first loaded
    /// (2.1, 3.5): a dialog without a phrase list gets its phrases from
    /// the function its `init_func` names, called with the dialog itself
    /// (a `CPhraseDialog`, an engine object living as long as the state),
    /// its result ignored; found and called as D 1.1 and 1.2 state, an
    /// error or a missing function fatal as [`Scripts::call_precondition`]'s.
    /// The dialog as the engine then shares it; phrase objects scripts keep
    /// refer to it. An empty `init_func` calls nothing (our choice). Which
    /// dialogs are loaded when is DG-9's and `files` 3.2's.
    pub fn init_dialog(&mut self, dialog: Dialog) -> Result<SharedDialog, Error> {
        self.dialogs_context()?;
        let name = match &dialog.body {
            Body::InitFunc(name) => name.clone(),
            Body::PhraseList(_) => return Ok(Rc::new(RefCell::new(dialog))),
        };
        let shared = Rc::new(RefCell::new(Dialog {
            body: Body::PhraseList(Vec::new()),
            ..dialog
        }));
        if name.is_empty() {
            return Ok(shared);
        }
        let part = PhraseDialog(Rc::clone(&shared));
        let object = (self.lua_mut()).new_engine_object("CPhraseDialog", |_| part)?;
        self.call_dialog_function(&name, Keep::Nothing, &[Value::Object(object)])?;
        Ok(shared)
    }

    /// Whether the player character holds every `has` id and no
    /// `dont_has` id of `lists` (`IP` 2).
    fn holds_lists(&self, ctx: &Context, lists: &ConditionsAndEffects) -> bool {
        let story = ctx.story.borrow();
        let holds = |id: &Option<String>| {
            (story.info_portions.as_ref()).is_some_and(|p| {
                let p = p.borrow();
                p.holds(p.player(), entry(id))
            })
        };
        lists.has_info.iter().all(holds) && !lists.dont_has_info.iter().any(holds)
    }

    /// The give list's ids given to the player character, then the disable
    /// list's disabled on it, each by a transfer (`IP` 3.1, 4.1) with what
    /// receiving runs.
    fn transfer_lists(&mut self, lists: &ConditionsAndEffects) -> Result<(), Error> {
        let changes = (lists.give_info.iter().map(|id| (id, true)))
            .chain(lists.disable_info.iter().map(|id| (id, false)));
        for (id, give) in changes {
            let id = game_bytes(entry(id));
            self.with_story(|c, story| {
                let Some(player) = story.info_portions.as_ref().map(|p| p.borrow().player()) else {
                    return Err("the information portions aren't set".into());
                };
                if give {
                    c.give_portion(story, player, &id)
                } else {
                    c.disable_portion(story, player, &id)
                }
            })?;
        }
        Ok(())
    }

    /// The one-object precondition form (3.4): has and dont-has asked of
    /// the player character, then each function with `object`'s handle,
    /// the result rule of 3.2. Believed unused in this edition.
    pub fn test_object_conditions(
        &mut self,
        lists: &ConditionsAndEffects,
        object: u16,
    ) -> Result<bool, Error> {
        let ctx = self.dialogs_context()?;
        if !self.holds_lists(&ctx, lists) {
            return Ok(false);
        }
        let args = [ctx.objects.borrow().handle_value(Some(object))];
        for name in &lists.precondition {
            let result = self.call_dialog_function(entry(name), Keep::Truth, &args)?;
            if !matches!(result, Value::Boolean(true)) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The one-object action form (3.4): each action function with
    /// `object`'s handle and `text` (nil for none), then the give and
    /// disable lists on the player character. Receiving an information
    /// portion runs it with the receiving holder and nil (`IP` 3.3).
    pub fn run_object_effects(
        &mut self,
        lists: &ConditionsAndEffects,
        object: u16,
        text: Option<&str>,
    ) -> Result<(), Error> {
        let ctx = self.dialogs_context()?;
        let args = [
            ctx.objects.borrow().handle_value(Some(object)),
            text.map_or(Value::Nil, lua_text),
        ];
        for name in &lists.action {
            self.call_dialog_function(entry(name), Keep::Nothing, &args)?;
        }
        self.transfer_lists(lists)
    }

    /// The level is destroyed: the default trade parameters go, to be made
    /// afresh at their next use (5.2).
    pub fn destroy_default_trade(&mut self) -> Result<(), Error> {
        self.dialogs_context()?.state.borrow_mut().default_trade = None;
        Ok(())
    }
}
