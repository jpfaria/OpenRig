//! Responsibility: resolves one query kind into its answer.
//! #831: the single [`QueryKind`] resolver.
//!
//! Before this module every frontend carried its own copy of the match —
//! `adapter-gui`'s `mcp_query_resolver` and `adapter-console`'s inline
//! closure — and each one decided its own payload for the reads it could
//! not serve. They drifted: the console wrote a literal `-120.0` for a
//! silent meter while the GUI wrote `engine::output_meter::SILENT_DBFS`,
//! and the console answered `Err("not available on the console adapter")`
//! for reads the GUI answers with numbers. A client could not address the
//! same resource across transports.
//!
//! The rules this module exists to enforce:
//!
//! - **A resource stays addressable on every transport.** A frontend that
//!   hosts no live source for a read gets the DOCUMENTED EMPTY SHAPE — the
//!   same fields, with the empty/silent values — never an error string and
//!   never a refusal. `None` from [`LiveSource`] means "this frontend hosts
//!   no such source", not "the read failed".
//! - **One error string per failure.** A genuine failure (no rig attached,
//!   unknown chain) reads identically whichever transport asked; see
//!   [`NO_RIG_ATTACHED`].
//! - **Serialization is never re-derived here.** Every arm delegates to the
//!   `crate::query*` helper that already owns the wire shape.
//! - **The core never enumerates devices.** `application` must not depend on
//!   `infra_cpal`; the device list arrives through [`LiveSource::devices`].

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use engine::output_meter::SILENT_DBFS;
use project::project::Project;
use project::rig::RigProject;

use crate::bridge::QueryKind;
use crate::dispatcher::CommandDispatcher;
use crate::live_source::LiveSource;
use crate::query_di::DiLoopReading;

/// The one "no rig" failure string, shared by every transport. The preset
/// reads need a `RigProject`, which is a session attachment rather than a
/// live source — its absence is a real failure, not an unhosted reading,
/// so it reports as an error. It just has to be the SAME error everywhere.
pub const NO_RIG_ATTACHED: &str = "no rig attached to the session";

/// Everything a read needs, borrowed for the length of one call. Nothing is
/// cached: a reply always reflects the frame the caller is serving.
///
/// The frontend supplies the borrows it already owns (`project` and `rig`
/// behind `RefCell`s in the GUI, plain values in the console) plus its
/// [`LiveSource`]; the resolver never reaches into a concrete frontend type.
pub struct ReadContext<'a> {
    pub project: &'a Project,
    /// The attached rig, when the session has one. `None` ⇒ the preset
    /// reads answer [`NO_RIG_ATTACHED`].
    pub rig: Option<&'a RigProject>,
    /// The per-machine I/O binding registry — the latency probe resolves a
    /// chain's real device/rate through it.
    pub io_bindings: &'a [IoBinding],
    pub dispatcher: &'a dyn CommandDispatcher,
    /// The frontend's live readings. [`crate::live_source::NoLiveSource`]
    /// for a frontend that hosts no audio runtime of its own.
    pub live: &'a dyn LiveSource,
}

