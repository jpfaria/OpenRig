//! Responsibility: maps every TONE3000 window control to its command.
//!
//! The window is a pure dispatcher: a search reads the query and filters off
//! the bridge, an install takes the architecture the user picked, and the
//! dispatcher validates, runs and persists (#879).

use std::rc::Rc;

use application::command::Tone3000Command;
use application::tone3000::Tone3000Architecture;
use serde::de::DeserializeOwned;
use slint::{ComponentHandle, Global};

use crate::{Tone3000Bridge, Tone3000Window};

/// Where a TONE3000 control sends its command.
pub(crate) type Tone3000Dispatch = Rc<dyn Fn(Tone3000Command)>;
/// Records the architecture the user picked for a tone.
pub(crate) type ArchPick = Rc<dyn Fn(u64, Tone3000Architecture)>;
/// The architecture the user picked for a tone, if any.
pub(crate) type ArchOf = Rc<dyn Fn(u64) -> Option<Tone3000Architecture>>;

/// Route every control of `window` through `dispatch`.
pub(crate) fn wire_tone3000_intents(
    window: &Tone3000Window,
    dispatch: Tone3000Dispatch,
    pick: ArchPick,
    arch_of: ArchOf,
) {
    let bridge = Tone3000Bridge::get(window);
    let weak = window.as_weak();
    let d = dispatch.clone();
    bridge.on_search(move |page| {
        if let Some(w) = weak.upgrade() {
            d(search_command(&Tone3000Bridge::get(&w), page));
        }
    });
    bridge.on_pick_arch(move |tone_id, arch| {
        if let (Some(id), Some(arch)) = (tone_id_of(&tone_id), arch_from_int(arch)) {
            pick(id, arch);
        }
    });
    let d = dispatch.clone();
    bridge.on_install(move |tone_id| {
        if let Some(id) = tone_id_of(&tone_id) {
            d(Tone3000Command::InstallTone3000 {
                tone_id: id,
                architecture: arch_of(id),
                block_type: None,
            });
        }
    });
    bridge.on_uninstall(move |plugin_id| {
        dispatch(Tone3000Command::UninstallTone3000 {
            plugin_id: plugin_id.to_string(),
        })
    });
}

/// The search the bridge's query and filters describe, from `page`.
pub(crate) fn search_command(bridge: &Tone3000Bridge, page: i32) -> Tone3000Command {
    Tone3000Command::SearchTone3000 {
        query: bridge.get_query().trim().to_string(),
        page: page.max(1) as u32,
        format: filter(&bridge.get_format()),
        gear: filter(&bridge.get_gear()),
        sort: filter(&bridge.get_sort()),
    }
}

/// A filter in its API spelling; blank or unknown means "all".
fn filter<T: DeserializeOwned>(spelling: &str) -> Option<T> {
    if spelling.is_empty() {
        return None;
    }
    serde_json::from_value(serde_json::Value::String(spelling.to_string())).ok()
}

fn tone_id_of(text: &str) -> Option<u64> {
    text.parse().ok()
}

/// The window's architecture code: 1 = A1, 2 = A2.
pub(crate) fn arch_from_int(code: i32) -> Option<Tone3000Architecture> {
    match code {
        1 => Some(Tone3000Architecture::A1),
        2 => Some(Tone3000Architecture::A2),
        _ => None,
    }
}

#[cfg(test)]
#[path = "tone3000_intents_tests.rs"]
mod tests;
