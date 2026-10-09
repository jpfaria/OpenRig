//! #1105: every output control port of an LV2 plugin gets its own slot.
//! Ardour's a-comp/a-exp read their gain-reduction port back as state on the
//! next run(); when all output ports shared one scratch buffer, the input
//! meter written last landed in that state and the plugin boosted a signal
//! that sat far below its threshold. With the defaults (threshold above the
//! test signal, no makeup) the output must match the input. Skips loudly
//! when this platform's binary is not in the tree.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};

fn rms_db(energy: f64, samples: usize) -> f64 {
    10.0 * (energy / samples as f64).log10()
}

#[test]
fn ace_dynamics_leave_a_signal_below_threshold_untouched() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/source");
    let packages = plugin_loader::discover(&root).expect("plugin tree readable");
    let mut checked = 0;
    for package in packages.into_iter().flatten() {
        let id = package.manifest.id.as_str();
        if id != "lv2_ardour_a_comp" && id != "lv2_ardour_a_exp" {
            continue;
        }
        let processor = match lv2::build_from_package(
            &package,
            &ParameterSet::default(),
            48_000.0,
            AudioChannelLayout::Stereo,
        ) {
            Ok(BlockProcessor::Stereo(p)) => p,
            Ok(BlockProcessor::Mono(_)) => panic!("{id}: expected a stereo processor"),
            Err(e) => {
                eprintln!("[#1105] {id} not buildable here ({e}) — skipping");
                continue;
            }
        };
        let mut processor = processor;
        let (mut e_in, mut e_out, mut n, mut t) = (0f64, 0f64, 0usize, 0usize);
        for block in 0..375 {
            let mut buf = [[0f32; 2]; 256];
            for f in buf.iter_mut() {
                let s = 0.3 * (t as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin();
                *f = [s, s];
                t += 1;
            }
            let input = buf;
            processor.process_block(&mut buf);
            if block < 75 {
                continue; // let attack/release and makeup smoothing settle
            }
            for (i, o) in input.iter().zip(buf.iter()) {
                e_in += (i[0] * i[0] + i[1] * i[1]) as f64;
                e_out += (o[0] * o[0] + o[1] * o[1]) as f64;
                n += 2;
            }
        }
        let (din, dout) = (rms_db(e_in, n), rms_db(e_out, n));
        assert!(
            (dout - din).abs() < 0.5,
            "BUG #1105: {id} changed a signal below its threshold: in {din:.1} dBFS, out {dout:.1} dBFS"
        );
        checked += 1;
    }
    if checked == 0 {
        eprintln!("[#1105] no ACE dynamics binary for this platform — skipping");
    }
}
