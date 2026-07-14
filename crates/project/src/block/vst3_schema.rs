//! #780: synthesise an OpenRig parameter schema for a VST3 block from the
//! plugin's own parameters (read off its `IEditController`, cached).
//!
//! The light discovery scan leaves `entry.info.params` empty, so a catalog VST3
//! has no manifest-authored knobs. Here each real parameter becomes an OpenRig
//! control chosen by its `step_count`:
//!
//! * `0`  → continuous knob (0–100 %),
//! * `1`  → on/off toggle when it reads like a switch (name/labels), otherwise a
//!          2-position selector,
//! * `>=2`→ selector, with one option per step (labels read from the plugin;
//!          the UI renders <=4 options as a rotary switch, more as a dropdown).
//!
//! Every parameter is stored under `p{id}`; the engine converts each value back
//! to a VST3 normalized 0..1 (`stereo::try_in_place_update` /
//! `runtime_block_core`), so the standard SetBlockParameter path drives a VST3
//! exactly like any other block.

use block_core::param::ParameterUnit;
use block_core::param::{bool_parameter, enum_parameter, float_parameter, ParameterSpec};

/// Build the parameter specs for a VST3 `model`, or an empty vec if the plugin
/// exposes none / cannot be read.
pub fn vst3_parameters(model: &str) -> Vec<ParameterSpec> {
    // Author-declared tab groups from the plugin's OpenRig package manifest
    // (empty when it ships none — the group then comes from dynamic grouping).
    let group_map = vst3_host::find_vst3_plugin(model)
        .map(|entry| plugin_loader::vst3_group_map_for_bundle(&entry.info.bundle_path))
        .unwrap_or_default();
    let params = vst3_host::catalog_params(model);
    let visible: Vec<_> = params
        .iter()
        .filter(|p| !is_unlabeled(&p.title, &p.short_title))
        .collect();
    let labels: Vec<String> = visible
        .iter()
        .map(|p| {
            if p.title.is_empty() {
                p.short_title.clone()
            } else {
                p.title.clone()
            }
        })
        .collect();
    // Manifest groups win; the dynamic grouping fills the gaps for any plugin
    // (or param) that declares none.
    let dynamic = dynamic_group_for(&labels);
    visible
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let path = format!("p{}", p.id);
            let label = &labels[i];
            let group = group_map
                .get(&p.id)
                .map(String::as_str)
                .or(dynamic[i].as_deref());
            let is_toggle = p.step_count == 1 && looks_like_on_off(&p.title, &p.enum_options);
            if is_toggle {
                bool_parameter(&path, label, group, Some(p.default_normalized >= 0.5))
            } else if p.step_count >= 1 {
                let options: Vec<(&str, &str)> = p
                    .enum_options
                    .iter()
                    .map(|(v, l)| (v.as_str(), l.as_str()))
                    .collect();
                let default_val = format!("{}", p.default_normalized * 100.0);
                enum_parameter(&path, label, group, Some(&default_val), &options)
            } else {
                float_parameter(
                    &path,
                    label,
                    group,
                    Some((p.default_normalized * 100.0) as f32),
                    0.0,
                    100.0,
                    1.0,
                    ParameterUnit::Percent,
                )
            }
        })
        .collect()
}

/// A parameter with no name (empty title and short title) is not a usable
/// control — a knob with a blank label tells the user nothing. Generic and
/// plugin-agnostic: OpenRig never hardcodes any plugin's naming (#780).
fn is_unlabeled(title: &str, short_title: &str) -> bool {
    title.trim().is_empty() && short_title.trim().is_empty()
}

/// Minimum parameters that must share a leading word before it becomes a tab.
/// Below this, the params stay ungrouped (a single default tab) rather than
/// spawning a swarm of two-knob tabs.
const MIN_DYNAMIC_GROUP: usize = 3;

/// Dynamic fallback grouping for a plugin that declares no manifest groups
/// (#780): a parameter is grouped under its leading word when at least
/// `MIN_DYNAMIC_GROUP` parameters share that word, otherwise `None` (the app
/// renders those in a single default tab). Plugin-agnostic — it reads only the
/// plugin's own parameter titles, never any hardcoded plugin name. Returns one
/// entry per input title, in order.
fn dynamic_group_for(titles: &[String]) -> Vec<Option<String>> {
    use std::collections::HashMap;
    let leading = |t: &str| t.split_whitespace().next().unwrap_or("").to_string();
    // Cluster the parameter indices by their leading word.
    let mut clusters: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, t) in titles.iter().enumerate() {
        let word = leading(t);
        if !word.is_empty() {
            clusters.entry(word).or_default().push(i);
        }
    }
    // A cluster with enough members becomes a tab, labelled by the longest
    // leading-token prefix its members share ("Input EQ", not "Input").
    let mut label_for: HashMap<String, String> = HashMap::new();
    for (word, idxs) in &clusters {
        if idxs.len() >= MIN_DYNAMIC_GROUP {
            let members: Vec<&str> = idxs.iter().map(|&i| titles[i].as_str()).collect();
            label_for.insert(word.clone(), common_token_prefix(&members));
        }
    }
    titles
        .iter()
        .map(|t| label_for.get(&leading(t)).cloned())
        .collect()
}

