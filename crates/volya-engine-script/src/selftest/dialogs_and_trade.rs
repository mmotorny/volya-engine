// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-dialogs-and-trade.md on every target, one
//! check per section (3.1 to 3.3 are [`super::dialogs`]'), over characters, portions and a script of dialog
//! functions of our own.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::info_portions::{Declarations, InfoPortions};
use volya_engine_formats::dialog_files::{Body, ConditionsAndEffects, Dialog};
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;
use volya_engine_formats::xml;

use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::dialogs::trade::{TradePart, first_factor_weight, item_factors, price, shown};
use crate::dialogs::{DialogsAndTrade, Event, SharedDialog, TraderMotion};
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass, SpawnData};
use crate::loader::{Prefix, Scripts};
use crate::object_factory::class_id;
use crate::services::EngineServices;
use crate::story::Story;
use crate::{Error, ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 the phrase classes", phrase_classes),
    ("3.4 the one-object forms", one_object_forms),
    ("3.5 init_func", init_func),
    ("4 talk", talk),
    ("4.4 the start dialog", start_dialog),
    ("5.1 trade enabled", trade_enabled),
    ("5.2 the trade-condition functions", trade_conditions),
    ("5.3 factors and price", factors_and_price),
    ("6 trader members", trader_members),
    ("7 game_difficulty", game_difficulty),
    ("8 worked example", worked_example),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("dialogs and trade {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

pub(super) const PLAYER: u16 = 0;
const RIFLE: u16 = 20;
pub(super) const STALKER: u16 = 30;
pub(super) const DOG: u16 = 40;
pub(super) const TRADER: u16 = 60;

const PORTIONS: &str = r#"<game_information_portions>
  <info_portion id="talked"/>
  <info_portion id="reward"/>
</game_information_portions>"#;

const SYSTEM: &str = "[trade]\r\nbuy_price_factor_hostile = 1\r\nbuy_price_factor_friendly = 2\r\n\
sell_price_factor_hostile = 3\r\nsell_price_factor_friendly = 4\r\n";

const SCRIPT: &str = r##"
calls = {}
local function label(v)
  if type(v) == "userdata" then
    local ok, id = pcall(function() return v:id() end)
    if ok then return "obj" .. id end
    return "userdata"
  end
  return tostring(v)
end
function note(...)
  local t = {}
  for i = 1, select("#", ...) do t[i] = label((select(i, ...))) end
  table.insert(calls, table.concat(t, " "))
end
function yes(...) note("yes", ...) return 1 end
function no(...) note("no", ...) end
function zero() note("zero") return 0 end
function text() note("text") return "" end
function falsy() note("false") return false end
function act(...) note("act", ...) return false end
function fails() error("boom", 0) end
function stop_both(a, b) a:stop_talk() b:stop_talk() note("stopped") end
function init_gen(dlg, ...)
  note("init", select("#", ...))
  kept_dialog = dlg
  local p0 = dlg:AddPhrase("first_text", "0", "", -10000)
  p0:GetPhraseScript():AddAction("talks.act")
  local a = dlg:AddPhrase("a_text", "1", "0", 5.9)
  again = dlg:AddPhrase("other", "1", "0", 0)
  dlg:AddPhrase("b_text", "2", "0", -3.7)
  local s = a:GetPhraseScript()
  s:AddPrecondition("talks.yes") s:AddPrecondition("talks.yes")
  s:AddHasInfo("talked") s:AddDontHasInfo("reward")
  s:AddGiveInfo("reward") s:AddDisableInfo("talked") s:AddDisableInfo("no such id")
  return "ignored"
end
function init_bad(dlg) dlg:AddPhrase("x", "1", "9", 0) end
function init_fails(dlg) error("init boom", 0) end
function init_medkit(dlg)
  local phr = dlg:AddPhrase("dm_general_help_medkit", "0", "", -10000)
  phr:GetPhraseScript():AddAction("talks.act")
  for _, v in ipairs({ { name = "thanks_1", phr_id = "1" }, { name = "thanks_2", phr_id = "2" } }) do
    local p = dlg:AddPhrase(v.name, v.phr_id, "0", -10000)
    if p ~= nil then p:GetPhraseScript():AddPrecondition("talks.yes") end
  end
end
"##;

/// A test state: the scripts, the dialogs and trade, the story and the
/// objects.
pub(super) struct State {
    pub(super) s: Scripts,
    dialogs: Rc<RefCell<DialogsAndTrade>>,
    story: Rc<RefCell<Story>>,
    pub(super) objects: Rc<RefCell<GameObjects>>,
}

pub(super) fn data(id: u16, name: &str, class: &str) -> SpawnData {
    SpawnData {
        id,
        name: name.as_bytes().to_vec(),
        section: name.as_bytes().to_vec(),
        class_id: class_id(class.as_bytes()),
        story_id: -1,
        custom_data: Vec::new(),
        parent: None,
        placement: transform([0.0; 3], [0.0; 3]),
        level_vertex: 0,
        game_vertex: 0,
        health: None,
        server: None,
    }
}

fn parse(text: &str) -> Result<xml::Document, String> {
    xml::parse(text.as_bytes()).map_err(|e| e.to_string())
}

fn message(e: Error) -> String {
    e.message
}

/// A state with everything up to the dialogs and trade registered, `_G`
/// loaded, a global `obj(id)` giving handles, the player, a rifle, a
/// stalker, a dog and a trader, and the portions.
pub(super) fn state() -> Result<State, String> {
    let tree = Tree(
        [
            ("gamedata/scripts/_g.script", ""),
            ("gamedata/scripts/talks.script", SCRIPT),
        ]
        .iter()
        .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
        .collect(),
    );
    let tree = Rc::new(tree);
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(message)?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree,
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(message)?;
    let story = Rc::new(RefCell::new(Story::new()));
    s.register_story(Rc::clone(&story)).map_err(message)?;
    let dialogs = Rc::new(RefCell::new(DialogsAndTrade::new()));
    s.register_dialogs_and_trade(Rc::clone(&dialogs))
        .map_err(message)?;
    let shared = Rc::clone(&objects);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(shared.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(message)?;
    // The engine's calls into scripts (spec D), as startup installs them.
    crate::entry_points::install(s.lua_mut()).map_err(message)?;
    s.load_globals().map_err(message)?;
    for (class, data) in [
        (ObjectClass::Player, data(PLAYER, "actor", "O_ACTOR")),
        (
            ObjectClass::Named("CWeaponAK74"),
            data(RIFLE, "rifle", "WP_AK74"),
        ),
        (
            ObjectClass::Named("CAI_Stalker"),
            data(STALKER, "stalker", "AI_STL_S"),
        ),
        (ObjectClass::Named("CAI_Dog"), data(DOG, "dog", "AI_DOG_R")),
        (
            ObjectClass::Named("CAI_Trader"),
            data(TRADER, "trader", "AI_TRADE"),
        ),
    ] {
        s.spawn_engine_object(class, data)
            .map_err(|e| e.error.message)?;
    }
    let declarations = Declarations::from_documents(
        &[("portions".into(), parse(PORTIONS)?)],
        &[("articles".into(), parse("<encyclopedia/>")?)],
        &[("dialogs".into(), parse("<game_dialogs/>")?)],
        &parse("<game_tasks/>")?,
    )
    .map_err(|e| e.to_string())?;
    story.borrow_mut().info_portions = Some(Rc::new(RefCell::new(InfoPortions::new(
        Arc::new(declarations),
        PLAYER,
    ))));
    // Load the script so its functions are reachable as `talks.*`.
    s.exec("=load", b"local _ = talks.note").map_err(message)?;
    Ok(State {
        s,
        dialogs,
        story,
        objects,
    })
}

const PRELUDE: &str =
    "local actor, rifle, stalker, dog, trader = obj(0), obj(20), obj(30), obj(40), obj(60)\n";

pub(super) fn check_lua(t: &mut State, name: &str, body: &str) -> Result<(), String> {
    run(&mut t.s, name, &format!("{PRELUDE}{body}"))
}

/// The calls the script noted since the last call, as texts.
fn calls(t: &mut State) -> Result<Vec<String>, String> {
    let results = (t.s.exec(
        "=calls",
        b"local c = talks.calls; talks.calls = {}; return c",
    ))
    .map_err(message)?;
    let Some(Value::Table(entries)) = results.into_iter().next() else {
        return Err("no calls table".into());
    };
    let mut entries: Vec<(f64, String)> = entries
        .into_iter()
        .map(|(k, v)| {
            let text = match v {
                Value::String(s) => String::from_utf8_lossy(&s).into_owned(),
                v => format!("{v:?}"),
            };
            (k.as_number().unwrap_or(0.0), text)
        })
        .collect();
    entries.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(entries.into_iter().map(|(_, t)| t).collect())
}

fn expect_calls(t: &mut State, want: &[&str]) -> Result<(), String> {
    let got = calls(t)?;
    ensure(got == want, || format!("calls {got:?}, want {want:?}"))
}

fn holds(t: &State, id: &str) -> bool {
    (t.story.borrow().info_portions.as_ref()).is_some_and(|p| p.borrow().holds(PLAYER, id))
}

fn generated(id: &str, init: &str) -> Dialog {
    Dialog {
        id: id.into(),
        priority: 0,
        caption: None,
        lists: ConditionsAndEffects::default(),
        body: Body::InitFunc(init.into()),
    }
}

fn names(list: &[&str]) -> Vec<Option<String>> {
    list.iter().map(|n| Some((*n).to_owned())).collect()
}

fn fatal_with(result: Result<impl std::fmt::Debug, Error>, text: &str) -> Result<(), String> {
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal && e.message.contains(text)),
        || format!("{result:?}, not fatal with {text:?}"),
    )
}

/// Section 1: what is registered.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "1",
        r#"
for _, m in ipairs({"run_talk_dialog", "switch_to_talk", "stop_talk", "is_talking",
    "enable_talk", "disable_talk", "is_talk_enabled", "set_start_dialog", "get_start_dialog",
    "restore_default_start_dialog", "enable_trade", "disable_trade", "is_trade_enabled",
    "switch_to_trade", "set_trader_global_anim", "set_trader_head_anim", "set_trader_sound"}) do
  eq(type(actor[m]), "function", m)
end
for _, f in ipairs({"sell_condition", "buy_condition", "show_condition"}) do
  eq(type(_G[f]), "function", f)
end
for _, c in ipairs({"CPhraseDialog", "CPhrase", "CPhraseScript", "game_difficulty"}) do
  raises(function() return _G[c]() end, "", c .. " has no constructor")
end
"#,
    )
}

