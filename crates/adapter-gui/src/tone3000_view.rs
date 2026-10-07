//! Responsibility: turns the TONE3000 browser state into what the window draws.
//!
//! Pure: a snapshot of the dispatcher's state plus the architecture the user
//! picked per tone in, rows out (#879).

use std::collections::HashMap;

use application::tone3000::api_types::Tone;
use application::tone3000::install::InstallProgress;
use application::tone3000::manifest_build::{plugin_id, PackageKind};
use application::tone3000::Tone3000Architecture;
use application::tone3000_state::{Tone3000InstalledEntry, Tone3000Snapshot};

/// The architecture the user picked per tone. A tone missing here shows the
/// one an install takes by default.
pub(crate) type ArchChoices = HashMap<u64, Tone3000Architecture>;

/// Where a tone's install stands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowInstall {
    Idle,
    Installed,
    Running(InstallProgress),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ToneRowView {
    pub tone_id: u64,
    pub title: String,
    pub author: String,
    pub gear: String,
    /// The architecture the install takes; `None` for an IR tone.
    pub arch: Option<Tone3000Architecture>,
    pub a1_count: u32,
    pub a2_count: u32,
    pub ir_count: u32,
    pub downloads: String,
    pub install: RowInstall,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InstalledRowView {
    pub plugin_id: String,
    pub name: String,
    /// Manifest spelling (`amp`, `gain_pedal`, …).
    pub block_type: String,
    /// `A1`, `A2`, or empty for an IR package.
    pub arch: String,
    pub captures: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Tone3000View {
    pub key_configured: bool,
    pub can_install: bool,
    pub searching: bool,
    pub searched: bool,
    pub error: String,
    pub page: u32,
    pub total_pages: u32,
    pub total: u32,
    pub results: Vec<ToneRowView>,
    pub installed: Vec<InstalledRowView>,
}

pub(crate) fn tone3000_view(snapshot: &Tone3000Snapshot, choices: &ArchChoices) -> Tone3000View {
    let search = &snapshot.search;
    let page = search.results.as_ref();
    Tone3000View {
        key_configured: snapshot.key_configured,
        can_install: snapshot.can_install,
        searching: search.in_flight,
        searched: page.is_some() || search.error.is_some(),
        error: search.error.clone().unwrap_or_default(),
        page: page.map_or(0, |p| p.page),
        total_pages: page.map_or(0, |p| p.total_pages),
        total: page.map_or(0, |p| p.total),
        results: page
            .map(|p| {
                p.data
                    .iter()
                    .map(|tone| tone_row(tone, snapshot, choices))
                    .collect()
            })
            .unwrap_or_default(),
        installed: snapshot.installed.iter().map(installed_row).collect(),
    }
}

/// The architecture an install of `tone` takes when the user picks none:
/// A2 when it has A2 captures, else A1; `None` for an IR tone.
pub(crate) fn default_arch(tone: &Tone) -> Option<Tone3000Architecture> {
    if is_ir(tone) {
        return None;
    }
    Some(if tone.a2_models_count > 0 {
        Tone3000Architecture::A2
    } else {
        Tone3000Architecture::A1
    })
}

/// `1234` → `1.2k`, `2_500_000` → `2.5M`; below a thousand as is.
pub(crate) fn compact_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => trim_decimal(n as f64 / 1_000.0, "k"),
        _ => trim_decimal(n as f64 / 1_000_000.0, "M"),
    }
}

fn trim_decimal(value: f64, unit: &str) -> String {
    let text = format!("{value:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{text}{unit}")
}

fn is_ir(tone: &Tone) -> bool {
    tone.format.as_deref() == Some("ir")
}

fn tone_row(tone: &Tone, snapshot: &Tone3000Snapshot, choices: &ArchChoices) -> ToneRowView {
    let arch = default_arch(tone).map(|default| choices.get(&tone.id).copied().unwrap_or(default));
    let kind = arch.map_or(PackageKind::Ir, PackageKind::Nam);
    let package = plugin_id(tone.id, kind);
    let entry = snapshot.installs.iter().find(|i| i.tone_id == tone.id);
    let install = match entry {
        Some(e) => match &e.error {
            Some(message) => RowInstall::Failed(message.clone()),
            None => RowInstall::Running(e.progress.clone()),
        },
        None if snapshot.installed.iter().any(|p| p.plugin_id == package) => RowInstall::Installed,
        None => RowInstall::Idle,
    };
    ToneRowView {
        tone_id: tone.id,
        title: tone.title.clone(),
        author: tone
            .user
            .as_ref()
            .map(|u| u.display_name.clone().unwrap_or_else(|| u.username.clone()))
            .unwrap_or_default(),
        gear: tone.gear.clone().unwrap_or_default(),
        arch,
        a1_count: tone.a1_models_count,
        a2_count: tone.a2_models_count,
        ir_count: tone.irs_count,
        downloads: if tone.downloads_count == 0 {
            String::new()
        } else {
            compact_count(tone.downloads_count)
        },
        install,
    }
}

fn installed_row(entry: &Tone3000InstalledEntry) -> InstalledRowView {
    InstalledRowView {
        plugin_id: entry.plugin_id.clone(),
        name: entry.display_name.clone(),
        block_type: serde_json::to_value(entry.block_type)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        arch: entry
            .architecture
            .map(|a| a.as_str().to_owned())
            .unwrap_or_default(),
        captures: entry.captures as u32,
    }
}

#[cfg(test)]
#[path = "tone3000_view_tests.rs"]
mod tests;
