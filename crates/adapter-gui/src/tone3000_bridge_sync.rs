//! Responsibility: draws the TONE3000 browser view onto the window's bridge.

use application::tone3000::install::InstallProgress;
use application::tone3000::Tone3000Architecture;
use slint::{ModelRc, SharedString, VecModel};

use crate::tone3000_view::{InstalledRowView, RowInstall, Tone3000View, ToneRowView};
use crate::{Tone3000Bridge, Tone3000InstalledRow, Tone3000ToneRow};

/// Publish the key status, the last search and the installed packages.
pub(crate) fn set_tone3000_view(bridge: &Tone3000Bridge, view: &Tone3000View) {
    bridge.set_key_configured(view.key_configured);
    bridge.set_can_install(view.can_install);
    bridge.set_searching(view.searching);
    bridge.set_searched(view.searched);
    bridge.set_error(view.error.as_str().into());
    bridge.set_page(view.page as i32);
    bridge.set_total_pages(view.total_pages as i32);
    bridge.set_total(view.total as i32);
    let results: Vec<Tone3000ToneRow> = view.results.iter().map(tone_row).collect();
    bridge.set_results(ModelRc::new(VecModel::from(results)));
    let installed: Vec<Tone3000InstalledRow> = view.installed.iter().map(installed_row).collect();
    bridge.set_installed(ModelRc::new(VecModel::from(installed)));
    bridge.set_installed_total(view.installed_total as i32);
}

/// The window's architecture code: 1 = A1, 2 = A2, 0 for an IR tone.
pub(crate) fn arch_to_int(arch: Option<Tone3000Architecture>) -> i32 {
    match arch {
        Some(Tone3000Architecture::A1) => 1,
        Some(Tone3000Architecture::A2) => 2,
        None => 0,
    }
}

fn tone_row(row: &ToneRowView) -> Tone3000ToneRow {
    let (stage, done, total) = match &row.install {
        RowInstall::Running(InstallProgress::Fetching) => (0, 0, 0),
        RowInstall::Running(InstallProgress::Downloading { done, total }) => {
            (1, *done as i32, *total as i32)
        }
        RowInstall::Running(InstallProgress::Measuring) => (2, 0, 0),
        _ => (0, 0, 0),
    };
    Tone3000ToneRow {
        tone_id: SharedString::from(row.tone_id.to_string()),
        title: row.title.as_str().into(),
        author: row.author.as_str().into(),
        gear: row.gear.as_str().into(),
        is_ir: row.arch.is_none(),
        a1_count: row.a1_count as i32,
        a2_count: row.a2_count as i32,
        ir_count: row.ir_count as i32,
        downloads: row.downloads.as_str().into(),
        arch: arch_to_int(row.arch),
        installed: matches!(row.install, RowInstall::Installed(_)),
        plugin_id: match &row.install {
            RowInstall::Installed(Some(id)) => id.as_str().into(),
            _ => SharedString::new(),
        },
        installing: matches!(row.install, RowInstall::Running(_)),
        stage,
        done,
        total,
        error: match &row.install {
            RowInstall::Failed(message) => message.as_str().into(),
            _ => SharedString::new(),
        },
    }
}

fn installed_row(row: &InstalledRowView) -> Tone3000InstalledRow {
    Tone3000InstalledRow {
        plugin_id: row.plugin_id.as_str().into(),
        name: row.name.as_str().into(),
        block_type: row.block_type.as_str().into(),
        arch: row.arch.as_str().into(),
        captures: row.captures as i32,
        removable: row.removable,
    }
}

#[cfg(test)]
#[path = "tone3000_bridge_sync_tests.rs"]
mod tests;
