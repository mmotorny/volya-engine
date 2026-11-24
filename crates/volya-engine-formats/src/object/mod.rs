// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Object records: the spawn and update packets that `spawns\all.spawn` and
//! saved games store per object (docs/formats/sav.md, sections 7.1–7.5 and
//! 10), parsed into typed state per class id and written back to the same
//! bytes.
//!
//! A record's layout depends on the `class` value of its configuration
//! section (the class id), which the caller looks up; [`section`] reads the
//! section name first.

pub mod blocks;
pub mod kinds;

use std::io;

use crate::packet::{Reader, Writer, invalid, length16};

pub use blocks::*;
pub use kinds::*;

/// Message type of a spawn packet.
pub const SPAWN_MESSAGE: u16 = 1;
/// Message type of an update packet.
pub const UPDATE_MESSAGE: u16 = 0;
/// Spawn flags bit: "local"; set in saves.
pub const FLAG_LOCAL: u16 = 0x01;
/// Spawn flags bit: with direct control 1, the player-controlled entity.
pub const FLAG_PLAYER: u16 = 0x08;
/// Spawn flags bit: a version follows.
pub const FLAG_VERSION: u16 = 0x20;
/// The record version this game writes. Only it is parsed (our choice:
/// versions below 118 have legacy fields the spec does not describe).
pub const VERSION: u16 = 118;
/// The script version this game writes into saves (`current_server_entity_version`).
pub const SCRIPT_VERSION: u16 = 7;

/// Kinds of simulation objects (section 7.4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Player,
    Stalker,
    Trader,
    Monster,
    CrowOrPhantom,
    Vehicle,
    Helicopter,
    Item,
    Torch,
    Pda,
    Document,
    Outfit,
    Ammunition,
    Knife,
    MagazineWeapon,
    WeaponWithGrenadeLauncher,
    Shotgun,
    Zone,
    Anomaly,
    VisualAnomaly,
    TorridZone,
    LevelChanger,
    Restrictor,
    SmartZone,
    TeamBase,
    PlainVisual,
    StationaryGun,
    HangingLamp,
    PhysicsObject,
    Breakable,
    Climbable,
    SkeletonObject,
    OnOffGroup,
    MonsterGroup,
}

/// Class ids that cannot occur in a save, with the reason (section 7.4.3).
pub const NOT_IN_SAVES: &[(&str, &str)] = &[
    ("SPECT", "not a simulation object (spectator)"),
    ("AI_GRAPH", "not a simulation object (graph point)"),
    ("G_RPG7", "a temporary server object (rocket)"),
    ("G_FAKE", "a temporary server object (rocket)"),
    (
        "O_SWITCH",
        "a plain visual server object, not a simulation object",
    ),
    ("AI_RAT", "no object kind is registered"),
    ("AI_RAT_G", "no object kind is registered"),
    ("AI_SPGRP", "no object kind is registered"),
    ("EVENT", "no object kind is registered"),
    ("II_BTTCH", "no object kind is registered"),
    ("NW_ATTCH", "no object kind is registered"),
];

