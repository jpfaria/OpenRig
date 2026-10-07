//! #398: the light/dark scheme is a per-machine `config.yaml` choice.

use crate::{AppConfig, Appearance};

#[test]
fn a_config_without_the_field_follows_the_system() {
    let config: AppConfig = serde_yaml::from_str("recent_projects: []\n").unwrap();
    assert_eq!(config.appearance, Appearance::System);
}

#[test]
fn a_forced_scheme_is_read_back() {
    let config: AppConfig = serde_yaml::from_str("appearance: dark\n").unwrap();
    assert_eq!(config.appearance, Appearance::Dark);
}

#[test]
fn the_choice_is_written_in_lower_case() {
    let config = AppConfig {
        appearance: Appearance::Light,
        ..AppConfig::default()
    };
    let yaml = serde_yaml::to_string(&config).unwrap();
    assert!(yaml.contains("appearance: light"), "{yaml}");
}