/// Section 2: `AddPhrase`, `GetPhraseScript` and the six lists, through
/// an init function.
pub fn phrase_classes() -> Result<(), String> {
    let mut t = state()?;
    let dialog =
        t.s.init_dialog(generated("gen", "talks.init_gen"))
            .map_err(message)?;
    let d = dialog.borrow();
    let p = d.phrases();
    let ids: Vec<&str> = p.iter().map(|p| p.id.as_str()).collect();
    ensure(ids == ["0", "1", "2"], || format!("phrase ids {ids:?}"))?;
    // 2.2 step 3: a link for the existing phrase too, after the earlier.
    ensure(p[0].links == [1, 1, 2], || {
        format!("links {:?}", p[0].links)
    })?;
    ensure(
        p[1].text == "a_text" && p[1].goodwill == 5 && p[2].goodwill == -3,
        || format!("texts and goodwill {p:?}"),
    )?;
    ensure(p[0].lists.action == names(&["talks.act"]), || {
        format!("phrase 0's actions {:?}", p[0].lists)
    })?;
    let l = &p[1].lists;
    ensure(
        l.precondition == names(&["talks.yes", "talks.yes"])
            && l.has_info == names(&["talked"])
            && l.dont_has_info == names(&["reward"])
            && l.give_info == names(&["reward"])
            && l.disable_info == names(&["talked", "no such id"]),
        || format!("phrase 1's lists {l:?}"),
    )?;
    drop(d);
    // The existing phrase's AddPhrase gave nil, and the objects kept
    // refer to the shared dialog.
    check_lua(
        &mut t,
        "2",
        r#"
eq(talks.again, nil, "AddPhrase of an existing id")
local p = talks.kept_dialog:AddPhrase("later", "3", "", 0)
eq(type(p), "userdata", "a new phrase")
raises(function() talks.kept_dialog:AddPhrase(1, "4", "", 0) end, "text expected", "a number text")
raises(function() talks.kept_dialog:AddPhrase("t", "4", "", "x") end, "number expected", "a text goodwill")
"#,
    )?;
    ensure(dialog.borrow().phrases().len() == 4, || {
        "the kept dialog's phrase not added".into()
    })?;
    // 2.2 step 3: a missing previous phrase (believed an access fault).
    fatal_with(
        t.s.init_dialog(generated("bad", "talks.init_bad")),
        "the dialog has no phrase 9",
    )
}

