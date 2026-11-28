// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Trade parameters (5.2, 5.3 of docs/behavior/script-api-dialogs-and-trade.md):
//! each character's own ([`crate::game_object::InventoryOwner`]'s, which
//! script API M's `game_object` forms of `sell_condition`, `buy_condition`
//! and `show_condition` set) and the game's default ones, which the global
//! functions set ([`super::DialogsAndTrade::default_trade`]); reading a
//! section into them, and what the trade asks of them: an item's factors,
//! its price and whether it is shown. What the trade does with them is
//! #284's.

use volya_engine_core::config::{Config, Section, value};

use crate::game_object::game_bytes;

/// The section of the system configuration the defaults are made from
/// (5.2).
pub const TRADE_SECTION: &str = "trade";

/// A buy or sell part (5.3): two default factors (first, second), an item
/// list and a factor table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradePart {
    /// The first and second default factors.
    pub default: (f32, f32),
    /// Item sections with the price 0 for this part.
    pub item_list: Vec<Vec<u8>>,
    /// Item sections with their own first and second factors, each section
    /// once.
    pub factor_table: Vec<(Vec<u8>, f32, f32)>,
}

impl TradePart {
    /// Reads `section` into the part (5.3): the item list and the factor
    /// table emptied, then each line in order, a line without a value
    /// adding its key to the item list, a line `key = a, b` giving `key`
    /// the factors `a` and `b`, replacing earlier ones for that key. The
    /// default factors are not changed.
    pub fn read(&mut self, section: Section) {
        self.item_list.clear();
        self.factor_table.clear();
        for entry in section.entries() {
            let key = game_bytes(entry.key());
            match entry.value() {
                None => self.item_list.push(key),
                Some(v) => {
                    let [a, b] = value::floats::<2>(v);
                    match self.factor_table.iter_mut().find(|(k, ..)| *k == key) {
                        Some(e) => *e = (key, a, b),
                        None => self.factor_table.push((key, a, b)),
                    }
                }
            }
        }
    }

    fn listed(&self, item: &[u8]) -> bool {
        self.item_list.iter().any(|i| i == item)
    }

    fn table_entry(&self, item: &[u8]) -> Option<(f32, f32)> {
        (self.factor_table.iter())
            .find(|(k, ..)| k == item)
            .map(|&(_, a, b)| (a, b))
    }
}

/// A set of trade parameters (5.3): a buy part, a sell part and a show
/// list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradeParameters {
    pub buy: TradePart,
    pub sell: TradePart,
    /// Item sections the show list holds.
    pub show: Vec<Vec<u8>>,
}

/// Reads `section` as a show list (5.3): emptied, then each line without
/// a value adds its key; lines with a value are skipped.
pub fn read_show(show: &mut Vec<Vec<u8>>, section: Section) {
    show.clear();
    show.extend(
        section
            .entries()
            .filter(|e| e.value().is_none())
            .map(|e| game_bytes(e.key())),
    );
}

impl TradeParameters {
    /// The default trade parameters as made on their first use (5.2): the
    /// buy factors from `[trade]`'s `buy_price_factor_hostile` (first) and
    /// `buy_price_factor_friendly` (second), the sell factors from the
    /// `sell_price_factor_` pair, empty lists and tables. A missing key is
    /// `Err` with the key (the caller's failed check: our choice).
    pub fn defaults(system: &Config) -> Result<Self, String> {
        let factor = |key: &str| {
            system
                .get(TRADE_SECTION, key)
                .flatten()
                .map(value::float)
                .ok_or_else(|| format!("{TRADE_SECTION}: {key}"))
        };
        let part = |kind: &str| -> Result<TradePart, String> {
            Ok(TradePart {
                default: (
                    factor(&format!("{kind}_price_factor_hostile"))?,
                    factor(&format!("{kind}_price_factor_friendly"))?,
                ),
                ..TradePart::default()
            })
        };
        Ok(TradeParameters {
            buy: part("buy")?,
            sell: part("sell")?,
            show: Vec::new(),
        })
    }
}

/// The factors of `item` for a part (5.3), the trading character's part
/// `own` and the defaults' part `defaults`: none (the price 0) for an item
/// in either item list; else the character's table entry, else the
/// defaults' table entry, else the character's own default factors.
pub fn item_factors(own: &TradePart, defaults: &TradePart, item: &[u8]) -> Option<(f32, f32)> {
    if own.listed(item) || defaults.listed(item) {
        return None;
    }
    Some(
        own.table_entry(item)
            .or_else(|| defaults.table_entry(item))
            .unwrap_or(own.default),
    )
}

/// The weight of the first factor for the value `v` (5.3): 0.5 + v / 2000
/// limited to 0 … 1, 0 when the value is missing.
pub fn first_factor_weight(v: Option<f32>) -> f32 {
    v.map_or(0.0, |v| (0.5 + v * 0.0005).clamp(0.0, 1.0))
}

/// The price of an item of cost `cost` and condition `c` (5.3) with the
/// factors `factors` ([`item_factors`]; none gives 0) at the value `v`:
/// the factor *f*₂ + (*f*₁ − *f*₂)·*t* times (0.9·*c* + 0.1)^0.75 times the
/// cost. Which part serves which direction of a deal, and what *v* and *c*
/// are, are #284's (believed the attitude and the item's condition).
pub fn price(factors: Option<(f32, f32)>, v: Option<f32>, c: f32, cost: f32) -> f32 {
    let Some((f1, f2)) = factors else {
        return 0.0;
    };
    let factor = f2 + (f1 - f2) * first_factor_weight(v);
    factor * (0.9 * c + 0.1).powf(0.75) * cost
}

/// Whether `item` is in neither show list (5.3), the character's `own`
/// and the defaults'. What the answer decides is unestablished (believed:
/// which items a dead character's body offers); the trade asks it only
/// of a character whose health is 0 or less.
pub fn shown(own: &[Vec<u8>], defaults: &[Vec<u8>], item: &[u8]) -> bool {
    !own.iter().chain(defaults).any(|i| i == item)
}
