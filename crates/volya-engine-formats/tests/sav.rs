// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Object records and the save container against the Russian edition's
//! `spawns\all.spawn` and configuration (BYOA), checking the verification
//! facts of docs/formats/sav.md (section 15). Skips when
//! `VOLYA_GOG_INSTALLER` is unset. No saved game of the original exists, so
//! the container is checked only against our own reader and an independent
//! LZO1X decoder, not against the game.

#[path = "support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::ops::ControlFlow;
use std::time::Instant;

use volya_engine_formats::chunks::Chunks;
use volya_engine_formats::db::{Archive, Entry, Scrambling};
use volya_engine_formats::ltx::{Builder, Config};
use volya_engine_formats::object::{
    FLAG_LOCAL, Kind, NOT_IN_SAVES, Object, ObjectRecord, PlayerPhysics, SKELETON_BONE_STATE,
    SKELETON_NOT_SAVED, STATE_CAN_BE_SAVED, SmartZoneScript, section,
};
use volya_engine_formats::sav::{
    self, GameTime, RawRecord, SIMULATION_VERSION, Save, SpawnBinding, empty_registries, game_time,
};
use volya_engine_formats::{cp1251, lzo1x, spawn};

/// The combined game-data tree of the archives: the later archive wins.
struct Tree {
    archives: Vec<Archive<BufReader<File>>>,
    files: HashMap<String, (usize, Entry)>,
}

impl Tree {
    fn open() -> Option<Self> {
        let cached = gog_archives::archives()?;
        let mut archives = Vec::new();
        let mut files = HashMap::new();
        for (i, a) in cached.iter().enumerate() {
            let archive = Archive::open(
                BufReader::new(File::open(&a.file).unwrap()),
                Scrambling::for_path(&a.path),
            )
            .unwrap();
            for e in archive.entries().iter().filter(|e| !e.is_dir()) {
                files.insert(e.path.to_ascii_lowercase(), (i, e.clone()));
            }
            archives.push(archive);
        }
        Some(Self { archives, files })
    }

    fn read(&mut self, path: &str) -> Vec<u8> {
        let (i, entry) = self
            .files
            .get(&path.to_ascii_lowercase())
            .unwrap_or_else(|| panic!("{path} missing"))
            .clone();
        self.archives[i].read(&entry).unwrap()
    }

    /// The configuration rooted at `path`, includes resolved relative to the
    /// including file (ltx-config.md, section 4).
    fn config(&mut self, path: &str) -> Config {
        fn add(tree: &mut Tree, path: &str, builder: &mut Builder) {
            let text = tree.read(path);
            let dir = &path[..path.rfind('\\').map_or(0, |i| i + 1)];
            builder
                .add_file(&text, &mut |include, builder| {
                    add(tree, &format!("{dir}{}", cp1251::decode(include)), builder);
                    Ok(())
                })
                .unwrap();
        }
        let mut builder = Builder::new();
        add(self, path, &mut builder);
        builder.finish()
    }
}

/// The class id of every section that has one.
fn class_ids(config: &Config) -> HashMap<String, String> {
    config
        .sections()
        .filter_map(|s| {
            let class = s.get("class")??;
            Some((s.name().to_ascii_lowercase(), class.trim().to_owned()))
        })
        .collect()
}