/// Section 3.4: one handle; actions first, then give and disable.
pub fn one_object_forms() -> Result<(), String> {
    let mut t = state()?;
    let lists = ConditionsAndEffects {
        precondition: names(&["talks.yes"]),
        action: names(&["talks.act"]),
        give_info: names(&["reward"]),
        ..ConditionsAndEffects::default()
    };
    let r = (t.s.test_object_conditions(&lists, TRADER)).map_err(message)?;
    ensure(r, || "the one-object test".into())?;
    expect_calls(&mut t, &["yes obj60"])?;
    (t.s.run_object_effects(&lists, TRADER, None)).map_err(message)?;
    expect_calls(&mut t, &["act obj60 nil"])?;
    (t.s.run_object_effects(&lists, TRADER, Some("t"))).map_err(message)?;
    expect_calls(&mut t, &["act obj60 t"])?;
    ensure(holds(&t, "reward"), || "given after the actions".into())
}

/// Section 3.5: called once with the dialog alone, its result ignored; an
/// error is fatal; a dialog with a phrase list calls nothing.
pub fn init_func() -> Result<(), String> {
    let mut t = state()?;
    t.s.init_dialog(generated("gen", "talks.init_gen"))
        .map_err(message)?;
    expect_calls(&mut t, &["init 0"])?;
    let listed = Dialog {
        body: Body::PhraseList(Vec::new()),
        ..generated("listed", "")
    };
    let d: SharedDialog = t.s.init_dialog(listed.clone()).map_err(message)?;
    ensure(*d.borrow() == listed, || "a listed dialog changed".into())?;
    let d = t.s.init_dialog(generated("none", "")).map_err(message)?;
    ensure(d.borrow().phrases().is_empty(), || {
        "an empty init_func".into()
    })?;
    expect_calls(&mut t, &[])?;
    fatal_with(
        t.s.init_dialog(generated("fails", "talks.init_fails")),
        "LUA error: init boom",
    )?;
    let mut t = state()?;
    fatal_with(
        t.s.init_dialog(generated("absent", "talks.init_absent")),
        "the dialog function talks.init_absent is missing",
    )
}

