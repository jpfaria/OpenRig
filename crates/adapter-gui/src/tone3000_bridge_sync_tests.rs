use application::tone3000::install::InstallProgress;
use application::tone3000::Tone3000Architecture;
use slint::{Global, Model};

use super::set_tone3000_view;
use crate::tone3000_view::{InstalledRowView, RowInstall, Tone3000View, ToneRowView};
use crate::{Tone3000Bridge, Tone3000Window};

fn row(install: RowInstall, arch: Option<Tone3000Architecture>) -> ToneRowView {
    ToneRowView {
        tone_id: 4521,
        title: "Plexi".into(),
        author: "The Player".into(),
        gear: "amp".into(),
        arch,
        a1_count: 4,
        a2_count: 2,
        ir_count: 0,
        downloads: "15.3k".into(),
        install,
    }
}

fn drawn(rows: Vec<ToneRowView>) -> (Tone3000Window, Vec<crate::Tone3000ToneRow>) {
    i_slint_backend_testing::init_no_event_loop();
    let w = Tone3000Window::new().unwrap();
    let view = Tone3000View {
        key_configured: true,
        can_install: true,
        searched: true,
        page: 2,
        total_pages: 3,
        total: 60,
        results: rows,
        installed: vec![InstalledRowView {
            plugin_id: "tone3000_9_a2".into(),
            name: "Rig".into(),
            block_type: "amp".into(),
            arch: "A2".into(),
            captures: 5,
        }],
        ..Tone3000View::default()
    };
    set_tone3000_view(&Tone3000Bridge::get(&w), &view);
    let rows = Tone3000Bridge::get(&w).get_results().iter().collect();
    (w, rows)
}

#[test]
fn the_page_status_and_packages_reach_the_bridge() {
    let (w, _) = drawn(vec![]);
    let bridge = Tone3000Bridge::get(&w);
    assert!(bridge.get_key_configured() && bridge.get_can_install() && bridge.get_searched());
    assert_eq!(
        (
            bridge.get_page(),
            bridge.get_total_pages(),
            bridge.get_total()
        ),
        (2, 3, 60)
    );
    let package = bridge.get_installed().row_data(0).unwrap();
    assert_eq!(package.plugin_id, "tone3000_9_a2");
    assert_eq!((package.arch.as_str(), package.captures), ("A2", 5));
}

#[test]
fn a_row_carries_its_tone_and_architecture_code() {
    let (_w, rows) = drawn(vec![
        row(RowInstall::Idle, Some(Tone3000Architecture::A1)),
        row(RowInstall::Idle, Some(Tone3000Architecture::A2)),
        row(RowInstall::Idle, None),
    ]);
    assert_eq!(rows[0].tone_id, "4521");
    assert_eq!(rows[0].downloads, "15.3k");
    assert_eq!(
        rows.iter().map(|r| r.arch).collect::<Vec<_>>(),
        vec![1, 2, 0]
    );
    assert!(rows[2].is_ir && !rows[0].is_ir);
}

#[test]
fn each_install_step_has_its_stage() {
    let a2 = Some(Tone3000Architecture::A2);
    let (_w, rows) = drawn(vec![
        row(RowInstall::Running(InstallProgress::Fetching), a2),
        row(
            RowInstall::Running(InstallProgress::Downloading { done: 2, total: 6 }),
            a2,
        ),
        row(RowInstall::Running(InstallProgress::Measuring), a2),
    ]);
    assert!(rows.iter().all(|r| r.installing && !r.installed));
    assert_eq!(
        rows.iter()
            .map(|r| (r.stage, r.done, r.total))
            .collect::<Vec<_>>(),
        vec![(0, 0, 0), (1, 2, 6), (2, 0, 0)]
    );
}

#[test]
fn installed_and_failed_rows_read_as_such() {
    let a2 = Some(Tone3000Architecture::A2);
    let (_w, rows) = drawn(vec![
        row(RowInstall::Installed, a2),
        row(RowInstall::Failed("TONE3000 rate limit reached".into()), a2),
    ]);
    assert!(rows[0].installed && rows[0].error.is_empty());
    assert!(!rows[1].installed && !rows[1].installing);
    assert_eq!(rows[1].error, "TONE3000 rate limit reached");
}
