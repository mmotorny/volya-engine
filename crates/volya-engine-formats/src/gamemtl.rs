// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game material library `gamemtl.xr`: the materials in file order
//! with their ids, names, flags and physical values, as far as collision
//! needs them (docs/formats/level-cform.md section 3), and the material
//! pairs of chunk 0x1003 with their lists and the table that finds the pair
//! of two materials (docs/formats/material-pairs.md, S9).

use std::io;

use crate::chunks::Chunks;
use crate::cp1251;

/// Top-level chunks (section 3.1).
pub const VERSION_CHUNK: u32 = 0x1000;
pub const MATERIALS_CHUNK: u32 = 0x1002;
pub const PAIRS_CHUNK: u32 = 0x1003;
/// The only library version the game loads.
pub const VERSION: u16 = 1;

/// Chunks of a material record (section 3.1).
const ID_NAME_CHUNK: u32 = 0x1000;
const FLAGS_CHUNK: u32 = 0x1001;
const PHYSICS_CHUNK: u32 = 0x1002;
const FACTORS_CHUNK: u32 = 0x1003;
const LIQUID_CHUNK: u32 = 0x1004;
const DESCRIPTION_CHUNK: u32 = 0x1005;
const DAMAGE_CHUNK: u32 = 0x1006;

/// Material flags tested by collision (section 3.2).
pub const BOUNCY: u32 = 0x4;
pub const PASS_THROUGH: u32 = 0x80;
pub const DYNAMIC: u32 = 0x100;
/// Copied into triangle bit 14 at load (section 3.3).
pub const TRIANGLE_BIT_14: u32 = 0x400;
/// Copied into triangle bit 15 at load (section 3.3).
pub const TRIANGLE_BIT_15: u32 = 0x800;
pub const ACTOR_OBSTACLE: u32 = 0x1000;
pub const DEADLY: u32 = 0x1000_0000;
pub const LIQUID: u32 = 0x8000_0000;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One game material (section 3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct GameMaterial {
    pub id: u32,
    /// E.g. `materials\concrete`.
    pub name: String,
    pub description: String,
    pub flags: u32,
    pub friction: f32,
    pub damping: f32,
    pub spring: f32,
    pub bounce_start_speed: f32,
    pub bounce: f32,
    /// Factor 1 is the impact factor (section 4.4); 0, 2 and 3 are
    /// unestablished.
    pub factors: [f32; 4],
    pub liquid_factor: f32,
    pub damage_rate: f32,
}

impl Default for GameMaterial {
    /// The values a record starts from (section 3.1).
    fn default() -> Self {
        Self {
            id: 0xFFFF_FFFF,
            name: "unknown".into(),
            description: String::new(),
            flags: 0,
            friction: 1.0,
            damping: 1.0,
            spring: 1.0,
            bounce_start_speed: 0.0,
            bounce: 0.1,
            factors: [0.0, 1.0, 0.0, 0.0],
            liquid_factor: 1.0,
            damage_rate: 0.0,
        }
    }
}

impl GameMaterial {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    /// Factor 1 (section 4.4).
    pub fn impact_factor(&self) -> f32 {
        self.factors[1]
    }
}

fn floats<const N: usize>(data: &[u8], what: &str) -> io::Result<[f32; N]> {
    if data.len() < 4 * N {
        return Err(invalid(format!(
            "game material {what}: {} bytes, needs {}",
            data.len(),
            4 * N
        )));
    }
    Ok(std::array::from_fn(|i| {
        f32::from_le_bytes(data[4 * i..4 * i + 4].try_into().unwrap())
    }))
}

fn string_z(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    cp1251::decode(&data[..end])
}

fn material(record: &[u8], index: usize) -> io::Result<GameMaterial> {
    let chunks = Chunks::parse(record)?;
    let what = format!("game material {index}");
    let require = |id| -> io::Result<_> { chunks.require(id, &what)?.content() };
    let mut m = GameMaterial::default();
    let id_name = require(ID_NAME_CHUNK)?;
    if id_name.len() < 4 {
        return Err(invalid(format!(
            "{what}: id chunk of {} bytes",
            id_name.len()
        )));
    }
    m.id = u32::from_le_bytes(id_name[..4].try_into().unwrap());
    m.name = string_z(&id_name[4..]);
    if let Some(c) = chunks.find(DESCRIPTION_CHUNK) {
        m.description = string_z(&c.content()?);
    }
    let flags = require(FLAGS_CHUNK)?;
    m.flags = flags
        .get(..4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| invalid(format!("{what}: flags chunk of {} bytes", flags.len())))?;
    [
        m.friction,
        m.damping,
        m.spring,
        m.bounce_start_speed,
        m.bounce,
    ] = floats(&require(PHYSICS_CHUNK)?, "physics")?;
    m.factors = floats(&require(FACTORS_CHUNK)?, "factors")?;
    if let Some(c) = chunks.find(LIQUID_CHUNK) {
        [m.liquid_factor] = floats(&c.content()?, "liquid factor")?;
    }
    if let Some(c) = chunks.find(DAMAGE_CHUNK) {
        [m.damage_rate] = floats(&c.content()?, "damage rate")?;
    }
    Ok(m)
}

/// The materials of a `gamemtl.xr` file in file order: a material's index
/// is its position in the returned list (section 3.1). A version other than
/// 1 is an error (the game loads no materials then, so every level load
/// fails, section 3.3); so is a record missing a required chunk.
pub fn parse(file: &[u8]) -> io::Result<Vec<GameMaterial>> {
    let chunks = Chunks::parse(file)?;
    let version = chunks.require(VERSION_CHUNK, "gamemtl.xr")?.content()?;
    let version = version
        .get(..2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap()));
    if version != Some(VERSION) {
        return Err(invalid(format!(
            "gamemtl.xr: invalid library version {version:?}"
        )));
    }
    let list = chunks.require(MATERIALS_CHUNK, "gamemtl.xr")?.content()?;
    Chunks::parse(&list)?
        .iter()
        .enumerate()
        .map(|(i, c)| material(&c.content()?, i))
        .collect()
}

