//! Responsibility: picks which discovered VST3 bundle backs each catalog model id.

use crate::discovery::Vst3PluginInfo;
use crate::plugin_uid_cache::make_model_id;
use std::collections::HashSet;

/// One entry per model id. A bundle shipped in an OpenRig plugins root wins
/// over a system-installed copy with the same id: OpenRig distributes that
/// build and its package resolves to it (#1104).
pub(crate) fn merge_discovered(
    system: Vec<Vst3PluginInfo>,
    bundled: Vec<Vst3PluginInfo>,
) -> Vec<Vst3PluginInfo> {
    let mut seen: HashSet<String> = HashSet::new();
    bundled
        .into_iter()
        .chain(system)
        .filter(|info| seen.insert(make_model_id(info)))
        .collect()
}

#[cfg(test)]
#[path = "catalog_merge_tests.rs"]
mod tests;
