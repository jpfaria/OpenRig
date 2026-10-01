//! #328: an LV2 plugin that declares a latency port (`lv2:reportsLatency`)
//! reports that latency through its processor, so a chain split can line its
//! paths up. Needs the owner's plugin tree (`OPENRIG_OWNER_PLUGINS` or a
//! sibling `OpenRig-plugins` checkout); skips loudly without it, or when no
//! buildable LV2 plugin in it declares a latency port.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use plugin_loader::dispatch::scan_lv2_ports;
use plugin_loader::manifest::Backend;

fn owner_plugins_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPENRIG_OWNER_PLUGINS") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let cand = dir.join("OpenRig-plugins/plugins/source");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn reported(processor: BlockProcessor) -> usize {
    match processor {
        BlockProcessor::Mono(p) => p.latency_samples(),
        BlockProcessor::Stereo(p) => p.latency_samples(),
    }
}

#[test]
fn an_lv2_plugin_with_a_latency_port_reports_it() {
    let Some(root) = owner_plugins_root() else {
        eprintln!("[#328] owner plugin tree absent (set OPENRIG_OWNER_PLUGINS) — skipping");
        return;
    };
    let packages = plugin_loader::discover(&root).expect("plugin tree readable");
    let mut reports = Vec::new();
    for package in packages.into_iter().flatten() {
        let Backend::Lv2 { plugin_uri, .. } = &package.manifest.backend else {
            continue;
        };
        let Ok(ports) = scan_lv2_ports(&package.root.join("data"), plugin_uri) else {
            continue;
        };
        if !ports.iter().any(|p| p.reports_latency) {
            continue;
        }
        for layout in [AudioChannelLayout::Stereo, AudioChannelLayout::Mono] {
            if let Ok(processor) =
                lv2::build_from_package(&package, &ParameterSet::default(), 48_000.0, layout)
            {
                reports.push((package.manifest.id.clone(), reported(processor)));
                break;
            }
        }
    }
    if reports.is_empty() {
        eprintln!("[#328] no buildable LV2 plugin with a latency port here — skipping");
        return;
    }
    assert!(
        reports.iter().any(|(_, samples)| *samples > 0),
        "BUG #328: every LV2 plugin that declares a latency port reported 0: {reports:?}"
    );
}
