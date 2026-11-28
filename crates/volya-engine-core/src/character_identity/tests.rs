// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The choice and the set (docs/behavior/character-identity.md sections
//! 2, 3) on trader records and synthetic descriptions; the stalkers' steps
//! and the shipped data are the game-data test's
//! (crates/volya-engine/tests/character_identity.rs).

use std::collections::HashMap;

use volya_engine_formats::character_descriptions::{FailedCheck, Money};
use volya_engine_formats::ltx;
use volya_engine_formats::object::{ClassId, ObjectBlock, SpawnHeader, Trader, Visual};

use super::*;
use crate::particles::random::Generator;

struct Fake {
    profiles: HashMap<String, Profile>,
    all: Vec<SpecificCharacter>,
}

impl Descriptions for Fake {
    fn profile(&mut self, id: &str) -> io::Result<&Profile> {
        self.profiles
            .get(id)
            .ok_or_else(|| failed("item not found, id", Some(id)))
    }
    fn count(&self) -> usize {
        self.all.len()
    }
    fn at(&mut self, index: usize) -> io::Result<&SpecificCharacter> {
        Ok(&self.all[index])
    }
    fn find(&mut self, id: &str) -> io::Result<&SpecificCharacter> {
        self.all
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| failed("item not found, id", Some(id)))
    }
}

fn explicit(id: &str) -> Profile {
    Profile {
        id: id.into(),
        specific_character: Some(id.into()),
        class: None,
        rank: NOT_SET,
        reputation: NOT_SET,
    }
}

fn class(id: &str, class: &str) -> Profile {
    Profile {
        id: id.into(),
        specific_character: None,
        class: Some(class.into()),
        rank: NOT_SET,
        reputation: NOT_SET,
    }
}

/// A description of class `class`, `team_default`, community `stalker`.
fn description(id: &str, class: &str) -> SpecificCharacter {
    SpecificCharacter {
        id: id.into(),
        team_default: true,
        name: format!("{id}_name"),
        visual: format!("actors\\{id}.OGF"),
        classes: vec![class.into()],
        community: "stalker".into(),
        rank: 100,
        reputation: 0,
        ..SpecificCharacter::default()
    }
}

/// The trader of section 8, as a description.
fn escape_trader() -> SpecificCharacter {
    SpecificCharacter {
        id: "escape_trader".into(),
        no_random: true,
        name: "escape_trader_name".into(),
        visual: "actors\\trader\\trader".into(),
        classes: vec!["escape_trader".into()],
        community: "trader".into(),
        rank: 330,
        reputation: 23,
        money: Money {
            min: 100_000,
            max: 100_000,
            infinitive: false,
        },
        ..SpecificCharacter::default()
    }
}

/// A trader record of profile `profile`, identity unset (1.1).
fn trader(profile: &str) -> ObjectRecord {
    ObjectRecord {
        header: SpawnHeader {
            section: "m_trader".into(),
            name: "escape_trader".into(),
            game_type: 0,
            after_game_type: 254,
            position: [0.0; 3],
            angles: [0.0; 3],
            respawn_time: 0,
            id: 0xFFFF,
            parent_id: 0xFFFF,
            phantom_id: 0xFFFF,
            flags: 0,
            version: 118,
            script_version: 6,
            client_data: Vec::new(),
            spawn_id: 84,
        },
        class_id: ClassId::new("AI_TRD_S").unwrap(),
        single_player: true,
        object: Object::Trader(Box::new(Trader {
            object: ObjectBlock {
                game_graph_vertex: 0,
                distance: 0.0,
                direct_control: 1,
                level_graph_node: 0,
                state_flags: 0,
                custom_data: String::new().into(),
                story_id: -1,
                spawn_story_id: -1,
            },
            visual: Visual {
                model: "actors\\trader\\trader".into(),
                flags: 0,
            },
            trader: TraderLike {
                money: 1_000_000,
                specific_character: String::new().into(),
                trader_flags: 0,
                character_profile: profile.into(),
                community: NO_COMMUNITY,
                rank: NOT_SET,
                reputation: NOT_SET,
                character_name: String::new().into(),
            },
        })),
    }
}

fn block(record: &ObjectRecord) -> &TraderLike {
    trader_like(record).unwrap()
}

