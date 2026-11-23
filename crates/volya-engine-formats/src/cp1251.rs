// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Windows-1251 (Cyrillic) single-byte text, the encoding of the game's text
//! files. Bytes `0x00`–`0x7F` are ASCII. Byte `0x98` is unassigned in
//! Windows-1251; it decodes to U+0098 so that decoding is a bijection and
//! [`encode`] restores the original bytes exactly.

/// Characters of bytes `0x80`–`0xBF`; `0xC0`–`0xFF` are U+0410–U+044F.
#[rustfmt::skip]
const HIGH: [char; 64] = [
    'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ',
    'ђ', '‘', '’', '“', '”', '•', '–', '—', '\u{98}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ',
    '\u{a0}', 'Ў', 'ў', 'Ј', '¤', 'Ґ', '¦', '§', 'Ё', '©', 'Є', '«', '¬', '\u{ad}', '®', 'Ї',
    '°', '±', 'І', 'і', 'ґ', 'µ', '¶', '·', 'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ', 'ї',
];

/// The character of byte `b`.
pub fn decode_byte(b: u8) -> char {
    match b {
        0x00..=0x7F => char::from(b),
        0x80..=0xBF => HIGH[usize::from(b - 0x80)],
        0xC0..=0xFF => char::from_u32(0x410 + u32::from(b - 0xC0)).expect("Cyrillic letter"),
    }
}

/// Decodes `bytes` to a string; every byte maps to one character.
pub fn decode(bytes: &[u8]) -> String {
    if bytes.is_ascii() {
        // ASCII is the common case; skip the per-character work.
        return String::from_utf8(bytes.to_vec()).expect("ASCII");
    }
    bytes.iter().map(|&b| decode_byte(b)).collect()
}

/// The byte of character `c`, or `None` if Windows-1251 cannot represent it.
pub fn encode_char(c: char) -> Option<u8> {
    match u32::from(c) {
        0x00..=0x7F => Some(c as u8),
        0x410..=0x44F => Some((u32::from(c) - 0x410 + 0xC0) as u8),
        _ => HIGH.iter().position(|&h| h == c).map(|i| 0x80 + i as u8),
    }
}

/// Encodes `text`, or `None` if it has a character Windows-1251 cannot
/// represent.
pub fn encode(text: &str) -> Option<Vec<u8>> {
    text.chars().map(encode_char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_byte() {
        let all: Vec<u8> = (0..=255).collect();
        let text = decode(&all);
        assert_eq!(text.chars().count(), 256);
        assert_eq!(encode(&text).unwrap(), all);
    }

    #[test]
    fn decodes_cyrillic() {
        assert_eq!(
            decode(b"\xcf\xf0\xe8\xe2\xe5\xf2, \xa8\xb8!"),
            "Привет, Ёё!"
        );
        assert_eq!(decode(b"\xb9 \x96 \x85"), "№ – …");
        assert_eq!(encode("€"), Some(vec![0x88]));
        assert_eq!(encode("日"), None);
    }
}
