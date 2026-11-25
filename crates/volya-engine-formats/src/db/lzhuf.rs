// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Directory decompression: LZSS with adaptive Huffman coding, the public
//! "LZHUF" method with this game's rescaling threshold
//! (docs/formats/db-archive.md, section 5).

use std::io;

const RING_SIZE: usize = 4096;
const MAX_MATCH: usize = 60;
const MIN_MATCH: usize = 3;
/// Literals 0–255 plus match lengths 3–60.
const SYMBOLS: usize = 256 + MAX_MATCH - MIN_MATCH + 1;
const SLOTS: usize = 2 * SYMBOLS - 1;
const ROOT: usize = SLOTS - 1;
/// Node contents at or above this are leaves holding `content - LEAF`.
const LEAF: u16 = SLOTS as u16;
/// The model is rebuilt when the root's frequency reaches this value.
const REBUILD_AT: u32 = 0x4000;

/// Decodes a descrambled directory payload: a 32-bit decoded length, then
/// the bit stream.
pub fn decode(input: &[u8]) -> io::Result<Vec<u8>> {
    let Some((len, stream)) = input.split_first_chunk::<4>() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "directory stream shorter than its length field",
        ));
    };
    let len = u32::from_le_bytes(*len) as usize;
    // A bit stream of b bytes yields at most 8b symbols of up to 60 bytes;
    // past its end, bits read as 0 (a stream of literal or match symbols
    // that never ends), so bound the claimed length by the input size.
    if len > (stream.len() + 1).saturating_mul(8 * MAX_MATCH) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "directory stream claims {len} bytes from {} input bytes",
                input.len()
            ),
        ));
    }
    let mut bits = BitReader::new(stream);
    let mut model = Model::new();
    let mut ring = [b' '; RING_SIZE];
    let mut pos = RING_SIZE - MAX_MATCH;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let symbol = model.read_symbol(&mut bits);
        model.update(symbol);
        if symbol < 256 {
            let byte = symbol as u8;
            out.push(byte);
            ring[pos] = byte;
            pos = (pos + 1) % RING_SIZE;
        } else {
            let length = symbol - 256 + MIN_MATCH;
            let distance = read_distance(&mut bits);
            let mut src = (pos + RING_SIZE - distance - 1) % RING_SIZE;
            for _ in 0..length {
                if out.len() == len {
                    break;
                }
                let byte = ring[src];
                out.push(byte);
                ring[pos] = byte;
                pos = (pos + 1) % RING_SIZE;
                src = (src + 1) % RING_SIZE;
            }
        }
    }
    Ok(out)
}

/// Reads a match distance in 0..4096.
fn read_distance(bits: &mut BitReader) -> usize {
    let mut p = bits.read(8);
    let (upper, width) = distance_tables();
    let upper = usize::from(upper[p]);
    for _ in 2..width[p] {
        p = (p << 1) | bits.read(1);
    }
    upper * 64 + p % 64
}

/// The fixed `UPPER` and `WIDTH` tables, built from their run descriptions.
fn distance_tables() -> &'static ([u8; 256], [u8; 256]) {
    static TABLES: std::sync::OnceLock<([u8; 256], [u8; 256])> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        let fill = |runs: &[(u8, u8, usize)]| {
            // (first value, last value, entries per value)
            let mut table = [0u8; 256];
            let mut i = 0;
            for &(first, last, count) in runs {
                for value in first..=last {
                    table[i..i + count].fill(value);
                    i += count;
                }
            }
            assert_eq!(i, 256);
            table
        };
        let upper = fill(&[
            (0, 0, 32),
            (1, 3, 16),
            (4, 11, 8),
            (12, 23, 4),
            (24, 47, 2),
            (48, 63, 1),
        ]);
        let width = fill(&[
            (3, 3, 32),
            (4, 4, 48),
            (5, 5, 64),
            (6, 6, 48),
            (7, 7, 48),
            (8, 8, 16),
        ]);
        (upper, width)
    })
}