fn model(record: &ObjectRecord) -> &str {
    record.object.visual().unwrap().model.as_str()
}

/// The communities of 1.3.
fn communities() -> Vec<(String, i32)> {
    [
        ("actor", 0),
        ("actor_dolg", 1),
        ("actor_freedom", 2),
        ("stalker", 5),
        ("monolith", 6),
        ("military", 7),
        ("killer", 8),
        ("ecolog", 9),
        ("dolg", 10),
        ("freedom", 11),
        ("bandit", 12),
        ("zombied", 13),
        ("stranger", 14),
        ("trader", 15),
        ("arena_enemy", 16),
    ]
    .map(|(n, i)| (n.to_owned(), i))
    .to_vec()
}

struct World {
    fake: Fake,
    system: Config,
    strings: StringTable,
    communities: Vec<(String, i32)>,
    random: Shared,
}

impl World {
    fn new(profiles: Vec<Profile>, all: Vec<SpecificCharacter>, seed: u32) -> Self {
        Self {
            fake: Fake {
                profiles: profiles.into_iter().map(|p| (p.id.clone(), p)).collect(),
                all,
            },
            system: ltx::parse(
                b"[stalker_names_stalker]\nname_cnt = 3\nlast_name_cnt = 5\n\
                  [no_names]\n",
            )
            .unwrap(),
            strings: StringTable::new(
                [
                    ("escape_trader_name", "Sidorovich"),
                    ("gen_name", "GENERATE_NAME_stalker"),
                    ("bad_name", "GENERATE_NAME_none"),
                    ("name_stalker_0", "A"),
                    ("name_stalker_1", "B"),
                    ("name_stalker_2", "C"),
                    ("lname_stalker_3", "D"),
                ]
                .map(|(k, v)| (k.to_owned(), v.to_owned())),
            ),
            communities: communities(),
            random: Shared::new(seed),
        }
    }

    fn identities(&mut self) -> Identities<'_> {
        Identities {
            descriptions: &mut self.fake,
            system: &self.system,
            strings: &self.strings,
            communities: &self.communities,
            random: &self.random,
        }
    }
}

/// The draws a generator of state `seed` gives.
fn draws(seed: u32, n: usize) -> Vec<u32> {
    let mut g = Generator::new(seed);
    (0..n).map(|_| g.next_k()).collect()
}

#[test]
fn the_trader_takes_his_profile_s_description_without_a_draw() {
    // Section 8, step 1.
    let mut w = World::new(vec![explicit("escape_trader")], vec![escape_trader()], 7);
    let mut r = trader("escape_trader");
    let mut in_use = InUse::default();
    let chosen = w
        .identities()
        .choose(&mut r, None, Some(&mut in_use))
        .unwrap();
    assert_eq!(chosen.as_deref(), Some("escape_trader"));
    let t = block(&r).clone();
    assert_eq!(t.specific_character.as_str(), "escape_trader");
    assert_eq!((t.community, t.rank, t.reputation), (13, 330, 23));
    assert_eq!(t.character_name.as_str(), "Sidorovich");
    assert_eq!(t.money, 100_000);
    assert_eq!(model(&r), "actors\\trader\\trader");
    assert_eq!(in_use.entries().collect::<Vec<_>>(), [("escape_trader", 1)]);
    // No draw at all.
    assert_eq!(w.random.get(), Generator::new(7));

    // Step 2: the supplies step sets the same description again.
    w.identities().supplies(&mut r, None, &mut in_use).unwrap();
    assert_eq!(block(&r), &t);
    assert_eq!(in_use.len(), 1);
    assert_eq!(w.random.get(), Generator::new(7));
}

#[test]
fn a_set_specific_character_is_kept() {
    let mut w = World::new(vec![explicit("escape_trader")], vec![escape_trader()], 1);
    let mut r = trader("escape_trader");
    if let Object::Trader(o) = &mut r.object {
        o.trader.specific_character = "other".into();
    }
    let chosen = w.identities().choose(&mut r, None, None).unwrap();
    assert_eq!(chosen.as_deref(), Some("other"));
    assert_eq!(block(&r).community, NO_COMMUNITY);
}

