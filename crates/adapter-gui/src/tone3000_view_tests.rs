use application::tone3000::api_types::{Page, Tone, User};
use application::tone3000::install::InstallProgress;
use application::tone3000::Tone3000Architecture;
use application::tone3000_state::{
    Tone3000InstallEntry, Tone3000InstalledEntry, Tone3000SearchSnapshot, Tone3000Snapshot,
};
use plugin_loader::manifest::{BlockType, NamArchitecture};

use super::*;

fn tone(id: u64, format: &str, a1: u32, a2: u32) -> Tone {
    Tone {
        id,
        title: format!("Tone {id}"),
        description: None,
        tags: vec![],
        makes: vec![],
        images: vec![],
        user: Some(User {
            username: "player".into(),
            display_name: Some("The Player".into()),
            url: None,
        }),
        url: None,
        format: Some(format.into()),
        gear: Some("amp".into()),
        license: None,
        models_count: a1 + a2,
        a1_models_count: a1,
        a2_models_count: a2,
        irs_count: if format == "ir" { 3 } else { 0 },
        downloads_count: 15_300,
        favorites_count: 0,
        updated_at: None,
    }
}

fn snapshot(tones: Vec<Tone>) -> Tone3000Snapshot {
    Tone3000Snapshot {
        key_configured: true,
        can_install: true,
        search: Tone3000SearchSnapshot {
            query: None,
            in_flight: false,
            results: Some(Page {
                data: tones,
                page: 2,
                page_size: 25,
                total: 60,
                total_pages: 3,
            }),
            error: None,
        },
        installs: vec![],
        installed: vec![],
    }
}

fn installed(plugin_id: &str, arch: Option<NamArchitecture>) -> Tone3000InstalledEntry {
    let tone_id = plugin_id
        .strip_prefix("tone3000_")
        .and_then(|rest| rest.split('_').next())
        .and_then(|id| id.parse().ok());
    Tone3000InstalledEntry {
        plugin_id: plugin_id.into(),
        tone_ids: tone_id.into_iter().collect(),
        display_name: "Plexi".into(),
        block_type: BlockType::GainPedal,
        architecture: arch,
        captures: 7,
        removable: true,
        updated_at: None,
        dir: std::path::PathBuf::from("/plugins/nam").join(plugin_id),
    }
}

fn built_in(plugin_id: &str, name: &str, tone_id: u64) -> Tone3000InstalledEntry {
    Tone3000InstalledEntry {
        plugin_id: plugin_id.into(),
        tone_ids: vec![tone_id],
        display_name: name.into(),
        block_type: BlockType::Preamp,
        architecture: Some(NamArchitecture::A2),
        captures: 4,
        removable: false,
        updated_at: None,
        dir: std::path::PathBuf::from("/app/plugins/nam").join(plugin_id),
    }
}

fn view(s: &Tone3000Snapshot) -> Tone3000View {
    tone3000_view(s, &ArchChoices::new(), "")
}

#[test]
fn a_nam_tone_installs_as_a2_when_it_has_a2_captures() {
    let view = tone3000_view(
        &snapshot(vec![tone(1, "nam", 4, 2)]),
        &ArchChoices::new(),
        "",
    );
    assert_eq!(view.results[0].arch, Some(Tone3000Architecture::A2));
}

#[test]
fn a_nam_tone_without_a2_captures_installs_as_a1() {
    let view = tone3000_view(
        &snapshot(vec![tone(1, "nam", 4, 0)]),
        &ArchChoices::new(),
        "",
    );
    assert_eq!(view.results[0].arch, Some(Tone3000Architecture::A1));
}

#[test]
fn the_users_pick_wins_over_the_default_architecture() {
    let choices = ArchChoices::from([(1, Tone3000Architecture::A1)]);
    let view = tone3000_view(&snapshot(vec![tone(1, "nam", 4, 2)]), &choices, "");
    assert_eq!(view.results[0].arch, Some(Tone3000Architecture::A1));
}