/// Reads bits from the most significant bit of each byte down; bits past the
/// end read as 0.
struct BitReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn read(&mut self, count: u32) -> usize {
        let mut value = 0;
        for _ in 0..count {
            let byte = self.data.get(self.bit / 8).copied().unwrap_or(0);
            value = (value << 1) | usize::from((byte >> (7 - self.bit % 8)) & 1);
            self.bit += 1;
        }
        value
    }
}

/// The adaptive Huffman model: slots in non-decreasing frequency order.
struct Model {
    /// Slot frequencies, plus the position after the root at `0xFFFF`.
    freq: [u32; SLOTS + 1],
    /// Leaf `LEAF + symbol`, or internal node with children `c` and `c + 1`.
    node: [u16; SLOTS],
    /// Parent slot of every slot but the root.
    parent: [u16; SLOTS],
    /// Slot of each symbol's leaf.
    leaf: [u16; SYMBOLS],
}

impl Model {
    fn new() -> Self {
        let mut model = Self {
            freq: [0; SLOTS + 1],
            node: [0; SLOTS],
            parent: [0; SLOTS],
            leaf: [0; SYMBOLS],
        };
        for i in 0..SYMBOLS {
            model.freq[i] = 1;
            model.node[i] = LEAF + i as u16;
        }
        for j in SYMBOLS..SLOTS {
            let c = 2 * (j - SYMBOLS);
            model.freq[j] = model.freq[c] + model.freq[c + 1];
            model.node[j] = c as u16;
        }
        model.freq[SLOTS] = 0xFFFF;
        model.relink_all();
        model
    }

    fn read_symbol(&self, bits: &mut BitReader) -> usize {
        let mut slot = ROOT;
        loop {
            let content = self.node[slot];
            if content >= LEAF {
                return usize::from(content - LEAF);
            }
            slot = usize::from(content) + bits.read(1);
        }
    }

    /// Adapts the model after `symbol` was read.
    fn update(&mut self, symbol: usize) {
        if self.freq[ROOT] == REBUILD_AT {
            self.rebuild();
        }
        let mut slot = usize::from(self.leaf[symbol]);
        loop {
            self.freq[slot] += 1;
            let f = self.freq[slot];
            if f > self.freq[slot + 1] {
                let mut last = slot + 1;
                while self.freq[last + 1] < f {
                    last += 1;
                }
                self.freq.swap(slot, last);
                self.node.swap(slot, last);
                self.relink(slot);
                self.relink(last);
                slot = last;
            }
            if slot == ROOT {
                break;
            }
            slot = usize::from(self.parent[slot]);
        }
    }

    /// Points whatever `slot` holds (a leaf, or both children) back at it.
    fn relink(&mut self, slot: usize) {
        let content = self.node[slot];
        if content >= LEAF {
            self.leaf[usize::from(content - LEAF)] = slot as u16;
        } else {
            let c = usize::from(content);
            self.parent[c] = slot as u16;
            self.parent[c + 1] = slot as u16;
        }
    }

    fn relink_all(&mut self) {
        for slot in 0..SLOTS {
            self.relink(slot);
        }
    }

    fn rebuild(&mut self) {
        let mut j = 0;
        for slot in 0..SLOTS {
            if self.node[slot] >= LEAF {
                self.freq[j] = self.freq[slot].div_ceil(2);
                self.node[j] = self.node[slot];
                j += 1;
            }
        }
        debug_assert_eq!(j, SYMBOLS);
        for j in SYMBOLS..SLOTS {
            let i = 2 * (j - SYMBOLS);
            let f = self.freq[i] + self.freq[i + 1];
            let mut k = j;
            while k > 0 && self.freq[k - 1] > f {
                k -= 1;
            }
            self.freq.copy_within(k..j, k + 1);
            self.node.copy_within(k..j, k + 1);
            self.freq[k] = f;
            self.node[k] = i as u16;
        }
        self.relink_all();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `data` encoded as literals only, for other formats' tests of
    /// compressed chunks.
    pub(crate) fn encode_literals(data: &[u8]) -> Vec<u8> {
        let tokens: Vec<Token> = data.iter().map(|&b| Token::Literal(b)).collect();
        encode(data.len() as u32, &tokens)
    }

    #[test]
    fn distance_tables_match_spec() {
        let (upper, width) = distance_tables();
        assert_eq!(
            crate::md5_hex(&upper[..]),
            "d43a9b84d1d221fddd5253ae3c1af3f9"
        );
        assert_eq!(
            crate::md5_hex(&width[..]),
            "c06b2e9b5a1ef82097400f668a13450d"
        );
    }

    /// Writes bits most significant first.
    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        bit: usize,
    }