/// Sub-chunks of a pair (S9 section 2).
const PAIR_HEADER_CHUNK: u32 = 0x1000;
const PAIR_BREAKING_CHUNK: u32 = 0x1002;
const PAIR_STEP_CHUNK: u32 = 0x1003;
const PAIR_COLLIDE_CHUNK: u32 = 0x1005;
/// A pair's header: five `u32`.
const PAIR_HEADER_SIZE: usize = 20;
/// No link (S9 section 2).
pub const NO_LINK: u32 = 0xFFFF_FFFF;
/// A list has at most this many items (S9 section 3).
pub const MAX_ITEMS: usize = 4;
/// The longest item of a sound list, and of a particle or mark list (S9
/// section 3: the original's buffers of 128 and 256 bytes).
pub const MAX_SOUND_ITEM: usize = 127;
pub const MAX_NAME_ITEM: usize = 255;

/// One material pair (S9 section 2): its header kept as stored and its five
/// lists as stored strings. The link and the fifth word play no part (S9
/// section 6): a pair's lists are its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialPair {
    /// The ids of the two materials, unordered.
    pub first: u32,
    pub second: u32,
    /// The pair's own id.
    pub id: u32,
    /// The id of another pair, or [`NO_LINK`].
    pub link: u32,
    pub fifth_word: u32,
    pub breaking: String,
    pub step: String,
    pub collide: String,
    pub particles: String,
    pub marks: String,
}