#[test]
fn object_records_and_saves_from_game_data() {
    let Some(mut tree) = Tree::open() else {
        return;
    };
    let file = tree.read("spawns\\all.spawn");
    assert_eq!(file.len(), 4_474_073);
    let config = tree.config("config\\system.ltx");
    let classes = class_ids(&config);

    // Every class id of the configuration is in section 7.4.2 or 7.4.3.
    let distinct: BTreeSet<&str> = classes.values().map(String::as_str).collect();
    assert_eq!(distinct.len(), 127);
    for id in &distinct {
        assert!(
            Kind::from_class_id(id).is_ok() || NOT_IN_SAVES.iter().any(|(c, _)| c == id),
            "{id}"
        );
    }

    let start = Instant::now();
    let mut kinds: BTreeMap<Kind, usize> = BTreeMap::new();
    let mut update_lengths: BTreeMap<(Kind, usize), usize> = BTreeMap::new();
    let mut flags: BTreeMap<u16, usize> = BTreeMap::new();
    let mut records = Vec::new();
    let mut player = None;
    let mut scripted_anomalies = 0;
    spawn::for_each_record(&file, |r| {
        let section = section(r.spawn)?;
        let class_id = &classes[&section.to_ascii_lowercase()];
        // all.spawn is the single-player world (our choice for the ZS_ fields).
        let record = ObjectRecord::parse(r.spawn, r.update, class_id, true)
            .unwrap_or_else(|e| panic!("object {}: {e}", r.index));
        // The ZS_ records end exactly only with their single-player field.
        if class_id.starts_with("ZS_") {
            assert!(ObjectRecord::parse(r.spawn, r.update, class_id, false).is_err());
            scripted_anomalies += 1;
        }
        let kind = record.kind();
        *kinds.entry(kind).or_default() += 1;
        *update_lengths.entry((kind, r.update.len())).or_default() += 1;

        // Round trip: the same bytes.
        let (s, u) = record.write().unwrap();
        assert_eq!(s, r.spawn, "object {}: spawn packet", r.index);
        assert_eq!(u, r.update, "object {}: update packet", r.index);

        let h = &record.header;
        assert_eq!(
            (h.game_type, h.after_game_type, h.respawn_time),
            (0, 254, 0),
            "object {}",
            r.index
        );
        assert_eq!((h.id, h.parent_id, h.phantom_id), (0xFFFF, 0xFFFF, 0xFFFF));
        assert_eq!((h.version, h.script_version), (118, 6));
        assert!(h.client_data.is_empty());
        assert_eq!(u32::from(h.spawn_id), r.index);
        *flags.entry(h.flags).or_default() += 1;
        assert_ne!(
            record.object.object_block().state_flags & STATE_CAN_BE_SAVED,
            0
        );
        if let Some(s) = record.object.skeleton() {
            assert_eq!(s.flags & (SKELETON_BONE_STATE | SKELETON_NOT_SAVED), 0);
        }
        assert!(record.can_be_saved(), "object {}", r.index);
        assert_ne!(record.class_id, "II_BOLT");
        match &record.object {
            Object::SmartZone(z) => {
                if let SmartZoneScript::SmartTerrain(t) = &z.script {
                    assert_eq!(t.working, None);
                }
            }
            Object::Stalker(s) => {
                assert_eq!(s.equipment_preferences.len(), 5);
                assert_eq!(s.main_weapon_preferences.len(), 4);
                let prefs = s
                    .equipment_preferences
                    .iter()
                    .chain(&s.main_weapon_preferences);
                assert!(prefs.into_iter().all(|&v| v <= 2));
            }
            Object::Player(p) => {
                assert_eq!(r.index, 63);
                assert_eq!(h.flags, 0x29);
                assert_eq!(r.spawn.len(), 298);
                // Data size 222: the state and its own two bytes.
                assert_eq!(spawn::parse_spawn_packet(63, r.spawn)?.data.len() + 2, 222);
                assert_eq!(p.creature.object.game_graph_vertex, 4);
                assert_eq!(p.creature.object.distance, 0.7);
                assert_eq!(p.creature.object.direct_control, 1);
                assert_eq!(p.creature.health, 1.0);
                assert_eq!(p.update.physics, PlayerPhysics::None);
                player = Some(record.clone());
            }
            _ => {}
        }
        records.push((r.index, record));
        Ok(ControlFlow::Continue(()))
    })
    .unwrap();
    eprintln!(
        "parsed and rewrote {} records in {:.1?}",
        records.len(),
        start.elapsed()
    );
    assert_eq!(records.len(), 8_641);
    assert_eq!(scripted_anomalies, 764);

    use Kind::*;
    let expected = BTreeMap::from([
        (PhysicsObject, 1_905),
        (Anomaly, 1_350),
        (Stalker, 1_073),
        (Restrictor, 1_027),
        (HangingLamp, 745),
        (Item, 459),
        (Breakable, 406),
        (SmartZone, 376),
        (Monster, 276),
        (PlainVisual, 256),
        (Climbable, 232),
        (CrowOrPhantom, 197),
        (Ammunition, 96),
        (VisualAnomaly, 92),
        (Outfit, 44),
        (LevelChanger, 37),
        (Helicopter, 27),
        (MagazineWeapon, 17),
        (WeaponWithGrenadeLauncher, 13),
        (Vehicle, 6),
        (Shotgun, 4),
        (Player, 1),
        (Trader, 1),
        (TorridZone, 1),
    ]);
    assert_eq!(kinds, expected);
    // Update-packet lengths, the 2-byte message type included.
    for (&(kind, len), &n) in &update_lengths {
        let ok = match kind {
            Player => len == 63,
            Stalker => len == 55,
            Monster => len == 54,
            CrowOrPhantom => len == 42,
            Item => (len, n) == (3, 415) || (len, n) == (22, 44),
            Outfit => (len, n) == (4, 42) || (len, n) == (23, 2),
            Ammunition => len == 5,
            MagazineWeapon => len == 12,
            WeaponWithGrenadeLauncher | Shotgun => len == 13,
            _ => len == 2,
        };
        assert!(ok, "{}: {n} updates of {len} bytes", kind.name());
    }
    assert_eq!(
        flags,
        BTreeMap::from([(0x21, 8_629), (0x23, 11), (0x29, 1)])
    );

    // The spawn binding of a save of this edition (section 15).
    let identifier = spawn::identifier(&file).unwrap();
    assert_eq!(
        identifier,
        [
            0x9a, 0x80, 0xc8, 0x32, 0x91, 0x73, 0x8f, 0x42, 0xb3, 0x3b, 0x35, 0x90, 0x3c, 0xea,
            0xd5, 0x44
        ]
    );
    let binding = SpawnBinding::new("all", identifier, records.len() as u32);
    let binding_bytes = binding.write().unwrap();
    assert_eq!(binding_bytes.len(), 69_164);
    let binding_chunks = Chunks::parse(&binding_bytes).unwrap();
    let name = binding_chunks.find(0).unwrap().payload;
    assert_eq!(&name[..4], b"all\0");
    assert_eq!(name[4..], identifier);
    assert_eq!(binding_chunks.find(1).unwrap().payload.len(), 69_128);

    // A save built from all.spawn's records as section 11.3 allows: the
    // player first with the smallest id, ids assigned, flags bit 0x01 set.
    let player = player.unwrap();
    let others = records
        .iter()
        .map(|(_, r)| r)
        .filter(|r| r.kind() != Player);
    let objects = std::iter::once(&player)
        .chain(others)
        .zip(0..)
        .map(|(record, id)| {
            let mut record = record.clone();
            record.header.id = id;
            record.header.flags |= FLAG_LOCAL;
            let (spawn, update) = record.write().unwrap();
            RawRecord { spawn, update }
        })
        .collect();
    let save = Save {
        simulation_version: SIMULATION_VERSION,
        game_time: GameTime {
            time: game_time(2012, 5, 1, 5, 30, 0, 0),
            time_factor: 10.0,
            normal_time_factor: 10.0,
        },
        spawn_binding: binding,
        objects,
        registries: empty_registries(),
    };
    let start = Instant::now();
    let body = save.write_body().unwrap();
    let file = save.write().unwrap();
    eprintln!(
        "save: body {} bytes, file {} bytes, written in {:.1?}",
        body.len(),
        file.len(),
        start.elapsed()
    );
    assert_eq!(file[..8], [0xff, 0xff, 0xff, 0xff, 3, 0, 0, 0]);
    // An independent LZO1X decoder reads our stream.
    assert_eq!(
        lzokay_native::decompress_all(&file[12..], Some(body.len())).unwrap(),
        body
    );
    let start = Instant::now();
    let read = Save::read(&file).unwrap();
    eprintln!("save read in {:.1?}", start.elapsed());
    assert_eq!(read, save);
    assert!(sav::is_valid(&file));
    // Every record parses back typed; the player is first, with bits 0x01
    // and 0x08 and direct control 1.
    for (i, r) in read.objects.iter().enumerate() {
        let section = section(&r.spawn).unwrap();
        let record = ObjectRecord::parse(
            &r.spawn,
            &r.update,
            &classes[&section.to_ascii_lowercase()],
            true,
        )
        .unwrap();
        assert_eq!(usize::from(record.header.id), i);
        assert_eq!(record.header.flags & 0x21, 0x21);
        if i == 0 {
            assert_eq!(record.header.section, "actor");
            assert_eq!(record.header.flags & 0x09, 0x09);
            assert_eq!(record.object.object_block().direct_control, 1);
        }
    }

    // The compressor on the whole spawn file, checked by both decoders.
    let all = tree.read("spawns\\all.spawn");
    let start = Instant::now();
    let packed = lzo1x::compress(&all);
    let packed_in = start.elapsed();
    let start = Instant::now();
    assert_eq!(lzo1x::decompress(&packed, all.len()).unwrap(), all);
    eprintln!(
        "all.spawn: {} → {} bytes, compressed in {packed_in:.1?}, decompressed in {:.1?}",
        all.len(),
        packed.len(),
        start.elapsed()
    );
    assert_eq!(
        lzokay_native::decompress_all(&packed, Some(all.len())).unwrap(),
        all
    );
}
