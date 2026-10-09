//! #1105: ZamVerb's convolver only runs when `run()` gets exactly the block
//! size the host announced, and passes the input through otherwise. Its
//! manifest declares `block_length`, so the host feeds it fixed blocks and
//! the reverb is heard. Uses the in-repo plugin tree (`plugins/source`);
//! skips loudly when this platform has no ZamVerb binary.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};

fn zamverb() -> Option<plugin_loader::LoadedPackage> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/source");
    plugin_loader::discover(&root)
        .ok()?
        .into_iter()
        .flatten()
        .find(|package| package.manifest.id == "lv2_zamverb")
}

fn energy(frames: &[[f32; 2]]) -> f32 {
    frames.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum()
}

#[test]
fn zamverb_adds_its_reverb_tail_in_odd_sized_callbacks() {
    let Some(package) = zamverb() else {
        eprintln!("[#1105] lv2_zamverb not in plugins/source — skipping");
        return;
    };
    let processor = match lv2::build_from_package(
        &package,
        &ParameterSet::default(),
        48_000.0,
        AudioChannelLayout::Stereo,
    ) {
        Ok(BlockProcessor::Stereo(processor)) => processor,
        Ok(BlockProcessor::Mono(_)) => panic!("ZamVerb is a stereo plugin"),
        Err(error) => {
            eprintln!("[#1105] ZamVerb does not load on this platform ({error}) — skipping");
            return;
        }
    };
    let mut processor = processor;
    // One loud 10 ms burst, then a second of silence, in 100-frame callbacks
    // (never the plugin's block size). Dry, the silence stays silent.
    let mut signal = vec![[0.0f32; 2]; 48_000];
    for (i, frame) in signal.iter_mut().take(480).enumerate() {
        let s = (i as f32 * 0.3).sin() * 0.5;
        *frame = [s, s];
    }
    for chunk in signal.chunks_mut(100) {
        processor.process_block(chunk);
    }
    let tail = energy(&signal[9_600..]);
    assert!(
        tail > 1e-3,
        "BUG #1105: ZamVerb left no reverb tail after the burst (energy {tail})"
    );
}