/// The longest leading run of whitespace-separated tokens shared by every
/// title (at least the first token, since callers cluster by it).
fn common_token_prefix(titles: &[&str]) -> String {
    let tokens: Vec<Vec<&str>> = titles
        .iter()
        .map(|t| t.split_whitespace().collect())
        .collect();
    let first = &tokens[0];
    let mut len = first.len();
    for toks in &tokens[1..] {
        let mut k = 0;
        while k < len && toks.get(k) == first.get(k) {
            k += 1;
        }
        len = k;
    }
    first[..len.max(1)].join(" ")
}

/// Heuristic: does a 2-state parameter read as an on/off switch (→ toggle)
/// rather than a 2-way selector? Uses the parameter title and its two step
/// labels. Deliberately conservative and name-based (#780); when unsure it
/// falls back to a selector so a real mode switch (e.g. "Mode 1"/"Mode 2") is
/// never flattened into on/off.
fn looks_like_on_off(title: &str, options: &[(String, String)]) -> bool {
    const ON_OFF_NAMES: &[&str] = &[
        "bypass", "enable", "enabled", "mute", "mono", "power", "on/off", "active",
    ];
    let t = title.to_lowercase();
    if ON_OFF_NAMES.iter().any(|w| t.contains(w)) {
        return true;
    }
    // Label-based: empty/numeric labels, or a recognised on/off pair.
    let labels: Vec<String> = options
        .iter()
        .map(|(_, l)| l.trim().to_lowercase())
        .collect();
    if labels
        .iter()
        .all(|l| l.is_empty() || l.chars().all(|c| c.is_ascii_digit()))
    {
        return true;
    }
    const ON_OFF_PAIRS: &[[&str; 2]] = &[
        ["off", "on"],
        ["no", "yes"],
        ["false", "true"],
        ["disabled", "enabled"],
    ];
    let set: std::collections::HashSet<&str> = labels.iter().map(String::as_str).collect();
    ON_OFF_PAIRS
        .iter()
        .any(|pair| pair.iter().all(|x| set.contains(x)))
}

#[cfg(test)]
mod tests {
    use super::{dynamic_group_for, is_unlabeled, looks_like_on_off};

    #[test]
    fn dynamic_grouping_labels_by_longest_common_prefix() {
        // Real QDelay sections: params share more than the first word, so the
        // tab label should be the longest common leading-token prefix, not just
        // the first token ("Input EQ", not "Input").
        let titles: Vec<String> = [
            "Input EQ Band1 Freq",
            "Input EQ Band2 Gain",
            "Input EQ Band3 Q",
            "Saturation Pre",
            "Saturation Post",
            "Saturation Drive",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let g = dynamic_group_for(&titles);
        assert_eq!(g[0].as_deref(), Some("Input EQ"));
        assert_eq!(g[1].as_deref(), Some("Input EQ"));
        assert_eq!(g[2].as_deref(), Some("Input EQ"));
        // Saturation members diverge at token 2 → prefix collapses to "Saturation".
        assert_eq!(g[3].as_deref(), Some("Saturation"));
        assert_eq!(g[5].as_deref(), Some("Saturation"));
    }

    #[test]
    fn dynamic_grouping_buckets_shared_leading_word() {
        let titles: Vec<String> = [
            "Gain Boost",
            "Gain Drive",
            "Gain Tone",
            "Delay Time",
            "Delay Feedback",
            "Level",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let groups = dynamic_group_for(&titles);
        // "Gain" is shared by 3 params (>= MIN_DYNAMIC_GROUP) → a tab.
        assert_eq!(groups[0].as_deref(), Some("Gain"));
        assert_eq!(groups[1].as_deref(), Some("Gain"));
        assert_eq!(groups[2].as_deref(), Some("Gain"));
        // "Delay" is shared by only 2 (< MIN) → ungrouped.
        assert_eq!(groups[3], None);
        assert_eq!(groups[4], None);
        // Unique word → ungrouped.
        assert_eq!(groups[5], None);
    }

    #[test]
    fn unlabeled_params_are_dropped() {
        assert!(is_unlabeled("", ""));
        assert!(is_unlabeled("  ", " ")); // whitespace only
        assert!(!is_unlabeled("Gain", ""));
        assert!(!is_unlabeled("", "Drv")); // short_title is a usable label
                                           // No plugin-specific names: a param literally named "Blank" is kept —
                                           // OpenRig does not know or care about any plugin's placeholder convention.
        assert!(!is_unlabeled("Blank", ""));
    }

    fn opts(a: &str, b: &str) -> Vec<(String, String)> {
        vec![("0".into(), a.to_string()), ("100".into(), b.to_string())]
    }

    #[test]
    fn on_off_by_name() {
        assert!(looks_like_on_off("Bypass", &opts("A", "B")));
        assert!(looks_like_on_off("Mono", &opts("A", "B")));
        assert!(looks_like_on_off("Gate Enable", &opts("A", "B")));
    }

    #[test]
    fn on_off_by_labels() {
        assert!(looks_like_on_off("Foo", &opts("Off", "On")));
        assert!(looks_like_on_off("Foo", &opts("No", "Yes")));
        assert!(looks_like_on_off("Foo", &opts("", ""))); // empty labels
        assert!(looks_like_on_off("Foo", &opts("0", "1"))); // numeric labels
    }

    #[test]
    fn real_mode_switch_stays_a_selector() {
        // Distinct, meaningful labels → NOT on/off → a 2-way selector.
        assert!(!looks_like_on_off("Mode", &opts("Sunlion", "Germanium")));
        assert!(!looks_like_on_off("Voicing", &opts("Vintage", "Modern")));
    }
}
