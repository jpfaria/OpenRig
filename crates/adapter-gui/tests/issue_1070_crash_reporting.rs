//! Issue #1070 — every crash report carries the audio setup that was running
//! (interface, rate, buffer, chains, blocks) and the host machine (CPU, RAM).
//! The app talks to a vendor-neutral `CrashReporter`; Sentry is one
//! implementation, picked by `config.yaml`.

use std::sync::{Arc, Mutex, OnceLock};

use adapter_gui::crash_context::{contexts, publish_audio};
use adapter_gui::crash_context_audio::audio_context;
use adapter_gui::crash_context_host::host_context;
use adapter_gui::crash_log_bridge::ReportingLogger;
use adapter_gui::crash_reporter::{CrashReporter, Report};
use adapter_gui::crash_reporter_sentry::SentryReporter;
use adapter_gui::crash_reporting::{choose, Backend};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_filesystem::crash_reporting_config::{CrashReportingConfig, CrashReportingProvider};
use project::block::{AudioBlock, AudioBlockKind, NamBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;
use sentry::protocol::Context;
use serde_json::{json, Value};

const DEVICE: &str = "coreaudio:Quantum HD 8:11";

fn endpoint(name: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEVICE.into()),
        mode: ChannelMode::Mono,
        channels,
    }
}

