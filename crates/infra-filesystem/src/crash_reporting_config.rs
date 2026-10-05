//! Responsibility: describes which crash reporter a machine sends to (#1070).
//!
//! ADR 0003 puts this in the SYSTEM `config.yaml`: where a machine reports
//! its crashes is about that machine, not about the rig it plays.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CrashReportingConfig {
    #[serde(default)]
    pub provider: CrashReportingProvider,
    /// Overrides the DSN baked into release builds. `None` keeps the build's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dsn: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CrashReportingProvider {
    #[default]
    Sentry,
    /// Nothing leaves the machine; the session log on disk still records all.
    None,
}

#[cfg(test)]
#[path = "crash_reporting_config_tests.rs"]
mod tests;