/// Resolve one read into the payload every transport hands back.
pub fn resolve(kind: &QueryKind, ctx: &ReadContext<'_>) -> Result<String, String> {
    match kind {
        QueryKind::ProjectYaml => {
            infra_yaml::serialize_project(ctx.project).map_err(|e| e.to_string())
        }
        // The core never enumerates: the list is whatever the frontend that
        // owns an audio host supplies. No host ⇒ an empty listing, not an
        // error — the resource stays addressable. A host that IS there and
        // failed to enumerate is a different answer and propagates as one.
        QueryKind::Devices => match ctx.live.devices() {
            Some(devices) => devices.map(|d| d.join("\n")),
            None => Ok(String::new()),
        },
        QueryKind::Ids => Ok(crate::query::list_ids(ctx.project)),
        QueryKind::ChainMeters => Ok(chain_meters(ctx)),
        QueryKind::TunerReadings => Ok(tuner_readings(ctx)),
        QueryKind::SpectrumReadings => Ok(spectrum_readings(ctx)),
        QueryKind::DiLoopState => Ok(di_loop_state(ctx)),
        QueryKind::ChainLoopers { chain } => chain_loopers(ctx, chain),
        QueryKind::ChainLatency { chain } => crate::query_latency::chain_latency_report(
            ctx.project,
            ctx.io_bindings,
            chain,
            chain_probe_rate(ctx, chain),
        ),
        QueryKind::ListChainPresets { chain } => match ctx.rig {
            Some(rig) => crate::query::list_chain_presets(rig, chain),
            None => Err(NO_RIG_ATTACHED.to_string()),
        },
        QueryKind::ListProjectPresets => match ctx.rig {
            Some(rig) => Ok(crate::query::list_project_presets(rig)),
            None => Err(NO_RIG_ATTACHED.to_string()),
        },
        QueryKind::ListPluginCatalog => Ok(crate::query::list_plugin_catalog()),
        QueryKind::GetPlugin { id } => Ok(crate::query::get_plugin(id)),
        QueryKind::FindPlugins { query } => Ok(crate::query::find_plugins(query)),
        QueryKind::GetPluginParams { plugin_id } => Ok(crate::query::get_plugin_params(plugin_id)),
        QueryKind::GetBlockParams { chain, block } => {
            crate::query::get_block_params(ctx.project, chain, block)
        }
        QueryKind::Paths => Ok(crate::query::resolved_paths_json()),
        QueryKind::ChainQualityReport { chain } => {
            crate::query_chain_quality::chain_quality_report(ctx.project, chain)
        }
        QueryKind::ChainToneReport { chain } => Ok(ctx.dispatcher.tone_report_json(chain)),
        QueryKind::MetronomeState => Ok(metronome_state(ctx)),
    }
}

/// The rate the latency probe runs at when the chain's input device has no
/// saved per-device setting: what the FRONTEND resolves for that chain, and
/// only then the dispatcher's tracked engine rate.
///
/// The order matters. `engine_sr` mirrors a RUNNING stream and goes back to
/// `REFERENCE_SAMPLE_RATE` when the rig stops (#127), so asking it first
/// measured a stopped rig on a 44.1 kHz interface at 48 kHz — a number that was
/// right before that change. Neither source is ever a guess: a frontend that
/// cannot resolve the chain's devices answers `None` (issue #723).
fn chain_probe_rate(ctx: &ReadContext<'_>, chain: &ChainId) -> f32 {
    ctx.live
        .chain_sample_rate(chain)
        .unwrap_or_else(|| ctx.dispatcher.engine_sr() as f32)
}

/// The metronome, both halves in one payload: the settings the DISPATCHER
/// owns (so they read the same on a transport that hosts no audio) and the
/// beat position only a live stream can report.
///
/// `running` comes from the audio side when there is one — that is the truth
/// about whether anything is audible. With no runtime hosted, the
/// control-plane flag answers instead, and the position reads as beat zero
/// rather than a fabricated one.
fn metronome_state(ctx: &ReadContext<'_>) -> String {
    let snapshot = ctx.dispatcher.metronome_snapshot();
    let live = ctx.live.metronome();
    let running = live.as_ref().map_or(snapshot.running, |m| m.running);
    let settings = snapshot.settings;
    serde_json::json!({
        "running": running,
        "bpm": settings.bpm,
        "beats_per_bar": settings.beats_per_bar,
        "subdivision": settings.subdivision.key(),
        "timbre": settings.timbre.key(),
        "volume": settings.volume,
        "count_in": settings.count_in,
        "output": snapshot.output_key,
        "bar": live.as_ref().map_or(0, |m| m.bar),
        "beat": live.as_ref().map_or(0, |m| m.beat),
        "tick": live.as_ref().map_or(0, |m| m.tick),
        "counting_in": live.as_ref().is_some_and(|m| m.counting_in),
    })
    .to_string()
}

