//! Frontend side of the read bus: resolves every [`QueryKind`] that needs
//! the GUI thread's live state (`!Send` project, runtime meters, analyzer
//! sessions) into the serialized payload a transport hands back.
//!
//! Extracted from `desktop_app` (#829) so the resolver can grow with the
//! read surface without inflating the window bootstrap, and so each arm is
//! one call into `application::query*` — the adapter never re-derives what
//! domain code already serializes.

use std::cell::RefCell;
use std::rc::Rc;

use application::bridge::QueryKind;
use slint::{Model, VecModel};

use infra_cpal::ProjectRuntimeController;

use crate::spectrum_session::SpectrumSession;
use crate::state::ProjectSession;
use crate::tuner_session::TunerSession;
use crate::ProjectChainItem;

/// Live handles the resolver reads from. Borrowed per tick — nothing is
/// cached, so a reply always reflects the frame the user is looking at.
pub(crate) struct QueryResolver<'a> {
    pub(crate) session: &'a ProjectSession,
    /// Chain rows the GUI meters write into (`meter_in_dbfs` / `meter_out_dbfs`).
    pub(crate) chain_rows: &'a Rc<VecModel<ProjectChainItem>>,
    pub(crate) tuner: &'a Rc<RefCell<Option<TunerSession>>>,
    pub(crate) spectrum: &'a Rc<RefCell<Option<SpectrumSession>>>,
    /// Live runtime — DI playback state and peaks come from it, per chain.
    pub(crate) runtime: &'a Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl QueryResolver<'_> {
    pub(crate) fn resolve(&self, kind: &QueryKind) -> Result<String, String> {
        let project = &self.session.project;
        match kind {
            QueryKind::ProjectYaml => {
                infra_yaml::serialize_project(&project.borrow()).map_err(|e| e.to_string())
            }
            QueryKind::Devices => infra_cpal::list_devices()
                .map(|d| d.join("\n"))
                .map_err(|e| e.to_string()),
            QueryKind::Ids => Ok(application::query::list_ids(&project.borrow())),
            QueryKind::ChainMeters => Ok(self.chain_meters()),
            // #829: the tuner / spectrum numbers the user reads on screen.
            QueryKind::TunerReadings => Ok(self.tuner_readings()),
            QueryKind::SpectrumReadings => Ok(self.spectrum_readings()),
            QueryKind::DiLoopState => Ok(self.di_loop_state()),
            QueryKind::ChainLatency { chain } => application::query_latency::chain_latency_report(
                &project.borrow(),
                &self.session.io_bindings.borrow(),
                chain,
                self.session.dispatcher.engine_sr() as f32,
            ),
            QueryKind::ListChainPresets { chain } => {
                // #554: the chain's preset bank, served from the in-memory
                // RigProject so MCP / gRPC see the same list the GUI shows
                // in the chain-title combobox.
                match self.session.rig.as_ref() {
                    Some(rig) => application::query::list_chain_presets(&rig.borrow(), chain),
                    None => Err("no rig attached to the session".to_string()),
                }
            }
            QueryKind::ListProjectPresets => {
                // #554 follow-up: project-level preset pool
                // (RigProject.presets in memory). A preset can sit here
                // without being wired to any input bank yet.
                match self.session.rig.as_ref() {
                    Some(rig) => Ok(application::query::list_project_presets(&rig.borrow())),
                    None => Err("no rig attached to the session".to_string()),
                }
            }
            // #561 (expanded scope): plugin catalog reads — same pure
            // helpers MCP would call (process-wide registry, no project
            // state).
            QueryKind::ListPluginCatalog => Ok(application::query::list_plugin_catalog()),
            QueryKind::GetPlugin { id } => Ok(application::query::get_plugin(id)),
            QueryKind::FindPlugins { query } => Ok(application::query::find_plugins(query)),
            // #572: per-plugin parameter schema (catalog-level).
            QueryKind::GetPluginParams { plugin_id } => {
                Ok(application::query::get_plugin_params(plugin_id))
            }
            // #572: per-block-instance descriptors (schema + current value).
            QueryKind::GetBlockParams { chain, block } => {
                application::query::get_block_params(&project.borrow(), chain, block)
            }
            // #582: resolved system paths (reloads config.yaml).
            QueryKind::Paths => Ok(application::query::resolved_paths_json()),
            // #791: objective quality report, measured offline.
            QueryKind::ChainQualityReport { chain } => {
                application::query_chain_quality::chain_quality_report(&project.borrow(), chain)
            }
        }
    }

    /// `<chain id>\t<in dBFS>\t<out dBFS>` per line — the numbers the IN/OUT
    /// bars show, read from the same rows.
    fn chain_meters(&self) -> String {
        let project = self.session.project.borrow();
        let mut out = String::new();
        for (idx, chain) in project.chains.iter().enumerate() {
            let (in_db, out_db) = self
                .chain_rows
                .row_data(idx)
                .map(|r| (r.meter_in_dbfs, r.meter_out_dbfs))
                .unwrap_or((
                    engine::output_meter::SILENT_DBFS,
                    engine::output_meter::SILENT_DBFS,
                ));
            out.push_str(&format!("{}\t{:.1}\t{:.1}\n", chain.id.0, in_db, out_db));
        }
        out
    }

    fn tuner_readings(&self) -> String {
        let session = self.tuner.borrow();
        let rows = session
            .as_ref()
            .map(TunerSession::readings)
            .unwrap_or_default();
        application::query_analyzers::tuner_readings_json(
            session.is_some(),
            crate::tuner_session::REFERENCE_HZ,
            &rows,
        )
    }

    /// Per-chain DI loop state, read from the live controller — the same
    /// `di_stream_active` / `di_playback_peaks` the chain tile shows.
    fn di_loop_state(&self) -> String {
        let runtime = self.runtime.borrow();
        let project = self.session.project.borrow();
        let rows: Vec<application::query_di::DiLoopReading> = project
            .chains
            .iter()
            .map(|chain| {
                let playing = runtime
                    .as_ref()
                    .map(|c| c.di_stream_active(&chain.id))
                    .unwrap_or(false);
                let meter = crate::di_meter::di_meter_from_peaks(
                    runtime
                        .as_ref()
                        .and_then(|c| c.di_playback_peaks(&chain.id)),
                    playing,
                );
                application::query_di::DiLoopReading {
                    chain: chain.id.0.clone(),
                    playing,
                    in_dbfs: meter.in_dbfs,
                    out_dbfs: meter.out_dbfs,
                    source: self.session.dispatcher.di_loop_source_for_chain(&chain.id),
                }
            })
            .collect();
        application::query_di::di_loop_state_json(&rows)
    }

    fn spectrum_readings(&self) -> String {
        let session = self.spectrum.borrow();
        let rows = session
            .as_ref()
            .map(SpectrumSession::readings)
            .unwrap_or_default();
        application::query_analyzers::spectrum_readings_json(
            session.is_some(),
            &feature_dsp::spectrum_fft::BAND_FREQS,
            &rows,
        )
    }
}