impl MaterialPair {
    /// The canonical line of S9 section 7 (without its LF): pair id, the
    /// two materials, link and fifth word in decimal, then the five lists,
    /// tab-separated.
    pub fn canonical_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.id,
            self.first,
            self.second,
            self.link,
            self.fifth_word,
            self.breaking,
            self.step,
            self.collide,
            self.particles,
            self.marks
        )
    }

    /// The items of the step list (S9 section 3), checked as loading checks
    /// them.
    pub fn step_items(&self) -> io::Result<Vec<String>> {
        checked_items(&self.step, MAX_SOUND_ITEM, self.id)
    }

    pub fn breaking_items(&self) -> io::Result<Vec<String>> {
        checked_items(&self.breaking, MAX_SOUND_ITEM, self.id)
    }

    pub fn collide_items(&self) -> io::Result<Vec<String>> {
        checked_items(&self.collide, MAX_SOUND_ITEM, self.id)
    }

    pub fn particle_items(&self) -> io::Result<Vec<String>> {
        checked_items(&self.particles, MAX_NAME_ITEM, self.id)
    }

    pub fn mark_items(&self) -> io::Result<Vec<String>> {
        checked_items(&self.marks, MAX_NAME_ITEM, self.id)
    }
}

/// [`list_items`], with more than [`MAX_ITEMS`] items a fatal error (S9
/// section 3) and an item longer than `longest` rejected (not supported by
/// the original).
fn checked_items(list: &str, longest: usize, pair: u32) -> io::Result<Vec<String>> {
    let items = list_items(list);
    if items.len() > MAX_ITEMS {
        return Err(invalid(format!(
            "material pair {pair}: {} items in list '{list}', at most {MAX_ITEMS}",
            items.len()
        )));
    }
    // Bytes of the stored (single-byte) text, one per character.
    if let Some(long) = items.iter().find(|i| i.chars().count() > longest) {
        return Err(invalid(format!(
            "material pair {pair}: item '{long}' longer than {longest} bytes"
        )));
    }
    Ok(items)
}

/// The number of items of a list (S9 section 3): 0 for an empty string;
/// otherwise the commas counted from the start, stopping after a comma
/// directly followed by another (that one counted), plus 1 if anything
/// follows the last counted comma or none was counted.
pub fn item_count(list: &str) -> usize {
    let b = list.as_bytes();
    if b.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut after = 0;
    for (i, &c) in b.iter().enumerate() {
        if c == b',' {
            count += 1;
            after = i + 1;
            if b.get(i + 1) == Some(&b',') {
                break;
            }
        }
    }
    if count == 0 || after < b.len() {
        count += 1;
    }
    count
}

/// Item `k` of a list (S9 section 3): the text after the `k`-th comma (the
/// start for 0) up to the next comma or the end, without leading and
/// trailing bytes 0x00 – 0x20; empty past the last comma.
pub fn list_item(list: &str, k: usize) -> &str {
    let mut rest = list;
    for _ in 0..k {
        match rest.find(',') {
            Some(i) => rest = &rest[i + 1..],
            None => return "",
        }
    }
    let end = rest.find(',').unwrap_or(rest.len());
    rest[..end].trim_matches(|c: char| c <= ' ')
}

/// The items of a list (S9 section 3), [`item_count`] of them.
pub fn list_items(list: &str) -> Vec<String> {
    (0..item_count(list))
        .map(|k| list_item(list, k).to_owned())
        .collect()
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap())
}

/// The `N` zero-terminated strings in a row of a sub-chunk. A missing
/// terminator ends a string at the chunk's end, and missing strings are
/// empty (S9's review gap; our choice).
fn strings<const N: usize>(data: &[u8]) -> [String; N] {
    let mut rest = data;
    std::array::from_fn(|_| {
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        let s = cp1251::decode(&rest[..end]);
        rest = rest.get(end + 1..).unwrap_or(&[]);
        s
    })
}