/// Section 4: the talk flags, `run_talk_dialog`'s offer, `switch_to_talk`.
pub fn talk() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "4.1",
        r#"
eq(actor:is_talking(), false, "not talking")
eq(trader:is_talk_enabled(), true, "talk enabled when made")
eq(dog:is_talk_enabled(), false, "a monster")
dog:enable_talk()
eq(dog:is_talk_enabled(), false, "a monster, enabled")
rifle:enable_talk() rifle:stop_talk()
eq(rifle:is_talking(), false, "not a character")
eq(rifle:is_talk_enabled(), false, "not a character")
trader:disable_talk()
eq(trader:is_talk_enabled(), false, "disabled")
actor:run_talk_dialog(trader)
eq(actor:is_talking(), false, "refused")
trader:enable_talk()
stalker:run_talk_dialog(trader)
eq(trader:is_talking(), false, "not the player's")
actor:run_talk_dialog(dog)
eq(dog:is_talking(), false, "a monster refuses")
actor:run_talk_dialog(trader)
eq(actor:is_talking(), true, "the player talks")
eq(trader:is_talking(), true, "the trader talks")
trader:stop_talk()
eq(trader:is_talking(), false, "the trader's side stopped")
eq(actor:is_talking(), true, "the player's side goes on")
actor:switch_to_talk() trader:switch_to_talk()
"#,
    )?;
    {
        let objects = t.objects.borrow();
        let talk = |id| {
            objects
                .get(id)
                .and_then(|o| o.owner.as_ref())
                .map(|o| o.talk.clone())
        };
        let (player, trader) = (
            talk(PLAYER).ok_or("no player")?,
            talk(TRADER).ok_or("no trader")?,
        );
        ensure(
            player.partner == Some(TRADER)
                && player.trade_partner == Some(TRADER)
                && trader.partner.is_none()
                && trader.trade_partner.is_none(),
            || format!("the sides {player:?} {trader:?}"),
        )?;
    }
    let events = t.dialogs.borrow_mut().take_events();
    ensure(
        events == [Event::TalkWindowShown { partner: TRADER }],
        || format!("events {events:?}"),
    )?;
    // Health 0 refuses.
    if let Some(e) = t
        .objects
        .borrow_mut()
        .get_mut(STALKER)
        .and_then(|o| o.entity.as_mut())
    {
        e.health = 0.0;
    }
    check_lua(
        &mut t,
        "4.2",
        r#"
actor:run_talk_dialog(stalker)
eq(stalker:is_talking(), false, "a dead partner refuses")
"#,
    )?;
    ensure(t.dialogs.borrow_mut().take_events().is_empty(), || {
        "an event for a refusal".into()
    })?;
    let result = t.s.exec(
        "=4.2",
        format!("{PRELUDE}pcall(function() actor:run_talk_dialog(rifle) end)").as_bytes(),
    );
    fatal_with(result, "the partner is not a character")
}

