//! #1070: which crash reporter runs is a per-machine `config.yaml` choice.

use super::{CrashReportingConfig, CrashReportingProvider};
use crate::AppConfig;

#[test]
fn a_config_without_the_section_reports_through_sentry_with_the_build_dsn() {
    let config: AppConfig = serde_yaml::from_str("recent_projects: []\n").unwrap();
    assert_eq!(
        config.crash_reporting.provider,
        CrashReportingProvider::Sentry
    );
    assert_eq!(config.crash_reporting.dsn, None);
}

#[test]
fn the_provider_can_be_turned_off() {
    let config: AppConfig = serde_yaml::from_str("crash_reporting:\n  provider: none\n").unwrap();
    assert_eq!(
        config.crash_reporting.provider,
        CrashReportingProvider::None
    );
}

#[test]
fn a_dsn_in_the_config_round_trips() {
    let config = AppConfig {
        crash_reporting: CrashReportingConfig {
            provider: CrashReportingProvider::Sentry,
            dsn: Some("https://key@example.com/1".into()),
        },
        ..AppConfig::default()
    };
    let yaml = serde_yaml::to_string(&config).unwrap();
    let restored: AppConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(restored.crash_reporting, config.crash_reporting);
}
