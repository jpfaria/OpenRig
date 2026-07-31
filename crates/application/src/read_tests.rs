use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::bridge::QueryKind;
use crate::live_source::{ChainMeterReading, LiveSource, NoLiveSource};
use domain::ids::{BlockId, ChainId};
use project::block::types::{AudioBlock, AudioBlockKind, InputBlock};
use project::chain::{Chain, LooperConfig, LooperSpeed};
use project::project::Project;

/// Every variant, so a new one cannot be added without deciding what an
/// empty frontend answers. Kept exhaustive by `match_all_kinds` below.
fn all_kinds() -> Vec<QueryKind> {
    let chain = ChainId("guitar".to_string());
    vec![
        QueryKind::ProjectYaml,
        QueryKind::Devices,
        QueryKind::Ids,
        QueryKind::ChainMeters,
        QueryKind::TunerReadings,
        QueryKind::SpectrumReadings,
        QueryKind::DiLoopState,
        QueryKind::ChainLoopers {
            chain: chain.clone(),
        },
        QueryKind::ChainLatency {
            chain: chain.clone(),
        },
        QueryKind::ListChainPresets {
            chain: chain.clone(),
        },
        QueryKind::ListProjectPresets,
        QueryKind::ListPluginCatalog,
        QueryKind::GetPlugin {
            id: "x".to_string(),
        },
        QueryKind::FindPlugins {
            query: String::new(),
        },
        QueryKind::GetPluginParams {
            plugin_id: "x".to_string(),
        },
        QueryKind::GetBlockParams {
            chain: chain.clone(),
            block: BlockId("b".to_string()),
        },
        QueryKind::Paths,
        QueryKind::ChainQualityReport {
            chain: chain.clone(),
        },
        QueryKind::ChainToneReport { chain },
        QueryKind::MetronomeState,
    ]
}

/// Names of every `QueryKind`, in one place. Adding a variant forces a new
/// arm in `match_all_kinds` (exhaustive match) AND a new entry here (fixed
/// array length), and `all_kinds_covers_every_variant` then fails until
/// `all_kinds` lists it too — the loop below cannot silently skip a kind.
const KIND_NAMES: [&str; 20] = [
    "ProjectYaml",
    "Devices",
    "Ids",
    "ChainMeters",
    "TunerReadings",
    "SpectrumReadings",
    "DiLoopState",
    "ChainLoopers",
    "ChainLatency",
    "ListChainPresets",
    "ListProjectPresets",
    "ListPluginCatalog",
    "GetPlugin",
    "FindPlugins",
    "GetPluginParams",
    "GetBlockParams",
    "Paths",
    "ChainQualityReport",
    "ChainToneReport",
    "MetronomeState",
];

fn match_all_kinds(kind: &QueryKind) -> &'static str {
    match kind {
        QueryKind::ProjectYaml => "ProjectYaml",
        QueryKind::Devices => "Devices",
        QueryKind::Ids => "Ids",
        QueryKind::ChainMeters => "ChainMeters",
        QueryKind::TunerReadings => "TunerReadings",
        QueryKind::SpectrumReadings => "SpectrumReadings",
        QueryKind::DiLoopState => "DiLoopState",
        QueryKind::ChainLoopers { .. } => "ChainLoopers",
        QueryKind::ChainLatency { .. } => "ChainLatency",
        QueryKind::ListChainPresets { .. } => "ListChainPresets",
        QueryKind::ListProjectPresets => "ListProjectPresets",
        QueryKind::ListPluginCatalog => "ListPluginCatalog",
        QueryKind::GetPlugin { .. } => "GetPlugin",
        QueryKind::FindPlugins { .. } => "FindPlugins",
        QueryKind::GetPluginParams { .. } => "GetPluginParams",
        QueryKind::GetBlockParams { .. } => "GetBlockParams",
        QueryKind::Paths => "Paths",
        QueryKind::ChainQualityReport { .. } => "ChainQualityReport",
        QueryKind::ChainToneReport { .. } => "ChainToneReport",
        QueryKind::MetronomeState => "MetronomeState",
    }
}

fn test_project_with_one_chain() -> Project {
    Project {
        name: Some("Parity".to_string()),
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("guitar".to_string()),
            description: None,
            instrument: "guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![AudioBlock {
                id: BlockId("b".to_string()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "default".to_string(),
                    io: String::new(),
                    endpoint: String::new(),
                }),
            }],
            di_output: None,
            // A persisted looper, so the looper arm is exercised against a
            // chain that actually has one — an empty `loopers` vec would
            // pass whatever the rate resolution did.
            loopers: vec![LooperConfig {
                uid: 1,
                mix: 0.8,
                decay: 0.5,
                speed: LooperSpeed::Normal,
                reverse: false,
                audio_file: None,
                input: None,
                output: None,
                preset: None,
            }],
        }],
        midi: None,
    }
}

