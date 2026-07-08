//! #771 owner screenshot: with the DI playing (healthy stream peaks), the
//! main screen's DI meter row sat at the guitar's noise floor (-75 dB IN,
//! "—" OUT). The row was still bound to the CHAIN's aggregate meters
//! (`meter_in_dbfs`/`meter_out_dbfs`); the DI plays on its own isolated
//! stream, so the row must read `chain.di_meter` — the playback's own
//! peaks — like the compact view already does.

use std::path::PathBuf;

fn slint() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/pages/chain_row.slint");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn di_row_reads_the_di_streams_own_meter() {
    let src = slint();
    let di_row_start = src
        .find("root.chain.di_loop_playing")
        .expect("chain_row.slint must render the DI meter row while the DI plays");
    let di_row = &src[di_row_start..];
    assert!(
        di_row.contains("root.chain.di_meter.in_dbfs")
            && di_row.contains("root.chain.di_meter.out_dbfs"),
        "the DI row's bar + dB text must read the DI playback's own peaks \
         (`chain.di_meter`), not the chain aggregate"
    );
}