    impl BitWriter {
        fn write(&mut self, value: usize, count: u32) {
            for i in (0..count).rev() {
                if self.bit.is_multiple_of(8) {
                    self.bytes.push(0);
                }
                if (value >> i) & 1 == 1 {
                    *self.bytes.last_mut().unwrap() |= 0x80 >> (self.bit % 8);
                }
                self.bit += 1;
            }
        }
    }

    enum Token {
        Literal(u8),
        Match { length: usize, distance: usize },
    }

    /// Encodes `tokens` with the same model, as an encoder would.
    fn encode(len: u32, tokens: &[Token]) -> Vec<u8> {
        let mut model = Model::new();
        let mut bits = BitWriter::default();
        let mut put_symbol = |bits: &mut BitWriter, symbol: usize| {
            let mut path = Vec::new();
            let mut slot = usize::from(model.leaf[symbol]);
            while slot != ROOT {
                let parent = usize::from(model.parent[slot]);
                path.push(slot - usize::from(model.node[parent]));
                slot = parent;
            }
            for &bit in path.iter().rev() {
                bits.write(bit, 1);
            }
            model.update(symbol);
        };
        let (upper, width) = distance_tables();
        for token in tokens {
            match *token {
                Token::Literal(byte) => put_symbol(&mut bits, byte.into()),
                Token::Match { length, distance } => {
                    put_symbol(&mut bits, 256 + length - MIN_MATCH);
                    let p = (0..256).find(|&p| usize::from(upper[p]) == distance / 64);
                    let w = u32::from(width[p.unwrap()]);
                    bits.write(p.unwrap() >> (8 - w), w);
                    bits.write(distance % 64, 6);
                }
            }
        }
        let mut out = len.to_le_bytes().to_vec();
        out.extend(bits.bytes);
        out
    }

    #[test]
    fn decodes_literals_and_matches() {
        use Token::*;
        let tokens = [
            Literal(b'a'),
            Literal(b'b'),
            Literal(b'c'),
            // Overlapping copy of "abc" repeated.
            Match {
                length: 7,
                distance: 2,
            },
            // Reaches back into the initial spaces.
            Match {
                length: 3,
                distance: 4095,
            },
            Literal(b'!'),
            // Cut short by the declared length.
            Match {
                length: 60,
                distance: 0,
            },
        ];
        let out = decode(&encode(20, &tokens)).unwrap();
        assert_eq!(out, b"abcabcabca   !!!!!!!");
    }

    #[test]
    fn rebuilds_model_consistently() {
        // Enough symbols to rebuild the model several times, with a skewed
        // distribution so slots move around.
        let mut tokens = Vec::new();
        let mut expected = Vec::new();
        let mut state = 1u32;
        while tokens.len() < 3 * REBUILD_AT as usize {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let r = (state >> 16) as usize;
            if r.is_multiple_of(5) && expected.len() > 100 {
                let length = MIN_MATCH + r % (MAX_MATCH - MIN_MATCH + 1);
                let distance = (r >> 3) % expected.len().min(RING_SIZE - 1);
                for _ in 0..length {
                    expected.push(expected[expected.len() - distance - 1]);
                }
                tokens.push(Token::Match { length, distance });
            } else {
                let byte = (r % 7 * r % 40) as u8;
                expected.push(byte);
                tokens.push(Token::Literal(byte));
            }
        }
        let out = decode(&encode(expected.len() as u32, &tokens)).unwrap();
        assert!(out == expected);
    }

    #[test]
    fn rejects_truncated_and_oversized_streams() {
        assert!(decode(&[1, 0]).is_err());
        assert!(decode(&[0xff, 0xff, 0xff, 0x7f, 0]).is_err());
        assert_eq!(decode(&[0, 0, 0, 0]).unwrap(), b"");
    }
}
