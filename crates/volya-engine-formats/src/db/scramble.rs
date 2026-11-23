// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Directory descrambling (docs/formats/db-archive.md, section 4).

/// Seed of the generator that builds the substitution table.
const TABLE_SEED: u32 = 0x0132_9436;
/// Seed of the keystream, fresh for every directory payload.
const KEYSTREAM_SEED: u32 = 0x0131_A9D3;
const TABLE_ROUNDS: usize = 2048;

/// The linear congruential generator shared by the table and the keystream.
struct Lcg(u32);

impl Lcg {
    /// Advances the state and draws its top 8 bits.
    fn next_byte(&mut self) -> u8 {
        self.0 = self.0.wrapping_mul(0x0808_8405).wrapping_add(1);
        (self.0 >> 24) as u8
    }
}

/// The substitution permutation `P`.
fn substitution() -> [u8; 256] {
    let mut p: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut lcg = Lcg(TABLE_SEED);
    for _ in 0..TABLE_ROUNDS {
        let a = lcg.next_byte();
        let mut b = lcg.next_byte();
        while b == a {
            b = lcg.next_byte();
        }
        p.swap(a.into(), b.into());
    }
    p
}

/// The descrambling table `Q`, the inverse of `P`.
fn descrambling_table() -> [u8; 256] {
    let p = substitution();
    let mut q = [0u8; 256];
    for (i, &pi) in p.iter().enumerate() {
        q[usize::from(pi)] = i as u8;
    }
    q
}

/// Descrambles a directory chunk payload in place.
pub fn descramble(payload: &mut [u8]) {
    let q = descrambling_table();
    let mut keystream = Lcg(KEYSTREAM_SEED);
    for byte in payload {
        *byte = q[usize::from(*byte ^ keystream.next_byte())];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_spec_vectors() {
        let p = substitution();
        assert_eq!(
            p[..16],
            [
                0xbe, 0xd6, 0x7d, 0x5b, 0xc5, 0x76, 0xf6, 0x66, 0x42, 0x33, 0xa2, 0x89, 0xe2, 0x29,
                0xcf, 0x79
            ]
        );
        let q = descrambling_table();
        assert_eq!(
            q[..16],
            [
                0x15, 0xe8, 0xc6, 0xa7, 0xd3, 0x76, 0x64, 0x94, 0x60, 0xbc, 0x19, 0x2e, 0xed, 0x40,
                0xf9, 0xdc
            ]
        );
        assert_eq!(crate::md5_hex(&q[..]), "58efe758d41da298e781286f998e5555");
        let mut keystream = Lcg(KEYSTREAM_SEED);
        let first: Vec<u8> = (0..16).map(|_| keystream.next_byte()).collect();
        assert_eq!(
            first,
            [
                0x88, 0xa4, 0x7d, 0xf7, 0xd1, 0xc9, 0x98, 0xbb, 0x13, 0x8f, 0x87, 0xc8, 0x9f, 0xd3,
                0x9a, 0x92
            ]
        );
    }

    #[test]
    fn inverts_scrambling() {
        // Scrambling is c = P[x] XOR k, so descrambling recovers x.
        let p = substitution();
        let plain: Vec<u8> = (0..=255u8).chain(0..=255u8).collect();
        let mut keystream = Lcg(KEYSTREAM_SEED);
        let mut data: Vec<u8> = plain
            .iter()
            .map(|&x| p[usize::from(x)] ^ keystream.next_byte())
            .collect();
        descramble(&mut data);
        assert_eq!(data, plain);
    }
}
