//! Loads each `.nam` file passed on the command line, runs the
//! loudness probe, and prints what the probe sees — so we can debug
//! mismatched levels without round-tripping through the running app.
//!
//! Usage: `cargo run --example probe_dump -- path/to/a.nam path/to/b.nam ...`

use anyhow::Result;
use nam::loudness_probe::{
    diagnose_model, MAX_OFFSET_DB, MIN_OFFSET_DB, PEAK_CEILING_DBFS, PROBE_INPUT_PEAK_DBFS,
    TARGET_RMS_DBFS,
};
use nam::processor::{close_model_diag, open_model_diag, recommended_adjustments};

fn main() -> Result<()> {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: probe_dump <a.nam> [b.nam ...]");
        std::process::exit(2);
    }

    println!(
        "TARGET_RMS={TARGET_RMS_DBFS:.1} dBFS  \
         PEAK_CEILING={PEAK_CEILING_DBFS:.1} dBFS  \
         INPUT_PEAK={PROBE_INPUT_PEAK_DBFS:.1} dBFS  \
         CLAMP=[{MIN_OFFSET_DB:.1}, {MAX_OFFSET_DB:.1}] dB"
    );
    println!();
    println!(
        "{:<60} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "model", "in_pk", "in_rms", "out_pk", "out_rms", "want_L", "allow_pk", "OFFSET"
    );

    for path in &paths {
        let model = open_model_diag(path)?;
        let (rec_in, rec_out) = unsafe { recommended_adjustments(model) };
        let r = unsafe { diagnose_model(model) };
        unsafe { close_model_diag(model) };

        let label = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".nam");
        println!(
            "{:<60} {:>+9.2} {:>+9.2} {:>+9.2} {:>+9.2} {:>+9.2} {:>+9.2} {:>+9.2}",
            label,
            r.input_peak_dbfs,
            r.input_rms_dbfs,
            r.output_peak_dbfs,
            r.output_rms_dbfs,
            r.want_for_loudness_db,
            r.allowed_by_peak_db,
            r.final_offset_db
        );
        println!(
            "    baked recommended: input_db={rec_in:+.2}  output_db={rec_out:+.2}",
        );
    }

    Ok(())
}