impl Kind {
    /// The kind of class id `class_id`. Ids of section 7.4.3 and unknown ids
    /// are errors.
    pub fn from_class_id(class_id: &str) -> io::Result<Self> {
        use Kind::*;
        Ok(match class_id {
            "O_ACTOR" => Player,
            "AI_STL" | "AI_STL_S" => Stalker,
            "AI_TRADE" | "AI_TRD_S" => Trader,
            "AI_FLESH" | "AI_HIMER" | "AI_DOG_R" | "AI_BLOOD" | "AI_BOAR" | "AI_DOG_B"
            | "AI_DOG_P" | "AI_DOG_F" | "AI_BURER" | "AI_GIANT" | "AI_CONTR" | "AI_POLTR"
            | "AI_ZOM" | "AI_FRACT" | "AI_SNORK" | "AI_CAT" | "AI_TUSH" => Monster,
            id if id.starts_with("SM_") => Monster,
            "AI_CROW" | "AI_PHANT" => CrowOrPhantom,
            "C_NIVA" | "SCRPTCAR" => Vehicle,
            "C_HLCPTR" | "C_HLCP_S" => Helicopter,
            "W_SCOPE" | "W_SILENC" | "W_GLAUNC" | "WP_SCOPE" | "II_MEDKI" | "II_BANDG"
            | "II_ANTIR" | "II_FOOD" | "II_BOTTL" | "II_ATTCH" | "II_BOLT" | "II_EXPLO"
            | "G_F1" | "G_RGD5" | "D_SIMDET" | "MP_PLBAG" | "ARTEFACT" | "SCRPTART" => Item,
            id if id.starts_with("AF_") => Item,
            "D_TORCH" | "TORCH_S" => Torch,
            "D_PDA" => Pda,
            "II_DOC" => Document,
            "EQU_SCIE" | "EQU_STLK" | "EQU_MLTR" | "EQU_EXO" | "E_STLK" => Outfit,
            "AMMO" | "A_VOG25" | "A_OG7B" | "A_M209" => Ammunition,
            "W_KNIFE" | "WP_KNIFE" => Knife,
            "W_WMAGAZ" | "W_LR300" | "W_HPSA" | "W_PM" | "W_FORT" | "W_BINOC" | "W_SVD"
            | "W_SVU" | "W_RPG7" | "W_VAL" | "W_VINT" | "W_WALTHR" | "W_USP45" | "WP_LR300"
            | "WP_BINOC" | "WP_SVD" | "WP_HPSA" | "WP_PM" | "WP_RPG7" | "WP_SVU" | "WP_USP45"
            | "WP_VAL" | "WP_VINT" | "WP_WALTH" => MagazineWeapon,
            "W_WMAGGL" | "W_FN2000" | "W_AK74" | "W_GROZA" | "WP_AK74" | "WP_GROZA" => {
                WeaponWithGrenadeLauncher
            }
            "W_SHOTGN" | "W_BM16" | "W_RG6" | "WP_BM16" | "WP_RG6" | "WP_SHOTG" => Shotgun,
            "Z_ZONE" => Zone,
            "Z_MBALD" | "Z_MINCER" | "Z_ACIDF" | "Z_GALANT" | "Z_RADIO" | "Z_DEAD" | "Z_NOGRAV"
            | "ZS_MBALD" | "ZS_GALAN" | "ZS_MINCE" => Anomaly,
            "Z_BFUZZ" | "Z_RUSTYH" | "Z_AMEBA" | "ZS_BFUZZ" => VisualAnomaly,
            "Z_TORRID" => TorridZone,
            "LVL_CHNG" => LevelChanger,
            "SCRIPTZN" | "SPACE_RS" | "SPC_RS_S" => Restrictor,
            "SMRTZONE" | "SMRTTRRN" | "RE_SPAWN" => SmartZone,
            "Z_TEAMBS" => TeamBase,
            "O_SEARCH" | "W_MOUNTD" | "SCRPTOBJ" | "O_INVBOX" => PlainVisual,
            "W_STMGUN" => StationaryGun,
            "O_HLAMP" => HangingLamp,
            "O_PHYSIC" | "P_DSTRBL" | "O_PHYS_S" => PhysicsObject,
            "O_BRKBL" => Breakable,
            "O_CLMBL" => Climbable,
            "P_SKELET" => SkeletonObject,
            "ON_OFF_G" => OnOffGroup,
            "AI_FLE_G" => MonsterGroup,
            id => {
                return Err(invalid(match NOT_IN_SAVES.iter().find(|(c, _)| *c == id) {
                    Some((_, reason)) => {
                        format!("class id {id} cannot occur in a save: {reason}")
                    }
                    None => format!("unknown class id {id:?}"),
                }));
            }
        })
    }