fn rc_project(project: &Project) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(project.clone()))
}

/// A frontend that hosts meters and nothing else.
struct MetersOnly(Vec<ChainMeterReading>);

impl LiveSource for MetersOnly {
    fn chain_meters(&self) -> Option<Vec<ChainMeterReading>> {
        Some(
            self.0
                .iter()
                .map(|r| ChainMeterReading {
                    chain: r.chain.clone(),
                    in_dbfs: r.in_dbfs,
                    out_dbfs: r.out_dbfs,
                })
                .collect(),
        )
    }
}

/// A frontend that hosts a device source whose enumeration FAILED — a dead
/// audio host, or a JACK server that is down.
struct BrokenDeviceHost;

impl LiveSource for BrokenDeviceHost {
    fn devices(&self) -> Option<Result<Vec<String>, String>> {
        Some(Err("no audio host".to_string()))
    }
}

/// A frontend that enumerates devices successfully.
struct DeviceHost;

impl LiveSource for DeviceHost {
    fn devices(&self) -> Option<Result<Vec<String>, String>> {
        Some(Ok(vec!["Scarlett".to_string(), "Built-in".to_string()]))
    }
}

/// A frontend that hosts looper transport state but could NOT resolve this
/// chain's own rate (e.g. its device/endpoint could not be found) — a real
/// per-chain failure, never a fabricated rate (issue #723).
struct BrokenLooperHost;

impl LiveSource for BrokenLooperHost {
    fn chain_loopers(
        &self,
        _chain: &ChainId,
    ) -> Option<Result<(Vec<engine::LooperStatus>, u32), String>> {
        Some(Err("no resolved sample rate for chain guitar".to_string()))
    }
}

/// A frontend that hosts loopers and resolved a real (non-48kHz) rate.
struct HostedLooperSource;

impl LiveSource for HostedLooperSource {
    fn chain_loopers(
        &self,
        _chain: &ChainId,
    ) -> Option<Result<(Vec<engine::LooperStatus>, u32), String>> {
        Some(Ok((vec![], 96_000)))
    }
}

/// A frontend whose rig is STOPPED but whose chain resolves to a 44.1 kHz
/// interface — the scenario Task 11 regressed: with no `device_settings` entry
/// for that device, the probe fell back to the dispatcher's `engine_sr`, which
/// since #127 is `REFERENCE_SAMPLE_RATE` once nothing is running.
struct StoppedRigOnA44kInterface;

impl LiveSource for StoppedRigOnA44kInterface {
    fn chain_sample_rate(&self, _chain: &ChainId) -> Option<f32> {
        Some(44_100.0)
    }
}

fn resolve_hosted_by(live: &dyn LiveSource, kind: QueryKind) -> Result<String, String> {
    let project = test_project_with_one_chain();
    let dispatcher = crate::local_dispatcher::LocalDispatcher::new(rc_project(&project));
    let ctx = ReadContext {
        project: &project,
        rig: None,
        io_bindings: &[],
        dispatcher: &dispatcher,
        live,
    };
    resolve(&kind, &ctx)
}

fn resolve_empty(kind: QueryKind) -> String {
    resolve_hosted_by(&NoLiveSource, kind).expect("empty frontend must still answer")
}

fn resolve_with_meters(meters: Vec<ChainMeterReading>) -> String {
    resolve_hosted_by(&MetersOnly(meters), QueryKind::ChainMeters)
        .expect("hosted meters must answer")
}

#[test]
fn all_kinds_covers_every_variant() {
    let listed: Vec<&'static str> = all_kinds().iter().map(match_all_kinds).collect();
    for name in KIND_NAMES {
        assert!(
            listed.contains(&name),
            "{name} is a QueryKind variant that all_kinds() does not exercise"
        );
    }
    assert_eq!(
        listed.len(),
        KIND_NAMES.len(),
        "duplicate kind in all_kinds"
    );
}