fn fixture() -> (Project, Vec<IoBinding>) {
    let binding = IoBinding {
        id: "rig".into(),
        name: "Quantum HD 8".into(),
        inputs: vec![endpoint("Guitar 4", vec![3])],
        outputs: vec![endpoint("Main", vec![0, 1])],
    };
    let chain = Chain {
        id: ChainId("rig:input-4".into()),
        description: Some("DIGITAL".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["rig".into()],
        blocks: vec![AudioBlock {
            id: BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Nam(NamBlock {
                model: "nam_vox_ac30_a2".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    };
    let project = Project {
        name: None,
        device_settings: vec![DeviceSettings {
            device_id: DeviceId(DEVICE.into()),
            sample_rate: 44_100,
            buffer_size_frames: 64,
            bit_depth: 32,
            #[cfg(target_os = "linux")]
            realtime: true,
            #[cfg(target_os = "linux")]
            rt_priority: 70,
            #[cfg(target_os = "linux")]
            nperiods: 3,
        }],
        chains: vec![chain],
        midi: None,
    };
    (project, vec![binding])
}

fn publish_fixture() {
    let (project, bindings) = fixture();
    publish_audio(audio_context(&project, &bindings, Some(44_100)));
}

/// Records what the app hands a reporter — no vendor involved.
#[derive(Default)]
struct Recorder {
    reports: Mutex<Vec<(String, String, Value)>>,
    breadcrumbs: Mutex<Vec<(log::Level, String)>>,
}

impl CrashReporter for Recorder {
    fn capture(&self, report: Report<'_>) {
        let contexts = serde_json::to_value(&report.contexts).unwrap();
        self.reports.lock().unwrap().push((
            report.target.to_string(),
            report.message.to_string(),
            contexts,
        ));
    }
    fn breadcrumb(&self, level: log::Level, _target: &str, message: &str) {
        self.breadcrumbs
            .lock()
            .unwrap()
            .push((level, message.to_string()));
    }
    fn flush(&self, _timeout: std::time::Duration) {}
}

struct Shared(Arc<Recorder>);

impl CrashReporter for Shared {
    fn capture(&self, report: Report<'_>) {
        self.0.capture(report)
    }
    fn breadcrumb(&self, level: log::Level, target: &str, message: &str) {
        self.0.breadcrumb(level, target, message)
    }
    fn flush(&self, timeout: std::time::Duration) {
        self.0.flush(timeout)
    }
}

fn log(logger: &dyn log::Log, level: log::Level, message: &str) {
    logger.log(
        &log::Record::builder()
            .level(level)
            .target("openrig::issue_1070")
            .args(format_args!("{message}"))
            .build(),
    );
}

#[test]
fn issue_1070_audio_context_names_interface_rate_buffer_and_chain() {
    let (project, bindings) = fixture();
    let ctx = audio_context(&project, &bindings, Some(44_100));

    assert_eq!(ctx["live_sample_rate"], json!(44_100));
    assert!(ctx["backend"].is_string(), "{ctx:?}");
    let device = &ctx["devices"][0];
    assert_eq!(device["device_id"], json!(DEVICE));
    assert_eq!(device["sample_rate"], json!(44_100));
    assert_eq!(device["buffer_size_frames"], json!(64));

    let chain = &ctx["chains"][0];
    assert_eq!(chain["id"], json!("rig:input-4"));
    assert_eq!(chain["description"], json!("DIGITAL"));
    assert_eq!(chain["enabled"], json!(true));
    assert_eq!(chain["io_bindings"][0]["name"], json!("Quantum HD 8"));
    assert_eq!(chain["io_bindings"][0]["inputs"][0]["channels"], json!([3]));
    assert_eq!(
        chain["io_bindings"][0]["outputs"][0]["device_id"],
        json!(DEVICE)
    );
    assert_eq!(chain["blocks"][0], json!("nam/nam_vox_ac30_a2"));
}

#[test]
fn issue_1070_audio_context_without_runtime_has_no_live_rate() {
    let (project, bindings) = fixture();
    let ctx = audio_context(&project, &bindings, None);
    assert_eq!(ctx["live_sample_rate"], Value::Null);
}

#[test]
fn issue_1070_host_context_reports_cpu_and_memory() {
    let ctx = host_context();
    assert!(ctx["cpu_cores"].as_u64().unwrap_or(0) > 0, "{ctx:?}");
    assert!(ctx["memory_total_mb"].as_u64().unwrap_or(0) > 0, "{ctx:?}");
    // A parallel sample holds the process sampler: then it says so instead.
    assert!(
        ctx["process_memory_mb"].as_u64().is_some() || ctx["process_sampling"] == json!("busy"),
        "{ctx:?}"
    );
    assert!(ctx["cpu_brand"].is_string(), "{ctx:?}");
}

#[test]
fn issue_1070_contexts_carry_the_published_audio_and_a_host_sample() {
    publish_fixture();
    let all = contexts();
    assert_eq!(all["audio"]["chains"][0]["id"], json!("rig:input-4"));
    assert!(all["host"]["memory_total_mb"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn issue_1070_the_config_picks_the_reporter() {
    let sentry = |dsn: Option<&str>| CrashReportingConfig {
        provider: CrashReportingProvider::Sentry,
        dsn: dsn.map(str::to_string),
    };
    let off = CrashReportingConfig {
        provider: CrashReportingProvider::None,
        dsn: Some("https://k@example.com/1".into()),
    };

    assert_eq!(choose(&off, Some("https://b@example.com/2")), None);
    assert_eq!(
        choose(
            &sentry(Some("https://k@example.com/1")),
            Some("https://b@example.com/2")
        ),
        Some(Backend::Sentry {
            dsn: "https://k@example.com/1".into()
        })
    );
    assert_eq!(
        choose(&sentry(None), Some("https://b@example.com/2")),
        Some(Backend::Sentry {
            dsn: "https://b@example.com/2".into()
        })
    );
    assert_eq!(choose(&sentry(None), None), None);
    assert_eq!(choose(&sentry(Some("")), None), None);
}

/// The log bridge speaks only the interface: `error!` is a report with the
/// contexts, `warn!`/`info!` are breadcrumbs, `debug!` stays local.
#[test]
fn issue_1070_the_log_bridge_reports_errors_with_contexts() {
    static SLOT: OnceLock<Box<dyn CrashReporter>> = OnceLock::new();
    let recorder = Arc::new(Recorder::default());
    let _ = SLOT.set(Box::new(Shared(recorder.clone())));
    let logger = ReportingLogger::new(Box::new(env_logger::Logger::from_default_env()), &SLOT);

    publish_fixture();
    log(&logger, log::Level::Info, "chain started");
    log(&logger, log::Level::Debug, "noise");
    log(
        &logger,
        log::Level::Error,
        "audio overload on chain 'rig:input-4'",
    );

    let reports = recorder.reports.lock().unwrap();
    assert_eq!(reports.len(), 1);
    let (target, message, contexts) = &reports[0];
    assert_eq!(target, "openrig::issue_1070");
    assert_eq!(message, "audio overload on chain 'rig:input-4'");
    assert_eq!(
        contexts["audio"]["devices"][0]["buffer_size_frames"],
        json!(64)
    );
    assert!(contexts["host"].is_object());
    let crumbs = recorder.breadcrumbs.lock().unwrap();
    assert_eq!(
        *crumbs,
        vec![(log::Level::Info, "chain started".to_string())]
    );
}

/// The Sentry implementation over sentry's in-memory transport: a report
/// leaves as an event with the release and both contexts.
#[test]
fn issue_1070_the_sentry_reporter_sends_the_contexts() {
    publish_fixture();
    let events = sentry::test::with_captured_events_options(
        || {
            SentryReporter::on_current_hub().capture(Report {
                target: "openrig::issue_1070",
                message: "audio overload on chain 'rig:input-4'",
                contexts: contexts(),
            });
        },
        SentryReporter::options(),
    );

    assert_eq!(events.len(), 1, "{events:?}");
    let event = &events[0];
    assert_eq!(
        event.message.as_deref(),
        Some("audio overload on chain 'rig:input-4'")
    );
    assert!(event
        .release
        .as_deref()
        .unwrap_or("")
        .starts_with("openrig@"));
    let Some(Context::Other(audio)) = event.contexts.get("audio") else {
        panic!("no audio context: {:?}", event.contexts.keys());
    };
    assert_eq!(
        audio["chains"][0]["blocks"][0],
        json!("nam/nam_vox_ac30_a2")
    );
    assert!(matches!(
        event.contexts.get("host"),
        Some(Context::Other(_))
    ));
}

#[test]
fn issue_1070_a_malformed_dsn_disables_sentry_instead_of_panicking() {
    assert!(SentryReporter::start("not a dsn").is_none());
}

/// Sends ONE real event when `OPENRIG_SENTRY_SMOKE_DSN` is set, so its
/// contexts can be read back with `scripts/sentry.py`; skips loudly otherwise.
#[test]
fn issue_1070_smoke_real_sentry_event() {
    let Ok(dsn) = std::env::var("OPENRIG_SENTRY_SMOKE_DSN") else {
        eprintln!("SKIP issue_1070_smoke_real_sentry_event: OPENRIG_SENTRY_SMOKE_DSN not set");
        return;
    };
    let reporter = SentryReporter::start(&dsn).expect("valid smoke DSN");
    publish_fixture();
    reporter.capture(Report {
        target: "openrig::issue_1070",
        message: "issue-1070 smoke: audio overload on chain 'rig:input-4'",
        contexts: contexts(),
    });
    reporter.flush(std::time::Duration::from_secs(10));
}
