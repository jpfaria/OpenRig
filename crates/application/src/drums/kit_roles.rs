//! Responsibility: decides which kit instrument plays each drum role.
//!
//! A kit's own `roles.yaml` wins; otherwise the instrument names decide, and
//! the General MIDI note fills any role still empty. Articulations a groove
//! never asks for (chokes, swishes, tom rims, ride shanks) are left out.

use std::collections::BTreeMap;

use feature_dsp::drums::DrumRole;

use super::hydrogen_kit::KitDescription;

/// Name fragments of articulations no groove plays.
const SKIPPED: [&str; 5] = ["choke", "swish", "shank", "tomedge", "stickclick"];

fn normalized(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Whether the name is an articulation no groove plays.
fn is_articulation(name: &str) -> bool {
    let n = normalized(name);
    SKIPPED.iter().any(|s| n.contains(s)) || (n.contains("tom") && n.contains("rim"))
}

/// The role an instrument name describes, if any.
pub fn role_for_name(name: &str) -> Option<DrumRole> {
    if is_articulation(name) {
        return None;
    }
    let n = normalized(name);
    let has = |s: &str| n.contains(s);
    let role = if has("kick") || has("bassdrum") || (has("bd") && n.len() <= 4) {
        DrumRole::Kick
    } else if has("sidestick") || has("crossstick") {
        DrumRole::SideStick
    } else if has("snare") {
        if has("edge") || has("rim") {
            DrumRole::SnareRim
        } else {
            DrumRole::Snare
        }
    } else if has("clap") {
        DrumRole::Clap
    } else if has("hat") || has("hh") {
        if has("pedal") || has("foot") {
            DrumRole::HatPedal
        } else if has("open") || has("semi") {
            DrumRole::HatOpen
        } else {
            DrumRole::HatClosed
        }
    } else if has("tom") {
        if has("floor") || has("fl") {
            DrumRole::TomFloor
        } else if has("high") || has("hi") {
            DrumRole::TomHigh
        } else {
            DrumRole::TomMid
        }
    } else if has("china") {
        DrumRole::China
    } else if has("splash") {
        DrumRole::Splash
    } else if has("ride") {
        if has("bell") {
            DrumRole::RideBell
        } else {
            DrumRole::Ride
        }
    } else if has("crash") {
        DrumRole::Crash
    } else if has("tambourine") {
        DrumRole::Tambourine
    } else if has("cowbell") {
        DrumRole::Cowbell
    } else {
        return None;
    };
    Some(role)
}

/// The instrument index playing each role. `overrides` maps a role key to an
/// instrument name and wins over everything else.
pub fn assign_roles(
    kit: &KitDescription,
    overrides: &BTreeMap<String, String>,
) -> [Option<usize>; DrumRole::COUNT] {
    let mut roles = [None; DrumRole::COUNT];
    let mut used = vec![false; kit.instruments.len()];
    let mut claim = |roles: &mut [Option<usize>; DrumRole::COUNT], role: DrumRole, i: usize| {
        if roles[role.index()].is_none() && !used[i] {
            roles[role.index()] = Some(i);
            used[i] = true;
        }
    };

    for (key, name) in overrides {
        let found = kit.instruments.iter().position(|ins| &ins.name == name);
        if let (Some(role), Some(i)) = (DrumRole::from_key(key), found) {
            claim(&mut roles, role, i);
        }
    }
    for (i, instrument) in kit.instruments.iter().enumerate() {
        let Some(role) = role_for_name(&instrument.name) else {
            continue;
        };
        let role = match role {
            DrumRole::Crash if roles[role.index()].is_some() => DrumRole::Crash2,
            DrumRole::TomMid if roles[role.index()].is_some() => DrumRole::TomHigh,
            other => other,
        };
        claim(&mut roles, role, i);
    }
    for (i, instrument) in kit.instruments.iter().enumerate() {
        if is_articulation(&instrument.name) {
            continue;
        }
        if let Some(role) = instrument.midi_note.and_then(DrumRole::from_general_midi) {
            claim(&mut roles, role, i);
        }
    }
    roles
}