#[test]
fn every_kind_answers_on_a_frontend_that_hosts_nothing() {
    let project = test_project_with_one_chain();
    let dispatcher = crate::local_dispatcher::LocalDispatcher::new(rc_project(&project));
    let ctx = ReadContext {
        project: &project,
        rig: None,
        io_bindings: &[],
        dispatcher: &dispatcher,
        live: &NoLiveSource,
    };
    for kind in all_kinds() {
        let out = resolve(&kind, &ctx);
        match kind {
            // The preset reads need a `RigProject`, which is a session
            // attachment, not a live source: no rig is a genuine failure,
            // not an unhosted reading. It still has to be the SAME failure
            // on every transport — one string, not per-adapter prose.
            QueryKind::ListChainPresets { .. } | QueryKind::ListProjectPresets => {
                assert_eq!(out, Err(NO_RIG_ATTACHED.to_string()), "{kind:?}");
            }
            _ => assert!(
                out.is_ok(),
                "{kind:?} refused on a frontend that hosts nothing — every resource \
                 must stay addressable with the documented empty shape: {out:?}"
            ),
        }
    }
}

#[test]
fn silent_meters_use_the_engine_constant_not_a_literal() {
    let meters = resolve_empty(QueryKind::ChainMeters);
    let expected = format!(
        "guitar\t{:.1}\t{:.1}\n",
        engine::output_meter::SILENT_DBFS,
        engine::output_meter::SILENT_DBFS
    );
    assert_eq!(meters, expected);
}

#[test]
fn a_hosting_frontend_returns_the_same_shape_with_live_values() {
    // Same field layout, different numbers — a client cannot tell which
    // adapter served it apart from the values.
    let hosted = resolve_with_meters(vec![ChainMeterReading {
        chain: ChainId("guitar".to_string()),
        in_dbfs: -12.0,
        out_dbfs: -6.0,
    }]);
    let empty = resolve_empty(QueryKind::ChainMeters);
    assert_eq!(hosted.lines().count(), empty.lines().count());
    assert_eq!(hosted.split('\t').count(), empty.split('\t').count());
    assert!(hosted.starts_with("guitar\t-12.0\t-6.0"), "{hosted}");
}

#[test]
fn unhosted_analyzers_report_not_running_with_no_rows() {
    let tuner = resolve_empty(QueryKind::TunerReadings);
    assert!(tuner.contains("\"running\":false"), "{tuner}");
    assert!(tuner.contains("\"rows\":[]"), "{tuner}");
    let spectrum = resolve_empty(QueryKind::SpectrumReadings);
    assert!(spectrum.contains("\"running\":false"), "{spectrum}");
    assert!(spectrum.contains("\"rows\":[]"), "{spectrum}");
}

#[test]
fn unhosted_di_loop_reports_one_silent_row_per_chain() {
    let di = resolve_empty(QueryKind::DiLoopState);
    // `{:?}` matches how serde_json renders an f32 (`-120.0`, not `-120`).
    let expected = format!(
        "{{\"chains\":[{{\"chain\":\"guitar\",\"playing\":false,\"in_dbfs\":{:?},\"out_dbfs\":{:?},\"source\":null}}]}}",
        engine::output_meter::SILENT_DBFS,
        engine::output_meter::SILENT_DBFS
    );
    assert_eq!(di, expected);
}

#[test]
fn unhosted_loopers_still_answer_with_the_chains_persisted_shape() {
    let project = test_project_with_one_chain();
    let dispatcher = crate::local_dispatcher::LocalDispatcher::new(rc_project(&project));
    // The rate a frontend that hosts no looper runtime falls back to: the
    // dispatcher's engine rate, which is what `attach_engine_sr` syncs from
    // the live stream. Read from the dispatcher rather than written as a
    // number here, so the test cannot bless an invented rate (#723).
    let expected_rate = dispatcher.engine_sr();
    let ctx = ReadContext {
        project: &project,
        rig: None,
        io_bindings: &[],
        dispatcher: &dispatcher,
        live: &NoLiveSource,
    };
    let loopers = resolve(
        &QueryKind::ChainLoopers {
            chain: ChainId("guitar".to_string()),
        },
        &ctx,
    )
    .expect("an unhosted frontend still answers the chain's own loopers");
    assert!(loopers.contains("\"chain\":\"guitar\""), "{loopers}");
    assert!(
        loopers.contains(&format!("\"sample_rate\":{expected_rate}")),
        "{loopers}"
    );
    // The chain's persisted looper is there with its saved parameters; only
    // the live transport state reads empty.
    assert!(loopers.contains("\"uid\":1"), "{loopers}");
    assert!(loopers.contains("\"state\":\"empty\""), "{loopers}");
    assert!(loopers.contains("\"len_frames\":0"), "{loopers}");
    assert!(loopers.contains("\"length_seconds\":0.0"), "{loopers}");
}

