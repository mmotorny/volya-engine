// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Reverb (docs/behavior/sound-reverb.md): what of it is observable here.
//!
//! The program uses reverb only through the device's EAX 2.0 listener
//! reverb, and only while it is **active**: `snd_efx` on and EAX usable on
//! the device (section 2.2), which needs the device to report the OpenAL
//! extension `EAX2.0`. Volya Engine's device is its own mixer
//! (`volya-engine::audio::MixerDevice`), which has no such extension, so
//! reverb is never active and nothing is sent, whatever `snd_efx` says:
//! no reverb is heard. On `l01_escape`, which has no `level.snd_env`, that
//! is also what the spec gives with reverb active (section 9: the none
//! environment, room −10000 mB).
//!
//! TODO(#320, docs/behavior/sound-reverb.md sections 3 – 7): the
//! reverb-output decision (whether and how to render the environments of
//! `senvironment.xr` and `level.snd_env` on a device of ours, given that the
//! reverb itself is the device's) goes to the user; until then nothing of
//! sections 3 – 8 is built.

/// Whether reverb is active at an update (2.2): `snd_efx` on and EAX usable
/// on the device.
pub fn active(snd_efx: bool, eax_usable: bool) -> bool {
    snd_efx && eax_usable
}

/// Whether the device Volya Engine plays through reports EAX 2.0: never
/// (2.2, rule 1).
pub const EAX_USABLE: bool = false;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverb_is_never_active_on_our_device() {
        assert!(!active(true, EAX_USABLE));
        assert!(!active(false, EAX_USABLE));
        assert!(active(true, true));
    }
}