    /// The spec's name of the kind (section 7.4.2, section 15's counts).
    pub fn name(self) -> &'static str {
        use Kind::*;
        match self {
            Player => "player",
            Stalker => "stalker",
            Trader => "trader",
            Monster => "monster",
            CrowOrPhantom => "crow, phantom",
            Vehicle => "vehicle",
            Helicopter => "helicopter",
            Item => "item",
            Torch => "torch",
            Pda => "PDA",
            Document => "document",
            Outfit => "outfit",
            Ammunition => "ammunition",
            Knife => "knife",
            MagazineWeapon => "magazine weapon",
            WeaponWithGrenadeLauncher => "weapon with grenade launcher",
            Shotgun => "shotgun",
            Zone => "zone",
            Anomaly => "anomaly",
            VisualAnomaly => "visual anomaly",
            TorridZone => "torrid zone",
            LevelChanger => "level changer",
            Restrictor => "restrictor",
            SmartZone => "smart zone",
            TeamBase => "team base",
            PlainVisual => "plain visual object",
            StationaryGun => "stationary gun",
            HangingLamp => "hanging lamp",
            PhysicsObject => "physics object",
            Breakable => "breakable",
            Climbable => "climbable",
            SkeletonObject => "skeleton object",
            OnOffGroup => "online/offline group",
            MonsterGroup => "monster group",
        }
    }
}

/// What a record's layout depends on besides its bytes.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub class_id: &'a str,
    /// The record's script version (section 7.5).
    pub script_version: u16,
    /// Whether the game is single-player: decides the `ZS_*` additions.
    pub single_player: bool,
}

/// The fields of a spawn packet around the object state (section 7.2).
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnHeader {
    pub section: String,
    pub name: String,
    /// Meaning unestablished (0 in all.spawn).
    pub game_type: u8,
    /// The `u8` after the game type; meaning unestablished (254 in all.spawn).
    pub after_game_type: u8,
    pub position: [f32; 3],
    pub angles: [f32; 3],
    pub respawn_time: u16,
    /// 0 … 0xFFFE; 0xFFFF in all.spawn.
    pub id: u16,
    /// 0xFFFF for none.
    pub parent_id: u16,
    /// Meaning unestablished.
    pub phantom_id: u16,
    pub flags: u16,
    pub version: u16,
    pub script_version: u16,
    /// The online counterpart's state (section 8), kept as stored.
    pub client_data: Vec<u8>,
    /// Object index of the all.spawn record the object was created from.
    pub spawn_id: u16,
}

/// The section name of spawn packet `spawn` (its data, after the `u16`
/// length).
pub fn section(spawn: &[u8]) -> io::Result<String> {
    let mut r = Reader::new(spawn);
    message(&mut r, SPAWN_MESSAGE, "spawn")?;
    r.string()
}

fn message(r: &mut Reader, expected: u16, what: &str) -> io::Result<()> {
    match r.u16()? {
        m if m == expected => Ok(()),
        m => Err(invalid(format!("{what} packet: message type {m}"))),
    }
}

/// One object record, typed.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectRecord {
    pub header: SpawnHeader,
    /// The class id the record was read with; decides how it is saved.
    pub class_id: String,
    pub object: Object,
}

