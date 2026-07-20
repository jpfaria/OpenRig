//! LV2 (`backend: lv2`) bundle TTL → parameter schema.

pub(crate) fn lv2_parameters(
    package: &plugin_loader::LoadedPackage,
    plugin_uri: &str,
    binaries: &std::collections::BTreeMap<plugin_loader::manifest::Lv2Slot, std::path::PathBuf>,
) -> Vec<block_core::param::ParameterSpec> {
    use plugin_loader::dispatch::Lv2PortRole;
    // Prefer the deduplicated `<package>/data/` TTL bundle; fall back
    // to the legacy per-platform layout where TTLs lived next to the
    // binary. Either layout works.
    let data_dir = package.root.join("data");
    let bundle_dir: std::path::PathBuf = if data_dir.is_dir() {
        data_dir
    } else if let Some((_, rel_binary)) = binaries.iter().next() {
        let bin_path = package.root.join(rel_binary);
        match bin_path.parent() {
            Some(parent) => parent.to_path_buf(),
            None => return Vec::new(),
        }
    } else {
        return Vec::new();
    };
    let Ok(ports) = plugin_loader::dispatch::scan_lv2_ports(&bundle_dir, plugin_uri) else {
        return Vec::new();
    };
    ports
        .into_iter()
        .filter(|port| port.role == Lv2PortRole::ControlIn)
        .map(one_lv2_param)
        .collect()
}

/// Translate one LV2 ControlIn port into the corresponding
/// `ParameterSpec`. Routing — checked in this order so that an
/// `enumeration + integer` port (a common pattern) lands as an enum:
///
/// 1. `lv2:toggled` → bool checkbox.
/// 2. `lv2:enumeration` with at least one `lv2:scalePoint` → enum dropdown.
/// 3. `lv2:integer` (no scalePoint) → integer-stepped float.
/// 4. otherwise → continuous float (legacy behaviour).
fn one_lv2_param(port: plugin_loader::dispatch::Lv2Port) -> block_core::param::ParameterSpec {
    let label = port.name.clone().unwrap_or_else(|| port.symbol.clone());

    if port.is_toggle {
        let default = port.default_value.map(|value| value >= 0.5).or(Some(false));
        return block_core::param::bool_parameter(&port.symbol, &label, None, default);
    }

    if port.is_enumeration && !port.scale_points.is_empty() {
        // Enum values keep the original numeric ordering. Stored values
        // are the numeric `rdf:value` (stringified) so the runtime can
        // round-trip them back to the LV2 control port.
        let options: Vec<(String, String)> = port
            .scale_points
            .iter()
            .map(|sp| (sp.value.to_string(), sp.label.clone()))
            .collect();
        let options_refs: Vec<(&str, &str)> = options
            .iter()
            .map(|(value, label)| (value.as_str(), label.as_str()))
            .collect();
        let default = port
            .default_value
            .and_then(|value| {
                port.scale_points
                    .iter()
                    .find(|sp| (sp.value - value).abs() < f32::EPSILON)
            })
            .map(|sp| sp.value.to_string());
        return block_core::param::enum_parameter(
            &port.symbol,
            &label,
            None,
            default.as_deref(),
            &options_refs,
        );
    }

    let min = port.minimum.unwrap_or(0.0);
    let max = port.maximum.unwrap_or(1.0).max(min + 0.001);
    let default = port.default_value.unwrap_or((min + max) / 2.0);

    let step = if port.is_integer {
        // pprop:rangeSteps tells us exactly how many discrete positions
        // the host should expose; fall back to step=1 for plain integer
        // ports without explicit step count.
        port.range_steps
            .filter(|n| *n > 0)
            .map(|n| (max - min) / n as f32)
            .unwrap_or(1.0)
    } else {
        // Continuous control. step = 0 = "no snap-to-grid".
        0.0
    };

    block_core::param::float_parameter(
        &port.symbol,
        &label,
        None,
        Some(default),
        min,
        max,
        step,
        block_core::param::ParameterUnit::None,
    )
}