/// Section 4.4.
pub fn start_dialog() -> Result<(), String> {
    let mut t = state()?;
    if let Some(o) = t
        .objects
        .borrow_mut()
        .get_mut(STALKER)
        .and_then(|o| o.owner.as_mut())
    {
        o.talk.default_start_dialog = b"default_dialog".to_vec();
    }
    check_lua(
        &mut t,
        "4.4",
        r##"
eq(stalker:get_start_dialog(), nil, "none")
eq(select("#", stalker:get_start_dialog()), 1, "one result")
stalker:set_start_dialog("meet_dialog")
eq(stalker:get_start_dialog(), "meet_dialog", "set")
stalker:restore_default_start_dialog()
eq(stalker:get_start_dialog(), "default_dialog", "restored")
trader:set_start_dialog("t_dialog")
eq(trader:get_start_dialog(), "t_dialog", "a trader's")
for _, o in ipairs({actor, dog, rifle}) do
  o:set_start_dialog("x") o:restore_default_start_dialog()
  eq(select("#", o:get_start_dialog()), 0, "nothing on another object")
end
"##,
    )?;
    let objects = t.objects.borrow();
    let player = objects
        .get(PLAYER)
        .and_then(|o| o.owner.as_ref())
        .ok_or("no player")?;
    ensure(player.talk.start_dialog.is_empty(), || {
        "the player's start dialog".into()
    })
}

/// Section 5.1.
pub fn trade_enabled() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "5.1",
        r#"
