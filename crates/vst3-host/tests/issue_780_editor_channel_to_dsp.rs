//! Issue #780 follow-up — Layer 2: does a parameter edit delivered the way the
//! native editor delivers it (host `ComponentHandler::performEdit` → param
//! channel) reach the audio processor's DSP? Mirrors the engine wiring:
//! register a context (→ channel), build the stereo processor on that channel,
//! then push through the SAME `ComponentHandler` the editor installs.
//!
//! Env-gated on OPENRIG_TEST_VST3_DIR; run with --test-threads=1.

use std::path::PathBuf;

use block_core::StereoProcessor;
use vst3::Steinberg::Vst::IComponentHandlerTrait;

const SR: f64 = 48_000.0;

fn chow() -> Option<&'static vst3_host::Vst3CatalogEntry> {
    let dir = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)?;
    vst3_host::init_vst3_catalog(SR, &[dir]);
    vst3_host::vst3_catalog().iter().find(|e| {
        e.info
            .bundle_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("ChowCentaur.vst3"))
            .unwrap_or(false)
    })
}

fn rms(frames: &[[f32; 2]]) -> f32 {
    let n = frames.len().max(1) as f32;
    (frames.iter().map(|f| f[0] * f[0]).sum::<f32>() / n).sqrt()
}

fn sine_frames(n: usize, amp: f32) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let s = amp * (2.0 * std::f32::consts::PI * 220.0 * (i as f32) / SR as f32).sin();
            [s, s]
        })
        .collect()
}

/// Find a parameter that measurably changes ChowCentaur's output, so the Layer-2
/// assertion drives something audible rather than an inert param.
fn find_audible_param(entry: &vst3_host::Vst3CatalogEntry, uid: &[u8; 16]) -> Option<u32> {
    let probe = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, uid, SR, 2, 512, &[]).ok()?;
    let ids: Vec<u32> = (0..probe.param_count())
        .filter_map(|i| probe.param_info(i).ok())
        .filter(|pi| !pi.title.to_lowercase().contains("bypass"))
        .map(|pi| pi.id)
        .collect();
    drop(probe);
    for id in ids {
        let ra = process_via_channel(entry, uid, Some((id, 0.0)));
        let rb = process_via_channel(entry, uid, Some((id, 1.0)));
        if (ra - rb).abs() > 1e-4 {
            return Some(id);
        }
    }
    None
}

/// Build the engine wiring (register context → stereo processor on its channel),
/// optionally deliver a param edit through the editor's `ComponentHandler`, then
/// process 8 blocks of a fresh sine and return the output RMS.
fn process_via_channel(
    entry: &vst3_host::Vst3CatalogEntry,
    uid: &[u8; 16],
    edit: Option<(u32, f64)>,
) -> f32 {
    let key = format!("io:{:?}", edit.map(|e| e.0)); // unique key per instance
    let plugin = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, uid, SR, 2, 512, &[]).unwrap();
    let controller = plugin.controller_clone();
    let channel = vst3_host::register_vst3_gui_context(
        &key,
        &entry.model_id,
        controller,
        plugin.library_arc(),
    );
    let mut proc = vst3_host::StereoVst3Processor::new(plugin, Some(channel.clone()));

    // The native editor delivers edits through the host ComponentHandler.
    if let Some((id, v)) = edit {
        let handler = vst3_host::component_handler::ComponentHandler::new(channel);
        unsafe {
            handler.performEdit(id, v);
        }
    }

    let mut out = Vec::new();
    for _ in 0..8 {
        let mut frames = sine_frames(512, 0.5);
        proc.process_block(&mut frames);
        out = frames;
    }
    rms(&out)
}

#[test]
fn editor_performedit_reaches_the_dsp_through_the_processor() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(&entry.model_id).unwrap();
    let Some(id) = find_audible_param(entry, &uid) else {
        panic!("no audible parameter found — Layer 1 precondition broken");
    };

    let low = process_via_channel(entry, &uid, Some((id, 0.0)));
    let high = process_via_channel(entry, &uid, Some((id, 1.0)));
    assert!(
        (low - high).abs() > 1e-4,
        "a performEdit through the ComponentHandler did not change the DSP output \
         (low={low}, high={high}) — the editor→channel→processor path is broken"
    );
}