#[test]
fn an_ir_tone_has_no_architecture() {
    let view = tone3000_view(
        &snapshot(vec![tone(1, "ir", 0, 0)]),
        &ArchChoices::new(),
        "",
    );
    assert_eq!(view.results[0].arch, None);
    assert_eq!(view.results[0].ir_count, 3);
}

#[test]
fn a_row_carries_the_name_author_gear_and_downloads() {
    let view = tone3000_view(
        &snapshot(vec![tone(9, "nam", 4, 2)]),
        &ArchChoices::new(),
        "",
    );
    let row = &view.results[0];
    assert_eq!(row.tone_id, 9);
    assert_eq!(row.title, "Tone 9");
    assert_eq!(row.author, "The Player");
    assert_eq!(row.gear, "amp");
    assert_eq!(row.downloads, "15.3k");
    assert_eq!((row.a1_count, row.a2_count), (4, 2));
}

#[test]
fn the_page_and_totals_come_from_the_last_search() {
    let view = tone3000_view(&snapshot(vec![]), &ArchChoices::new(), "");
    assert_eq!((view.page, view.total_pages, view.total), (2, 3, 60));
    assert!(view.searched);
    assert!(!view.searching);
}

#[test]
fn before_any_search_nothing_was_searched() {
    let view = tone3000_view(&Tone3000Snapshot::default(), &ArchChoices::new(), "");
    assert!(!view.searched);
    assert!(view.results.is_empty());
    assert_eq!(view.page, 0);
}

#[test]
fn a_failed_search_shows_its_message() {
    let mut s = Tone3000Snapshot::default();
    s.search.error = Some("TONE3000 rejected the API key".into());
    let view = tone3000_view(&s, &ArchChoices::new(), "");
    assert_eq!(view.error, "TONE3000 rejected the API key");
    assert!(view.searched);
}

#[test]
fn a_tone_is_installed_only_for_the_architecture_on_disk() {
    let mut s = snapshot(vec![tone(5, "nam", 4, 2)]);
    s.installed = vec![installed("tone3000_5_a1", Some(NamArchitecture::A1))];
    let default = tone3000_view(&s, &ArchChoices::new(), "");
    assert_eq!(default.results[0].install, RowInstall::Idle);
    let picked = tone3000_view(&s, &ArchChoices::from([(5, Tone3000Architecture::A1)]), "");
    assert_eq!(
        picked.results[0].install,
        RowInstall::Installed(Some("tone3000_5_a1".into()))
    );
}

#[test]
fn an_installed_ir_tone_reads_installed() {
    let mut s = snapshot(vec![tone(5, "ir", 0, 0)]);
    s.installed = vec![installed("tone3000_5", None)];
    let view = tone3000_view(&s, &ArchChoices::new(), "");
    assert_eq!(
        view.results[0].install,
        RowInstall::Installed(Some("tone3000_5".into()))
    );
}

#[test]
fn a_running_install_shows_its_progress() {
    let mut s = snapshot(vec![tone(5, "nam", 4, 2)]);
    s.installs = vec![Tone3000InstallEntry {
        tone_id: 5,
        progress: InstallProgress::Downloading { done: 2, total: 6 },
        error: None,
    }];
    let view = tone3000_view(&s, &ArchChoices::new(), "");
    assert_eq!(
        view.results[0].install,
        RowInstall::Running(InstallProgress::Downloading { done: 2, total: 6 })
    );
}

#[test]
fn a_failed_install_shows_its_reason() {
    let mut s = snapshot(vec![tone(5, "nam", 4, 2)]);
    s.installs = vec![Tone3000InstallEntry {
        tone_id: 5,
        progress: InstallProgress::Fetching,
        error: Some("TONE3000 rate limit reached".into()),
    }];
    let view = tone3000_view(&s, &ArchChoices::new(), "");
    assert_eq!(
        view.results[0].install,
        RowInstall::Failed("TONE3000 rate limit reached".into())
    );
}