eq(trader:is_trade_enabled(), true, "enabled when made")
eq(rifle:is_trade_enabled(), false, "not a character")
rifle:enable_trade()
actor:switch_to_trade()
actor:run_talk_dialog(trader)
actor:switch_to_trade()
"#,
    )?;
    let events = t.dialogs.borrow_mut().take_events();
    ensure(
        events == [Event::TalkWindowShown { partner: TRADER }],
        || format!("no switch while the window is hidden: {events:?}"),
    )?;
    t.story.borrow_mut().talk_window_shown = true;
    check_lua(
        &mut t,
        "5.1",
        r#"
trader:disable_trade()
eq(trader:is_trade_enabled(), false, "disabled")
actor:switch_to_trade()
trader:enable_trade()
trader:switch_to_trade()
actor:switch_to_trade()
"#,
    )?;
    let events = t.dialogs.borrow_mut().take_events();
    ensure(events == [Event::SwitchedToTrade], || {
        format!("events {events:?}")
    })
}

/// Section 5.2: the global functions on the defaults, made from `[trade]`
/// on first use and destroyed with the level.
pub fn trade_conditions() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "5.2",
        r#"
local ini = create_ini_file("[sell]\r\nwpn_pm = 0.5, 1.5\r\nlisted\r\nwpn_pm = 2, 3\r\n[show]\r\nwpn_pm\r\nbread = 1\r\n")
sell_condition(ini, "sell")
buy_condition(0.25, 0.75)
show_condition(ini, "show")
raises(function() show_condition(1, 2) end, "no form", "show_condition's numbers")
raises(function() sell_condition("a") end, "no form", "a text")
"#,
    )?;
    {
        let d = t.dialogs.borrow();
        let d = d.default_trade.as_ref().ok_or("no defaults")?;
        ensure(
            d.sell.default == (3.0, 4.0)
                && d.sell.factor_table == [(b"wpn_pm".to_vec(), 2.0, 3.0)]
                && d.sell.item_list == [b"listed".to_vec()]
                && d.buy.default == (0.25, 0.75)
                && d.buy.item_list.is_empty()
                && d.show == [b"wpn_pm".to_vec()],
            || format!("the defaults {d:?}"),
        )?;
        // The characters' own are untouched.
        let objects = t.objects.borrow();
        let own = objects
            .get(TRADER)
            .and_then(|o| o.owner.as_ref())
            .ok_or("no trader")?;
        ensure(own.trade.sell.factor_table.is_empty(), || {
            "the trader's own".into()
        })?;
    }
    t.s.destroy_default_trade().map_err(message)?;
    ensure(t.dialogs.borrow().default_trade.is_none(), || {
        "not destroyed".into()
    })?;
    check_lua(
        &mut t,
        "5.2",
        "show_condition(create_ini_file('[s]\\r\\n'), 's')",
    )?;
    {
        let d = t.dialogs.borrow();
        let d = d.default_trade.as_ref().ok_or("not made again")?;
        ensure(d.buy.default == (1.0, 2.0), || format!("made again {d:?}"))?;
    }
    let result = t.s.exec(
        "=5.2",
        b"pcall(function() sell_condition(create_ini_file(''), 'absent') end)",
    );
    fatal_with(result, "cannot find section absent")
}