fn pair(data: &[u8], index: usize) -> io::Result<MaterialPair> {
    let chunks = Chunks::parse(data)?;
    let what = format!("material pair {index}");
    // S9 section 2: each of the four is required (an assertion failure).
    let require = |id| -> io::Result<_> { chunks.require(id, &what)?.content() };
    let header = require(PAIR_HEADER_CHUNK)?;
    if header.len() < PAIR_HEADER_SIZE {
        // TODO(#325): a header shorter than 20 bytes is not stated (S9's
        // review gap); rejected.
        return Err(invalid(format!("{what}: header of {} bytes", header.len())));
    }
    let [breaking] = strings(&require(PAIR_BREAKING_CHUNK)?);
    let [step] = strings(&require(PAIR_STEP_CHUNK)?);
    let [collide, particles, marks] = strings(&require(PAIR_COLLIDE_CHUNK)?);
    Ok(MaterialPair {
        first: u32_at(&header, 0),
        second: u32_at(&header, 1),
        id: u32_at(&header, 2),
        link: u32_at(&header, 3),
        fifth_word: u32_at(&header, 4),
        breaking,
        step,
        collide,
        particles,
        marks,
    })
}

/// The material pairs of a `gamemtl.xr` file in stored order (S9 section
/// 1): every sub-chunk of chunk 0x1003, whatever its id. Without the chunk
/// there are none (S9's review gap: the game then reads no pairs and
/// reports nothing).
pub fn parse_pairs(file: &[u8]) -> io::Result<Vec<MaterialPair>> {
    let chunks = Chunks::parse(file)?;
    let Some(list) = chunks.find(PAIRS_CHUNK) else {
        return Ok(Vec::new());
    };
    let list = list.content()?;
    Chunks::parse(&list)?
        .iter()
        .enumerate()
        .map(|(i, c)| pair(&c.content()?, i))
        .collect()
}

/// The table that finds the pair of two materials (S9 section 5): *N* × *N*
/// entries, each a pair's position in the pair list or none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairTable {
    n: usize,
    entries: Vec<Option<u32>>,
}

impl PairTable {
    /// Built from the materials (file order) and the pairs (load order):
    /// each pair's two material ids become the indices of the first material
    /// with that id, and the pair is stored at both orders, a later pair
    /// replacing an earlier one. A pair naming an id no material has is
    /// rejected (S9 section 5: undefined in the original).
    pub fn build(materials: &[GameMaterial], pairs: &[MaterialPair]) -> io::Result<Self> {
        let n = materials.len();
        let index = |id: u32, pair: &MaterialPair| {
            materials.iter().position(|m| m.id == id).ok_or_else(|| {
                invalid(format!(
                    "material pair {}: no material has id {id}",
                    pair.id
                ))
            })
        };
        let mut entries = vec![None; n * n];
        for (k, p) in pairs.iter().enumerate() {
            let (a, b) = (index(p.first, p)?, index(p.second, p)?);
            let k = Some(u32::try_from(k).map_err(|_| invalid("too many material pairs"))?);
            entries[a * n + b] = k;
            entries[b * n + a] = k;
        }
        Ok(Self { n, entries })
    }

    /// The number of materials *N*.
    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// The pair (its position in the pair list) of the materials at indices
    /// `a` and `b`, in either order; `None` for no pair. An index ≥ *N* is
    /// a fatal error (S9 section 5), returned as `Err`.
    pub fn pair(&self, a: u32, b: u32) -> Result<Option<usize>, String> {
        let (a, b) = (a as usize, b as usize);
        if a >= self.n || b >= self.n {
            return Err(format!(
                "material pair lookup: index {} of {} materials",
                a.max(b),
                self.n
            ));
        }
        Ok(self.entries[a * self.n + b].map(|k| k as usize))
    }
}

#[cfg(test)]
pub(crate) fn record(id: u32, name: &str, flags: u32, physics: [f32; 5]) -> Vec<u8> {
    use crate::chunks::chunk;
    let mut id_name = id.to_le_bytes().to_vec();
    id_name.extend(name.as_bytes());
    id_name.push(0);
    let f = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let mut out = chunk(ID_NAME_CHUNK, &id_name);
    out.extend(chunk(FLAGS_CHUNK, &flags.to_le_bytes()));
    out.extend(chunk(PHYSICS_CHUNK, &f(&physics)));
    out.extend(chunk(FACTORS_CHUNK, &f(&[0.0, 100.0, 0.0, 0.0])));
    out
}

