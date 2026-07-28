//! The GUI's answer to every `QueryKind` the bridge queues (#791 split out of
//! `desktop_app`).
//!
//! Reads that need the live session — the project the GUI owns, its rig, the
//! meter rows, the dispatcher's ephemeral state — cannot resolve off-frontend,
//! so they land here on the 16 ms tick. Everything a transport can see goes
//! through this one function: MCP, gRPC and the GUI itself read the same
//! numbers by construction.

use application::bridge::QueryKind;
use slint::Model;

use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// Resolve one query against the live session.
pub(crate) fn resolve(
    kind: &QueryKind,
    session: &ProjectSession,
    meter_rows: &std::rc::Rc<slint::VecModel<ProjectChainItem>>,
) -> Result<String, String> {
    let project = &session.project;
    match kind {
        QueryKind::ProjectYaml => {
            infra_yaml::serialize_project(&project.borrow()).map_err(|e| e.to_string())
        }
        QueryKind::Devices => infra_cpal::list_devices()
            .map(|d| d.join("\n"))
            .map_err(|e| e.to_string()),
        QueryKind::Ids => Ok(application::query::list_ids(&project.borrow())),
        QueryKind::ChainMeters => {
            let proj_borrow = project.borrow();
            let mut out = String::new();
            for (idx, chain) in proj_borrow.chains.iter().enumerate() {
                let row = meter_rows.row_data(idx);
                let (in_db, out_db) = row.map(|r| (r.meter_in_dbfs, r.meter_out_dbfs)).unwrap_or((
                    engine::output_meter::SILENT_DBFS,
                    engine::output_meter::SILENT_DBFS,
                ));
                out.push_str(&format!("{}\t{:.1}\t{:.1}\n", chain.id.0, in_db, out_db));
            }
            Ok(out)
        }
        // #554: the chain's preset bank, served from the in-memory RigProject
        // so MCP / gRPC see the same list the GUI shows in the chain-title
        // combobox.
        QueryKind::ListChainPresets { chain } => match session.rig.as_ref() {
            Some(rig) => application::query::list_chain_presets(&rig.borrow(), chain),
            None => Err("no rig attached to the session".to_string()),
        },
        // #554 follow-up: project-level preset pool (RigProject.presets in
        // memory). A preset can sit here without being wired to any input bank.
        QueryKind::ListProjectPresets => match session.rig.as_ref() {
            Some(rig) => Ok(application::query::list_project_presets(&rig.borrow())),
            None => Err("no rig attached to the session".to_string()),
        },
        // #561 (expanded scope): plugin catalog reads — same pure helpers MCP
        // would call (process-wide registry, no project state).
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
        // #791: the Tone Doctor's last verdict, read back from dispatcher
        // state — MCP sees exactly what the panel is showing.
        QueryKind::ChainToneReport { chain } => Ok(session.dispatcher.tone_report_json(chain)),
    }
}