/// Section 5.3: an item's factors, its price and the show lists.
pub fn factors_and_price() -> Result<(), String> {
    let own = TradePart {
        default: (1.5, 0.5),
        item_list: vec![b"own_listed".to_vec()],
        factor_table: vec![(b"both".to_vec(), 2.0, 1.0)],
    };
    let defaults = TradePart {
        default: (9.0, 9.0),
        item_list: vec![b"default_listed".to_vec()],
        factor_table: vec![
            (b"both".to_vec(), 7.0, 7.0),
            (b"default_only".to_vec(), 3.0, 4.0),
        ],
    };
    for (item, want) in [
        (&b"own_listed"[..], None),
        (b"default_listed", None),
        (b"both", Some((2.0, 1.0))),
        (b"default_only", Some((3.0, 4.0))),
        (b"other", Some((1.5, 0.5))),
    ] {
        let got = item_factors(&own, &defaults, item);
        ensure(got == want, || format!("{item:?}: {got:?}"))?;
    }
    for (v, want) in [
        (None, 0.0),
        (Some(1000.0), 1.0),
        (Some(5000.0), 1.0),
        (Some(-1000.0), 0.0),
        (Some(0.0), 0.5),
    ] {
        let got = first_factor_weight(v);
        ensure(got == want, || format!("weight at {v:?}: {got}"))?;
    }
    // The first factor in full at 1000, the second at -1000, condition 1.
    ensure(
        price(Some((2.0, 1.0)), Some(1000.0), 1.0, 100.0) == 200.0,
        || "the first factor".into(),
    )?;
    ensure(
        price(Some((2.0, 1.0)), Some(-1000.0), 1.0, 100.0) == 100.0,
        || "the second factor".into(),
    )?;
    let half = price(Some((1.0, 1.0)), None, 0.0, 100.0);
    let want = 0.1f32.powf(0.75) * 100.0;
    ensure(half == want, || {
        format!("condition 0: {half} against {want}")
    })?;
    ensure(price(None, Some(0.0), 1.0, 100.0) == 0.0, || {
        "a listed item".into()
    })?;
    let own_show = [b"a".to_vec()];
    let default_show = [b"b".to_vec()];
    ensure(
        !shown(&own_show, &default_show, b"a")
            && !shown(&own_show, &default_show, b"b")
            && shown(&own_show, &default_show, b"c"),
        || "the show lists".into(),
    )
}

/// Section 6: traders only.
pub fn trader_members() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "6",
        r#"
trader:set_trader_global_anim("idle")
trader:set_trader_head_anim("talk")
trader:set_trader_sound("characters_voice\\trader", "speak")
stalker:set_trader_global_anim("idle")
actor:set_trader_sound("s", "h")
raises(function() trader:set_trader_head_anim(1) end, "text expected", "a number")
"#,
    )?;
    let events = t.dialogs.borrow_mut().take_events();
    let want = [
        Event::TraderMotion {
            trader: TRADER,
            motion: TraderMotion::Body,
            name: b"idle".to_vec(),
        },
        Event::TraderMotion {
            trader: TRADER,
            motion: TraderMotion::Head,
            name: b"talk".to_vec(),
        },
        Event::TraderSound {
            trader: TRADER,
            sound: b"characters_voice\\trader".to_vec(),
            head_motion: b"speak".to_vec(),
        },
    ];
    ensure(events == want, || format!("events {events:?}"))
}

/// Section 7.
pub fn game_difficulty() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "7",
        r#"
eq(game_difficulty.novice, 0, "novice")
eq(game_difficulty.stalker, 1, "stalker")
eq(game_difficulty.veteran, 2, "veteran")
eq(game_difficulty.master, 3, "master")
"#,
    )
}

/// Section 8: the generated help dialog's shape, over a function like
/// `dialog_manager`'s.
pub fn worked_example() -> Result<(), String> {
    let mut t = state()?;
    let dialog =
        t.s.init_dialog(generated(
            "dm_help_wounded_medkit_dialog",
            "talks.init_medkit",
        ))
        .map_err(message)?;
    let lists = {
        let d = dialog.borrow();
        let p = d.phrases();
        ensure(
            p.len() == 3 && p[0].links == [1, 2] && p[0].text == "dm_general_help_medkit",
            || format!("phrases {p:?}"),
        )?;
        p.iter().map(|p| p.lists.clone()).collect::<Vec<_>>()
    };
    let id = "dm_help_wounded_medkit_dialog";
    for action in &lists[0].action {
        let action = action.as_deref().unwrap_or("");
        (t.s.call_action(action, STALKER, PLAYER, id, "0")).map_err(message)?;
    }
    for precondition in &lists[1].precondition {
        let precondition = precondition.as_deref().unwrap_or("");
        (t.s.call_precondition(precondition, PLAYER, STALKER, [id, "0", "1"])).map_err(message)?;
    }
    expect_calls(
        &mut t,
        &[
            "act obj30 obj0 dm_help_wounded_medkit_dialog 0",
            "yes obj0 obj30 dm_help_wounded_medkit_dialog 0 1",
        ],
    )
}