#[cfg(test)]
pub(crate) fn library(records: &[Vec<u8>]) -> Vec<u8> {
    library_with_pairs(records, &[])
}

/// A library with material records and pair sub-chunks.
#[cfg(test)]
pub(crate) fn library_with_pairs(records: &[Vec<u8>], pairs: &[Vec<u8>]) -> Vec<u8> {
    use crate::chunks::chunk;
    let mut out = chunk(VERSION_CHUNK, &VERSION.to_le_bytes());
    out.extend(chunk(0x1001, &[0; 8]));
    let mut list = Vec::new();
    for (i, r) in records.iter().enumerate() {
        list.extend(chunk(i as u32, r));
    }
    out.extend(chunk(MATERIALS_CHUNK, &list));
    let mut list = Vec::new();
    for (i, p) in pairs.iter().enumerate() {
        list.extend(chunk(i as u32, p));
    }
    out.extend(chunk(PAIRS_CHUNK, &list));
    out
}

/// A pair's payload: its header and lists (breaking, step, collide sounds,
/// particles, marks).
#[cfg(test)]
pub(crate) fn pair_record(header: [u32; 5], lists: [&str; 5]) -> Vec<u8> {
    use crate::chunks::chunk;
    let z = |s: &str| {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    };
    let h: Vec<u8> = header.iter().flat_map(|w| w.to_le_bytes()).collect();
    let mut out = chunk(PAIR_HEADER_CHUNK, &h);
    out.extend(chunk(PAIR_BREAKING_CHUNK, &z(lists[0])));
    out.extend(chunk(PAIR_STEP_CHUNK, &z(lists[1])));
    let mut c = z(lists[2]);
    c.extend(z(lists[3]));
    c.extend(z(lists[4]));
    out.extend(chunk(PAIR_COLLIDE_CHUNK, &c));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::chunk;

    #[test]
    fn parses_materials_in_file_order() {
        let mut water = record(
            11,
            "materials\\water",
            0x8000_0082,
            [1.0, 1.0, 1.0, 0.0, 0.1],
        );
        water.extend(chunk(LIQUID_CHUNK, &0.8f32.to_le_bytes()));
        water.extend(chunk(DESCRIPTION_CHUNK, &[0xC2, 0xEE, 0xE4, 0xE0, 0]));
        let mut death = record(92, "materials\\death", 0x1000_0080, [1.0; 5]);
        death.extend(chunk(DAMAGE_CHUNK, &30f32.to_le_bytes()));
        let file = library(&[
            record(0, "default", 0x6000_0010, [1.0, 1.0, 1.0, 1.5, 1.0]),
            water,
            death,
        ]);
        let m = parse(&file).unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!((m[0].id, m[0].name.as_str()), (0, "default"));
        assert_eq!((m[0].bounce_start_speed, m[0].bounce), (1.5, 1.0));
        assert_eq!(m[0].liquid_factor, 1.0);
        assert_eq!(m[1].liquid_factor, 0.8);
        assert_eq!(m[1].description, "Вода");
        assert!(m[1].has(LIQUID) && m[1].has(PASS_THROUGH) && !m[1].has(DYNAMIC));
        assert_eq!((m[2].damage_rate, m[2].impact_factor()), (30.0, 100.0));
        assert!(m[2].has(DEADLY));
    }

    #[test]
    fn rejects_bad_version_and_missing_chunks() {
        let mut file = chunk(VERSION_CHUNK, &2u16.to_le_bytes());
        file.extend(chunk(MATERIALS_CHUNK, &[]));
        assert!(parse(&file).is_err());
        let mut no_flags = Vec::new();
        no_flags.extend(chunk(ID_NAME_CHUNK, b"\0\0\0\0x\0"));
        assert!(parse(&library(&[no_flags])).is_err());
    }

    #[test]
    fn counts_list_items_as_s9_does() {
        // S9 section 3's examples.
        assert_eq!(item_count(""), 0);
        assert_eq!(list_items("a"), ["a"]);
        assert_eq!(list_items("a,b"), ["a", "b"]);
        assert_eq!(list_items("a,b,"), ["a", "b"]);
        assert_eq!(list_items("a,,b"), ["a", ""]);
        assert_eq!(list_items(" a , b\t"), ["a", "b"]);
        assert_eq!(item_count(","), 1);
        assert_eq!(item_count("a,b,c,d,e"), 5);
    }

    #[test]
    fn parses_pairs_and_finds_them_in_either_order() {
        let materials = [
            record(0, "default", 0, [1.0; 5]),
            record(7, "materials\\earth", 0, [1.0; 5]),
            record(90, "creatures\\actor", 0, [1.0; 5]),
        ];
        // S9 section 7's pair 1211.
        let step = "material\\actor\\step\\earth1,material\\actor\\step\\earth2,\
                    material\\actor\\step\\earth3,material\\actor\\step\\earth4";
        let pairs = [
            pair_record([90, 7, 1211, NO_LINK, NO_LINK], ["", step, "", "", ""]),
            pair_record([0, 90, 1204, 1211, 0x30], ["b", "s", "c", "p1,p2", "m"]),
        ];
        let file = library_with_pairs(&materials, &pairs);
        let m = parse(&file).unwrap();
        let p = parse_pairs(&file).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].first, p[0].second, p[0].id), (90, 7, 1211));
        let items = p[0].step_items().unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[3], "material\\actor\\step\\earth4");
        assert!(p[0].collide_items().unwrap().is_empty());
        assert_eq!((p[1].link, p[1].fifth_word), (1211, 0x30));
        assert_eq!(p[1].particle_items().unwrap(), ["p1", "p2"]);
        assert_eq!(p[1].mark_items().unwrap(), ["m"]);
        assert_eq!(
            p[1].canonical_line(),
            "1204\t0\t90\t1211\t48\tb\ts\tc\tp1,p2\tm"
        );
        let table = PairTable::build(&m, &p).unwrap();
        assert_eq!(table.len(), 3);
        assert_eq!(table.pair(2, 1), Ok(Some(0)));
        assert_eq!(table.pair(1, 2), Ok(Some(0)));
        assert_eq!(table.pair(0, 2), Ok(Some(1)));
        assert_eq!(table.pair(0, 0), Ok(None));
        assert!(table.pair(3, 0).is_err());
    }

    #[test]
    fn rejects_what_loading_refuses() {
        let materials = [record(0, "default", 0, [1.0; 5])];
        // More than 4 items.
        let five = pair_record([0, 0, 1, NO_LINK, NO_LINK], ["", "a,b,c,d,e", "", "", ""]);
        let p = parse_pairs(&library_with_pairs(&materials, &[five])).unwrap();
        assert!(p[0].step_items().is_err());
        // An id no material has.
        let stray = pair_record([0, 5, 1, NO_LINK, NO_LINK], ["", "", "", "", ""]);
        let m = parse(&library(&materials)).unwrap();
        let p = parse_pairs(&library_with_pairs(&materials, &[stray])).unwrap();
        assert!(PairTable::build(&m, &p).is_err());
        // A missing sub-chunk.
        let mut missing = chunk(PAIR_HEADER_CHUNK, &[0; 20]);
        missing.extend(chunk(PAIR_BREAKING_CHUNK, b"\0"));
        assert!(parse_pairs(&library_with_pairs(&materials, &[missing])).is_err());
        // No pair chunk at all: no pairs.
        let mut bare = chunk(VERSION_CHUNK, &VERSION.to_le_bytes());
        bare.extend(chunk(MATERIALS_CHUNK, &[]));
        assert!(parse_pairs(&bare).unwrap().is_empty());
    }
}
