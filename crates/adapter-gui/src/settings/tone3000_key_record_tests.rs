//! #879 — saving the TONE3000 Secret Key from the settings screen.

use std::cell::RefCell;
use std::rc::Rc;

use infra_filesystem::AppConfig;
use project::project::Project;

use super::tone3000_key_record::{normalize_key, record_tone3000_key, KeyRecord};
use crate::state::ProjectSession;

fn open_session() -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-879-tone3000-key-tests"),
    ))))
}

fn config() -> Rc<RefCell<AppConfig>> {
    Rc::new(RefCell::new(AppConfig::default()))
}

#[test]
fn a_key_is_trimmed_and_blank_means_none() {
    assert_eq!(normalize_key("  t3k_cs_x \n"), Some("t3k_cs_x".into()));
    assert_eq!(normalize_key("   "), None);
}

#[test]
fn in_the_launcher_the_snapshot_moves_and_the_caller_writes() {
    let cfg = config();
    let record = record_tone3000_key(&Rc::new(RefCell::new(None)), &cfg, " t3k_cs_x ");
    assert_eq!(
        record,
        KeyRecord {
            configured: true,
            on_the_bus: false
        }
    );
    assert_eq!(cfg.borrow().tone3000.api_key.as_deref(), Some("t3k_cs_x"));
}

#[test]
fn with_a_project_the_key_reaches_the_browser() {
    let session = open_session();
    let cfg = config();
    let record = record_tone3000_key(&session, &cfg, "t3k_cs_x");
    assert!(record.on_the_bus && record.configured);
    let snapshot = session
        .borrow()
        .as_ref()
        .unwrap()
        .dispatcher
        .tone3000_snapshot();
    assert!(snapshot.key_configured);
}

#[test]
fn clearing_the_key_clears_it_everywhere() {
    let session = open_session();
    let cfg = config();
    record_tone3000_key(&session, &cfg, "t3k_cs_x");
    let record = record_tone3000_key(&session, &cfg, "");
    assert!(!record.configured);
    assert_eq!(cfg.borrow().tone3000.api_key, None);
    let snapshot = session
        .borrow()
        .as_ref()
        .unwrap()
        .dispatcher
        .tone3000_snapshot();
    assert!(!snapshot.key_configured);
}