impl ObjectRecord {
    /// Parses a spawn packet and an update packet (each without its `u16`
    /// length) of an object of class id `class_id`. Both must be filled
    /// exactly.
    pub fn parse(
        spawn: &[u8],
        update: &[u8],
        class_id: &str,
        single_player: bool,
    ) -> io::Result<Self> {
        let kind = Kind::from_class_id(class_id)?;
        let mut r = Reader::new(spawn);
        message(&mut r, SPAWN_MESSAGE, "spawn")?;
        let section = r.string()?;
        let name = r.string()?;
        let game_type = r.u8()?;
        let after_game_type = r.u8()?;
        let position = r.float3()?;
        let angles = r.float3()?;
        let respawn_time = r.u16()?;
        let id = r.u16()?;
        let parent_id = r.u16()?;
        let phantom_id = r.u16()?;
        let flags = r.u16()?;
        if flags & FLAG_VERSION == 0 {
            return Err(invalid(format!("{name}: no version (flags {flags:#x})")));
        }
        let version = r.u16()?;
        if version != VERSION {
            return Err(invalid(format!("{name}: version {version}, not {VERSION}")));
        }
        let script_version = r.u16()?;
        let client_len = usize::from(r.u16()?);
        let client_data = r.take(client_len)?.to_vec();
        let spawn_id = r.u16()?;
        let size = usize::from(r.u16()?);
        // The data size counts itself; the state must not be empty (every
        // kind here: only the spectator, not a simulation object, may be).
        if size <= 2 {
            return Err(invalid(format!(
                "{name}: data size {size}: not successfully saved"
            )));
        }
        let mut state = Reader::new(r.take(size - 2)?);
        r.finish(&format!("{name}: spawn packet"))?;
        let mut u = Reader::new(update);
        message(&mut u, UPDATE_MESSAGE, "update")?;
        let cx = Context {
            class_id,
            script_version,
            single_player,
        };
        let object = Object::read(kind, &mut state, &mut u, &cx)
            .map_err(|e| invalid(format!("{name} ({class_id}): {e}")))?;
        state.finish(&format!("{name} ({class_id}): object state"))?;
        u.finish(&format!("{name} ({class_id}): update packet"))?;
        Ok(Self {
            header: SpawnHeader {
                section,
                name,
                game_type,
                after_game_type,
                position,
                angles,
                respawn_time,
                id,
                parent_id,
                phantom_id,
                flags,
                version,
                script_version,
                client_data,
                spawn_id,
            },
            class_id: class_id.to_owned(),
            object,
        })
    }

    /// Writes the spawn packet and the update packet (each without its `u16`
    /// length), field by field as read.
    pub fn write(&self) -> io::Result<(Vec<u8>, Vec<u8>)> {
        let h = &self.header;
        if h.flags & FLAG_VERSION == 0 || h.version != VERSION {
            return Err(invalid(format!(
                "{}: only version {VERSION} records with flags bit 0x20 are written",
                h.name
            )));
        }
        let mut state = Writer::new();
        let mut update = Writer::new();
        update.u16(UPDATE_MESSAGE);
        self.object.write(&mut state, &mut update)?;
        let state = state.into_inner();
        if state.is_empty() {
            return Err(invalid(format!("{}: empty object state", h.name)));
        }
        let mut w = Writer::new();
        w.u16(SPAWN_MESSAGE);
        w.string(&h.section)?;
        w.string(&h.name)?;
        w.u8(h.game_type);
        w.u8(h.after_game_type);
        w.float3(h.position);
        w.float3(h.angles);
        w.u16(h.respawn_time);
        w.u16(h.id);
        w.u16(h.parent_id);
        w.u16(h.phantom_id);
        w.u16(h.flags);
        w.u16(h.version);
        w.u16(h.script_version);
        w.u16(length16(h.client_data.len(), "client data")?);
        w.bytes(&h.client_data);
        w.u16(h.spawn_id);
        w.u16(length16(state.len() + 2, "object state")?);
        w.bytes(&state);
        let spawn = w.into_inner();
        length16(spawn.len(), "spawn packet")?;
        let update = update.into_inner();
        length16(update.len(), "update packet")?;
        Ok((spawn, update))
    }

    pub fn kind(&self) -> Kind {
        Kind::from_class_id(&self.class_id).expect("parsed with this class id")
    }

    /// Whether the object "can be saved" (section 7.1).
    pub fn can_be_saved(&self) -> bool {
        match self.class_id.as_str() {
            "O_ACTOR" => true,
            "II_BOLT" => false,
            "O_PHYSIC" | "P_DSTRBL" | "O_PHYS_S" | "P_SKELET" | "C_NIVA" | "SCRPTCAR"
            | "C_HLCPTR" | "C_HLCP_S" => self
                .object
                .skeleton()
                .is_some_and(|s| s.flags & SKELETON_NOT_SAVED == 0),
            _ => self.object.object_block().state_flags & STATE_CAN_BE_SAVED != 0,
        }
    }
}

#[cfg(test)]
mod tests;
