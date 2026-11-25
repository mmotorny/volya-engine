// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The display's gamma ramp (docs/behavior/display-gamma.md): the colour
//! table of docs/formats/save-picture.md 4.1 computed from the display
//! colour settings, which act on everything presented.
//!
//! The settings are copied from all three variables whenever any of them
//! is executed (3.1), and the variables change only by being executed, so
//! the display colour settings always equal the variables' current
//! values: the ramp is the table of the console's `rs_c_gamma`,
//! `rs_c_brightness` and `rs_c_contrast` (the balance is always 1.0).
//! The warm-up re-applications (3.3, 4) set the same ramp again: this
//! platform never loses the ramp, so they change nothing here.

use crate::console::Console;

/// The display colour settings (section 1): gamma, brightness, contrast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayColour {
    pub gamma: f32,
    pub brightness: f32,
    pub contrast: f32,
}

impl Default for DisplayColour {
    /// 1.0 each, as when the program starts.
    fn default() -> Self {
        Self {
            gamma: 1.0,
            brightness: 1.0,
            contrast: 1.0,
        }
    }
}

/// The balance of save-picture.md 4.1, always 1.0.
const BALANCE: f64 = 1.0;

impl DisplayColour {
    /// The settings as the console's variables hold them.
    pub fn of(console: &Console) -> Self {
        Self {
            gamma: console.number("rs_c_gamma"),
            brightness: console.number("rs_c_brightness"),
            contrast: console.number("rs_c_contrast"),
        }
    }

    /// The table of save-picture.md 4.1: one 16-bit entry per 8-bit input,
    /// the same for R, G and B, evaluated in double precision with the
    /// single-precision rounding it states (display-gamma.md 2.1).
    pub fn table(&self) -> [u16; 256] {
        let g = 1.0 / (f64::from(self.gamma) + 0.000_01);
        let (b, c) = (f64::from(self.brightness), f64::from(self.contrast));
        std::array::from_fn(|i| {
            let v = (i as f64 * (1.0 / 255.0)).powf(g) * (c * 0.5 + 0.5) * 65535.0
                + (b * 0.5 - 0.5) * 32768.0
                - c * 0.5 * 32768.0
                + 16384.0;
            let v = v as f32;
            (f64::from(v) * BALANCE).floor().clamp(0.0, 65535.0) as u16
        })
    }

    /// The ramp when it changes some colour; `None` for the identity
    /// (257 × *i* for every entry), which leaves every colour as it is.
    pub fn ramp(&self) -> Option<[u16; 256]> {
        let table = self.table();
        let identity = (0..256).all(|i| u32::from(table[i]) == 257 * i as u32);
        (!identity).then_some(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(table: &[u16; 256]) -> [u16; 5] {
        [0, 64, 128, 192, 255].map(|i| table[i])
    }

    #[test]
    fn the_shipped_settings_are_the_identity() {
        // Sections 2.1 and 7.1.
        let table = DisplayColour::default().table();
        assert!((0..256).all(|i| u32::from(table[i]) == 257 * i as u32));
        assert_eq!(table.iter().map(|&e| u32::from(e)).sum::<u32>(), 8_388_480);
        assert_eq!(DisplayColour::default().ramp(), None);
    }

    #[test]
    fn the_sixteen_bit_test_vectors() {
        // Section 2.1's table.
        let cases = [
            ((1.5, 1.0, 1.0), [0, 26075, 41392, 54239, 65535]),
            ((0.5, 1.0, 1.0), [0, 4128, 16512, 37153, 65535]),
            ((1.0, 1.5, 1.0), [8192, 24640, 41088, 57536, 65535]),
            ((1.0, 0.5, 1.0), [0, 8256, 24704, 41152, 57343]),
            ((1.0, 1.0, 1.5), [0, 12368, 32928, 53488, 65535]),
            ((1.0, 1.0, 0.5), [8192, 20528, 32864, 45200, 57343]),
        ];
        for ((gamma, brightness, contrast), want) in cases {
            let d = DisplayColour {
                gamma,
                brightness,
                contrast,
            };
            let table = d.table();
            assert_eq!(at(&table), want, "{d:?}");
            assert!(table.windows(2).all(|w| w[0] <= w[1]), "{d:?}");
            assert_eq!(d.ramp(), Some(table));
        }
    }

    #[test]
    fn the_settings_are_the_console_variables() {
        let mut c = Console::new(Default::default());
        assert_eq!(DisplayColour::of(&c), DisplayColour::default());
        c.exec("rs_c_gamma 1.5");
        c.exec("rs_c_contrast 0.5");
        // Rejected: out of range, the variable keeps its value (3.1).
        c.exec("rs_c_brightness 2");
        assert_eq!(
            DisplayColour::of(&c),
            DisplayColour {
                gamma: 1.5,
                brightness: 1.0,
                contrast: 0.5,
            }
        );
    }
}