#[test]
fn an_installed_package_row_names_its_block_architecture_and_captures() {
    let mut s = Tone3000Snapshot::default();
    s.installed = vec![
        installed("tone3000_5_a2", Some(NamArchitecture::A2)),
        installed("tone3000_6", None),
    ];
    let view = tone3000_view(&s, &ArchChoices::new(), "");
    assert_eq!(
        view.installed[0],
        InstalledRowView {
            plugin_id: "tone3000_5_a2".into(),
            name: "Plexi".into(),
            block_type: "gain_pedal".into(),
            arch: "A2".into(),
            captures: 7,
            removable: true,
        }
    );
    assert_eq!(view.installed[1].arch, "");
}

#[test]
fn counts_read_short() {
    assert_eq!(compact_count(0), "0");
    assert_eq!(compact_count(999), "999");
    assert_eq!(compact_count(1_000), "1k");
    assert_eq!(compact_count(15_340), "15.3k");
    assert_eq!(compact_count(2_500_000), "2.5M");
}

#[test]
fn a_built_in_tone_reads_installed_but_cannot_be_removed() {
    let mut s = snapshot(vec![tone(52557, "nam", 2, 2)]);
    s.installed = vec![built_in(
        "nam_synergy_dumble_os_a2",
        "Dumble OS Module",
        52557,
    )];
    assert_eq!(view(&s).results[0].install, RowInstall::Installed(None));
    let a1 = tone3000_view(
        &s,
        &ArchChoices::from([(52557, Tone3000Architecture::A1)]),
        "",
    );
    assert_eq!(a1.results[0].install, RowInstall::Idle);
    assert!(!view(&s).installed[0].removable);
}

#[test]
fn the_installed_list_filters_by_name_ignoring_case() {
    let mut s = Tone3000Snapshot::default();
    s.installed = vec![
        built_in("nam_synergy_dumble_os_a2", "Dumble OS Module", 1),
        built_in("nam_ada_mp_1", "MP-1", 2),
    ];
    let filtered = tone3000_view(&s, &ArchChoices::new(), "dumble");
    let names: Vec<&str> = filtered.installed.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["Dumble OS Module"]);
    assert_eq!(filtered.installed_total, 2);
    assert_eq!(view(&s).installed.len(), 2);
}

fn versioned(tone_version: Option<&str>, on_disk: Option<&str>) -> Tone3000Snapshot {
    let mut published = tone(5, "nam", 0, 2);
    published.updated_at = tone_version.map(Into::into);
    let mut s = snapshot(vec![published]);
    s.installed = vec![Tone3000InstalledEntry {
        updated_at: on_disk.map(Into::into),
        ..installed("tone3000_5_a2", Some(NamArchitecture::A2))
    }];
    s
}

#[test]
fn a_newer_tone_on_tone3000_offers_the_update() {
    let s = versioned(Some("2026-06-15T08:30:00Z"), Some("2026-05-01T10:00:00Z"));
    assert_eq!(
        view(&s).results[0].install,
        RowInstall::Outdated("tone3000_5_a2".into())
    );
}

#[test]
fn the_same_version_reads_installed() {
    let s = versioned(Some("2026-05-01T10:00:00Z"), Some("2026-05-01T10:00:00Z"));
    assert_eq!(
        view(&s).results[0].install,
        RowInstall::Installed(Some("tone3000_5_a2".into()))
    );
}

#[test]
fn an_unknown_version_never_offers_the_update() {
    for (published, on_disk) in [
        (Some("2026-06-15T08:30:00Z"), None),
        (None, Some("2026-05-01T10:00:00Z")),
    ] {
        let s = versioned(published, on_disk);
        assert_eq!(
            view(&s).results[0].install,
            RowInstall::Installed(Some("tone3000_5_a2".into())),
            "{published:?} vs {on_disk:?}"
        );
    }
}

#[test]
fn a_built_in_tone_is_never_updated_by_the_browser() {
    let mut published = tone(52557, "nam", 2, 2);
    published.updated_at = Some("2026-06-15T08:30:00Z".into());
    let mut s = snapshot(vec![published]);
    s.installed = vec![Tone3000InstalledEntry {
        updated_at: Some("2026-05-01T10:00:00Z".into()),
        ..built_in("nam_synergy_dumble_os_a2", "Dumble OS Module", 52557)
    }];
    assert_eq!(view(&s).results[0].install, RowInstall::Installed(None));
}