#[test]
fn a_random_choice_takes_an_unused_description_of_the_class() {
    let mut no_random = description("n", "novice");
    no_random.no_random = true;
    no_random.team_default = false;
    let all = vec![
        description("a", "novice"),
        no_random,
        description("other_class", "bandit"),
        description("b", "novice"),
        description("c", "novice"),
    ];
    let seed = 12_345;
    let mut w = World::new(vec![class("p", "novice")], all, seed);
    let mut in_use = InUse::from_saved([("b".to_owned(), 1)]);
    let mut r = trader("p");
    let chosen = w
        .identities()
        .choose(&mut r, None, Some(&mut in_use))
        .unwrap();
    // Candidates a, c (b in use); one draw.
    let a = draws(seed, 1)[0] as usize;
    let want = ["a", "c"][a % 2];
    assert_eq!(chosen.as_deref(), Some(want));
    assert_eq!(model(&r), format!("actors\\{want}"));
    // Community `stalker` is position 3; a trader has no team.
    assert_eq!(block(&r).community, 3);
    assert_eq!(block(&r).money, 1_000_000);
    assert!(in_use.contains(want) && in_use.contains("b"));
}

#[test]
fn all_in_use_falls_back_on_the_defaults() {
    let mut not_default = description("x", "novice");
    not_default.team_default = false;
    let all = vec![
        description("a", "novice"),
        not_default,
        description("b", "novice"),
    ];
    let seed = 99;
    let mut w = World::new(vec![class("p", "novice")], all, seed);
    let mut in_use = InUse::from_saved(["a", "x", "b"].map(|i| (i.to_owned(), 1)));
    let mut r = trader("p");
    let chosen = w
        .identities()
        .choose(&mut r, None, Some(&mut in_use))
        .unwrap();
    let a = draws(seed, 1)[0] as usize;
    assert_eq!(chosen.as_deref(), Some(["a", "b"][a % 2]));
    assert_eq!(in_use.len(), 3);
}

#[test]
fn a_class_without_defaults_is_an_assertion_failure() {
    let mut candidate = description("a", "novice");
    candidate.team_default = false;
    let mut w = World::new(vec![class("p", "novice")], vec![candidate], 1);
    let e = w
        .identities()
        .choose(&mut trader("p"), None, None)
        .unwrap_err();
    let check = FailedCheck::of(&e).unwrap();
    assert_eq!(
        check.description,
        "no default specific character set for class"
    );
    assert_eq!(check.argument.as_deref(), Some("novice"));
}

#[test]
fn setting_an_empty_id_is_an_assertion_failure() {
    // Section 3: before any step.
    let mut w = World::new(vec![], vec![], 1);
    let mut r = trader("p");
    let before = r.clone();
    let mut in_use = InUse::default();
    let e = w
        .identities()
        .set(&mut r, &description("", "novice"), None, Some(&mut in_use))
        .unwrap_err();
    assert!(FailedCheck::of(&e).is_some());
    assert_eq!(r, before);
    assert_eq!(in_use.len(), 0);
}

#[test]
fn rank_and_reputation_bound_the_candidates() {
    let ranked = |id: &str, rank: i32, reputation: i32| SpecificCharacter {
        rank,
        reputation,
        ..description(id, "novice")
    };
    let all = vec![
        ranked("far_rank", 110, 0),
        ranked("near", 91, -9),
        ranked("far_reputation", 100, -10),
    ];
    let mut p = class("p", "novice");
    (p.rank, p.reputation) = (100, 0);
    let mut w = World::new(vec![p], all, 5);
    let mut r = trader("p");
    let chosen = w.identities().choose(&mut r, None, None).unwrap();
    assert_eq!(chosen.as_deref(), Some("near"));
    assert_eq!((block(&r).rank, block(&r).reputation), (91, -9));
}

#[test]
fn a_generated_name_and_money_draw_in_order() {
    let mut d = description("a", "novice");
    d.name = "gen_name".into();
    d.money = Money {
        min: 600,
        max: 2_000,
        infinitive: false,
    };
    let seed = 4_242;
    let mut w = World::new(vec![explicit("a")], vec![d], seed);
    let mut r = trader("a");
    w.identities().choose(&mut r, None, None).unwrap();
    let k = draws(seed, 3);
    let first = ["A", "B", "C"][k[0] as usize % 3];
    let last = format!("lname_stalker_{}", k[1] % 5);
    let last = if last == "lname_stalker_3" {
        "D"
    } else {
        &last
    };
    assert_eq!(block(&r).character_name.as_str(), format!("{first} {last}"));
    assert_eq!(block(&r).money, 600 + k[2] % 1_400);
}

