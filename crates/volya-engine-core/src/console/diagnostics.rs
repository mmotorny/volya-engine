// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The diagnostic commands' lines (docs/behavior/console-diagnostics.md):
//! `stat_memory`'s memory statistics, `dump_resources`'s counts of the
//! graphics resources, `dump_open_files`' list and the phase lines of a
//! load.

use super::Log;

/// The figures `stat_memory` prints (section 2.1), in bytes, as the
/// program's parts report them ([`super::Console::set_memory_statistics`]).
/// TODO(#636): nothing reports them yet, so every figure is 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryStatistics {
    /// The address space's free regions (*a*).
    pub free: u64,
    /// Its reserved, not committed, regions (*b*).
    pub reserved: u64,
    /// Its committed regions (*c*).
    pub committed: u64,
    /// The textures the resource manager holds (*t*).
    pub textures: u64,
    /// The C runtime heap's in-use blocks (*h*).
    pub crt_heap: u64,
    /// The process heap's in-use blocks (*p*).
    pub process_heap: u64,
    /// The game's scripts' pool in use (*g*).
    pub game_lua: u64,
    /// The engine's scripts' pool in use (*e*).
    pub engine_lua: u64,
    /// The renderer's pool in use (*r*).
    pub render: u64,
    /// The bytes saved by sharing identical strings (*s*; may be negative).
    pub strings: i64,
    /// The bytes saved by sharing memory blocks (*m*; may be negative).
    pub smem: i64,
    /// The program's own memory manager's bytes in use (`* phase cmem`,
    /// section 2.4).
    pub cmem: u64,
}

/// K: a byte count divided by 1024, rounded down (conventions).
fn k(bytes: u64) -> u64 {
    bytes / 1024
}

impl MemoryStatistics {
    /// `stat_memory`'s four lines (2.1). The memory it releases first has
    /// no counterpart here: nothing holds unused memory to give back.
    pub(super) fn print(&self, log: &mut Log) {
        log.message(&format!(
            "* [win32]: free[{} K], reserved[{} K], committed[{} K]",
            k(self.free),
            k(self.reserved),
            k(self.committed)
        ));
        log.message(&format!("* [ D3D ]: textures[{} K]", k(self.textures)));
        log.message(&format!(
            "* [x-ray]: crt heap[{} K], process heap[{} K], game lua[{} K], engine lua[{} K], render[{} K]",
            k(self.crt_heap),
            k(self.process_heap),
            k(self.game_lua),
            k(self.engine_lua),
            k(self.render)
        ));
        // Rounded toward zero.
        log.message(&format!(
            "* [x-ray]: economy: strings[{} K], smem[{} K]",
            self.strings / 1024,
            self.smem / 1024
        ));
    }
}

/// `dump_resources`' labels, in its order (section 2.2).
pub const RESOURCE_LABELS: [&str; 13] = [
    "textures  ",
    "rtargets  ",
    "rtargetsc ",
    "vs        ",
    "ps        ",
    "dcl       ",
    "states    ",
    "tex_list  ",
    "matrices  ",
    "lst_constants",
    "v_passes  ",
    "v_elements",
    "v_shaders ",
];

/// The resource manager's counts, by [`RESOURCE_LABELS`] (section 2.2), as
/// the renderer reports them ([`super::Console::set_resource_counts`]).
/// TODO(#636): nothing reports them yet, so every count is 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceCounts(pub [u64; 13]);

impl ResourceCounts {
    /// The thirteen lines (2.2; at shutdown, 2.5). TODO(#636): at shutdown
    /// a remaining resource of the first five kinds is listed after its
    /// kind's line; none is reported, so none is listed.
    pub(super) fn print(&self, log: &mut Log) {
        for (label, count) in RESOURCE_LABELS.iter().zip(self.0) {
            log.message(&format!("* RM_Dump: {label}: {count}"));
        }
    }
}

/// C's `atoi`: leading white space, a sign, then the leading digits; 0
/// without any.
pub(super) fn atoi(text: &str) -> i64 {
    let text = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i64, |v, d| {
            v.saturating_mul(10).saturating_add(i64::from(d - b'0'))
        });
    if negative { -value } else { value }
}

/// `dump_open_files <argument>` (section 2.3). Files are listed only with
/// `-file_activity` on the command line; Volya Engine has none, so the list
/// stays empty: 1 prints nothing, any other value `----un-used` alone.
pub(super) fn dump_open_files(log: &mut Log, argument: &str) {
    if atoi(argument) != 1 {
        log.message("----un-used");
    }
}

/// The phase pair of a load (2.4): `ms` since the previous caption began
/// (or the load), and the memory manager's bytes in use.
pub(super) fn phase(log: &mut Log, ms: u64, memory: &MemoryStatistics) {
    log.message(&format!("* phase time: {ms} ms"));
    log.message(&format!("* phase cmem: {} K", k(memory.cmem)));
}
