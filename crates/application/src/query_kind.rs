//! Responsibility: names the read-only state a transport can ask for.

/// Read-only state a transport can request. Resolved on the frontend thread
/// (which owns the `!Send` `Project`); serialization is done by domain code,
/// never re-derived in the adapter.
#[derive(Clone, Debug)]
pub enum QueryKind {
    /// Whole project as YAML.
    ProjectYaml,
    /// Available audio devices, one per line.
    Devices,
    /// Human-readable chain/block ID listing (for `midi-map.yaml` authors
    /// and the MCP `openrig://ids` resource). See [`crate::query::list_ids`].
    Ids,
    /// Per-chain input/output peak meters (`(chain_id, in_dbfs, out_dbfs)`,
    /// one record per line). Same numbers the GUI's IN/OUT bars read —
    /// every transport gets the same view (`openrig-code-quality` lei).
    ChainMeters,
    /// #554: the named preset bank of one chain (`rig:<input>`) as JSON.
    /// Resolved from the in-memory `RigProject.inputs[input].bank` — the
    /// disk-side preset library is a separate concept (different
    /// follow-up). Lets MCP / gRPC clients see the same preset list the
    /// GUI shows in the chain title combobox.
    ListChainPresets { chain: domain::ids::ChainId },
    /// #554 follow-up: every preset name in the project's in-memory
    /// `RigProject.presets` pool as JSON. A preset can sit in the pool
    /// without being bound to any input bank; tone-builder Step 0
    /// reads this to avoid silently overwriting an existing preset.
    ListProjectPresets,
    /// #561 (expanded scope): full plugin catalog as a JSON listing.
    /// See [`crate::query::list_plugin_catalog`]. Read parity for the
    /// reload / load / unload Commands so any transport can show the
    /// agent / user what is currently addressable.
    ListPluginCatalog,
    /// #561 (expanded scope): single plugin by manifest id, or
    /// `{"plugin": null}` when absent. See [`crate::query::get_plugin`].
    GetPlugin { id: String },
    /// #561 (expanded scope): text search across the catalog
    /// (case-insensitive substring on id / display_name / brand).
    /// Empty query = all entries. See [`crate::query::find_plugins`].
    FindPlugins { query: String },
    /// #572: full parameter schema for one plugin (catalog-level). No
    /// placed instance required. Resolved via
    /// `project::block::schema_for_block_model` and wrapped under a
    /// `params` envelope by [`crate::query::get_plugin_params`].
    /// Unknown id → `{"params": null}`.
    GetPluginParams { plugin_id: String },
    /// #572: list of materialised `BlockParameterDescriptor` for one
    /// placed block instance (schema + `current_value` per parameter).
    /// Resolved by [`crate::query::get_block_params`], which delegates
    /// to `AudioBlock::parameter_descriptors()` (same helper the GUI
    /// uses). Unknown chain / block → `Err`.
    GetBlockParams {
        chain: domain::ids::ChainId,
        block: domain::ids::BlockId,
    },
    /// #582: effective resolved system paths (data root + every
    /// configurable directory) as a JSON envelope. Resolved by
    /// [`crate::query::paths::resolved_paths_json`] over
    /// `AppConfig.paths` from `FilesystemStorage::load_app_config`.
    /// Every field reports the absolute resolved path — `None`
    /// overrides fall back to the OS default (consumers don't
    /// re-implement the fallback). MCP serves this as `openrig://paths`.
    Paths,
    /// #791: objective audio-quality report for one chain (THD+N, noise floor,
    /// level, dynamic range, clipping). Derived from the snapshot's chain by
    /// running the synthetic battery through the offline render, so it resolves
    /// off-frontend. MCP serves this as `openrig://chains/{chain}/quality`.
    ChainQualityReport { chain: domain::ids::ChainId },
    /// #323: one chain's loopers — persisted parameters merged with the live
    /// transport state the audio thread publishes. Runtime-coupled, so it is
    /// served by the frontend like `ChainMeters`. Same view the panel shows;
    /// MCP serves it as `openrig://chains/<id>/loopers` (query-parity law).
    ChainLoopers { chain: domain::ids::ChainId },
    /// #791: the chain's last Tone Doctor run (state + verdict). Lives in
    /// dispatcher state, not the snapshot, so it resolves on the frontend.
    /// MCP serves it as `openrig://chains/{chain}/tone`.
    ChainToneReport { chain: domain::ids::ChainId },
    /// #829: live tuner readings (note / cents / frequency per tap), the
    /// same numbers the Tuner window shows. Serialized by
    /// [`crate::query_analyzers::tuner_readings_json`]. `running: false`
    /// when the analyzer is powered off — the caller dispatches
    /// `SetTunerEnabled` first.
    TunerReadings,
    /// #829: live spectrum readings (per-band levels and peak holds), the
    /// same numbers the Spectrum window shows. Serialized by
    /// [`crate::query_analyzers::spectrum_readings_json`].
    SpectrumReadings,
    /// #829: per-chain DI loop state (playing, playback peaks, loaded
    /// source) — read parity for the DI commands. Serialized by
    /// [`crate::query_di::di_loop_state_json`].
    DiLoopState,
    /// #829: measured DSP latency for one chain, probed at the chain
    /// input's real rate and buffer (never a hardcoded 48 kHz — #723).
    /// Serialized by [`crate::query_latency::chain_latency_report`].
    ChainLatency { chain: domain::ids::ChainId },
    /// #127: the metronome — the settings the dispatcher owns plus the live
    /// beat position the click's own stream publishes. Read parity for the
    /// metronome commands: a client that can turn the click on must be able
    /// to see the tempo it is running at and the beat it is on. Serialized by
    /// [`crate::read`].
    MetronomeState,
}