#[test]
fn a_generated_name_needs_its_section() {
    let mut d = description("a", "novice");
    d.name = "bad_name".into();
    let mut w = World::new(vec![explicit("a")], vec![d], 1);
    assert!(w.identities().choose(&mut trader("a"), None, None).is_err());
}

#[test]
fn a_set_community_and_rank_are_kept() {
    let mut w = World::new(vec![explicit("escape_trader")], vec![escape_trader()], 1);
    let mut r = trader("escape_trader");
    if let Object::Trader(o) = &mut r.object {
        (o.trader.community, o.trader.rank) = (5, 7);
    }
    w.identities().choose(&mut r, None, None).unwrap();
    assert_eq!((block(&r).community, block(&r).rank), (5, 7));
    assert_eq!(block(&r).reputation, 23);
}

#[test]
fn the_supplies_choice_excludes_the_spawn_file_choice() {
    // 4.2 step 3: the object's own spawn-file choice stays in use.
    let all = vec![description("a", "novice"), description("b", "novice")];
    let mut w = World::new(vec![class("p", "novice")], all, 3);
    let mut in_use = InUse::default();
    let mut r = trader("p");
    let first = w
        .identities()
        .spawn_file_choice(&mut r, &mut in_use)
        .unwrap()
        .unwrap();
    let first = first.trader.specific_character.to_string();
    let second = w.identities().supplies(&mut r, None, &mut in_use).unwrap();
    let second = second.unwrap();
    assert_ne!(first, second);
    assert_eq!(in_use.len(), 2);
    // A third choice finds none unused: the defaults.
    let third = w.identities().supplies(&mut r, None, &mut in_use).unwrap();
    assert!(["a", "b"].contains(&third.unwrap().as_str()));
}

#[test]
fn the_player_s_record_takes_its_identity() {
    // Section 7: the player's Trader-like block, read from the spawn file,
    // makes the same choice; it has no supplies choice (4.1).
    let actor = SpecificCharacter {
        id: "actor".into(),
        no_random: true,
        name: "actor_name".into(),
        visual: "actors\\hero\\stalker_novice".into(),
        community: "actor".into(),
        rank: 0,
        reputation: 0,
        ..SpecificCharacter::default()
    };
    let mut w = World::new(vec![explicit("actor")], vec![actor], 1);
    w.strings = StringTable::new([("actor_name".to_owned(), "Marked".to_owned())]);
    let mut in_use = InUse::default();
    let mut r = crate::character::tests::record();
    let choice = (w.identities().spawn_file_choice(&mut r, &mut in_use))
        .unwrap()
        .unwrap();
    let mut created = crate::character::tests::record();
    choice.apply(&mut created);
    let Object::Player(p) = &created.object else {
        panic!()
    };
    let t = p.trader.clone();
    assert_eq!(t.specific_character.as_str(), "actor");
    assert_eq!((t.community, t.rank, t.reputation, t.money), (0, 0, 0, 40));
    assert_eq!(t.character_name.as_str(), "Marked");
    let model = p.creature.visual.model.as_str();
    assert_eq!(model, "actors\\hero\\stalker_novice");
    assert_eq!(in_use.entries().collect::<Vec<_>>(), [("actor", 1)]);
    let supplied = w.identities().supplies(&mut created, None, &mut in_use);
    assert_eq!(supplied.unwrap(), None);
    let Object::Player(p) = &created.object else {
        panic!()
    };
    assert_eq!(p.trader, t);
    // No draw.
    assert_eq!(w.random.get(), Generator::new(1));
}

#[test]
fn model_names_lose_extensions() {
    assert_eq!(
        model_name("Actors\\Trader\\trader.ogf"),
        "actors\\trader\\trader"
    );
    assert_eq!(model_name("weapons\\vodka\\vodka"), "weapons\\vodka\\vodka");
}