#[test]
fn a_hosted_looper_source_reports_its_real_rate_not_the_dispatcher_fallback() {
    // A hosted rate must win over the dispatcher's tracked engine rate —
    // otherwise a hosted-but-idle frontend would silently report whatever
    // the dispatcher happens to default to instead of the real number.
    let loopers = resolve_hosted_by(
        &HostedLooperSource,
        QueryKind::ChainLoopers {
            chain: ChainId("guitar".to_string()),
        },
    )
    .expect("a hosted looper source must answer");
    assert!(loopers.contains("\"sample_rate\":96000"), "{loopers}");
}

#[test]
fn a_hosted_looper_source_that_failed_to_resolve_a_rate_reports_the_error_not_a_fabricated_one() {
    // Issue #723 regression: a chain whose rate genuinely could not be
    // resolved must propagate that failure, never fall back to whatever
    // number the dispatcher happens to default to.
    assert_eq!(
        resolve_hosted_by(
            &BrokenLooperHost,
            QueryKind::ChainLoopers {
                chain: ChainId("guitar".to_string()),
            },
        ),
        Err("no resolved sample rate for chain guitar".to_string())
    );
}

#[test]
fn unhosted_devices_answer_an_empty_listing_not_an_error() {
    assert_eq!(resolve_empty(QueryKind::Devices), String::new());
}

#[test]
fn a_hosted_device_source_reports_the_names_it_enumerated() {
    assert_eq!(
        resolve_hosted_by(&DeviceHost, QueryKind::Devices),
        Ok("Scarlett\nBuilt-in".to_string())
    );
}

#[test]
fn a_hosted_device_source_that_failed_reports_the_error_not_an_empty_listing() {
    // "the audio host is broken" and "this transport has no device source"
    // are different answers. Collapsing the failure into the empty shape
    // would hide a dead host / a JACK server that is down.
    assert_eq!(
        resolve_hosted_by(&BrokenDeviceHost, QueryKind::Devices),
        Err("no audio host".to_string())
    );
}

/// #127 (FINAL review): the latency badge reported 48 kHz on a stopped rig.
///
/// `measure_chain_latency` falls back to the rate the caller hands it when the
/// chain's input device has no saved setting, and `read::resolve` handed it
/// `engine_sr` — which Task 11 made reset to `REFERENCE_SAMPLE_RATE` on
/// teardown. On a 44.1 kHz interface the DSP latency came back ~8.8 % wrong,
/// and it was right before that change. The frontend that owns the devices can
/// resolve the chain's real rate with nothing running, so it is asked first.
#[test]
fn the_latency_probe_measures_at_the_rate_the_frontend_resolved() {
    let json = resolve_hosted_by(
        &StoppedRigOnA44kInterface,
        QueryKind::ChainLatency {
            chain: ChainId("guitar".to_string()),
        },
    )
    .expect("a known chain reports its latency");

    let value: serde_json::Value = serde_json::from_str(&json).expect("latency json");
    assert_eq!(
        value["sample_rate"], 44_100.0,
        "the probe must run at the rate this chain's devices resolve to, not at the \
         reference rate a stopped rig reports: {json}"
    );
}

/// The other half, so the fix cannot become an invented rate: a frontend that
/// cannot tell leaves the dispatcher's tracked engine rate in charge.
#[test]
fn a_frontend_that_cannot_resolve_a_rate_leaves_the_engine_rate_in_charge() {
    let project = test_project_with_one_chain();
    let dispatcher = crate::local_dispatcher::LocalDispatcher::new(rc_project(&project));
    let expected = dispatcher.engine_sr() as f64;
    let ctx = ReadContext {
        project: &project,
        rig: None,
        io_bindings: &[],
        dispatcher: &dispatcher,
        live: &NoLiveSource,
    };

    let json = resolve(
        &QueryKind::ChainLatency {
            chain: ChainId("guitar".to_string()),
        },
        &ctx,
    )
    .expect("an unhosted frontend still answers the chain's latency");

    let value: serde_json::Value = serde_json::from_str(&json).expect("latency json");
    assert_eq!(
        value["sample_rate"].as_f64().expect("a rate"),
        expected,
        "with nothing to resolve the rate against, the dispatcher's tracked rate is the \
         answer — never a number written here: {json}"
    );
}