/// `<chain id>\t<in dBFS>\t<out dBFS>` per line, one line per project chain.
///
/// The rows come from the project, not from the frontend, so the line count
/// and order are identical whether or not meters are hosted; a chain the
/// live source says nothing about reads silent.
fn chain_meters(ctx: &ReadContext<'_>) -> String {
    let live = ctx.live.chain_meters().unwrap_or_default();
    let mut out = String::new();
    for chain in &ctx.project.chains {
        let (in_db, out_db) = live
            .iter()
            .find(|r| r.chain == chain.id)
            .map_or((SILENT_DBFS, SILENT_DBFS), |r| (r.in_dbfs, r.out_dbfs));
        out.push_str(&format!("{}\t{:.1}\t{:.1}\n", chain.id.0, in_db, out_db));
    }
    out
}

/// `running` is exactly "a frontend hosts this analyzer": powered off (or
/// not hosted at all) reports `false` with no rows, so a client knows to
/// dispatch `SetTunerEnabled` instead of reading silence as "no signal".
fn tuner_readings(ctx: &ReadContext<'_>) -> String {
    let rows = ctx.live.tuner();
    crate::query_analyzers::tuner_readings_json(
        rows.is_some(),
        feature_dsp::pitch_yin::DEFAULT_REFERENCE_HZ,
        &rows.unwrap_or_default(),
    )
}

fn spectrum_readings(ctx: &ReadContext<'_>) -> String {
    let rows = ctx.live.spectrum();
    crate::query_analyzers::spectrum_readings_json(
        rows.is_some(),
        &feature_dsp::spectrum_fft::BAND_FREQS,
        &rows.unwrap_or_default(),
    )
}

/// One row per project chain. Playback state and peaks come from the live
/// source (silent when unhosted); the loaded source always comes from the
/// dispatcher, which is the only owner of that state — no frontend has to
/// know to fill it in, so it cannot drift between transports.
///
/// `DiLoopReading` doubles as the input and the output type, so a frontend
/// CAN set `source` — and it is discarded here. Supply playback state and
/// peaks only; the dispatcher answers for the rest.
fn di_loop_state(ctx: &ReadContext<'_>) -> String {
    let live = ctx.live.di_loop().unwrap_or_default();
    let rows: Vec<DiLoopReading> = ctx
        .project
        .chains
        .iter()
        .map(|chain| {
            // `DiLoopReading.chain` is a plain `String` while the rest of
            // the read side keys on `ChainId` — bridge it here rather than
            // reshaping the published wire type.
            let hosted = live.iter().find(|r| r.chain == chain.id.0);
            DiLoopReading {
                chain: chain.id.0.clone(),
                playing: hosted.is_some_and(|r| r.playing),
                in_dbfs: hosted.map_or(SILENT_DBFS, |r| r.in_dbfs),
                out_dbfs: hosted.map_or(SILENT_DBFS, |r| r.out_dbfs),
                source: ctx.dispatcher.di_loop_source_for_chain(&chain.id),
            }
        })
        .collect();
    crate::query_di::di_loop_state_json(&rows)
}

/// The chain's PERSISTED loopers merged with whatever live transport state
/// the frontend hosts. Unhosted ⇒ no live statuses, so every looper reads
/// empty — the shape is the chain's own, which exists on every transport.
///
/// Frame counts mean nothing without the rate they were counted at, so the
/// rate always comes from [`LiveSource::chain_loopers`] itself when the
/// frontend hosts one: a hosted-but-unresolvable rate is a real failure
/// (`Some(Err(_))`) and propagates as one — never silently substituted with
/// a fabricated number (issue #723). Only a frontend that hosts NO looper
/// runtime at all (`None`) falls back to the dispatcher's tracked engine
/// rate, synced from the live stream by `attach_engine_sr`.
fn chain_loopers(ctx: &ReadContext<'_>, chain: &ChainId) -> Result<String, String> {
    let c = ctx
        .project
        .chains
        .iter()
        .find(|c| &c.id == chain)
        .ok_or_else(|| format!("chain not found: {}", chain.0))?;
    match ctx.live.chain_loopers(chain) {
        Some(Ok((statuses, rate))) => Ok(crate::query_loopers::loopers_json(c, &statuses, rate)),
        Some(Err(e)) => Err(e),
        None => Ok(crate::query_loopers::loopers_json(
            c,
            &[],
            ctx.dispatcher.engine_sr(),
        )),
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
