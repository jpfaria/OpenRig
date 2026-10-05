//! #328: a VST3 block reports the processing latency its plugin declares
//! (`IAudioProcessor::getLatencySamples`), so a chain split can line its two
//! paths up. Scans the installed VST3 plugins for one that declares a
//! non-zero latency; skips loudly when none is installed (CI).

use block_core::{MonoProcessor, StereoProcessor};
use vst3::Steinberg::Vst::{IAudioProcessor, IAudioProcessorTrait};

const SR: f64 = 48_000.0;

fn declared_latency(plugin: &vst3_host::Vst3Plugin) -> usize {
    let processor = plugin
        .component()
        .cast::<IAudioProcessor>()
        .expect("the component is an audio processor");
    unsafe { processor.getLatencySamples() as usize }
}

#[test]
fn vst3_processors_report_the_plugins_declared_latency() {
    vst3_host::init_vst3_catalog(SR, &[]);
    for entry in vst3_host::vst3_catalog() {
        let Ok(uid) = vst3_host::resolve_uid_for_model(entry.model_id) else {
            continue;
        };
        let Ok(plugin) =
            vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[])
        else {
            continue;
        };
        let declared = declared_latency(&plugin);
        if declared == 0 {
            continue;
        }
        let stereo = vst3_host::StereoVst3Processor::new(plugin, None);
        assert_eq!(
            stereo.latency_samples(),
            declared,
            "{}: the stereo processor must report the plugin's latency",
            entry.model_id
        );
        let plugin = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[])
            .expect("second instance");
        let mono = vst3_host::Vst3Processor::new(plugin, None);
        assert_eq!(
            mono.latency_samples(),
            declared,
            "{}: the mono processor must report the plugin's latency",
            entry.model_id
        );
        return;
    }
    eprintln!("[#328] no installed VST3 declares a processing latency — skipping");
}
